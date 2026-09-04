//! Notice changes while Circinus is open: Steam updating a Workshop item, the game rewriting
//! ModsConfig.xml, a mod folder dropped into Mods/. Cheap polling of a handful of mtimes —
//! one stat call each every few seconds — rather than a recursive file watcher over
//! thousands of folders. When a re-read turns up something new, the window gets a toast
//! (via `state-changed`) and the desktop gets a notification, so an update Steam applies
//! while Circinus sits behind the game is not missed.

use crate::commands::Shared;
use circinus_core::changes::{self, ChangeKind, ChangeReason};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

const INTERVAL: Duration = Duration::from_secs(8);
/// Steam writes the ACF before it has finished laying files down; give it a moment.
const SETTLE: Duration = Duration::from_secs(2);

fn mtime(p: &PathBuf) -> u64 {
    std::fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn watched_paths(st: &Shared) -> Vec<PathBuf> {
    let Ok(app) = st.try_lock() else { return Vec::new() };
    let loc = &app.locations;
    [loc.workshop_acf(), loc.mods_config_path(), loc.local_mods_dir.clone(), loc.workshop_dir.clone()].into_iter().flatten().collect()
}

fn signature(paths: &[PathBuf]) -> Vec<(PathBuf, u64)> {
    paths.iter().map(|p| (p.clone(), mtime(p))).collect()
}

/// Keys of the changes currently reported, so a re-read can tell old from new.
fn change_keys(st: &Shared) -> (HashSet<String>, Option<String>) {
    match st.lock() {
        Ok(app) => (app.changes.iter().map(|c| format!("{:?}:{}", c.kind, c.uid)).collect(), app.list_change.as_ref().map(|l| format!("{l:?}"))),
        Err(_) => (HashSet::new(), None),
    }
}

fn notify(handle: &AppHandle, st: &Shared, before: &HashSet<String>, before_list: &Option<String>) {
    let Ok(app) = st.lock() else { return };
    let fresh: Vec<_> = app.changes.iter().filter(|c| !before.contains(&format!("{:?}:{}", c.kind, c.uid))).collect();
    let list_now = app.list_change.as_ref().map(|l| format!("{l:?}"));
    let (title, body) = if let Some(first) = fresh.first() {
        let what = match first.kind {
            ChangeKind::Updated if first.reasons.contains(&ChangeReason::WorkshopUpdate) => format!("Steam updated {}", first.name),
            ChangeKind::Updated => format!("{} changed on disk", first.name),
            ChangeKind::Added => format!("{} was installed", first.name),
            ChangeKind::Removed => format!("{} was removed", first.name),
        };
        let title = if fresh.len() == 1 { what } else { format!("{what} and {} more", fresh.len() - 1) };
        let owned: Vec<changes::ModChange> = fresh.iter().map(|c| (*c).clone()).collect();
        let active = owned.iter().filter(|c| c.active).count();
        (title, format!("{}{}. Open Circinus to see what changed.", changes::summary(&owned), if active > 0 { format!(" · {active} in your active list") } else { String::new() }))
    } else if list_now.is_some() && list_now != *before_list {
        ("Your mod list was changed outside Circinus".to_string(), "ModsConfig.xml was rewritten by RimWorld or another manager.".to_string())
    } else {
        return;
    };
    drop(app);
    tracing::info!(%title, "notify");
    if let Err(e) = handle.notification().builder().title(title).body(body).show() {
        tracing::warn!("desktop notification failed: {e}");
    }
}

pub fn start(handle: AppHandle, st: Shared) {
    tauri::async_runtime::spawn(async move {
        let mut last: Option<Vec<(PathBuf, u64)>> = None;
        loop {
            tokio::time::sleep(INTERVAL).await;
            let paths = watched_paths(&st);
            if paths.is_empty() {
                continue; // locked by a scan, or nothing configured yet
            }
            let sig = signature(&paths);
            let changed = matches!(&last, Some(prev) if *prev != sig);
            if !changed {
                last = Some(sig);
                continue;
            }
            let what: Vec<String> = sig.iter().filter(|(p, t)| last.as_ref().map(|l| !l.contains(&(p.clone(), *t))).unwrap_or(false)).map(|(p, _)| p.display().to_string()).collect();
            tracing::info!(changed = ?what, "watch: something changed on disk, re-reading");
            tokio::time::sleep(SETTLE).await;
            let (before, before_list) = change_keys(&st);
            let h = handle.clone();
            let s = st.clone();
            let _ = tauri::async_runtime::spawn_blocking(move || crate::run_scan(h, s, false)).await;
            notify(&handle, &st, &before, &before_list);
            // Steam may still be mid-update; the next tick picks up the rest.
            let after = watched_paths(&st);
            last = Some(if after.is_empty() { sig } else { signature(&after) });
        }
    });
}
