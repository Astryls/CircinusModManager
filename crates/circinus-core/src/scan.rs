//! Discover and parse installed mods from the three roots RimWorld reads:
//! `<game>/Data`, `<game>/Mods` and the Workshop content folder.

use crate::about::{parse_about, parse_load_folders, parse_manifest};
use crate::cache::Cache;
use crate::game::GameVersion;
use crate::model::*;
use crate::paths::Locations;
use crate::xmlutil::read_text;
use crate::Result;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

/// Bump when the parser changes so cached entries are re-parsed.
const PARSER_VERSION: u32 = 3;

/// File inventory kept out of `ModInfo` (too large for the UI): relative paths from the mod
/// root, lowercase, forward slashes. Textures are stored without extension because RimWorld
/// resolves them that way (`Wall_Atlas.png` and `Wall_Atlas.dds` are the same texture).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModFiles {
    pub textures: Vec<String>,
    pub patches: Vec<String>,
    pub defs: Vec<String>,
    pub assemblies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanOptions {
    pub locations: Locations,
    pub game_version: GameVersion,
    pub use_cache: bool,
    /// Workshop id → Steam's `timeupdated` from `appworkshop_294100.acf`. Steam can replace
    /// files deep inside an item without touching the folder's mtime, so this is part of the
    /// cache stamp and of `ModInfo::modified` for Workshop items.
    #[serde(default)]
    pub workshop_updated: HashMap<u64, u64>,
}

#[derive(Debug, Default)]
pub struct ScanOutput {
    pub mods: Vec<ModInfo>,
    pub files: HashMap<String, ModFiles>,
    pub from_cache: usize,
    pub parsed: usize,
    /// uids whose folders have not been walked yet (quick scan). Pass them to `inspect_mods`.
    pub shallow: Vec<String>,
    /// uid → cache stamp for entries not yet written to the cache.
    pub stamps: HashMap<String, String>,
}

/// Result of walking one mod folder.
#[derive(Debug, Clone)]
pub struct Inspection {
    pub uid: String,
    pub contents: Contents,
    pub files: ModFiles,
    pub modified: u64,
}

#[derive(Serialize, Deserialize)]
struct CachedEntry {
    info: ModInfo,
    files: ModFiles,
}

fn mtime_secs(md: &std::fs::Metadata) -> u64 {
    md.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0)
}

/// Case-insensitive lookup of a direct child entry.
fn find_entry(dir: &Path, name: &str) -> Option<PathBuf> {
    let direct = dir.join(name);
    if direct.exists() {
        return Some(direct);
    }
    let rd = std::fs::read_dir(dir).ok()?;
    rd.filter_map(|e| e.ok()).map(|e| e.path()).find(|p| p.file_name().map(|f| f.to_string_lossy().eq_ignore_ascii_case(name)).unwrap_or(false))
}

struct Candidate {
    path: PathBuf,
    source: Source,
    about_xml: Option<PathBuf>,
    stamp: String,
    /// Newest of the cheap signals: folder mtime, About.xml mtime, Steam's timeupdated.
    modified: u64,
}

fn candidates_in(root: &Path, source: Source, game_version: &str, workshop_updated: &HashMap<u64, u64>) -> Vec<Candidate> {
    let Ok(rd) = std::fs::read_dir(root) else { return Vec::new() };
    let mut out = Vec::new();
    for entry in rd.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if name.starts_with('.') || name.eq_ignore_ascii_case("__MACOSX") {
            continue;
        }
        let about_dir = find_entry(&path, "About");
        let about_xml = about_dir.as_ref().and_then(|d| find_entry(d, "About.xml"));
        let dir_mtime = std::fs::metadata(&path).map(|m| mtime_secs(&m)).unwrap_or(0);
        let about_mtime = about_xml.as_ref().and_then(|p| std::fs::metadata(p).ok()).map(|m| mtime_secs(&m)).unwrap_or(0);
        // Workshop folders are named after the item id; Steam's own record of when it last
        // updated the item is the only reliable sign of an in-place update.
        let ws_updated = if source == Source::Workshop { name.parse::<u64>().ok().and_then(|id| workshop_updated.get(&id).copied()).unwrap_or(0) } else { 0 };
        let stamp = if ws_updated > 0 { format!("{PARSER_VERSION}:{game_version}:{dir_mtime}:{about_mtime}:{ws_updated}") } else { format!("{PARSER_VERSION}:{game_version}:{dir_mtime}:{about_mtime}") };
        out.push(Candidate { source, path, about_xml, stamp, modified: dir_mtime.max(about_mtime).max(ws_updated) });
    }
    out
}

/// Walk a mod folder once: counts, sizes, newest mtime and the file inventory.
fn inspect_folder(root: &Path) -> (Contents, ModFiles, u64) {
    let mut c = Contents::default();
    let mut f = ModFiles::default();
    let mut newest = 0u64;
    let mut language_dirs = std::collections::HashSet::new();
    let walker = WalkDir::new(root).follow_links(false).into_iter().filter_entry(|e| {
        let n = e.file_name().to_string_lossy();
        !(e.depth() > 0 && (n == ".git" || n.eq_ignore_ascii_case("Source") || n == ".vs" || n.eq_ignore_ascii_case("obj")))
    });
    for entry in walker.filter_map(|e| e.ok()) {
        let Ok(md) = entry.metadata() else { continue };
        newest = newest.max(mtime_secs(&md));
        if !md.is_file() {
            continue;
        }
        c.size_bytes += md.len();
        let rel = entry.path().strip_prefix(root).unwrap_or(entry.path());
        let rel_s = rel.to_string_lossy().replace('\\', "/").to_ascii_lowercase();
        let segs: Vec<&str> = rel_s.split('/').collect();
        let ext = rel.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        let has_seg = |s: &str| segs[..segs.len().saturating_sub(1)].iter().any(|x| *x == s);
        if has_seg("assemblies") && ext == "dll" {
            c.assemblies += 1;
            if segs.last().map(|n| *n == "0harmony.dll").unwrap_or(false) {
                c.bundles_harmony = true;
            }
            f.assemblies.push(rel_s.clone());
        } else if has_seg("patches") && ext == "xml" {
            c.patches += 1;
            f.patches.push(rel_s.clone());
        } else if has_seg("defs") && ext == "xml" {
            c.defs += 1;
            f.defs.push(rel_s.clone());
        } else if has_seg("textures") && matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "dds" | "psd") {
            if ext == "dds" {
                c.dds += 1;
            } else {
                c.textures += 1;
            }
            let stem = match rel_s.rfind('.') {
                Some(i) => rel_s[..i].to_string(),
                None => rel_s.clone(),
            };
            if !f.textures.last().map(|l| *l == stem).unwrap_or(false) {
                f.textures.push(stem);
            }
        } else if has_seg("sounds") && matches!(ext.as_str(), "wav" | "ogg" | "mp3") {
            c.sounds += 1;
        } else if let Some(i) = segs.iter().position(|s| *s == "languages") {
            if let Some(lang) = segs.get(i + 1) {
                language_dirs.insert(lang.to_string());
            }
        }
    }
    f.textures.sort();
    f.textures.dedup();
    c.languages = language_dirs.len() as u32;
    (c, f, newest)
}

fn read_published_file_id(about_dir: Option<&Path>, folder: &Path) -> Option<u64> {
    if let Some(p) = about_dir.and_then(|d| find_entry(d, "PublishedFileId.txt")) {
        if let Ok(t) = read_text(&p) {
            let digits: String = t.trim().chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(v) = digits.parse::<u64>() {
                if v > 0 {
                    return Some(v);
                }
            }
        }
    }
    folder.file_name().and_then(|n| n.to_string_lossy().parse::<u64>().ok()).filter(|v| *v > 0)
}

fn classify(info: &ModInfo) -> ModKind {
    let c = &info.contents;
    if info.source == Source::Ludeon {
        return ModKind::Official;
    }
    if info.invalid.as_deref().map(|r| r.starts_with("Scenario")).unwrap_or(false) {
        return ModKind::Scenario;
    }
    if c.assemblies > 0 {
        return ModKind::Code;
    }
    if c.languages > 0 && c.defs == 0 && c.patches == 0 && c.textures + c.dds == 0 {
        return ModKind::Translation;
    }
    if c.textures + c.dds > 0 && c.defs == 0 && c.patches == 0 {
        return ModKind::Textures;
    }
    if c.defs + c.patches > 0 {
        return ModKind::Xml;
    }
    ModKind::Unknown
}

/// Everything that can be known from `About/` alone — no folder walk. `contents` stays empty and
/// `kind` is Unknown until `inspect_mods` fills them in.
fn parse_quick(c: &Candidate, gv: &GameVersion) -> ModInfo {
    let uid = c.path.to_string_lossy().to_string();
    let about_dir = c.about_xml.as_ref().and_then(|p| p.parent().map(|p| p.to_path_buf()));
    let mut info = ModInfo { uid: uid.clone(), path: c.path.clone(), source: c.source, ..Default::default() };
    info.modified = c.modified;

    match &c.about_xml {
        Some(about_path) => match read_text(about_path).and_then(|t| parse_about(&t, about_path, &gv.major_minor)) {
            Ok(a) => {
                info.package_id = a.package_id;
                info.name = a.name;
                info.authors = a.authors;
                info.description = a.description;
                info.supported_versions = a.supported_versions;
                info.mod_version = a.mod_version;
                info.url = a.url;
                info.steam_app_id = a.steam_app_id;
                info.rules = a.rules;
            }
            Err(e) => {
                info.invalid = Some(format!("About.xml could not be read: {e}"));
            }
        },
        None => {
            let rsc = std::fs::read_dir(&c.path)
                .ok()
                .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().map(|e| e == "rsc").unwrap_or(false)).collect::<Vec<_>>())
                .unwrap_or_default();
            if rsc.len() == 1 {
                info.name = rsc[0].file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                info.invalid = Some("Scenario file, not a mod".into());
            } else {
                info.invalid = Some("No About/About.xml in this folder".into());
            }
        }
    }

    if info.source == Source::Ludeon {
        if let Some(n) = official_name(&info.package_id) {
            if info.name.is_empty() {
                info.name = n.to_string();
            }
        }
        if info.name.is_empty() {
            info.name = c.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        }
        if info.authors.is_empty() {
            info.authors.push("Ludeon Studios".into());
        }
        if info.supported_versions.is_empty() {
            info.supported_versions.push(gv.major_minor.clone());
        }
        if info.steam_app_id.is_none() {
            info.steam_app_id = OFFICIAL.iter().find(|(_, p, _)| *p == info.package_id).map(|(a, _, _)| *a);
        }
    }
    if info.name.is_empty() {
        info.name = if !info.package_id.is_empty() { info.package_id.clone() } else { c.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default() };
    }
    info.published_file_id = if info.source == Source::Ludeon { None } else { read_published_file_id(about_dir.as_deref(), &c.path) };
    if info.source == Source::Local && info.published_file_id.is_some() && about_dir.as_ref().and_then(|d| find_entry(d, "PublishedFileId.txt")).is_some() {
        info.source = Source::SteamCmd;
    }
    if info.source == Source::Local && c.path.join(".git").exists() {
        info.source = Source::Git;
    }
    info.preview = about_dir.as_ref().and_then(|d| find_entry(d, "Preview.png"));
    if let Some(mp) = about_dir.as_ref().and_then(|d| find_entry(d, "Manifest.xml")) {
        if let Ok(m) = read_text(&mp).and_then(|t| parse_manifest(&t, &mp)) {
            info.manifest = Some(m);
        }
    }
    if let Some(lf) = find_entry(&c.path, "LoadFolders.xml") {
        if let Ok(Some(folders)) = read_text(&lf).and_then(|t| parse_load_folders(&t, &lf, &gv.major_minor)) {
            info.load_folders = Some(folders);
        }
    }
    info.kind = classify(&info);
    info
}

/// Walk one mod folder (the slow part) and return what it contains.
pub fn inspect_one(info: &ModInfo) -> Inspection {
    let (contents, files, newest) = inspect_folder(&info.path);
    Inspection { uid: info.uid.clone(), contents, files, modified: newest.max(info.modified) }
}

/// Walk many mod folders in parallel. `progress(done, total)` is called from worker threads.
pub fn inspect_mods(mods: &[ModInfo], progress: &(dyn Fn(usize, usize) + Sync)) -> Vec<Inspection> {
    let total = mods.len();
    let done = AtomicUsize::new(0);
    mods.par_iter()
        .map(|m| {
            let r = inspect_one(m);
            progress(done.fetch_add(1, Ordering::Relaxed) + 1, total);
            r
        })
        .collect()
}

/// Merge inspections into a mod list, re-classify, and write the finished entries to the cache.
pub fn apply_inspections(mods: &mut [ModInfo], files: &mut HashMap<String, ModFiles>, stamps: &HashMap<String, String>, inspections: Vec<Inspection>, cache: Option<&Cache>) -> Result<usize> {
    let mut to_store: Vec<(String, String, String)> = Vec::new();
    for ins in inspections {
        let Some(m) = mods.iter_mut().find(|m| m.uid == ins.uid) else { continue };
        m.contents = ins.contents;
        m.modified = ins.modified;
        m.kind = classify(m);
        if let Some(stamp) = stamps.get(&m.uid) {
            if let Ok(json) = serde_json::to_string(&CachedEntry { info: m.clone(), files: ins.files.clone() }) {
                to_store.push((m.uid.clone(), stamp.clone(), json));
            }
        }
        files.insert(ins.uid, ins.files);
    }
    let n = to_store.len();
    if let Some(c) = cache {
        if !to_store.is_empty() {
            c.store_mod_entries(&to_store)?;
        }
    }
    Ok(n)
}

/// Scan all roots. With `deep` false, folders that are not in the cache are only read from
/// `About/` (fast) and listed in `ScanOutput::shallow` for a later `inspect_mods` pass; with
/// `deep` true everything is walked here. `progress(done, total)` is called from worker threads.
pub fn scan(opts: &ScanOptions, cache: Option<&Cache>, deep: bool, progress: &(dyn Fn(usize, usize) + Sync)) -> Result<ScanOutput> {
    let started = std::time::Instant::now();
    let loc = &opts.locations;
    let gv = &opts.game_version;
    let mut cands: Vec<Candidate> = Vec::new();
    if let Some(data) = loc.data_dir() {
        cands.extend(candidates_in(&data, Source::Ludeon, &gv.major_minor, &opts.workshop_updated));
    }
    if let Some(local) = &loc.local_mods_dir {
        cands.extend(candidates_in(local, Source::Local, &gv.major_minor, &opts.workshop_updated));
    }
    if let Some(ws) = &loc.workshop_dir {
        cands.extend(candidates_in(ws, Source::Workshop, &gv.major_minor, &opts.workshop_updated));
    }
    let total = cands.len();
    let cached: HashMap<String, (String, String)> = match (opts.use_cache, cache) {
        (true, Some(c)) => c.load_mod_entries()?,
        _ => HashMap::new(),
    };
    let done = AtomicUsize::new(0);
    // (info, files, from_cache, inspected)
    let results: Vec<(ModInfo, ModFiles, bool, bool)> = cands
        .par_iter()
        .map(|c| {
            let uid = c.path.to_string_lossy().to_string();
            let hit = cached.get(&uid).filter(|(stamp, _)| *stamp == c.stamp).and_then(|(_, json)| serde_json::from_str::<CachedEntry>(json).ok());
            let out = match hit {
                Some(e) => (e.info, e.files, true, true),
                None => {
                    let mut info = parse_quick(c, gv);
                    if deep {
                        let ins = inspect_one(&info);
                        info.contents = ins.contents;
                        info.modified = ins.modified;
                        info.kind = classify(&info);
                        (info, ins.files, false, true)
                    } else {
                        (info, ModFiles::default(), false, false)
                    }
                }
            };
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            progress(n, total);
            out
        })
        .collect();

    let mut out = ScanOutput::default();
    let mut to_store: Vec<(String, String, String)> = Vec::new();
    for ((info, files, from_cache, inspected), cand) in results.into_iter().zip(cands.iter()) {
        if from_cache {
            out.from_cache += 1;
        } else {
            out.parsed += 1;
            out.stamps.insert(info.uid.clone(), cand.stamp.clone());
            if inspected {
                if let Ok(json) = serde_json::to_string(&CachedEntry { info: info.clone(), files: files.clone() }) {
                    to_store.push((info.uid.clone(), cand.stamp.clone(), json));
                }
            } else {
                out.shallow.push(info.uid.clone());
            }
        }
        out.files.insert(info.uid.clone(), files);
        out.mods.push(info);
    }
    if let Some(c) = cache {
        if !to_store.is_empty() {
            c.store_mod_entries(&to_store)?;
        }
        let live: Vec<&str> = out.mods.iter().map(|m| m.uid.as_str()).collect();
        c.prune_mod_entries(&live)?;
    }
    // A folder can only be one mod; if two roots overlap (junctions, a Mods folder pointed at
    // the Workshop), keep the first sighting so every uid is unique.
    let mut seen_uids = std::collections::HashSet::new();
    out.mods.retain(|m| seen_uids.insert(m.uid.clone()));
    out.mods.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    tracing::info!(mods = out.mods.len(), from_cache = out.from_cache, parsed = out.parsed, shallow = out.shallow.len(), ms = started.elapsed().as_millis() as u64, "scan");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(p: &Path, s: &str) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, s).unwrap();
    }

    fn fixture_game(dir: &Path) -> Locations {
        write(&dir.join("Version.txt"), "1.6.4530 rev1235");
        write(&dir.join("Data/Core/About/About.xml"), "<ModMetaData><packageId>Ludeon.RimWorld</packageId></ModMetaData>");
        write(&dir.join("Data/Royalty/About/About.xml"), "<ModMetaData><packageId>Ludeon.RimWorld.Royalty</packageId><steamAppId>1149640</steamAppId></ModMetaData>");
        write(
            &dir.join("Mods/Harmony/About/About.xml"),
            "<ModMetaData><packageId>brrainz.harmony</packageId><name>Harmony</name><author>Brrainz</author><supportedVersions><li>1.5</li><li>1.6</li></supportedVersions></ModMetaData>",
        );
        write(&dir.join("Mods/Harmony/About/PublishedFileId.txt"), "2009463077\n");
        write(&dir.join("Mods/Harmony/Current/Assemblies/0Harmony.dll"), "x");
        write(&dir.join("Mods/Harmony/Current/Assemblies/HarmonyMod.dll"), "x");
        write(
            &dir.join("Mods/Walls/About/About.xml"),
            "<ModMetaData><packageId>nyx.retrowalls</packageId><name>Retro Wall Textures</name><supportedVersions><li>1.6</li></supportedVersions></ModMetaData>",
        );
        write(&dir.join("Mods/Walls/Textures/Things/Building/Linked/Wall_Atlas.png"), "png");
        write(&dir.join("Mods/Walls/Textures/Things/Building/Linked/Wall_Atlas.dds"), "dds");
        write(&dir.join("Mods/Walls/LoadFolders.xml"), "<loadFolders><v1.6><li>/</li><li>1.6</li></v1.6></loadFolders>");
        write(&dir.join("Mods/Junk/readme.txt"), "not a mod");
        Locations { game_dir: Some(dir.to_path_buf()), config_dir: None, local_mods_dir: Some(dir.join("Mods")), workshop_dir: None }
    }

    #[test]
    fn scans_and_classifies() {
        let tmp = tempfile::tempdir().unwrap();
        let loc = fixture_game(tmp.path());
        let opts = ScanOptions { locations: loc, game_version: GameVersion::parse("1.6.4530 rev1235").unwrap(), use_cache: false, workshop_updated: HashMap::new() };
        let out = scan(&opts, None, true, &|_, _| {}).unwrap();
        let by_id = |id: &str| out.mods.iter().find(|m| m.package_id == id).unwrap();
        let core = by_id("ludeon.rimworld");
        assert_eq!(core.name, "RimWorld");
        assert_eq!(core.source, Source::Ludeon);
        assert_eq!(core.kind, ModKind::Official);
        let royalty = by_id("ludeon.rimworld.royalty");
        assert_eq!(royalty.steam_app_id, Some(1149640));
        let harmony = by_id("brrainz.harmony");
        assert_eq!(harmony.source, Source::SteamCmd);
        assert_eq!(harmony.published_file_id, Some(2009463077));
        assert_eq!(harmony.contents.assemblies, 2);
        assert!(harmony.contents.bundles_harmony);
        assert_eq!(harmony.kind, ModKind::Code);
        let walls = by_id("nyx.retrowalls");
        assert_eq!(walls.kind, ModKind::Textures);
        assert_eq!(walls.contents.textures, 1);
        assert_eq!(walls.contents.dds, 1);
        assert_eq!(out.files[&walls.uid].textures, vec!["textures/things/building/linked/wall_atlas"]);
        assert_eq!(walls.load_folders.as_ref().unwrap().len(), 2);
        let junk = out.mods.iter().find(|m| m.name == "Junk").unwrap();
        assert!(junk.invalid.is_some());
    }

    #[test]
    fn quick_then_inspect_matches_deep() {
        let tmp = tempfile::tempdir().unwrap();
        let loc = fixture_game(tmp.path());
        let cache = Cache::open(&tmp.path().join("cache.sqlite")).unwrap();
        let opts = ScanOptions { locations: loc, game_version: GameVersion::parse("1.6.4530 rev1235").unwrap(), use_cache: true, workshop_updated: HashMap::new() };
        let mut quick = scan(&opts, Some(&cache), false, &|_, _| {}).unwrap();
        assert_eq!(quick.shallow.len(), quick.mods.len());
        assert!(quick.mods.iter().all(|m| m.contents.assemblies == 0 && matches!(m.kind, ModKind::Unknown | ModKind::Official | ModKind::Scenario)));
        let shallow: Vec<ModInfo> = quick.mods.iter().filter(|m| quick.shallow.contains(&m.uid)).cloned().collect();
        let ins = inspect_mods(&shallow, &|_, _| {});
        let stored = apply_inspections(&mut quick.mods, &mut quick.files, &quick.stamps, ins, Some(&cache)).unwrap();
        assert_eq!(stored, quick.mods.len());
        let deep = scan(&opts, None, true, &|_, _| {}).unwrap();
        assert_eq!(quick.mods, deep.mods);
        assert_eq!(quick.files, deep.files);
        // and the cache now serves everything
        let again = scan(&opts, Some(&cache), false, &|_, _| {}).unwrap();
        assert_eq!(again.from_cache, deep.mods.len());
        assert!(again.shallow.is_empty());
    }

    #[test]
    fn steam_timeupdated_invalidates_workshop_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let mut loc = fixture_game(tmp.path());
        let ws = tmp.path().join("workshop/content/294100");
        write(&ws.join("2009463077/About/About.xml"), "<ModMetaData><packageId>ws.mod</packageId><name>WS</name></ModMetaData>");
        loc.workshop_dir = Some(ws);
        let cache = Cache::open(&tmp.path().join("cache.sqlite")).unwrap();
        let gv = GameVersion::parse("1.6.4530 rev1235").unwrap();
        let mut opts = ScanOptions { locations: loc, game_version: gv, use_cache: true, workshop_updated: HashMap::from([(2009463077u64, 4_000_000_000u64)]) };
        let first = scan(&opts, Some(&cache), true, &|_, _| {}).unwrap();
        let m = first.mods.iter().find(|m| m.package_id == "ws.mod").unwrap();
        assert_eq!(m.source, Source::Workshop);
        assert_eq!(m.published_file_id, Some(2009463077));
        assert_eq!(m.modified, 4_000_000_000, "Steam's timeupdated is newer than the folder");
        let second = scan(&opts, Some(&cache), true, &|_, _| {}).unwrap();
        assert_eq!(second.from_cache, second.mods.len());
        // Steam updated the item in place: nothing on disk changed except the ACF.
        opts.workshop_updated.insert(2009463077, 4_100_000_000);
        let third = scan(&opts, Some(&cache), true, &|_, _| {}).unwrap();
        assert_eq!(third.parsed, 1, "only the updated item is re-read");
        assert_eq!(third.mods.iter().find(|m| m.package_id == "ws.mod").unwrap().modified, 4_100_000_000);
    }

    #[test]
    fn cache_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let loc = fixture_game(tmp.path());
        let cache = Cache::open(&tmp.path().join("cache.sqlite")).unwrap();
        let opts = ScanOptions { locations: loc, game_version: GameVersion::parse("1.6.4530 rev1235").unwrap(), use_cache: true, workshop_updated: HashMap::new() };
        let first = scan(&opts, Some(&cache), true, &|_, _| {}).unwrap();
        assert_eq!(first.from_cache, 0);
        let second = scan(&opts, Some(&cache), true, &|_, _| {}).unwrap();
        assert_eq!(second.from_cache, first.mods.len());
        assert_eq!(first.mods, second.mods);
        assert_eq!(first.files, second.files);
    }
}
