//! Player.log analysis: find the file, parse it, and tie what it says to the mods that are
//! installed — by the `[Source: …]` name, by Workshop id or folder in a path, by the Harmony
//! patch owner id, or by the assembly a stack frame's namespace lives in.

use crate::state::App;
use circinus_core::playerlog::{self, LogReport};
use circinus_core::scan::ModFiles;
use circinus_core::ModInfo;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModRef {
    pub uid: Option<String>,
    pub name: String,
    pub active: bool,
    pub missing_parents: usize,
    pub xml_errors: usize,
    pub dds_failures: usize,
    /// Exception groups whose innermost mod frame or patch owner points here.
    pub exceptions: usize,
    /// Sum of exception counts.
    pub exception_hits: usize,
    pub crash_culprit: bool,
    pub load_failure_patch: bool,
    pub duplicate_folders: usize,
    /// Sits above Core or a DLC in the current list. Its defs cannot inherit official parents
    /// there — the usual reason for "Could not find parent node" on Core names.
    pub above_official: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogAnalysis {
    pub path: String,
    pub bytes: u64,
    pub modified: i64,
    pub report: LogReport,
    /// Mods the log implicates, worst first.
    pub mods: Vec<ModRef>,
    /// Frame namespace / patch owner / source name → resolved mod name, for the UI.
    pub resolved: HashMap<String, String>,
}

/// `<config>/../Player.log` and `Player-prev.log`.
pub fn default_paths(app: &App) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(cfg) = &app.locations.config_dir {
        if let Some(parent) = cfg.parent() {
            out.push(parent.join("Player.log"));
            out.push(parent.join("Player-prev.log"));
        }
    }
    out
}

/// Read the newest Player.log for one thing only: how long the last load took.
///
/// Runs at launch, so it is deliberately cheap -- `playerlog::load_run_of` is a linear pass with
/// two rules, not the full analysis, which gathers exceptions and stacks nobody asked for here.
/// Returns the figure and the log's own modified time, so the screen can say how old it is.
///
/// Takes paths rather than the `App` on purpose: a log is tens of megabytes and this must not run
/// with the state locked, which is what taking `&App` would have meant.
///
/// Every failure is `None`. A log that is missing, locked by a running game or unreadable is not
/// worth a word: the estimate is what shows instead, which is what showed before this existed.
pub fn last_load(paths: &[PathBuf]) -> Option<(circinus_core::playerlog::LoadRun, i64)> {
    let mut best: Option<(circinus_core::playerlog::LoadRun, i64)> = None;
    for path in paths {
        let Ok(md) = std::fs::metadata(path) else { continue };
        let Ok(text) = std::fs::read_to_string(path) else { continue };
        let Some(run) = circinus_core::playerlog::load_run_of(&text) else { continue };
        let at = md.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
        // Player.log and Player-prev.log are this run and the one before; take whichever was
        // written last rather than whichever the path list happened to name first.
        if best.as_ref().is_none_or(|(_, b)| at > *b) {
            best = Some((run, at));
        }
    }
    best
}

/// `<config>/../StartupImpactData.xml`, where the Loading Progress mod writes its measurement.
///
/// The game's save-data folder is the parent of its Config folder -- the same folder Player.log
/// sits in -- so this needs no path of its own.
pub fn impact_path(app: &App) -> Option<PathBuf> {
    app.locations.config_dir.as_ref().and_then(|c| c.parent()).map(|p| p.join("StartupImpactData.xml"))
}

/// Read what Loading Progress measured, and when it wrote it.
///
/// Takes the path rather than the `App`, for the same reason `last_load` does: this is file I/O
/// and it must not hold the state lock. Cheaper than the log by a wide margin -- the file is a
/// few hundred kilobytes against tens of megabytes -- but the rule is about the lock, not size.
///
/// `None` covers every ordinary case: the mod is not installed, or it is and its two settings
/// are off, or the game is mid-write. None of those is worth a word; the model shows instead.
pub fn startup_impact(path: &Path) -> Option<(circinus_core::startupimpact::StartupImpact, i64)> {
    let md = std::fs::metadata(path).ok()?;
    let text = std::fs::read_to_string(path).ok()?;
    let parsed = circinus_core::startupimpact::parse(&text)?;
    let at = md.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
    Some((parsed, at))
}

/// Re-read both records of the last start-up and put them in the state.
///
/// The log and the measurement answer the same question from two sides, they live in the same
/// folder, and neither may be read with the state locked -- so they are taken together, and
/// paths come out under the lock before any file is touched.
///
/// This is not only a launch-time job. It used to be, and the result was that starting the game
/// from Circinus and coming back showed the *previous* run's figure until the app was restarted
/// -- the one moment a player has a reason to look at that card is the one moment it was stale.
/// Call it again when the game exits.
///
/// A new measurement is also the moment a run becomes shareable, so this queues one when the
/// player has said yes. Queuing is not sending: the spool is drained separately, and a build
/// running against a site that does not serve the endpoint yet simply keeps its runs.
///
/// Returns whether anything changed, so a caller can skip the redraw when nothing has.
pub fn refresh_last_run(shared: &crate::commands::Shared) -> bool {
    let (paths, impact, had_run_at, had_impact_at) = match shared.lock() {
        Ok(a) => (default_paths(&a), impact_path(&a), a.load_run_at, a.startup_impact_at),
        Err(_) => return false,
    };

    // Stat before read. This runs whenever the window comes back to the front, and a Player.log
    // is tens of megabytes: reading it to discover it has not changed would be the most
    // expensive way in the app to learn nothing. A file is worth opening only when it is newer
    // than the figure already on screen.
    let run = paths
        .iter()
        .any(|p| mtime(p).is_some_and(|t| t > had_run_at))
        .then(|| last_load(&paths))
        .flatten();
    let measured = impact
        .as_deref()
        .filter(|p| mtime(p).is_some_and(|t| t > had_impact_at))
        .and_then(startup_impact);

    if run.is_none() && measured.is_none() {
        return false;
    }
    let Ok(mut a) = shared.lock() else { return false };
    let mut changed = false;
    if let Some((r, at)) = run {
        if a.load_run.as_ref() != Some(&r) || a.load_run_at != at {
            a.load_run = Some(r);
            a.load_run_at = at;
            changed = true;
        }
    }
    if let Some((m, at)) = measured {
        if a.startup_impact.as_ref() != Some(&m) || a.startup_impact_at != at {
            queue_from(&a, &m, at);
            a.startup_impact = Some(m);
            a.startup_impact_at = at;
            changed = true;
        }
    }
    changed
}

/// Turn a fresh measurement into a queued run, if the player has said it may be shared.
///
/// Called with the state locked, which is fine: building the report is arithmetic over data
/// already in memory, and the one write is a small file. Nothing here talks to the network.
pub fn queue_from(app: &crate::state::App, impact: &circinus_core::startupimpact::StartupImpact, at: i64) {
    if !app.user.sharing.may_send() {
        return;
    }
    // Only the active list, and only the three things the wire may carry about each mod.
    let facts: Vec<circinus_core::telemetry::ModFacts> = app
        .active
        .iter()
        .filter_map(|uid| app.mods.iter().find(|m| &m.uid == uid))
        .map(|m| circinus_core::telemetry::ModFacts {
            package_id: m.package_id.to_ascii_lowercase().trim_end_matches("_steam").to_string(),
            workshop_id: m.published_file_id,
            version: m.mod_version.clone(),
        })
        .collect();

    // The run id is the measurement's own timestamp plus a little randomness: two launches in
    // the same second are unlikely, and a collision would only cost one duplicate rejection.
    let run_id = format!("{at:x}{}", &crate::sharing::new_install_id()[..8]);
    let report = circinus_core::telemetry::build(
        impact,
        &facts,
        crate::sharing::machine(),
        &run_id,
        &app.user.sharing.install_id,
        env!("CARGO_PKG_VERSION"),
        &app.game_version.major_minor,
    );
    crate::outbox::enqueue(&app.data_dir, &report);
}

/// A file's modified time in unix seconds, or `None` if it cannot be asked.
fn mtime(path: &Path) -> Option<i64> {
    std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs() as i64)
}

struct Index<'a> {
    mods: &'a [ModInfo],
    files: &'a HashMap<String, ModFiles>,
    by_name: HashMap<String, usize>,
    by_pfid: HashMap<u64, usize>,
    by_folder: HashMap<String, usize>,
    by_pkg: HashMap<String, usize>,
}

impl<'a> Index<'a> {
    fn new(mods: &'a [ModInfo], files: &'a HashMap<String, ModFiles>) -> Index<'a> {
        let mut by_name = HashMap::new();
        let mut by_pfid = HashMap::new();
        let mut by_folder = HashMap::new();
        let mut by_pkg = HashMap::new();
        for (i, m) in mods.iter().enumerate() {
            by_name.entry(m.name.to_lowercase()).or_insert(i);
            if let Some(id) = m.published_file_id {
                by_pfid.entry(id).or_insert(i);
            }
            if let Some(f) = m.path.file_name() {
                by_folder.entry(f.to_string_lossy().to_lowercase()).or_insert(i);
            }
            if !m.package_id.is_empty() {
                by_pkg.entry(m.package_id.clone()).or_insert(i);
            }
        }
        Index { mods, files, by_name, by_pfid, by_folder, by_pkg }
    }

    fn by_source(&self, name: &str) -> Option<usize> {
        self.by_name.get(&name.to_lowercase()).copied()
    }

    fn by_path(&self, path: &str) -> Option<usize> {
        let norm = path.replace('\\', "/");
        let lower = norm.to_lowercase();
        if let Some(i) = lower.find("/workshop/content/294100/") {
            let id: u64 = lower[i + 25..].split('/').next()?.parse().ok()?;
            return self.by_pfid.get(&id).copied();
        }
        if let Some(i) = lower.find("/mods/") {
            let folder = lower[i + 6..].split('/').next()?;
            return self.by_folder.get(folder).copied();
        }
        None
    }

    /// "RedMattis.GravShipSize" (a Harmony id) or "GravshipSize.HarmonyPatches.X" (a frame):
    /// an exact packageId, else an assembly whose file name matches the first or last segment.
    fn by_code(&self, ident: &str) -> Option<usize> {
        let lower = ident.to_lowercase();
        if let Some(i) = self.by_pkg.get(&lower) {
            return Some(*i);
        }
        let segs: Vec<&str> = lower.split(['.', '+', ':']).filter(|s| !s.is_empty()).collect();
        let candidates: Vec<&str> = [segs.first().copied(), segs.get(1).copied(), segs.last().copied()].into_iter().flatten().collect();
        // Harmony ids are often the packageId with different case, or "author.mod".
        for cand in &candidates {
            if cand.len() < 4 {
                continue;
            }
            for (uid, f) in self.files {
                if f.assemblies.iter().any(|a| Path::new(a).file_stem().map(|s| s.to_string_lossy().to_lowercase() == *cand).unwrap_or(false)) {
                    return self.mods.iter().position(|m| &m.uid == uid);
                }
            }
        }
        // Whole id contained in a packageId, or packageId's tail equal to a segment.
        for (pkg, i) in &self.by_pkg {
            if pkg == &lower || (lower.len() > 6 && pkg.contains(&lower)) {
                return Some(*i);
            }
        }
        for cand in &candidates {
            if cand.len() < 5 {
                continue;
            }
            for (pkg, i) in &self.by_pkg {
                if pkg.rsplit('.').next() == Some(cand) {
                    return Some(*i);
                }
            }
        }
        None
    }
}

/// Which installed mods the report points at. `active` is the current list in load order.
pub fn link(report: &LogReport, mods: &[ModInfo], files: &HashMap<String, ModFiles>, active: &[String]) -> (Vec<ModRef>, HashMap<String, String>) {
    let ix = Index::new(mods, files);
    let position: HashMap<&str, usize> = active.iter().enumerate().map(|(i, u)| (u.as_str(), i)).collect();
    let last_official = active.iter().rposition(|u| mods.iter().any(|m| &m.uid == u && m.is_official()));
    let active: std::collections::HashSet<&str> = active.iter().map(|s| s.as_str()).collect();
    let mut refs: HashMap<String, ModRef> = HashMap::new(); // key: uid or "?name"
    let mut resolved: HashMap<String, String> = HashMap::new();
    let entry = |refs: &mut HashMap<String, ModRef>, ix: Option<usize>, fallback: &str| -> String {
        let (key, name, uid) = match ix {
            Some(i) => (mods[i].uid.clone(), mods[i].name.clone(), Some(mods[i].uid.clone())),
            None => (format!("?{fallback}"), fallback.to_string(), None),
        };
        refs.entry(key.clone()).or_insert_with(|| ModRef { active: uid.as_deref().map(|u| active.contains(u)).unwrap_or(false), uid, name, ..ModRef::default() });
        key
    };
    for p in &report.missing_parents {
        let src = p.source_mod.as_deref().or_else(|| p.file.as_deref()).unwrap_or("unknown source");
        let found = p.source_mod.as_deref().and_then(|s| ix.by_source(s)).or_else(|| p.file.as_deref().and_then(|f| ix.by_path(f)));
        let k = entry(&mut refs, found, src);
        refs.get_mut(&k).unwrap().missing_parents += 1;
    }
    for p in &report.xml_errors {
        let src = p.source_mod.as_deref().or_else(|| p.file.as_deref()).unwrap_or("unknown source");
        let found = p.source_mod.as_deref().and_then(|s| ix.by_source(s)).or_else(|| p.file.as_deref().and_then(|f| ix.by_path(f)));
        let k = entry(&mut refs, found, src);
        refs.get_mut(&k).unwrap().xml_errors += 1;
    }
    for d in &report.dds_failures {
        let found = ix.by_path(&d.path);
        let label = d.workshop_id.map(|id| id.to_string()).or_else(|| d.mod_folder.clone()).unwrap_or_else(|| d.path.clone());
        let k = entry(&mut refs, found, &label);
        refs.get_mut(&k).unwrap().dds_failures += 1;
    }
    for e in &report.exceptions {
        let mut blamed: Option<String> = None;
        if let Some(f) = &e.mod_frame {
            let found = ix.by_code(f);
            let root = f.split(['.', '+', ':']).next().unwrap_or(f).to_string();
            if let Some(i) = found {
                resolved.insert(root.clone(), mods[i].name.clone());
            }
            let k = entry(&mut refs, found, &root);
            blamed = Some(k);
        }
        for owner in &e.patch_owners {
            if let Some(i) = ix.by_code(owner) {
                resolved.insert(owner.clone(), mods[i].name.clone());
                if blamed.is_none() {
                    let k = entry(&mut refs, Some(i), owner);
                    blamed = Some(k);
                }
            }
        }
        if let Some(k) = blamed {
            let r = refs.get_mut(&k).unwrap();
            r.exceptions += 1;
            r.exception_hits += e.count;
        }
    }
    if let Some(lf) = &report.load_failure {
        for owner in &lf.patch_owners {
            if let Some(i) = ix.by_code(owner) {
                resolved.insert(owner.clone(), mods[i].name.clone());
                let k = entry(&mut refs, Some(i), owner);
                refs.get_mut(&k).unwrap().load_failure_patch = true;
            }
        }
        if let Some(f) = &lf.mod_frame {
            if let Some(i) = ix.by_code(f) {
                let root = f.split(['.', '+', ':']).next().unwrap_or(f).to_string();
                resolved.insert(root, mods[i].name.clone());
                let k = entry(&mut refs, Some(i), f);
                refs.get_mut(&k).unwrap().load_failure_patch = true;
            }
        }
    }
    if let Some(c) = &report.crash {
        if let Some(f) = &c.culprit_frame {
            let found = ix.by_code(f);
            let root = f.split(['.', '+', ':']).next().unwrap_or(f).to_string();
            if let Some(i) = found {
                resolved.insert(root.clone(), mods[i].name.clone());
            }
            let k = entry(&mut refs, found, &root);
            refs.get_mut(&k).unwrap().crash_culprit = true;
        }
    }
    for d in &report.duplicates {
        let found = ix.by_pkg.get(&d.package_id.to_lowercase()).copied();
        let k = entry(&mut refs, found, &d.package_id);
        refs.get_mut(&k).unwrap().duplicate_folders += d.folders.len();
    }
    let mut out: Vec<ModRef> = refs.into_values().collect();
    if let Some(last) = last_official {
        for r in &mut out {
            r.above_official = r.uid.as_deref().and_then(|u| position.get(u)).map(|p| *p < last).unwrap_or(false) && !r.uid.as_deref().and_then(|u| mods.iter().find(|m| m.uid == u)).map(|m| m.is_official()).unwrap_or(false);
        }
    }
    let score = |r: &ModRef| (r.above_official as usize) * 800 + (r.crash_culprit as usize) * 1000 + (r.load_failure_patch as usize) * 500 + r.missing_parents * 20 + r.xml_errors * 10 + r.exception_hits.min(50) + r.exceptions * 5 + r.dds_failures * 3 + r.duplicate_folders;
    out.sort_by(|a, b| score(b).cmp(&score(a)).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    (out, resolved)
}

pub fn analyze(app: &App, path: &Path) -> Result<LogAnalysis, String> {
    let md = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let report = playerlog::parse(&text);
    let (mods, resolved) = link(&report, &app.mods, &app.files, &app.active);
    Ok(LogAnalysis {
        path: path.display().to_string(),
        bytes: md.len(),
        modified: md.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0),
        report,
        mods,
        resolved,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(uid: &str, pkg: &str, name: &str, pfid: Option<u64>) -> ModInfo {
        ModInfo { uid: uid.into(), path: PathBuf::from(uid), package_id: pkg.into(), name: name.into(), published_file_id: pfid, ..Default::default() }
    }

    #[test]
    fn links_by_every_route() {
        let mods = vec![
            m("C:/ws/2988801276", "ancot.ancotlibrary", "Ancot Library", Some(2988801276)),
            m("C:/ws/1", "redmattis.gravshipsize", "Gravship Size", Some(1)),
            m("C:/RimWorld/Mods/WorkRoles", "someone.workroles", "Work Roles", None),
            m("C:/ws/2842502659", "x.paths", "Some Paths", Some(2842502659)),
        ];
        let mut files = HashMap::new();
        files.insert("C:/RimWorld/Mods/WorkRoles".to_string(), ModFiles { assemblies: vec!["assemblies/WorkRoles.dll".into()], ..Default::default() });
        let text = "XML error: Could not find parent node named \"Stun\" for node \"DamageDef\". Full node: <DamageDef><defName>A</defName></DamageDef>\n\nPossible Matches:\n[Source: Ancot Library]\n[File: C:\\ws\\2988801276\\1.6\\Defs\\x.xml]\nException from asynchronous event: System.NullReferenceException: x\n[Ref 1]\n  at GravshipSize.GravshipSizeSettings.ApplySettingsNow () [0x0003c] in <y>:0\n    - PREFIX RedMattis.GravShipSize: Void GravshipSize.HarmonyPatches:GenerateImpliedDefs_Prefix(Boolean hotReload)\nException loading UnityEngine.Texture2D from file.\nabsFilePath: C:\\steamapps\\workshop\\content\\294100\\2842502659\\Textures\\UI\\A.dds\nException: UnityEngine.UnityException: bad\nGraphics device is null.\nCrash!!!\n0x1 (Mono JIT Code) WorkRoles.UI.WorkRolesTex:MakeCircle (int)\n========== END OF STACKTRACE ===========\n";
        let report = playerlog::parse(text);
        let (refs, resolved) = link(&report, &mods, &files, &["C:/ws/1".to_string()]);
        let by = |n: &str| refs.iter().find(|r| r.name == n).unwrap();
        assert_eq!(by("Ancot Library").missing_parents, 1);
        assert!(!by("Ancot Library").above_official, "no official mod in the list");
        assert_eq!(by("Gravship Size").exceptions, 1);
        assert!(by("Gravship Size").active);
        assert_eq!(by("Some Paths").dds_failures, 1);
        assert!(by("Work Roles").crash_culprit);
        assert_eq!(refs[0].name, "Work Roles", "crash culprit sorts first");
        assert_eq!(resolved.get("GravshipSize").map(|s| s.as_str()), Some("Gravship Size"));
        assert_eq!(resolved.get("WorkRoles").map(|s| s.as_str()), Some("Work Roles"));
        // With Core in the list below Ancot Library, the missing Core parents are explained.
        let mut mods2 = mods.clone();
        mods2.push(ModInfo { uid: "C:/RimWorld/Data/Core".into(), path: PathBuf::from("C:/RimWorld/Data/Core"), package_id: "ludeon.rimworld".into(), name: "RimWorld".into(), source: circinus_core::Source::Ludeon, ..Default::default() });
        let order = ["C:/ws/2988801276".to_string(), "C:/RimWorld/Data/Core".to_string(), "C:/ws/1".to_string()];
        let (refs2, _) = link(&report, &mods2, &files, &order);
        let by2 = |n: &str| refs2.iter().find(|r| r.name == n).unwrap();
        assert!(by2("Ancot Library").above_official);
        assert!(!by2("Gravship Size").above_official);
        assert_eq!(refs2[0].name, "Work Roles", "a crash culprit still outranks a misplaced mod");
        assert_eq!(refs2[1].name, "Ancot Library");
    }
}
