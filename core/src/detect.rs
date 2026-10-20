//! Engine-Erkennung als reine Funktionen: Binary-Namen, Kandidatenpfade,
//! Versions-Parsing, Beschleuniger je Host. Das Ausführen von `--version`
//! und das Prüfen von Pfaden macht der Tauri-Teil.

use crate::model::Arch;

/// Betriebssystem des Hosts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostOs {
    MacOs,
    Windows,
    Linux,
}

impl HostOs {
    pub fn current() -> HostOs {
        if cfg!(target_os = "macos") {
            HostOs::MacOs
        } else if cfg!(target_os = "windows") {
            HostOs::Windows
        } else {
            HostOs::Linux
        }
    }
}

/// Host-Architektur (`x86_64` | `aarch64`).
pub fn host_arch() -> Arch {
    if cfg!(target_arch = "aarch64") {
        Arch::Aarch64
    } else {
        Arch::X86_64
    }
}

/// Beschleuniger, wenn Gast-Arch zur Host-Arch passt (i386 läuft auf x86_64-Hosts beschleunigt).
pub fn accel_for(host: HostOs, host_arch: Arch, guest: Arch) -> Option<&'static str> {
    let compatible = host_arch == guest || (host_arch == Arch::X86_64 && guest == Arch::I386);
    if !compatible {
        return None;
    }
    Some(match host {
        HostOs::MacOs => "hvf",
        HostOs::Windows => "whpx",
        HostOs::Linux => "kvm",
    })
}

/// Beschleuniger für eine Maschine: nur aktuelle Gäste (Kategorie `modern`) werden
/// virtualisiert. Retro-Profile laufen immer emuliert (TCG) — auch auf x86-Hosts, wo
/// i386 beschleunigt ginge: Mit HVF/WHPX griffe die Taktbremse (`-icount`) nicht, und
/// Windows 95 scheitert auf schnellen CPUs am Timer (Schutzfehler beim Start).
pub fn accel_for_machine(host: HostOs, host_arch: Arch, guest: Arch, category: &str) -> Option<&'static str> {
    if category != "modern" {
        return None;
    }
    accel_for(host, host_arch, guest)
}

/// Anzeige-Backend je Host (QEMU `-display`).
pub fn display_backend(host: HostOs) -> &'static str {
    match host {
        HostOs::MacOs => "cocoa",
        HostOs::Windows => "sdl",
        HostOs::Linux => "gtk",
    }
}

/// Audio-Backend je Host (QEMU `-audiodev`).
pub fn audio_backend(host: HostOs) -> &'static str {
    match host {
        HostOs::MacOs => "coreaudio",
        HostOs::Windows => "dsound",
        HostOs::Linux => "pa",
    }
}

/// Dateiendung der libretro-Cores.
pub fn core_ext(host: HostOs) -> &'static str {
    match host {
        HostOs::MacOs => "dylib",
        HostOs::Windows => "dll",
        HostOs::Linux => "so",
    }
}

/// Typische Installationsorte außerhalb von PATH (Homebrew, winget, Distro).
pub fn candidate_dirs(host: HostOs) -> Vec<String> {
    match host {
        HostOs::MacOs => vec![
            "/opt/homebrew/bin".into(),
            "/usr/local/bin".into(),
            "/opt/local/bin".into(),
            "/Applications/RetroArch.app/Contents/MacOS".into(),
        ],
        HostOs::Windows => vec![
            "C:\\Program Files\\qemu".into(),
            "C:\\Program Files (x86)\\qemu".into(),
            "C:\\RetroArch-Win64".into(),
            "C:\\Program Files\\RetroArch".into(),
        ],
        HostOs::Linux => vec!["/usr/bin".into(), "/usr/local/bin".into(), "/snap/bin".into(), "/var/lib/flatpak/exports/bin".into()],
    }
}

/// Installationshinweis je Host und Engine.
pub fn install_hint(host: HostOs, engine: &str) -> String {
    match (host, engine) {
        (HostOs::MacOs, "qemu") => "brew install qemu".into(),
        (HostOs::Windows, "qemu") => "winget install SoftwareFreedomConservancy.QEMU".into(),
        (HostOs::Linux, "qemu") => "sudo apt install qemu-system  ·  sudo dnf install qemu".into(),
        (HostOs::MacOs, "retroarch") => "brew install --cask retroarch-metal".into(),
        (HostOs::Windows, "retroarch") => "winget install Libretro.RetroArch".into(),
        (HostOs::Linux, "retroarch") => "sudo apt install retroarch  ·  flatpak install org.libretro.RetroArch".into(),
        (HostOs::MacOs, "swtpm") => "brew install swtpm".into(),
        (HostOs::Windows, "swtpm") => "swtpm ist unter Windows nicht verfügbar — TPM entfällt".into(),
        (HostOs::Linux, "swtpm") => "sudo apt install swtpm".into(),
        _ => String::new(),
    }
}

/// Standard-Ordner für libretro-Cores und System-Dateien je Host (RetroArch-Konventionen).
pub fn retroarch_dirs(host: HostOs, home: &str) -> (String, String) {
    match host {
        HostOs::MacOs => (
            format!("{home}/Library/Application Support/RetroArch/cores"),
            format!("{home}/Library/Application Support/RetroArch/system"),
        ),
        HostOs::Windows => ("C:\\RetroArch-Win64\\cores".into(), "C:\\RetroArch-Win64\\system".into()),
        HostOs::Linux => (format!("{home}/.config/retroarch/cores"), format!("{home}/.config/retroarch/system")),
    }
}

/// Kandidaten für UEFI-Firmware: relativ zum QEMU-Präfix (Homebrew/Distro: `<prefix>/share/qemu`),
/// zum Binary-Ordner (Windows-Installer: `<qemu>\\share`) und an Distro-Pfaden.
/// `secure`: Secure-Boot-Build (SMM) bevorzugen — nur x86.
pub fn uefi_candidates(prefix: &str, bin_dir: &str, guest: Arch, secure: bool) -> Vec<String> {
    match guest {
        Arch::X86_64 | Arch::I386 => {
            let mut v = Vec::new();
            if secure {
                v.push(format!("{prefix}/share/qemu/edk2-x86_64-secure-code.fd"));
                v.push(format!("{bin_dir}/share/edk2-x86_64-secure-code.fd"));
                v.push("/usr/share/OVMF/OVMF_CODE_4M.secboot.fd".into());
                v.push("/usr/share/OVMF/OVMF_CODE.secboot.fd".into());
                v.push("/usr/share/edk2/ovmf/OVMF_CODE.secboot.fd".into());
            }
            v.extend([
                format!("{prefix}/share/qemu/edk2-x86_64-code.fd"),
                format!("{bin_dir}/share/edk2-x86_64-code.fd"),
                "/usr/share/OVMF/OVMF_CODE_4M.fd".into(),
                "/usr/share/OVMF/OVMF_CODE.fd".into(),
                "/usr/share/edk2/ovmf/OVMF_CODE.fd".into(),
                "/usr/share/edk2-ovmf/x64/OVMF_CODE.fd".into(),
            ]);
            v
        }
        Arch::Aarch64 => vec![
            format!("{prefix}/share/qemu/edk2-aarch64-code.fd"),
            format!("{bin_dir}/share/edk2-aarch64-code.fd"),
            "/usr/share/AAVMF/AAVMF_CODE.fd".into(),
            "/usr/share/edk2/aarch64/QEMU_EFI-pflash.raw".into(),
        ],
        _ => Vec::new(),
    }
}

/// Vorlagen für die beschreibbare NVRAM-Kopie (passend zu den Code-Kandidaten oben).
pub fn uefi_vars_template(prefix: &str, bin_dir: &str, guest: Arch) -> Vec<String> {
    match guest {
        Arch::X86_64 | Arch::I386 => vec![
            format!("{prefix}/share/qemu/edk2-i386-vars.fd"),
            format!("{bin_dir}/share/edk2-i386-vars.fd"),
            "/usr/share/OVMF/OVMF_VARS_4M.fd".into(),
            "/usr/share/OVMF/OVMF_VARS.fd".into(),
            "/usr/share/edk2/ovmf/OVMF_VARS.fd".into(),
        ],
        Arch::Aarch64 => vec![
            format!("{prefix}/share/qemu/edk2-arm-vars.fd"),
            format!("{bin_dir}/share/edk2-arm-vars.fd"),
            "/usr/share/AAVMF/AAVMF_VARS.fd".into(),
        ],
        _ => Vec::new(),
    }
}

/// `QEMU emulator version 9.2.0 (v9.2.0)` → `9.2.0`; `RetroArch 1.19.1 …` → `1.19.1`.
pub fn parse_version(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .find(|w| w.chars().next().is_some_and(|c| c.is_ascii_digit()) && w.contains('.'))
        .map(|w| w.trim_matches(|c: char| !c.is_ascii_digit() && c != '.').to_string())
}

/// Mindestversion für job-basierte Snapshots (`snapshot-save`).
pub fn qemu_supports_snapshot_jobs(version: &str) -> bool {
    let mut it = version.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
    it.next().unwrap_or(0) >= 6
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accel_matches_host() {
        assert_eq!(accel_for(HostOs::MacOs, Arch::Aarch64, Arch::Aarch64), Some("hvf"));
        assert_eq!(accel_for(HostOs::MacOs, Arch::Aarch64, Arch::X86_64), None);
        assert_eq!(accel_for(HostOs::Linux, Arch::X86_64, Arch::I386), Some("kvm"));
        assert_eq!(accel_for(HostOs::Windows, Arch::X86_64, Arch::X86_64), Some("whpx"));
        assert_eq!(accel_for(HostOs::Linux, Arch::X86_64, Arch::M68k), None);
        // Retro-PC auf Windows/Intel-Mac: emuliert, damit Taktbremse und Timer stimmen
        assert_eq!(accel_for_machine(HostOs::Windows, Arch::X86_64, Arch::I386, "retro-pc"), None);
        assert_eq!(accel_for_machine(HostOs::MacOs, Arch::X86_64, Arch::I386, "retro-pc"), None);
        assert_eq!(accel_for_machine(HostOs::Windows, Arch::X86_64, Arch::X86_64, "modern"), Some("whpx"));
        assert_eq!(accel_for_machine(HostOs::MacOs, Arch::Aarch64, Arch::Aarch64, "modern"), Some("hvf"));
    }

    #[test]
    fn versions() {
        assert_eq!(parse_version("QEMU emulator version 9.2.0 (v9.2.0)\nCopyright"), Some("9.2.0".into()));
        assert_eq!(parse_version("RetroArch 1.19.1 (Git 6a5d4bd)"), Some("1.19.1".into()));
        assert_eq!(parse_version("nothing"), None);
        assert!(qemu_supports_snapshot_jobs("9.2.0"));
        assert!(!qemu_supports_snapshot_jobs("5.2.0"));
    }

    #[test]
    fn hints_and_dirs() {
        assert!(install_hint(HostOs::MacOs, "qemu").contains("brew"));
        assert!(candidate_dirs(HostOs::MacOs).iter().any(|d| d.contains("homebrew")));
        let (cores, system) = retroarch_dirs(HostOs::MacOs, "/Users/x");
        assert!(cores.ends_with("/cores") && system.ends_with("/system"));
        assert!(uefi_candidates("/opt/homebrew", "/opt/homebrew/bin", Arch::X86_64, false)[0].contains("edk2-x86_64-code.fd"));
        assert!(uefi_candidates("/opt/homebrew", "/opt/homebrew/bin", Arch::X86_64, true)[0].contains("secure-code"));
        // Windows-Installer: Firmware liegt in <qemu>\share, nicht <prefix>\share\qemu
        let win = uefi_candidates("C:\\Program Files", "C:\\Program Files\\qemu", Arch::X86_64, false);
        assert!(win.iter().any(|p| p == "C:\\Program Files\\qemu/share/edk2-x86_64-code.fd"));
        assert!(uefi_vars_template("C:\\Program Files", "C:\\Program Files\\qemu", Arch::X86_64).iter().any(|p| p.ends_with("qemu/share/edk2-i386-vars.fd")));
        assert!(uefi_candidates("/x", "/x/bin", Arch::M68k, false).is_empty());
    }
}
