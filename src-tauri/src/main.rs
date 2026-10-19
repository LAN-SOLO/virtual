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
use tauri::{Emitter, Manager};

/// Alle laufenden Engines hart beenden (App-Ende).
fn stop_all(st: &Arc<AppState>) {
    let mut rt = st.runtime.lock().unwrap();
    for (_, r) in rt.drain() {
        r.kill();
    }
}

fn any_running(app: &tauri::AppHandle) -> bool {
    app.try_state::<Arc<AppState>>()
        .map(|st| !st.runtime.lock().unwrap().is_empty())
        .unwrap_or(false)
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
            commands::quit_app,
        ])
        // Schließen mit laufenden Maschinen: erst nachfragen (die Oberfläche zeigt den
        // Dialog und ruft `quit_app`), statt Gäste kommentarlos hart zu beenden.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if any_running(window.app_handle()) {
                    api.prevent_close();
                    let _ = window.emit("close-requested", ());
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building virtual")
        .run(|app, event| match event {
            // Cmd+Q / Menü „Beenden“: wie Fenster schließen; `quit_app` beendet mit Code 0
            tauri::RunEvent::ExitRequested { api, code: None, .. } if any_running(app) => {
                api.prevent_exit();
                let _ = app.emit("close-requested", ());
            }
            tauri::RunEvent::Exit => {
                if let Some(st) = app.try_state::<Arc<AppState>>() {
                    stop_all(&st);
                }
            }
            _ => {}
        });
}
