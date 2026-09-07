//! What each mod's assemblies patch, read from the assemblies themselves.
//!
//! A C# mod changes the game by attaching Harmony patches to its methods. Two mods that prefix
//! the same method, or transpile the same method, are the usual reason a pair "doesn't work
//! together" — and until now that only became visible in a crash log. `crate::clr` reads the
//! metadata and IL of an assembly directly (it never loads or runs one, which would be both
//! unsafe and pointless from a Rust process) and says what it patches. This module caches those
//! readings by file stamp and turns them into the two views worth having: what one mod patches,
//! and who else patches the same thing.

use crate::cache::Cache;
use crate::model::ModInfo;
use crate::Result;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A whole reading as JSON. Nothing in the app produces this any more — `clr` hands back
/// `AssemblyPatches` directly — but `tests/clr_reader.rs` parses the C# oracle's output through
/// it, so the shape is the record of what the two readers have to agree on. Every field
/// defaults, so an oracle file written before a field existed still parses.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScanOutput {
    pub version: u32,
    pub assemblies: Vec<AssemblyPatches>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AssemblyPatches {
    pub path: String,
    pub name: Option<String>,
    /// The assembly's module id: it changes with every build, so it is a good cache key.
    pub mvid: Option<String>,
    /// Set when the file could not be read at all; the lists are then empty.
    pub error: Option<String>,
    /// Ids the mod gives Harmony (`new Harmony("brrainz.harmony")`).
    pub harmony_ids: Vec<String>,
    pub patches: Vec<PatchTarget>,
    pub manual_patches: Vec<ManualPatch>,
    pub startup_classes: Vec<String>,
    pub mod_classes: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PatchTarget {
    pub declaring_type: String,
    pub method: String,
    /// prefix | postfix | transpiler | finalizer | reverse | patch | unpatch.
    pub kind: String,
    pub target_type: Option<String>,
    pub target_method: Option<String>,
    /// normal | getter | setter | constructor | staticConstructor | enumerator | async.
    pub target_kind: String,
    pub argument_types: Option<Vec<String>>,
    pub priority: Option<i32>,
    pub before: Vec<String>,
    pub after: Vec<String>,
    /// attribute | manual.
    pub source: String,
}

impl PatchTarget {
    /// `RimWorld.Pawn::Tick`, the key two mods have to share to be in each other's way.
    pub fn target(&self) -> String {
        let ty = self.target_type.as_deref().unwrap_or("?");
        let method = self.target_method.as_deref().unwrap_or(match self.target_kind.as_str() {
            "constructor" => ".ctor",
            "staticConstructor" => ".cctor",
            _ => "?",
        });
        match self.target_kind.as_str() {
            "getter" => format!("{ty}::get_{method}"),
            "setter" => format!("{ty}::set_{method}"),
            _ => format!("{ty}::{method}"),
        }
    }
    /// Two patches of these kinds fight over the same method; a prefix and a postfix do not.
    fn conflicting_kind(&self) -> bool {
        matches!(self.kind.as_str(), "prefix" | "transpiler")
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ManualPatch {
    pub declaring_type: String,
    pub method: String,
    pub detail: String,
}

// ---------------------------------------------------------------------------------------------
// Reading the assemblies
// ---------------------------------------------------------------------------------------------

/// Cache key for one assembly: its path, size and mtime, which change whenever a mod updates.
/// `PARSER` is part of it so that improving the reader re-reads everything rather than serving
/// yesterday's answers. Kept in the existing key/value table so `cache.rs` stays the one place
/// that owns the schema.
fn stamp(path: &Path) -> String {
    /// Bumped whenever `clr` learns to see something it did not see before.
    const PARSER: u32 = 1;
    let meta = std::fs::metadata(path).ok();
    let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
    let mtime = meta
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let hash = xxhash_rust::xxh3::xxh3_64(path.to_string_lossy().as_bytes());
    format!("harmony:{PARSER}:{hash:016x}:{size}:{mtime}")
}

/// Read some assemblies, taking cached answers where the file has not changed. Reading is a few
/// milliseconds each and rayon spreads a batch across cores, so a two-thousand-mod folder is
/// seconds on a first run and instant afterwards.
pub fn scan_cached(cache: &Cache, dlls: &[PathBuf]) -> Result<Vec<AssemblyPatches>> {
    let mut out: Vec<AssemblyPatches> = Vec::with_capacity(dlls.len());
    let mut todo: Vec<(PathBuf, String)> = Vec::new();
    for p in dlls {
        let key = stamp(p);
        match cache.get::<AssemblyPatches>(&key) {
            Ok(Some(hit)) => out.push(hit),
            _ => todo.push((p.clone(), key)),
        }
    }
    let fresh: Vec<(String, AssemblyPatches)> = todo.par_iter().map(|(path, key)| (key.clone(), crate::clr::scan_assembly(path))).collect();
    for (key, a) in fresh {
        if let Err(e) = cache.set(&key, &a) {
            tracing::warn!("could not cache an assembly reading: {e}");
        }
        out.push(a);
    }
    Ok(out)
}

/// Read one assembly, ignoring the cache. For tests and one-off questions.
pub fn scan_one(path: &Path) -> AssemblyPatches {
    crate::clr::scan_assembly(path)
}

// ---------------------------------------------------------------------------------------------
// What it all means
// ---------------------------------------------------------------------------------------------

/// One mod patching one target.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Patcher {
    pub uid: String,
    pub mod_name: String,
    pub kind: String,
    pub declaring_type: String,
    pub method: String,
    pub priority: Option<i32>,
    pub before: Vec<String>,
    pub after: Vec<String>,
}

/// A game method and everyone who patches it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetGroup {
    /// `RimWorld.Pawn::Tick`.
    pub target: String,
    pub patchers: Vec<Patcher>,
    /// Two mods prefix it, or two mods transpile it: the shape of most "these two don't work
    /// together" reports. Postfixes stack cleanly and do not count.
    pub contested: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModPatches {
    pub uid: String,
    pub name: String,
    pub assemblies: usize,
    pub patches: usize,
    pub prefixes: usize,
    pub postfixes: usize,
    pub transpilers: usize,
    /// Places that patch in a way static reading cannot follow.
    pub manual: usize,
    pub harmony_ids: Vec<String>,
    /// Assemblies the scanner could not read, with the reason.
    pub unreadable: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchReport {
    pub per_mod: Vec<ModPatches>,
    pub targets: Vec<TargetGroup>,
    pub contested: usize,
    pub scanned: usize,
}

/// Fold the per-assembly results into the per-mod and per-target views. `owner` says which mod
/// an assembly path belongs to.
pub fn aggregate(results: &[AssemblyPatches], owner: &dyn Fn(&str) -> Option<ModInfo>) -> PatchReport {
    let mut per_mod: HashMap<String, ModPatches> = HashMap::new();
    let mut targets: HashMap<String, Vec<Patcher>> = HashMap::new();

    for a in results {
        let Some(m) = owner(&a.path) else { continue };
        let entry = per_mod.entry(m.uid.clone()).or_insert_with(|| ModPatches { uid: m.uid.clone(), name: m.name.clone(), ..Default::default() });
        entry.assemblies += 1;
        if let Some(e) = &a.error {
            entry.unreadable.push(format!("{}: {e}", file_name(&a.path)));
            continue;
        }
        for id in &a.harmony_ids {
            if !entry.harmony_ids.contains(id) {
                entry.harmony_ids.push(id.clone());
            }
        }
        entry.manual += a.manual_patches.len();
        for p in &a.patches {
            entry.patches += 1;
            match p.kind.as_str() {
                "prefix" => entry.prefixes += 1,
                "postfix" => entry.postfixes += 1,
                "transpiler" => entry.transpilers += 1,
                _ => {}
            }
            // A target nobody can name is not worth grouping on.
            if p.target_type.is_none() {
                continue;
            }
            targets.entry(p.target()).or_default().push(Patcher {
                uid: m.uid.clone(),
                mod_name: m.name.clone(),
                kind: p.kind.clone(),
                declaring_type: p.declaring_type.clone(),
                method: p.method.clone(),
                priority: p.priority,
                before: p.before.clone(),
                after: p.after.clone(),
            });
        }
    }

    let mut groups: Vec<TargetGroup> = targets
        .into_iter()
        .map(|(target, patchers)| {
            // Contested means two *different mods* fight over it. One mod prefixing its own
            // target twice is its own business.
            let mut fighting: Vec<&str> = patchers.iter().filter(|p| matches!(p.kind.as_str(), "prefix" | "transpiler")).map(|p| p.uid.as_str()).collect();
            fighting.sort_unstable();
            fighting.dedup();
            let contested = fighting.len() > 1;
            TargetGroup { target, patchers, contested }
        })
        .collect();
    // The contested ones first, then the busiest, then by name so the list is stable.
    groups.sort_by(|a, b| b.contested.cmp(&a.contested).then(b.patchers.len().cmp(&a.patchers.len())).then(a.target.cmp(&b.target)));

    let contested = groups.iter().filter(|g| g.contested).count();
    let mut per_mod: Vec<ModPatches> = per_mod.into_values().collect();
    per_mod.sort_by(|a, b| b.patches.cmp(&a.patches).then(a.name.cmp(&b.name)));
    PatchReport { per_mod, targets: groups, contested, scanned: results.len() }
}

/// `PatchTarget::conflicting_kind` in a form the UI can ask about one patch at a time.
pub fn is_conflicting(p: &PatchTarget) -> bool {
    p.conflicting_kind()
}

fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Source;

    /// Captured from `harmony-scan --pretty` over `tools/harmony-scan/testdata/Fixture`, with a
    /// second mod added so the contested case is real.
    const SAMPLE: &str = r#"{
      "version": 1,
      "assemblies": [
        {
          "path": "/mods/one/Assemblies/One.dll", "name": "One", "mvid": "a", "error": null,
          "harmonyIds": ["fixture.one"],
          "patches": [
            {"declaringType":"One.Patches","method":"Before","kind":"prefix","targetType":"RimWorld.Pawn","targetMethod":"Tick","targetKind":"normal","argumentTypes":null,"priority":600,"before":["two.mod"],"after":[],"source":"attribute"},
            {"declaringType":"One.Patches","method":"After","kind":"postfix","targetType":"RimWorld.Pawn","targetMethod":"Tick","targetKind":"normal","argumentTypes":null,"priority":null,"before":[],"after":[],"source":"attribute"},
            {"declaringType":"One.Props","method":"Getter","kind":"postfix","targetType":"RimWorld.Pawn","targetMethod":"HitPoints","targetKind":"getter","argumentTypes":null,"priority":null,"before":[],"after":[],"source":"attribute"},
            {"declaringType":"One.Ctor","method":"Ctor","kind":"prefix","targetType":"RimWorld.Pawn","targetMethod":null,"targetKind":"constructor","argumentTypes":null,"priority":null,"before":[],"after":[],"source":"attribute"}
          ],
          "manualPatches": [{"declaringType":"One.Entry","method":".cctor","detail":"Patch(…) called with a computed target"}],
          "startupClasses": ["One.Entry"], "modClasses": ["One.Entry"]
        },
        {
          "path": "/mods/two/Assemblies/Two.dll", "name": "Two", "mvid": "b", "error": null,
          "harmonyIds": ["fixture.two"],
          "patches": [
            {"declaringType":"Two.Patches","method":"Pre","kind":"prefix","targetType":"RimWorld.Pawn","targetMethod":"Tick","targetKind":"normal","argumentTypes":null,"priority":null,"before":[],"after":["fixture.one"],"source":"attribute"},
            {"declaringType":"Two.Patches","method":"Post","kind":"postfix","targetType":"RimWorld.Pawn","targetMethod":"HitPoints","targetKind":"getter","argumentTypes":null,"priority":null,"before":[],"after":[],"source":"attribute"}
          ],
          "manualPatches": [], "startupClasses": [], "modClasses": []
        },
        {
          "path": "/mods/two/Assemblies/Native.dll", "name": null, "mvid": null,
          "error": "the file is not a managed assembly",
          "harmonyIds": [], "patches": [], "manualPatches": [], "startupClasses": [], "modClasses": []
        }
      ]
    }"#;

    fn parsed() -> ScanOutput {
        serde_json::from_str(SAMPLE).unwrap()
    }

    fn owner(path: &str) -> Option<ModInfo> {
        let (uid, name) = if path.contains("/one/") { ("one", "Mod One") } else { ("two", "Mod Two") };
        Some(ModInfo { uid: uid.into(), name: name.into(), package_id: format!("a.{uid}"), source: Source::Local, ..Default::default() })
    }

    #[test]
    fn reads_the_oracles_json() {
        let out = parsed();
        assert_eq!(out.version, 1);
        assert_eq!(out.assemblies.len(), 3);
        assert_eq!(out.assemblies[0].harmony_ids, ["fixture.one"]);
        assert_eq!(out.assemblies[0].patches[0].priority, Some(600));
        assert_eq!(out.assemblies[2].error.as_deref(), Some("the file is not a managed assembly"));
        // an older oracle file that leaves fields out still parses
        let thin: ScanOutput = serde_json::from_str(r#"{"assemblies":[{"path":"x.dll"}]}"#).unwrap();
        assert_eq!(thin.assemblies[0].path, "x.dll");
        assert!(thin.assemblies[0].patches.is_empty());
    }

    #[test]
    fn target_names_read_like_the_game_writes_them() {
        let out = parsed();
        let p = &out.assemblies[0].patches;
        assert_eq!(p[0].target(), "RimWorld.Pawn::Tick");
        assert_eq!(p[2].target(), "RimWorld.Pawn::get_HitPoints");
        assert_eq!(p[3].target(), "RimWorld.Pawn::.ctor");
    }

    #[test]
    fn two_prefixes_on_one_method_are_contested_two_postfixes_are_not() {
        let r = aggregate(&parsed().assemblies, &owner);
        let tick = r.targets.iter().find(|t| t.target == "RimWorld.Pawn::Tick").unwrap();
        assert!(tick.contested, "One and Two both prefix Pawn.Tick");
        assert_eq!(tick.patchers.len(), 3);
        let getter = r.targets.iter().find(|t| t.target == "RimWorld.Pawn::get_HitPoints").unwrap();
        assert!(!getter.contested, "two postfixes stack");
        assert_eq!(getter.patchers.len(), 2);
        // contested groups come first
        assert_eq!(r.targets[0].target, "RimWorld.Pawn::Tick");
        assert_eq!(r.contested, 1);
    }

    #[test]
    fn per_mod_counts_and_unreadable_files() {
        let r = aggregate(&parsed().assemblies, &owner);
        let one = r.per_mod.iter().find(|m| m.uid == "one").unwrap();
        assert_eq!((one.patches, one.prefixes, one.postfixes, one.manual), (4, 2, 2, 1));
        assert_eq!(one.harmony_ids, ["fixture.one"]);
        let two = r.per_mod.iter().find(|m| m.uid == "two").unwrap();
        assert_eq!(two.assemblies, 2);
        assert_eq!(two.unreadable.len(), 1);
        assert!(two.unreadable[0].starts_with("Native.dll: "));
        assert_eq!(r.scanned, 3);
    }

    #[test]
    fn an_assembly_with_no_owner_is_skipped_rather_than_panicking() {
        let r = aggregate(&parsed().assemblies, &|_| None);
        assert!(r.per_mod.is_empty());
        assert!(r.targets.is_empty());
    }

    #[test]
    fn a_file_that_is_not_an_assembly_is_an_entry_with_a_reason_not_a_failure() {
        let a = scan_one(Path::new("/no/such/file.dll"));
        assert!(a.error.is_some(), "a missing file is reported, not panicked over");
        assert!(a.patches.is_empty());
    }

    #[test]
    fn a_reading_is_cached_by_the_file_and_re_read_when_it_changes() {
        let cache = Cache::in_memory().expect("cache");
        let dir = std::env::temp_dir().join(format!("circinus-clr-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let dll = dir.join("Nope.dll");
        std::fs::write(&dll, b"not an assembly").expect("write");
        let first = scan_cached(&cache, &[dll.clone()]).expect("scan");
        assert_eq!(first.len(), 1);
        assert!(first[0].error.is_some(), "a file that is not an assembly says so rather than failing");
        // Same file, same answer, and now from the cache.
        assert_eq!(scan_cached(&cache, &[dll.clone()]).expect("scan").len(), 1);
        // A changed file is a different stamp, so it is read again rather than served stale.
        std::fs::write(&dll, b"still not an assembly, but longer").expect("write");
        assert_ne!(stamp(&dll), {
            std::fs::write(&dll, b"not an assembly").expect("write");
            stamp(&dll)
        });
        let _ = std::fs::remove_dir_all(&dir);
    }
}
