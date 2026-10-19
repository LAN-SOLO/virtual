//! Engine-Erkennung auf dem Host: Binaries in PATH und typischen Installations-
//! orten suchen, `--version` ausführen, Firmware finden. Die reinen Helfer
//! (Kandidaten, Versions-Parsing, Beschleuniger) liegen in `virtual_core::detect`.

use crate::settings::Settings;
use std::path::{Path, PathBuf};
use virtual_core::detect::{self as d, HostOs};
use virtual_core::{Arch, EngineInfo};

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// Sucht ein Binary: erst im Überschreibungs-Ordner, dann PATH, dann Kandidatenordner.
pub fn find_binary(name: &str, override_dir: &str) -> Option<PathBuf> {
    let file = exe(name);
    let override_dir = virtual_core::fsutil::normalize_user_path(override_dir, cfg!(windows));
    if !override_dir.is_empty() {
        let p = Path::new(&override_dir);
        // Überschreibung darf Ordner oder Datei sein (Windows: Groß-/Kleinschreibung egal,
        // „RetroArch.exe“ = „retroarch.exe“)
        let same = |f: &std::ffi::OsStr| {
            let f = f.to_string_lossy();
            if cfg!(windows) {
                f.eq_ignore_ascii_case(&file)
            } else {
                f == file.as_str()
            }
        };
        if p.is_file() && p.file_name().map(same).unwrap_or(false) {
            return Some(p.to_path_buf());
        }
        let cand = p.join(&file);
        if cand.is_file() {
            return Some(cand);
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let cand = dir.join(&file);
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    for dir in d::candidate_dirs(HostOs::current()) {
        let cand = Path::new(&dir).join(&file);
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

pub fn version_of(bin: &Path) -> Option<String> {
    let out = crate::engine::command(bin).arg("--version").output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    d::parse_version(&text)
}

pub fn find_qemu_binary(arch: Arch, settings: &Settings) -> Option<PathBuf> {
    find_binary(arch.qemu_binary()?, &settings.qemu_dir)
}

pub fn qemu_img_path(settings: &Settings) -> Option<PathBuf> {
    find_binary("qemu-img", &settings.qemu_dir)
}

pub fn swtpm_path() -> Option<PathBuf> {
    if cfg!(windows) {
        return None;
    }
    find_binary("swtpm", "")
}

/// Samba-`smbd`, das QEMU für die SLIRP-SMB-Freigabe startet. QEMU bricht den Start
/// ab, wenn es fehlt — deshalb wird `smb=` nur gesetzt, wenn es da ist.
/// Homebrew-QEMU erwartet `samba-dot-org-smbd` (aus `brew install samba`).
pub fn smbd_available() -> bool {
    let candidates: &[&str] = match HostOs::current() {
        HostOs::MacOs => &["/opt/homebrew/sbin/samba-dot-org-smbd", "/usr/local/sbin/samba-dot-org-smbd"],
        HostOs::Linux => &["/usr/sbin/smbd"],
        HostOs::Windows => &[],
    };
    candidates.iter().any(|p| Path::new(p).is_file())
}

pub fn retroarch_path(settings: &Settings) -> Option<PathBuf> {
    let custom = virtual_core::fsutil::normalize_user_path(&settings.retroarch_path, cfg!(windows));
    let p = Path::new(&custom);
    if !custom.is_empty() && p.is_file() {
        return Some(p.to_path_buf());
    }
    find_binary("retroarch", &settings.retroarch_path)
}

fn home() -> String {
    dirs::home_dir().map(|h| h.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Ordner der libretro-Cores (Einstellung, sonst RetroArch-Konvention, sonst App-Bundle).
/// Windows: RetroArch ist portabel — Cores und System liegen neben `retroarch.exe`,
/// egal ob unter `C:\\RetroArch-Win64` oder dem winget-Installationsort.
fn retroarch_side_dir(settings: &Settings, sub: &str) -> Option<PathBuf> {
    if HostOs::current() != HostOs::Windows {
        return None;
    }
    let bin = retroarch_path(settings)?;
    let p = bin.parent()?.join(sub);
    p.is_dir().then_some(p)
}

pub fn cores_dir(settings: &Settings) -> PathBuf {
    let custom = virtual_core::fsutil::normalize_user_path(&settings.cores_dir, cfg!(windows));
    if !custom.is_empty() {
        return PathBuf::from(custom);
    }
    if let Some(p) = retroarch_side_dir(settings, "cores") {
        return p;
    }
    let (cores, _) = d::retroarch_dirs(HostOs::current(), &home());
    let p = PathBuf::from(cores);
    if !p.is_dir() && cfg!(target_os = "macos") {
        let bundle = PathBuf::from("/Applications/RetroArch.app/Contents/Resources/cores");
        if bundle.is_dir() {
            return bundle;
        }
    }
    p
}

/// System-/BIOS-Ordner von RetroArch.
pub fn system_dir(settings: &Settings) -> PathBuf {
    let custom = virtual_core::fsutil::normalize_user_path(&settings.system_dir, cfg!(windows));
    if !custom.is_empty() {
        return PathBuf::from(custom);
    }
    if let Some(p) = retroarch_side_dir(settings, "system") {
        return p;
    }
    let (_, system) = d::retroarch_dirs(HostOs::current(), &home());
    PathBuf::from(system)
}

/// Installationspräfix eines QEMU-Binaries (`/opt/homebrew/bin/qemu-…` → `/opt/homebrew`).
pub fn prefix_of(bin: &Path) -> String {
    bin.parent()
        .and_then(|b| b.parent())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/usr".into())
}

/// UEFI-Firmware: (Code, Vars-Vorlage) — erstes existierendes Paar.
pub fn find_uefi(qemu_bin: &Path, arch: Arch, secure: bool) -> (Option<PathBuf>, Option<PathBuf>) {
    let prefix = prefix_of(qemu_bin);
    let bin_dir = qemu_bin.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let code = d::uefi_candidates(&prefix, &bin_dir, arch, secure).into_iter().map(PathBuf::from).find(|p| p.is_file());
    let vars = d::uefi_vars_template(&prefix, &bin_dir, arch).into_iter().map(PathBuf::from).find(|p| p.is_file());
    (code, vars)
}

fn info(engine: &str, binary: &str, path: Option<PathBuf>, accel: Option<String>) -> EngineInfo {
    let version = path.as_deref().and_then(version_of);
    let ok = path.is_some();
    EngineInfo {
        engine: engine.into(),
        binary: binary.into(),
        path: path.map(|p| p.to_string_lossy().into_owned()),
        version,
        ok,
        accel,
        hint: if ok {
            String::new()
        } else {
            // qemu-img kommt mit dem QEMU-Paket
            d::install_hint(HostOs::current(), if engine == "qemu-img" { "qemu" } else { engine })
        },
    }
}

/// Überblick über alle Engines für die Einstellungen.
pub fn detect_all(settings: &Settings) -> Vec<EngineInfo> {
    let host = HostOs::current();
    let host_arch = d::host_arch();
    let qemu_bin = host_arch.qemu_binary().unwrap_or("qemu-system-x86_64");
    let qemu = find_binary(qemu_bin, &settings.qemu_dir);
    let accel = d::accel_for(host, host_arch, host_arch).map(String::from);
    vec![
        info("qemu", qemu_bin, qemu, accel),
        info("qemu-img", "qemu-img", qemu_img_path(settings), None),
        info("retroarch", "retroarch", retroarch_path(settings), None),
        info("swtpm", "swtpm", swtpm_path(), None),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_from_binary_path() {
        assert_eq!(prefix_of(Path::new("/opt/homebrew/bin/qemu-system-x86_64")), "/opt/homebrew");
        assert_eq!(prefix_of(Path::new("/usr/bin/qemu-system-i386")), "/usr");
    }

    #[test]
    fn missing_binary_yields_hint() {
        let s = Settings::default();
        let all = detect_all(&s);
        assert_eq!(all.len(), 4);
        for e in all {
            if !e.ok && e.engine != "swtpm" {
                assert!(!e.hint.is_empty(), "{}: Hinweis fehlt", e.engine);
            }
        }
    }
}
