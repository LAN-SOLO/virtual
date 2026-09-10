// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod detect;
mod engine;
mod settings;
mod state;
mod store;

use state::AppState;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::Manager;

/// Alle laufenden Engines hart beenden (App-Ende).
fn stop_all(st: &Arc<AppState>) {
    let mut rt = st.runtime.lock().unwrap();
    for (_, r) in rt.drain() {
        r.kill();
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let config_dir = settings::config_dir(&handle);
            let data_dir = settings::data_dir(&handle);
            let s = settings::load(&config_dir);
            app.manage(Arc::new(AppState {
                settings: Mutex::new(s),
                runtime: Mutex::new(HashMap::new()),
                config_dir,
                data_dir,
            }));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::set_settings,
            commands::data_path,
            commands::list_profiles,
            commands::list_machines,
            commands::get_machine,
            commands::create_machine,
            commands::save_machine,
            commands::delete_machine,
            commands::machine_dir,
            commands::preview_command,
            commands::start_machine,
            commands::stop_machine,
            commands::pause_machine,
            commands::resume_machine,
            commands::reset_machine,
            commands::machine_status,
            commands::all_status,
            commands::machine_log,
            commands::attach_media,
            commands::detach_media,
            commands::list_snapshots,
            commands::create_snapshot,
            commands::restore_snapshot,
            commands::delete_snapshot,
            commands::detect_engines,
            commands::missing_bios,
            commands::open_path,
            commands::check_update,
            commands::install_update,
        ])
        .build(tauri::generate_context!())
        .expect("error while building virtual")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                if let Some(st) = app.try_state::<Arc<AppState>>() {
                    stop_all(&st);
                }
            }
        });
}
