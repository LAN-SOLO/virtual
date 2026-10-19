//! QEMU als Engine: Prozess starten, über QMP steuern, Snapshots verwalten.

use super::{command, emit_status, free_port, now_iso, short_id, socket_path, spawn_log_readers, Control, Running};
use crate::detect;
use crate::settings::Settings;
use crate::state::AppState;
use serde_json::Value;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
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
    /// Laufende Nummer für eindeutige Antwort-ids.
    seq: u64,
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
        let mut c = QmpClient { reader: BufReader::new(r), writer: w, state, version: String::new(), seq: 0 };
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

    /// Sendet eine Kommandozeile (JSON mit `id`) und liefert das `return` (bis 20 s).
    pub fn send(&mut self, line: &str) -> Result<Value, String> {
        self.send_wait(line, Duration::from_secs(20))
    }

    /// Wie `send`, wartet aber bis `wait` auf die Antwort. Jede Anfrage bekommt eine
    /// eindeutige `id`: Kommt die Antwort eines abgebrochenen Aufrufs später doch noch,
    /// wird sie übersprungen statt dem nächsten Aufruf zugeordnet.
    pub fn send_wait(&mut self, line: &str, wait: Duration) -> Result<Value, String> {
        self.seq += 1;
        let mut msg: Value = serde_json::from_str(line).map_err(|e| format!("QMP-Kommando: {e}"))?;
        let want_id = msg.get("id").and_then(Value::as_str).map(|id| format!("{id}-{}", self.seq));
        if let (Some(obj), Some(id)) = (msg.as_object_mut(), want_id.as_ref()) {
            obj.insert("id".into(), Value::String(id.clone()));
        }
        let line = msg.to_string();
        self.writer.write_all(line.as_bytes()).map_err(|e| format!("QMP senden: {e}"))?;
        self.writer.write_all(b"\n").map_err(|e| format!("QMP senden: {e}"))?;
        self.writer.flush().map_err(|e| format!("QMP senden: {e}"))?;
        let deadline = Instant::now() + wait;
        let mut buf = String::new();
        loop {
            match self.reader.read_line(&mut buf) {
                Ok(0) => return Err("QMP-Verbindung geschlossen".into()),
                Ok(_) => {}
                // Lese-Timeout des Sockets: bis zur Frist weiter warten (savevm mit viel RAM dauert)
                Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) && Instant::now() < deadline => continue,
                Err(e) => return Err(format!("QMP lesen: {e}")),
            }
            let msg = qmp::parse_line(&buf);
            buf.clear();
            match msg {
                Some(qmp::Message::Return { id, value }) if id == want_id => return Ok(value),
                Some(qmp::Message::Error { id, desc, .. }) if id == want_id => return Err(desc),
                Some(qmp::Message::Event { name, .. }) => self.handle_event(&name),
                _ => {}
            }
        }
    }

    /// HMP-Kommando über QMP (`savevm`, `loadvm`, `delvm`, `info snapshots`).
    /// Snapshots schreiben bzw. lesen den ganzen Gast-RAM — dafür bis zu 15 Minuten Zeit.
    pub fn hmp(&mut self, command: &str) -> Result<String, String> {
        let v = self.send_wait(
            &qmp::command("human-monitor-command", Some(serde_json::json!({ "command-line": command })), Some("hmp")),
            Duration::from_secs(15 * 60),
        )?;
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

/// QMP-Endpunkt: Unix-Socket (macOS/Linux, nur für den Benutzer erreichbar) bzw.
/// localhost-TCP unter Windows (Rust kennt dort keine Unix-Sockets).
fn qmp_endpoint(dir: &Path, id: &str) -> Result<String, String> {
    if cfg!(windows) {
        Ok(format!("tcp:127.0.0.1:{}", free_port(false)?))
    } else {
        Ok(socket_path(dir, "qmp.sock", id).to_string_lossy().into_owned())
    }
}

fn tpm_socket(dir: &Path, id: &str) -> PathBuf {
    socket_path(&dir.join("tpm"), "swtpm.sock", id)
}

/// Binary und Host-Kontext für eine Maschine — ohne Seiteneffekte (Vorschau).
pub fn context(m: &Machine, dir: &Path, settings: &Settings, qmp_endpoint: String, tpm_socket: Option<PathBuf>) -> Result<(PathBuf, HostContext), String> {
    let binary_name = m.arch.qemu_binary().ok_or("Maschine ohne QEMU-Architektur")?;
    let bin = detect::find_qemu_binary(m.arch, settings)
        .ok_or_else(|| format!("{binary_name} nicht gefunden — {}", d::install_hint(HostOs::current(), "qemu")))?;
    let host = HostOs::current();
    let accel = d::accel_for(host, d::host_arch(), m.arch).map(String::from);
    let (uefi_code, uefi_vars_template) = if matches!(m.firmware, Firmware::Uefi | Firmware::UefiSecure) {
        // Secure-Boot-Firmware nur, wo sie mit SMM auch startet (TCG/KVM, q35)
        let secure = m.firmware == Firmware::UefiSecure
            && m.machine_type == "q35"
            && matches!(accel.as_deref(), None | Some("kvm"));
        detect::find_uefi(&bin, m.arch, secure)
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
        process_names: host == HostOs::Linux,
        // Homebrew-QEMU (cocoa) ist ohne OpenGL gebaut
        gl: host != HostOs::MacOs,
        smb: detect::smbd_available(),
        virtfs: host != HostOs::Windows,
    };
    Ok((bin, ctx))
}

/// Kommandozeile zur Ansicht.
pub fn preview(m: &Machine, dir: &Path, settings: &Settings) -> Result<String, String> {
    let endpoint = if cfg!(windows) { "tcp:127.0.0.1:4444".to_string() } else { socket_path(dir, "qmp.sock", &m.id).to_string_lossy().into_owned() };
    let tpm = if m.tpm && detect::swtpm_path().is_some() { Some(tpm_socket(dir, &m.id)) } else { None };
    let (bin, ctx) = context(m, dir, settings, endpoint, tpm)?;
    let args = argv::qemu_argv(m, &ctx)?;
    let mut out = argv::render(&bin.to_string_lossy(), &args);
    for n in argv::host_notes(m, &ctx) {
        out.push_str("\n# ");
        out.push_str(&n);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Lebenszyklus
// ---------------------------------------------------------------------------

fn spawn_swtpm(dir: &Path, sock: &Path) -> Option<std::process::Child> {
    let swtpm = detect::swtpm_path()?;
    let tpm_dir = dir.join("tpm");
    std::fs::create_dir_all(&tpm_dir).ok()?;
    let _ = std::fs::remove_file(sock);
    command(swtpm)
        .args([
            "socket",
            "--tpmstate",
            &format!("dir={}", argv::qv(&tpm_dir.to_string_lossy())),
            "--ctrl",
            &format!("type=unixio,path={}", argv::qv(&sock.to_string_lossy())),
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
    let endpoint = qmp_endpoint(dir, &m.id)?;
    let mut sockets = Vec::new();
    if !endpoint.starts_with("tcp:") {
        let _ = std::fs::remove_file(&endpoint);
        sockets.push(PathBuf::from(&endpoint));
    }
    let mut helpers = Vec::new();
    let tpm_socket = if m.tpm {
        let sock = tpm_socket(dir, &m.id);
        match spawn_swtpm(dir, &sock) {
            Some(child) => {
                helpers.push(child);
                // swtpm legt den Socket asynchron an — kurz darauf warten
                let t0 = Instant::now();
                while !sock.exists() && t0.elapsed() < Duration::from_secs(3) {
                    std::thread::sleep(Duration::from_millis(50));
                }
                sockets.push(sock.clone());
                Some(sock)
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
    let args = match argv::qemu_argv(m, &ctx) {
        Ok(a) => a,
        Err(e) => {
            for h in helpers.iter_mut() {
                let _ = h.kill();
            }
            return Err(e);
        }
    };

    crate::store::truncate_log(dir);
    let log = Arc::new(Mutex::new(VecDeque::new()));
    super::push_log(app, &m.id, dir, &log, format!("$ {}", argv::render(&bin.to_string_lossy(), &args)));
    for n in argv::host_notes(m, &ctx) {
        super::push_log(app, &m.id, dir, &log, format!("# {n}"));
    }

    let spawned = command(&bin)
        .args(&args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match spawned {
        Ok(c) => c,
        Err(e) => {
            for h in helpers.iter_mut() {
                let _ = h.kill();
            }
            return Err(format!("QEMU starten fehlgeschlagen: {e}"));
        }
    };
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
        sockets,
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

/// Steuerkommandos (Pause, Reset, Stopp, Statusabfrage) warten höchstens `wait`
/// auf die QMP-Verbindung. Hält ein langer Snapshot (savevm/loadvm, bis 15 min)
/// sie fest, gibt es eine klare Meldung statt eines hängenden Knopfs — und die
/// Statusabfrage alle 4 s stapelt keine blockierten Threads auf.
fn with_qmp_wait<T>(r: &Running, wait: Duration, f: impl FnOnce(&mut QmpClient) -> Result<T, String>) -> Result<T, String> {
    let Control::Qemu(slot) = &r.control else {
        return Err("Keine QEMU-Maschine".into());
    };
    let deadline = Instant::now() + wait;
    loop {
        match slot.try_lock() {
            Ok(mut guard) => {
                let c = guard.as_mut().ok_or("QMP-Steuerung (noch) nicht verbunden")?;
                return f(c);
            }
            Err(std::sync::TryLockError::Poisoned(p)) => {
                let mut guard = p.into_inner();
                let c = guard.as_mut().ok_or("QMP-Steuerung (noch) nicht verbunden")?;
                return f(c);
            }
            Err(std::sync::TryLockError::WouldBlock) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            Err(std::sync::TryLockError::WouldBlock) => return Err(QMP_BUSY.into()),
        }
    }
}

pub const QMP_BUSY: &str = "Engine beschäftigt (Snapshot läuft) — bitte warten";
const CONTROL_WAIT: Duration = Duration::from_secs(3);

/// Zustand frisch aus QEMU holen (query-status).
pub fn refresh(r: &Running) {
    if let Ok(v) = with_qmp_wait(r, Duration::ZERO, |c| c.send(&qmp::query_status())) {
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
    with_qmp_wait(r, CONTROL_WAIT, |c| c.send(&qmp::stop()))?;
    r.set_state("paused");
    Ok(())
}

pub fn resume(r: &Running) -> Result<(), String> {
    with_qmp_wait(r, CONTROL_WAIT, |c| c.send(&qmp::cont()))?;
    r.set_state("running");
    Ok(())
}

pub fn reset(r: &Running) -> Result<(), String> {
    with_qmp_wait(r, CONTROL_WAIT, |c| c.send(&qmp::system_reset())).map(|_| ())
}

/// Sanft: ACPI-Ausschalten, nach 10 s `quit`, danach kill. Hart: sofort `quit` + kill.
pub fn stop(r: &Running, force: bool) -> Result<(), String> {
    if force {
        // Hart heißt sofort — auch während ein Snapshot die Steuerung belegt
        if with_qmp_wait(r, Duration::ZERO, |c| c.send(&qmp::quit())).is_ok() {
            std::thread::sleep(Duration::from_millis(300));
        }
        r.kill();
        return Ok(());
    }
    match with_qmp_wait(r, CONTROL_WAIT, |c| c.send(&qmp::system_powerdown())) {
        Ok(_) => {}
        // Snapshot läuft: nicht hart abbrechen, sondern melden (Stopp erzwingen geht)
        Err(e) if e == QMP_BUSY => return Err(e),
        // ohne Steuerung bleibt nur der harte Weg
        Err(_) => {
            r.kill();
            return Ok(());
        }
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
    let out = command(bin).args(args).output().map_err(|e| format!("qemu-img: {e}"))?;
    if !out.status.success() {
        return Err(format!("qemu-img: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn create_snapshot(m: &Machine, dir: &Path, running: Option<&Running>, settings: &Settings, name: &str, note: &str) -> Result<Snapshot, String> {
    let disk = first_disk(m, dir)?;
    let tag = format!("vs-{}", short_id());
    if running.is_some() && m.isolation.throwaway {
        return Err("Wegwerf-Modus: Änderungen und Snapshots im laufenden Betrieb verfallen beim Beenden — Snapshot bitte bei gestoppter Maschine anlegen oder den Wegwerf-Modus abschalten.".into());
    }
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

#[cfg(all(test, unix))]
mod tests {
    //! Gegen ein echtes QEMU (im PATH bzw. Homebrew): `cargo test -p virtual -- --ignored qemu_live`
    use super::*;
    use virtual_core::profile::builtin_profiles;
    use virtual_core::DiskRef;

    #[test]
    #[ignore]
    fn qemu_live_qmp_and_snapshots() {
        let settings = Settings::default();
        let Some(img) = detect::qemu_img_path(&settings) else { return };
        let dir = std::env::temp_dir().join(format!("virtual-live-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = builtin_profiles().into_iter().find(|p| p.id == "pc-1996-pentium133").unwrap();
        let mut m = p.instantiate("live-1", "Live, Test", "2026-10-19T00:00:00Z");
        m.display.kind = "headless".into();
        m.disks.push(DiskRef { id: "d0".into(), size_gb: 1, ..DiskRef::default() });
        let out = command(&img).args(argv::qemu_img_create(&dir.join("disk-0.qcow2"), "qcow2", 1)).output().unwrap();
        assert!(out.status.success());

        // Offline: qemu-img snapshot -c/-a/-d über die App-Funktionen
        let snap = create_snapshot(&m, &dir, None, &settings, "offline", "").unwrap();
        restore_snapshot(&m, &dir, None, &settings, &snap).unwrap();
        delete_snapshot(&m, &dir, None, &settings, &snap).unwrap();
        assert!(crate::store::load_snapshots(&dir).is_empty());

        // Laufend: QMP stop/cont/reset + HMP savevm/loadvm/delvm
        let endpoint = socket_path(&dir, "qmp.sock", &m.id).to_string_lossy().into_owned();
        let (bin, ctx) = context(&m, &dir, &settings, endpoint.clone(), None).unwrap();
        let args = argv::qemu_argv(&m, &ctx).unwrap();
        let mut child = command(&bin).args(&args).current_dir(&dir).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
        let state = Arc::new(Mutex::new("starting".to_string()));
        let client = QmpClient::connect(&endpoint, state.clone(), Duration::from_secs(10)).expect("QMP");
        let running = Running {
            child: Arc::new(Mutex::new(command("true").spawn().unwrap())),
            pid: child.id(),
            state: state.clone(),
            error: Arc::new(Mutex::new(None)),
            since: now_iso(),
            started: Instant::now(),
            log: Arc::new(Mutex::new(VecDeque::new())),
            control: Control::Qemu(Arc::new(Mutex::new(Some(client)))),
            helpers: Arc::new(Mutex::new(Vec::new())),
            sockets: vec![],
        };
        pause(&running).unwrap();
        refresh(&running);
        assert_eq!(running.state(), "paused");
        resume(&running).unwrap();
        refresh(&running);
        assert_eq!(running.state(), "running");
        reset(&running).unwrap();
        let snap = create_snapshot(&m, &dir, Some(&running), &settings, "live", "mit RAM").unwrap();
        assert_eq!(snap.kind, "internal");
        restore_snapshot(&m, &dir, Some(&running), &settings, &snap).unwrap();
        delete_snapshot(&m, &dir, Some(&running), &settings, &snap).unwrap();
        // Wegwerf-Modus: laufende Snapshots werden abgelehnt
        let mut t = m.clone();
        t.isolation.throwaway = true;
        assert!(create_snapshot(&t, &dir, Some(&running), &settings, "x", "").is_err());
        let _ = with_qmp(&running, |c| c.send(&qmp::quit()));
        let t0 = Instant::now();
        while matches!(child.try_wait(), Ok(None)) && t0.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        let _ = std::fs::remove_dir_all(&dir);
        eprintln!("qemu_live: QMP + Snapshots (offline/laufend) mit {} geprüft", bin.display());
    }
}

#[cfg(test)]
mod qmp_client_tests {
    use super::*;

    fn client(replies: &str, seq: u64) -> QmpClient {
        QmpClient {
            reader: BufReader::new(Box::new(std::io::Cursor::new(replies.as_bytes().to_vec()))),
            writer: Box::new(std::io::sink()),
            state: Arc::new(Mutex::new("running".into())),
            version: String::new(),
            seq,
        }
    }

    #[test]
    fn late_reply_of_an_aborted_call_is_skipped() {
        // Antwort auf den abgebrochenen Aufruf 1 kommt erst jetzt, danach Event und die eigene
        let replies = "{\"return\": \"alt\", \"id\": \"hmp-1\"}\n{\"event\": \"STOP\"}\n{\"return\": \"\", \"id\": \"hmp-2\"}\n";
        let mut c = client(replies, 1);
        assert_eq!(c.hmp("savevm vs-x").unwrap(), "");
        assert_eq!(*c.state.lock().unwrap(), "paused", "Events dazwischen werden verarbeitet");
    }

    fn running_with(client: Option<QmpClient>) -> Running {
        let child = if cfg!(windows) { command("cmd").args(["/C", "exit"]).spawn() } else { command("true").spawn() };
        Running {
            child: Arc::new(Mutex::new(child.unwrap())),
            pid: 0,
            state: Arc::new(Mutex::new("running".into())),
            error: Arc::new(Mutex::new(None)),
            since: now_iso(),
            started: Instant::now(),
            log: Arc::new(Mutex::new(VecDeque::new())),
            control: Control::Qemu(Arc::new(Mutex::new(client))),
            helpers: Arc::new(Mutex::new(Vec::new())),
            sockets: vec![],
        }
    }

    #[test]
    fn control_commands_do_not_hang_behind_a_long_snapshot() {
        let r = running_with(Some(client("{\"return\": {}, \"id\": \"x\"}\n", 0)));
        let Control::Qemu(slot) = &r.control else { unreachable!() };
        let held = slot.lock().unwrap(); // „savevm läuft“
        let t0 = Instant::now();
        assert_eq!(with_qmp_wait(&r, Duration::from_millis(200), |c| c.send(&qmp::stop())).unwrap_err(), QMP_BUSY);
        assert!(t0.elapsed() < Duration::from_secs(2));
        refresh(&r); // kehrt sofort zurück, ändert nichts
        assert_eq!(r.state(), "running");
        // Stopp erzwingen wartet nicht auf die Steuerung
        let t0 = Instant::now();
        stop(&r, true).unwrap();
        assert!(t0.elapsed() < Duration::from_secs(2));
        drop(held);
        let r = running_with(None);
        assert!(pause(&r).unwrap_err().contains("nicht verbunden"));
    }

    #[test]
    fn hmp_errors_and_closed_connection() {
        let mut c = client("{\"return\": \"Error: no snapshot\", \"id\": \"hmp-1\"}\n", 0);
        assert!(c.hmp("loadvm x").is_err());
        let mut c = client("", 0);
        assert!(c.send(&qmp::query_status()).unwrap_err().contains("geschlossen"));
    }
}
