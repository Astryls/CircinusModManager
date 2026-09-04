//! The command surface the Svelte UI calls with `invoke`.

use crate::downloads::{AddResult, Downloads, SteamCmdStatus};
use crate::textures::{self, ModTextures, Report, TexState, Textures};
use crate::state::{App, Settings, Snapshot, UserData};
use circinus_core::steam::steamcmd::QueueState;
use circinus_core::steam::webapi;
use circinus_core::import::{self, ImportedList};
use circinus_core::model::*;
use circinus_core::paths::Locations;
use circinus_core::rules::{self, RulesFile};
use circinus_core::scan::ModFiles;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, State};

pub type Shared = Arc<Mutex<App>>;

type CmdResult<T> = std::result::Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// Run `f` with the locked app on a blocking thread.
async fn with_app<T: Send + 'static>(state: &State<'_, Shared>, f: impl FnOnce(&mut App) -> CmdResult<T> + Send + 'static) -> CmdResult<T> {
    let shared = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut app = shared.lock().map_err(|_| "state lock poisoned".to_string())?;
        f(&mut app)
    })
    .await
    .map_err(err)?
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    /// "read" (About folders) or "inspect" (walking contents).
    pub phase: &'static str,
    pub done: usize,
    pub total: usize,
}

#[tauri::command]
pub async fn get_snapshot(state: State<'_, Shared>) -> CmdResult<Snapshot> {
    let snap = with_app(&state, |app| Ok(app.snapshot())).await?;
    if cfg!(debug_assertions) {
        let bytes = serde_json::to_vec(&snap).map(|v| v.len()).unwrap_or(0);
        tracing::info!(bytes, "get_snapshot serialized");
    }
    Ok(snap)
}

#[tauri::command]
pub async fn get_description(state: State<'_, Shared>, uid: String) -> CmdResult<String> {
    with_app(&state, move |app| Ok(app.description(&uid))).await
}

/// Re-read the mod folders. Returns as soon as the quick phase is done; the contents
/// inspection continues in the background and ends with a `state-changed` event.
#[tauri::command]
pub async fn rescan(app_handle: AppHandle, state: State<'_, Shared>, full: bool) -> CmdResult<Snapshot> {
    let shared = state.inner().clone();
    let handle = app_handle.clone();
    let shallow = tauri::async_runtime::spawn_blocking(move || crate::scan_quick_phase(&handle, &shared, full)).await.map_err(err)??;
    let shared = state.inner().clone();
    let handle = app_handle.clone();
    tauri::async_runtime::spawn_blocking(move || crate::inspect_phase(&handle, &shared, shallow));
    with_app(&state, |app| Ok(app.snapshot())).await
}

#[tauri::command]
pub async fn set_active(state: State<'_, Shared>, uids: Vec<String>) -> CmdResult<Snapshot> {
    with_app(&state, move |app| {
        app.set_active(uids);
        Ok(app.snapshot())
    })
    .await
}

#[tauri::command]
pub async fn activate(state: State<'_, Shared>, uids: Vec<String>, at: Option<usize>) -> CmdResult<Snapshot> {
    with_app(&state, move |app| {
        app.activate(&uids, at);
        Ok(app.snapshot())
    })
    .await
}

#[tauri::command]
pub async fn deactivate(state: State<'_, Shared>, uids: Vec<String>) -> CmdResult<Snapshot> {
    with_app(&state, move |app| {
        app.deactivate(&uids);
        Ok(app.snapshot())
    })
    .await
}

#[tauri::command]
pub async fn halo(state: State<'_, Shared>, apply: bool) -> CmdResult<SortResult> {
    with_app(&state, move |app| Ok(app.halo(apply))).await
}

#[tauri::command]
pub async fn validate(state: State<'_, Shared>) -> CmdResult<Vec<Issue>> {
    with_app(&state, |app| Ok(app.issues())).await
}

#[tauri::command]
pub async fn save_mods_config(state: State<'_, Shared>) -> CmdResult<String> {
    with_app(&state, |app| app.save().map(|p| p.display().to_string()).map_err(err)).await
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub list: ImportedList,
    pub uids: Vec<String>,
    pub missing: Vec<String>,
}

#[tauri::command]
pub async fn import_list(state: State<'_, Shared>, path: Option<String>, text: Option<String>) -> CmdResult<ImportPreview> {
    with_app(&state, move |app| {
        let list = match (path, text) {
            (Some(p), _) => import::import_file(&PathBuf::from(p)).map_err(err)?,
            (None, Some(t)) => import::import_text(&t, &PathBuf::from("pasted.txt")).map_err(err)?,
            _ => return Err("Nothing to import".into()),
        };
        let (uids, missing) = app.resolve_import(&list);
        Ok(ImportPreview { list, uids, missing })
    })
    .await
}

#[tauri::command]
pub async fn apply_import(state: State<'_, Shared>, uids: Vec<String>, append: bool) -> CmdResult<Snapshot> {
    with_app(&state, move |app| {
        if append {
            app.activate(&uids, None);
        } else {
            app.set_active(uids);
        }
        Ok(app.snapshot())
    })
    .await
}

#[tauri::command]
pub async fn update_settings(state: State<'_, Shared>, settings: Settings) -> CmdResult<Snapshot> {
    with_app(&state, move |app| {
        let locations_changed = app.settings.locations != settings.locations;
        app.settings = settings;
        app.persist().map_err(err)?;
        if locations_changed {
            app.resolve_locations();
            app.load_databases();
        }
        Ok(app.snapshot())
    })
    .await
}

#[tauri::command]
pub async fn autodetect_locations() -> CmdResult<Locations> {
    tauri::async_runtime::spawn_blocking(Locations::detect).await.map_err(err)
}

#[tauri::command]
pub async fn update_user(state: State<'_, Shared>, user: UserData) -> CmdResult<Snapshot> {
    with_app(&state, move |app| {
        app.user = user;
        app.persist().map_err(err)?;
        Ok(app.snapshot())
    })
    .await
}

#[tauri::command]
pub async fn update_databases(state: State<'_, Shared>) -> CmdResult<Vec<String>> {
    let (sources, dir, version) = {
        let app = state.inner().lock().map_err(|_| "state lock poisoned".to_string())?;
        (app.settings.db_sources.clone(), app.data_dir.join("dbs"), app.game_version.major_minor.clone())
    };
    let client = reqwest::Client::builder().user_agent(circinus_core::weight::USER_AGENT).build().map_err(err)?;
    let mut report = Vec::new();
    for src in sources.iter().filter(|s| s.enabled) {
        match rules::fetch_source(&client, src, &dir, &version).await {
            Ok(true) => report.push(format!("{}: updated", src.label)),
            Ok(false) => report.push(format!("{}: already current", src.label)),
            Err(e) => report.push(format!("{}: failed ({e})", src.label)),
        }
    }
    with_app(&state, |app| {
        app.load_databases();
        Ok(())
    })
    .await?;
    Ok(report)
}

#[tauri::command]
pub async fn refresh_weights(state: State<'_, Shared>) -> CmdResult<usize> {
    let client = reqwest::Client::builder().user_agent(circinus_core::weight::USER_AGENT).build().map_err(err)?;
    let fetched = circinus_core::weight::fetch_all(&client).await.map_err(err)?;
    with_app(&state, move |app| {
        let n = app.store_weights(fetched).map_err(err)?;
        let local = if app.settings.include_local_runs { app.merge_local_weights() } else { 0 };
        Ok(n + local)
    })
    .await
}

#[tauri::command]
pub async fn refresh_local_weights(state: State<'_, Shared>) -> CmdResult<usize> {
    with_app(&state, |app| Ok(app.merge_local_weights())).await
}

#[tauri::command]
pub async fn get_files(state: State<'_, Shared>, uid: String) -> CmdResult<ModFiles> {
    with_app(&state, move |app| Ok(app.files.get(&uid).cloned().unwrap_or_default())).await
}

#[tauri::command]
pub async fn get_user_rules(state: State<'_, Shared>) -> CmdResult<RulesFile> {
    with_app(&state, |app| Ok(app.db.user.clone())).await
}

#[tauri::command]
pub async fn set_user_rules(state: State<'_, Shared>, rules: RulesFile) -> CmdResult<Snapshot> {
    with_app(&state, move |app| {
        app.write_user_rules(&rules).map_err(err)?;
        Ok(app.snapshot())
    })
    .await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleEdit {
    pub rule: Rule,
    pub remove: bool,
}

/// Add or remove one user rule (or an ignore entry when `rule.source` is not `user`).
#[tauri::command]
pub async fn edit_user_rule(state: State<'_, Shared>, edit: RuleEdit) -> CmdResult<Snapshot> {
    with_app(&state, move |app| {
        let mut file = app.db.user.clone();
        let mut r = edit.rule;
        if r.source == RuleSource::User {
            file.rules.retain(|x| !(x.kind == r.kind && x.subject == r.subject && x.target == r.target));
            if !edit.remove {
                file.rules.push(r);
            }
        } else {
            r.source = RuleSource::User;
            file.ignore.retain(|x| !(x.kind == r.kind && x.subject == r.subject && x.target == r.target));
            if !edit.remove {
                file.ignore.push(r);
            }
        }
        app.write_user_rules(&file).map_err(err)?;
        Ok(app.snapshot())
    })
    .await
}

#[tauri::command]
pub fn app_data_dir(state: State<'_, Shared>) -> CmdResult<String> {
    let app = state.inner().lock().map_err(|_| "state lock poisoned".to_string())?;
    Ok(app.data_dir.display().to_string())
}


// ---------------------------------------------------------------- downloads

type Dl<'a> = State<'a, Arc<Downloads>>;

#[tauri::command]
pub fn downloads_state(dl: Dl<'_>) -> QueueState {
    dl.snapshot()
}

#[tauri::command]
pub async fn downloads_add(dl: Dl<'_>, ids: Vec<u64>) -> CmdResult<AddResult> {
    Ok(dl.add(ids).await)
}

/// Workshop URLs, ids or pasted text; single collection links are expanded.
#[tauri::command]
pub async fn downloads_add_text(dl: Dl<'_>, text: String) -> CmdResult<AddResult> {
    let mut ids = webapi::extract_workshop_ids(&text);
    if ids.is_empty() {
        return Err("No workshop ids or links found in that text".into());
    }
    if ids.len() <= 5 {
        let client = reqwest::Client::builder().user_agent(circinus_core::weight::USER_AGENT).build().map_err(err)?;
        let mut expanded = Vec::new();
        for id in &ids {
            match webapi::collection_items(&client, *id).await {
                Ok(children) => expanded.extend(children),
                Err(_) => expanded.push(*id),
            }
        }
        ids = expanded;
    }
    Ok(dl.add(ids).await)
}

#[tauri::command]
pub fn downloads_remove(dl: Dl<'_>, ids: Vec<u64>) -> QueueState {
    dl.remove(&ids);
    dl.snapshot()
}

#[tauri::command]
pub fn downloads_retry_failed(dl: Dl<'_>) -> usize {
    dl.retry_failed()
}

#[tauri::command]
pub fn downloads_clear_finished(dl: Dl<'_>) -> QueueState {
    dl.clear_finished();
    dl.snapshot()
}

#[tauri::command]
pub fn downloads_pause(dl: Dl<'_>, paused: bool) -> QueueState {
    dl.set_paused(paused);
    dl.snapshot()
}

#[tauri::command]
pub async fn steamcmd_install(dl: Dl<'_>) -> CmdResult<()> {
    let d = dl.inner().clone();
    d.install_steamcmd().await
}

#[tauri::command]
pub fn steamcmd_status(dl: Dl<'_>) -> SteamCmdStatus {
    dl.status()
}

/// `+login anonymous +quit`, output streamed into the download log.
#[tauri::command]
pub async fn steamcmd_test(dl: Dl<'_>) -> CmdResult<circinus_core::steam::steamcmd::TestOutcome> {
    let d = dl.inner().clone();
    d.test_steamcmd().await
}

/// The user has read the "what changed" list: measure future changes from now.
#[tauri::command]
pub async fn acknowledge_changes(state: State<'_, Shared>) -> CmdResult<Snapshot> {
    with_app(&state, |app| {
        app.acknowledge_changes();
        Ok(app.snapshot())
    })
    .await
}

/// Queue everything listed in ModsConfig.xml that is not installed (resolved via the Steam DB).
#[tauri::command]
pub async fn downloads_add_missing(dl: Dl<'_>, state: State<'_, Shared>) -> CmdResult<(AddResult, Vec<String>)> {
    let (ids, unresolved) = with_app(&state, |app| Ok(app.workshop_ids_for_missing())).await?;
    if ids.is_empty() {
        return Ok((AddResult { added: 0, skipped: vec![] }, unresolved));
    }
    Ok((dl.add(ids).await, unresolved))
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CollectionPreview {
    pub ids: Vec<u64>,
    /// (workshop id, uid) for items already installed.
    pub installed: Vec<(u64, String)>,
    pub missing: Vec<u64>,
    pub names: HashMap<u64, String>,
}

use std::collections::HashMap;

/// Expand a Steam collection (or a pasted list of workshop links) and match it against the install.
#[tauri::command]
pub async fn import_collection(state: State<'_, Shared>, text: String) -> CmdResult<CollectionPreview> {
    let ids = webapi::extract_workshop_ids(&text);
    if ids.is_empty() {
        return Err("No workshop link or id found".into());
    }
    let client = reqwest::Client::builder().user_agent(circinus_core::weight::USER_AGENT).build().map_err(err)?;
    let mut all: Vec<u64> = Vec::new();
    for id in ids.iter().take(10) {
        match webapi::collection_items(&client, *id).await {
            Ok(children) => all.extend(children),
            Err(_) => all.push(*id),
        }
    }
    let mut names: HashMap<u64, String> = HashMap::new();
    if let Ok(items) = webapi::published_file_details(&client, &all).await {
        for i in items {
            names.insert(i.published_file_id, i.title);
        }
    }
    let all2 = all.clone();
    let (installed, missing) = with_app(&state, move |app| {
        let by_pfid: HashMap<u64, String> = app.mods.iter().filter(|m| m.invalid.is_none()).filter_map(|m| m.published_file_id.map(|id| (id, m.uid.clone()))).collect();
        let mut installed = Vec::new();
        let mut missing = Vec::new();
        for id in &all2 {
            match by_pfid.get(id) {
                Some(uid) => installed.push((*id, uid.clone())),
                None => missing.push(*id),
            }
        }
        Ok((installed, missing))
    })
    .await?;
    Ok(CollectionPreview { ids: all, installed, missing, names })
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RentryPreview {
    pub preview: ImportPreview,
    /// Workshop ids mentioned in the paste that are not installed.
    pub missing_workshop_ids: Vec<u64>,
}

#[tauri::command]
pub async fn import_rentry(state: State<'_, Shared>, url: String) -> CmdResult<RentryPreview> {
    let id = circinus_core::rentry::parse_rentry_id(&url).ok_or_else(|| "That does not look like a Rentry link".to_string())?;
    let client = reqwest::Client::builder().user_agent(circinus_core::weight::USER_AGENT).build().map_err(err)?;
    let text = circinus_core::rentry::fetch_rentry(&client, &id, None).await.map_err(err)?;
    let (list, workshop_ids) = circinus_core::rentry::parse_rentry_text(&text);
    with_app(&state, move |app| {
        let (uids, missing) = app.resolve_import(&list);
        let installed: std::collections::HashSet<u64> = app.mods.iter().filter_map(|m| m.published_file_id).collect();
        let missing_workshop_ids = workshop_ids.into_iter().filter(|id| !installed.contains(id)).collect();
        Ok(RentryPreview { preview: ImportPreview { list, uids, missing }, missing_workshop_ids })
    })
    .await
}

// ---------------------------------------------------------------- textures

type Tex<'a> = State<'a, Arc<Textures>>;

#[tauri::command]
pub fn dds_state(tex: Tex<'_>) -> TexState {
    tex.snapshot()
}

#[tauri::command]
pub async fn dds_overview(state: State<'_, Shared>) -> CmdResult<Vec<ModTextures>> {
    with_app(&state, |app| Ok(textures::overview(app))).await
}

/// Convert the textures of these mods in the background; progress arrives as `dds-progress`.
#[tauri::command]
pub async fn dds_start(tex: Tex<'_>, uids: Vec<String>) -> CmdResult<()> {
    if tex.snapshot().running {
        return Err("A texture job is already running".into());
    }
    let t = tex.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(e) = t.convert(uids) {
            tracing::warn!("dds convert: {e}");
        }
    });
    Ok(())
}

#[tauri::command]
pub fn dds_cancel(tex: Tex<'_>) {
    tex.cancel();
}

#[tauri::command]
pub async fn dds_revert(tex: Tex<'_>, uids: Vec<String>) -> CmdResult<Report> {
    let t = tex.inner().clone();
    tauri::async_runtime::spawn_blocking(move || t.revert(uids)).await.map_err(err)?
}

/// Ask the Workshop for the current update time of every installed workshop mod.
#[tauri::command]
pub async fn check_updates(state: State<'_, Shared>) -> CmdResult<usize> {
    let ids: Vec<u64> = with_app(&state, |app| Ok(app.workshop_ids().into_iter().map(|(_, id)| id).collect())).await?;
    if ids.is_empty() {
        return Ok(0);
    }
    let client = reqwest::Client::builder().user_agent(circinus_core::weight::USER_AGENT).build().map_err(err)?;
    let items = webapi::published_file_details(&client, &ids).await.map_err(err)?;
    with_app(&state, move |app| Ok(app.apply_update_check(&items))).await
}
