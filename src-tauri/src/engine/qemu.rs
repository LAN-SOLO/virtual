//! QEMU als Engine: Prozess starten, über QMP steuern, Snapshots verwalten.

use super::{emit_status, free_port, now_iso, short_id, spawn_log_readers, Control, Running};
use crate::detect;
use crate::settings::Settings;
use crate::state::AppState;
use serde_json::Value;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::AppHandle;
use virtual_core::argv::{self, HostContext};
use virtual_core::detect::{self as d, HostOs};
use virtual_core::qmp;
use virtual_core::{Engine, Firmware, Machine, Snapshot};

// ---------------------------------------------------------------------------
// QMP-Client (synchron, eine Verbindung je Maschine)
// ---------------------------------------------------------------------------

pub struct QmpClient {
    reader: BufReader<Box<dyn Read + Send>>,
    writer: Box<dyn Write + Send>,
    state: Arc<Mutex<String>>,
    pub version: String,
}

fn open_stream(endpoint: &str) -> Result<(Box<dyn Read + Send>, Box<dyn Write + Send>), String> {
    let timeout = Some(Duration::from_secs(20));
    if let Some(addr) = endpoint.strip_prefix("tcp:") {
        let s = std::net::TcpStream::connect(addr).map_err(|e| e.to_string())?;
        s.set_read_timeout(timeout).ok();
        let w = s.try_clone().map_err(|e| e.to_string())?;
        return Ok((Box::new(s), Box::new(w)));
    }
    #[cfg(unix)]
    {
        let s = std::os::unix::net::UnixStream::connect(endpoint).map_err(|e| e.to_string())?;
        s.set_read_timeout(timeout).ok();
        let w = s.try_clone().map_err(|e| e.to_string())?;
        Ok((Box::new(s), Box::new(w)))
    }
    #[cfg(not(unix))]
    {
        Err(format!("Unix-Socket nicht verfügbar: {endpoint}"))
    }
}

impl QmpClient {
    /// Verbindet (mit Wiederholung, bis QEMU den Socket geöffnet hat) und
    /// handelt die Fähigkeiten aus.
    pub fn connect(endpoint: &str, state: Arc<Mutex<String>>, wait: Duration) -> Result<QmpClient, String> {
        let started = Instant::now();
        let (r, w) = loop {
            match open_stream(endpoint) {
                Ok(pair) => break pair,
                Err(e) if started.elapsed() < wait => {
                    let _ = e;
                    std::thread::sleep(Duration::from_millis(150));
                }
                Err(e) => return Err(format!("QMP-Verbindung fehlgeschlagen: {e}")),
            }
        };
        let mut c = QmpClient { reader: BufReader::new(r), writer: w, state, version: String::new() };
        // Begrüßung lesen
        let mut line = String::new();
        c.reader.read_line(&mut line).map_err(|e| format!("QMP-Begrüßung: {e}"))?;
        if let Some(qmp::Message::Greeting { version }) = qmp::parse_line(&line) {
            c.version = version;
        }
        c.send(&qmp::capabilities())?;
        Ok(c)
    }

    fn handle_event(&self, name: &str) {
        let mut s = self.state.lock().unwrap();
        match name {
            "STOP" => *s = "paused".into(),
            "RESUME" => *s = "running".into(),
            "SHUTDOWN" | "POWERDOWN" => {}
            _ => {}
        }
    }

    /// Sendet eine Kommandozeile (JSON mit `id`) und liefert das `return`.
    pub fn send(&mut self, line: &str) -> Result<Value, String> {
        let want_id: Option<String> = serde_json::from_str::<Value>(line)
            .ok()
            .and_then(|v| v.get("id").and_then(Value::as_str).map(String::from));
        self.writer.write_all(line.as_bytes()).map_err(|e| format!("QMP senden: {e}"))?;
        self.writer.write_all(b"\n").map_err(|e| format!("QMP senden: {e}"))?;
        self.writer.flush().map_err(|e| format!("QMP senden: {e}"))?;
        loop {
            let mut buf = String::new();
            let n = self.reader.read_line(&mut buf).map_err(|e| format!("QMP lesen: {e}"))?;
            if n == 0 {
                return Err("QMP-Verbindung geschlossen".into());
            }
            match qmp::parse_line(&buf) {
                Some(qmp::Message::Return { id, value }) if id == want_id => return Ok(value),
                Some(qmp::Message::Error { id, desc, .. }) if id == want_id => return Err(desc),
                Some(qmp::Message::Event { name, .. }) => self.handle_event(&name),
                _ => {}
            }
        }
    }

    /// HMP-Kommando über QMP (`savevm`, `loadvm`, `delvm`, `info snapshots`).
    pub fn hmp(&mut self, command: &str) -> Result<String, String> {
        let v = self.send(&qmp::command(
            "human-monitor-command",
            Some(serde_json::json!({ "command-line": command })),
            Some("hmp"),
        ))?;
        let text = v.as_str().unwrap_or("").trim().to_string();
        if text.to_lowercase().starts_with("error") || text.contains("failed") {
            return Err(text);
        }
        Ok(text)
    }
}

// ---------------------------------------------------------------------------
// Kontext & Kommandozeile
// ---------------------------------------------------------------------------

fn qmp_endpoint(dir: &Path) -> Result<String, String> {
    if cfg!(windows) {
        Ok(format!("tcp:127.0.0.1:{}", free_port(false)?))
    } else {
        Ok(dir.join("qmp.sock").to_string_lossy().into_owned())
    }
}

/// Binary und Host-Kontext für eine Maschine — ohne Seiteneffekte (Vorschau).
pub fn context(m: &Machine, dir: &Path, settings: &Settings, qmp_endpoint: String, tpm_socket: Option<PathBuf>) -> Result<(PathBuf, HostContext), String> {
    let binary_name = m.arch.qemu_binary().ok_or("Maschine ohne QEMU-Architektur")?;
    let bin = detect::find_qemu_binary(m.arch, settings)
        .ok_or_else(|| format!("{binary_name} nicht gefunden — {}", d::install_hint(HostOs::current(), "qemu")))?;
    let host = HostOs::current();
    let accel = d::accel_for(host, d::host_arch(), m.arch).map(String::from);
    let (uefi_code, uefi_vars_template) = if matches!(m.firmware, Firmware::Uefi | Firmware::UefiSecure) {
        detect::find_uefi(&bin, m.arch)
    } else {
        (None, None)
    };
    let uefi_vars = if uefi_code.is_some() {
        let nvram = dir.join("nvram.fd");
        if !nvram.exists() {
            match &uefi_vars_template {
                Some(t) => {
                    std::fs::copy(t, &nvram).map_err(|e| format!("NVRAM anlegen: {e}"))?;
                }
                None => {
                    // Ohne Vorlage: leere 64-MiB-Datei (Größe der edk2-Builds für aarch64)
                    let size = if m.arch == virtual_core::Arch::Aarch64 { 64 * 1024 * 1024 } else { 540672 };
                    std::fs::write(&nvram, vec![0u8; size]).map_err(|e| format!("NVRAM anlegen: {e}"))?;
                }
            }
        }
        Some(nvram)
    } else {
        None
    };
    let ctx = HostContext {
        machine_dir: dir.to_path_buf(),
        accel,
        qmp: qmp_endpoint,
        uefi_code,
        uefi_vars,
        tpm_socket,
        display_backend: d::display_backend(host).into(),
        audio_backend: d::audio_backend(host).into(),
    };
    Ok((bin, ctx))
}

/// Kommandozeile zur Ansicht.
pub fn preview(m: &Machine, dir: &Path, settings: &Settings) -> Result<String, String> {
    let endpoint = if cfg!(windows) { "tcp:127.0.0.1:4444".to_string() } else { dir.join("qmp.sock").to_string_lossy().into_owned() };
    let tpm = if m.tpm && detect::swtpm_path().is_some() { Some(dir.join("tpm").join("sock")) } else { None };
    let (bin, ctx) = context(m, dir, settings, endpoint, tpm)?;
    let args = argv::qemu_argv(m, &ctx)?;
    Ok(argv::render(&bin.to_string_lossy(), &args))
}

// ---------------------------------------------------------------------------
// Lebenszyklus
// ---------------------------------------------------------------------------

fn spawn_swtpm(dir: &Path) -> Option<std::process::Child> {
    let swtpm = detect::swtpm_path()?;
    let tpm_dir = dir.join("tpm");
    std::fs::create_dir_all(&tpm_dir).ok()?;
    let sock = tpm_dir.join("sock");
    let _ = std::fs::remove_file(&sock);
    Command::new(swtpm)
        .args([
            "socket",
            "--tpmstate",
            &format!("dir={}", tpm_dir.to_string_lossy()),
            "--ctrl",
            &format!("type=unixio,path={}", sock.to_string_lossy()),
            "--tpm2",
            "--log",
            "level=0",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()
}

pub fn start(app: &AppHandle, st: &Arc<AppState>, m: &Machine, dir: &Path) -> Result<Running, String> {
    if m.engine != Engine::Qemu {
        return Err("Keine QEMU-Maschine".into());
    }
    let settings = st.settings_clone();
    let endpoint = qmp_endpoint(dir)?;
    if !endpoint.starts_with("tcp:") {
        let _ = std::fs::remove_file(&endpoint);
    }
    let mut helpers = Vec::new();
    let tpm_socket = if m.tpm {
        match spawn_swtpm(dir) {
            Some(child) => {
                helpers.push(child);
                std::thread::sleep(Duration::from_millis(300));
                Some(dir.join("tpm").join("sock"))
            }
            None => None,
        }
    } else {
        None
    };
    let (bin, ctx) = match context(m, dir, &settings, endpoint.clone(), tpm_socket) {
        Ok(x) => x,
        Err(e) => {
            for h in helpers.iter_mut() {
                let _ = h.kill();
            }
            return Err(e);
        }
    };
    let args = argv::qemu_argv(m, &ctx)?;

    crate::store::truncate_log(dir);
    let log = Arc::new(Mutex::new(VecDeque::new()));
    super::push_log(app, &m.id, dir, &log, format!("$ {}", argv::render(&bin.to_string_lossy(), &args)));

    let mut child = Command::new(&bin)
        .args(&args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("QEMU starten fehlgeschlagen: {e}"))?;
    let pid = child.id();
    spawn_log_readers(app, &m.id, dir, &mut child, &log);

    let state = Arc::new(Mutex::new("starting".to_string()));
    let running = Running {
        child: Arc::new(Mutex::new(child)),
        pid,
        state: state.clone(),
        error: Arc::new(Mutex::new(None)),
        since: now_iso(),
        started: Instant::now(),
        log,
        control: Control::Qemu(Arc::new(Mutex::new(None))),
        helpers: Arc::new(Mutex::new(helpers)),
        dir: dir.to_path_buf(),
    };
    emit_status(app, &m.id, "starting", None);

    // QMP verbinden — im Hintergrund, damit der Start nicht blockiert.
    if let Control::Qemu(slot) = &running.control {
        let slot = slot.clone();
        let app = app.clone();
        let id = m.id.clone();
        let child = running.child.clone();
        let error = running.error.clone();
        std::thread::spawn(move || match QmpClient::connect(&endpoint, state.clone(), Duration::from_secs(8)) {
            Ok(mut c) => {
                if let Ok(v) = c.send(&qmp::query_status()) {
                    let s = qmp::status_from_return(&v);
                    *state.lock().unwrap() = if s == "paused" { "paused".into() } else { "running".into() };
                }
                *slot.lock().unwrap() = Some(c);
                let s = state.lock().unwrap().clone();
                emit_status(&app, &id, &s, None);
            }
            Err(e) => {
                let alive = matches!(child.lock().unwrap().try_wait(), Ok(None));
                if alive {
                    // Prozess läuft, aber keine Steuerung — trotzdem als laufend führen
                    *state.lock().unwrap() = "running".into();
                    *error.lock().unwrap() = Some(format!("Keine QMP-Steuerung: {e}"));
                    emit_status(&app, &id, "running", Some(format!("Keine QMP-Steuerung: {e}")));
                }
            }
        });
    }
    Ok(running)
}

fn with_qmp<T>(r: &Running, f: impl FnOnce(&mut QmpClient) -> Result<T, String>) -> Result<T, String> {
    match &r.control {
        Control::Qemu(slot) => {
            let mut guard = slot.lock().unwrap();
            let c = guard.as_mut().ok_or("QMP-Steuerung (noch) nicht verbunden")?;
            f(c)
        }
        _ => Err("Keine QEMU-Maschine".into()),
    }
}

/// Zustand frisch aus QEMU holen (query-status).
pub fn refresh(r: &Running) {
    if let Ok(v) = with_qmp(r, |c| c.send(&qmp::query_status())) {
        let s = qmp::status_from_return(&v);
        let mapped = match s.as_str() {
            "paused" | "prelaunch" | "suspended" => "paused",
            "shutdown" | "guest-panicked" => "stopped",
            _ => "running",
        };
        r.set_state(mapped);
    }
}

pub fn pause(r: &Running) -> Result<(), String> {
    with_qmp(r, |c| c.send(&qmp::stop()))?;
    r.set_state("paused");
    Ok(())
}

pub fn resume(r: &Running) -> Result<(), String> {
    with_qmp(r, |c| c.send(&qmp::cont()))?;
    r.set_state("running");
    Ok(())
}

pub fn reset(r: &Running) -> Result<(), String> {
    with_qmp(r, |c| c.send(&qmp::system_reset())).map(|_| ())
}

/// Sanft: ACPI-Ausschalten, nach 10 s `quit`, danach kill. Hart: sofort `quit` + kill.
pub fn stop(r: &Running, force: bool) -> Result<(), String> {
    if force {
        let _ = with_qmp(r, |c| c.send(&qmp::quit()));
        std::thread::sleep(Duration::from_millis(300));
        r.kill();
        return Ok(());
    }
    if with_qmp(r, |c| c.send(&qmp::system_powerdown())).is_err() {
        // ohne Steuerung bleibt nur der harte Weg
        r.kill();
        return Ok(());
    }
    let child = r.child.clone();
    let control = match &r.control {
        Control::Qemu(slot) => Some(slot.clone()),
        _ => None,
    };
    let helpers = r.helpers.clone();
    std::thread::spawn(move || {
        for _ in 0..40 {
            std::thread::sleep(Duration::from_millis(250));
            if !matches!(child.lock().unwrap().try_wait(), Ok(None)) {
                return;
            }
        }
        if let Some(slot) = control {
            if let Some(c) = slot.lock().unwrap().as_mut() {
                let _ = c.send(&qmp::quit());
            }
        }
        std::thread::sleep(Duration::from_millis(500));
        let _ = child.lock().unwrap().kill();
        for h in helpers.lock().unwrap().iter_mut() {
            let _ = h.kill();
        }
    });
    Ok(())
}

// ---------------------------------------------------------------------------
// Snapshots — laufend über HMP (savevm/loadvm/delvm), sonst qemu-img
// ---------------------------------------------------------------------------

fn first_disk(m: &Machine, dir: &Path) -> Result<PathBuf, String> {
    let d = m.disks.first().ok_or("Maschine hat keine Platte — Snapshots brauchen eine qcow2-Platte")?;
    if d.format != "qcow2" {
        return Err(format!("Snapshots brauchen qcow2, die Platte ist {}", d.format));
    }
    let p = Path::new(&d.path);
    Ok(if p.is_absolute() { p.to_path_buf() } else { dir.join(p) })
}

fn qemu_img(settings: &Settings, args: &[&str]) -> Result<String, String> {
    let bin = detect::qemu_img_path(settings).ok_or("qemu-img nicht gefunden")?;
    let out = Command::new(bin).args(args).output().map_err(|e| format!("qemu-img: {e}"))?;
    if !out.status.success() {
        return Err(format!("qemu-img: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn create_snapshot(m: &Machine, dir: &Path, running: Option<&Running>, settings: &Settings, name: &str, note: &str) -> Result<Snapshot, String> {
    let disk = first_disk(m, dir)?;
    let tag = format!("vs-{}", short_id());
    match running {
        Some(r) => {
            with_qmp(r, |c| c.hmp(&format!("savevm {tag}")))?;
        }
        None => {
            qemu_img(settings, &["snapshot", "-c", &tag, &disk.to_string_lossy()])?;
        }
    }
    let mut snaps = crate::store::load_snapshots(dir);
    let parent = snaps.last().map(|s| s.id.clone());
    let snap = Snapshot {
        id: uuid::Uuid::new_v4().to_string(),
        machine_id: m.id.clone(),
        parent_id: parent,
        name: if name.trim().is_empty() { tag.clone() } else { name.trim().into() },
        note: note.into(),
        created_at: now_iso(),
        kind: "internal".into(),
        tag,
    };
    snaps.push(snap.clone());
    crate::store::save_snapshots(dir, &snaps)?;
    Ok(snap)
}

pub fn restore_snapshot(m: &Machine, dir: &Path, running: Option<&Running>, settings: &Settings, snap: &Snapshot) -> Result<(), String> {
    let disk = first_disk(m, dir)?;
    match running {
        Some(r) => {
            with_qmp(r, |c| c.hmp(&format!("loadvm {}", snap.tag)))?;
            r.set_state("running");
            Ok(())
        }
        None => qemu_img(settings, &["snapshot", "-a", &snap.tag, &disk.to_string_lossy()]).map(|_| ()),
    }
}

pub fn delete_snapshot(m: &Machine, dir: &Path, running: Option<&Running>, settings: &Settings, snap: &Snapshot) -> Result<(), String> {
    let disk = first_disk(m, dir)?;
    match running {
        Some(r) => with_qmp(r, |c| c.hmp(&format!("delvm {}", snap.tag))).map(|_| ())?,
        None => {
            qemu_img(settings, &["snapshot", "-d", &snap.tag, &disk.to_string_lossy()])?;
        }
    }
    let snaps: Vec<Snapshot> = crate::store::load_snapshots(dir).into_iter().filter(|s| s.id != snap.id).collect();
    crate::store::save_snapshots(dir, &snaps)
}
