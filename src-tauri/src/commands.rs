//! Tauri-Kommandos — der Vertrag steht in `src/api.ts`.

use crate::detect;
use crate::engine::{self, libretro as lr, qemu, Running};
use crate::settings::{self, Settings};
use crate::state::AppState;
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
    if matches!(ext.as_str(), "img" | "ima" | "dsk" | "vfd" | "flp") {
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
    let dst = dir.join(image::iso_name_for(src));
    let reuse = dst.is_file()
        && match (dst.metadata().and_then(|m| m.modified()), src.metadata().and_then(|m| m.modified())) {
            (Ok(d), Ok(s)) => d >= s,
            _ => false,
        };
    if !reuse {
        image::write_iso(&analysis, &dst)?;
    }
    Ok(dst.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string()))
}

// --- Einstellungen ----------------------------------------------------------

#[tauri::command]
pub fn get_settings(st: State<'_, Shared>) -> Settings {
    st.settings_clone()
}

#[tauri::command]
pub fn set_settings(st: State<'_, Shared>, settings: Settings) {
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
    let dir = store::create_machine_dir(&base, name)?;
    let now = now_iso();
    let mut m = profile.instantiate(&uuid::Uuid::new_v4().to_string(), name, &now);

    if profile.engine == Engine::Qemu && input.disk_gb > 0 {
        let disk = DiskRef { id: uuid::Uuid::new_v4().to_string(), size_gb: input.disk_gb, ..DiskRef::default() };
        let path = dir.join(&disk.path);
        let args = virtual_core::argv::qemu_img_create(&path, &disk.format, disk.size_gb);
        let bin = detect::qemu_img_path(&settings).ok_or("qemu-img nicht gefunden")?;
        let out = std::process::Command::new(bin).args(&args).output().map_err(|e| format!("qemu-img: {e}"))?;
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
    if is_running(&st, &machine.id) {
        return Err("Maschine läuft — erst stoppen, dann ändern".into());
    }
    let (dir, old) = load(&st, &machine.id)?;
    // Unveränderliches bleibt erhalten; der Ordner wird nie umbenannt.
    machine.created_at = old.created_at;
    machine.profile_id = if machine.profile_id.is_empty() { old.profile_id } else { machine.profile_id };
    machine.updated_at = now_iso();
    store::save_machine(&dir, &machine)?;
    Ok(machine)
}

#[tauri::command]
pub fn delete_machine(st: State<'_, Shared>, id: String, delete_files: bool) -> Result<(), String> {
    if is_running(&st, &id) {
        return Err("Maschine läuft — erst stoppen, dann löschen".into());
    }
    let (dir, _) = load(&st, &id)?;
    store::delete(&dir, delete_files)
}

#[tauri::command]
pub fn preview_command(st: State<'_, Shared>, id: String) -> Result<String, String> {
    let (dir, m) = load(&st, &id)?;
    let settings = st.settings_clone();
    match m.engine {
        Engine::Qemu => qemu::preview(&m, &dir, &settings),
        Engine::Libretro => lr::preview(&m, &dir, &settings),
    }
}

// --- Laufzeit ---------------------------------------------------------------

fn status_of(st: &Shared, id: &str) -> MachineStatus {
    let rt = st.runtime.lock().unwrap();
    match rt.get(id) {
        Some(r) => r.status(id),
        None => engine::stopped_status(id),
    }
}

#[tauri::command]
pub fn start_machine(app: AppHandle, st: State<'_, Shared>, id: String) -> Result<MachineStatus, String> {
    if is_running(&st, &id) {
        return Err("Maschine läuft bereits".into());
    }
    let (dir, m) = load(&st, &id)?;
    let shared: Shared = st.inner().clone();
    let running: Running = match m.engine {
        Engine::Qemu => qemu::start(&app, &shared, &m, &dir)?,
        Engine::Libretro => lr::start(&app, &shared, &m, &dir)?,
    };
    let status = running.status(&id);
    shared.runtime.lock().unwrap().insert(id.clone(), running);
    engine::spawn_waiter(&app, shared.clone(), &id);
    Ok(status)
}

fn with_running<T>(st: &Shared, id: &str, f: impl FnOnce(&Running, Engine) -> Result<T, String>) -> Result<T, String> {
    let (_, m) = load(st, id)?;
    let rt = st.runtime.lock().unwrap();
    let r = rt.get(id).ok_or("Maschine läuft nicht")?;
    f(r, m.engine)
}

#[tauri::command]
pub fn stop_machine(st: State<'_, Shared>, id: String, force: bool) -> Result<MachineStatus, String> {
    with_running(&st, &id, |r, engine| match engine {
        Engine::Qemu => qemu::stop(r, force),
        Engine::Libretro => lr::stop(r, force),
    })?;
    Ok(status_of(&st, &id))
}

#[tauri::command]
pub fn pause_machine(app: AppHandle, st: State<'_, Shared>, id: String) -> Result<MachineStatus, String> {
    with_running(&st, &id, |r, engine| match engine {
        Engine::Qemu => qemu::pause(r),
        Engine::Libretro => lr::pause(r),
    })?;
    engine::emit_status(&app, &id, "paused", None);
    Ok(status_of(&st, &id))
}

#[tauri::command]
pub fn resume_machine(app: AppHandle, st: State<'_, Shared>, id: String) -> Result<MachineStatus, String> {
    with_running(&st, &id, |r, engine| match engine {
        Engine::Qemu => qemu::resume(r),
        Engine::Libretro => lr::resume(r),
    })?;
    engine::emit_status(&app, &id, "running", None);
    Ok(status_of(&st, &id))
}

#[tauri::command]
pub fn reset_machine(st: State<'_, Shared>, id: String) -> Result<MachineStatus, String> {
    with_running(&st, &id, |r, engine| match engine {
        Engine::Qemu => qemu::reset(r),
        Engine::Libretro => lr::reset(r),
    })?;
    Ok(status_of(&st, &id))
}

#[tauri::command]
pub fn machine_status(st: State<'_, Shared>, id: String) -> MachineStatus {
    // Zustand bei Bedarf frisch von der Engine holen
    if let Ok((_, m)) = load(&st, &id) {
        let rt = st.runtime.lock().unwrap();
        if let Some(r) = rt.get(&id) {
            if r.state() != "starting" {
                match m.engine {
                    Engine::Qemu => qemu::refresh(r),
                    Engine::Libretro => lr::refresh(r),
                }
            }
        }
    }
    status_of(&st, &id)
}

#[tauri::command]
pub fn all_status(st: State<'_, Shared>) -> Vec<MachineStatus> {
    let ids: Vec<String> = store::list(&st.machines_dir()).into_iter().map(|(_, m)| m.id).collect();
    ids.iter().map(|id| status_of(&st, id)).collect()
}

#[tauri::command]
pub fn machine_log(st: State<'_, Shared>, id: String) -> Vec<String> {
    let rt = st.runtime.lock().unwrap();
    if let Some(r) = rt.get(&id) {
        return r.log_lines();
    }
    drop(rt);
    // Gestoppt: letzte Zeilen aus log.txt
    load(&st, &id)
        .ok()
        .and_then(|(dir, _)| std::fs::read_to_string(dir.join("log.txt")).ok())
        .map(|t| {
            let lines: Vec<String> = t.lines().map(String::from).collect();
            let skip = lines.len().saturating_sub(engine::LOG_LINES);
            lines.into_iter().skip(skip).collect()
        })
        .unwrap_or_default()
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
    if is_running(st, id) {
        return Err("Medienwechsel im laufenden Betrieb folgt in 0.2 — bitte erst stoppen".into());
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
    if is_running(&st, &id) {
        return Err("Medienwechsel im laufenden Betrieb folgt in 0.2 — bitte erst stoppen".into());
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
pub fn create_snapshot(st: State<'_, Shared>, id: String, name: String, note: String) -> Result<Snapshot, String> {
    let (dir, mut m) = load(&st, &id)?;
    let settings = st.settings_clone();
    let rt = st.runtime.lock().unwrap();
    let running = rt.get(&id);
    match m.engine {
        Engine::Qemu => qemu::create_snapshot(&m, &dir, running, &settings, &name, &note),
        Engine::Libretro => {
            let snap = lr::create_snapshot(&mut m, &dir, running, &name, &note)?;
            store::save_machine(&dir, &m)?;
            Ok(snap)
        }
    }
}

fn find_snapshot(dir: &Path, snapshot_id: &str) -> Result<Snapshot, String> {
    store::load_snapshots(dir)
        .into_iter()
        .find(|s| s.id == snapshot_id)
        .ok_or_else(|| "Snapshot nicht gefunden".into())
}

#[tauri::command]
pub fn restore_snapshot(st: State<'_, Shared>, id: String, snapshot_id: String) -> Result<(), String> {
    let (dir, mut m) = load(&st, &id)?;
    let snap = find_snapshot(&dir, &snapshot_id)?;
    let settings = st.settings_clone();
    let rt = st.runtime.lock().unwrap();
    let running = rt.get(&id);
    match m.engine {
        Engine::Qemu => qemu::restore_snapshot(&m, &dir, running, &settings, &snap),
        Engine::Libretro => {
            lr::restore_snapshot(&mut m, running, &snap)?;
            store::save_machine(&dir, &m)
        }
    }
}

#[tauri::command]
pub fn delete_snapshot(st: State<'_, Shared>, id: String, snapshot_id: String) -> Result<(), String> {
    let (dir, m) = load(&st, &id)?;
    let snap = find_snapshot(&dir, &snapshot_id)?;
    let settings = st.settings_clone();
    let rt = st.runtime.lock().unwrap();
    let running = rt.get(&id);
    match m.engine {
        Engine::Qemu => qemu::delete_snapshot(&m, &dir, running, &settings, &snap),
        Engine::Libretro => lr::delete_snapshot(&m, &dir, &snap),
    }
}

// --- Engines ----------------------------------------------------------------

#[tauri::command]
pub fn detect_engines(st: State<'_, Shared>) -> Vec<EngineInfo> {
    detect::detect_all(&st.settings_clone())
}

#[tauri::command]
pub fn missing_bios(st: State<'_, Shared>, id: String) -> Result<Vec<String>, String> {
    let (dir, m) = load(&st, &id)?;
    Ok(lr::missing_bios(&m, &dir, &st.settings_clone()))
}

// --- App --------------------------------------------------------------------

#[tauri::command]
pub fn open_path(app: AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    // Links (Website, GitHub) und lokale Ordner laufen über dasselbe Kommando.
    if path.starts_with("http://") || path.starts_with("https://") {
        app.opener().open_url(path, None::<&str>).map_err(|e| e.to_string())
    } else {
        app.opener().open_path(path, None::<&str>).map_err(|e| e.to_string())
    }
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
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|e| format!("Update-Prüfung fehlgeschlagen: {e}"))?
        .ok_or("Kein Update verfügbar")?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| format!("Update fehlgeschlagen: {e}"))?;
    app.restart();
}
