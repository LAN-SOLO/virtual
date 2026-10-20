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
    /// Kurz: Hardware speichern / Medium entfernen (schreibt machine.json)
    Edit,
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
        Op::Edit => "Maschine wird gerade gespeichert".into(),
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


    /// Läuft gerade ein Vorgang (Snapshot, Wandlung, Löschen, Start, Anlegen)?
    pub fn any_busy(&self) -> bool {
        !self.busy.lock().unwrap().is_empty()
    }

    /// Wartet, bis kein Vorgang mehr läuft — höchstens `max`. `true` = alle fertig.
    /// Beenden und Update warten so auf ein laufendes `qemu-img` (Snapshot einer
    /// gestoppten Maschine) bzw. eine Wandlung, statt es mitten im Schreiben abzuschneiden.
    pub fn wait_idle(&self, max: std::time::Duration) -> bool {
        let t0 = std::time::Instant::now();
        loop {
            if !self.any_busy() {
                return true;
            }
            if t0.elapsed() >= max {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
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
        // Speichern/Entfernen während einer Wandlung: abgelehnt, sonst überschriebe die
        // Wandlung die Änderung am Ende mit ihrem älteren Stand
        let conv = st.begin("a", Op::Media).unwrap();
        assert!(st.begin("a", Op::Edit).err().expect("belegt").contains("Medium"));
        drop(conv);
        assert!(st.begin("a", Op::Edit).is_ok());
    }

    #[test]
    fn quit_waits_for_running_operations() {
        use std::sync::Arc;
        use std::time::{Duration, Instant};
        let st = Arc::new(state());
        assert!(st.wait_idle(Duration::from_millis(10)), "nichts belegt: sofort fertig");
        // Vorgang endet nach 300 ms in einem anderen Thread → Warten endet danach
        st.busy.lock().unwrap().insert("a".into(), Op::Snapshot);
        let st2 = st.clone();
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            st2.busy.lock().unwrap().remove("a");
        });
        let t0 = Instant::now();
        assert!(st.wait_idle(Duration::from_secs(5)));
        assert!(t0.elapsed() >= Duration::from_millis(250));
        t.join().unwrap();
        // Hängt der Vorgang, gibt das Warten nach der Frist auf
        let _g = st.begin("b", Op::Media).unwrap();
        let t0 = Instant::now();
        assert!(!st.wait_idle(Duration::from_millis(300)));
        assert!(t0.elapsed() < Duration::from_secs(2));
    }
}
