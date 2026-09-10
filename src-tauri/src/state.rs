use crate::engine::Running;
use crate::settings::Settings;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct AppState {
    pub settings: Mutex<Settings>,
    /// Laufende Engine-Prozesse je Maschinen-Id.
    pub runtime: Mutex<HashMap<String, Running>>,
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
}

impl AppState {
    /// Ordner mit den Maschinenordnern — Einstellung oder App-Datenordner/machines.
    pub fn machines_dir(&self) -> PathBuf {
        let s = self.settings.lock().unwrap();
        let dir = if s.machines_dir.trim().is_empty() {
            self.data_dir.join("machines")
        } else {
            PathBuf::from(s.machines_dir.trim())
        };
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    pub fn settings_clone(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    /// Ordner mit eigenen Profilen (überlagern die eingebauten per `id`).
    pub fn profiles_dir(&self) -> PathBuf {
        let dir = self.config_dir.join("profiles");
        let _ = std::fs::create_dir_all(&dir);
        dir
    }
}
