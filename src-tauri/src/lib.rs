mod commands;
mod state;

use commands::Shared;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};

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
        .manage(shared.clone())
        .setup(move |app| {
            // First scan in the background so the window appears immediately.
            let handle = app.handle().clone();
            let st = shared.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let progress = {
                    let h = handle.clone();
                    move |done: usize, total: usize| {
                        let _ = h.emit("scan-progress", commands::ScanProgress { done, total });
                    }
                };
                let result = {
                    let mut app = st.lock().unwrap();
                    app.scan(false, &progress)
                };
                match result {
                    Ok(()) => {
                        let _ = handle.emit("state-changed", ());
                    }
                    Err(e) => {
                        let _ = handle.emit("scan-error", e.to_string());
                    }
                }
            });
            #[cfg(debug_assertions)]
            if let Some(w) = app.get_webview_window("main") {
                w.open_devtools();
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running Circinus");
}
