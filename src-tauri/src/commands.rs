//! Tauri-Kommandos — der Vertrag steht in `src/api.ts`.

use crate::detect;
use crate::engine::{self, libretro as lr, qemu, Running};
use crate::settings::{self, Settings};
use crate::state::{AppState, Op};
use crate::store;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, State};
use virtual_core::image;
use virtual_core::{load_profiles, Engine, Machine, MachineStatus, MediaKind, MediaRef, Profile, Snapshot};
use virtual_core::{DiskRef, EngineInfo};

type Shared = Arc<AppState>;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateMachineInput {
    pub profile_id: String,
    pub name: String,
    pub disk_gb: u32,
    pub media_path: Option<String>,
    pub rom_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfoDto {
    pub version: String,
    pub notes: Option<String>,
    pub date: Option<String>,
}

fn now_iso() -> String {
    engine::now_iso()
}

/// Blockierende Arbeit (Prozesse, QMP, Dateien) vom Hauptthread fernhalten —
/// synchrone Tauri-Kommandos laufen sonst im UI-Thread und frieren das Fenster ein.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

/// Laufende Engine ohne gehaltene Sperre (Arc-Klon).
fn running_of(st: &Shared, id: &str) -> Option<Arc<Running>> {
    st.runtime.lock().unwrap().get(id).cloned()
}

/// Maschine samt Ordner laden.
fn load(st: &Shared, id: &str) -> Result<(PathBuf, Machine), String> {
    let base = st.machines_dir();
    let dir = store::find_dir(&base, id).ok_or("Maschine nicht gefunden")?;
    let m = store::load_machine(&dir).ok_or("machine.json nicht lesbar")?;
    Ok((dir, m))
}

fn is_running(st: &Shared, id: &str) -> bool {
    st.runtime.lock().unwrap().contains_key(id)
}

fn profiles(st: &Shared) -> Vec<Profile> {
    let user: Vec<String> = std::fs::read_dir(st.profiles_dir())
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().map(|x| x == "json").unwrap_or(false))
                .filter_map(|p| std::fs::read_to_string(p).ok())
                .collect()
        })
        .unwrap_or_default();
    load_profiles(&user)
}

fn media_kind_for(path: &str, engine: Engine) -> MediaKind {
    if engine == Engine::Libretro {
        return MediaKind::Rom;
    }
    let ext = Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    // `.img` ist auch CloneCD bzw. ein roher CD-Abzug: Disketten sind höchstens 2,88 MB groß
    let floppy_sized = || std::fs::metadata(path).map(|m| m.len() <= 2_949_120).unwrap_or(true);
    if matches!(ext.as_str(), "img" | "ima" | "dsk" | "vfd" | "flp") && floppy_sized() {
        MediaKind::Floppy
    } else {
        MediaKind::Cdrom
    }
}

/// Endungen, die ohne Wandlung sicher nicht laufen — hier muss die Analyse gelingen.
fn must_convert(path: &str) -> bool {
    let ext = Path::new(path).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    matches!(ext.as_str(), "nrg" | "cue" | "mds" | "ccd")
}

/// CD-Abbild einlegen: was QEMU nicht direkt liest (NRG, BIN/CUE, MDF, rohe 2352er),
/// wird als `<stem>.iso` in den Maschinenordner gewandelt; sonst bleibt der Pfad wie er ist.
fn prepare_cd_image(dir: &Path, path: &str) -> Result<String, String> {
    let src = Path::new(path);
    let analysis = match image::analyze(src) {
        Ok(a) => a,
        Err(e) if must_convert(path) => return Err(format!("Abbild nicht lesbar: {e}")),
        // Unbekanntes Format (z. B. reine HFS-CD ohne ISO-9660): QEMU roh probieren lassen
        Err(_) => return Ok(path.to_string()),
    };
    if analysis.layout.is_none() {
        return Ok(path.to_string());
    }
    let dst = converted_target(dir, src);
    if !converted_is_current(&dst, src) {
        image::write_iso(&analysis, &dst)?;
        let _ = std::fs::write(source_marker(&dst), source_fingerprint(src));
    }
    Ok(dst.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string()))
}

/// Merkzettel neben der gewandelten ISO: aus welcher Quelle sie stammt.
fn source_marker(iso: &Path) -> PathBuf {
    iso.with_extension("iso.source")
}

/// Quelle eindeutig beschreiben: Pfad, Größe, Änderungszeit.
fn source_fingerprint(src: &Path) -> String {
    let abs = std::fs::canonicalize(src).unwrap_or_else(|_| src.to_path_buf());
    let meta = std::fs::metadata(src).ok();
    let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
    let mtime = meta
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{}\n{size}\n{mtime}\n", abs.to_string_lossy())
}

/// Vorhandene Wandlung nur wiederverwenden, wenn sie aus genau dieser Quelle stammt.
fn converted_is_current(iso: &Path, src: &Path) -> bool {
    iso.is_file() && std::fs::read_to_string(source_marker(iso)).map(|m| m == source_fingerprint(src)).unwrap_or(false)
}

/// Zielname `<stem>.iso`; belegt eine fremde Quelle den Namen, `<stem>-2.iso` usw.
/// (zwei CDs mit gleichem Dateinamen aus verschiedenen Ordnern überschreiben sich nicht).
fn converted_target(dir: &Path, src: &Path) -> PathBuf {
    let first = dir.join(image::iso_name_for(src));
    if !first.exists() || converted_is_current(&first, src) {
        return first;
    }
    let stem = image::iso_stem_for(src);
    for n in 2.. {
        let cand = dir.join(format!("{stem}-{n}.iso"));
        if !cand.exists() || converted_is_current(&cand, src) {
            return cand;
        }
    }
    unreachable!()
}

// --- Einstellungen ----------------------------------------------------------

#[tauri::command]
pub fn get_settings(st: State<'_, Shared>) -> Settings {
    st.settings_clone()
}

#[tauri::command]
pub fn set_settings(st: State<'_, Shared>, mut settings: Settings) {
    // Pfade bereinigen: Anführungszeichen aus „Als Pfad kopieren“, `D:` → `D:\`
    let win = cfg!(windows);
    for p in [&mut settings.machines_dir, &mut settings.qemu_dir, &mut settings.retroarch_path, &mut settings.cores_dir, &mut settings.system_dir] {
        *p = virtual_core::fsutil::normalize_user_path(p, win);
    }
    settings::store(&st.config_dir, &settings);
    *st.settings.lock().unwrap() = settings;
}

#[tauri::command]
pub fn data_path(st: State<'_, Shared>) -> String {
    st.machines_dir().to_string_lossy().into_owned()
}

// --- Profile & Maschinen ----------------------------------------------------

#[tauri::command]
pub fn list_profiles(st: State<'_, Shared>) -> Vec<Profile> {
    profiles(&st)
}

#[tauri::command]
pub fn list_machines(st: State<'_, Shared>) -> Vec<Machine> {
    store::list(&st.machines_dir()).into_iter().map(|(_, m)| m).collect()
}

#[tauri::command]
pub fn get_machine(st: State<'_, Shared>, id: String) -> Result<Machine, String> {
    load(&st, &id).map(|(_, m)| m)
}

#[tauri::command]
pub fn machine_dir(st: State<'_, Shared>, id: String) -> Result<String, String> {
    load(&st, &id).map(|(d, _)| d.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn create_machine(st: State<'_, Shared>, input: CreateMachineInput) -> Result<Machine, String> {
    let st = st.inner().clone();
    tauri::async_runtime::spawn_blocking(move || create_machine_blocking(&st, input))
        .await
        .map_err(|e| e.to_string())?
}

fn create_machine_blocking(st: &Shared, input: CreateMachineInput) -> Result<Machine, String> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err("Name fehlt".into());
    }
    let profile = profiles(st)
        .into_iter()
        .find(|p| p.id == input.profile_id)
        .ok_or("Profil nicht gefunden")?;
    let settings = st.settings_clone();
    if profile.engine == Engine::Qemu && input.disk_gb > 0 && detect::qemu_img_path(&settings).is_none() {
        return Err(format!(
            "qemu-img nicht gefunden — {}",
            virtual_core::detect::install_hint(virtual_core::detect::HostOs::current(), "qemu")
        ));
    }
    let base = st.machines_dir();
    let new_id = uuid::Uuid::new_v4().to_string();
    // Anlegen (qemu-img create, ggf. minutenlange CD-Wandlung) zählt als Vorgang:
    // Beenden fragt dann nach und wartet, statt einen halben Ordner zu hinterlassen
    let _busy = st.begin(&new_id, Op::Media)?;
    let dir = store::create_machine_dir(&base, name)?;
    let now = now_iso();
    let mut m = profile.instantiate(&new_id, name, &now);

    if profile.engine == Engine::Qemu && input.disk_gb > 0 {
        let disk = DiskRef { id: uuid::Uuid::new_v4().to_string(), size_gb: input.disk_gb, ..DiskRef::default() };
        let path = dir.join(&disk.path);
        let args = virtual_core::argv::qemu_img_create(&path, &disk.format, disk.size_gb);
        let bin = detect::qemu_img_path(&settings).ok_or("qemu-img nicht gefunden")?;
        let out = engine::command(bin).args(&args).output().map_err(|e| format!("qemu-img: {e}"))?;
        if !out.status.success() {
            let _ = store::delete(&dir, true);
            return Err(format!("Platte anlegen fehlgeschlagen: {}", String::from_utf8_lossy(&out.stderr).trim()));
        }
        m.disks.push(disk);
    }
    if let Some(p) = input.media_path.as_deref().filter(|p| !p.trim().is_empty()) {
        let kind = media_kind_for(p, profile.engine);
        let path = if kind == MediaKind::Cdrom {
            match prepare_cd_image(&dir, p) {
                Ok(x) => x,
                Err(e) => {
                    let _ = store::delete(&dir, true);
                    return Err(e);
                }
            }
        } else {
            p.to_string()
        };
        m.media.push(MediaRef { id: uuid::Uuid::new_v4().to_string(), kind, path, slot: 0 });
    }
    if let Some(p) = input.rom_path.as_deref().filter(|p| !p.trim().is_empty()) {
        if profile.engine == Engine::Qemu {
            m.media.push(MediaRef { id: uuid::Uuid::new_v4().to_string(), kind: MediaKind::Rom, path: p.into(), slot: 0 });
        }
    }
    store::save_machine(&dir, &m)?;
    Ok(m)
}

#[tauri::command]
pub fn save_machine(st: State<'_, Shared>, mut machine: Machine) -> Result<Machine, String> {
    // Während einer Wandlung/eines Löschens nicht schreiben: Die Wandlung speichert
    // am Ende ihren (älteren) Stand und hätte die Änderung sonst still überschrieben.
    let _busy = st.begin(&machine.id, Op::Edit)?;
    if is_running(&st, &machine.id) {
        return Err("Maschine läuft — erst stoppen, dann ändern".into());
    }
    let (dir, old) = load(&st, &machine.id)?;
    // Unveränderliches bleibt erhalten; der Ordner wird nie umbenannt.
    machine.created_at = old.created_at;
    machine.profile_id = if machine.profile_id.is_empty() { old.profile_id } else { machine.profile_id };
    // Platten und Medien verwalten eigene Kommandos (attach/detach) — ein älterer Entwurf
    // aus der Oberfläche darf sie nicht zurückdrehen; ebenso den Save-State-Slot.
    machine.disks = old.disks;
    machine.media = old.media;
    machine.engine = old.engine;
    if let (Some(new), Some(prev)) = (machine.libretro.as_mut(), old.libretro.as_ref()) {
        new.state_slot = prev.state_slot;
    }
    if machine.name.trim().is_empty() {
        machine.name = old.name;
    }
    machine.cpu.cores = machine.cpu.cores.clamp(1, 64);
    machine.cpu.clock_percent = machine.cpu.clock_percent.clamp(10, 100);
    machine.memory_mb = machine.memory_mb.max(1);
    machine.updated_at = now_iso();
    store::save_machine(&dir, &machine)?;
    Ok(machine)
}

#[tauri::command]
pub async fn delete_machine(st: State<'_, Shared>, id: String, delete_files: bool) -> Result<(), String> {
    let st = st.inner().clone();
    // Große Platten löschen dauert (Windows: Virenscanner) — nicht im UI-Thread
    blocking(move || {
        let _busy = st.begin(&id, Op::Delete)?;
        if is_running(&st, &id) {
            return Err("Maschine läuft — erst stoppen, dann löschen".into());
        }
        let (dir, _) = load(&st, &id)?;
        store::delete(&dir, delete_files)
    })
    .await
}

#[tauri::command]
pub async fn preview_command(st: State<'_, Shared>, id: String) -> Result<String, String> {
    let st = st.inner().clone();
    blocking(move || {
        let (dir, m) = load(&st, &id)?;
        let settings = st.settings_clone();
        match m.engine {
            Engine::Qemu => qemu::preview(&m, &dir, &settings),
            Engine::Libretro => lr::preview(&m, &dir, &settings),
        }
    })
    .await
}

// --- Laufzeit ---------------------------------------------------------------

fn status_of(st: &Shared, id: &str) -> MachineStatus {
    match running_of(st, id) {
        Some(r) => r.status(id),
        None => engine::stopped_status(id),
    }
}

#[tauri::command]
pub async fn start_machine(app: AppHandle, st: State<'_, Shared>, id: String) -> Result<MachineStatus, String> {
    let st = st.inner().clone();
    blocking(move || start_machine_blocking(&app, &st, &id)).await
}

fn start_machine_blocking(app: &AppHandle, shared: &Shared, id: &str) -> Result<MachineStatus, String> {
    if is_running(shared, id) {
        return Err("Maschine läuft bereits".into());
    }
    // Zweiter Start derselben Maschine, solange der erste noch hochfährt (oder ein
    // Snapshot/eine Wandlung läuft): ablehnen, bevor er Socket und Platten-Lock anfasst.
    let _busy = shared.begin(id, Op::Start)?;
    start_machine_inner(app, shared, id)
}

fn start_machine_inner(app: &AppHandle, shared: &Shared, id: &str) -> Result<MachineStatus, String> {
    if is_running(shared, id) {
        return Err("Maschine läuft bereits".into());
    }
    let (dir, mut m) = load(shared, id)?;
    let running: Running = match m.engine {
        Engine::Qemu => qemu::start(app, shared, &m, &dir)?,
        Engine::Libretro => lr::start(app, shared, &m, &dir)?,
    };
    let status = running.status(id);
    {
        let mut rt = shared.runtime.lock().unwrap();
        if rt.contains_key(id) {
            // Doppelklick: zweiter Start verliert, sein Prozess wird beendet
            running.kill();
            return Err("Maschine läuft bereits".into());
        }
        rt.insert(id.to_string(), Arc::new(running));
    }
    // RetroArch beginnt jeden Start in Slot 0 (retroarch.cfg) — Zählung angleichen
    if let Some(cfg) = m.libretro.as_mut() {
        if cfg.state_slot != 0 {
            cfg.state_slot = 0;
            let _ = store::save_machine(&dir, &m);
        }
    }
    engine::spawn_waiter(app, shared.clone(), id);
    Ok(status)
}

fn with_running<T>(st: &Shared, id: &str, f: impl FnOnce(&Running, Engine) -> Result<T, String>) -> Result<T, String> {
    let (_, m) = load(st, id)?;
    let r = running_of(st, id).ok_or("Maschine läuft nicht")?;
    f(&r, m.engine)
}

#[tauri::command]
pub async fn stop_machine(st: State<'_, Shared>, id: String, force: bool) -> Result<MachineStatus, String> {
    let st = st.inner().clone();
    blocking(move || {
        with_running(&st, &id, |r, engine| match engine {
            Engine::Qemu => qemu::stop(r, force),
            Engine::Libretro => lr::stop(r, force),
        })?;
        Ok(status_of(&st, &id))
    })
    .await
}

#[tauri::command]
pub async fn pause_machine(app: AppHandle, st: State<'_, Shared>, id: String) -> Result<MachineStatus, String> {
    let st = st.inner().clone();
    blocking(move || {
        with_running(&st, &id, |r, engine| match engine {
            Engine::Qemu => qemu::pause(r),
            Engine::Libretro => lr::pause(r),
        })?;
        engine::emit_status(&app, &id, "paused", None);
        Ok(status_of(&st, &id))
    })
    .await
}

#[tauri::command]
pub async fn resume_machine(app: AppHandle, st: State<'_, Shared>, id: String) -> Result<MachineStatus, String> {
    let st = st.inner().clone();
    blocking(move || {
        with_running(&st, &id, |r, engine| match engine {
            Engine::Qemu => qemu::resume(r),
            Engine::Libretro => lr::resume(r),
        })?;
        engine::emit_status(&app, &id, "running", None);
        Ok(status_of(&st, &id))
    })
    .await
}

#[tauri::command]
pub async fn reset_machine(st: State<'_, Shared>, id: String) -> Result<MachineStatus, String> {
    let st = st.inner().clone();
    blocking(move || {
        with_running(&st, &id, |r, engine| match engine {
            Engine::Qemu => qemu::reset(r),
            Engine::Libretro => lr::reset(r),
        })?;
        Ok(status_of(&st, &id))
    })
    .await
}

#[tauri::command]
pub async fn machine_status(st: State<'_, Shared>, id: String) -> Result<MachineStatus, String> {
    let st = st.inner().clone();
    blocking(move || {
        // Zustand frisch von der Engine holen (z. B. Pause aus dem QEMU-Fenstermenü)
        if let (Ok((_, m)), Some(r)) = (load(&st, &id), running_of(&st, &id)) {
            if r.state() != "starting" {
                match m.engine {
                    Engine::Qemu => qemu::refresh(&r),
                    Engine::Libretro => lr::refresh(&r),
                }
            }
        }
        Ok(status_of(&st, &id))
    })
    .await
}

#[tauri::command]
pub fn all_status(st: State<'_, Shared>) -> Vec<MachineStatus> {
    let ids: Vec<String> = store::list(&st.machines_dir()).into_iter().map(|(_, m)| m.id).collect();
    ids.iter().map(|id| status_of(&st, id)).collect()
}

#[tauri::command]
pub fn machine_log(st: State<'_, Shared>, id: String) -> Vec<String> {
    if let Some(r) = running_of(&st, &id) {
        return r.log_lines();
    }
    // Gestoppt: letzte Zeilen aus log.txt (nur das Ende lesen — gesprächige Engines
    // schreiben in einer langen Sitzung viele MB)
    load(&st, &id).ok().map(|(dir, _)| log_tail(&dir.join("log.txt"), engine::LOG_LINES)).unwrap_or_default()
}

/// Letzte `max` Zeilen einer Logdatei, gelesen aus höchstens den letzten 512 KiB.
pub fn log_tail(path: &Path, max: usize) -> Vec<String> {
    use std::io::{Read, Seek, SeekFrom};
    const WINDOW: u64 = 512 * 1024;
    let Ok(mut f) = std::fs::File::open(path) else { return Vec::new() };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let start = len.saturating_sub(WINDOW);
    if f.seek(SeekFrom::Start(start)).is_err() {
        return Vec::new();
    }
    let mut buf = Vec::new();
    if f.read_to_end(&mut buf).is_err() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&buf);
    let mut lines: Vec<&str> = text.lines().collect();
    // mitten im Fenster angeschnittene erste Zeile verwerfen
    if start > 0 && !lines.is_empty() {
        lines.remove(0);
    }
    let skip = lines.len().saturating_sub(max);
    lines.into_iter().skip(skip).map(String::from).collect()
}

// --- Medien -----------------------------------------------------------------

#[tauri::command]
pub async fn attach_media(st: State<'_, Shared>, id: String, kind: MediaKind, path: String, slot: u32) -> Result<Machine, String> {
    let st = st.inner().clone();
    tauri::async_runtime::spawn_blocking(move || attach_media_blocking(&st, &id, kind, path, slot))
        .await
        .map_err(|e| e.to_string())?
}

fn attach_media_blocking(st: &Shared, id: &str, kind: MediaKind, path: String, slot: u32) -> Result<Machine, String> {
    let _busy = st.begin(id, Op::Media)?;
    if is_running(st, id) {
        return Err("Medienwechsel im laufenden Betrieb folgt in einer späteren Version — bitte erst stoppen".into());
    }
    let (dir, mut m) = load(st, id)?;
    // Wandlung (NRG, BIN/CUE, MDF …) passiert vor dem Eintrag — bei Fehler bleibt die Maschine unverändert
    let path = if kind == MediaKind::Cdrom && m.engine == Engine::Qemu { prepare_cd_image(&dir, &path)? } else { path };
    m.media.retain(|x| !(x.kind == kind && x.slot == slot));
    m.media.push(MediaRef { id: uuid::Uuid::new_v4().to_string(), kind, path, slot });
    m.updated_at = now_iso();
    store::save_machine(&dir, &m)?;
    Ok(m)
}

#[tauri::command]
pub fn detach_media(st: State<'_, Shared>, id: String, media_id: String) -> Result<Machine, String> {
    let _busy = st.begin(&id, Op::Edit)?;
    if is_running(&st, &id) {
        return Err("Medienwechsel im laufenden Betrieb folgt in einer späteren Version — bitte erst stoppen".into());
    }
    let (dir, mut m) = load(&st, &id)?;
    m.media.retain(|x| x.id != media_id);
    m.updated_at = now_iso();
    store::save_machine(&dir, &m)?;
    Ok(m)
}

// --- Snapshots --------------------------------------------------------------

#[tauri::command]
pub fn list_snapshots(st: State<'_, Shared>, id: String) -> Result<Vec<Snapshot>, String> {
    let (dir, _) = load(&st, &id)?;
    Ok(store::load_snapshots(&dir))
}

#[tauri::command]
pub async fn create_snapshot(st: State<'_, Shared>, id: String, name: String, note: String) -> Result<Snapshot, String> {
    let st = st.inner().clone();
    blocking(move || {
        let _busy = st.begin(&id, Op::Snapshot)?;
        let (dir, mut m) = load(&st, &id)?;
        let settings = st.settings_clone();
        let running = running_of(&st, &id);
        match m.engine {
            Engine::Qemu => qemu::create_snapshot(&m, &dir, running.as_deref(), &settings, &name, &note),
            Engine::Libretro => {
                // Slot-Stand auch nach einem Fehler sichern: RetroArch hat den Slot dann schon
                // gewechselt, sonst zählte virtual beim nächsten Mal vom alten Slot aus
                let res = lr::create_snapshot(&mut m, &dir, running.as_deref(), &name, &note);
                store::save_machine(&dir, &m)?;
                res
            }
        }
    })
    .await
}

fn find_snapshot(dir: &Path, snapshot_id: &str) -> Result<Snapshot, String> {
    store::load_snapshots(dir)
        .into_iter()
        .find(|s| s.id == snapshot_id)
        .ok_or_else(|| "Snapshot nicht gefunden".into())
}

#[tauri::command]
pub async fn restore_snapshot(st: State<'_, Shared>, id: String, snapshot_id: String) -> Result<(), String> {
    let st = st.inner().clone();
    blocking(move || {
        let _busy = st.begin(&id, Op::Snapshot)?;
        let (dir, mut m) = load(&st, &id)?;
        let snap = find_snapshot(&dir, &snapshot_id)?;
        let settings = st.settings_clone();
        let running = running_of(&st, &id);
        match m.engine {
            Engine::Qemu => qemu::restore_snapshot(&m, &dir, running.as_deref(), &settings, &snap),
            Engine::Libretro => {
                let res = lr::restore_snapshot(&mut m, running.as_deref(), &snap);
                store::save_machine(&dir, &m)?;
                res
            }
        }
    })
    .await
}

#[tauri::command]
pub async fn delete_snapshot(st: State<'_, Shared>, id: String, snapshot_id: String) -> Result<(), String> {
    let st = st.inner().clone();
    blocking(move || {
        let _busy = st.begin(&id, Op::Snapshot)?;
        let (dir, m) = load(&st, &id)?;
        let snap = find_snapshot(&dir, &snapshot_id)?;
        let settings = st.settings_clone();
        let running = running_of(&st, &id);
        match m.engine {
            Engine::Qemu => qemu::delete_snapshot(&m, &dir, running.as_deref(), &settings, &snap),
            Engine::Libretro => lr::delete_snapshot(&m, &dir, &snap),
        }
    })
    .await
}

// --- Engines ----------------------------------------------------------------

#[tauri::command]
pub async fn detect_engines(st: State<'_, Shared>) -> Result<Vec<EngineInfo>, String> {
    let settings = st.settings_clone();
    blocking(move || Ok(detect::detect_all(&settings))).await
}

#[tauri::command]
pub fn missing_bios(st: State<'_, Shared>, id: String) -> Result<Vec<String>, String> {
    let (dir, m) = load(&st, &id)?;
    Ok(lr::missing_bios(&m, &dir, &st.settings_clone()))
}

// --- App --------------------------------------------------------------------

/// Was `open_path` öffnen darf: https-Links und vorhandene Ordner innerhalb des
/// Maschinenordners — nie Dateien, denn „öffnen“ startet bei .exe/.command ein
/// Programm. Unter macOS ist auch ein `.app`-Bundle ein Ordner, deshalb zählt nur,
/// was im Maschinenordner liegt, und Ordner mit Bundle-Endung werden abgelehnt.
pub fn open_target_allowed(path: &str, machines_dir: &Path) -> bool {
    if path.starts_with("https://") {
        return !path.chars().any(|c| c.is_whitespace() || c.is_control());
    }
    let p = Path::new(path);
    if !p.is_absolute() || !p.is_dir() {
        return false;
    }
    let (Ok(full), Ok(base)) = (std::fs::canonicalize(p), std::fs::canonicalize(machines_dir)) else {
        return false;
    };
    if !full.starts_with(&base) {
        return false;
    }
    // Bundle-Ordner (macOS „öffnen“ = starten) nirgends im Pfad unterhalb des Basisordners
    !full.strip_prefix(&base).map(|rel| {
        rel.components().any(|c| {
            Path::new(c.as_os_str())
                .extension()
                .map(|e| {
                    let e = e.to_string_lossy().to_ascii_lowercase();
                    matches!(e.as_str(), "app" | "bundle" | "framework" | "plugin" | "prefpane" | "kext" | "workflow" | "action" | "saver" | "appex" | "xpc" | "command" | "tool")
                })
                .unwrap_or(false)
        })
    }).unwrap_or(true)
}

#[tauri::command]
pub fn open_path(app: AppHandle, st: State<'_, Shared>, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    if !open_target_allowed(&path, &st.machines_dir()) {
        return Err("Nur Links (https) und Maschinenordner lassen sich öffnen".into());
    }
    // Links (Website, GitHub) und Maschinenordner laufen über dasselbe Kommando.
    if path.starts_with("https://") {
        app.opener().open_url(path, None::<&str>).map_err(|e| e.to_string())
    } else {
        app.opener().open_path(path, None::<&str>).map_err(|e| e.to_string())
    }
}

/// Höchstens so lange warten Beenden und Update auf laufende Vorgänge.
const QUIT_WAIT: std::time::Duration = std::time::Duration::from_secs(60);

/// Laufen Vorgänge (Snapshot, Wandlung, Löschen, Anlegen)? Für die Nachfrage beim Update.
#[tauri::command]
pub fn busy_ops(st: State<'_, Shared>) -> bool {
    st.any_busy()
}

/// Beenden nach Bestätigung in der Oberfläche: laufende Engines hart stoppen, dann
/// auf laufende Vorgänge warten (max. 60 s) — ein `qemu-img snapshot` einer gestoppten
/// Maschine schreibt sonst nur halb in die Platte bzw. fehlt danach in der Liste.
/// Ein Snapshot einer laufenden Maschine bricht mit dem Ende der Engine sofort ab.
#[tauri::command]
pub fn quit_app(app: AppHandle, st: State<'_, Shared>) {
    for (_, r) in st.runtime.lock().unwrap().drain() {
        r.kill();
    }
    let st = st.inner().clone();
    // Nicht im UI-Thread warten (synchrone Kommandos laufen dort)
    std::thread::spawn(move || {
        st.wait_idle(QUIT_WAIT);
        app.exit(0);
    });
}

#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<Option<UpdateInfoDto>, String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| e.to_string())?;
    match updater.check().await {
        Ok(Some(update)) => Ok(Some(UpdateInfoDto {
            version: update.version.clone(),
            notes: update.body.clone(),
            date: update.date.map(|d| d.to_string()),
        })),
        Ok(None) => Ok(None),
        Err(e) => Err(format!("Update-Prüfung fehlgeschlagen: {e}")),
    }
}

#[tauri::command]
pub async fn install_update(app: AppHandle, st: State<'_, Shared>) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|e| format!("Update-Prüfung fehlgeschlagen: {e}"))?
        .ok_or("Kein Update verfügbar")?;
    // Erst laden (dauert), dann laufende Engines beenden, dann installieren: Der
    // Windows-Installer beendet virtual sofort — QEMU/RetroArch liefen sonst
    // verwaist weiter. Die Oberfläche hat das vorher bestätigen lassen.
    let bytes = update
        .download(|_, _| {}, || {})
        .await
        .map_err(|e| format!("Update fehlgeschlagen: {e}"))?;
    let running: Vec<Arc<Running>> = st.runtime.lock().unwrap().drain().map(|(_, r)| r).collect();
    for r in running {
        r.kill();
    }
    // Laufende Vorgänge (qemu-img, Wandlung) erst zu Ende kommen lassen: Der
    // Windows-Installer beendet virtual sofort und schnitte sie sonst ab.
    let idle = st.inner().clone();
    let _ = tauri::async_runtime::spawn_blocking(move || idle.wait_idle(QUIT_WAIT)).await;
    update.install(bytes).map_err(|e| format!("Update fehlgeschlagen: {e}"))?;
    app.restart();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("virtual-cmd-test-{}-{name}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// 20 ISO-Sektoren als rohe Mode-1-Sektoren (2352 Byte) — wird gewandelt.
    fn raw_bin(tag: u8) -> Vec<u8> {
        let mut iso = vec![tag; 20 * 2048];
        iso[16 * 2048] = 1;
        iso[16 * 2048 + 1..16 * 2048 + 6].copy_from_slice(b"CD001");
        let mut out = Vec::new();
        for chunk in iso.chunks(2048) {
            out.extend_from_slice(&[0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0, 2, 0, 1]);
            out.extend_from_slice(chunk);
            out.resize(out.len() + 288, 0);
        }
        out
    }

    #[test]
    fn same_named_images_from_different_folders_do_not_collide() {
        let base = tempdir("collide");
        let vm = base.join("vm");
        std::fs::create_dir_all(&vm).unwrap();
        for (d, tag) in [("cd1", 0x11u8), ("cd2", 0x22u8)] {
            std::fs::create_dir_all(base.join(d)).unwrap();
            std::fs::write(base.join(d).join("disc.bin"), raw_bin(tag)).unwrap();
        }
        let a = prepare_cd_image(&vm, &base.join("cd1/disc.bin").to_string_lossy()).unwrap();
        let b = prepare_cd_image(&vm, &base.join("cd2/disc.bin").to_string_lossy()).unwrap();
        assert_eq!(a, "disc.iso");
        assert_eq!(b, "disc-2.iso");
        assert_eq!(std::fs::read(vm.join(&a)).unwrap()[0], 0x11);
        assert_eq!(std::fs::read(vm.join(&b)).unwrap()[0], 0x22);
        // erneutes Einlegen derselben Quelle nutzt die vorhandene Wandlung
        assert_eq!(prepare_cd_image(&vm, &base.join("cd1/disc.bin").to_string_lossy()).unwrap(), "disc.iso");
        assert!(!vm.join("disc-3.iso").exists());
        // reine ISO bleibt, wo sie ist
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn wizard_media_kind_by_extension_and_size() {
        let base = tempdir("kind");
        let fl = base.join("dos622.img");
        std::fs::write(&fl, vec![0u8; 1_474_560]).unwrap();
        assert_eq!(media_kind_for(&fl.to_string_lossy(), Engine::Qemu), MediaKind::Floppy);
        // CloneCD-IMG bzw. roher CD-Abzug (> 2,88 MB) ist eine CD, keine Diskette
        let cd = base.join("game.img");
        std::fs::write(&cd, vec![0u8; 3 * 1024 * 1024]).unwrap();
        assert_eq!(media_kind_for(&cd.to_string_lossy(), Engine::Qemu), MediaKind::Cdrom);
        assert_eq!(media_kind_for("/x/disk.VFD", Engine::Qemu), MediaKind::Floppy);
        assert_eq!(media_kind_for("/x/win98.iso", Engine::Qemu), MediaKind::Cdrom);
        assert_eq!(media_kind_for("/x/game.img", Engine::Libretro), MediaKind::Rom);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn log_tail_reads_only_the_end() {
        let base = tempdir("logtail");
        let f = base.join("log.txt");
        assert!(log_tail(&f, 10).is_empty(), "fehlende Datei = leer");
        let text: String = (0..100_000).map(|i| format!("zeile {i}\n")).collect();
        std::fs::write(&f, &text).unwrap();
        let tail = log_tail(&f, 500);
        assert_eq!(tail.len(), 500);
        assert_eq!(tail.last().unwrap(), "zeile 99999");
        assert_eq!(tail[0], "zeile 99500");
        std::fs::write(&f, "a\nb\n").unwrap();
        assert_eq!(log_tail(&f, 500), vec!["a", "b"]);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn open_path_allows_only_https_and_folders() {
        let base = tempdir("open");
        let machines = base.join("machines");
        let vm = machines.join("win-98");
        std::fs::create_dir_all(&vm).unwrap();
        assert!(open_target_allowed("https://lan-solo.com/de/tools/virtual/", &machines));
        assert!(!open_target_allowed("http://example.com", &machines));
        assert!(!open_target_allowed("file:///etc/passwd", &machines));
        assert!(!open_target_allowed("https://x y", &machines));
        assert!(open_target_allowed(&vm.to_string_lossy(), &machines));
        assert!(open_target_allowed(&machines.to_string_lossy(), &machines));
        let f = vm.join("tool.command");
        std::fs::write(&f, "#!/bin/sh\n").unwrap();
        assert!(!open_target_allowed(&f.to_string_lossy(), &machines), "Dateien (ausführbar) nie öffnen");
        assert!(!open_target_allowed("relative/dir", &machines));
        // Ordner außerhalb des Maschinenordners und .app-Bundles (unter macOS Ordner, „öffnen“ = starten)
        assert!(!open_target_allowed(&base.to_string_lossy(), &machines));
        let outside_app = base.join("Evil.app");
        std::fs::create_dir_all(&outside_app).unwrap();
        assert!(!open_target_allowed(&outside_app.to_string_lossy(), &machines));
        let inside_app = vm.join("Evil.app");
        std::fs::create_dir_all(&inside_app).unwrap();
        assert!(!open_target_allowed(&inside_app.to_string_lossy(), &machines));
        // Ausbruch über ..
        let escape = vm.join("..").join("..").join("Evil.app");
        assert!(!open_target_allowed(&escape.to_string_lossy(), &machines));
        let _ = std::fs::remove_dir_all(&base);
    }
}
