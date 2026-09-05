//! Instances: a named set of folders — game, config, local mods, Workshop — with its own
//! launch settings and its own named lists. Switching instance re-points Circinus at another
//! install (or another config folder beside the same install) and reads it again.
//!
//! An instance owns its named lists (`<data>/lists/named/<instance id>/`) and remembers which
//! of them is being worked on. That is the whole relationship between the two: instances are
//! where the mods live, lists are which of them are active. There is no second profile system.

use crate::state::{now_secs, App, LaunchSettings, Settings};
use circinus_core::cache::Cache;
use circinus_core::paths::Locations;
use circinus_core::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The key/value row in the cache that holds every instance.
const INSTANCES_KEY: &str = "instances";
/// Bumped when the shape of `Store` changes; `load` migrates anything older.
pub const INSTANCES_VERSION: u32 = 1;
/// The instance an existing install becomes when instances first appear.
pub const DEFAULT_ID: &str = "default";

/// One install as the user thinks of it: "1.6 vanilla-ish", "CE playthrough", "modding sandbox".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    pub id: String,
    pub name: String,
    /// The user's folder overrides, exactly as `Settings::locations` means them: a None field
    /// falls back to autodetection.
    pub locations: Locations,
    pub launch: LaunchSettings,
    /// Unix seconds.
    pub created_at: i64,
}

/// Every instance, and which one is open.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Store {
    pub version: u32,
    pub instances: Vec<Instance>,
    pub current: String,
}

impl Default for Store {
    fn default() -> Self {
        Store { version: INSTANCES_VERSION, instances: Vec::new(), current: DEFAULT_ID.to_string() }
    }
}

impl Store {
    pub fn get(&self, id: &str) -> Option<&Instance> {
        self.instances.iter().find(|i| i.id == id)
    }
    pub fn current(&self) -> Option<&Instance> {
        self.get(&self.current)
    }
}

/// Where an instance's named lists live. Each instance has its own folder so two instances can
/// hold lists of the same name without meeting.
pub fn named_dir_for(data_dir: &Path, id: &str) -> PathBuf {
    data_dir.join("lists").join("named").join(id)
}

/// The named list being worked on, remembered per instance.
pub fn current_list_key(id: &str) -> String {
    format!("current_list:{id}")
}

/// What "changed since last time" measures from, per instance.
///
/// The baseline is scoped to the instance although the mod cache is not, and the two are right
/// for different reasons. The cache is keyed by folder path, so two instances sharing a Workshop
/// folder share the parse of every mod in it — that is free and correct. The baseline is a
/// snapshot of *which* mods were installed and which were active; switching instance changes
/// both wholesale, so one shared baseline would report every mod of the other instance as added
/// or removed on every switch. The DDS manifest stays unscoped for the cache's reason: it records
/// the textures Circinus converted on disk, keyed by mod folder, and those files are shared by
/// whatever instance points at that folder. Scoping it would let one instance believe textures
/// are unconverted while they sit converted next to the PNGs.
pub fn baseline_key(id: &str) -> String {
    format!("mod_baseline:{id}")
}

/// A file-system- and JSON-safe id from a name, with a short suffix so two instances may share
/// a name without sharing a folder.
fn make_id(name: &str, taken: &[Instance]) -> String {
    let slug: String = name.trim().to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let slug = slug.trim_matches('-').replace("--", "-");
    let slug = if slug.is_empty() { "instance".to_string() } else { slug.chars().take(40).collect() };
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    for n in 0..64u32 {
        let id = format!("{slug}-{:x}", nanos.wrapping_add(n * 7919) % 0xfff_fff);
        if !taken.iter().any(|i| i.id == id) {
            return id;
        }
    }
    format!("{slug}-{}", now_secs())
}

fn default_instance(settings: &Settings) -> Instance {
    Instance { id: DEFAULT_ID.to_string(), name: "Default".to_string(), locations: settings.locations.clone(), launch: settings.launch.clone(), created_at: now_secs() }
}

/// Copy a key/value row to another key, leaving the old one alone (it costs nothing and an
/// older Circinus opened on the same folder still finds what it wrote).
fn carry(cache: &Cache, from: &str, to: &str) {
    if let Ok(Some(v)) = cache.get::<serde_json::Value>(from) {
        if let Err(e) = cache.set(to, &v) {
            tracing::warn!("could not carry {from} over to {to}: {e}");
        }
    }
}

/// Move the flat `lists/named/*.xml` folder into the default instance's own.
fn move_named_lists(data_dir: &Path, id: &str) -> Result<usize> {
    let flat = data_dir.join("lists").join("named");
    let dest = named_dir_for(data_dir, id);
    let Ok(rd) = std::fs::read_dir(&flat) else { return Ok(0) };
    let files: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_file() && p.extension().map(|x| x == "xml").unwrap_or(false)).collect();
    if files.is_empty() {
        return Ok(0);
    }
    std::fs::create_dir_all(&dest)?;
    let mut moved = 0;
    for p in files {
        let Some(name) = p.file_name() else { continue };
        // A rename within one folder tree; a copy is the fallback for the odd file system that
        // refuses one. Either way the list keeps its name, so nothing the user sees changes.
        if std::fs::rename(&p, dest.join(name)).is_err() {
            std::fs::copy(&p, dest.join(name))?;
            let _ = std::fs::remove_file(&p);
        }
        moved += 1;
    }
    Ok(moved)
}

/// Read the instances, making them from the settings the user already has on the first run.
///
/// Nothing is lost by the migration: the install that exists becomes the Default instance with
/// the same folders, its named lists move into that instance's folder under the same names, and
/// the current list and the change baseline are carried over to the instance-scoped keys.
pub fn load(cache: &Cache, data_dir: &Path, settings: &Settings) -> Result<Store> {
    if let Some(mut store) = cache.get::<Store>(INSTANCES_KEY)? {
        let mut dirty = store.version != INSTANCES_VERSION;
        if store.instances.is_empty() {
            store.instances.push(default_instance(settings));
            dirty = true;
        }
        if store.current().is_none() {
            store.current = store.instances[0].id.clone();
            dirty = true;
        }
        if dirty {
            store.version = INSTANCES_VERSION;
            cache.set(INSTANCES_KEY, &store)?;
        }
        return Ok(store);
    }
    let inst = default_instance(settings);
    match move_named_lists(data_dir, &inst.id) {
        Ok(n) if n > 0 => tracing::info!(lists = n, "moved the named lists into the default instance"),
        Err(e) => tracing::warn!("could not move the named lists into the default instance: {e}"),
        _ => {}
    }
    carry(cache, "current_list", &current_list_key(&inst.id));
    carry(cache, "mod_baseline", &baseline_key(&inst.id));
    let store = Store { version: INSTANCES_VERSION, current: inst.id.clone(), instances: vec![inst] };
    cache.set(INSTANCES_KEY, &store)?;
    Ok(store)
}

fn save(cache: &Cache, store: &Store) -> Result<()> {
    cache.set(INSTANCES_KEY, store)
}

fn read_store(app: &App) -> Result<Store> {
    load(&app.cache, &app.data_dir, &app.settings)
}

pub fn list(app: &App) -> Vec<Instance> {
    read_store(app).map(|s| s.instances).unwrap_or_else(|_| vec![app.instance.clone()])
}

/// Write the settings the user has been editing back into the instance they belong to. Settings
/// and the current instance hold the same folders on purpose: Settings edits the one that is open.
pub fn sync_current(app: &mut App) -> Result<()> {
    app.instance.locations = app.settings.locations.clone();
    app.instance.launch = app.settings.launch.clone();
    let mut store = read_store(app)?;
    match store.instances.iter_mut().find(|i| i.id == app.instance.id) {
        Some(slot) => *slot = app.instance.clone(),
        None => store.instances.push(app.instance.clone()),
    }
    store.current = app.instance.id.clone();
    save(&app.cache, &store)
}

/// Make an instance. With `from_current`, it starts with the folders and launch settings of the
/// instance that is open (the usual way to make "the same install, another config folder");
/// otherwise it starts empty and autodetection fills it in until the user chooses folders.
pub fn create(app: &mut App, name: &str, from_current: bool) -> Result<Instance> {
    let name = clean_name(name)?;
    let mut store = read_store(app)?;
    let inst = Instance {
        id: make_id(&name, &store.instances),
        name,
        locations: if from_current { app.settings.locations.clone() } else { Locations::default() },
        launch: if from_current { app.settings.launch.clone() } else { LaunchSettings::default() },
        created_at: now_secs(),
    };
    store.instances.push(inst.clone());
    save(&app.cache, &store)?;
    Ok(inst)
}

/// Copy an instance, folders and all. The copy points at the same folders on disk: nothing is
/// duplicated there, and that is the point — two instances may share a game or Workshop folder.
pub fn duplicate(app: &mut App, id: &str, name: Option<&str>) -> Result<Instance> {
    let mut store = read_store(app)?;
    let src = store.get(id).cloned().ok_or_else(|| Error::Other("No such instance".into()))?;
    let name = clean_name(name.unwrap_or(&format!("{} copy", src.name)))?;
    let inst = Instance { id: make_id(&name, &store.instances), name, locations: src.locations.clone(), launch: src.launch.clone(), created_at: now_secs() };
    store.instances.push(inst.clone());
    save(&app.cache, &store)?;
    Ok(inst)
}

pub fn rename(app: &mut App, id: &str, name: &str) -> Result<Instance> {
    let name = clean_name(name)?;
    let mut store = read_store(app)?;
    let slot = store.instances.iter_mut().find(|i| i.id == id).ok_or_else(|| Error::Other("No such instance".into()))?;
    slot.name = name;
    let out = slot.clone();
    if app.instance.id == id {
        app.instance.name = out.name.clone();
    }
    save(&app.cache, &store)?;
    Ok(out)
}

/// Change an instance's folders and launch settings. When it is the one that is open, the
/// settings follow and the folders are resolved again straight away.
pub fn update(app: &mut App, id: &str, locations: Locations, launch: LaunchSettings) -> Result<Instance> {
    let mut store = read_store(app)?;
    let slot = store.instances.iter_mut().find(|i| i.id == id).ok_or_else(|| Error::Other("No such instance".into()))?;
    slot.locations = locations.clone();
    slot.launch = launch.clone();
    let out = slot.clone();
    save(&app.cache, &store)?;
    if app.instance.id == id {
        app.instance = out.clone();
        let changed = app.settings.locations != locations;
        app.settings.locations = locations;
        app.settings.launch = launch;
        app.persist()?;
        if changed {
            app.resolve_locations();
            app.load_databases();
        }
    }
    Ok(out)
}

/// Forget an instance. Nothing on disk that belongs to the game is touched: the mods, the saves
/// and the config folder it pointed at stay exactly where they are — deleting here is only
/// Circinus forgetting the arrangement. Its named lists stay in the data folder too, so a
/// mistaken delete costs nothing.
pub fn delete(app: &mut App, id: &str) -> Result<String> {
    let mut store = read_store(app)?;
    if store.instances.len() < 2 {
        return Err(Error::Other("This is the only instance. Make another one first.".into()));
    }
    let inst = store.get(id).cloned().ok_or_else(|| Error::Other("No such instance".into()))?;
    store.instances.retain(|i| i.id != id);
    let switch_to = if store.current == id { store.instances[0].id.clone() } else { store.current.clone() };
    store.current = switch_to.clone();
    save(&app.cache, &store)?;
    if app.instance.id == id {
        adopt(app, &switch_to)?;
    }
    Ok(format!("Forgot the instance {}. Its mods, saves and config folder are where they were.", inst.name))
}

/// Point the app at another instance. The caller scans afterwards: this only moves the state.
fn adopt(app: &mut App, id: &str) -> Result<()> {
    let mut store = read_store(app)?;
    let inst = store.get(id).cloned().ok_or_else(|| Error::Other("No such instance".into()))?;
    store.current = inst.id.clone();
    save(&app.cache, &store)?;
    app.instance = inst.clone();
    app.settings.locations = inst.locations.clone();
    app.settings.launch = inst.launch.clone();
    app.persist()?;
    // Nothing of the old instance's install may survive into the new one: its mods live in other
    // folders and its active list belongs to another ModsConfig.xml.
    app.mods.clear();
    app.files.clear();
    app.active.clear();
    app.missing.clear();
    app.shallow.clear();
    app.pending_stamps.clear();
    app.changes.clear();
    app.list_change = None;
    app.list_reset = None;
    app.dirty = false;
    app.adopt_instance_state();
    app.resolve_locations();
    app.load_databases();
    app.read_mods_config();
    Ok(())
}

/// Switch instance: keep what the open one is holding, then take up the other one.
///
/// Unsaved list changes stop the switch the way the rest of the app treats `dirty`; `discard`
/// is the user saying to drop them.
pub fn switch(app: &mut App, id: &str, discard: bool) -> Result<Instance> {
    if app.instance.id == id {
        return Ok(app.instance.clone());
    }
    if app.dirty && !discard {
        return Err(Error::Other("The list has unsaved changes. Save it first, or switch and lose them.".into()));
    }
    sync_current(app)?;
    adopt(app, id)?;
    Ok(app.instance.clone())
}

fn clean_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 80 {
        return Err(Error::Other("Give the instance a name of up to 80 characters".into()));
    }
    Ok(name.to_string())
}

/// The `-savedatafolder` argument an instance needs, if it needs one.
///
/// This is the one genuinely dangerous thing about instances. RimWorld writes ModsConfig.xml,
/// Prefs.xml and the saves under one folder, and unless it is told otherwise that folder is the
/// same for every copy of the game on the machine. So an instance with a config folder of its
/// own is a lie unless the game is started with `-savedatafolder=<that folder's parent>`: the
/// user would switch instance, press Play, and the game would read and rewrite the *default*
/// ModsConfig.xml — quietly undoing the list Circinus just wrote and mixing two playthroughs'
/// saves. Every launch therefore carries the argument whenever the config folder is not the
/// platform default one, and an argument the user wrote themselves wins over ours.
///
/// RimWorld appends `Config` to the folder it is given, so a config folder of `<X>/Config` means
/// `-savedatafolder=<X>`. A config folder not named `Config` is passed as it is: it is not a
/// shape RimWorld makes, and guessing a parent would point the game somewhere the user never named.
pub fn save_data_folder(locations: &Locations) -> Option<PathBuf> {
    let config = locations.config_dir.as_ref()?;
    if circinus_core::paths::default_config_dir().as_deref() == Some(config.as_path()) {
        return None;
    }
    match config.file_name() {
        Some(n) if n.eq_ignore_ascii_case("config") => config.parent().map(|p| p.to_path_buf()),
        _ => Some(config.clone()),
    }
}

/// The launch arguments for an instance: what the user wrote, plus `-savedatafolder` when the
/// config folder needs it and the user did not write one of their own.
pub fn launch_args(locations: &Locations, args: &str) -> Vec<String> {
    let mut out = circinus_core::paths::split_args(args);
    if out.iter().any(|a| a.to_lowercase().starts_with("-savedatafolder")) {
        return out;
    }
    if let Some(folder) = save_data_folder(locations) {
        out.push(format!("-savedatafolder={}", folder.display()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use circinus_core::modsconfig;

    fn write(p: &Path, s: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, s).unwrap();
    }

    /// Two game folders, each with Core and its own ModsConfig.xml, and one mod only the second
    /// has. Returns the temp dir (game A, game B, data folder).
    fn fixture() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        for (name, list) in [("a", "<li>ludeon.rimworld</li><li>brrainz.harmony</li>"), ("b", "<li>ludeon.rimworld</li>")] {
            let game = tmp.path().join(name);
            write(&game.join("Version.txt"), "1.6.4530 rev1235");
            write(&game.join("Data/Core/About/About.xml"), "<ModMetaData><packageId>Ludeon.RimWorld</packageId></ModMetaData>");
            write(&game.join("Mods/Harmony/About/About.xml"), "<ModMetaData><packageId>brrainz.harmony</packageId><name>Harmony</name><supportedVersions><li>1.6</li></supportedVersions></ModMetaData>");
            write(&game.join("Config/ModsConfig.xml"), &format!("<ModsConfigData><version>1.6.4530 rev1235</version><activeMods>{list}</activeMods><knownExpansions></knownExpansions></ModsConfigData>"));
        }
        tmp
    }

    fn settings_for(game: &Path) -> Settings {
        let mut s = Settings::default();
        s.locations.game_dir = Some(game.to_path_buf());
        s.locations.config_dir = Some(game.join("Config"));
        s.locations.local_mods_dir = Some(game.join("Mods"));
        s
    }

    fn open(data: &Path, game: &Path) -> App {
        let mut app = App::open_at(data.to_path_buf(), Some(settings_for(game))).unwrap();
        let shallow = app.scan_quick(true, &|_, _| {}).unwrap();
        app.apply_inspections(circinus_core::scan::inspect_mods(&shallow, &|_, _| {})).unwrap();
        app
    }

    #[test]
    fn a_new_instance_starts_from_the_current_settings() {
        let tmp = fixture();
        let mut app = open(&tmp.path().join("data"), &tmp.path().join("a"));
        assert_eq!(app.instance.id, DEFAULT_ID);
        assert_eq!(app.instance.locations, app.settings.locations, "the default instance holds what the user already had");
        let made = create(&mut app, "CE playthrough", true).unwrap();
        assert_eq!(made.name, "CE playthrough");
        assert_ne!(made.id, DEFAULT_ID);
        assert_eq!(made.locations, app.settings.locations, "made from the current settings");
        let empty = create(&mut app, "Modding sandbox", false).unwrap();
        assert_eq!(empty.locations, Locations::default(), "made empty when asked for");
        assert_eq!(list(&app).len(), 3);
        // Duplicating shares the folders: two instances on one game folder is a normal thing.
        let copy = duplicate(&mut app, &made.id, None).unwrap();
        assert_eq!(copy.name, "CE playthrough copy");
        assert_eq!(copy.locations, made.locations);
        assert_ne!(copy.id, made.id);
        assert_eq!(rename(&mut app, &copy.id, "CE 1.6").unwrap().name, "CE 1.6");
        assert!(create(&mut app, "   ", true).is_err(), "a name is required");
    }

    #[test]
    fn switching_moves_the_folders_and_the_current_list() {
        let tmp = fixture();
        let data = tmp.path().join("data");
        let mut app = open(&data, &tmp.path().join("a"));
        assert_eq!(app.active.len(), 2, "game A has Core and Harmony active");
        app.save_named_list("A list").unwrap();
        assert_eq!(app.current_list.as_deref(), Some("A list"));
        // A second instance on the other game folder.
        let b = create(&mut app, "Game B", false).unwrap();
        update(&mut app, &b.id, settings_for(&tmp.path().join("b")).locations, LaunchSettings::default()).unwrap();
        // An unsaved change stops the switch until it is discarded.
        app.deactivate(&[app.active[app.active.len() - 1].clone()]);
        assert!(app.dirty);
        assert!(switch(&mut app, &b.id, false).is_err(), "unsaved list changes stop the switch");
        switch(&mut app, &b.id, true).unwrap();
        assert_eq!(app.instance.id, b.id);
        assert_eq!(app.settings.locations.game_dir, Some(tmp.path().join("b")), "the folders followed");
        assert_eq!(app.locations.config_dir, Some(tmp.path().join("b/Config")));
        assert!(app.current_list.is_none(), "the new instance has no list open yet");
        assert!(app.named_lists().is_empty(), "and none of the other instance's lists");
        let shallow = app.scan_quick(false, &|_, _| {}).unwrap();
        app.apply_inspections(circinus_core::scan::inspect_mods(&shallow, &|_, _| {})).unwrap();
        assert_eq!(app.active.len(), 1, "game B's own ModsConfig.xml was read");
        app.save_named_list("B list").unwrap();
        // Back again: instance A's folders, its list and its named lists are all as they were.
        switch(&mut app, DEFAULT_ID, true).unwrap();
        assert_eq!(app.settings.locations.game_dir, Some(tmp.path().join("a")));
        assert_eq!(app.current_list.as_deref(), Some("A list"));
        assert_eq!(app.named_lists().iter().map(|l| l.name.clone()).collect::<Vec<_>>(), vec!["A list"]);
        let shallow = app.scan_quick(false, &|_, _| {}).unwrap();
        app.apply_inspections(circinus_core::scan::inspect_mods(&shallow, &|_, _| {})).unwrap();
        assert_eq!(app.active.len(), 2, "game A's list is back, unsaved changes and all discarded");
        // And the switch survives a restart.
        let again = App::open_at(data.clone(), None).unwrap();
        assert_eq!(again.instance.id, DEFAULT_ID);
        assert_eq!(again.settings.locations.game_dir, Some(tmp.path().join("a")));
        assert_eq!(again.current_list.as_deref(), Some("A list"));
    }

    #[test]
    fn an_existing_install_becomes_the_default_instance_without_losing_anything() {
        let tmp = fixture();
        let data = tmp.path().join("data");
        let game = tmp.path().join("a");
        // An install as it was before instances existed: settings, two named lists in the flat
        // folder, a current list and a change baseline, all under the old keys.
        {
            let cache = Cache::open(&data.join("cache.sqlite")).unwrap();
            let settings = settings_for(&game);
            cache.set("settings", &settings).unwrap();
            cache.set("current_list", &Some("Vanilla plus".to_string())).unwrap();
            cache.set("mod_baseline", &serde_json::json!({ "mods": {}, "workshop": {}, "active": ["ludeon.rimworld"], "takenAt": 1_756_000_000 })).unwrap();
            let flat = data.join("lists").join("named");
            std::fs::create_dir_all(&flat).unwrap();
            for name in ["Vanilla plus", "Full run"] {
                let cfg = modsconfig::ModsConfig { version: "1.6.4530 rev1235".into(), active_mods: vec!["ludeon.rimworld".into()], known_expansions: vec![] };
                std::fs::write(flat.join(format!("{name}.xml")), modsconfig::to_xml(&cfg)).unwrap();
            }
        }
        let app = App::open_at(data.clone(), None).unwrap();
        assert_eq!(app.instance.id, DEFAULT_ID);
        assert_eq!(app.instance.name, "Default");
        assert_eq!(app.instance.locations.game_dir, Some(game.clone()), "the folders the user had are the default instance's");
        assert_eq!(app.settings.locations.game_dir, Some(game.clone()), "and the settings still say so");
        assert_eq!(app.current_list.as_deref(), Some("Vanilla plus"), "the list being worked on is still open");
        let mut names: Vec<String> = app.named_lists().into_iter().map(|l| l.name).collect();
        names.sort();
        assert_eq!(names, vec!["Full run", "Vanilla plus"], "the named lists moved into the default instance");
        assert!(data.join("lists/named/default/Vanilla plus.xml").is_file());
        assert!(!data.join("lists/named/Vanilla plus.xml").exists(), "and are not left behind in the flat folder");
        assert!(app.baseline.is_some(), "and what changed since last time is still measured from the same point");
        // Opening again does not migrate a second time or lose the instance.
        let again = App::open_at(data.clone(), None).unwrap();
        assert_eq!(again.instance.id, DEFAULT_ID);
        assert_eq!(again.named_lists().len(), 2);
        assert_eq!(list(&again).len(), 1);
    }

    #[test]
    fn deleting_an_instance_forgets_it_and_leaves_the_folders_alone() {
        let tmp = fixture();
        let mut app = open(&tmp.path().join("data"), &tmp.path().join("a"));
        let b = create(&mut app, "Game B", false).unwrap();
        update(&mut app, &b.id, settings_for(&tmp.path().join("b")).locations, LaunchSettings::default()).unwrap();
        switch(&mut app, &b.id, true).unwrap();
        let what = delete(&mut app, &b.id).unwrap();
        assert!(what.contains("Forgot the instance Game B"), "{what}");
        assert!(what.contains("where they were"), "{what}");
        assert_eq!(app.instance.id, DEFAULT_ID, "deleting the open instance falls back to another");
        assert_eq!(list(&app).len(), 1);
        // Every folder the deleted instance pointed at is untouched.
        let b_dir = tmp.path().join("b");
        assert!(b_dir.join("Version.txt").is_file());
        assert!(b_dir.join("Config/ModsConfig.xml").is_file());
        assert!(b_dir.join("Mods/Harmony/About/About.xml").is_file());
        assert!(tmp.path().join("a/Config/ModsConfig.xml").is_file());
        assert!(delete(&mut app, DEFAULT_ID).is_err(), "the last instance stays");
    }

    #[test]
    fn a_config_folder_of_its_own_is_carried_on_the_command_line() {
        let mut loc = Locations { game_dir: Some(PathBuf::from("/games/RimWorld")), ..Default::default() };
        // No config folder, nothing to say.
        assert_eq!(launch_args(&loc, "-popupwindow"), vec!["-popupwindow"]);
        loc.config_dir = Some(PathBuf::from("/saves/ce/Config"));
        assert_eq!(save_data_folder(&loc), Some(PathBuf::from("/saves/ce")), "RimWorld appends Config itself");
        assert_eq!(launch_args(&loc, "-popupwindow"), vec!["-popupwindow", "-savedatafolder=/saves/ce"]);
        // A folder not named Config is passed as it is rather than guessing at its parent.
        loc.config_dir = Some(PathBuf::from("/saves/ce-settings"));
        assert_eq!(save_data_folder(&loc), Some(PathBuf::from("/saves/ce-settings")));
        // What the user wrote wins.
        assert_eq!(launch_args(&loc, "-savedatafolder=/elsewhere"), vec!["-savedatafolder=/elsewhere"]);
        // The platform's own config folder needs no argument: it is where the game looks anyway.
        if let Some(d) = circinus_core::paths::default_config_dir() {
            loc.config_dir = Some(d);
            assert_eq!(save_data_folder(&loc), None);
            assert!(launch_args(&loc, "").is_empty());
        }
    }
}
