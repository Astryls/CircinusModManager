//! The command surface the Svelte UI calls with `invoke`.

use crate::state::{App, Settings, Snapshot, UserData};
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
