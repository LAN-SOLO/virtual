//! RetroArch als Engine für Konsolen: Core und BIOS prüfen, Konfiguration je
//! Maschine schreiben, Prozess starten, über das UDP-Kommando-Interface steuern.
//! Save States sind hier die Snapshots (ein Slot je Snapshot).

use super::{emit_status, free_port, now_iso, set_paused_flag, spawn_log_readers, Control, Running};
use crate::detect;
use crate::settings::Settings;
use crate::state::AppState;
use std::collections::VecDeque;
use std::net::UdpSocket;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::AppHandle;
use virtual_core::detect::{self as d, HostOs};
use virtual_core::libretro::{self, cmd, LibretroContext};
use virtual_core::{Engine, Machine, MediaKind, Snapshot};

fn context(dir: &Path, settings: &Settings, port: u16) -> LibretroContext {
    LibretroContext {
        machine_dir: dir.to_path_buf(),
        cores_dir: detect::cores_dir(settings),
        system_dir: detect::system_dir(settings),
        cmd_port: port,
        core_ext: d::core_ext(HostOs::current()).into(),
    }
}

/// Fehlende BIOS-Dateien einer Konsolen-Maschine.
pub fn missing_bios(m: &Machine, dir: &Path, settings: &Settings) -> Vec<String> {
    match &m.libretro {
        Some(cfg) => libretro::missing_bios(cfg, &context(dir, settings, 0), |p| p.exists()),
        None => Vec::new(),
    }
}

fn rom_stem(m: &Machine) -> Option<String> {
    m.media
        .iter()
        .find(|x| x.kind == MediaKind::Rom)
        .and_then(|r| Path::new(&r.path).file_stem().map(|s| s.to_string_lossy().into_owned()))
}

/// Kommandozeile zur Ansicht (ohne Seiteneffekte).
pub fn preview(m: &Machine, dir: &Path, settings: &Settings) -> Result<String, String> {
    let cfg = m.libretro.as_ref().ok_or("Keine Konsolen-Maschine")?;
    let ctx = context(dir, settings, 55355);
    let bin = detect::retroarch_path(settings)
        .ok_or_else(|| format!("retroarch nicht gefunden — {}", d::install_hint(HostOs::current(), "retroarch")))?;
    let core = libretro::resolve_core(cfg, &ctx, |p| p.exists())?;
    let args = libretro::retroarch_argv(m, &ctx, &core, &dir.join("retroarch.cfg"))?;
    Ok(virtual_core::argv::render(&bin.to_string_lossy(), &args))
}

pub fn start(app: &AppHandle, st: &Arc<AppState>, m: &Machine, dir: &Path) -> Result<Running, String> {
    if m.engine != Engine::Libretro {
        return Err("Keine Konsolen-Maschine".into());
    }
    let cfg = m.libretro.as_ref().ok_or("Konsolen-Maschine ohne libretro-Konfiguration")?;
    let settings = st.settings_clone();
    let bin = detect::retroarch_path(&settings)
        .ok_or_else(|| format!("retroarch nicht gefunden — {}", d::install_hint(HostOs::current(), "retroarch")))?;
    let port = free_port(true)?;
    let ctx = context(dir, &settings, port);
    let core = libretro::resolve_core(cfg, &ctx, |p| p.exists())?;
    let missing = libretro::missing_bios(cfg, &ctx, |p| p.exists());
    if !missing.is_empty() {
        return Err(format!(
            "BIOS-Dateien fehlen im System-Ordner {}: {}",
            ctx.system_dir.to_string_lossy(),
            missing.join(", ")
        ));
    }
    for sub in ["saves", "states", "screenshots"] {
        let _ = std::fs::create_dir_all(dir.join(sub));
    }
    let cfg_path = dir.join("retroarch.cfg");
    std::fs::write(&cfg_path, libretro::machine_config(m, &ctx)).map_err(|e| format!("retroarch.cfg schreiben: {e}"))?;
    let args = libretro::retroarch_argv(m, &ctx, &core, &cfg_path)?;

    crate::store::truncate_log(dir);
    let log = Arc::new(Mutex::new(VecDeque::new()));
    super::push_log(app, &m.id, dir, &log, format!("$ {}", virtual_core::argv::render(&bin.to_string_lossy(), &args)));

    let mut child = Command::new(&bin)
        .args(&args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("RetroArch starten fehlgeschlagen: {e}"))?;
    let pid = child.id();
    spawn_log_readers(app, &m.id, dir, &mut child, &log);

    let state = Arc::new(Mutex::new("running".to_string()));
    let running = Running {
        child: Arc::new(Mutex::new(child)),
        pid,
        state,
        error: Arc::new(Mutex::new(None)),
        since: now_iso(),
        started: Instant::now(),
        log,
        control: Control::Libretro { port, paused: Arc::new(AtomicBool::new(false)) },
        helpers: Arc::new(Mutex::new(Vec::new())),
        dir: dir.to_path_buf(),
    };
    emit_status(app, &m.id, "running", None);
    Ok(running)
}

fn port_of(r: &Running) -> Result<(u16, Arc<AtomicBool>), String> {
    match &r.control {
        Control::Libretro { port, paused } => Ok((*port, paused.clone())),
        _ => Err("Keine Konsolen-Maschine".into()),
    }
}

fn send_cmd(port: u16, command: &str) -> Result<(), String> {
    let s = UdpSocket::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    s.send_to(command.as_bytes(), ("127.0.0.1", port)).map_err(|e| format!("RetroArch-Kommando: {e}"))?;
    Ok(())
}

/// Kommando mit Antwort (GET_STATUS).
fn query(port: u16, command: &str) -> Option<String> {
    let s = UdpSocket::bind("127.0.0.1:0").ok()?;
    s.set_read_timeout(Some(Duration::from_millis(600))).ok()?;
    s.send_to(command.as_bytes(), ("127.0.0.1", port)).ok()?;
    let mut buf = [0u8; 1024];
    let (n, _) = s.recv_from(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf[..n]).into_owned())
}

/// Zustand frisch aus RetroArch holen.
pub fn refresh(r: &Running) {
    if let Ok((port, paused)) = port_of(r) {
        if let Some(reply) = query(port, cmd::GET_STATUS) {
            match libretro::parse_status(&reply) {
                Some("paused") => {
                    set_paused_flag(&paused, true);
                    r.set_state("paused");
                }
                Some("running") => {
                    set_paused_flag(&paused, false);
                    r.set_state("running");
                }
                _ => {}
            }
        }
    }
}

pub fn pause(r: &Running) -> Result<(), String> {
    let (port, paused) = port_of(r)?;
    if !paused.load(Ordering::SeqCst) {
        send_cmd(port, cmd::PAUSE_TOGGLE)?;
        set_paused_flag(&paused, true);
    }
    r.set_state("paused");
    Ok(())
}

pub fn resume(r: &Running) -> Result<(), String> {
    let (port, paused) = port_of(r)?;
    if paused.load(Ordering::SeqCst) {
        send_cmd(port, cmd::PAUSE_TOGGLE)?;
        set_paused_flag(&paused, false);
    }
    r.set_state("running");
    Ok(())
}

pub fn reset(r: &Running) -> Result<(), String> {
    let (port, _) = port_of(r)?;
    send_cmd(port, cmd::RESET)
}

pub fn stop(r: &Running, force: bool) -> Result<(), String> {
    if force {
        r.kill();
        return Ok(());
    }
    let (port, _) = port_of(r)?;
    if send_cmd(port, cmd::QUIT).is_err() {
        r.kill();
        return Ok(());
    }
    let child = r.child.clone();
    std::thread::spawn(move || {
        for _ in 0..20 {
            std::thread::sleep(Duration::from_millis(250));
            if !matches!(child.lock().unwrap().try_wait(), Ok(None)) {
                return;
            }
        }
        let _ = child.lock().unwrap().kill();
    });
    Ok(())
}

// ---------------------------------------------------------------------------
// Save States als Snapshots
// ---------------------------------------------------------------------------

fn state_file(m: &Machine, dir: &Path, slot: u32) -> Option<PathBuf> {
    rom_stem(m).map(|stem| dir.join("states").join(libretro::state_filename(&stem, slot)))
}

/// Wechselt den Slot (RetroArch kennt nur +/−) und merkt sich den neuen Stand.
fn goto_slot(port: u16, m: &mut Machine, target: u32) -> Result<(), String> {
    let cfg = m.libretro.as_mut().ok_or("Keine Konsolen-Maschine")?;
    for c in libretro::slot_commands(cfg.state_slot, target) {
        send_cmd(port, c)?;
        std::thread::sleep(Duration::from_millis(40));
    }
    cfg.state_slot = target;
    Ok(())
}

/// Erstellt einen Save State im nächsten freien Slot. `m` wird (Slot) aktualisiert.
pub fn create_snapshot(m: &mut Machine, dir: &Path, running: Option<&Running>, name: &str, note: &str) -> Result<Snapshot, String> {
    let r = running.ok_or("Konsole muss laufen, um einen Spielstand zu sichern")?;
    let (port, _) = port_of(r)?;
    let mut snaps = crate::store::load_snapshots(dir);
    let slot = snaps.iter().filter_map(|s| s.tag.parse::<u32>().ok()).max().map(|x| x + 1).unwrap_or(0);
    goto_slot(port, m, slot)?;
    send_cmd(port, cmd::SAVE_STATE)?;
    std::thread::sleep(Duration::from_millis(400));
    if let Some(f) = state_file(m, dir, slot) {
        if !f.exists() {
            return Err(format!("Save State wurde nicht geschrieben ({}) — läuft ein Spiel?", f.to_string_lossy()));
        }
    }
    let snap = Snapshot {
        id: uuid::Uuid::new_v4().to_string(),
        machine_id: m.id.clone(),
        parent_id: snaps.last().map(|s| s.id.clone()),
        name: if name.trim().is_empty() { format!("Slot {slot}") } else { name.trim().into() },
        note: note.into(),
        created_at: now_iso(),
        kind: "state".into(),
        tag: slot.to_string(),
    };
    snaps.push(snap.clone());
    crate::store::save_snapshots(dir, &snaps)?;
    Ok(snap)
}

pub fn restore_snapshot(m: &mut Machine, running: Option<&Running>, snap: &Snapshot) -> Result<(), String> {
    let r = running.ok_or("Konsole muss laufen, um einen Spielstand zu laden")?;
    let (port, paused) = port_of(r)?;
    let slot: u32 = snap.tag.parse().map_err(|_| "Ungültiger Slot")?;
    goto_slot(port, m, slot)?;
    send_cmd(port, cmd::LOAD_STATE)?;
    set_paused_flag(&paused, false);
    r.set_state("running");
    Ok(())
}

pub fn delete_snapshot(m: &Machine, dir: &Path, snap: &Snapshot) -> Result<(), String> {
    if let Ok(slot) = snap.tag.parse::<u32>() {
        if let Some(f) = state_file(m, dir, slot) {
            let _ = std::fs::remove_file(&f);
            let _ = std::fs::remove_file(f.with_extension(format!("{}.auto", f.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default())));
        }
    }
    let snaps: Vec<Snapshot> = crate::store::load_snapshots(dir).into_iter().filter(|s| s.id != snap.id).collect();
    crate::store::save_snapshots(dir, &snaps)
}
