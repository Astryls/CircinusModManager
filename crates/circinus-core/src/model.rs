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

impl Dependency {
    /// The Workshop item this dependency points at, from either URL it may carry
    /// (`steam://url/CommunityFilePage/2009463077`, `…filedetails/?id=2009463077`).
    pub fn workshop_id(&self) -> Option<u64> {
        fn id_in(url: &str) -> Option<u64> {
            let tail = match url.rfind("id=") {
                Some(i) => &url[i + 3..],
                None => url.rsplit('/').next()?,
            };
            let digits: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
            (digits.len() >= 6).then(|| digits.parse().ok()).flatten()
        }
        self.workshop_url.as_deref().and_then(id_in).or_else(|| self.download_url.as_deref().and_then(id_in))
    }
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
    /// What loading the mod costs the game, from the folder alone (see `loadcost`).
    #[serde(default)]
    pub load: LoadCost,
}

/// The parts of a mod folder that cost loading time, and the estimate made from them.
/// Counts and bytes are facts; `score_ms` is a model (`loadcost::score`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LoadCost {
    /// Bytes of XML under Defs: parsed, inherited, reflected into objects.
    pub def_bytes: u64,
    /// Bytes of XML under Patches.
    pub patch_bytes: u64,
    /// Patch operations, nested ones included.
    pub patch_ops: u32,
    /// Operations whose XPath searches the whole document (`//`, wildcard steps, `contains()`).
    pub heavy_ops: u32,
    /// Pixels of PNG/JPG textures the game will decode: those with no DDS beside them.
    pub png_pixels: u64,
    pub png_bytes: u64,
    /// Bytes of DDS files, loaded as they are.
    pub dds_bytes: u64,
    pub dll_bytes: u64,
    pub sound_bytes: u64,
    /// The estimate, in milliseconds on a typical machine. Relative to other mods it means
    /// something; on its own it is a guess.
    pub score_ms: u64,
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
    /// Newest mtime seen under the folder, seconds since the epoch. For a Workshop item this is
    /// the newer of that and Steam's `timeupdated`, because Steam can replace files deep inside
    /// an item without touching the folder -- so this answers "when did the content last change",
    /// which is what the change detector and the cache stamp need.
    pub modified: u64,
    /// When Steam says the author last published an update, seconds since the epoch; 0 for
    /// anything Steam has never updated. Kept apart from `modified` rather than folded into it,
    /// because "the files on my disk changed" and "the author shipped something" are different
    /// questions and a player sorting a list is usually asking one or the other.
    #[serde(default)]
    pub updated: u64,
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
            // "Preloads", not "Before the game": the second one is what this table said while
            // `PHASES` in types.ts -- the copy the window actually renders -- said the first,
            // and nobody noticed because nothing on screen reads this one. Two copies of one
            // table drift; keeping them in step is the only thing holding that shut.
            Phase::Prepatch => "Preloads",
            Phase::Framework => "Libraries",
            Phase::Content => "Content",
            Phase::Patch => "Patches",
            Phase::Texture => "Texture packs",
            Phase::Late => "Late loaders",
            Phase::Optimization => "Performance",
        }
    }
}

/// Where a rule came from, in ascending precedence: what Circinus worked out for itself is the
/// weakest thing in the graph, and the person at the keyboard is the strongest.
///
/// The order is the whole precedence system -- `Ord` is derived from it, the contradiction
/// resolver compares on it, and the cycle cutter drops the `min` -- so it is worth saying what it
/// means. A mod's author is the authority on their own mod, and About.xml is the file the game
/// itself sorts by, so nothing Circinus infers may outrank it. The databases sit below the author
/// because they are other people's read of somebody else's mod, and they ship switched off. HALO
/// is last because everything at that rung is a guess: an order read out of a file the game does
/// not read, a dependency taken to mean an order, a rule kept in reserve for mods nobody has said
/// anything about. A guess should be the first thing dropped when a loop has to be cut, and it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleSource {
    /// Worked out by Circinus rather than declared by anybody: a `modDependencies` entry read as
    /// "load after", an order read out of a Fluffy `Manifest.xml`, the rule that keeps a mod
    /// nobody has spoken for below the game's own content.
    Halo,
    /// The downloaded rule databases. Off until switched on.
    Community,
    /// The mod's author, in About.xml -- the file RimWorld itself sorts by.
    About,
    /// You.
    User,
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
        /// The mods along the loop, in order. `uids[i]` loads before `uids[i + 1]`, and the last
        /// wraps back to the first.
        uids: Vec<String>,
        /// Human-readable chain, e.g. "A → B → C → A".
        chain: String,
        /// The rule behind each step, in the same order as `uids`: `rules[i]` is why `uids[i]`
        /// must come before the next one. This is what tells a player which rule to change.
        rules: Vec<Rule>,
        /// The one rule that was dropped so sorting could finish. None only if the loop could
        /// not be cut, which should not happen.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cut: Option<Rule>,
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
        /// Somebody asked for this: the mod's own About.xml, a database in use, a user rule, or
        /// the user filing it under Prepatch. Then it is a warning about what the placement may
        /// cost rather than an error about a list nobody chose -- a mod whose defs inherit
        /// nothing from the game loads above it perfectly well, and only its author knows.
        declared: bool,
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
            Issue::AboveOfficial { declared, .. } => {
                if *declared {
                    Severity::Warning
                } else {
                    Severity::Error
                }
            }
            Issue::MissingDependency { .. } | Issue::Incompatible { .. } | Issue::Cycle { .. } | Issue::Invalid { .. } => Severity::Error,
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
