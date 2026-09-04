//! `ModsConfig.xml`: the file RimWorld reads its active list from.

use crate::model::*;
use crate::xmlutil::{child_text, escape, list, read_text, with_document};
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModsConfig {
    /// Game version string the file was written by.
    pub version: String,
    /// packageIds in load order, as written (may carry the `_steam` suffix).
    pub active_mods: Vec<String>,
    pub known_expansions: Vec<String>,
}

pub fn parse(text: &str, path: &Path) -> Result<ModsConfig> {
    with_document(text, path, |doc| {
        let root = doc.root_element();
        Ok(ModsConfig {
            version: child_text(root, "version").unwrap_or_default(),
            active_mods: list(root, "activeMods").into_iter().map(|s| s.trim().to_ascii_lowercase()).collect(),
            known_expansions: list(root, "knownExpansions").into_iter().map(|s| s.trim().to_ascii_lowercase()).collect(),
        })
    })
}

pub fn read(path: &Path) -> Result<ModsConfig> {
    let text = read_text(path)?;
    parse(&text, path)
}

pub fn to_xml(cfg: &ModsConfig) -> String {
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<ModsConfigData>\n");
    s.push_str(&format!("  <version>{}</version>\n", escape(&cfg.version)));
    s.push_str("  <activeMods>\n");
    for id in &cfg.active_mods {
        s.push_str(&format!("    <li>{}</li>\n", escape(id)));
    }
    s.push_str("  </activeMods>\n  <knownExpansions>\n");
    for id in &cfg.known_expansions {
        s.push_str(&format!("    <li>{}</li>\n", escape(id)));
    }
    s.push_str("  </knownExpansions>\n</ModsConfigData>\n");
    s
}

/// Write atomically (temp file + rename) and keep one `.bak` of the previous file.
pub fn write(path: &Path, cfg: &ModsConfig) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if path.exists() {
        let _ = std::fs::copy(path, path.with_extension("xml.bak"));
    }
    let tmp = path.with_extension("xml.tmp");
    std::fs::write(&tmp, to_xml(cfg))?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// True when a list is what RimWorld writes after it gave up loading: official content only.
pub fn looks_reset(ids: &[String]) -> bool {
    !ids.is_empty() && ids.iter().all(|id| id.to_ascii_lowercase().starts_with("ludeon.rimworld"))
}

/// One archived copy of the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedList {
    pub path: std::path::PathBuf,
    /// Unix seconds, from the file name.
    pub saved_at: i64,
    /// "saved" (written by Circinus), "seen" (found on disk, written by something else).
    pub label: String,
    pub count: usize,
    pub game_version: String,
}

fn archive_name(path: &Path) -> Option<(i64, String)> {
    let stem = path.file_stem()?.to_string_lossy().to_string();
    let (ts, label) = stem.split_once('-')?;
    Some((ts.parse().ok()?, label.to_string()))
}

/// Keep a copy of a list in `dir` as `<unix>-<label>.xml`, pruning to `keep` files. Returns
/// the path, or None when the newest copy already holds this exact list.
pub fn archive(dir: &Path, cfg: &ModsConfig, label: &str, keep: usize) -> Result<Option<std::path::PathBuf>> {
    std::fs::create_dir_all(dir)?;
    let existing = list_archive(dir);
    if let Some(newest) = existing.first() {
        if let Ok(prev) = read(&newest.path) {
            if prev.active_mods == cfg.active_mods {
                return Ok(None);
            }
        }
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let path = dir.join(format!("{now}-{label}.xml"));
    std::fs::write(&path, to_xml(cfg))?;
    for old in existing.iter().skip(keep.saturating_sub(1)) {
        let _ = std::fs::remove_file(&old.path);
    }
    Ok(Some(path))
}

/// Archived lists, newest first.
pub fn list_archive(dir: &Path) -> Vec<SavedList> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<SavedList> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "xml").unwrap_or(false))
        .filter_map(|p| {
            let (saved_at, label) = archive_name(&p)?;
            let cfg = read(&p).ok()?;
            Some(SavedList { path: p, saved_at, label, count: cfg.active_mods.len(), game_version: cfg.version })
        })
        .collect();
    out.sort_by(|a, b| b.saved_at.cmp(&a.saved_at).then_with(|| b.path.cmp(&a.path)));
    out
}

/// Source priority when one packageId is installed more than once (RimWorld prefers the
/// non-Workshop copy; the `_steam` suffix selects the Workshop copy explicitly).
fn priority(source: Source, prefer_steam: bool) -> u8 {
    match (source, prefer_steam) {
        (Source::Workshop, true) => 0,
        (Source::Ludeon, _) => 1,
        (Source::Local, _) => 2,
        (Source::SteamCmd, _) => 3,
        (Source::Git, _) => 4,
        (Source::Workshop, false) => 5,
    }
}

/// Map the ids in a ModsConfig to installed mod uids. Returns (uids in order, ids not installed).
pub fn resolve_active(ids: &[String], mods: &[ModInfo]) -> (Vec<String>, Vec<String>) {
    let mut by_id: HashMap<&str, Vec<&ModInfo>> = HashMap::new();
    for m in mods.iter().filter(|m| m.invalid.is_none() && !m.package_id.is_empty()) {
        by_id.entry(m.package_id.as_str()).or_default().push(m);
    }
    let mut out = Vec::new();
    let mut missing = Vec::new();
    let mut used = std::collections::HashSet::new();
    for raw in ids {
        let raw = raw.to_ascii_lowercase();
        let (id, prefer_steam) = match raw.strip_suffix("_steam") {
            Some(base) if by_id.contains_key(base) => (base.to_string(), true),
            _ => (raw.clone(), false),
        };
        match by_id.get(id.as_str()) {
            Some(cands) => {
                let mut sorted: Vec<&&ModInfo> = cands.iter().collect();
                sorted.sort_by_key(|m| (priority(m.source, prefer_steam), m.path.to_string_lossy().to_lowercase()));
                if let Some(pick) = sorted.into_iter().find(|m| !used.contains(&m.uid)) {
                    used.insert(pick.uid.clone());
                    out.push(pick.uid.clone());
                }
            }
            None => missing.push(raw),
        }
    }
    (out, missing)
}

/// Build the ModsConfig to save for an ordered list of uids.
pub fn build(order_uids: &[String], mods: &[ModInfo], version: &str, previous_known: &[String]) -> ModsConfig {
    let by_uid: HashMap<&str, &ModInfo> = mods.iter().map(|m| (m.uid.as_str(), m)).collect();
    let mut non_steam_ids = std::collections::HashSet::new();
    for m in mods {
        if m.source != Source::Workshop && !m.package_id.is_empty() {
            non_steam_ids.insert(m.package_id.as_str());
        }
    }
    let mut active = Vec::new();
    for uid in order_uids {
        if let Some(m) = by_uid.get(uid.as_str()) {
            if m.package_id.is_empty() {
                continue;
            }
            let needs_suffix = m.source == Source::Workshop && non_steam_ids.contains(m.package_id.as_str());
            active.push(if needs_suffix { format!("{}_steam", m.package_id) } else { m.package_id.clone() });
        }
    }
    let mut known: Vec<String> = previous_known.to_vec();
    for m in mods.iter().filter(|m| m.source == Source::Ludeon && m.package_id != CORE_PACKAGE_ID) {
        if !known.contains(&m.package_id) {
            known.push(m.package_id.clone());
        }
    }
    ModsConfig { version: version.to_string(), active_mods: active, known_expansions: known }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn mk(uid: &str, id: &str, source: Source) -> ModInfo {
        ModInfo { uid: uid.into(), path: PathBuf::from(uid), package_id: id.into(), name: id.into(), source, ..Default::default() }
    }

    #[test]
    fn round_trip() {
        let text = "<?xml version=\"1.0\" encoding=\"utf-8\"?><ModsConfigData><version>1.6.4530 rev1235</version><activeMods><li>ludeon.rimworld</li><li>Brrainz.Harmony</li></activeMods><knownExpansions><li>ludeon.rimworld.royalty</li></knownExpansions></ModsConfigData>";
        let cfg = parse(text, &PathBuf::from("ModsConfig.xml")).unwrap();
        assert_eq!(cfg.active_mods, vec!["ludeon.rimworld", "brrainz.harmony"]);
        let again = parse(&to_xml(&cfg), &PathBuf::from("x")).unwrap();
        assert_eq!(cfg, again);
    }

    #[test]
    fn archive_keeps_history_and_detects_reset() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("lists");
        let a = ModsConfig { version: "1.6".into(), active_mods: vec!["ludeon.rimworld".into(), "brrainz.harmony".into()], known_expansions: vec![] };
        let b = ModsConfig { active_mods: vec!["ludeon.rimworld".into()], ..a.clone() };
        assert!(archive(&dir, &a, "saved", 3).unwrap().is_some());
        assert!(archive(&dir, &a, "saved", 3).unwrap().is_none(), "same list is not archived twice");
        std::thread::sleep(std::time::Duration::from_millis(1100));
        assert!(archive(&dir, &b, "seen", 3).unwrap().is_some());
        let all = list_archive(&dir);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].label, "seen");
        assert_eq!(all[0].count, 1);
        assert_eq!(all[1].count, 2);
        assert!(looks_reset(&b.active_mods));
        assert!(!looks_reset(&a.active_mods));
        assert!(!looks_reset(&[]));
        // pruning
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let c = ModsConfig { active_mods: vec!["x.y".into()], ..a.clone() };
        archive(&dir, &c, "saved", 2).unwrap();
        assert_eq!(list_archive(&dir).len(), 2);
    }

    #[test]
    fn duplicates_and_steam_suffix() {
        let mods = vec![
            mk("/data/core", "ludeon.rimworld", Source::Ludeon),
            mk("/mods/harmony", "brrainz.harmony", Source::Local),
            mk("/ws/2009463077", "brrainz.harmony", Source::Workshop),
            mk("/ws/1", "a.b", Source::Workshop),
        ];
        let (uids, missing) = resolve_active(&["ludeon.rimworld".into(), "brrainz.harmony_steam".into(), "a.b".into(), "nope.mod".into()], &mods);
        assert_eq!(uids, vec!["/data/core", "/ws/2009463077", "/ws/1"]);
        assert_eq!(missing, vec!["nope.mod"]);
        let (uids, _) = resolve_active(&["brrainz.harmony".into()], &mods);
        assert_eq!(uids, vec!["/mods/harmony"]);
        let cfg = build(&["/data/core".into(), "/ws/2009463077".into(), "/ws/1".into()], &mods, "1.6", &[]);
        assert_eq!(cfg.active_mods, vec!["ludeon.rimworld", "brrainz.harmony_steam", "a.b"]);
    }
}
