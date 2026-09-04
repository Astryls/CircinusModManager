//! Texture override analysis. RimWorld loads every active mod's `Textures/` tree into one
//! namespace keyed by relative path (extension ignored); the last mod to provide a path wins.

use crate::game::cmp_major_minor;
use crate::model::*;
use crate::scan::ModFiles;
use std::collections::{HashMap, HashSet};

/// The load folders RimWorld would use for a mod, relative to its root ("" = root).
pub fn active_load_folders(m: &ModInfo, files: &ModFiles, major_minor: &str, active_ids: &HashSet<String>) -> Vec<String> {
    if let Some(folders) = &m.load_folders {
        return folders
            .iter()
            .filter(|f| f.if_mod_active.is_empty() || f.if_mod_active.iter().any(|id| active_ids.contains(id)))
            .filter(|f| f.if_mod_not_active.iter().all(|id| !active_ids.contains(id)))
            .map(|f| f.path.replace('\\', "/").to_ascii_lowercase())
            .collect();
    }
    // Defaults: the version folder (exact, else the newest one not above the game version),
    // then Common, then the root.
    let mut version_dirs: HashSet<String> = HashSet::new();
    for p in files.textures.iter().chain(files.defs.iter()).chain(files.patches.iter()).chain(files.assemblies.iter()) {
        if let Some(first) = p.split('/').next() {
            if crate::game::major_minor_of(first).map(|mm| mm == first).unwrap_or(false) {
                version_dirs.insert(first.to_string());
            }
        }
    }
    let mut out = Vec::new();
    if version_dirs.contains(major_minor) {
        out.push(major_minor.to_string());
    } else if let Some(best) = version_dirs.iter().filter(|v| cmp_major_minor(v, major_minor) != std::cmp::Ordering::Greater).max_by(|a, b| cmp_major_minor(a, b)) {
        out.push(best.clone());
    }
    out.push("common".into());
    out.push(String::new());
    out
}

/// Texture paths (relative to a Textures/ dir, no extension) this mod contributes under the
/// current game version and active set.
pub fn contributed_textures(m: &ModInfo, files: &ModFiles, major_minor: &str, active_ids: &HashSet<String>) -> Vec<String> {
    let folders = active_load_folders(m, files, major_minor, active_ids);
    let mut out: Vec<String> = Vec::new();
    for f in folders {
        let prefix = if f.is_empty() { "textures/".to_string() } else { format!("{}/textures/", f.trim_matches('/')) };
        for t in &files.textures {
            if let Some(rel) = t.strip_prefix(&prefix) {
                out.push(rel.to_string());
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Every texture path provided by more than one active mod, with the winner under `order`.
pub fn collisions(order: &[&ModInfo], files: &HashMap<String, ModFiles>, major_minor: &str) -> Vec<Issue> {
    let active_ids: HashSet<String> = order.iter().map(|m| m.package_id.clone()).collect();
    let mut providers: HashMap<String, Vec<String>> = HashMap::new();
    for m in order {
        let Some(f) = files.get(&m.uid) else { continue };
        if f.textures.is_empty() {
            continue;
        }
        for path in contributed_textures(m, f, major_minor, &active_ids) {
            providers.entry(path).or_default().push(m.uid.clone());
        }
    }
    let mut out: Vec<Issue> = providers
        .into_iter()
        .filter(|(_, uids)| uids.len() > 1)
        .map(|(path, uids)| Issue::TextureCollision { winner_uid: uids.last().cloned().unwrap_or_default(), path, uids })
        .collect();
    out.sort_by(|a, b| match (a, b) {
        (Issue::TextureCollision { path: pa, .. }, Issue::TextureCollision { path: pb, .. }) => pa.cmp(pb),
        _ => std::cmp::Ordering::Equal,
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(uid: &str, id: &str, lf: Option<Vec<LoadFolder>>) -> ModInfo {
        ModInfo { uid: uid.into(), package_id: id.into(), name: id.into(), load_folders: lf, ..Default::default() }
    }

    #[test]
    fn finds_collisions_with_load_folders() {
        let vte = m("vte", "vanillaexpanded.vtexe", None);
        let walls = m("walls", "nyx.retrowalls", Some(vec![LoadFolder { path: "".into(), ..Default::default() }, LoadFolder { path: "1.6".into(), ..Default::default() }, LoadFolder { path: "Royalty".into(), if_mod_active: vec!["ludeon.rimworld.royalty".into()], ..Default::default() }]));
        let mut files = HashMap::new();
        files.insert("vte".into(), ModFiles { textures: vec!["textures/things/building/linked/wall_atlas".into(), "textures/things/pawn/x".into(), "1.4/textures/old".into()], ..Default::default() });
        files.insert("walls".into(), ModFiles { textures: vec!["textures/things/building/linked/wall_atlas".into(), "1.6/textures/things/pawn/x".into(), "royalty/textures/things/building/linked/wall_atlas".into(), "1.5/textures/things/building/door".into()], ..Default::default() });
        let issues = collisions(&[&vte, &walls], &files, "1.6");
        assert_eq!(issues.len(), 2);
        match &issues[0] {
            Issue::TextureCollision { path, uids, winner_uid } => {
                assert_eq!(path, "things/building/linked/wall_atlas");
                assert_eq!(uids, &vec!["vte".to_string(), "walls".to_string()]);
                assert_eq!(winner_uid, "walls");
            }
            _ => panic!(),
        }
    }

    #[test]
    fn default_folders_pick_closest_version() {
        let mm = m("a", "a", None);
        let f = ModFiles { textures: vec!["1.4/textures/x".into(), "1.5/textures/x".into(), "common/textures/y".into()], ..Default::default() };
        let folders = active_load_folders(&mm, &f, "1.6", &HashSet::new());
        assert_eq!(folders, vec!["1.5", "common", ""]);
        let c = contributed_textures(&mm, &f, "1.6", &HashSet::new());
        assert_eq!(c, vec!["x", "y"]);
    }
}
