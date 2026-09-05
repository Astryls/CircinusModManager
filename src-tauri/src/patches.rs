//! What every active mod's code patches, as a background job: find the assemblies, hand them to
//! the Harmony scanner a batch at a time, and fold the answers into the report the UI shows.
//!
//! Reading two thousand assemblies takes seconds rather than milliseconds, so this follows the
//! texture job's shape — the state behind a mutex, progress as an event, a stop flag — and the
//! window stays usable while it runs.

use crate::commands::Shared;
use circinus_core::harmony::{self, AssemblyPatches, ManualPatch, ModPatches, PatchReport, PatchTarget, TargetGroup};
use circinus_core::ModInfo;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

/// Assemblies per call to the scanner. Starting the process is most of the per-call cost, so
/// bigger is faster; small enough that progress moves and Stop is answered within a moment.
const BATCH: usize = 64;

/// The shape of a finished run, small enough to travel with every progress event. The report
/// itself can hold tens of thousands of methods and comes from `patches_report` instead.
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PatchSummary {
    pub mods: usize,
    pub assemblies: usize,
    pub targets: usize,
    pub contested: usize,
    /// Assemblies the scanner could not read at all.
    pub unreadable: usize,
    pub seconds: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchJob {
    pub running: bool,
    /// idle | collecting | scanning
    pub phase: String,
    pub done: usize,
    pub total: usize,
    /// The assembly being read, for the progress line.
    pub current: String,
    pub started_at: i64,
    pub finished_at: i64,
    pub cancelled: bool,
    /// Why the run could not finish, in words. Empty when it did.
    pub error: Option<String>,
    pub summary: Option<PatchSummary>,
}

/// One mod's answer for the Inspector and for a row that expands: the counts, and every method
/// its assemblies patch.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModPatchDetail {
    pub summary: ModPatches,
    pub targets: Vec<PatchTarget>,
    /// Where it patches in a way static reading cannot follow.
    pub manual: Vec<ManualPatch>,
    /// The contested methods this mod takes part in, most-fought-over first.
    pub contested: Vec<TargetGroup>,
}

/// What the last run found, kept so one mod's detail costs no scan.
struct Run {
    report: PatchReport,
    results: Vec<AssemblyPatches>,
    /// Assembly path → the uid of the mod it came from.
    owner: HashMap<String, String>,
}

pub struct Patches {
    state: Mutex<PatchJob>,
    cancel: AtomicBool,
    handle: AppHandle,
    app: Shared,
    last: Mutex<Option<Run>>,
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// The message every command gives when the sidecar is not on this machine. The view offers the
/// publish command; this is what anything else says.
const NO_SCANNER: &str = "The Harmony scanner is not installed. Circinus reads what mods patch with a small tool that ships beside it; without it there is nothing to report.";

impl Patches {
    pub fn new(handle: AppHandle, app: Shared) -> Arc<Patches> {
        Arc::new(Patches { state: Mutex::new(PatchJob { phase: "idle".into(), ..PatchJob::default() }), cancel: AtomicBool::new(false), handle, app, last: Mutex::new(None) })
    }

    pub fn snapshot(&self) -> PatchJob {
        self.state.lock().map(|s| s.clone()).unwrap_or_default()
    }

    fn emit(&self) {
        let _ = self.handle.emit("patch-progress", self.snapshot());
    }

    pub fn stop(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Where the sidecar is, or None when it is not installed.
    pub fn scanner(&self) -> Option<PathBuf> {
        let dir = self.app.lock().ok()?.data_dir.clone();
        harmony::scanner_path(&dir)
    }

    fn begin(&self) -> Result<(), String> {
        let mut s = self.state.lock().map_err(|_| "the patch job's state is unreadable".to_string())?;
        if s.running {
            return Err("A patch scan is already running".into());
        }
        self.cancel.store(false, Ordering::Relaxed);
        *s = PatchJob { running: true, phase: "collecting".into(), started_at: now(), ..PatchJob::default() };
        Ok(())
    }

    fn fail(&self, msg: String) -> Result<PatchReport, String> {
        if let Ok(mut s) = self.state.lock() {
            s.running = false;
            s.phase = "idle".into();
            s.finished_at = now();
            s.error = Some(msg.clone());
        }
        self.emit();
        Err(msg)
    }

    /// Every active mod's assemblies, as (uid, name, dlls).
    ///
    /// A mod's code lives in the `Assemblies` folder of each load folder the game would use for
    /// it. `defs::flatten::load_folders` is what answers that here rather than
    /// `circinus_core::textures::active_load_folders`: the latter derives its folders from the
    /// lowercased relative paths in `ModFiles`, which cannot be turned back into a real path on
    /// a filesystem that cares about case, and it has nothing to say until the background
    /// inspection has run. `flatten::load_folders` reads the mod's own LoadFolders.xml (or
    /// probes the folder the way RimWorld does) and gives names as they are on disk.
    ///
    /// Taking the load folders rather than every `.dll` under the mod matters: a mod that ships
    /// `1.5/` and `1.6/` has two copies of the same assembly, and scanning both would report a
    /// mod as fighting with itself over every method it patches.
    fn collect(&self) -> Result<Vec<(String, String, Vec<PathBuf>)>, String> {
        let app = self.app.lock().map_err(|_| "state lock poisoned".to_string())?;
        let active: HashSet<String> = app.active.iter().filter_map(|uid| app.mods.iter().find(|m| &m.uid == uid)).map(|m| m.package_id.clone()).filter(|p| !p.is_empty()).collect();
        let major_minor = app.game_version.major_minor.clone();
        let mut out = Vec::new();
        for uid in &app.active {
            let Some(m) = app.mods.iter().find(|m| &m.uid == uid) else { continue };
            if m.invalid.is_some() {
                continue;
            }
            let root = m.link_target.clone().unwrap_or_else(|| m.path.clone());
            let mut dlls: Vec<PathBuf> = Vec::new();
            for folder in circinus_core::defs::flatten::load_folders(m, &major_minor, &active) {
                let dir = join_folder(&root, &folder);
                if let Some(asm) = child_named(&dir, "Assemblies") {
                    collect_dlls(&asm, &mut dlls);
                }
            }
            dlls.sort();
            dlls.dedup();
            if !dlls.is_empty() {
                out.push((m.uid.clone(), m.name.clone(), dlls));
            }
        }
        Ok(out)
    }

    /// Read every active mod's assemblies and build the report. Blocking; run it on a blocking
    /// thread.
    pub fn run(self: &Arc<Self>) -> Result<PatchReport, String> {
        self.begin()?;
        self.emit();
        let started = std::time::Instant::now();
        let Some(scanner) = self.scanner() else { return self.fail(NO_SCANNER.into()) };
        let mods = match self.collect() {
            Ok(m) => m,
            Err(e) => return self.fail(e),
        };
        // Which mod an assembly belongs to; the scanner echoes the path it was given.
        let mut owner: HashMap<String, String> = HashMap::new();
        let mut names: HashMap<String, String> = HashMap::new();
        let mut queue: Vec<PathBuf> = Vec::new();
        for (uid, name, dlls) in &mods {
            names.insert(uid.clone(), name.clone());
            for d in dlls {
                owner.insert(d.to_string_lossy().to_string(), uid.clone());
                queue.push(d.clone());
            }
        }
        {
            let mut s = self.state.lock().map_err(|_| "the patch job's state is unreadable".to_string())?;
            s.phase = "scanning".into();
            s.total = queue.len();
        }
        self.emit();

        let mut results: Vec<AssemblyPatches> = Vec::with_capacity(queue.len());
        let mut cancelled = false;
        for batch in queue.chunks(BATCH) {
            if self.cancel.load(Ordering::Relaxed) {
                cancelled = true;
                break;
            }
            // The cache lives behind the app lock, and `scan_cached` owns both the lookup and
            // the store, so the lock is held for one batch at a time — long enough to read
            // sixty-odd assemblies, short enough that nothing else waits noticeably.
            let batch_result = match self.app.lock() {
                Ok(app) => harmony::scan_cached(&app.cache, &scanner, batch),
                Err(_) => return self.fail("state lock poisoned".into()),
            };
            match batch_result {
                Ok(mut found) => results.append(&mut found),
                Err(e) => return self.fail(e.to_string()),
            }
            if let Ok(mut s) = self.state.lock() {
                s.done = results.len();
                s.current = batch.last().map(|p| file_name(&p.to_string_lossy()).to_string()).unwrap_or_default();
            }
            self.emit();
        }

        let lookup = |path: &str| -> Option<ModInfo> {
            let uid = owner.get(path)?;
            // aggregate only needs the mod's identity, and a full clone would carry the
            // description of every mod through the fold.
            Some(ModInfo { uid: uid.clone(), name: names.get(uid).cloned().unwrap_or_else(|| uid.clone()), ..ModInfo::default() })
        };
        let report = harmony::aggregate(&results, &lookup);
        let summary = PatchSummary {
            mods: report.per_mod.len(),
            assemblies: report.scanned,
            targets: report.targets.len(),
            contested: report.contested,
            unreadable: report.per_mod.iter().map(|m| m.unreadable.len()).sum(),
            seconds: started.elapsed().as_secs(),
        };
        if let Ok(mut last) = self.last.lock() {
            *last = Some(Run { report: report.clone(), results, owner });
        }
        if let Ok(mut s) = self.state.lock() {
            s.running = false;
            s.phase = "idle".into();
            s.finished_at = now();
            s.cancelled = cancelled;
            s.summary = Some(summary);
        }
        self.emit();
        Ok(report)
    }

    fn report(&self) -> Option<PatchReport> {
        self.last.lock().ok()?.as_ref().map(|r| r.report.clone())
    }

    fn for_mod(&self, uid: &str) -> Option<ModPatchDetail> {
        let guard = self.last.lock().ok()?;
        let run = guard.as_ref()?;
        let summary = run.report.per_mod.iter().find(|m| m.uid == uid)?.clone();
        let mut targets: Vec<PatchTarget> = Vec::new();
        let mut manual: Vec<ManualPatch> = Vec::new();
        for a in &run.results {
            if run.owner.get(&a.path).map(|u| u == uid).unwrap_or(false) {
                targets.extend(a.patches.iter().cloned());
                manual.extend(a.manual_patches.iter().cloned());
            }
        }
        targets.sort_by(|a, b| a.target().cmp(&b.target()).then(a.kind.cmp(&b.kind)));
        let contested: Vec<TargetGroup> = run.report.targets.iter().filter(|t| t.contested && t.patchers.iter().any(|p| p.uid == uid)).cloned().collect();
        Some(ModPatchDetail { summary, targets, manual, contested })
    }
}

/// A path segment joined the way a mod's load folder names it ("" and "." mean the root).
fn join_folder(root: &std::path::Path, folder: &str) -> PathBuf {
    let f = folder.trim_matches(['/', '\\']);
    if f.is_empty() || f == "." {
        return root.to_path_buf();
    }
    let mut p = root.to_path_buf();
    for seg in f.split(['/', '\\']) {
        p.push(seg);
    }
    p
}

/// The child directory of `dir` called `name`, whatever case it is written in. Mods are authored
/// on Windows, where `assemblies` and `Assemblies` are the same folder; on Linux they are not.
fn child_named(dir: &std::path::Path, name: &str) -> Option<PathBuf> {
    let exact = dir.join(name);
    if exact.is_dir() {
        return Some(exact);
    }
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| p.is_dir() && p.file_name().map(|n| n.to_string_lossy().eq_ignore_ascii_case(name)).unwrap_or(false))
}

/// Every `.dll` under an Assemblies folder. Mods keep them flat as a rule, but some nest a
/// dependency in a subfolder and RimWorld loads those too, so this goes a few levels down.
fn collect_dlls(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    if out.len() > 512 {
        return; // a folder this deep is a mistake, not a mod
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let p = entry.path();
        match entry.file_type() {
            Ok(t) if t.is_dir() => collect_dlls(&p, out),
            _ if p.extension().map(|e| e.eq_ignore_ascii_case("dll")).unwrap_or(false) => out.push(p),
            _ => {}
        }
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

// ---------------------------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------------------------

type Pat<'a> = tauri::State<'a, Arc<Patches>>;
type CmdResult<T> = std::result::Result<T, String>;

/// Read every active mod's assemblies in the background; progress arrives as `patch-progress`.
#[tauri::command]
pub async fn patches_start(pat: Pat<'_>) -> CmdResult<()> {
    if pat.snapshot().running {
        return Err("A patch scan is already running".into());
    }
    if pat.scanner().is_none() {
        return Err(NO_SCANNER.into());
    }
    let p = pat.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(e) = p.run() {
            tracing::warn!("harmony scan: {e}");
        }
    });
    Ok(())
}

#[tauri::command]
pub fn patches_status(pat: Pat<'_>) -> PatchJob {
    pat.snapshot()
}

#[tauri::command]
pub fn patches_stop(pat: Pat<'_>) {
    pat.stop();
}

/// The last run's report, or None when nothing has been read yet.
#[tauri::command]
pub fn patches_report(pat: Pat<'_>) -> CmdResult<Option<PatchReport>> {
    if pat.scanner().is_none() {
        return Err(NO_SCANNER.into());
    }
    Ok(pat.report())
}

/// One mod's patches: the counts, its methods, and the contested ones it is part of.
#[tauri::command]
pub fn patches_for_mod(pat: Pat<'_>, uid: String) -> CmdResult<Option<ModPatchDetail>> {
    if pat.scanner().is_none() {
        return Err(NO_SCANNER.into());
    }
    Ok(pat.for_mod(&uid))
}

/// Where the scanner is, or None when it is not installed.
#[tauri::command]
pub fn patches_scanner(pat: Pat<'_>) -> Option<String> {
    pat.scanner().map(|p| p.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_load_folder_joins_the_way_the_game_reads_it() {
        let root = std::path::Path::new("/mods/one");
        assert_eq!(join_folder(root, ""), PathBuf::from("/mods/one"));
        assert_eq!(join_folder(root, "."), PathBuf::from("/mods/one"));
        assert_eq!(join_folder(root, "/1.6/"), PathBuf::from("/mods/one/1.6"));
        assert_eq!(join_folder(root, "Common\\Extra"), PathBuf::from("/mods/one/Common/Extra"));
    }

    #[test]
    fn assemblies_are_found_whatever_case_the_folder_is_in() {
        let dir = std::env::temp_dir().join(format!("circinus-patches-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("1.6/assemblies/lib")).unwrap();
        std::fs::write(dir.join("1.6/assemblies/Mod.dll"), "x").unwrap();
        std::fs::write(dir.join("1.6/assemblies/lib/Dep.dll"), "x").unwrap();
        std::fs::write(dir.join("1.6/assemblies/notes.txt"), "x").unwrap();
        let asm = child_named(&dir.join("1.6"), "Assemblies").expect("the folder is found by name, not by case");
        let mut dlls = Vec::new();
        collect_dlls(&asm, &mut dlls);
        dlls.sort();
        assert_eq!(dlls.len(), 2, "both assemblies, and not the text file: {dlls:?}");
        assert!(dlls.iter().any(|p| p.ends_with("Mod.dll")));
        assert!(dlls.iter().any(|p| p.ends_with("Dep.dll")));
        assert!(child_named(&dir.join("1.6"), "Textures").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
