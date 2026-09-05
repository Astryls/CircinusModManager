//! Shared data model. Serialized to the UI as camelCase JSON.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// RimWorld's Steam app id.
pub const RIMWORLD_APP_ID: u32 = 294100;

/// Official content: Steam app id ↔ packageId. Their About.xml files omit names.
pub const OFFICIAL: &[(u32, &str, &str)] = &[
    (294100, "ludeon.rimworld", "RimWorld"),
    (1149640, "ludeon.rimworld.royalty", "Royalty"),
    (1392840, "ludeon.rimworld.ideology", "Ideology"),
    (1826140, "ludeon.rimworld.biotech", "Biotech"),
    (2380740, "ludeon.rimworld.anomaly", "Anomaly"),
    (3022790, "ludeon.rimworld.odyssey", "Odyssey"),
];

pub const CORE_PACKAGE_ID: &str = "ludeon.rimworld";

pub fn official_name(package_id: &str) -> Option<&'static str> {
    OFFICIAL.iter().find(|(_, p, _)| *p == package_id).map(|(_, _, n)| *n)
}

pub fn official_package_id(app_id: u32) -> Option<&'static str> {
    OFFICIAL.iter().find(|(a, _, _)| *a == app_id).map(|(_, p, _)| *p)
}

/// Where a mod folder lives.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// `<game>/Data` — Core and DLC.
    Ludeon,
    /// `<steam library>/steamapps/workshop/content/294100/<id>`
    Workshop,
    /// `<game>/Mods/<folder>` with no workshop marker.
    #[default]
    Local,
    /// `<game>/Mods/<id>` with `About/PublishedFileId.txt` — downloaded by SteamCMD.
    SteamCmd,
    /// `<game>/Mods/<folder>` that is a git checkout.
    Git,
}

/// One declared dependency (`modDependencies/li`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dependency {
    pub package_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workshop_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_url: Option<String>,
    /// Alternative packageIds that also satisfy this dependency.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alternatives: Vec<String>,
}

/// Ordering and compatibility declarations from About.xml, already resolved for the game version.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AboutRules {
    pub load_after: Vec<String>,
    pub load_before: Vec<String>,
    /// `forceLoadAfter`/`forceLoadBefore` — RimWorld enforces these itself; we treat them as strong.
    pub force_load_after: Vec<String>,
    pub force_load_before: Vec<String>,
    pub incompatible_with: Vec<String>,
    pub dependencies: Vec<Dependency>,
}

/// A versioned requirement from Fluffy's Mod Manager `About/Manifest.xml` (`ident [op version]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestRequirement {
    pub identifier: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub op: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Fluffy's Mod Manager manifest (MIT format, widely shipped by mods; ignored by other managers).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<ManifestRequirement>,
    #[serde(default)]
    pub incompatible_with: Vec<ManifestRequirement>,
    #[serde(default)]
    pub load_before: Vec<ManifestRequirement>,
    #[serde(default)]
    pub load_after: Vec<ManifestRequirement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_uri: Option<String>,
}

/// One `<li>` of a LoadFolders.xml version block.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadFolder {
    /// Relative to the mod root; "/" or "" means the root itself.
    pub path: String,
    /// `IfModActive` — any of these must be active (comma separated in the file).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub if_mod_active: Vec<String>,
    /// `IfModNotActive` — none of these may be active.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub if_mod_not_active: Vec<String>,
}

/// What a mod folder contains — drives HALO phase classification and the analyzers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contents {
    pub assemblies: u32,
    pub patches: u32,
    pub defs: u32,
    pub textures: u32,
    pub dds: u32,
    pub sounds: u32,
    pub languages: u32,
    /// Ships its own copy of 0Harmony.dll (a smell; Harmony should come from the Harmony mod).
    pub bundles_harmony: bool,
    pub size_bytes: u64,
}

/// A parsed, installed mod. `uid` is the folder path and is the identity everywhere.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModInfo {
    pub uid: String,
    /// The entry in the mod folder, the path RimWorld reports for the mod.
    pub path: PathBuf,
    /// Where the files really are when `path` is a link (a symlink, or a junction on Windows)
    /// to a folder kept elsewhere, as Modmixer and other dev tools do it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_target: Option<PathBuf>,
    /// Lowercased packageId. Empty when About.xml lacks one.
    pub package_id: String,
    pub name: String,
    pub authors: Vec<String>,
    pub description: String,
    pub supported_versions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mod_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steam_app_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_file_id: Option<u64>,
    pub source: Source,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<PathBuf>,
    pub rules: AboutRules,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest: Option<Manifest>,
    /// Load folders for the game version (None = RimWorld defaults).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_folders: Option<Vec<LoadFolder>>,
    pub contents: Contents,
    /// Newest mtime seen under the folder, seconds since the epoch.
    pub modified: u64,
    /// Human explanation when the folder is not a usable mod.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invalid: Option<String>,
    /// Loose classification for people: C# mod, XML mod, texture pack…
    pub kind: ModKind,
}

impl ModInfo {
    pub fn is_official(&self) -> bool {
        self.source == Source::Ludeon
    }
    pub fn supports(&self, major_minor: &str) -> bool {
        self.supported_versions.is_empty() || self.supported_versions.iter().any(|v| v == major_minor)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModKind {
    #[default]
    Unknown,
    Official,
    Code,
    Xml,
    Textures,
    Translation,
    Scenario,
}

/// Phases of the Harmonized Automated Load Order, in load order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Prepatch,
    Core,
    Framework,
    Content,
    Patch,
    Texture,
    /// Asked to load near the bottom (a `loadBottom` rule), together with everything that must
    /// load after it. Big content mods with many add-ons live here, not performance mods.
    Late,
    Optimization,
}

impl Phase {
    pub const ALL: [Phase; 8] = [
        Phase::Prepatch,
        Phase::Core,
        Phase::Framework,
        Phase::Content,
        Phase::Patch,
        Phase::Texture,
        Phase::Late,
        Phase::Optimization,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Phase::Core => "Game and DLC",
            Phase::Prepatch => "Before the game",
            Phase::Framework => "Libraries",
            Phase::Content => "Content",
            Phase::Patch => "Patches",
            Phase::Texture => "Texture packs",
            Phase::Late => "Late loaders",
            Phase::Optimization => "Performance",
        }
    }
}

/// Where a rule came from, in ascending precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleSource {
    About,
    Manifest,
    Community,
    User,
    /// Derived by HALO (phase placement, dependency implies loadAfter).
    Halo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuleKind {
    /// `subject` must load after `target`.
    LoadAfter,
    /// `subject` must load before `target`.
    LoadBefore,
    Incompatible,
    LoadTop,
    LoadBottom,
}

/// A single ordering/compatibility rule about `subject`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub kind: RuleKind,
    pub subject: String,
    /// None for LoadTop / LoadBottom.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub source: RuleSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Note,
}

/// Something the validator wants a person to see.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "kind")]
pub enum Issue {
    MissingDependency {
        uid: String,
        dependency: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display_name: Option<String>,
        /// Installed but inactive: can be activated rather than downloaded.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        installed_uid: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        workshop_url: Option<String>,
    },
    Incompatible {
        uid: String,
        other_uid: String,
        source: RuleSource,
    },
    OrderViolation {
        uid: String,
        target_uid: String,
        rule: RuleKind,
        source: RuleSource,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        comment: Option<String>,
    },
    VersionMismatch {
        uid: String,
        supported: Vec<String>,
    },
    Cycle {
        uids: Vec<String>,
        /// Human-readable chain, e.g. "A → B → C → A".
        chain: String,
        rules: Vec<Rule>,
    },
    TextureCollision {
        /// Relative path under Textures/, without extension.
        path: String,
        uids: Vec<String>,
        winner_uid: String,
    },
    MisplacedOptimization {
        uid: String,
        after_uids: Vec<String>,
    },
    DuplicatePackageId {
        package_id: String,
        uids: Vec<String>,
    },
    MissingPackageId {
        uid: String,
    },
    Invalid {
        uid: String,
        reason: String,
    },
    /// A mod that ships Defs sits above official content (or a DLC above Core). RimWorld resolves
    /// `ParentName` only against mods loaded earlier, so its defs lose their parents and loading
    /// fails.
    AboveOfficial {
        uid: String,
        /// The official mod it should be below.
        official_uid: String,
    },
    /// A declared rule HALO set aside, and why.
    RuleIgnored {
        uid: String,
        target_uid: String,
        rule: RuleKind,
        source: RuleSource,
        reason: String,
    },
}

impl Issue {
    pub fn severity(&self) -> Severity {
        match self {
            Issue::MissingDependency { .. } | Issue::Incompatible { .. } | Issue::Cycle { .. } | Issue::Invalid { .. } | Issue::AboveOfficial { .. } => Severity::Error,
            Issue::OrderViolation { .. } | Issue::VersionMismatch { .. } | Issue::MisplacedOptimization { .. } | Issue::DuplicatePackageId { .. } | Issue::MissingPackageId { .. } => Severity::Warning,
            Issue::TextureCollision { .. } | Issue::RuleIgnored { .. } => Severity::Note,
        }
    }
    /// The mod this issue is attached to in the UI.
    pub fn primary_uid(&self) -> Option<&str> {
        match self {
            Issue::MissingDependency { uid, .. }
            | Issue::Incompatible { uid, .. }
            | Issue::OrderViolation { uid, .. }
            | Issue::VersionMismatch { uid, .. }
            | Issue::MisplacedOptimization { uid, .. }
            | Issue::MissingPackageId { uid }
            | Issue::Invalid { uid, .. }
            | Issue::AboveOfficial { uid, .. }
            | Issue::RuleIgnored { uid, .. } => Some(uid),
            Issue::TextureCollision { winner_uid, .. } => Some(winner_uid),
            Issue::Cycle { uids, .. } | Issue::DuplicatePackageId { uids, .. } => uids.first().map(|s| s.as_str()),
        }
    }
}

/// What HALO decided for one mod.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    pub uid: String,
    pub phase: Phase,
    /// Why it landed in that phase, in one short sentence.
    pub reason: String,
    /// The user's group it sorts with when that group has its own place in the order (a
    /// section right after the ordinary members of `phase`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
}

/// Result of a HALO sort: the proposed order and what moved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SortResult {
    /// uids in proposed load order.
    pub order: Vec<String>,
    pub placements: Vec<Placement>,
    /// (uid, from index, to index) for every mod whose position changed.
    pub moves: Vec<(String, usize, usize)>,
    pub issues: Vec<Issue>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The UI reads camelCase; variant fields must follow (rename_all alone does not cover them).
    #[test]
    fn issue_fields_are_camel_case_on_the_wire() {
        let i = Issue::MisplacedOptimization { uid: "a".into(), after_uids: vec!["b".into()] };
        let json = serde_json::to_string(&i).unwrap();
        assert!(json.contains("\"afterUids\""), "{json}");
        let i = Issue::OrderViolation { uid: "a".into(), target_uid: "b".into(), rule: RuleKind::LoadAfter, source: RuleSource::Community, comment: None };
        let json = serde_json::to_string(&i).unwrap();
        assert!(json.contains("\"targetUid\"") && json.contains("\"loadAfter\"") && json.contains("\"community\""), "{json}");
        let i = Issue::MissingDependency { uid: "a".into(), dependency: "d".into(), display_name: Some("D".into()), installed_uid: None, workshop_url: None };
        let json = serde_json::to_string(&i).unwrap();
        assert!(json.contains("\"displayName\"") && !json.contains("installed_uid"), "{json}");
        let back: Issue = serde_json::from_str(&json).unwrap();
        assert_eq!(back, i);
    }
}
