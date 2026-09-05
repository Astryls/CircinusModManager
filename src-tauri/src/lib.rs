pub mod commands;
pub mod downloads;
pub mod logs;
pub mod state;
pub mod textures;
pub mod watch;

use commands::{ScanProgress, Shared};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};

fn progress_emitter(handle: AppHandle, phase: &'static str) -> impl Fn(usize, usize) + Sync {
    let last = AtomicU64::new(0);
    move |done: usize, total: usize| {
        // at most ~20 events per second, plus the final one
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
        if done == total || t.saturating_sub(last.load(Ordering::Relaxed)) >= 50 {
            last.store(t, Ordering::Relaxed);
            let _ = handle.emit("scan-progress", ScanProgress { phase, done, total });
        }
    }
}

/// Phase 1 (under the lock, seconds): read About/ folders and ModsConfig.xml, publish the list.
/// Returns the mods whose folders still need walking.
pub fn scan_quick_phase(handle: &AppHandle, st: &Shared, full: bool) -> Result<Vec<circinus_core::ModInfo>, String> {
    let progress = progress_emitter(handle.clone(), "read");
    let result = {
        let mut app = st.lock().map_err(|_| "state lock poisoned".to_string())?;
        app.scan_quick(full, &progress)
    };
    match result {
        Ok(shallow) => {
            let _ = handle.emit("state-changed", ());
            Ok(shallow)
        }
        Err(e) => {
            let _ = handle.emit("scan-error", e.to_string());
            Err(e.to_string())
        }
    }
}

/// Phase 2 (no lock while walking): inspect folder contents, merge, cache, publish again.
pub fn inspect_phase(handle: &AppHandle, st: &Shared, shallow: Vec<circinus_core::ModInfo>) {
    if shallow.is_empty() {
        return;
    }
    let started = std::time::Instant::now();
    let inspections = circinus_core::scan::inspect_mods(&shallow, &progress_emitter(handle.clone(), "inspect"));
    tracing::info!(mods = shallow.len(), ms = started.elapsed().as_millis() as u64, "inspect");
    let result = match st.lock() {
        Ok(mut app) => app.apply_inspections(inspections).map_err(|e| e.to_string()),
        Err(_) => Err("state lock poisoned".into()),
    };
    match result {
        Ok(_) => {
            let _ = handle.emit("state-changed", ());
        }
        Err(e) => {
            let _ = handle.emit("scan-error", e);
        }
    }
}

/// Both phases back to back (startup, watcher, downloads), then the texture re-check.
pub fn run_scan(handle: AppHandle, st: Shared, full: bool) {
    if let Ok(shallow) = scan_quick_phase(&handle, &st, full) {
        inspect_phase(&handle, &st, shallow);
        if let Some(tex) = handle.try_state::<Arc<textures::Textures>>() {
            tex.inner().after_scan();
        }
    }
}

pub fn run() {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive("circinus=info".parse().unwrap())).init();

    let app = state::App::open().unwrap_or_else(|e| {
        eprintln!("Circinus could not open its data folder: {e}");
        std::process::exit(1);
    });
    let shared: Shared = Arc::new(Mutex::new(app));

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .manage(shared.clone())
        .setup(move |app| {
            // The download manager needs the app handle for events; created here.
            let dl = downloads::Downloads::start(app.handle().clone(), shared.clone());
            app.manage(dl);
            app.manage(textures::Textures::new(app.handle().clone(), shared.clone()));
            // First scan in the background so the window appears immediately.
            let handle = app.handle().clone();
            let st = shared.clone();
            tauri::async_runtime::spawn_blocking(move || run_scan(handle, st, false));
            // Then keep an eye on Steam and the game while we are open.
            watch::start(app.handle().clone(), shared.clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::get_description,
            commands::rescan,
            commands::set_active,
            commands::activate,
            commands::deactivate,
            commands::halo,
            commands::validate,
            commands::save_mods_config,
            commands::import_list,
            commands::apply_import,
            commands::update_settings,
            commands::autodetect_locations,
            commands::update_user,
            commands::update_databases,
            commands::refresh_weights,
            commands::refresh_local_weights,
            commands::get_files,
            commands::get_user_rules,
            commands::set_user_rules,
            commands::edit_user_rule,
            commands::app_data_dir,
            commands::downloads_state,
            commands::downloads_add,
            commands::downloads_add_text,
            commands::downloads_remove,
            commands::downloads_retry_failed,
            commands::downloads_clear_finished,
            commands::downloads_pause,
            commands::downloads_add_missing,
            commands::steamcmd_install,
            commands::steamcmd_status,
            commands::steamcmd_test,
            commands::acknowledge_changes,
            commands::dds_state,
            commands::dds_overview,
            commands::dds_start,
            commands::dds_cancel,
            commands::dds_revert,
            commands::dds_audit,
            commands::dds_fix,
            commands::saved_lists,
            commands::restore_list,
            commands::save_named_list,
            commands::load_named_list,
            commands::delete_named_list,
            commands::rename_named_list,
            commands::detach_list,
            commands::delete_mod,
            commands::collection_track,
            commands::collection_refresh,
            commands::collection_acknowledge,
            commands::collection_untrack,
            commands::get_launch_info,
            commands::launch_game,
            commands::player_log_paths,
            commands::analyze_player_log,
            commands::import_collection,
            commands::import_rentry,
            commands::check_updates,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Circinus");
}
