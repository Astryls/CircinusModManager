//! Auto-update. circinus.sh/modmanager serves the feed; Tauri's updater plugin does the
//! asking, the download, the signature check and the install. This file turns that into two
//! commands, one quiet check a few seconds after launch, and `update-progress` events for the
//! banner while an install runs. A development build does nothing of the kind; `DEV_BUILD` says
//! why.
//!
//! The wire format is the plugin's own, written out in `docs/update-feed.md` for the site.

use crate::commands::Shared;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};

/// What a check found, as the UI shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheck {
    pub available: bool,
    /// The version this build is.
    pub current: String,
    /// The newest version the feed offers, when it is newer than this one.
    pub version: Option<String>,
    pub notes: Option<String>,
    /// RFC 3339, as the feed sent it.
    pub pub_date: Option<String>,
}

/// One `update-progress` event: how far the install has come.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    /// downloading | installing | restarting | failed
    pub phase: &'static str,
    pub version: String,
    pub downloaded: u64,
    /// The server's Content-Length, when it sent one.
    pub total: Option<u64>,
    pub error: Option<String>,
}

/// Why a development build says nothing about updates.
///
/// `main` carries a placeholder version on purpose: the real number is set on the release commit,
/// which belongs to the tag rather than to the branch. So a build made from `main` tells the feed
/// it is 0.1.0, every release circinus.sh has ever served looks newer than it, and a banner
/// offering the newest one comes up a few seconds into every `npm run tauri dev`. Pressing Install
/// on it would download the real installer and run it over the machine being developed on -- which
/// on Windows takes the running dev process with it. Only a developer ever reads this sentence: a
/// player's copy is a release build and checks normally.
const DEV_BUILD: &str = "This is a development build. It carries the placeholder version main keeps rather than a released one, so there is nothing for circinus.sh to compare it against. Build it with `npm run tauri build` to test the updater.";

pub struct Updater {
    handle: AppHandle,
    app: Shared,
    /// The update the last check found, so Install does not ask the feed a second time.
    found: Mutex<Option<Update>>,
    /// The launch-time check runs once; a setting change later does not start another.
    checked_on_start: AtomicBool,
    installing: AtomicBool,
}

/// How long after launch the quiet check runs: after the first scan has had its turn, and
/// late enough that a slow feed never delays the window.
const STARTUP_DELAY: Duration = Duration::from_secs(5);

impl Updater {
    pub fn new(handle: AppHandle, app: Shared) -> Arc<Updater> {
        Arc::new(Updater { handle, app, found: Mutex::new(None), checked_on_start: AtomicBool::new(false), installing: AtomicBool::new(false) })
    }

    fn current_version(&self) -> String {
        self.handle.package_info().version.to_string()
    }

    /// Ask the feed. Remembers what it offered so `install` can use it.
    pub async fn check(&self) -> Result<UpdateCheck, String> {
        if tauri::is_dev() {
            return Err(DEV_BUILD.into());
        }
        let current = self.current_version();
        let updater = self.handle.updater().map_err(plain)?;
        let result = updater.check().await.map_err(plain)?;
        let check = match &result {
            Some(u) => UpdateCheck { available: true, current, version: Some(u.version.clone()), notes: u.body.clone(), pub_date: u.raw_json.get("pub_date").and_then(|d| d.as_str()).map(str::to_string) },
            None => UpdateCheck { available: false, current, version: None, notes: None, pub_date: None },
        };
        if let Ok(mut found) = self.found.lock() {
            *found = result;
        }
        Ok(check)
    }

    /// The check on start: once, only when the setting is on, and quiet about anything that
    /// goes wrong. An update it finds arrives as an `update-available` event.
    pub fn start(self: &Arc<Self>) {
        if tauri::is_dev() {
            tracing::info!("development build: not checking for updates");
            return;
        }
        if self.checked_on_start.swap(true, Ordering::SeqCst) {
            return;
        }
        let this = self.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(STARTUP_DELAY).await;
            let wanted = this.app.lock().map(|a| a.settings.check_for_updates).unwrap_or(false);
            if !wanted {
                return;
            }
            match this.check().await {
                Ok(c) if c.available => {
                    tracing::info!(version = c.version.as_deref().unwrap_or("?"), "an update is available");
                    let _ = this.handle.emit("update-available", c);
                }
                Ok(_) => tracing::info!("no update: this build is the newest"),
                Err(e) => tracing::info!("the update check on start did not succeed: {e}"),
            }
        });
    }

    /// Download, verify, install and restart. Progress goes out as `update-progress`; this
    /// only returns when something went wrong, because on success the process is replaced.
    pub async fn install(self: Arc<Self>) -> Result<(), String> {
        if tauri::is_dev() {
            return Err(DEV_BUILD.into());
        }
        if self.installing.swap(true, Ordering::SeqCst) {
            return Err("An update is already being installed".into());
        }
        let result = self.install_inner().await;
        self.installing.store(false, Ordering::SeqCst);
        result
    }

    async fn install_inner(&self) -> Result<(), String> {
        let remembered = self.found.lock().ok().and_then(|f| f.clone());
        let update = match remembered {
            Some(u) => u,
            None => {
                // Install pressed without a check (or after one that found nothing): ask now.
                self.check().await?;
                match self.found.lock().ok().and_then(|f| f.clone()) {
                    Some(u) => u,
                    None => return Err(format!("Circinus {} is the newest version; there is nothing to install", self.current_version())),
                }
            }
        };
        let version = update.version.clone();
        let handle = self.handle.clone();
        let progress = move |phase: &'static str, downloaded: u64, total: Option<u64>, error: Option<String>| {
            let _ = handle.emit("update-progress", UpdateProgress { phase, version: version.clone(), downloaded, total, error });
        };
        progress("downloading", 0, None, None);
        // At most ~20 events per second: a fast connection delivers thousands of chunks.
        let mut downloaded: u64 = 0;
        let mut last = Instant::now() - Duration::from_secs(1);
        let mut size: Option<u64> = None;
        let on_chunk = |chunk: usize, total: Option<u64>| {
            downloaded += chunk as u64;
            size = total;
            if last.elapsed() >= Duration::from_millis(50) {
                last = Instant::now();
                progress("downloading", downloaded, total, None);
            }
        };
        let handle = self.handle.clone();
        let app = self.app.clone();
        let on_finished = || {
            // The window's place is normally remembered when it closes; the installer closes it
            // for us, so remember it now.
            if let Some(w) = handle.get_webview_window("main") {
                crate::remember_window(&w, &app);
            }
        };
        let outcome = update.download_and_install(on_chunk, on_finished).await;
        match outcome {
            Ok(()) => {
                // On Windows the installer has already taken over and this line is never
                // reached; on macOS and Linux the files are in place and a restart loads them.
                progress("restarting", downloaded, size, None);
                self.handle.restart();
            }
            Err(e) => {
                let msg = plain(e);
                progress("failed", downloaded, size, Some(msg.clone()));
                Err(msg)
            }
        }
    }
}

/// The plugin's errors in the words a player needs: what failed and what it means for them.
fn plain(e: tauri_plugin_updater::Error) -> String {
    use tauri_plugin_updater::Error as E;
    match e {
        E::Reqwest(r) if r.is_connect() => "Circinus could not reach circinus.sh. Check your internet connection and try again.".into(),
        E::Reqwest(r) if r.is_timeout() => "circinus.sh took too long to answer. Try again in a moment.".into(),
        E::Reqwest(r) => format!("The connection to circinus.sh failed: {r}"),
        E::Network(s) => format!("The download from circinus.sh was interrupted: {s}"),
        E::ReleaseNotFound | E::Serialization(_) => "circinus.sh answered, but not with an update Circinus understands. The update feed may be down; try again later.".into(),
        E::Minisign(_) | E::Base64(_) | E::SignatureUtf8(_) => "The download is not signed with Circinus's key, so it was not installed. Nothing on your computer was changed.".into(),
        E::Io(io) => format!("The update could not be written to disk: {io}"),
        E::TempDirNotOnSameMountPoint => "The update could not be put in place: the temporary folder is on a different drive from Circinus.".into(),
        E::AuthenticationFailed => "The update needs administrator rights and did not get them. Nothing was changed.".into(),
        other => other.to_string(),
    }
}

#[tauri::command]
pub async fn update_check(updater: State<'_, Arc<Updater>>) -> Result<UpdateCheck, String> {
    updater.check().await
}

#[tauri::command]
pub async fn update_install(updater: State<'_, Arc<Updater>>) -> Result<(), String> {
    updater.inner().clone().install().await
}

#[cfg(test)]
mod tests {
    /// The guard above is only as good as what `is_dev` means. `cargo test` builds the same way
    /// `tauri dev` does -- without the `custom-protocol` feature -- so this run is a development
    /// build, and saying so here means a change to that in Tauri fails a test rather than quietly
    /// waking the updater up in somebody's dev window again.
    #[test]
    fn a_development_build_is_recognised_as_one() {
        assert!(tauri::is_dev(), "a test build should look like a development build to the updater");
    }
}
