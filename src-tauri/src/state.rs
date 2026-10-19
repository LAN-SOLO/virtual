use crate::engine::Running;
use crate::settings::Settings;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct AppState {
    pub settings: Mutex<Settings>,
    /// Laufende Engine-Prozesse je Maschinen-Id.
    pub runtime: Mutex<HashMap<String, std::sync::Arc<Running>>>,
    /// Laufende Vorgänge je Maschine (Start, Snapshot, Medium wandeln, Löschen) —
    /// ein zweiter Vorgang derselben Maschine wird abgelehnt, statt sich mit dem
    /// ersten um Platte, Socket oder Ordner zu streiten (Doppelklick, Wechsel der
    /// Auswahl während eines langen Snapshots, Löschen während der Wandlung).
    pub busy: Mutex<HashMap<String, Op>>,
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
}

/// Art eines laufenden Vorgangs (für die Fehlermeldung beim zweiten Versuch).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Start,
    Snapshot,
    Media,
    Delete,
}

/// Belegt die Maschine für einen Vorgang; gibt sie beim Drop wieder frei
/// (auch bei Fehlern und Panics im Vorgang).
pub struct BusyGuard<'a> {
    state: &'a AppState,
    id: String,
}

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.state.busy.lock().unwrap().remove(&self.id);
    }
}

/// Fehlertext für einen belegten Vorgang (die Oberfläche übersetzt ihn).
pub fn busy_message(op: Op) -> String {
    match op {
        Op::Start => "Maschine startet bereits".into(),
        Op::Snapshot => "Für diese Maschine läuft gerade ein Snapshot — bitte warten".into(),
        Op::Media => "Für diese Maschine wird gerade ein Medium vorbereitet — bitte warten".into(),
        Op::Delete => "Maschine wird gerade gelöscht".into(),
    }
}

impl AppState {
    /// Maschine für `op` belegen — Fehler, wenn schon ein Vorgang läuft.
    pub fn begin(&self, id: &str, op: Op) -> Result<BusyGuard<'_>, String> {
        let mut busy = self.busy.lock().unwrap();
        if let Some(cur) = busy.get(id) {
            return Err(busy_message(*cur));
        }
        busy.insert(id.to_string(), op);
        Ok(BusyGuard { state: self, id: id.to_string() })
    }


    /// Ordner mit den Maschinenordnern — Einstellung oder App-Datenordner/machines.
    pub fn machines_dir(&self) -> PathBuf {
        let s = self.settings.lock().unwrap();
        let custom = virtual_core::fsutil::normalize_user_path(&s.machines_dir, cfg!(windows));
        let dir = if custom.is_empty() { self.data_dir.join("machines") } else { PathBuf::from(custom) };
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

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> AppState {
        AppState {
            settings: Mutex::new(Settings::default()),
            runtime: Mutex::new(HashMap::new()),
            busy: Mutex::new(HashMap::new()),
            config_dir: std::env::temp_dir(),
            data_dir: std::env::temp_dir(),
        }
    }

    #[test]
    fn second_operation_on_the_same_machine_is_refused() {
        let st = state();
        let g = st.begin("a", Op::Snapshot).unwrap();
        assert!(st.begin("a", Op::Start).err().expect("belegt").contains("Snapshot"));
        assert!(st.begin("a", Op::Delete).is_err());
        let other = st.begin("b", Op::Start).expect("andere Maschine bleibt frei");
        drop(g);
        assert!(st.begin("a", Op::Delete).is_ok(), "nach dem Vorgang wieder frei");
        assert_eq!(st.begin("b", Op::Start).err().expect("belegt"), busy_message(Op::Start));
        drop(other);
    }
}
