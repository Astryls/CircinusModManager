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
    // **An add-on replacing its parent's art is not a collision.**
    //
    // Reported as: Busywork shown as "replacing" Useful Marks, which it requires to work. It
    // ships its own `Textures/marks/wait`, Useful Marks ships the same path, and Busywork
    // sorts later precisely *because* it declares a dependency on Useful Marks -- so the one
    // fact that makes it an add-on was also the fact that made it the winner, and this rule
    // had never looked at it. Overriding the icons of the mod you extend is the add-on
    // working, and a note saying so is noise on every list that has one.
    //
    // Narrow on purpose: the collision is dropped only when **every** loser in it is a mod
    // the winner declares a dependency on. An add-on that also clobbers a texture belonging
    // to some third mod still gets its note, because that one is a real surprise.
    let by_uid: HashMap<&str, &&ModInfo> = order.iter().map(|m| (m.uid.as_str(), m)).collect();
    let declared = |winner: &str, loser: &str| -> bool {
        let (Some(w), Some(l)) = (by_uid.get(winner), by_uid.get(loser)) else { return false };
        let want = crate::order::id_base(&l.package_id);
        let about = w.rules.dependencies.iter().any(|d| {
            crate::order::id_base(&d.package_id) == want || d.alternatives.iter().any(|a| crate::order::id_base(a) == want)
        });
        // Fluffy's manifest is read at the weakest rung everywhere else and is read here too:
        // a mod that declares the requirement only there is still declaring it.
        let manifest = w
            .manifest
            .as_ref()
            .map(|mf| mf.dependencies.iter().any(|d| crate::order::id_base(&d.identifier.to_ascii_lowercase()) == want))
            .unwrap_or(false);
        about || manifest
    };
    let mut out: Vec<Issue> = providers
        .into_iter()
        .filter(|(_, uids)| uids.len() > 1)
        .filter(|(_, uids)| {
            let Some(winner) = uids.last() else { return false };
            !uids.iter().filter(|u| *u != winner).all(|loser| declared(winner, loser))
        })
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

    /// **An add-on overriding its parent's art is not a collision.**
    ///
    /// Busywork requires Useful Marks and ships its own `marks/wait`; the player was told
    /// "Its marks/wait is replaced again by Busywork, which loads later", which reads as one
    /// mod replacing another and is a note about nothing. The dependency that makes Busywork
    /// an add-on is the same fact that puts it later in the order, and this rule had never
    /// read it.
    ///
    /// Three cases, because the suppression has to be narrow: the add-on's own override goes
    /// quiet, an unrelated mod's does not, and an add-on that *also* steps on a third mod
    /// still gets told about that one. A test that only covered the first would pass on a
    /// rule that silenced every collision a dependency was anywhere near.
    #[test]
    fn an_addon_overriding_its_parent_is_not_a_collision() {
        let parent = m("marks", "andromeda.usefulmarks", None);
        let mut addon = m("busy", "andromeda.busywork", None);
        addon.rules.dependencies = vec![crate::model::Dependency { package_id: "andromeda.usefulmarks".into(), ..Default::default() }];
        let stranger = m("other", "someone.else", None);

        let mut files = HashMap::new();
        files.insert("marks".into(), ModFiles { textures: vec!["textures/marks/wait".into(), "textures/shared/icon".into()], ..Default::default() });
        files.insert("busy".into(), ModFiles { textures: vec!["textures/marks/wait".into(), "textures/third/thing".into()], ..Default::default() });
        files.insert("other".into(), ModFiles { textures: vec!["textures/shared/icon".into(), "textures/third/thing".into()], ..Default::default() });

        // Order: parent, stranger, add-on. The add-on wins both of its paths.
        let issues = collisions(&[&parent, &stranger, &addon], &files, "1.6");
        let paths: Vec<&str> = issues
            .iter()
            .map(|i| match i {
                Issue::TextureCollision { path, .. } => path.as_str(),
                _ => "",
            })
            .collect();

        // 1. The add-on over its own parent: silent.
        assert!(!paths.contains(&"marks/wait"), "an add-on was reported for replacing art in the mod it requires: {paths:?}");
        // 2. A mod it has no relationship with: still reported.
        assert!(paths.contains(&"third/thing"), "a real collision with an unrelated mod went missing: {paths:?}");
        // 3. And a collision the add-on is not even in is untouched.
        assert!(paths.contains(&"shared/icon"), "a collision between two other mods was dropped: {paths:?}");

        // Without the dependency it is an ordinary collision again, which is the control: the
        // suppression must turn on the declaration and nothing else.
        let mut plain = addon.clone();
        plain.rules.dependencies.clear();
        let again = collisions(&[&parent, &stranger, &plain], &files, "1.6");
        assert!(
            again.iter().any(|i| matches!(i, Issue::TextureCollision { path, .. } if path == "marks/wait")),
            "two mods that share a path and declare nothing are still a collision"
        );
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
