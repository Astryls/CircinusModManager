//! Application state: everything the UI sees, and the operations that change it.
//! No Tauri types here so it stays testable.

use circinus_core::cache::Cache;
use circinus_core::game::GameVersion;
use circinus_core::import::ImportedList;
use circinus_core::model::*;
use circinus_core::modsconfig::{self, ModsConfig};
use circinus_core::order::{self, Context, UserOverrides};
use circinus_core::paths::{app_data_dir, Locations};
use circinus_core::rules::{self, Databases, DbSource, RulesFile};
use circinus_core::scan::{self, Inspection, ModFiles, ScanOptions};
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
}

impl Default for Settings {
    fn default() -> Self {
        Settings { locations: Locations::default(), db_sources: rules::default_sources(), show_weight: false, include_local_runs: true, alphabetical_within_phase: false, update_databases_on_start: false }
    }
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
    pub dirty: bool,
    pub db_loaded: Vec<String>,
    pub scanned_at: i64,
    /// Mods whose folders are still being inspected in the background.
    pub inspecting: usize,
    /// Texture-collision issues left out of `issues` to keep the payload small (0 = none).
    pub issues_truncated: usize,
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
    pub dirty: bool,
    pub scanned_at: i64,
    /// uids not yet inspected, and the cache stamps to write once they are.
    pub shallow: Vec<String>,
    pub pending_stamps: HashMap<String, String>,
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

impl App {
    pub fn open() -> Result<App> {
        let data_dir = app_data_dir();
        std::fs::create_dir_all(data_dir.join("dbs"))?;
        let cache = Cache::open(&data_dir.join("cache.sqlite"))?;
        let settings: Settings = cache.get("settings")?.unwrap_or_default();
        let mut user: UserData = cache.get("user")?.unwrap_or_default();
        if user.groups.is_empty() {
            user.groups = default_groups();
        }
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
            dirty: false,
            scanned_at: 0,
            shallow: Vec::new(),
            pending_stamps: HashMap::new(),
        };
        app.resolve_locations();
        app.load_databases();
        app.load_cached_weights()?;
        Ok(app)
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
        let opts = ScanOptions { locations: self.locations.clone(), game_version: self.game_version.clone(), use_cache: !full };
        let out = scan::scan(&opts, Some(&self.cache), false, progress)?;
        self.mods = out.mods;
        self.files = out.files;
        self.shallow = out.shallow;
        self.pending_stamps = out.stamps;
        self.scanned_at = now();
        self.recompile();
        self.read_mods_config();
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
        Ok(n)
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
        self.previous_known = cfg.known_expansions;
        self.dirty = false;
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
            dirty: self.dirty,
            db_loaded: self.db.loaded.clone(),
            scanned_at: self.scanned_at,
            inspecting: self.shallow.len(),
            issues_truncated,
        }
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
        let all: HashMap<String, (Weight, i64)> = self.cache.weights_all()?;
        let mut newest = 0;
        for (id, (w, at)) in all {
            newest = newest.max(at);
            self.weights.insert(id, w);
        }
        self.weights_fetched_at = newest;
        Ok(())
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
