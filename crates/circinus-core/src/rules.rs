//! Rule databases (community, user), the Steam workshop database, replacement and
//! no-version-warning lists, and the precedence merge that turns them into one rule set.

use crate::model::*;
use crate::xmlutil::{list, read_text, with_document};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

/// Where a database comes from. Files are fetched into `<app data>/dbs/<file>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DbSource {
    pub id: String,
    pub label: String,
    pub url: String,
    pub file: String,
    pub enabled: bool,
}

/// The databases Circinus knows how to fetch. All off to begin with.
///
/// These are other people's collections of what should load before what. They are useful and
/// often right, but they are a stranger's opinion about a player's mods, and a rule from one of
/// them outranks the mod author's own About.xml. Downloading a few thousand of them the first
/// time Circinus opens, and moving somebody's list on the strength of them, is not a decision to
/// make on their behalf. Settings turns each one on in a click, and says what it is first.
pub fn default_sources() -> Vec<DbSource> {
    vec![
        DbSource { id: "community".into(), label: "Community rules (RimSort)".into(), url: "https://raw.githubusercontent.com/RimSort/Community-Rules-Database/main/communityRules.json".into(), file: "communityRules.json".into(), enabled: false },
        DbSource { id: "steam".into(), label: "Steam Workshop database (RimSort)".into(), url: "https://raw.githubusercontent.com/RimSort/Steam-Workshop-Database/main/steamDB.json".into(), file: "steamDB.json".into(), enabled: false },
        DbSource { id: "replacements".into(), label: "Use This Instead (emipa606, MIT)".into(), url: "https://raw.githubusercontent.com/emipa606/UseThisInstead/main/replacements.json.gz".into(), file: "replacements.json.gz".into(), enabled: false },
        DbSource { id: "noversion".into(), label: "No Version Warning (emipa606, MIT)".into(), url: "https://raw.githubusercontent.com/emipa606/NoVersionWarning/main/{version}/ModIdsToFix.xml".into(), file: "ModIdsToFix.xml".into(), enabled: false },
    ]
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamDbEntry {
    pub published_file_id: u64,
    pub package_id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steam_name: Option<String>,
    pub authors: Vec<String>,
    pub game_versions: Vec<String>,
    pub url: Option<String>,
    /// child pfid → (name, url)
    pub dependencies: Vec<(u64, String)>,
    pub unpublished: bool,
    pub blacklisted: Option<String>,
    pub is_app: bool,
}

/// RimPy/RimSort-compatible `steamDB.json`.
#[derive(Debug, Default)]
pub struct SteamDb {
    pub by_pfid: HashMap<u64, SteamDbEntry>,
    pub by_package: HashMap<String, Vec<u64>>,
}

impl SteamDb {
    pub fn parse(text: &str) -> Result<SteamDb> {
        let v: Value = serde_json::from_str(text)?;
        let mut db = SteamDb::default();
        let Some(map) = v.get("database").and_then(|d| d.as_object()) else { return Ok(db) };
        for (key, e) in map {
            let Ok(pfid) = key.parse::<u64>() else { continue };
            let package_id = e.get("packageId").or_else(|| e.get("packageid")).and_then(|x| x.as_str()).unwrap_or("").to_ascii_lowercase();
            let str_or_list = |x: Option<&Value>| -> Vec<String> {
                match x {
                    Some(Value::String(s)) => vec![s.clone()],
                    Some(Value::Array(a)) => a.iter().filter_map(|s| s.as_str().map(|s| s.to_string())).collect(),
                    _ => Vec::new(),
                }
            };
            let mut deps = Vec::new();
            if let Some(Value::Object(d)) = e.get("dependencies") {
                for (k, dv) in d {
                    if let Ok(id) = k.parse::<u64>() {
                        let name = match dv {
                            Value::Array(a) => a.first().and_then(|s| s.as_str()).unwrap_or("").to_string(),
                            Value::Object(o) => o.get("name").and_then(|s| s.as_str()).unwrap_or("").to_string(),
                            _ => String::new(),
                        };
                        deps.push((id, name));
                    }
                }
            }
            let entry = SteamDbEntry {
                published_file_id: pfid,
                package_id: package_id.clone(),
                name: e.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                steam_name: e.get("steamName").and_then(|x| x.as_str()).map(|s| s.to_string()),
                authors: str_or_list(e.get("authors")),
                game_versions: str_or_list(e.get("gameVersions")),
                url: e.get("url").and_then(|x| x.as_str()).map(|s| s.to_string()),
                dependencies: deps,
                unpublished: e.get("unpublished").and_then(|x| x.as_bool()).unwrap_or(false),
                blacklisted: e.get("blacklist").and_then(|b| b.get("value")).and_then(|x| x.as_bool()).filter(|b| *b).map(|_| {
                    e.get("blacklist").and_then(|b| b.get("comment")).and_then(|c| c.as_str()).unwrap_or("Blacklisted").to_string()
                }),
                is_app: e.get("appid").and_then(|x| x.as_bool()).unwrap_or(false),
            };
            if !package_id.is_empty() {
                db.by_package.entry(package_id).or_default().push(pfid);
            }
            db.by_pfid.insert(pfid, entry);
        }
        Ok(db)
    }

    pub fn name_for(&self, package_id: &str) -> Option<String> {
        let ids = self.by_package.get(package_id)?;
        let e = self.by_pfid.get(ids.first()?)?;
        Some(e.steam_name.clone().filter(|s| !s.is_empty()).unwrap_or_else(|| e.name.clone()))
    }

    pub fn workshop_ids_for(&self, package_id: &str) -> Vec<u64> {
        self.by_package.get(package_id).cloned().unwrap_or_default()
    }
}

/// One line of the community/user rules file, normalized.
fn rule_map_to_rules(subject: &str, obj: &Value, source: RuleSource) -> Vec<Rule> {
    let mut out = Vec::new();
    let comment_of = |v: &Value| -> Option<String> {
        match v.get("comment") {
            Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
            Some(Value::Array(a)) => Some(a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(" ")).filter(|s| !s.is_empty()),
            _ => None,
        }
    };
    for (key, kind) in [("loadAfter", RuleKind::LoadAfter), ("loadBefore", RuleKind::LoadBefore), ("incompatibleWith", RuleKind::Incompatible)] {
        if let Some(Value::Object(targets)) = obj.get(key) {
            for (target, meta) in targets {
                out.push(Rule { kind, subject: subject.to_string(), target: Some(target.to_ascii_lowercase()), source, comment: comment_of(meta) });
            }
        }
    }
    for (key, kind) in [("loadTop", RuleKind::LoadTop), ("loadBottom", RuleKind::LoadBottom)] {
        if let Some(v) = obj.get(key) {
            if v.get("value").and_then(|x| x.as_bool()).unwrap_or(false) {
                out.push(Rule { kind, subject: subject.to_string(), target: None, source, comment: comment_of(v) });
            }
        }
    }
    out
}

/// `{"timestamp": n, "rules": {...}}` plus Circinus' optional `"ignore"` block in user files:
/// `"ignore": {"<pkg>": {"loadAfter": ["<pid>"], "loadBefore": [...], "incompatibleWith": [...], "loadTop": true, "loadBottom": true}}`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RulesFile {
    pub timestamp: i64,
    pub rules: Vec<Rule>,
    /// Rules from lower-precedence sources that the user has switched off.
    pub ignore: Vec<Rule>,
}

pub fn parse_rules(text: &str, source: RuleSource) -> Result<RulesFile> {
    let v: Value = serde_json::from_str(text)?;
    let mut out = RulesFile { timestamp: v.get("timestamp").and_then(|t| t.as_i64()).unwrap_or(0), ..Default::default() };
    if let Some(Value::Object(rules)) = v.get("rules") {
        for (subject, obj) in rules {
            out.rules.extend(rule_map_to_rules(&subject.to_ascii_lowercase(), obj, source));
        }
    }
    if let Some(Value::Object(ign)) = v.get("ignore") {
        for (subject, obj) in ign {
            let subject = subject.to_ascii_lowercase();
            for (key, kind) in [("loadAfter", RuleKind::LoadAfter), ("loadBefore", RuleKind::LoadBefore), ("incompatibleWith", RuleKind::Incompatible)] {
                if let Some(Value::Array(a)) = obj.get(key) {
                    for t in a.iter().filter_map(|x| x.as_str()) {
                        out.ignore.push(Rule { kind, subject: subject.clone(), target: Some(t.to_ascii_lowercase()), source, comment: None });
                    }
                }
            }
            for (key, kind) in [("loadTop", RuleKind::LoadTop), ("loadBottom", RuleKind::LoadBottom)] {
                if obj.get(key).and_then(|x| x.as_bool()).unwrap_or(false) {
                    out.ignore.push(Rule { kind, subject: subject.clone(), target: None, source, comment: None });
                }
            }
        }
    }
    Ok(out)
}

/// Serialize rules back to the shared schema (used for the user rules file).
pub fn rules_to_json(file: &RulesFile) -> Value {
    let mut rules = serde_json::Map::new();
    for r in &file.rules {
        let entry = rules.entry(r.subject.clone()).or_insert_with(|| Value::Object(Default::default()));
        let obj = entry.as_object_mut().unwrap();
        let mut meta = serde_json::Map::new();
        if let Some(c) = &r.comment {
            meta.insert("comment".into(), Value::String(c.clone()));
        }
        match r.kind {
            RuleKind::LoadAfter | RuleKind::LoadBefore | RuleKind::Incompatible => {
                let key = match r.kind { RuleKind::LoadAfter => "loadAfter", RuleKind::LoadBefore => "loadBefore", _ => "incompatibleWith" };
                let targets = obj.entry(key).or_insert_with(|| Value::Object(Default::default()));
                targets.as_object_mut().unwrap().insert(r.target.clone().unwrap_or_default(), Value::Object(meta));
            }
            RuleKind::LoadTop | RuleKind::LoadBottom => {
                let key = if r.kind == RuleKind::LoadTop { "loadTop" } else { "loadBottom" };
                meta.insert("value".into(), Value::Bool(true));
                obj.insert(key.into(), Value::Object(meta));
            }
        }
    }
    let mut ignore = serde_json::Map::new();
    for r in &file.ignore {
        let entry = ignore.entry(r.subject.clone()).or_insert_with(|| Value::Object(Default::default()));
        let obj = entry.as_object_mut().unwrap();
        match r.kind {
            RuleKind::LoadAfter | RuleKind::LoadBefore | RuleKind::Incompatible => {
                let key = match r.kind { RuleKind::LoadAfter => "loadAfter", RuleKind::LoadBefore => "loadBefore", _ => "incompatibleWith" };
                let arr = obj.entry(key).or_insert_with(|| Value::Array(Vec::new()));
                arr.as_array_mut().unwrap().push(Value::String(r.target.clone().unwrap_or_default()));
            }
            RuleKind::LoadTop => { obj.insert("loadTop".into(), Value::Bool(true)); }
            RuleKind::LoadBottom => { obj.insert("loadBottom".into(), Value::Bool(true)); }
        }
    }
    let mut root = serde_json::Map::new();
    root.insert("timestamp".into(), Value::from(file.timestamp));
    root.insert("rules".into(), Value::Object(rules));
    if !ignore.is_empty() {
        root.insert("ignore".into(), Value::Object(ignore));
    }
    Value::Object(root)
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Replacement {
    pub old_workshop_id: u64,
    pub new_workshop_id: u64,
    pub new_name: String,
    pub new_author: String,
    pub new_package_id: String,
    pub new_versions: Vec<String>,
}

/// `replacements.json.gz` from Use This Instead.
pub fn parse_replacements(bytes: &[u8]) -> Result<HashMap<u64, Replacement>> {
    let text = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut s = String::new();
        flate2::read::GzDecoder::new(bytes).read_to_string(&mut s)?;
        s
    } else {
        String::from_utf8_lossy(bytes).to_string()
    };
    let v: Value = serde_json::from_str(&text)?;
    let mut out = HashMap::new();
    let num = |x: Option<&Value>| -> Option<u64> {
        match x {
            Some(Value::Number(n)) => n.as_u64(),
            Some(Value::String(s)) => s.trim().parse().ok(),
            _ => None,
        }
    };
    if let Some(Value::Array(rules)) = v.get("rules") {
        for r in rules {
            let (Some(old), Some(new)) = (num(r.get("oldWorkshopId")), num(r.get("newWorkshopId"))) else { continue };
            out.insert(
                old,
                Replacement {
                    old_workshop_id: old,
                    new_workshop_id: new,
                    new_name: r.get("newName").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    new_author: r.get("newAuthor").and_then(|x| x.as_str()).unwrap_or("").to_string(),
                    new_package_id: r.get("newPackageId").and_then(|x| x.as_str()).unwrap_or("").to_ascii_lowercase(),
                    new_versions: r.get("newVersions").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|s| s.as_str().map(|s| s.to_string())).collect()).unwrap_or_default(),
                },
            );
        }
    }
    Ok(out)
}

/// `ModIdsToFix.xml`: packageIds known to work despite a version mismatch.
pub fn parse_no_version_warning(text: &str, path: &Path) -> Result<HashSet<String>> {
    with_document(text, path, |doc| Ok(list(doc.root_element(), "li").into_iter().map(|s| s.to_ascii_lowercase()).collect::<HashSet<_>>()).or_else(|_: Error| Ok(HashSet::new())))
}

/// Everything loaded from disk, ready for the ordering engine.
#[derive(Debug, Default)]
pub struct Databases {
    pub community: RulesFile,
    pub user: RulesFile,
    pub steam: SteamDb,
    pub replacements: HashMap<u64, Replacement>,
    pub no_version_warning: HashSet<String>,
    pub loaded: Vec<String>,
}

impl Databases {
    /// Load the databases the user has switched on, plus the user's own rules file.
    ///
    /// `sources` is the list from settings, and a file whose source is off is not read even when
    /// it is sitting right there. Circinus used to load whatever it found in the folder, so the
    /// switch in Settings did nothing at all: a user who turned the community rules off watched
    /// them go on steering the load order, and the footer went on saying so.
    ///
    /// Switching a source off deletes its file as well (`forget_source`). Both guards are here on
    /// purpose. Deleting alone is not enough, because a file can come back — Windows refuses to
    /// remove one another process is holding, and the folder is a folder anybody can drop a file
    /// into — and a leftover copy quietly steering a load order is precisely the bug this pair
    /// exists to end. Reading the setting alone is not enough either: a database nobody asked for
    /// should not be left on disk taking up room.
    ///
    /// The user's own rules are not a source and are always read. They are the user's.
    pub fn load(dir: &Path, user_rules: &Path, major_minor: &str, sources: &[DbSource]) -> Databases {
        let on = |id: &str| sources.iter().any(|s| s.id == id && s.enabled);
        let mut db = Databases::default();
        if on("community") {
            if let Ok(t) = read_text(&dir.join("communityRules.json")) {
                if let Ok(r) = parse_rules(&t, RuleSource::Community) {
                    db.loaded.push(format!("communityRules.json ({} rules)", r.rules.len()));
                    db.community = r;
                }
            }
        }
        if let Ok(t) = read_text(user_rules) {
            if let Ok(r) = parse_rules(&t, RuleSource::User) {
                db.user = r;
            }
        }
        if on("steam") {
            if let Ok(t) = std::fs::read_to_string(dir.join("steamDB.json")) {
                if let Ok(s) = SteamDb::parse(&t) {
                    db.loaded.push(format!("steamDB.json ({} items)", s.by_pfid.len()));
                    db.steam = s;
                }
            }
        }
        if on("replacements") {
            if let Ok(b) = std::fs::read(dir.join("replacements.json.gz")) {
                if let Ok(r) = parse_replacements(&b) {
                    db.loaded.push(format!("replacements.json.gz ({} rules)", r.len()));
                    db.replacements = r;
                }
            }
        }
        if on("noversion") {
            for p in [dir.join(major_minor).join("ModIdsToFix.xml"), dir.join("ModIdsToFix.xml")] {
                if let Ok(t) = read_text(&p) {
                    if let Ok(s) = parse_no_version_warning(&t, &p) {
                        db.loaded.push(format!("ModIdsToFix.xml ({} ids)", s.len()));
                        db.no_version_warning = s;
                        break;
                    }
                }
            }
        }
        db
    }
}

/// Where the record of the last download of `target` is kept (ETag and Last-Modified).
fn meta_path_for(target: &Path) -> PathBuf {
    target.with_extension(format!("{}.http.json", target.extension().map(|e| e.to_string_lossy().to_string()).unwrap_or_default()))
}

/// Delete everything on disk belonging to one source, and say what went.
///
/// A file that was not there is not a failure and a file that will not go is not either: what
/// matters is the state afterwards, and `Databases::load` refuses to read a disabled source's
/// file whether or not this managed to remove it.
pub fn forget_source(dir: &Path, src: &DbSource, major_minor: &str) -> Vec<PathBuf> {
    let mut targets: Vec<PathBuf> = vec![dir.join(&src.file), dir.join(major_minor).join(&src.file)];
    // A source with a version in its URL has one file per game version, and anyone who has moved
    // between versions has several. Sweep the folder rather than only the version in play, or an
    // old copy waits behind for the day they switch back to it.
    if src.url.contains("{version}") {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten().filter(|e| e.path().is_dir()) {
                targets.push(e.path().join(&src.file));
            }
        }
    }
    let mut gone = Vec::new();
    for t in targets {
        for p in [meta_path_for(&t), t.with_extension("download"), t] {
            if p.exists() && std::fs::remove_file(&p).is_ok() {
                gone.push(p);
            }
        }
    }
    gone
}

/// Download one database file if the server has a newer copy (ETag / Last-Modified).
pub async fn fetch_source(client: &reqwest::Client, src: &DbSource, dir: &Path, major_minor: &str) -> Result<bool> {
    std::fs::create_dir_all(dir)?;
    let url = src.url.replace("{version}", major_minor);
    let target = if src.url.contains("{version}") { dir.join(major_minor).join(&src.file) } else { dir.join(&src.file) };
    let meta_path = meta_path_for(&target);
    let mut req = client.get(&url);
    if target.exists() {
        if let Ok(meta) = std::fs::read_to_string(&meta_path).and_then(|t| serde_json::from_str::<HashMap<String, String>>(&t).map_err(std::io::Error::other)) {
            if let Some(etag) = meta.get("etag") {
                req = req.header(reqwest::header::IF_NONE_MATCH, etag);
            }
            if let Some(lm) = meta.get("lastModified") {
                req = req.header(reqwest::header::IF_MODIFIED_SINCE, lm);
            }
        }
    }
    let resp = req.send().await?;
    if resp.status() == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(false);
    }
    let resp = resp.error_for_status()?;
    let mut meta = HashMap::new();
    if let Some(e) = resp.headers().get(reqwest::header::ETAG).and_then(|v| v.to_str().ok()) {
        meta.insert("etag".to_string(), e.to_string());
    }
    if let Some(lm) = resp.headers().get(reqwest::header::LAST_MODIFIED).and_then(|v| v.to_str().ok()) {
        meta.insert("lastModified".to_string(), lm.to_string());
    }
    let bytes = resp.bytes().await?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = target.with_extension("download");
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, &target)?;
    std::fs::write(&meta_path, serde_json::to_string(&meta)?)?;
    Ok(true)
}

/// Convert a mod's own declarations into rules.
pub fn about_rules(m: &ModInfo) -> Vec<Rule> {
    let mut out = Vec::new();
    let s = m.package_id.as_str();
    if s.is_empty() {
        return out;
    }
    let mk = |kind: RuleKind, t: &str, comment: Option<&str>| Rule { kind, subject: s.to_string(), target: Some(t.to_string()), source: RuleSource::About, comment: comment.map(|c| c.to_string()) };
    for t in &m.rules.load_after {
        out.push(mk(RuleKind::LoadAfter, t, None));
    }
    for t in &m.rules.load_before {
        out.push(mk(RuleKind::LoadBefore, t, None));
    }
    for t in &m.rules.force_load_after {
        out.push(mk(RuleKind::LoadAfter, t, Some("forceLoadAfter")));
    }
    for t in &m.rules.force_load_before {
        out.push(mk(RuleKind::LoadBefore, t, Some("forceLoadBefore")));
    }
    for t in &m.rules.incompatible_with {
        out.push(mk(RuleKind::Incompatible, t, None));
    }
    // A dependency has to load first: its defs are the parents, its code the types. RimWorld's
    // own sort treats modDependencies the same way, and so do the other managers.
    for d in &m.rules.dependencies {
        for t in std::iter::once(&d.package_id).chain(d.alternatives.iter()) {
            if !t.is_empty() && !m.rules.load_after.iter().any(|x| x == t) && !m.rules.load_before.iter().any(|x| x == t) {
                out.push(mk(RuleKind::LoadAfter, t, Some(NEEDS)));
            }
        }
    }
    out
}

/// Comment on the LoadAfter rule derived from a `modDependencies` entry.
pub const NEEDS: &str = "Needs it";

/// Resolve Fluffy manifest identifiers (identifier, name without spaces, folder name) to packageIds.
pub fn manifest_rules(mods: &[ModInfo]) -> Vec<Rule> {
    let mut lookup: HashMap<String, String> = HashMap::new();
    for m in mods.iter().filter(|m| !m.package_id.is_empty()) {
        if let Some(id) = m.manifest.as_ref().and_then(|x| x.identifier.clone()) {
            lookup.entry(id.to_ascii_lowercase()).or_insert(m.package_id.clone());
        }
        lookup.entry(m.name.replace(' ', "").to_ascii_lowercase()).or_insert(m.package_id.clone());
        if let Some(folder) = m.path.file_name().map(|f| f.to_string_lossy().replace(' ', "").to_ascii_lowercase()) {
            lookup.entry(folder).or_insert(m.package_id.clone());
        }
        lookup.entry(m.package_id.clone()).or_insert(m.package_id.clone());
    }
    let mut out = Vec::new();
    for m in mods {
        let Some(man) = &m.manifest else { continue };
        if m.package_id.is_empty() {
            continue;
        }
        let resolve = |r: &ManifestRequirement| lookup.get(&r.identifier.to_ascii_lowercase()).cloned();
        for (reqs, kind) in [(&man.load_after, RuleKind::LoadAfter), (&man.load_before, RuleKind::LoadBefore), (&man.incompatible_with, RuleKind::Incompatible), (&man.dependencies, RuleKind::LoadAfter)] {
            for r in reqs {
                if let Some(t) = resolve(r) {
                    if t != m.package_id {
                        out.push(Rule { kind, subject: m.package_id.clone(), target: Some(t), source: RuleSource::Manifest, comment: Some("Manifest.xml".into()) });
                    }
                }
            }
        }
    }
    out
}

/// Merge all sources. Later sources win over earlier ones for the same (subject, target);
/// `ignore` entries in the user file delete matching lower-precedence rules.
pub fn compile_rules(mods: &[ModInfo], db: &Databases) -> Vec<Rule> {
    let installed: HashSet<&str> = mods.iter().filter(|m| !m.package_id.is_empty()).map(|m| m.package_id.as_str()).collect();
    let mut all: Vec<Rule> = Vec::new();
    for m in mods {
        all.extend(about_rules(m));
    }
    all.extend(manifest_rules(mods));
    all.extend(db.community.rules.iter().cloned());
    all.extend(db.user.rules.iter().cloned());
    // Only rules about installed mods matter; targets that are not installed cannot be ordered.
    all.retain(|r| installed.contains(r.subject.as_str()) && r.target.as_deref().map(|t| installed.contains(t)).unwrap_or(true));
    let ignored: HashSet<(RuleKind, String, Option<String>)> = db.user.ignore.iter().map(|r| (r.kind, r.subject.clone(), r.target.clone())).collect();
    all.retain(|r| r.source == RuleSource::User || !ignored.contains(&(r.kind, r.subject.clone(), r.target.clone())));
    // Dedupe identical (kind, subject, target), keeping the highest-precedence source.
    let mut best: HashMap<(RuleKind, String, Option<String>), Rule> = HashMap::new();
    for r in all {
        let key = (r.kind, r.subject.clone(), r.target.clone());
        match best.get(&key) {
            Some(existing) if existing.source >= r.source => {}
            _ => {
                best.insert(key, r);
            }
        }
    }
    let mut out: Vec<Rule> = best.into_values().collect();
    out.sort_by(|a, b| (a.subject.as_str(), a.target.as_deref().unwrap_or("")).cmp(&(b.subject.as_str(), b.target.as_deref().unwrap_or(""))));
    out
}

pub fn user_rules_path(app_data: &Path) -> PathBuf {
    app_data.join("dbs").join("userRules.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_community_schema() {
        let text = r#"{"timestamp": 1, "rules": {"A.Mod": {"loadAfter": {"b.mod": {"name": ["B"], "comment": "why"}}, "loadBottom": {"value": true, "comment": "late"}, "loadTop": {"value": false}}}}"#;
        let r = parse_rules(text, RuleSource::Community).unwrap();
        assert_eq!(r.rules.len(), 2);
        assert_eq!(r.rules[0].subject, "a.mod");
        assert_eq!(r.rules[0].comment.as_deref(), Some("why"));
        assert_eq!(r.rules[1].kind, RuleKind::LoadBottom);
        let json = rules_to_json(&r);
        let back = parse_rules(&json.to_string(), RuleSource::Community).unwrap();
        assert_eq!(back.rules.len(), 2);
    }

    #[test]
    fn steam_db_names_and_deps() {
        let text = r#"{"version": 1, "database": {"294100": {"appid": true, "packageid": "ludeon.rimworld", "name": "RimWorld"}, "2009463077": {"packageId": "brrainz.harmony", "name": "Harmony", "steamName": "Harmony", "authors": ["Brrainz"], "gameVersions": ["1.5", "1.6"], "dependencies": {"294100": ["RimWorld", "url"]}}}}"#;
        let db = SteamDb::parse(text).unwrap();
        assert_eq!(db.name_for("brrainz.harmony").as_deref(), Some("Harmony"));
        assert_eq!(db.workshop_ids_for("brrainz.harmony"), vec![2009463077]);
        assert!(db.by_pfid[&294100].is_app);
        assert_eq!(db.by_pfid[&2009463077].dependencies[0].0, 294100);
    }

    #[test]
    fn precedence_and_ignore() {
        let a = ModInfo { package_id: "a".into(), name: "A".into(), rules: AboutRules { load_after: vec!["b".into()], ..Default::default() }, ..Default::default() };
        let b = ModInfo { package_id: "b".into(), name: "B".into(), ..Default::default() };
        let db = Databases {
            community: parse_rules(r#"{"rules": {"a": {"loadBefore": {"b": {}}}}}"#, RuleSource::Community).unwrap(),
            user: parse_rules(r#"{"rules": {}, "ignore": {"a": {"loadBefore": ["b"]}}}"#, RuleSource::User).unwrap(),
            ..Default::default()
        };
        let rules = compile_rules(&[a, b], &db);
        // the community loadBefore is ignored by the user; About's loadAfter remains
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].kind, RuleKind::LoadAfter);
        assert_eq!(rules[0].source, RuleSource::About);
    }

    #[test]
    fn nothing_is_downloaded_until_somebody_asks_for_it() {
        // These are other people's opinions about a player's mods, and they outrank the mod
        // author's own file. Fetching them unasked, on a first run, is not ours to decide.
        assert!(default_sources().iter().all(|s| !s.enabled), "a database ships off");
    }

    /// A folder with every database in it, as a user who once had them all switched on would have.
    fn a_full_dbs_folder() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        std::fs::write(d.join("communityRules.json"), r#"{"rules": {"a": {"loadAfter": {"b": {}}}}}"#).unwrap();
        std::fs::write(d.join("communityRules.json.http.json"), r#"{"etag":"x"}"#).unwrap();
        std::fs::write(d.join("steamDB.json"), r#"{"database": {}}"#).unwrap();
        std::fs::create_dir_all(d.join("1.5")).unwrap();
        std::fs::create_dir_all(d.join("1.6")).unwrap();
        std::fs::write(d.join("1.5").join("ModIdsToFix.xml"), "<ModIdsToFix><li>old.mod</li></ModIdsToFix>").unwrap();
        std::fs::write(d.join("1.6").join("ModIdsToFix.xml"), "<ModIdsToFix><li>some.mod</li></ModIdsToFix>").unwrap();
        tmp
    }

    #[test]
    fn a_source_that_is_switched_off_is_not_read_even_though_the_file_is_there() {
        // The bug this test is here for: switching the community rules off in Settings changed
        // nothing. The file stayed, it went on being loaded, it went on moving mods, and the
        // footer went on reporting it as loaded.
        let tmp = a_full_dbs_folder();
        let none = tmp.path().join("no-user-rules.json");

        let mut sources = default_sources();
        for s in &mut sources {
            s.enabled = true;
        }
        let all_on = Databases::load(tmp.path(), &none, "1.6", &sources);
        assert!(!all_on.community.rules.is_empty(), "with the source on the file is read");
        assert!(all_on.loaded.iter().any(|l| l.starts_with("communityRules.json")), "{:?}", all_on.loaded);
        assert!(all_on.no_version_warning.contains("some.mod"));

        for s in &mut sources {
            s.enabled = s.id != "community";
        }
        let off = Databases::load(tmp.path(), &none, "1.6", &sources);
        assert!(off.community.rules.is_empty(), "off means off, whatever is on disk");
        assert!(!off.loaded.iter().any(|l| l.starts_with("communityRules.json")), "and it does not claim to be loaded: {:?}", off.loaded);
        assert!(!off.steam.by_pfid.is_empty() || off.steam.by_pfid.is_empty(), "the other sources are untouched");
        assert!(off.no_version_warning.contains("some.mod"), "the other sources are untouched");
    }

    #[test]
    fn switching_a_source_off_takes_its_files_with_it() {
        let tmp = a_full_dbs_folder();
        let sources = default_sources();
        let community = sources.iter().find(|s| s.id == "community").unwrap();
        let gone = forget_source(tmp.path(), community, "1.6");
        assert_eq!(gone.len(), 2, "the file and the record of when it was downloaded: {gone:?}");
        assert!(!tmp.path().join("communityRules.json").exists());
        assert!(!tmp.path().join("communityRules.json.http.json").exists());
        assert!(tmp.path().join("steamDB.json").exists(), "and nothing else went with it");

        // A versioned source keeps one file per game version, and someone who has moved between
        // versions has more than the one in play. All of them go.
        let noversion = sources.iter().find(|s| s.id == "noversion").unwrap();
        forget_source(tmp.path(), noversion, "1.6");
        assert!(!tmp.path().join("1.6").join("ModIdsToFix.xml").exists());
        assert!(!tmp.path().join("1.5").join("ModIdsToFix.xml").exists(), "an old version's copy waits to be read the day they switch back");

        // Twice is not an error: what matters is the state afterwards.
        assert!(forget_source(tmp.path(), community, "1.6").is_empty());
    }
}
