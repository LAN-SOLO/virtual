//! Engine-Prozesse: Start, Überwachung, Log, Ereignisse an die Oberfläche.
//! Die eigentliche Steuerung sitzt je Engine in `qemu.rs` (QMP) und
//! `libretro.rs` (RetroArch-UDP-Kommandos).

pub mod libretro;
pub mod qemu;

use serde::Serialize;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tauri::{AppHandle, Emitter};
use virtual_core::MachineStatus;

pub const LOG_LINES: usize = 500;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusEvent {
    pub id: String,
    pub state: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEvent {
    pub id: String,
    pub line: String,
}

/// Steuerkanal je Engine.
pub enum Control {
    Qemu(Arc<Mutex<Option<qemu::QmpClient>>>),
    Libretro { port: u16, paused: Arc<AtomicBool> },
}

/// Ein laufender Engine-Prozess samt Zustand.
pub struct Running {
    pub child: Arc<Mutex<Child>>,
    pub pid: u32,
    /// `starting` | `running` | `paused` | `stopped` | `error`
    pub state: Arc<Mutex<String>>,
    pub error: Arc<Mutex<Option<String>>>,
    pub since: String,
    pub started: Instant,
    pub log: Arc<Mutex<VecDeque<String>>>,
    pub control: Control,
    /// Hilfsprozesse (swtpm), werden mit der Maschine beendet.
    pub helpers: Arc<Mutex<Vec<Child>>>,
    pub dir: PathBuf,
}

impl Running {
    pub fn state(&self) -> String {
        self.state.lock().unwrap().clone()
    }

    pub fn set_state(&self, s: &str) {
        *self.state.lock().unwrap() = s.to_string();
    }

    pub fn status(&self, id: &str) -> MachineStatus {
        MachineStatus {
            id: id.into(),
            state: self.state(),
            pid: Some(self.pid),
            since: Some(self.since.clone()),
            error: self.error.lock().unwrap().clone(),
        }
    }

    /// Prozess hart beenden (plus Hilfsprozesse).
    pub fn kill(&self) {
        let _ = self.child.lock().unwrap().kill();
        for h in self.helpers.lock().unwrap().iter_mut() {
            let _ = h.kill();
        }
    }

    pub fn log_lines(&self) -> Vec<String> {
        self.log.lock().unwrap().iter().cloned().collect()
    }
}

pub fn stopped_status(id: &str) -> MachineStatus {
    MachineStatus { id: id.into(), state: "stopped".into(), pid: None, since: None, error: None }
}

pub fn emit_status(app: &AppHandle, id: &str, state: &str, error: Option<String>) {
    let _ = app.emit("machine-status", StatusEvent { id: id.into(), state: state.into(), error });
}

pub fn push_log(app: &AppHandle, id: &str, dir: &Path, log: &Arc<Mutex<VecDeque<String>>>, line: String) {
    {
        let mut l = log.lock().unwrap();
        if l.len() >= LOG_LINES {
            l.pop_front();
        }
        l.push_back(line.clone());
    }
    crate::store::append_log(dir, &line);
    let _ = app.emit("machine-log", LogEvent { id: id.into(), line });
}

/// Liest stdout/stderr des Kindprozesses zeilenweise in Log und Ereignisse.
pub fn spawn_log_readers(app: &AppHandle, id: &str, dir: &Path, child: &mut Child, log: &Arc<Mutex<VecDeque<String>>>) {
    let streams: Vec<Box<dyn Read + Send>> = [
        child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>),
        child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>),
    ]
    .into_iter()
    .flatten()
    .collect();
    for stream in streams {
        let app = app.clone();
        let id = id.to_string();
        let dir = dir.to_path_buf();
        let log = log.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(stream);
            for line in reader.lines().map_while(Result::ok) {
                push_log(&app, &id, &dir, &log, line);
            }
        });
    }
}

/// Wartet auf das Prozessende und meldet `stopped` bzw. `error`
/// (Abbruch mit Fehlercode in den ersten drei Sekunden = Startfehler).
pub fn spawn_waiter(app: &AppHandle, st: Arc<crate::state::AppState>, id: &str) {
    let app = app.clone();
    let id = id.to_string();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(250));
        let finished = {
            let rt = st.runtime.lock().unwrap();
            let Some(r) = rt.get(&id) else { return };
            let waited = r.child.lock().unwrap().try_wait();
            let result = match waited {
                Ok(Some(status)) => Some((status, r.started.elapsed().as_secs_f32())),
                Ok(None) => None,
                Err(_) => Some((std::process::ExitStatus::default(), 0.0)),
            };
            result
        };
        if let Some((status, secs)) = finished {
            let mut rt = st.runtime.lock().unwrap();
            if let Some(r) = rt.remove(&id) {
                for h in r.helpers.lock().unwrap().iter_mut() {
                    let _ = h.kill();
                }
                // Socket-Reste des QMP-Endpunkts aufräumen
                let _ = std::fs::remove_file(r.dir.join("qmp.sock"));
                let failed = !status.success() && secs < 3.0;
                let err = if failed {
                    let tail: Vec<String> = r.log_lines().into_iter().rev().take(5).collect::<Vec<_>>().into_iter().rev().collect();
                    Some(format!("Engine beendet mit {}: {}", status, tail.join(" | ")))
                } else {
                    None
                };
                let state = if failed { "error" } else { "stopped" };
                emit_status(&app, &id, state, err);
            }
            return;
        }
    });
}

/// Freier TCP-/UDP-Port auf localhost (kurz gebunden und wieder freigegeben).
pub fn free_port(udp: bool) -> Result<u16, String> {
    if udp {
        let s = std::net::UdpSocket::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        Ok(s.local_addr().map_err(|e| e.to_string())?.port())
    } else {
        let l = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        Ok(l.local_addr().map_err(|e| e.to_string())?.port())
    }
}

/// Kurz-Id für Snapshot-Tags.
pub fn short_id() -> String {
    uuid::Uuid::new_v4().to_string()[..8].to_string()
}

pub fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

pub fn set_paused_flag(flag: &Arc<AtomicBool>, v: bool) {
    flag.store(v, Ordering::SeqCst);
}
