//! App-Einstellungen als JSON im Config-Ordner der App (`settings.json`).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::Manager;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// "de" | "en"
    pub language: String,
    /// "dark" | "light"
    pub theme: String,
    /// "blue" | "emerald" | "violet" | "amber"
    pub accent: String,
    pub auto_update: bool,
    /// Tarif „nested“ — im Vorabzugang frei umschaltbar, keine Lizenzprüfung.
    pub nested: bool,
    /// Ordner mit den Maschinenordnern (leer = App-Datenordner/machines).
    pub machines_dir: String,
    /// Pfad-Überschreibungen für Engines (leer = automatisch suchen).
    pub qemu_dir: String,
    pub retroarch_path: String,
    pub cores_dir: String,
    pub system_dir: String,
    /// Vor dem Löschen und harten Stoppen nachfragen (nur UI).
    pub confirm_dangerous: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: if sys_locale_is_german() { "de" } else { "en" }.into(),
            theme: "dark".into(),
            accent: "blue".into(),
            auto_update: false,
            nested: false,
            machines_dir: String::new(),
            qemu_dir: String::new(),
            retroarch_path: String::new(),
            cores_dir: String::new(),
            system_dir: String::new(),
            confirm_dangerous: true,
        }
    }
}

fn sys_locale_is_german() -> bool {
    sys_locale::get_locale()
        .map(|l| l.to_lowercase().starts_with("de"))
        .unwrap_or(false)
}

pub fn config_dir(app: &tauri::AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_config_dir()
        .unwrap_or_else(|_| dirs::config_dir().unwrap_or_default().join("virtual"));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn data_dir(app: &tauri::AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| dirs::data_dir().unwrap_or_default().join("virtual"));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn settings_path(config_dir: &std::path::Path) -> PathBuf {
    config_dir.join("settings.json")
}

pub fn load(config_dir: &std::path::Path) -> Settings {
    std::fs::read_to_string(settings_path(config_dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn store(config_dir: &std::path::Path, settings: &Settings) {
    if let Ok(json) = serde_json::to_string_pretty(settings) {
        let _ = crate::store::write_atomic(&settings_path(config_dir), json.as_bytes());
    }
}
