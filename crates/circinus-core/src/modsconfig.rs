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
