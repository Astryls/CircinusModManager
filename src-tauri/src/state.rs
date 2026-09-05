//! Application state: everything the UI sees, and the operations that change it.
//! No Tauri types here so it stays testable.

use circinus_core::cache::Cache;
use circinus_core::changes::{self, Baseline, ListChange, ModChange};
use circinus_core::dds;
use circinus_core::game::GameVersion;
use circinus_core::import::ImportedList;
use circinus_core::model::*;
use circinus_core::modsconfig::{self, ModsConfig, SavedList};
use circinus_core::order::{self, Context, UserOverrides};
use circinus_core::paths::{app_data_dir, Locations};
use circinus_core::rules::{self, Databases, DbSource, RulesFile};
use circinus_core::scan::{self, Inspection, ModFiles, ScanOptions, Unreadable};
use circinus_core::steam::webapi::WorkshopItem;
use circinus_core::weight::{self, Weight};
use circinus_core::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// User overrides; None fields fall back to autodetection.
    pub locations: Locations,
    pub db_sources: Vec<DbSource>,
    pub show_weight: bool,
    pub include_local_runs: bool,
    pub alphabetical_within_phase: bool,
    pub update_databases_on_start: bool,
    pub dds: DdsSettings,
    pub launch: LaunchSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { locations: Locations::default(), db_sources: rules::default_sources(), show_weight: false, include_local_runs: true, alphabetical_within_phase: false, update_databases_on_start: false, dds: DdsSettings::default(), launch: LaunchSettings::default() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LaunchMethod {
    /// Steam when the game lives in a Steam library, the executable otherwise.
    #[default]
    Auto,
    /// `steam://rungameid/294100` — the overlay and Workshop sync behave as usual.
    Steam,
    /// Start the game's executable directly (GOG, DRM-free, a copy outside Steam).
    Executable,
}

/// How the Play button starts the game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LaunchSettings {
    pub method: LaunchMethod,
    /// Explicit executable; None = detect from the game folder.
    pub executable: Option<PathBuf>,
    /// Extra command-line arguments, e.g. `-popupwindow`.
    pub args: String,
    /// Write ModsConfig.xml first when there are unsaved changes.
    pub save_first: bool,
}

impl Default for LaunchSettings {
    fn default() -> Self {
        LaunchSettings { method: LaunchMethod::Auto, executable: None, args: String::new(), save_first: true }
    }
}

/// Texture optimisation preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DdsSettings {
    /// Format for textures with alpha; opaque ones are always BC1.
    pub alpha_format: dds::Format,
    pub quality: dds::Quality,
    pub mipmaps: bool,
    /// 0 = all cores but one.
    pub threads: usize,
    /// Convert new and updated mods on their own once anything has been converted.
    pub auto: bool,
}

impl Default for DdsSettings {
    fn default() -> Self {
        DdsSettings { alpha_format: dds::Format::Bc7, quality: dds::Quality::Balanced, mipmaps: true, threads: 0, auto: false }
    }
}

/// What the manifest says about one mod, kept in memory for the UI.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DdsSummary {
    pub count: usize,
    pub dds_bytes: u64,
    pub png_bytes: u64,
    /// Bytes the GPU would hold for these textures uncompressed (RGBA8 with mips).
    pub vram_before: u64,
    pub newest: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: String,
    pub name: String,
    pub color: String,
    /// Phase every member is placed in (unless the member has its own override).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<Phase>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UserData {
    pub groups: Vec<Group>,
    pub mod_groups: HashMap<String, String>,
    pub pinned: HashSet<String>,
    pub phase_overrides: HashMap<String, Phase>,
    pub notes: HashMap<String, String>,
    pub muted: HashSet<String>,
    /// Mods whose textures must not be converted.
    pub dds_excluded: HashSet<String>,
}

fn default_groups() -> Vec<Group> {
    vec![
        Group { id: "core".into(), name: "Core".into(), color: "blue".into(), phase: None },
        Group { id: "frameworks".into(), name: "Frameworks".into(), color: "teal".into(), phase: None },
        Group { id: "qol".into(), name: "Quality of life".into(), color: "green".into(), phase: None },
        Group { id: "visual".into(), name: "Visual".into(), color: "amber".into(), phase: None },
        Group { id: "performance".into(), name: "Performance".into(), color: "coral".into(), phase: Some(Phase::Optimization) },
    ]
}

/// Everything the UI needs to render, in one message.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub locations: Locations,
    pub game_version: GameVersion,
    pub mods: Vec<ModInfo>,
    pub active: Vec<String>,
    pub missing: Vec<String>,
    pub issues: Vec<Issue>,
    pub placements: Vec<Placement>,
    pub rules: Vec<Rule>,
    pub user: UserData,
    pub settings: Settings,
    pub weights: HashMap<String, Weight>,
    pub weights_fetched_at: i64,
    /// One raw record as circinus.sh sent it, for checking the field names when figures look off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weights_sample: Option<String>,
    pub dirty: bool,
    pub db_loaded: Vec<String>,
    pub scanned_at: i64,
    /// Mods whose folders are still being inspected in the background.
    pub inspecting: usize,
    /// Entries in a mod folder the scan had to leave out, with the reason.
    #[serde(default)]
    pub unreadable: Vec<Unreadable>,
    /// Texture-collision issues left out of `issues` to keep the payload small (0 = none).
    pub issues_truncated: usize,
    /// Installed workshop mods with a newer version on the Workshop (from the last check).
    pub updates: Vec<UpdateInfo>,
    pub updates_checked_at: i64,
    /// Mods that appeared, disappeared or changed since the previous session (or since the
    /// user last acknowledged the list).
    pub changes: Vec<ModChange>,
    /// Edits to ModsConfig.xml made outside Circinus since then.
    pub list_change: Option<ListChange>,
    /// Unix seconds of the baseline the changes are measured from (0 = first run).
    pub changes_since: i64,
    /// uid → what Circinus has converted for it.
    pub dds: HashMap<String, DdsSummary>,
    /// Set when ModsConfig.xml holds only official content although a real list was there
    /// before: RimWorld failed to load and reset it.
    pub list_reset: Option<ListReset>,
}

/// RimWorld gave up loading and wrote a Core-only list; here is what to put back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListReset {
    /// Mods in the list before the reset.
    pub previous_count: usize,
    /// The newest archived copy of a real list, if any.
    pub restore_from: Option<SavedList>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub uid: String,
    pub published_file_id: u64,
    pub name: String,
    pub local_modified: u64,
    pub remote_updated: u64,
    pub source: Source,
}

/// Texture collisions kept in a snapshot; the rest are available through the analyzer commands.
const MAX_COLLISIONS: usize = 3000;

pub struct App {
    pub data_dir: PathBuf,
    pub cache: Cache,
    pub settings: Settings,
    pub user: UserData,
    pub locations: Locations,
    pub game_version: GameVersion,
    pub mods: Vec<ModInfo>,
    pub files: HashMap<String, ModFiles>,
    pub active: Vec<String>,
    pub missing: Vec<String>,
    pub previous_known: Vec<String>,
    pub db: Databases,
    pub rules: Vec<Rule>,
    pub weights: HashMap<String, Weight>,
    pub weights_fetched_at: i64,
    pub weights_sample: Option<String>,
    pub dirty: bool,
    pub scanned_at: i64,
    /// uids not yet inspected, and the cache stamps to write once they are.
    pub shallow: Vec<String>,
    pub pending_stamps: HashMap<String, String>,
    /// Mod folder entries the last scan could not read (links to nowhere, mostly).
    pub unreadable: Vec<Unreadable>,
    pub updates: Vec<UpdateInfo>,
    pub updates_checked_at: i64,
    /// What the previous session last saw; `changes` is the diff against it.
    pub baseline: Option<Baseline>,
    pub changes: Vec<ModChange>,
    pub list_change: Option<ListChange>,
    /// Steam's `timeupdated` per installed Workshop item, read with every scan.
    pub workshop_updated: HashMap<u64, u64>,
    /// The active list exactly as ModsConfig.xml has it (package ids, lowercase).
    pub file_active: Vec<String>,
    /// uid → converted-texture summary, from the manifest.
    pub dds_index: HashMap<String, DdsSummary>,
    pub list_reset: Option<ListReset>,
}

/// How many archived lists to keep.
const LIST_HISTORY: usize = 40;

const BASELINE_KEY: &str = "mod_baseline";

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

impl App {
    pub fn open() -> Result<App> {
        App::open_at(app_data_dir(), None)
    }

    /// Open with an explicit data folder and (for tests and tools) explicit settings.
    pub fn open_at(data_dir: PathBuf, settings_override: Option<Settings>) -> Result<App> {
        std::fs::create_dir_all(data_dir.join("dbs"))?;
        let cache = Cache::open(&data_dir.join("cache.sqlite"))?;
        let settings: Settings = match settings_override {
            Some(s) => s,
            None => cache.get("settings")?.unwrap_or_default(),
        };
        let mut user: UserData = cache.get("user")?.unwrap_or_default();
        if user.groups.is_empty() {
            user.groups = default_groups();
        }
        let baseline: Option<Baseline> = cache.get(BASELINE_KEY).unwrap_or(None);
        let mut app = App {
            data_dir,
            cache,
            settings,
            user,
            locations: Locations::default(),
            game_version: GameVersion::fallback("1.6"),
            mods: Vec::new(),
            files: HashMap::new(),
            active: Vec::new(),
            missing: Vec::new(),
            previous_known: Vec::new(),
            db: Databases::default(),
            rules: Vec::new(),
            weights: HashMap::new(),
            weights_fetched_at: 0,
            weights_sample: None,
            dirty: false,
            scanned_at: 0,
            shallow: Vec::new(),
            pending_stamps: HashMap::new(),
            unreadable: Vec::new(),
            updates: Vec::new(),
            updates_checked_at: 0,
            baseline,
            changes: Vec::new(),
            list_change: None,
            workshop_updated: HashMap::new(),
            file_active: Vec::new(),
            dds_index: HashMap::new(),
            list_reset: None,
        };
        app.resolve_locations();
        app.load_databases();
        app.load_cached_weights()?;
        app.reload_dds_index();
        Ok(app)
    }

    /// Rebuild the per-mod summary from the manifest table.
    pub fn reload_dds_index(&mut self) {
        let all: HashMap<String, Vec<dds::Entry>> = self.cache.dds_all().unwrap_or_default();
        self.dds_index = all
            .into_iter()
            .map(|(uid, entries)| {
                let s = DdsSummary {
                    count: entries.len(),
                    dds_bytes: entries.iter().map(|e| e.dds_len).sum(),
                    png_bytes: entries.iter().map(|e| e.src_len).sum(),
                    vram_before: entries.iter().map(|e| (e.width as u64 * e.height as u64 * 4) * 4 / 3).sum(),
                    newest: entries.iter().map(|e| e.created_at).max().unwrap_or(0),
                };
                (uid, s)
            })
            .collect();
    }

    /// Autodetect, then apply user overrides on top.
    pub fn resolve_locations(&mut self) {
        let mut loc = Locations::detect();
        let o = &self.settings.locations;
        if o.game_dir.is_some() {
            loc.game_dir = o.game_dir.clone();
            loc.local_mods_dir = None;
        }
        if o.config_dir.is_some() {
            loc.config_dir = o.config_dir.clone();
        }
        if o.local_mods_dir.is_some() {
            loc.local_mods_dir = o.local_mods_dir.clone();
        }
        if o.workshop_dir.is_some() {
            loc.workshop_dir = o.workshop_dir.clone();
        }
        loc.fill_derived();
        self.game_version = loc.game_dir.as_ref().and_then(|g| GameVersion::read(g)).unwrap_or_else(|| GameVersion::fallback("1.6"));
        self.locations = loc;
    }

    pub fn load_databases(&mut self) {
        let user_rules = rules::user_rules_path(&self.data_dir);
        self.db = Databases::load(&self.data_dir.join("dbs"), &user_rules, &self.game_version.major_minor);
        self.recompile();
    }

    fn recompile(&mut self) {
        self.rules = rules::compile_rules(&self.mods, &self.db);
    }

    fn overrides(&self) -> UserOverrides {
        let mut phases: HashMap<String, Phase> = HashMap::new();
        for (uid, gid) in &self.user.mod_groups {
            if let Some(p) = self.user.groups.iter().find(|g| &g.id == gid).and_then(|g| g.phase) {
                phases.insert(uid.clone(), p);
            }
        }
        for (uid, p) in &self.user.phase_overrides {
            phases.insert(uid.clone(), *p);
        }
        UserOverrides { phases, pinned: self.user.pinned.clone(), alphabetical: self.settings.alphabetical_within_phase }
    }

    fn with_context<T>(&self, f: impl FnOnce(&Context) -> T) -> T {
        let ov = self.overrides();
        let ctx = Context { mods: &self.mods, files: &self.files, rules: &self.rules, db: &self.db, major_minor: &self.game_version.major_minor, overrides: &ov };
        f(&ctx)
    }

    /// Phase 1: read every mod's About/ folder (fast) and resolve ModsConfig.xml. Returns the
    /// mods whose folders still need walking; hand them to `scan::inspect_mods` outside the lock,
    /// then call `apply_inspections`.
    pub fn scan_quick(&mut self, full: bool, progress: &(dyn Fn(usize, usize) + Sync)) -> Result<Vec<ModInfo>> {
        if full {
            self.cache.clear_mod_cache()?;
        }
        self.workshop_updated = self.locations.workshop_updated();
        let opts = ScanOptions { locations: self.locations.clone(), game_version: self.game_version.clone(), use_cache: !full, workshop_updated: self.workshop_updated.clone() };
        let out = scan::scan(&opts, Some(&self.cache), false, progress)?;
        self.mods = out.mods;
        self.files = out.files;
        self.shallow = out.shallow;
        self.pending_stamps = out.stamps;
        self.unreadable = out.unreadable;
        self.scanned_at = now();
        self.recompile();
        self.read_mods_config();
        self.refresh_changes();
        self.store_baseline();
        let shallow: Vec<ModInfo> = self.mods.iter().filter(|m| self.shallow.contains(&m.uid)).cloned().collect();
        Ok(shallow)
    }

    /// Phase 2: merge folder inspections and write them to the cache.
    pub fn apply_inspections(&mut self, inspections: Vec<Inspection>) -> Result<usize> {
        let done: HashSet<String> = inspections.iter().map(|i| i.uid.clone()).collect();
        let n = scan::apply_inspections(&mut self.mods, &mut self.files, &self.pending_stamps, inspections, Some(&self.cache))?;
        self.shallow.retain(|u| !done.contains(u));
        for u in &done {
            self.pending_stamps.remove(u);
        }
        self.refresh_changes();
        self.store_baseline();
        Ok(n)
    }

    /// Diff the install against the baseline the session started with.
    fn refresh_changes(&mut self) {
        match &self.baseline {
            Some(b) => {
                let active: HashSet<String> = self.active.iter().cloned().collect();
                let (c, l) = changes::diff(b, &self.mods, &self.workshop_updated, &active, Some(&self.file_active));
                self.changes = c;
                self.list_change = l;
            }
            None => {
                self.changes.clear();
                self.list_change = None;
            }
        }
    }

    /// Remember the current install for the next launch. Becomes the session baseline too on
    /// the very first run, once the folders have been fully inspected (quick-phase mtimes
    /// would otherwise read as changes a moment later).
    fn store_baseline(&mut self) {
        let b = Baseline::take(&self.mods, &self.workshop_updated, &self.file_active, now());
        if let Err(e) = self.cache.set(BASELINE_KEY, &b) {
            tracing::warn!("could not store the mod baseline: {e}");
        }
        if self.baseline.is_none() && self.shallow.is_empty() {
            self.baseline = Some(b);
        }
    }

    /// The user has seen the changes: measure from now on.
    pub fn acknowledge_changes(&mut self) {
        let b = Baseline::take(&self.mods, &self.workshop_updated, &self.file_active, now());
        if let Err(e) = self.cache.set(BASELINE_KEY, &b) {
            tracing::warn!("could not store the mod baseline: {e}");
        }
        self.baseline = Some(b);
        self.changes.clear();
        self.list_change = None;
    }

    /// Read ModsConfig.xml and resolve it against installed mods (keeps unsaved edits if dirty).
    pub fn read_mods_config(&mut self) {
        if self.dirty {
            let existing = self.active.clone();
            let live: HashSet<&str> = self.mods.iter().map(|m| m.uid.as_str()).collect();
            self.active = existing.into_iter().filter(|u| live.contains(u.as_str())).collect();
            return;
        }
        let cfg = self.locations.mods_config_path().and_then(|p| modsconfig::read(&p).ok()).unwrap_or_else(|| ModsConfig { version: self.game_version.full.clone(), active_mods: vec![CORE_PACKAGE_ID.into()], known_expansions: vec![] });
        let (uids, missing) = modsconfig::resolve_active(&cfg.active_mods, &self.mods);
        self.active = uids;
        self.missing = missing;
        self.file_active = cfg.active_mods.iter().map(|p| p.to_lowercase()).collect();
        self.previous_known = cfg.known_expansions.clone();
        self.dirty = false;
        // Keep every real list we come across, whoever wrote it — and notice RimWorld's
        // "resetting mods config" recovery, which leaves only official content behind.
        if modsconfig::looks_reset(&cfg.active_mods) {
            // The list this session started with is a copy too: archive it before it is lost.
            if let Some(b) = &self.baseline {
                if b.active.len() > cfg.active_mods.len() && !modsconfig::looks_reset(&b.active) {
                    let prev = ModsConfig { version: self.game_version.full.clone(), active_mods: b.active.clone(), known_expansions: cfg.known_expansions.clone() };
                    let _ = modsconfig::archive(&self.lists_dir(), &prev, "before-reset", LIST_HISTORY);
                }
            }
            let history: Vec<SavedList> = modsconfig::list_archive(&self.lists_dir()).into_iter().filter(|l| l.count > cfg.active_mods.len()).collect();
            let previous_count = self.baseline.as_ref().map(|b| b.active.len()).filter(|n| *n > cfg.active_mods.len()).or_else(|| history.first().map(|l| l.count)).unwrap_or(0);
            if previous_count > cfg.active_mods.len() {
                self.list_reset = Some(ListReset { previous_count, restore_from: history.into_iter().next() });
            }
        } else {
            self.list_reset = None;
            if let Err(e) = modsconfig::archive(&self.lists_dir(), &cfg, "seen", LIST_HISTORY) {
                tracing::warn!("could not archive the list: {e}");
            }
        }
    }

    pub fn lists_dir(&self) -> PathBuf {
        self.data_dir.join("lists")
    }

    /// Archived lists, newest first.
    pub fn saved_lists(&self) -> Vec<SavedList> {
        modsconfig::list_archive(&self.lists_dir())
    }

    /// Make an archived list the active list (unsaved until `save`).
    pub fn restore_list(&mut self, path: &std::path::Path) -> Result<(usize, Vec<String>)> {
        let cfg = modsconfig::read(path)?;
        let (uids, missing) = modsconfig::resolve_active(&cfg.active_mods, &self.mods);
        let n = uids.len();
        self.set_active(uids);
        self.list_reset = None;
        Ok((n, missing))
    }

    pub fn issues(&self) -> Vec<Issue> {
        self.with_context(|ctx| order::validate(&self.active, ctx))
    }

    pub fn placements(&self) -> Vec<Placement> {
        let by_uid: HashMap<&str, &ModInfo> = self.mods.iter().map(|m| (m.uid.as_str(), m)).collect();
        let order: Vec<&ModInfo> = self.active.iter().filter_map(|u| by_uid.get(u.as_str()).copied()).collect();
        self.with_context(|ctx| order::placements(&order, ctx))
    }

    pub fn snapshot(&self) -> Snapshot {
        let started = std::time::Instant::now();
        // Descriptions are the bulk of the payload and only one is ever shown at a time:
        // the UI fetches them with `get_description`.
        let mods: Vec<ModInfo> = self.mods.iter().map(|m| ModInfo { description: String::new(), ..m.clone() }).collect();
        let mut issues = self.issues();
        let collisions = issues.iter().filter(|i| matches!(i, Issue::TextureCollision { .. })).count();
        let mut issues_truncated = 0;
        if collisions > MAX_COLLISIONS {
            let mut kept = 0;
            issues.retain(|i| {
                if matches!(i, Issue::TextureCollision { .. }) {
                    kept += 1;
                    kept <= MAX_COLLISIONS
                } else {
                    true
                }
            });
            issues_truncated = collisions - MAX_COLLISIONS;
        }
        let placements = self.placements();
        tracing::info!(mods = mods.len(), active = self.active.len(), issues = issues.len(), rules = self.rules.len(), ms = started.elapsed().as_millis() as u64, "snapshot");
        Snapshot {
            locations: self.locations.clone(),
            game_version: self.game_version.clone(),
            mods,
            active: self.active.clone(),
            missing: self.missing.clone(),
            issues,
            placements,
            rules: self.rules.clone(),
            user: self.user.clone(),
            settings: self.settings.clone(),
            weights: self.weights.clone(),
            weights_fetched_at: self.weights_fetched_at,
            weights_sample: self.weights_sample.clone(),
            dirty: self.dirty,
            db_loaded: self.db.loaded.clone(),
            scanned_at: self.scanned_at,
            inspecting: self.shallow.len(),
            unreadable: self.unreadable.clone(),
            issues_truncated,
            updates: self.updates.clone(),
            updates_checked_at: self.updates_checked_at,
            changes: self.changes.clone(),
            list_change: self.list_change.clone(),
            changes_since: self.baseline.as_ref().map(|b| b.taken_at).unwrap_or(0),
            dds: self.dds_index.clone(),
            list_reset: self.list_reset.clone(),
        }
    }

    /// Workshop ids of every installed mod that came from the Workshop or SteamCMD.
    pub fn workshop_ids(&self) -> Vec<(String, u64)> {
        self.mods.iter().filter(|m| m.invalid.is_none()).filter_map(|m| m.published_file_id.map(|id| (m.uid.clone(), id))).collect()
    }

    /// Compare Workshop `time_updated` with what is on disk.
    pub fn apply_update_check(&mut self, items: &[WorkshopItem]) -> usize {
        let by_id: HashMap<u64, &WorkshopItem> = items.iter().map(|i| (i.published_file_id, i)).collect();
        let mut out = Vec::new();
        for m in self.mods.iter().filter(|m| m.invalid.is_none()) {
            let Some(id) = m.published_file_id else { continue };
            let Some(item) = by_id.get(&id) else { continue };
            if item.time_updated > m.modified + 60 {
                out.push(UpdateInfo { uid: m.uid.clone(), published_file_id: id, name: m.name.clone(), local_modified: m.modified, remote_updated: item.time_updated, source: m.source });
            }
        }
        out.sort_by(|a, b| b.remote_updated.cmp(&a.remote_updated));
        self.updates = out;
        self.updates_checked_at = now();
        self.updates.len()
    }

    /// Resolve package ids that are missing from the install to workshop ids via the Steam DB.
    pub fn workshop_ids_for_missing(&self) -> (Vec<u64>, Vec<String>) {
        let mut ids = Vec::new();
        let mut unresolved = Vec::new();
        for pkg in &self.missing {
            let base = pkg.trim_end_matches("_steam");
            match self.db.steam.workshop_ids_for(base).first() {
                Some(id) => ids.push(*id),
                None => unresolved.push(pkg.clone()),
            }
        }
        (ids, unresolved)
    }

    pub fn description(&self, uid: &str) -> String {
        self.mods.iter().find(|m| m.uid == uid).map(|m| m.description.clone()).unwrap_or_default()
    }

    pub fn set_active(&mut self, uids: Vec<String>) {
        let live: HashSet<&str> = self.mods.iter().map(|m| m.uid.as_str()).collect();
        let mut seen = HashSet::new();
        self.active = uids.into_iter().filter(|u| live.contains(u.as_str()) && seen.insert(u.clone())).collect();
        self.dirty = true;
    }

    pub fn activate(&mut self, uids: &[String], at: Option<usize>) {
        let mut order = self.active.clone();
        let new: Vec<String> = uids.iter().filter(|u| !order.contains(u)).cloned().collect();
        match at {
            Some(i) if i <= order.len() => {
                let tail = order.split_off(i);
                order.extend(new);
                order.extend(tail);
            }
            _ => order.extend(new),
        }
        self.set_active(order);
    }

    pub fn deactivate(&mut self, uids: &[String]) {
        let drop: HashSet<&String> = uids.iter().collect();
        let order: Vec<String> = self.active.iter().filter(|u| !drop.contains(u)).cloned().collect();
        self.set_active(order);
    }

    pub fn halo(&mut self, apply: bool) -> SortResult {
        let result = self.with_context(|ctx| order::sort(&self.active, ctx));
        if apply && result.order != self.active {
            self.active = result.order.clone();
            self.dirty = true;
        }
        result
    }

    pub fn save(&mut self) -> Result<PathBuf> {
        let path = self.locations.mods_config_path().ok_or_else(|| circinus_core::Error::Other("RimWorld's config folder is not set. Choose it in Settings.".into()))?;
        let cfg = modsconfig::build(&self.active, &self.mods, &self.game_version.full, &self.previous_known);
        modsconfig::write(&path, &cfg)?;
        self.dirty = false;
        self.list_reset = None;
        if let Err(e) = modsconfig::archive(&self.lists_dir(), &cfg, "saved", LIST_HISTORY) {
            tracing::warn!("could not archive the list: {e}");
        }
        // Our own edit is not "a change made outside Circinus".
        self.file_active = cfg.active_mods.iter().map(|p| p.to_lowercase()).collect();
        if let Some(b) = &mut self.baseline {
            b.active = self.file_active.clone();
        }
        self.refresh_changes();
        self.store_baseline();
        Ok(path)
    }

    pub fn resolve_import(&self, list: &ImportedList) -> (Vec<String>, Vec<String>) {
        modsconfig::resolve_active(&list.package_ids, &self.mods)
    }

    pub fn persist(&self) -> Result<()> {
        self.cache.set("settings", &self.settings)?;
        self.cache.set("user", &self.user)?;
        Ok(())
    }

    pub fn write_user_rules(&mut self, file: &RulesFile) -> Result<()> {
        let path = rules::user_rules_path(&self.data_dir);
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        let mut f = file.clone();
        f.timestamp = now();
        std::fs::write(&path, serde_json::to_string_pretty(&rules::rules_to_json(&f))?)?;
        self.db.user = f;
        self.recompile();
        Ok(())
    }

    fn load_cached_weights(&mut self) -> Result<()> {
        self.weights_sample = self.cache.get("weights_sample").unwrap_or(None);
        let all: HashMap<String, (Weight, i64)> = self.cache.weights_all()?;
        let mut newest = 0;
        for (id, (w, at)) in all {
            newest = newest.max(at);
            self.weights.insert(id, w);
        }
        self.weights_fetched_at = newest;
        Ok(())
    }

    /// Remember what one record from the API looks like.
    pub fn store_weights_sample(&mut self, sample: Option<serde_json::Value>) {
        let text = sample.map(|v| serde_json::to_string_pretty(&v).unwrap_or_default()).map(|t| if t.len() > 6000 { format!("{}\n...", &t[..6000]) } else { t });
        if let Some(t) = &text {
            let _ = self.cache.set("weights_sample", t);
        }
        self.weights_sample = text;
    }

    /// Merge freshly fetched API weights (and local runs) into the cache.
    pub fn store_weights(&mut self, fetched: Vec<Weight>) -> Result<usize> {
        let at = now();
        let entries: Vec<(String, Weight)> = fetched.into_iter().map(|w| (w.package_id.clone(), w)).collect();
        self.cache.weights_store(&entries, at)?;
        for (id, w) in entries.iter() {
            self.weights.insert(id.clone(), w.clone());
        }
        self.weights_fetched_at = at;
        Ok(entries.len())
    }

    pub fn merge_local_weights(&mut self) -> usize {
        let Some(cfg) = &self.locations.config_dir else { return 0 };
        let dir = weight::local_runs_dir(cfg);
        let local = weight::read_local_runs(&dir).unwrap_or_default();
        let mut n = 0;
        for (id, w) in local {
            // Local figures only fill gaps; published, ranked figures stay authoritative.
            let keep_api = self.weights.get(&id).map(|x| x.origin == "api" && x.share.is_some()).unwrap_or(false);
            if !keep_api {
                self.weights.insert(id, w);
                n += 1;
            }
        }
        n
    }
}
