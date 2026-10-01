//! SteamCMD: install on first use, download workshop items in batches, and the smart throttle
//! that keeps Steam from locking us out.
//!
//! Facts about SteamCMD this relies on (they are behaviours of Valve's tool, not our design):
//! more than ~25 items per script trips an internal profiler limit and aborts; anonymous login
//! is enough for RimWorld workshop items; output is unreliable on a pipe on Windows, so the
//! console log is tailed as well; a moved-away item must be forgotten in the workshop ACF or
//! SteamCMD will believe it is still installed and skip it.

use crate::model::RIMWORLD_APP_ID;
use crate::steam::acf;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

pub const MAX_BATCH: usize = 25;
pub const MIN_BATCH: usize = 3;
pub const MAX_ATTEMPTS: u32 = 4;
/// No output for this long means SteamCMD is wedged (Steam silently stalls large items).
pub const STALL_TIMEOUT: Duration = Duration::from_secs(150);
/// A polite pause between batches even when everything is healthy.
pub const BATCH_PAUSE: Duration = Duration::from_secs(3);

#[cfg(target_os = "windows")]
const DOWNLOAD_URL: &str = "https://steamcdn-a.akamaihd.net/client/installer/steamcmd.zip";
#[cfg(target_os = "macos")]
const DOWNLOAD_URL: &str = "https://steamcdn-a.akamaihd.net/client/installer/steamcmd_osx.tar.gz";
#[cfg(all(unix, not(target_os = "macos")))]
const DOWNLOAD_URL: &str = "https://steamcdn-a.akamaihd.net/client/installer/steamcmd_linux.tar.gz";

/// Paths of one SteamCMD prefix: `<root>/steamcmd` holds the tool, `<root>/steam` is the
/// install root SteamCMD downloads into.
#[derive(Debug, Clone)]
pub struct SteamCmd {
    pub root: PathBuf,
}

impl SteamCmd {
    pub fn new(root: PathBuf) -> SteamCmd {
        SteamCmd { root }
    }
    pub fn tool_dir(&self) -> PathBuf {
        self.root.join("steamcmd")
    }
    pub fn exe(&self) -> PathBuf {
        if cfg!(target_os = "windows") {
            self.tool_dir().join("steamcmd.exe")
        } else {
            self.tool_dir().join("steamcmd.sh")
        }
    }
    pub fn install_dir(&self) -> PathBuf {
        self.root.join("steam")
    }
    pub fn downloads_dir(&self) -> PathBuf {
        self.install_dir().join("steamapps").join("workshop").join("content").join(RIMWORLD_APP_ID.to_string())
    }
    pub fn acf_path(&self) -> PathBuf {
        self.install_dir().join("steamapps").join("workshop").join(format!("appworkshop_{RIMWORLD_APP_ID}.acf"))
    }
    pub fn console_log(&self) -> PathBuf {
        self.tool_dir().join("logs").join("console_log.txt")
    }
    pub fn is_installed(&self) -> bool {
        self.exe().is_file()
    }

    /// Download and unpack SteamCMD, then run it once so it can update itself.
    pub async fn install(&self, client: &reqwest::Client, log: &(dyn Fn(String) + Sync)) -> Result<()> {
        std::fs::create_dir_all(self.tool_dir())?;
        std::fs::create_dir_all(self.install_dir())?;
        log(format!("Downloading SteamCMD from {DOWNLOAD_URL}"));
        let bytes = client.get(DOWNLOAD_URL).send().await?.error_for_status()?.bytes().await?;
        log(format!("Unpacking {} KB", bytes.len() / 1024));
        let dir = self.tool_dir();
        tokio::task::spawn_blocking(move || unpack(&bytes, &dir)).await.map_err(|e| Error::Other(e.to_string()))??;
        if !self.is_installed() {
            return Err(Error::Other("SteamCMD archive did not contain the expected executable".into()));
        }
        log("First run: SteamCMD updates itself (this can take a minute)".into());
        let out = self.run_raw(&["+quit".to_string()], Duration::from_secs(600), &mut |l| log(l.to_string())).await?;
        if out.stalled {
            return Err(Error::Other("SteamCMD did not finish its first-run update".into()));
        }
        Ok(())
    }

    /// Log in anonymously and quit: proves the install works and that its output reaches us.
    pub async fn test(&self, on_line: &mut (dyn FnMut(&str) + Send)) -> Result<TestOutcome> {
        let started = Instant::now();
        let mut logged_in = false;
        let mut lines = 0usize;
        let raw = self
            .run_raw(&["+login".to_string(), "anonymous".to_string(), "+quit".to_string()], Duration::from_secs(180), &mut |line| {
                lines += 1;
                let lower = line.to_ascii_lowercase();
                if lower.contains("logged in ok") || lower.contains("waiting for user info...ok") || lower.contains("waiting for client config...ok") {
                    logged_in = true;
                }
                on_line(line);
            })
            .await?;
        Ok(TestOutcome { logged_in, lines, stalled: raw.stalled, exit_code: raw.exit_code, seconds: started.elapsed().as_secs() })
    }

    /// Write a runscript for a batch: (item id, validate?).
    pub fn write_script(&self, items: &[(u64, bool)]) -> Result<PathBuf> {
        let mut s = String::new();
        s.push_str("@ShutdownOnFailedCommand 0\n@NoPromptForPassword 1\n");
        s.push_str(&format!("force_install_dir \"{}\"\n", self.install_dir().display()));
        s.push_str("login anonymous\n");
        for (id, validate) in items {
            s.push_str(&format!("workshop_download_item {RIMWORLD_APP_ID} {id}{}\n", if *validate { " validate" } else { "" }));
        }
        s.push_str("quit\n");
        let path = self.root.join("batch.txt");
        std::fs::write(&path, s)?;
        Ok(path)
    }

    /// Run one batch and report per-item results.
    pub async fn run_batch(&self, items: &[(u64, bool)], stall: Duration, on_line: &mut (dyn FnMut(&str) + Send)) -> Result<BatchOutcome> {
        let script = self.write_script(items)?;
        let started = Instant::now();
        let mut results: HashMap<u64, ItemResult> = HashMap::new();
        let mut auth_failed = false;
        let mut current: Option<u64> = None;
        let raw = self
            .run_raw(&["+runscript".to_string(), script.display().to_string()], stall, &mut |line| {
                match parse_line(line) {
                    LineEvent::Downloading(id) => current = Some(id),
                    LineEvent::Success(id, path, bytes) => {
                        results.insert(id, ItemResult::Ok { path, bytes });
                    }
                    LineEvent::Failed(id, reason) => {
                        results.insert(id, ItemResult::Failed(reason));
                    }
                    LineEvent::AuthFailed(reason) => {
                        auth_failed = true;
                        on_line(&format!("Login problem: {reason}"));
                    }
                    LineEvent::Other => {}
                }
                on_line(line);
            })
            .await?;
        if raw.stalled {
            if let Some(id) = current {
                results.entry(id).or_insert(ItemResult::TimedOut);
            }
        }
        for (id, _) in items {
            results.entry(*id).or_insert(if raw.stalled || auth_failed { ItemResult::NotAttempted } else { ItemResult::Failed("SteamCMD exited without reporting this item".into()) });
        }
        Ok(BatchOutcome { results, auth_failed, stalled: raw.stalled, exit_code: raw.exit_code, seconds: started.elapsed().as_secs() })
    }

    /// Spawn SteamCMD, stream its lines (stdout, stderr and — on Windows — the console log),
    /// and kill it if it goes quiet for `stall`.
    async fn run_raw(&self, args: &[String], stall: Duration, on_line: &mut (dyn FnMut(&str) + Send)) -> Result<RawOutcome> {
        let exe = self.exe();
        if !exe.is_file() {
            return Err(Error::Other("SteamCMD is not installed yet".into()));
        }
        let log_path = self.console_log();
        let log_start = std::fs::metadata(&log_path).map(|m| m.len()).unwrap_or(0);
        let mut cmd = Command::new(&exe);
        cmd.args(args).current_dir(self.tool_dir()).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).kill_on_drop(true);
        #[cfg(target_os = "windows")]
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        let mut child = cmd.spawn().map_err(|e| Error::Other(format!("Could not start SteamCMD: {e}")))?;
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        if let Some(out) = child.stdout.take() {
            let tx = tx.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(out).lines();
                while let Ok(Some(l)) = lines.next_line().await {
                    let _ = tx.send(l);
                }
            });
        }
        if let Some(err) = child.stderr.take() {
            let tx = tx.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(err).lines();
                while let Ok(Some(l)) = lines.next_line().await {
                    let _ = tx.send(l);
                }
            });
        }
        // Windows: SteamCMD writes progress to the console, not the pipe; tail its log file.
        let tail_stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        if cfg!(target_os = "windows") {
            let tx = tx.clone();
            let stop = tail_stop.clone();
            tokio::spawn(async move {
                let mut offset = log_start;
                let mut carry = String::new();
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    tokio::time::sleep(Duration::from_millis(400)).await;
                    if let Ok(text) = tokio::fs::read(&log_path).await {
                        if (text.len() as u64) > offset {
                            let chunk = String::from_utf8_lossy(&text[offset as usize..]).to_string();
                            offset = text.len() as u64;
                            carry.push_str(&chunk);
                            while let Some(nl) = carry.find('\n') {
                                let line = carry[..nl].trim_end_matches('\r').to_string();
                                carry = carry[nl + 1..].to_string();
                                let _ = tx.send(line);
                            }
                        }
                    }
                }
            });
        }
        drop(tx);
        let mut stalled = false;
        let mut exit_code = None;
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        loop {
            tokio::select! {
                line = rx.recv() => {
                    match line {
                        Some(l) => {
                            let clean = strip_ansi(&l);
                            let clean = clean.trim();
                            if clean.is_empty() { continue; }
                            // stdout and the log file can carry the same line; report each once.
                            if seen.insert(clean.to_string()) { on_line(clean); }
                        }
                        None => break,
                    }
                }
                status = child.wait() => {
                    exit_code = status.ok().and_then(|s| s.code());
                    // drain whatever is left, briefly
                    let deadline = Instant::now() + Duration::from_secs(2);
                    while let Ok(Some(l)) = tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), rx.recv()).await {
                        let clean = strip_ansi(&l);
                        let clean = clean.trim();
                        if !clean.is_empty() && seen.insert(clean.to_string()) { on_line(clean); }
                    }
                    break;
                }
                _ = tokio::time::sleep(stall) => {
                    stalled = true;
                    on_line("No output from SteamCMD for too long. Stopping it");
                    let _ = child.kill().await;
                    break;
                }
            }
        }
        tail_stop.store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(RawOutcome { stalled, exit_code })
    }

    /// Move a finished download into the Mods folder as `<id>` and mark it as a SteamCMD mod.
    ///
    /// **The item is removed from the workshop ACF as part of the move**, and that is not
    /// tidying. SteamCMD builds a download out of chunks it believes are already on disk: an
    /// item still listed as installed, whose folder has gone, makes a *later* download that
    /// shares a file with it fail while reading that chunk. SteamCMD answers that failure by
    /// validating the whole app, which re-downloads every item it still has listed -- all the
    /// ones moved before it, hundreds of megabytes, into a folder they were just taken out of.
    ///
    /// The file header has said a moved item must be forgotten since this module was written.
    /// What was missing is that `forget` only ran *before* a batch and only for that batch's own
    /// ids, so nothing ever removed an item after `collect` moved it, and the very next batch
    /// three seconds later walked into it.
    pub fn collect(&self, id: u64, mods_dir: &Path) -> Result<PathBuf> {
        let src = self.downloads_dir().join(id.to_string());
        if !src.is_dir() {
            return Err(Error::Other(format!("SteamCMD reported success but {} is missing", src.display())));
        }
        std::fs::create_dir_all(mods_dir)?;
        let dest = mods_dir.join(id.to_string());
        if dest.exists() {
            std::fs::remove_dir_all(&dest)?;
        }
        move_dir(&src, &dest)?;
        // After the move, not before: until the files are somewhere else, the ACF entry is true.
        self.forget_listed(&[id]);
        let about = dest.join("About");
        std::fs::create_dir_all(&about)?;
        let marker = about.join("PublishedFileId.txt");
        if !marker.exists() {
            std::fs::write(marker, id.to_string())?;
        }
        Ok(dest)
    }

    /// Remove ids from the workshop ACF, and nothing else.
    ///
    /// Separate from `forget` because `collect` runs inside a batch: clearing the depot cache
    /// mid-batch would throw away chunks the items still downloading are about to want.
    fn forget_listed(&self, ids: &[u64]) {
        let acf = self.acf_path();
        if let Ok(text) = std::fs::read_to_string(&acf) {
            let (new_text, _) = acf::forget_items(&text, ids);
            let _ = std::fs::write(&acf, new_text);
        }
    }

    /// Forget **everything** the workshop ACF lists, and clear the content folder with it.
    ///
    /// Run before every batch. `forget(&ids)` was not enough: it cleared the ids about to be
    /// downloaded, and left every item some earlier run had moved away still listed. One of
    /// those is all it takes, because the failure is triggered by whatever the *new* download
    /// shares a file with, which is not something the queue can predict.
    ///
    /// Clearing the content folder as well is the other half. A leftover folder there is a
    /// download nobody collected -- a crash, a kill, a failed move -- and it is equally capable
    /// of being half of a chunk reuse.
    ///
    /// Nothing here touches Steam's own install. This is Circinus's private SteamCMD prefix
    /// under its data folder, and the ACF being cleared is that prefix's own.
    pub fn forget_everything(&self) -> Result<()> {
        let acf = self.acf_path();
        let listed: Vec<u64> = std::fs::read_to_string(&acf).map(|t| acf::installed_items(&t).into_iter().map(|(id, _)| id).collect()).unwrap_or_default();
        if !listed.is_empty() {
            self.forget_listed(&listed);
        }
        for dir in [self.tool_dir().join("depotcache"), self.install_dir().join("depotcache"), self.install_dir().join("steamapps").join("workshop").join("downloads"), self.install_dir().join("steamapps").join("workshop").join("temp")] {
            if dir.exists() {
                let _ = std::fs::remove_dir_all(&dir);
            }
        }
        // Anything still sitting in content is an uncollected download, and keeping it buys
        // nothing: the queue re-downloads what it needs, and `collect` reads from a fresh folder.
        if let Ok(rd) = std::fs::read_dir(self.downloads_dir()) {
            for e in rd.flatten() {
                if e.path().is_dir() {
                    let _ = std::fs::remove_dir_all(e.path());
                }
            }
        }
        Ok(())
    }

    /// Replace Steam's own copy of a subscribed item with what SteamCMD just downloaded.
    ///
    /// **Why this exists.** Every SteamCMD download used to land in `Mods/<id>`, Force update of
    /// a subscribed mod included. When the same packageId exists in both Mods and the Workshop
    /// folder, RimWorld suffixes the *Workshop* copy with `_steam`, so the plain packageId in
    /// ModsConfig.xml now names the copy in Mods. From that moment the game loads Circinus's
    /// copy, Steam goes on updating a folder nothing reads, and the mod is frozen at whatever
    /// Force update fetched until somebody deletes the folder by hand.
    ///
    /// So a Steam mod is replaced where Steam keeps it. Three rules hold this safe:
    ///
    /// - **Never while Steam is working on the item.** A `downloads/<id>` or `temp/<id>` staging
    ///   folder beside the content folder means Steam has the item open; replacing under it
    ///   would race a process we do not control.
    /// - **The old copy is moved aside, not deleted**, and moved back if the new one cannot be
    ///   put in place. A failure here must cost the player nothing; deleting first and failing
    ///   second would uninstall a mod they still have subscribed.
    /// - **Steam's `appworkshop_294100.acf` is never written.** It is Steam's bookkeeping, and
    ///   the worst outcome of leaving it alone is that Steam re-downloads the item later, which
    ///   is a slow correct answer rather than a fast corrupt one.
    pub fn replace_workshop_copy(&self, id: u64, workshop_dir: &Path) -> Result<PathBuf> {
        let src = self.downloads_dir().join(id.to_string());
        if !src.is_dir() {
            return Err(Error::Other(format!("SteamCMD reported success but {} is missing", src.display())));
        }
        // `workshop_dir` is Steam's `steamapps/workshop/content/294100`; its grandparent holds
        // the staging folders Steam uses while it downloads.
        let ws_root = workshop_dir.parent().and_then(|p| p.parent()).map(|p| p.to_path_buf());
        if let Some(root) = &ws_root {
            for staging in ["downloads", "temp"] {
                if root.join(staging).join(RIMWORLD_APP_ID.to_string()).join(id.to_string()).exists() {
                    return Err(Error::Other(format!("Steam is downloading {id} right now. Let it finish and try again")));
                }
            }
        }
        let dest = workshop_dir.join(id.to_string());
        std::fs::create_dir_all(workshop_dir)?;

        // Aside, and OUTSIDE content/: a folder parked next to the real ones would be scanned
        // as a mod, and on the next launch RimWorld would see the same packageId twice.
        let aside = if dest.exists() {
            let holding = self.root.join("replaced");
            std::fs::create_dir_all(&holding)?;
            let parked = holding.join(format!("{id}.{}", std::process::id()));
            if parked.exists() {
                std::fs::remove_dir_all(&parked)?;
            }
            move_dir(&dest, &parked)?;
            Some(parked)
        } else {
            None
        };

        match move_dir(&src, &dest) {
            Ok(()) => {
                self.forget_listed(&[id]);
                if let Some(parked) = aside {
                    let _ = std::fs::remove_dir_all(parked);
                }
                Ok(dest)
            }
            Err(e) => {
                // Put it back. The player subscribed to this mod and must still have it.
                if let Some(parked) = aside {
                    let _ = move_dir(&parked, &dest);
                }
                Err(e)
            }
        }
    }

    /// Make SteamCMD forget items so a later download really downloads: purge them from the
    /// workshop ACF and clear caches that would otherwise short-circuit the fetch.
    pub fn forget(&self, ids: &[u64]) -> Result<()> {
        let acf = self.acf_path();
        if let Ok(text) = std::fs::read_to_string(&acf) {
            let (new_text, _) = acf::forget_items(&text, ids);
            std::fs::write(&acf, new_text)?;
        }
        for dir in [self.tool_dir().join("depotcache"), self.install_dir().join("depotcache"), self.install_dir().join("steamapps").join("workshop").join("downloads"), self.install_dir().join("steamapps").join("workshop").join("temp")] {
            if dir.exists() {
                let _ = std::fs::remove_dir_all(&dir);
            }
        }
        for id in ids {
            let leftover = self.downloads_dir().join(id.to_string());
            if leftover.exists() {
                let _ = std::fs::remove_dir_all(&leftover);
            }
        }
        Ok(())
    }
}

fn unpack(bytes: &[u8], dir: &Path) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| Error::Other(format!("bad zip: {e}")))?;
        archive.extract(dir).map_err(|e| Error::Other(format!("unzip failed: {e}")))?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let gz = flate2::read::GzDecoder::new(bytes);
        let mut tar = tar::Archive::new(gz);
        tar.unpack(dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for name in ["steamcmd.sh", "steamcmd"] {
                let p = dir.join(name);
                if p.exists() {
                    let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755));
                }
            }
        }
    }
    Ok(())
}

/// Rename, falling back to copy + delete across drives.
pub fn move_dir(src: &Path, dest: &Path) -> Result<()> {
    if std::fs::rename(src, dest).is_ok() {
        return Ok(());
    }
    copy_dir(src, dest)?;
    std::fs::remove_dir_all(src)?;
    Ok(())
}

fn copy_dir(src: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let target = dest.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

pub struct RawOutcome {
    pub stalled: bool,
    pub exit_code: Option<i32>,
}

/// Result of `SteamCmd::test`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestOutcome {
    /// SteamCMD reported an anonymous login.
    pub logged_in: bool,
    /// Lines of output that reached Circinus (0 on Windows means the console-log tail failed).
    pub lines: usize,
    pub stalled: bool,
    pub exit_code: Option<i32>,
    pub seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemResult {
    Ok { path: PathBuf, bytes: u64 },
    Failed(String),
    TimedOut,
    /// The batch died before SteamCMD reached this item; it costs no attempt.
    NotAttempted,
}

#[derive(Debug)]
pub struct BatchOutcome {
    pub results: HashMap<u64, ItemResult>,
    pub auth_failed: bool,
    pub stalled: bool,
    pub exit_code: Option<i32>,
    pub seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineEvent {
    Downloading(u64),
    Success(u64, PathBuf, u64),
    Failed(u64, String),
    AuthFailed(String),
    Other,
}

fn strip_ansi(s: &str) -> String {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"\x1b\[[0-9;?]*[A-Za-z]").unwrap());
    re.replace_all(s, "").to_string()
}

/// Classify one line of SteamCMD output.
pub fn parse_line(line: &str) -> LineEvent {
    static DOWNLOADING: OnceLock<regex::Regex> = OnceLock::new();
    static SUCCESS: OnceLock<regex::Regex> = OnceLock::new();
    static FAILED: OnceLock<regex::Regex> = OnceLock::new();
    static TIMEOUT: OnceLock<regex::Regex> = OnceLock::new();
    let downloading = DOWNLOADING.get_or_init(|| regex::Regex::new(r"Downloading item (\d+)").unwrap());
    let success = SUCCESS.get_or_init(|| regex::Regex::new(r#"Success\. Downloaded item (\d+) to "([^"]+)" \((\d+) bytes\)"#).unwrap());
    let failed = FAILED.get_or_init(|| regex::Regex::new(r"Download item (\d+) failed \(([^)]*)\)").unwrap());
    let timeout = TIMEOUT.get_or_init(|| regex::Regex::new(r"Timeout downloading item (\d+)").unwrap());
    if let Some(c) = success.captures(line) {
        return LineEvent::Success(c[1].parse().unwrap_or(0), PathBuf::from(&c[2]), c[3].parse().unwrap_or(0));
    }
    if let Some(c) = failed.captures(line) {
        return LineEvent::Failed(c[1].parse().unwrap_or(0), c[2].to_string());
    }
    if let Some(c) = timeout.captures(line) {
        return LineEvent::Failed(c[1].parse().unwrap_or(0), "Timeout".into());
    }
    if let Some(c) = downloading.captures(line) {
        return LineEvent::Downloading(c[1].parse().unwrap_or(0));
    }
    let lower = line.to_ascii_lowercase();
    if lower.contains("not logged on") || lower.contains("login failure") || (lower.contains("failed") && lower.contains("rate limit")) || lower.contains("rate limit exceeded") {
        return LineEvent::AuthFailed(line.trim().to_string());
    }
    LineEvent::Other
}

/// Adaptive batch size and exponential cooldown, updated after every batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Throttle {
    pub batch_size: usize,
    pub clean_streak: u32,
    /// 0 = calm; each throttled batch raises it, each clean batch lowers it.
    pub level: u32,
    /// Unix seconds until which no batch may start.
    pub cooldown_until: Option<i64>,
    pub last: Option<BatchStats>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchStats {
    pub requested: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub timed_out: usize,
    pub auth_failed: bool,
    pub stalled: bool,
    pub seconds: u64,
}

impl Default for Throttle {
    fn default() -> Self {
        Throttle { batch_size: MAX_BATCH, clean_streak: 0, level: 0, cooldown_until: None, last: None }
    }
}

impl Throttle {
    pub fn cooldown_seconds(level: u32) -> i64 {
        if level == 0 {
            0
        } else {
            (30i64 * (1i64 << (level - 1).min(5))).min(600)
        }
    }

    pub fn after_batch(&mut self, s: &BatchStats, now: i64) {
        let bad = s.failed + s.timed_out;
        let throttled = s.auth_failed || s.stalled || (s.requested > 0 && bad * 2 >= s.requested);
        if throttled {
            self.level += 1;
            self.clean_streak = 0;
            self.batch_size = (self.batch_size / 2).max(MIN_BATCH);
            self.cooldown_until = Some(now + Self::cooldown_seconds(self.level));
        } else if bad > 0 {
            self.clean_streak = 0;
            self.batch_size = (self.batch_size * 2 / 3).max(MIN_BATCH);
            self.cooldown_until = None;
        } else {
            self.clean_streak += 1;
            self.level = self.level.saturating_sub(1);
            self.cooldown_until = None;
            if self.clean_streak >= 2 {
                self.batch_size = (self.batch_size * 3 / 2).clamp(MIN_BATCH, MAX_BATCH);
            }
        }
        self.last = Some(s.clone());
    }

    pub fn remaining_cooldown(&self, now: i64) -> i64 {
        self.cooldown_until.map(|t| (t - now).max(0)).unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemStatus {
    Queued,
    Downloading,
    Done,
    Failed,
    Cancelled,
}

/// Which copy of a mod a download is meant to replace.
///
/// **Decided when the item is queued, not when it finishes.** The destination used to be worked
/// out at the end, from the id alone: if any installed Workshop mod had that id, the download
/// went to Steam's folder. That is wrong the moment a mod exists twice, which is exactly what
/// "Keep my own copy" creates -- a copy in `Mods/<id>` *and* a live subscription with the same
/// id. Force update on the kept copy then replaced Steam's copy instead, the kept copy stayed
/// at its old version, the rescan found it still behind, and it sat on the update list for
/// ever being "updated" into the wrong folder. The two Force update buttons were
/// indistinguishable by the time the decision was made.
///
/// The caller always knows which copy the user pressed: every entry point has the `ModInfo`
/// in hand. This is that knowledge, carried along instead of thrown away and guessed at.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dest {
    /// Steam's own workshop folder: replace the subscribed copy where Steam keeps it.
    Workshop,
    /// The game's Mods folder, at `Mods/<id>`. New downloads and kept copies both land here.
    Mods,
    /// Not stated. What an item queued by an older build looks like, and the only case that
    /// still falls back to reading the installed list at completion time -- which is why it
    /// is the default: an upgrade must change nothing about a queue already on disk.
    #[default]
    Unsaid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueItem {
    pub id: u64,
    /// Where this download is to be put. See `Dest`.
    ///
    /// `#[serde(default)]` is load-bearing: the queue is persisted as JSON in the cache and is
    /// read back with `.ok().flatten()`, so a field an old row lacks would fail the whole parse
    /// and silently empty somebody's queue on upgrade. The default is `Unsaid`, which behaves
    /// exactly as this build's predecessor did.
    #[serde(default)]
    pub dest: Dest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub status: ItemStatus,
    pub attempts: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub added_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<i64>,
}

/// The persistent download queue plus the live state the UI renders.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct QueueState {
    pub items: Vec<QueueItem>,
    pub throttle: Throttle,
    pub paused: bool,
    /// Ids in the batch SteamCMD is working on right now.
    pub current_batch: Vec<u64>,
    /// Item SteamCMD last reported as downloading.
    pub current_item: Option<u64>,
    pub running: bool,
    pub steamcmd_installed: bool,
    pub installing: bool,
    /// Last lines of SteamCMD output, newest last.
    pub log: Vec<String>,
}

impl QueueState {
    /// Add ids; an id already queued or downloading is left alone, a finished or failed one is
    /// re-queued.
    pub fn add(&mut self, ids: &[u64], names: &HashMap<u64, String>, dests: &HashMap<u64, Dest>, now: i64) -> usize {
        let mut added = 0;
        for id in ids {
            let dest = dests.get(id).copied().unwrap_or_default();
            match self.items.iter_mut().find(|i| i.id == *id) {
                Some(existing) => {
                    if matches!(existing.status, ItemStatus::Done | ItemStatus::Failed | ItemStatus::Cancelled) {
                        existing.status = ItemStatus::Queued;
                        existing.attempts = 0;
                        existing.error = None;
                        existing.finished_at = None;
                        added += 1;
                    }
                    // The destination is overwritten, not kept. A second Force update is a new
                    // decision about where this mod goes, and both copies of a localized mod
                    // share an id -- so a stale destination here would send the second press
                    // wherever the first one went.
                    if dest != Dest::Unsaid {
                        existing.dest = dest;
                    }
                    if existing.name.is_none() {
                        existing.name = names.get(id).cloned();
                    }
                }
                None => {
                    self.items.push(QueueItem { id: *id, dest, name: names.get(id).cloned(), status: ItemStatus::Queued, attempts: 0, error: None, bytes: None, path: None, added_at: now, finished_at: None });
                    added += 1;
                }
            }
        }
        added
    }

    pub fn remove(&mut self, ids: &[u64]) {
        self.items.retain(|i| !(ids.contains(&i.id) && i.status != ItemStatus::Downloading));
        for i in self.items.iter_mut().filter(|i| ids.contains(&i.id) && i.status == ItemStatus::Downloading) {
            i.status = ItemStatus::Cancelled;
        }
    }

    pub fn retry_failed(&mut self) -> usize {
        let mut n = 0;
        for i in self.items.iter_mut().filter(|i| i.status == ItemStatus::Failed) {
            i.status = ItemStatus::Queued;
            i.attempts = 0;
            i.error = None;
            n += 1;
        }
        n
    }

    pub fn clear_finished(&mut self) {
        self.items.retain(|i| !matches!(i.status, ItemStatus::Done | ItemStatus::Cancelled));
    }

    /// The next batch to run: (id, validate) — validate after the first failure.
    pub fn next_batch(&self) -> Vec<(u64, bool)> {
        self.items.iter().filter(|i| i.status == ItemStatus::Queued).take(self.throttle.batch_size.max(1)).map(|i| (i.id, i.attempts > 0)).collect()
    }

    pub fn queued(&self) -> usize {
        self.items.iter().filter(|i| i.status == ItemStatus::Queued).count()
    }

    /// Fold a batch outcome into the items. Returns the ids that were downloaded successfully.
    pub fn apply(&mut self, outcome: &BatchOutcome, now: i64) -> Vec<u64> {
        let mut done = Vec::new();
        for item in self.items.iter_mut().filter(|i| outcome.results.contains_key(&i.id)) {
            match &outcome.results[&item.id] {
                ItemResult::Ok { path, bytes } => {
                    item.status = ItemStatus::Done;
                    item.bytes = Some(*bytes);
                    item.path = Some(path.display().to_string());
                    item.error = None;
                    item.finished_at = Some(now);
                    done.push(item.id);
                }
                ItemResult::NotAttempted => {
                    if item.status != ItemStatus::Cancelled {
                        item.status = ItemStatus::Queued;
                    }
                }
                other => {
                    let reason = match other {
                        ItemResult::Failed(r) => r.clone(),
                        _ => "Timed out".to_string(),
                    };
                    item.attempts += 1;
                    item.error = Some(reason);
                    item.status = if item.attempts >= MAX_ATTEMPTS { ItemStatus::Failed } else { ItemStatus::Queued };
                    if item.status == ItemStatus::Failed {
                        item.finished_at = Some(now);
                    }
                }
            }
        }
        let stats = BatchStats {
            requested: outcome.results.values().filter(|r| !matches!(r, ItemResult::NotAttempted)).count(),
            succeeded: done.len(),
            failed: outcome.results.values().filter(|r| matches!(r, ItemResult::Failed(_))).count(),
            timed_out: outcome.results.values().filter(|r| matches!(r, ItemResult::TimedOut)).count(),
            auth_failed: outcome.auth_failed,
            stalled: outcome.stalled,
            seconds: outcome.seconds,
        };
        self.throttle.after_batch(&stats, now);
        self.current_batch.clear();
        self.current_item = None;
        done
    }

    pub fn push_log(&mut self, line: &str) {
        self.log.push(line.to_string());
        if self.log.len() > 200 {
            let drop = self.log.len() - 200;
            self.log.drain(..drop);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An ACF listing three items, in the shape SteamCMD writes.
    fn acf_with(ids: &[u64]) -> String {
        let items: String = ids.iter().map(|id| format!("\t\t\"{id}\"\n\t\t{{\n\t\t\t\"size\"\t\t\"100\"\n\t\t\t\"timeupdated\"\t\t\"1700000000\"\n\t\t}}\n")).collect();
        format!("\"AppWorkshop\"\n{{\n\t\"appid\"\t\t\"294100\"\n\t\"WorkshopItemsInstalled\"\n\t{{\n{items}\t}}\n\t\"WorkshopItemDetails\"\n\t{{\n{items}\t}}\n}}\n")
    }

    fn lab(name: &str) -> (tempfile::TempDir, SteamCmd) {
        let tmp = tempfile::tempdir().unwrap();
        let cmd = SteamCmd::new(tmp.path().join(name));
        std::fs::create_dir_all(cmd.downloads_dir()).unwrap();
        std::fs::create_dir_all(cmd.acf_path().parent().unwrap()).unwrap();
        (tmp, cmd)
    }

    fn listed(cmd: &SteamCmd) -> Vec<u64> {
        let text = std::fs::read_to_string(cmd.acf_path()).unwrap_or_default();
        let mut v: Vec<u64> = acf::installed_items(&text).into_iter().map(|(id, _)| id).collect();
        v.sort_unstable();
        v
    }

    /// The bug this whole change is about.
    ///
    /// SteamCMD builds a download out of chunks it believes are on disk. An item still listed
    /// as installed, whose folder has moved into Mods, makes a LATER download that shares a
    /// file with it fail while reading that chunk -- and SteamCMD answers a read failure by
    /// validating the whole app, which re-downloads every item it still has listed. One report
    /// had 9,797 files come back three seconds after a batch of 25 was collected.
    ///
    /// The file header has said a moved item must be forgotten since this module was written.
    /// What was missing is that nothing did it after the move.
    #[test]
    fn collecting_an_item_takes_it_out_of_the_acf() {
        let (tmp, cmd) = lab("prefix");
        std::fs::write(cmd.acf_path(), acf_with(&[2009463077, 818773962])).unwrap();
        let src = cmd.downloads_dir().join("2009463077");
        std::fs::create_dir_all(src.join("About")).unwrap();
        std::fs::write(src.join("About/About.xml"), "<ModMetaData/>").unwrap();

        let mods = tmp.path().join("Mods");
        let dest = cmd.collect(2009463077, &mods).unwrap();

        assert!(dest.join("About/About.xml").is_file(), "the files really moved");
        assert!(!cmd.downloads_dir().join("2009463077").exists(), "and are gone from content/");
        assert_eq!(listed(&cmd), vec![818773962], "the moved item is no longer listed as installed");
        // The other item was untouched: this is surgery, not a reset.
        assert!(std::fs::read_to_string(cmd.acf_path()).unwrap().contains("818773962"));
    }

    /// `forget(&ids)` cleared the ids about to be downloaded and left every item an earlier run
    /// had moved away still listed. The queue cannot predict which of those a new download will
    /// share a file with, so the only safe list is all of them.
    #[test]
    fn a_batch_starts_with_nothing_listed_and_nothing_left_over() {
        let (_tmp, cmd) = lab("prefix");
        std::fs::write(cmd.acf_path(), acf_with(&[2009463077, 818773962, 2023507013])).unwrap();
        // An uncollected download from a crash or a failed move: equally capable of being half
        // of a chunk reuse.
        let leftover = cmd.downloads_dir().join("2023507013");
        std::fs::create_dir_all(&leftover).unwrap();
        std::fs::write(leftover.join("x.txt"), "x").unwrap();

        cmd.forget_everything().unwrap();

        assert!(listed(&cmd).is_empty(), "everything, not just this batch's ids");
        assert!(!leftover.exists(), "and the content folder is empty too");
        assert!(cmd.acf_path().is_file(), "the file itself survives; only its item lists are emptied");
    }

    /// A Force update of a subscribed mod used to land in `Mods/<id>`. With the same packageId
    /// in both places RimWorld suffixes the WORKSHOP copy `_steam`, so ModsConfig.xml's plain
    /// packageId names Circinus's copy: the game loads that one, Steam goes on updating a folder
    /// nothing reads, and the mod is frozen until somebody deletes it by hand.
    #[test]
    fn a_subscribed_mod_is_replaced_where_steam_keeps_it() {
        let (tmp, cmd) = lab("prefix");
        std::fs::write(cmd.acf_path(), acf_with(&[2009463077])).unwrap();
        let ws = tmp.path().join("library/steamapps/workshop/content/294100");
        let old = ws.join("2009463077");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("old.txt"), "the version Steam installed").unwrap();
        let src = cmd.downloads_dir().join("2009463077");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("new.txt"), "the version SteamCMD fetched").unwrap();

        let dest = cmd.replace_workshop_copy(2009463077, &ws).unwrap();

        assert_eq!(dest, old, "in place, so no second copy with the same packageId exists");
        assert!(dest.join("new.txt").is_file());
        assert!(!dest.join("old.txt").exists(), "replaced, not merged");
        assert!(!tmp.path().join("Mods/2009463077").exists(), "and nothing was written to Mods");
        // Steam's own bookkeeping is never written. Ours is.
        assert!(listed(&cmd).is_empty(), "the item leaves OUR acf, the same as a collect");
    }

    /// Replacing under a Steam that is mid-download would race a process we do not control, so
    /// it is refused and said plainly rather than attempted carefully.
    #[test]
    fn steam_downloading_the_item_stops_the_replacement() {
        let (tmp, cmd) = lab("prefix");
        let ws = tmp.path().join("library/steamapps/workshop/content/294100");
        let old = ws.join("2009463077");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("old.txt"), "still Steam's").unwrap();
        std::fs::create_dir_all(cmd.downloads_dir().join("2009463077")).unwrap();
        // Steam's staging folder for the item, beside content/.
        std::fs::create_dir_all(tmp.path().join("library/steamapps/workshop/downloads/294100/2009463077")).unwrap();

        let err = cmd.replace_workshop_copy(2009463077, &ws).unwrap_err();
        assert!(err.to_string().contains("downloading"), "{err}");
        assert!(old.join("old.txt").is_file(), "and Steam's copy is exactly as it was");
    }

    /// A missing download is refused before anything is touched.
    ///
    /// This used to be called `a_failed_replacement_puts_steams_copy_back` and did not test
    /// that: with no download to move in, `replace_workshop_copy` returns at its first guard
    /// and never reaches the park-aside or the restore. It was a duplicate of this assertion
    /// wearing the name of the one below, so the restore path had no cover at all.
    #[test]
    fn a_missing_download_is_refused_before_anything_moves() {
        let (tmp, cmd) = lab("prefix");
        let ws = tmp.path().join("library/steamapps/workshop/content/294100");
        let old = ws.join("2009463077");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("old.txt"), "the version Steam installed").unwrap();
        let err = cmd.replace_workshop_copy(2009463077, &ws).unwrap_err();
        assert!(err.to_string().contains("missing"), "{err}");
        assert!(old.join("old.txt").is_file(), "Steam's copy is untouched");
    }

    /// A failure must cost the player nothing. The old copy is moved aside rather than deleted,
    /// so a mod they are still subscribed to cannot be uninstalled by a half-done update.
    ///
    /// Reaching the restore means getting past the pre-flight guard and then failing the move,
    /// and `move_dir` tries twice: `rename`, then a copy. Both have to fail, and neither can be
    /// made to fail with permissions, because tests run as root often enough that a
    /// permission trick is a test that quietly passes without executing anything.
    ///
    /// So: the download is put on a different filesystem from Steam's folder, which makes
    /// `rename` fail with `EXDEV` whoever is running; and it holds a symlink pointing at
    /// nothing, which `std::fs::copy` fails on because it follows the link. The failure is
    /// real, it is the production code path, and it does not depend on who runs the test.
    #[cfg(unix)]
    #[test]
    fn a_failed_replacement_puts_steams_copy_back() {
        // `/dev/shm` is tmpfs and the temp dir is not, so a rename between them is EXDEV.
        // Without a second filesystem there is no way to fail a rename, so the test says so
        // out loud rather than passing on an assertion it never reached.
        let shm = std::path::Path::new("/dev/shm");
        if !shm.is_dir() {
            eprintln!("skipped: no second filesystem to force a cross-device rename");
            return;
        }
        let other = shm.join(format!("circinus-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&other);
        let cmd = SteamCmd::new(other.join("prefix"));
        std::fs::create_dir_all(cmd.downloads_dir()).unwrap();
        std::fs::create_dir_all(cmd.acf_path().parent().unwrap()).unwrap();

        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().join("library/steamapps/workshop/content/294100");
        let old = ws.join("2009463077");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("old.txt"), "the version Steam installed").unwrap();

        // A download that exists, so the pre-flight guard passes, and that cannot be copied.
        let src = cmd.downloads_dir().join("2009463077");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("new.txt"), "the version SteamCMD fetched").unwrap();
        // A symlink pointing at nothing. `copy_dir` sees a non-directory and calls
        // `std::fs::copy`, which follows the link and fails because there is nothing there.
        //
        // A FIFO was the first idea and is worse than wrong: `std::fs::copy` *opens* the
        // source, and opening a FIFO for reading blocks until somebody writes to it, so the
        // test hung instead of failing. A dangling link fails immediately and needs no
        // dependency the crate does not already have.
        std::os::unix::fs::symlink("nowhere-at-all", src.join("dangling")).unwrap();

        let out = cmd.replace_workshop_copy(2009463077, &ws);
        let _ = std::fs::remove_dir_all(&other);

        assert!(out.is_err(), "the move should have failed: a cross-device rename of a folder holding a FIFO");
        assert!(old.join("old.txt").is_file(), "Steam's copy was not put back after a failed update");
        assert_eq!(std::fs::read_to_string(old.join("old.txt")).unwrap(), "the version Steam installed", "Steam's copy came back changed");
        // And nothing was left parked where a later scan would find it as a second mod.
        assert!(!cmd.root.join("replaced").join("2009463077.{}").exists());
    }

    /// **Where a download goes is decided when it is queued, not when it finishes** (#3).
    ///
    /// "Keep my own copy" leaves the Steam subscription in place, so a localized mod is
    /// installed twice under one workshop id. Deciding the destination at the end, from the
    /// id, cannot tell those two apart: it saw the subscription and sent every Force update to
    /// Steam's folder, so the kept copy the user pressed was never touched and stayed on the
    /// update list for ever.
    ///
    /// The three cases are written out rather than asserted as "a dest is stored", because the
    /// third one -- an item queued by a build that had no such field -- is the one that has to
    /// keep behaving exactly as before, and it is the one a careless default would break.
    #[test]
    fn a_queued_item_remembers_which_copy_it_is_for() {
        let now = 1_700_000_000;
        let names = HashMap::new();
        let mut q = QueueState::default();

        let mut dests = HashMap::new();
        dests.insert(2009463077u64, Dest::Mods);
        dests.insert(818773962u64, Dest::Workshop);
        q.add(&[2009463077, 818773962, 294100], &names, &dests, now);

        let of = |q: &QueueState, id: u64| q.items.iter().find(|i| i.id == id).unwrap().dest;
        assert_eq!(of(&q, 2009463077), Dest::Mods, "the kept copy's update was not marked for Mods");
        assert_eq!(of(&q, 818773962), Dest::Workshop);
        // Queued without one: nothing is claimed, and the old id-based guess still applies.
        assert_eq!(of(&q, 294100), Dest::Unsaid);

        // Re-queueing the other copy of the same mod is a new decision, and must overwrite.
        // Both copies share this id, so a destination kept from the first press would send the
        // second one to the wrong folder -- which is the bug, one step later.
        q.items[0].status = ItemStatus::Done;
        let mut again = HashMap::new();
        again.insert(2009463077u64, Dest::Workshop);
        q.add(&[2009463077], &names, &again, now + 1);
        assert_eq!(of(&q, 2009463077), Dest::Workshop, "a second Force update kept the first one's destination");

        // And an item written by an older build, which has no `dest` key at all, must load
        // rather than fail: the queue is persisted as JSON and a parse failure empties it.
        let old_row = r#"{"id":1,"status":"queued","attempts":0,"addedAt":1}"#;
        let item: QueueItem = serde_json::from_str(old_row).expect("an item from an older build no longer loads");
        assert_eq!(item.dest, Dest::Unsaid);
    }

    /// A folder parked inside content/ would be scanned as a mod, and RimWorld would see the
    /// same packageId twice on the next launch. It goes in the prefix instead.
    #[test]
    fn the_old_copy_is_parked_outside_the_content_folder() {
        let (tmp, cmd) = lab("prefix");
        let ws = tmp.path().join("library/steamapps/workshop/content/294100");
        std::fs::create_dir_all(ws.join("2009463077")).unwrap();
        std::fs::write(ws.join("2009463077/old.txt"), "old").unwrap();
        std::fs::create_dir_all(cmd.downloads_dir().join("2009463077")).unwrap();
        std::fs::write(cmd.downloads_dir().join("2009463077/new.txt"), "new").unwrap();

        cmd.replace_workshop_copy(2009463077, &ws).unwrap();

        let strays: Vec<String> = std::fs::read_dir(&ws).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
        assert_eq!(strays, vec!["2009463077".to_string()], "exactly one folder in content/");
    }

    #[test]
    fn parses_steamcmd_lines() {
        assert_eq!(parse_line("Downloading item 2009463077 ..."), LineEvent::Downloading(2009463077));
        assert_eq!(parse_line(r#"Success. Downloaded item 2009463077 to "C:\x\steamapps\workshop\content\294100\2009463077" (935611 bytes) "#), LineEvent::Success(2009463077, PathBuf::from(r"C:\x\steamapps\workshop\content\294100\2009463077"), 935611));
        assert_eq!(parse_line("ERROR! Download item 818773962 failed (Failure)."), LineEvent::Failed(818773962, "Failure".into()));
        assert_eq!(parse_line("ERROR! Timeout downloading item 818773962"), LineEvent::Failed(818773962, "Timeout".into()));
        assert_eq!(parse_line("ERROR! Not logged on."), LineEvent::AuthFailed("ERROR! Not logged on.".into()));
        assert_eq!(parse_line("FAILED (Rate Limit Exceeded)"), LineEvent::AuthFailed("FAILED (Rate Limit Exceeded)".into()));
        assert_eq!(parse_line("Waiting for user info...OK"), LineEvent::Other);
        assert_eq!(strip_ansi("\x1b[0mLoading Steam API...OK\x1b[0m"), "Loading Steam API...OK");
    }

    #[test]
    fn throttle_backs_off_and_recovers() {
        let mut t = Throttle::default();
        assert_eq!(t.batch_size, 25);
        let bad = BatchStats { requested: 25, succeeded: 5, failed: 20, timed_out: 0, auth_failed: false, stalled: false, seconds: 60 };
        t.after_batch(&bad, 1000);
        assert_eq!(t.batch_size, 12);
        assert_eq!(t.level, 1);
        assert_eq!(t.remaining_cooldown(1000), 30);
        t.after_batch(&bad, 2000);
        assert_eq!(t.batch_size, 6);
        assert_eq!(t.remaining_cooldown(2000), 60);
        let ok = BatchStats { requested: 6, succeeded: 6, failed: 0, timed_out: 0, auth_failed: false, stalled: false, seconds: 30 };
        t.after_batch(&ok, 3000);
        assert_eq!(t.level, 1);
        assert_eq!(t.remaining_cooldown(3000), 0);
        assert_eq!(t.batch_size, 6);
        t.after_batch(&ok, 3100);
        assert_eq!(t.batch_size, 9);
        assert_eq!(t.level, 0);
        for _ in 0..10 {
            t.after_batch(&ok, 4000);
        }
        assert_eq!(t.batch_size, 25);
        let stalled = BatchStats { requested: 25, succeeded: 24, failed: 0, timed_out: 1, auth_failed: false, stalled: true, seconds: 200 };
        t.after_batch(&stalled, 5000);
        assert_eq!(t.batch_size, 12);
        assert!(t.remaining_cooldown(5000) > 0);
        assert_eq!(Throttle::cooldown_seconds(9), 600);
    }

    #[test]
    fn queue_lifecycle() {
        let mut q = QueueState::default();
        let mut names = HashMap::new();
        names.insert(1u64, "One".to_string());
        assert_eq!(q.add(&[1, 2, 3], &names, &HashMap::new(), 10), 3);
        assert_eq!(q.add(&[1], &names, &HashMap::new(), 11), 0);
        assert_eq!(q.next_batch(), vec![(1, false), (2, false), (3, false)]);
        let mut results = HashMap::new();
        results.insert(1, ItemResult::Ok { path: PathBuf::from("/x/1"), bytes: 10 });
        results.insert(2, ItemResult::Failed("Failure".into()));
        results.insert(3, ItemResult::NotAttempted);
        let outcome = BatchOutcome { results, auth_failed: false, stalled: true, exit_code: None, seconds: 5 };
        let done = q.apply(&outcome, 20);
        assert_eq!(done, vec![1]);
        assert_eq!(q.items[0].status, ItemStatus::Done);
        assert_eq!(q.items[1].status, ItemStatus::Queued);
        assert_eq!(q.items[1].attempts, 1);
        assert_eq!(q.items[2].attempts, 0);
        assert_eq!(q.next_batch(), vec![(2, true), (3, false)]);
        for _ in 0..3 {
            let mut r = HashMap::new();
            r.insert(2, ItemResult::Failed("Failure".into()));
            q.apply(&BatchOutcome { results: r, auth_failed: false, stalled: false, exit_code: Some(0), seconds: 1 }, 30);
        }
        assert_eq!(q.items[1].status, ItemStatus::Failed);
        assert_eq!(q.retry_failed(), 1);
        q.clear_finished();
        assert_eq!(q.items.len(), 2);
        q.remove(&[3]);
        assert_eq!(q.items.len(), 1);
    }
}
