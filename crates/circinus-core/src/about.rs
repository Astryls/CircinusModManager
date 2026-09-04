//! Parsers for the files inside a mod's `About/` folder: About.xml, Manifest.xml (Fluffy's Mod
//! Manager) and the mod-root LoadFolders.xml.

use crate::game::version_tag_matches;
use crate::model::*;
use crate::xmlutil::{child, child_text, li_texts, list, tag_is, with_document};
use crate::Result;
use roxmltree::Node;
use std::path::Path;

/// Everything About.xml tells us, before the folder is inspected.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct About {
    pub package_id: String,
    pub name: String,
    pub authors: Vec<String>,
    pub description: String,
    pub supported_versions: Vec<String>,
    pub mod_version: Option<String>,
    pub url: Option<String>,
    pub steam_app_id: Option<u32>,
    pub rules: AboutRules,
}

fn norm_id(s: &str) -> String {
    s.trim().to_ascii_lowercase()
}

fn norm_ids(v: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(v.len());
    for s in v {
        let id = norm_id(&s);
        if !id.is_empty() && !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

/// Pick the child of a `*ByVersion` node that applies to `major_minor`.
/// Exact `vX.Y` wins, then `X.Y`, then any tag that normalizes to X.Y (e.g. `v1.6.4530`).
fn by_version<'a, 'i>(node: Node<'a, 'i>, major_minor: &str) -> Option<Node<'a, 'i>> {
    let elems: Vec<Node> = node.children().filter(|c| c.is_element()).collect();
    let want_v = format!("v{major_minor}");
    if let Some(n) = elems.iter().find(|n| n.tag_name().name().eq_ignore_ascii_case(&want_v)) {
        return Some(*n);
    }
    if let Some(n) = elems.iter().find(|n| n.tag_name().name() == major_minor) {
        return Some(*n);
    }
    elems.into_iter().find(|n| version_tag_matches(n.tag_name().name(), major_minor))
}

/// Base list, replaced entirely by a matching ByVersion block (RimWorld's semantics: a versioned
/// block overrides, it does not add).
fn versioned_list(root: Node, base_tag: &str, major_minor: &str) -> Vec<String> {
    let base = list(root, base_tag);
    let by = child(root, &format!("{base_tag}ByVersion"));
    match by.and_then(|n| by_version(n, major_minor)) {
        Some(block) => li_texts(block),
        None => base,
    }
}

fn parse_dependency(li: Node) -> Option<Dependency> {
    if li.attribute("IsNull").map(|v| v.eq_ignore_ascii_case("true")).unwrap_or(false) {
        return None;
    }
    let package_id = child_text(li, "packageId").map(|s| norm_id(&s))?;
    if package_id.is_empty() {
        return None;
    }
    Some(Dependency {
        package_id,
        display_name: child_text(li, "displayName"),
        workshop_url: child_text(li, "steamWorkshopUrl"),
        download_url: child_text(li, "downloadUrl"),
        alternatives: norm_ids(list(li, "alternativePackageIds")),
    })
}

fn dependencies_of(node: Node) -> Vec<Dependency> {
    let mut out: Vec<Dependency> = Vec::new();
    for li in node.children().filter(|c| tag_is(*c, "li")) {
        if let Some(d) = parse_dependency(li) {
            if !out.iter().any(|x| x.package_id == d.package_id) {
                out.push(d);
            }
        }
    }
    out
}

/// Parse About.xml text. `major_minor` selects the `*ByVersion` blocks.
pub fn parse_about(text: &str, path: &Path, major_minor: &str) -> Result<About> {
    with_document(text, path, |doc| {
        let root = doc.root_element();
        // RimWorld requires <ModMetaData>; some mods get the casing wrong. Accept anything.
        let mut a = About::default();
        a.package_id = child_text(root, "packageId").map(|s| norm_id(&s)).unwrap_or_default();
        a.name = child_text(root, "name").unwrap_or_default();
        let mut authors = Vec::new();
        if let Some(author) = child_text(root, "author") {
            authors.extend(author.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()));
        }
        for x in list(root, "authors") {
            if !authors.contains(&x) {
                authors.push(x);
            }
        }
        a.authors = authors;
        a.description = child_text(root, "description").unwrap_or_default();
        if let Some(by) = child(root, "descriptionsByVersion").and_then(|n| by_version(n, major_minor)) {
            if let Some(t) = by.text() {
                let t = t.trim();
                if !t.is_empty() {
                    a.description = t.to_string();
                }
            }
        }
        let mut versions: Vec<String> = list(root, "supportedVersions")
            .iter()
            .filter_map(|v| crate::game::major_minor_of(v))
            .collect();
        if versions.is_empty() {
            if let Some(tv) = child_text(root, "targetVersion").and_then(|v| crate::game::major_minor_of(&v)) {
                versions.push(tv);
            }
        }
        versions.dedup();
        a.supported_versions = versions;
        a.mod_version = child_text(root, "modVersion");
        a.url = child_text(root, "url");
        a.steam_app_id = child_text(root, "steamAppId").and_then(|s| s.trim().parse().ok());

        let mut rules = AboutRules::default();
        rules.load_after = norm_ids(versioned_list(root, "loadAfter", major_minor));
        rules.load_before = norm_ids(versioned_list(root, "loadBefore", major_minor));
        rules.force_load_after = norm_ids(list(root, "forceLoadAfter"));
        rules.force_load_before = norm_ids(list(root, "forceLoadBefore"));
        rules.incompatible_with = norm_ids(versioned_list(root, "incompatibleWith", major_minor));
        let base_deps = child(root, "modDependencies").map(dependencies_of).unwrap_or_default();
        rules.dependencies = match child(root, "modDependenciesByVersion").and_then(|n| by_version(n, major_minor)) {
            Some(block) => dependencies_of(block),
            None => base_deps,
        };
        a.rules = rules;
        Ok(a)
    })
}

fn parse_requirement(s: &str) -> Option<ManifestRequirement> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    match parts.as_slice() {
        [] => None,
        [id] => Some(ManifestRequirement { identifier: id.to_string(), op: None, version: None }),
        [id, op, ver, ..] => Some(ManifestRequirement { identifier: id.to_string(), op: Some(op.to_string()), version: Some(ver.to_string()) }),
        [id, _] => Some(ManifestRequirement { identifier: id.to_string(), op: None, version: None }),
    }
}

/// Parse Fluffy's `About/Manifest.xml`.
pub fn parse_manifest(text: &str, path: &Path) -> Result<Manifest> {
    with_document(text, path, |doc| {
        let root = doc.root_element();
        let reqs = |tag: &str| list(root, tag).iter().filter_map(|s| parse_requirement(s)).collect::<Vec<_>>();
        Ok(Manifest {
            identifier: child_text(root, "identifier"),
            version: child_text(root, "version"),
            dependencies: reqs("dependencies"),
            incompatible_with: reqs("incompatibleWith"),
            load_before: reqs("loadBefore"),
            load_after: reqs("loadAfter"),
            manifest_uri: child_text(root, "manifestUri"),
            download_uri: child_text(root, "downloadUri"),
        })
    })
}

/// Parse `LoadFolders.xml` for one game version. Returns None when the file has no block for
/// this version and no `default`, in which case RimWorld's implicit folders apply.
pub fn parse_load_folders(text: &str, path: &Path, major_minor: &str) -> Result<Option<Vec<LoadFolder>>> {
    with_document(text, path, |doc| {
        let root = doc.root_element();
        let block = by_version(root, major_minor).or_else(|| child(root, "default"));
        let Some(block) = block else { return Ok(None) };
        let split = |s: Option<&str>| -> Vec<String> {
            s.map(|v| v.split(',').map(|x| norm_id(x)).filter(|x| !x.is_empty()).collect()).unwrap_or_default()
        };
        let folders = block
            .children()
            .filter(|c| tag_is(*c, "li"))
            .map(|li| LoadFolder {
                path: li.text().unwrap_or("").trim().trim_matches(['/', '\\']).to_string(),
                if_mod_active: split(li.attribute("IfModActive").or_else(|| li.attribute("IfModActiveAny"))),
                if_mod_not_active: split(li.attribute("IfModNotActive")),
            })
            .collect();
        Ok(Some(folders))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const ABOUT: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<ModMetaData>
  <packageId>VouLT.BetterPawnControl</packageId>
  <name>Better Pawn Control</name>
  <author>VouLT</author>
  <supportedVersions><li>1.5</li><li>1.6</li></supportedVersions>
  <modDependencies>
    <li><packageId>brrainz.harmony</packageId><displayName>Harmony</displayName>
      <steamWorkshopUrl>steam://url/CommunityFilePage/2009463077</steamWorkshopUrl></li>
  </modDependencies>
  <loadAfter><li>brrainz.harmony</li><li>UnlimitedHugs.HugsLib</li></loadAfter>
  <loadAfterByVersion>
    <v1.6><li>brrainz.harmony</li><li>ludeon.rimworld.biotech</li></v1.6>
  </loadAfterByVersion>
  <incompatibleWith><li>Some.Mod &amp; Co</li></incompatibleWith>
  <description>Tools &amp; policies</description>
</ModMetaData>"#;

    #[test]
    fn about_basics_and_byversion_replacement() {
        let a = parse_about(ABOUT, &PathBuf::from("About.xml"), "1.6").unwrap();
        assert_eq!(a.package_id, "voult.betterpawncontrol");
        assert_eq!(a.name, "Better Pawn Control");
        assert_eq!(a.authors, vec!["VouLT"]);
        assert_eq!(a.supported_versions, vec!["1.5", "1.6"]);
        assert_eq!(a.rules.dependencies.len(), 1);
        assert_eq!(a.rules.dependencies[0].workshop_url.as_deref(), Some("steam://url/CommunityFilePage/2009463077"));
        // v1.6 block replaces the base loadAfter list entirely
        assert_eq!(a.rules.load_after, vec!["brrainz.harmony", "ludeon.rimworld.biotech"]);
        // no 1.5 block: base list applies
        let b = parse_about(ABOUT, &PathBuf::from("About.xml"), "1.5").unwrap();
        assert_eq!(b.rules.load_after, vec!["brrainz.harmony", "unlimitedhugs.hugslib"]);
        assert_eq!(a.rules.incompatible_with, vec!["some.mod & co"]);
    }

    #[test]
    fn lenient_about_with_bare_ampersand() {
        let text = ABOUT.replace("&amp;", "&");
        let a = parse_about(&text, &PathBuf::from("About.xml"), "1.6").unwrap();
        assert_eq!(a.description, "Tools & policies");
    }

    #[test]
    fn manifest_requirements() {
        let m = parse_manifest(
            r#"<Manifest><identifier>ModManager</identifier><version>0.1.0.0</version>
            <dependencies><li>FrameWorkMod</li><li>SomeMod &gt;= 4.0</li></dependencies>
            <loadAfter><li>ToModOrNotToMod</li></loadAfter></Manifest>"#,
            &PathBuf::from("Manifest.xml"),
        )
        .unwrap();
        assert_eq!(m.identifier.as_deref(), Some("ModManager"));
        assert_eq!(m.dependencies[1].op.as_deref(), Some(">="));
        assert_eq!(m.dependencies[1].version.as_deref(), Some("4.0"));
        assert_eq!(m.load_after[0].identifier, "ToModOrNotToMod");
    }

    #[test]
    fn load_folders_pick_version_then_default() {
        let text = r#"<loadFolders>
          <default><li>/</li><li>Common</li></default>
          <v1.6><li>/</li><li>1.6</li><li IfModActive="Ludeon.RimWorld.Royalty">Royalty</li><li IfModNotActive="a.b, c.d">NoAB</li></v1.6>
        </loadFolders>"#;
        let f = parse_load_folders(text, &PathBuf::from("LoadFolders.xml"), "1.6").unwrap().unwrap();
        assert_eq!(f.len(), 4);
        assert_eq!(f[0].path, "");
        assert_eq!(f[2].if_mod_active, vec!["ludeon.rimworld.royalty"]);
        assert_eq!(f[3].if_mod_not_active, vec!["a.b", "c.d"]);
        let d = parse_load_folders(text, &PathBuf::from("LoadFolders.xml"), "1.4").unwrap().unwrap();
        assert_eq!(d.len(), 2);
        assert_eq!(d[1].path, "Common");
    }
}
