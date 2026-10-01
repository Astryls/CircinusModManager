//! The download manager: one background task that feeds SteamCMD batches from the queue,
//! honours the throttle, moves finished mods into the Mods folder and publishes progress.

use crate::commands::Shared;
use circinus_core::steam::steamcmd::{Dest, ItemStatus, QueueState, SteamCmd, BATCH_PAUSE, STALL_TIMEOUT};
use circinus_core::steam::webapi;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};
use tokio::sync::Notify;

const QUEUE_KEY: &str = "download_queue";

pub struct Downloads {
    /// Set by Skip, read by the batch runner. SteamCMD cannot abandon one item of a script,
    /// so a skip stops the whole run; the queue then starts the next batch without the item
    /// that was skipped, which is already marked Cancelled by the time the run ends.
    skip: Arc<AtomicBool>,
    pub state: Mutex<QueueState>,
    notify: Notify,
    handle: AppHandle,
    app: Shared,
    client: reqwest::Client,
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AddResult {
    pub added: usize,
    pub skipped: Vec<(u64, String)>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SteamCmdStatus {
    pub installed: bool,
    pub installing: bool,
    pub root: String,
    pub exe: String,
    pub downloads_dir: String,
    pub console_log: String,
    pub console_log_bytes: u64,
    pub mods_dir: Option<String>,
    pub workshop_dir: Option<String>,
    pub queued: usize,
    pub running: bool,
    pub paused: bool,
    pub batch_size: usize,
    pub cooldown_until: Option<i64>,
}

impl Downloads {
    pub fn start(handle: AppHandle, app: Shared) -> Arc<Downloads> {
        let mut state: QueueState = app.lock().ok().and_then(|a| a.cache.get::<QueueState>(QUEUE_KEY).ok().flatten()).unwrap_or_default();
        for item in state.items.iter_mut().filter(|i| i.status == ItemStatus::Downloading) {
            item.status = ItemStatus::Queued;
        }
        state.running = false;
        state.installing = false;
        state.current_batch.clear();
        state.current_item = None;
        let client = reqwest::Client::builder().user_agent(circinus_core::weight::USER_AGENT).build().unwrap_or_default();
        let dl = Arc::new(Downloads { skip: Arc::new(AtomicBool::new(false)), state: Mutex::new(state), notify: Notify::new(), handle, app, client });
        dl.refresh_installed();
        let runner = dl.clone();
        tauri::async_runtime::spawn(async move { runner.run().await });
        dl
    }

    pub fn steamcmd(&self) -> SteamCmd {
        let root = self.app.lock().map(|a| a.data_dir.join("steamcmd")).unwrap_or_else(|_| std::path::PathBuf::from("steamcmd"));
        SteamCmd::new(root)
    }

    fn refresh_installed(&self) {
        let installed = self.steamcmd().is_installed();
        if let Ok(mut s) = self.state.lock() {
            s.steamcmd_installed = installed;
        }
    }

    pub fn snapshot(&self) -> QueueState {
        self.state.lock().map(|s| s.clone()).unwrap_or_default()
    }

    fn persist(&self) {
        let snap = self.snapshot();
        if let Ok(app) = self.app.lock() {
            let _ = app.cache.set(QUEUE_KEY, &snap);
        }
    }

    pub fn emit(&self) {
        let _ = self.handle.emit("download-progress", self.snapshot());
    }

    fn wake(&self) {
        self.notify.notify_one();
    }

    /// Names for ids we know from the Steam database, so the queue is readable before the
    /// Web API answers.
    fn known_names(&self, ids: &[u64]) -> HashMap<u64, String> {
        let mut names = HashMap::new();
        if let Ok(app) = self.app.lock() {
            for id in ids {
                if let Some(e) = app.db.steam.by_pfid.get(id) {
                    let n = e.steam_name.clone().filter(|s| !s.is_empty()).unwrap_or_else(|| e.name.clone());
                    if !n.is_empty() {
                        names.insert(*id, n);
                    }
                }
            }
        }
        names
    }

    /// Queue ids. Looks the ids up on Steam to name them and to skip things that are not
    /// RimWorld mods; if Steam does not answer, they are queued by number.
    ///
    /// `dests` says, per id, which copy of the mod this download is replacing. A caller that
    /// is fetching something not installed can leave it out: a new download has only one place
    /// to go. A caller acting on a mod the user picked must not, because two copies of one mod
    /// share an id and nothing downstream can tell them apart afterwards.
    pub async fn add(&self, ids: Vec<u64>, dests: HashMap<u64, Dest>) -> AddResult {
        let mut names = self.known_names(&ids);
        let mut skipped = Vec::new();
        let mut accepted: Vec<u64> = Vec::new();
        match webapi::published_file_details(&self.client, &ids).await {
            Ok(items) => {
                let by_id: HashMap<u64, _> = items.into_iter().map(|i| (i.published_file_id, i)).collect();
                for id in &ids {
                    match by_id.get(id) {
                        Some(item) if item.is_rimworld_mod() => {
                            names.insert(*id, item.title.clone());
                            accepted.push(*id);
                        }
                        Some(item) if item.file_type == 2 => skipped.push((*id, "that is a collection. Import it from the Import dialog".into())),
                        Some(item) if item.result != 1 => skipped.push((*id, "Steam says this item is hidden or removed".into())),
                        Some(_) => skipped.push((*id, "not a RimWorld workshop item".into())),
                        None => accepted.push(*id),
                    }
                }
            }
            Err(_) => accepted = ids.clone(),
        }
        let added = {
            let mut s = self.state.lock().unwrap();
            s.add(&accepted, &names, &dests, now())
        };
        self.persist();
        self.emit();
        self.wake();
        AddResult { added, skipped }
    }

    /// Give up on the item SteamCMD is working on and carry on with the rest.
    ///
    /// Reported as: a large download that stalls takes the whole queue with it, and Pause
    /// does not help because the run in flight keeps going. SteamCMD has no way to abandon
    /// one item of a script, so this marks the item Cancelled and stops the run; the loop
    /// then starts the next batch, which no longer contains it.
    ///
    /// Returns false when there is nothing in flight, so the window can say so rather than
    /// appearing to do nothing.
    pub fn skip_current(&self) -> bool {
        let skipped = {
            let mut s = self.state.lock().unwrap();
            let Some(id) = s.current_item.or_else(|| s.current_batch.first().copied()) else { return false };
            if let Some(item) = s.items.iter_mut().find(|i| i.id == id) {
                item.status = ItemStatus::Cancelled;
                item.error = Some("Skipped".into());
                item.finished_at = Some(now());
            }
            s.push_log(&format!("Skipping {id}"));
            true
        };
        if skipped {
            self.skip.store(true, Ordering::SeqCst);
            self.persist();
            self.emit();
        }
        skipped
    }

    pub fn remove(&self, ids: &[u64]) {
        self.state.lock().unwrap().remove(ids);
        self.persist();
        self.emit();
    }

    pub fn retry_failed(&self) -> usize {
        let n = self.state.lock().unwrap().retry_failed();
        self.persist();
        self.emit();
        self.wake();
        n
    }

    pub fn clear_finished(&self) {
        self.state.lock().unwrap().clear_finished();
        self.persist();
        self.emit();
    }

    pub fn set_paused(&self, paused: bool) {
        self.state.lock().unwrap().paused = paused;
        self.persist();
        self.emit();
        self.wake();
    }

    pub async fn install_steamcmd(self: &Arc<Self>) -> Result<(), String> {
        {
            let mut s = self.state.lock().unwrap();
            if s.installing {
                return Err("SteamCMD is already being installed".into());
            }
            s.installing = true;
            s.push_log("Installing SteamCMD…");
        }
        self.emit();
        let cmd = self.steamcmd();
        let me = self.clone();
        let log = move |line: String| {
            if let Ok(mut s) = me.state.lock() {
                s.push_log(&line);
            }
            me.emit();
        };
        let result = cmd.install(&self.client, &log).await.map_err(|e| e.to_string());
        {
            let mut s = self.state.lock().unwrap();
            s.installing = false;
            s.steamcmd_installed = cmd.is_installed();
            match &result {
                Ok(()) => s.push_log("SteamCMD is ready."),
                Err(e) => s.push_log(&format!("SteamCMD install failed: {e}")),
            }
        }
        self.emit();
        self.wake();
        result
    }

    /// Where things are and whether SteamCMD is usable — for Settings and the Downloads view.
    pub fn status(&self) -> SteamCmdStatus {
        self.refresh_installed();
        let cmd = self.steamcmd();
        let console_log = cmd.console_log();
        let (mods_dir, workshop_dir) = self.app.lock().map(|a| (a.locations.local_mods_dir.clone(), a.locations.workshop_dir.clone())).unwrap_or((None, None));
        let s = self.snapshot();
        SteamCmdStatus {
            installed: s.steamcmd_installed,
            installing: s.installing,
            root: cmd.root.display().to_string(),
            exe: cmd.exe().display().to_string(),
            downloads_dir: cmd.downloads_dir().display().to_string(),
            console_log: console_log.display().to_string(),
            console_log_bytes: std::fs::metadata(&console_log).map(|m| m.len()).unwrap_or(0),
            mods_dir: mods_dir.map(|p| p.display().to_string()),
            workshop_dir: workshop_dir.map(|p| p.display().to_string()),
            queued: s.queued(),
            running: s.running,
            paused: s.paused,
            batch_size: s.throttle.batch_size,
            cooldown_until: s.throttle.cooldown_until,
        }
    }

    /// Run `+login anonymous +quit` and stream the output into the log: the quickest way to
    /// see whether SteamCMD works on this machine (and, on Windows, whether its console log
    /// reaches us).
    pub async fn test_steamcmd(self: &Arc<Self>) -> Result<circinus_core::steam::steamcmd::TestOutcome, String> {
        {
            let mut s = self.state.lock().unwrap();
            if s.installing {
                return Err("SteamCMD is still being installed".into());
            }
            if s.running {
                return Err("A download batch is running. Try again when it finishes".into());
            }
            if !s.steamcmd_installed {
                return Err("SteamCMD is not installed yet".into());
            }
            s.push_log("Test: login anonymous, then quit…");
        }
        self.emit();
        let cmd = self.steamcmd();
        let me = self.clone();
        let mut last_emit = std::time::Instant::now();
        let mut on_line = move |line: &str| {
            if let Ok(mut s) = me.state.lock() {
                s.push_log(line);
            }
            if last_emit.elapsed() > std::time::Duration::from_millis(300) {
                me.emit();
                last_emit = std::time::Instant::now();
            }
        };
        let result = cmd.test(&mut on_line).await.map_err(|e| e.to_string());
        {
            let mut s = self.state.lock().unwrap();
            match &result {
                Ok(t) if t.logged_in => s.push_log(&format!("Test passed: anonymous login in {} s, {} lines of output.", t.seconds, t.lines)),
                Ok(t) if t.stalled => s.push_log("Test failed: SteamCMD produced no output and was stopped."),
                Ok(t) => s.push_log(&format!("Test finished without a login confirmation (exit code {:?}, {} lines).", t.exit_code, t.lines)),
                Err(e) => s.push_log(&format!("Test failed: {e}")),
            }
        }
        self.emit();
        result
    }

    async fn run(self: Arc<Self>) {
        loop {
            // Wait for work.
            let (ready, wait_ms) = {
                let s = self.state.lock().unwrap();
                let remaining = s.throttle.remaining_cooldown(now());
                let ready = !s.paused && !s.installing && s.steamcmd_installed && s.queued() > 0 && remaining == 0;
                (ready, if remaining > 0 { 1000 } else { 30_000 })
            };
            if !ready {
                tokio::select! {
                    _ = self.notify.notified() => {}
                    _ = tokio::time::sleep(std::time::Duration::from_millis(wait_ms)) => {}
                }
                if wait_ms == 1000 {
                    self.emit(); // cooldown countdown
                }
                continue;
            }
            // **Nowhere to put it is a reason not to start, not a reason to fail at the end.**
            //
            // Reported with a screenshot: a whole collection downloaded, thirteen minutes of
            // it, and every item then failed with "Downloaded but could not be moved into
            // Mods: no local Mods folder is configured". The check was real and in the right
            // place for correctness and the worst possible place for a person -- after the
            // bytes were on disk, once per item, by which time the only thing left to do is
            // say so fourteen times.
            //
            // `local_mods_dir` is derived from the game folder whenever it is not set by
            // hand, so it is missing only when Circinus cannot find RimWorld at all. That is
            // worth stopping for, and worth saying once.
            if self.app.lock().map(|a| a.locations.local_mods_dir.is_none()).unwrap_or(false) {
                let mut s = self.state.lock().unwrap();
                if !s.paused {
                    s.paused = true;
                    s.push_log("Paused: Circinus has not found the game's Mods folder, so a download would have nowhere to go. Settings, Where RimWorld lives, will set it.");
                }
                drop(s);
                self.persist();
                self.emit();
                continue;
            }
            let batch = {
                let mut s = self.state.lock().unwrap();
                let batch = s.next_batch();
                for item in s.items.iter_mut() {
                    if batch.iter().any(|(id, _)| *id == item.id) {
                        item.status = ItemStatus::Downloading;
                    }
                }
                s.current_batch = batch.iter().map(|(id, _)| *id).collect();
                s.running = true;
                let size = s.throttle.batch_size;
                s.push_log(&format!("Batch of {} (batch size {})", batch.len(), size));
                batch
            };
            self.persist();
            self.emit();
            let cmd = self.steamcmd();
            // Everything, not just this batch's ids.
            //
            // `forget(&ids)` cleared the items about to be downloaded and left every item an
            // earlier run had moved away still listed as installed. SteamCMD then built a new
            // download out of chunks it believed were on disk, failed reading one from a folder
            // that had moved into Mods, and answered by validating the whole app -- which
            // re-downloaded every item still listed. One report had 9,797 files and 336 MB come
            // back three seconds after a batch of 25 had been collected.
            //
            // The queue cannot predict which earlier item a new download will share a file
            // with, so the only safe list is all of them.
            let _ = cmd.forget_everything();
            let me = self.clone();
            let mut last_emit = std::time::Instant::now();
            let mut on_line = move |line: &str| {
                if let Ok(mut s) = me.state.lock() {
                    s.push_log(line);
                    if let circinus_core::steam::steamcmd::LineEvent::Downloading(id) = circinus_core::steam::steamcmd::parse_line(line) {
                        s.current_item = Some(id);
                    }
                }
                if last_emit.elapsed() > std::time::Duration::from_millis(300) {
                    me.emit();
                    last_emit = std::time::Instant::now();
                }
            };
            let outcome = cmd.run_batch(&batch, STALL_TIMEOUT, &self.skip, &mut on_line).await;
            // Cleared whatever happened, so a skip stops one run rather than every run after
            // it. Anything still Downloading when the run was cut goes back in the queue: the
            // item that was skipped is already Cancelled and `next_batch` will not pick it up.
            if self.skip.swap(false, Ordering::SeqCst) {
                let mut s = self.state.lock().unwrap();
                for item in s.items.iter_mut().filter(|i| i.status == ItemStatus::Downloading) {
                    item.status = ItemStatus::Queued;
                }
            }
            // Where each finished item goes is decided by where the copy being updated already
            // lives, which is read once here rather than per item.
            let (mods_dir, workshop_dir, subscribed) = self
                .app
                .lock()
                .ok()
                .map(|a| {
                    let subs: std::collections::HashSet<u64> = a
                        .mods
                        .iter()
                        .filter(|m| m.source == circinus_core::model::Source::Workshop)
                        .filter_map(|m| m.published_file_id)
                        .collect();
                    (a.locations.local_mods_dir.clone(), a.locations.workshop_dir.clone(), subs)
                })
                .unwrap_or((None, None, Default::default()));
            let mut any_done = false;
            match outcome {
                Ok(outcome) => {
                    let done = {
                        let mut s = self.state.lock().unwrap();
                        s.apply(&outcome, now())
                    };
                    for id in done {
                        /*
                         * A MOD IS REPLACED WHERE IT ALREADY LIVES, AND THE CHOICE WAS MADE
                         * WHEN THE ITEM WAS QUEUED.
                         *
                         * Everything used to go to `Mods/<id>`, Force update of a subscribed
                         * mod included. With the same packageId in Mods and in Steam's folder,
                         * RimWorld suffixes the Workshop copy `_steam`, so ModsConfig.xml's
                         * plain packageId now names Circinus's copy: the game loads that one,
                         * Steam keeps updating a folder nothing reads, and the mod is stuck at
                         * whatever this download fetched until somebody deletes it by hand.
                         *
                         * Fixing that by reading the installed list *here* traded one bug for
                         * another (#3). "Keep my own copy" leaves the subscription in place on
                         * purpose, so a localized mod is installed twice under one id; this
                         * loop saw the subscription and sent the download to Steam, whichever
                         * of the two Force update buttons had been pressed. The kept copy was
                         * never touched, stayed behind, and was offered the same useless
                         * update again on every rescan.
                         *
                         * The id cannot answer the question, because the question is about a
                         * copy and two copies share an id. `item.dest` is the answer, recorded
                         * where it was known: at the button. `Unsaid` is an item queued by an
                         * older build, and only that case still guesses from the id.
                         */
                        let dest = { self.state.lock().unwrap().items.iter().find(|i| i.id == id).map(|i| i.dest).unwrap_or_default() };
                        let to_steam = match dest {
                            Dest::Workshop => true,
                            Dest::Mods => false,
                            Dest::Unsaid => subscribed.contains(&id),
                        };
                        let placed = match (to_steam, &workshop_dir, &mods_dir) {
                            (true, Some(ws), _) => cmd.replace_workshop_copy(id, ws).map(|p| p.display().to_string()),
                            // Subscribed, but Circinus cannot see Steam's folder. Falling back
                            // to Mods is what caused the bug, so it is refused and said plainly.
                            (true, None, _) => Err(circinus_core::Error::Other("this mod is installed through Steam and Circinus cannot find Steam's workshop folder. Set it in Settings, or let Steam update the mod".into())),
                            (false, _, Some(dir)) => cmd.collect(id, dir).map(|p| p.display().to_string()),
                            (false, _, None) => Err(circinus_core::Error::Other("no local Mods folder is configured".into())),
                        };
                        let mut s = self.state.lock().unwrap();
                        if let Some(item) = s.items.iter_mut().find(|i| i.id == id) {
                            match placed {
                                Ok(p) => {
                                    item.path = Some(p);
                                    any_done = true;
                                }
                                Err(e) => {
                                    item.status = ItemStatus::Failed;
                                    item.error = Some(if to_steam { format!("Downloaded but could not replace Steam's copy: {e}") } else { format!("Downloaded but could not be moved into Mods: {e}") });
                                }
                            }
                        }
                    }
                    let mut s = self.state.lock().unwrap();
                    let (cool, size) = (s.throttle.remaining_cooldown(now()), s.throttle.batch_size);
                    let msg = match cool {
                        0 => format!("Batch done in {} s; next batch size {}", outcome.seconds, size),
                        c => format!("Steam is refusing downloads. Waiting {c} s, batch size now {size}"),
                    };
                    s.push_log(&msg);
                }
                Err(e) => {
                    let mut s = self.state.lock().unwrap();
                    s.push_log(&format!("SteamCMD could not run: {e}"));
                    for item in s.items.iter_mut().filter(|i| i.status == ItemStatus::Downloading) {
                        item.status = ItemStatus::Queued;
                    }
                    s.current_batch.clear();
                    s.paused = true;
                }
            }
            {
                let mut s = self.state.lock().unwrap();
                s.running = false;
            }
            self.persist();
            self.emit();
            if any_done {
                let handle = self.handle.clone();
                let app = self.app.clone();
                tauri::async_runtime::spawn_blocking(move || crate::run_scan(handle, app, false));
            }
            tokio::time::sleep(BATCH_PAUSE).await;
        }
    }
}
