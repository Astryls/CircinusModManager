//! What changed since Circinus last looked.
//!
//! After every scan a fingerprint of each installed mod is stored (the *baseline*). The next
//! launch compares the fresh scan against it and reports mods that appeared, disappeared or
//! were updated — a Workshop update Steam applied while the game was launching, a mod
//! replaced by hand, a version bump — plus edits to the active list made outside Circinus.
//!
//! Steam is the awkward case: an update can replace files deep inside a mod without touching
//! the folder's own mtime or About.xml, so for Workshop items the `timeupdated` Steam records
//! in `appworkshop_294100.acf` is the signal that counts.

use crate::model::{ModInfo, Source};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// The identity and freshness of one installed mod.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fingerprint {
    pub name: String,
    pub package_id: String,
    #[serde(default)]
    pub mod_version: Option<String>,
    #[serde(default)]
    pub published_file_id: Option<u64>,
    pub source: Source,
    /// Newest mtime known for the folder (`ModInfo::modified`).
    pub modified: u64,
    /// Steam's `timeupdated` for Workshop items (0 when unknown).
    #[serde(default)]
    pub workshop_updated: u64,
}

impl Fingerprint {
    pub fn of(m: &ModInfo, workshop_updated: u64) -> Fingerprint {
        Fingerprint {
            name: m.name.clone(),
            package_id: m.package_id.clone(),
            mod_version: m.mod_version.clone(),
            published_file_id: m.published_file_id,
            source: m.source,
            modified: m.modified,
            workshop_updated,
        }
    }
}

/// Everything remembered from the last scan.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Baseline {
    /// Unix seconds when this baseline was taken.
    pub taken_at: i64,
    /// uid → fingerprint.
    pub mods: HashMap<String, Fingerprint>,
    /// The active list as ModsConfig.xml had it (package ids, lowercase, in order).
    pub active: Vec<String>,
}

impl Baseline {
    pub fn take(mods: &[ModInfo], workshop_updated: &HashMap<u64, u64>, active: &[String], now: i64) -> Baseline {
        let mut map = HashMap::with_capacity(mods.len());
        for m in mods {
            let ws = m.published_file_id.and_then(|id| workshop_updated.get(&id).copied()).unwrap_or(0);
            map.insert(m.uid.clone(), Fingerprint::of(m, ws));
        }
        Baseline { taken_at: now, mods: map, active: active.to_vec() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangeKind {
    Added,
    Removed,
    Updated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChangeReason {
    /// Steam installed a newer Workshop version.
    WorkshopUpdate,
    /// `modVersion` in About.xml changed.
    VersionChange,
    /// Files in the folder are newer than they were.
    FilesChanged,
    /// The name in About.xml changed.
    Renamed,
    /// The mod now comes from a different place (Workshop → local copy, for example).
    SourceChanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModChange {
    pub kind: ChangeKind,
    pub uid: String,
    pub name: String,
    pub package_id: String,
    #[serde(default)]
    pub published_file_id: Option<u64>,
    pub source: Source,
    /// In the active list (for removed mods: was in the list the baseline recorded).
    pub active: bool,
    pub reasons: Vec<ChangeReason>,
    #[serde(default)]
    pub old_version: Option<String>,
    #[serde(default)]
    pub new_version: Option<String>,
    /// Unix seconds of the change when known: the Workshop update time, else the folder's mtime.
    pub when: u64,
}

/// Edits to the active list made outside Circinus (in the game, by another manager, by hand).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListChange {
    /// Package ids now in the list that were not.
    pub added: Vec<String>,
    /// Package ids that were in the list and are gone.
    pub removed: Vec<String>,
    /// Same members, different order.
    pub reordered: bool,
}

impl ListChange {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && !self.reordered
    }
}

/// Compare the previous baseline with what is installed now.
///
/// `active_uids` is the current active list; `file_active` is the list ModsConfig.xml holds
/// right now (package ids), compared with what the baseline recorded.
pub fn diff(prev: &Baseline, mods: &[ModInfo], workshop_updated: &HashMap<u64, u64>, active_uids: &HashSet<String>, file_active: Option<&[String]>) -> (Vec<ModChange>, Option<ListChange>) {
    let mut out = Vec::new();
    let prev_active: HashSet<&str> = prev.active.iter().map(|s| s.as_str()).collect();
    let mut seen: HashSet<&str> = HashSet::with_capacity(mods.len());
    for m in mods {
        seen.insert(m.uid.as_str());
        if m.invalid.is_some() && m.name.is_empty() {
            continue;
        }
        let ws = m.published_file_id.and_then(|id| workshop_updated.get(&id).copied()).unwrap_or(0);
        let now = Fingerprint::of(m, ws);
        let active = active_uids.contains(&m.uid);
        match prev.mods.get(&m.uid) {
            None => {
                // New folder. A mod that merely moved (same package id, other folder) still counts
                // as added here and removed below; the UI pairs them up by package id.
                out.push(ModChange {
                    kind: ChangeKind::Added,
                    uid: m.uid.clone(),
                    name: m.name.clone(),
                    package_id: m.package_id.clone(),
                    published_file_id: m.published_file_id,
                    source: m.source,
                    active,
                    reasons: Vec::new(),
                    old_version: None,
                    new_version: m.mod_version.clone(),
                    when: if ws > 0 { ws } else { m.modified },
                });
            }
            Some(old) if *old != now => {
                let mut reasons = Vec::new();
                if now.workshop_updated != old.workshop_updated && now.workshop_updated > 0 {
                    reasons.push(ChangeReason::WorkshopUpdate);
                }
                if now.mod_version != old.mod_version {
                    reasons.push(ChangeReason::VersionChange);
                }
                if now.name != old.name {
                    reasons.push(ChangeReason::Renamed);
                }
                if now.source != old.source {
                    reasons.push(ChangeReason::SourceChanged);
                }
                if reasons.is_empty() || (now.modified != old.modified && !reasons.contains(&ChangeReason::WorkshopUpdate)) {
                    reasons.push(ChangeReason::FilesChanged);
                }
                let when = if reasons.contains(&ChangeReason::WorkshopUpdate) { now.workshop_updated } else { now.modified };
                out.push(ModChange {
                    kind: ChangeKind::Updated,
                    uid: m.uid.clone(),
                    name: m.name.clone(),
                    package_id: m.package_id.clone(),
                    published_file_id: m.published_file_id,
                    source: m.source,
                    active,
                    reasons,
                    old_version: old.mod_version.clone(),
                    new_version: now.mod_version.clone(),
                    when,
                });
            }
            Some(_) => {}
        }
    }
    for (uid, old) in &prev.mods {
        if seen.contains(uid.as_str()) {
            continue;
        }
        out.push(ModChange {
            kind: ChangeKind::Removed,
            uid: uid.clone(),
            name: old.name.clone(),
            package_id: old.package_id.clone(),
            published_file_id: old.published_file_id,
            source: old.source,
            active: prev_active.contains(old.package_id.as_str()) || prev_active.contains(format!("{}_steam", old.package_id).as_str()),
            reasons: Vec::new(),
            old_version: old.mod_version.clone(),
            new_version: None,
            when: 0,
        });
    }
    // Newest first, then by name so the list is stable.
    out.sort_by(|a, b| b.when.cmp(&a.when).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));

    let list = file_active.map(|cur| {
        let before: HashSet<&str> = prev.active.iter().map(|s| s.as_str()).collect();
        let after: HashSet<&str> = cur.iter().map(|s| s.as_str()).collect();
        let added: Vec<String> = cur.iter().filter(|p| !before.contains(p.as_str())).cloned().collect();
        let removed: Vec<String> = prev.active.iter().filter(|p| !after.contains(p.as_str())).cloned().collect();
        let reordered = added.is_empty() && removed.is_empty() && prev.active != cur;
        ListChange { added, removed, reordered }
    });
    (out, list.filter(|l| !l.is_empty()))
}

/// One-line summary for a toast or a banner: "3 updated, 1 new, 2 removed".
pub fn summary(changes: &[ModChange]) -> String {
    let n = |k: ChangeKind| changes.iter().filter(|c| c.kind == k).count();
    let (u, a, r) = (n(ChangeKind::Updated), n(ChangeKind::Added), n(ChangeKind::Removed));
    let mut parts = Vec::new();
    if u > 0 {
        parts.push(format!("{u} updated"));
    }
    if a > 0 {
        parts.push(format!("{a} new"));
    }
    if r > 0 {
        parts.push(format!("{r} removed"));
    }
    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn m(uid: &str, pkg: &str, name: &str, version: Option<&str>, pfid: Option<u64>, source: Source, modified: u64) -> ModInfo {
        ModInfo {
            uid: uid.into(),
            path: PathBuf::from(uid),
            package_id: pkg.into(),
            name: name.into(),
            mod_version: version.map(|v| v.to_string()),
            published_file_id: pfid,
            source,
            modified,
            ..Default::default()
        }
    }

    #[test]
    fn detects_added_removed_updated() {
        let before = vec![
            m("ws/1", "a.one", "One", Some("1.0"), Some(1), Source::Workshop, 100),
            m("ws/2", "b.two", "Two", None, Some(2), Source::Workshop, 100),
            m("local/three", "c.three", "Three", Some("2.0"), None, Source::Local, 100),
            m("local/gone", "d.gone", "Gone", None, None, Source::Local, 100),
        ];
        let mut ws: HashMap<u64, u64> = HashMap::from([(1, 1000), (2, 1000)]);
        let base = Baseline::take(&before, &ws, &["a.one".into(), "d.gone".into()], 5);

        let after = vec![
            m("ws/1", "a.one", "One", Some("1.1"), Some(1), Source::Workshop, 100), // workshop update + version bump
            m("ws/2", "b.two", "Two", None, Some(2), Source::Workshop, 100),        // unchanged
            m("local/three", "c.three", "Three", Some("2.0"), None, Source::Local, 200), // files changed
            m("local/new", "e.new", "New", None, None, Source::Local, 300),
        ];
        ws.insert(1, 2000);
        let active: HashSet<String> = HashSet::from(["ws/1".to_string(), "local/new".to_string()]);
        let file_active = vec!["a.one".to_string(), "e.new".to_string()];
        let (changes, list) = diff(&base, &after, &ws, &active, Some(&file_active));

        let by_uid = |u: &str| changes.iter().find(|c| c.uid == u).unwrap();
        let one = by_uid("ws/1");
        assert_eq!(one.kind, ChangeKind::Updated);
        assert_eq!(one.reasons, vec![ChangeReason::WorkshopUpdate, ChangeReason::VersionChange]);
        assert_eq!(one.old_version.as_deref(), Some("1.0"));
        assert_eq!(one.new_version.as_deref(), Some("1.1"));
        assert_eq!(one.when, 2000);
        assert!(one.active);
        assert!(changes.iter().all(|c| c.uid != "ws/2"));
        let three = by_uid("local/three");
        assert_eq!(three.reasons, vec![ChangeReason::FilesChanged]);
        assert_eq!(three.when, 200);
        assert!(!three.active);
        assert_eq!(by_uid("local/new").kind, ChangeKind::Added);
        let gone = by_uid("local/gone");
        assert_eq!(gone.kind, ChangeKind::Removed);
        assert!(gone.active, "was in the recorded active list");
        assert_eq!(changes[0].uid, "ws/1", "newest first (the Workshop update at 2000)");

        let list = list.unwrap();
        assert_eq!(list.added, vec!["e.new"]);
        assert_eq!(list.removed, vec!["d.gone"]);
        assert!(!list.reordered);
        assert_eq!(summary(&changes), "2 updated, 1 new, 1 removed");
    }

    #[test]
    fn reorder_only_is_reported_as_such() {
        let mods = vec![m("a", "a", "A", None, None, Source::Local, 1), m("b", "b", "B", None, None, Source::Local, 1)];
        let base = Baseline::take(&mods, &HashMap::new(), &["a".into(), "b".into()], 0);
        let (changes, list) = diff(&base, &mods, &HashMap::new(), &HashSet::new(), Some(&["b".to_string(), "a".to_string()]));
        assert!(changes.is_empty());
        assert_eq!(list, Some(ListChange { added: vec![], removed: vec![], reordered: true }));
        let (_, same) = diff(&base, &mods, &HashMap::new(), &HashSet::new(), Some(&["a".to_string(), "b".to_string()]));
        assert!(same.is_none());
    }

    #[test]
    fn baseline_round_trips_through_json() {
        let mods = vec![m("ws/1", "a.one", "One", Some("1.0"), Some(1), Source::Workshop, 100)];
        let base = Baseline::take(&mods, &HashMap::from([(1u64, 7u64)]), &["a.one".into()], 9);
        let json = serde_json::to_string(&base).unwrap();
        assert!(json.contains("\"workshopUpdated\":7"));
        let back: Baseline = serde_json::from_str(&json).unwrap();
        assert_eq!(back.mods["ws/1"], base.mods["ws/1"]);
        assert_eq!(back.active, vec!["a.one"]);
        // An older baseline without the newer fields still loads.
        let old: Baseline = serde_json::from_str(r#"{"takenAt":1,"mods":{"x":{"name":"X","packageId":"x","source":"local","modified":3}},"active":[]}"#).unwrap();
        assert_eq!(old.mods["x"].workshop_updated, 0);
    }
}
