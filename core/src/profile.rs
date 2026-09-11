//! Maschinenprofile — ein Profil beschreibt einen kompletten Rechner. Die
//! Standard-Profile liegen als JSON in `profiles/` und werden zur Compile-Zeit
//! eingebettet; eigene Profile im Konfigurationsordner überlagern sie per `id`.

use crate::model::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct ProfileOs {
    /// Anzeigename der OS-Familie, z. B. „Windows 9x“.
    pub family: String,
    /// Konkrete Versionen, die auf diesem Profil laufen.
    pub versions: Vec<String>,
    /// Hinweis für die Installation (Treiber, ROM, BIOS …).
    pub hint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct ProfileLibretro {
    pub core: String,
    pub fallback_core: Option<String>,
    pub bios: Vec<String>,
    /// Dateiendungen der ROMs/Abbilder (ohne Punkt).
    pub extensions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    /// Baujahr der nachgebildeten Maschine (0 = aktuell).
    pub epoch: u32,
    /// `modern` | `retro-pc` | `retro-mac` | `other-arch` | `console`
    pub category: String,
    pub os: ProfileOs,
    pub engine: Engine,
    pub arch: Arch,
    pub machine_type: String,
    pub cpu: Cpu,
    pub memory_mb: u32,
    pub firmware: Firmware,
    pub tpm: bool,
    pub devices: Devices,
    pub network: Network,
    pub isolation: Isolation,
    pub display: Display,
    /// `free` | `nested`
    pub tier: String,
    pub beta: bool,
    pub notes: String,
    /// Vorschlag für die Systemplatte (0 = keine Platte, z. B. Konsolen).
    pub default_disk_gb: u32,
    /// Firmware-ROM nötig (q800, mac99 optional) — Nutzer liefert die Datei.
    pub needs_rom: bool,
    pub libretro: Option<ProfileLibretro>,
}

impl Profile {
    /// Neue Maschine aus dem Profil — Ids, Zeitstempel und Platte setzt der Aufrufer.
    pub fn instantiate(&self, id: &str, name: &str, now: &str) -> Machine {
        Machine {
            id: id.into(),
            name: name.into(),
            profile_id: self.id.clone(),
            category: self.category.clone(),
            engine: self.engine,
            arch: self.arch,
            machine_type: self.machine_type.clone(),
            cpu: self.cpu.clone(),
            memory_mb: self.memory_mb,
            firmware: self.firmware,
            tpm: self.tpm,
            disks: Vec::new(),
            media: Vec::new(),
            network: self.network.clone(),
            display: self.display.clone(),
            devices: self.devices.clone(),
            usb: Vec::new(),
            shared_folders: Vec::new(),
            isolation: self.isolation.clone(),
            libretro: self.libretro.as_ref().map(|l| LibretroConfig {
                core: l.core.clone(),
                fallback_core: l.fallback_core.clone(),
                bios: l.bios.clone(),
                state_slot: 0,
            }),
            boot: "auto".into(),
            extra_args: Vec::new(),
            notes: String::new(),
            created_at: now.into(),
            updated_at: now.into(),
        }
    }
}

/// Eingebettete Standard-Profile (Reihenfolge = Anzeige im Assistenten).
const BUILTIN: &[&str] = &[
    include_str!("../../profiles/modern-x86-uefi.json"),
    include_str!("../../profiles/modern-x86-bios.json"),
    include_str!("../../profiles/modern-arm-uefi.json"),
    include_str!("../../profiles/pc-1992-486.json"),
    include_str!("../../profiles/pc-1996-pentium133.json"),
    include_str!("../../profiles/pc-1999-pentium3.json"),
    include_str!("../../profiles/pc-2003-xp.json"),
    include_str!("../../profiles/mac-1994-quadra.json"),
    include_str!("../../profiles/mac-1999-g3.json"),
    include_str!("../../profiles/linux-riscv.json"),
    include_str!("../../profiles/linux-mips.json"),
    include_str!("../../profiles/console-nes.json"),
    include_str!("../../profiles/console-snes.json"),
    include_str!("../../profiles/console-gameboy.json"),
    include_str!("../../profiles/console-gba.json"),
    include_str!("../../profiles/console-megadrive.json"),
    include_str!("../../profiles/console-pce.json"),
    include_str!("../../profiles/console-n64.json"),
    include_str!("../../profiles/console-psx.json"),
    include_str!("../../profiles/console-psp.json"),
    include_str!("../../profiles/console-nds.json"),
    include_str!("../../profiles/console-saturn.json"),
    include_str!("../../profiles/console-dreamcast.json"),
    include_str!("../../profiles/console-arcade.json"),
    include_str!("../../profiles/console-atari2600.json"),
    include_str!("../../profiles/console-msx.json"),
    include_str!("../../profiles/console-dos.json"),
];

pub fn builtin_profiles() -> Vec<Profile> {
    BUILTIN
        .iter()
        .map(|s| serde_json::from_str::<Profile>(s).expect("eingebettetes Profil ist ungültig"))
        .collect()
}

/// Standard-Profile plus Nutzer-Profile (JSON-Texte); gleiche `id` überlagert.
pub fn load_profiles(user_json: &[String]) -> Vec<Profile> {
    let mut all = builtin_profiles();
    for text in user_json {
        if let Ok(p) = serde_json::from_str::<Profile>(text) {
            if p.id.is_empty() {
                continue;
            }
            match all.iter_mut().find(|x| x.id == p.id) {
                Some(slot) => *slot = p,
                None => all.push(p),
            }
        }
    }
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_profiles_parse_and_are_unique() {
        let all = builtin_profiles();
        assert!(all.len() >= 20);
        let mut ids: Vec<&str> = all.iter().map(|p| p.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), all.len(), "doppelte Profil-Id");
        for p in &all {
            assert!(!p.name.is_empty());
            assert!(matches!(
                p.category.as_str(),
                "modern" | "retro-pc" | "retro-mac" | "other-arch" | "console"
            ));
            match p.engine {
                Engine::Qemu => {
                    assert!(p.arch.qemu_binary().is_some(), "{}: Arch fehlt", p.id);
                    assert!(!p.machine_type.is_empty(), "{}: machineType fehlt", p.id);
                    assert!(p.memory_mb > 0);
                }
                Engine::Libretro => {
                    let l = p.libretro.as_ref().expect("Konsole ohne libretro-Block");
                    assert!(!l.core.is_empty());
                    assert!(!l.extensions.is_empty(), "{}: extensions fehlen", p.id);
                }
            }
        }
    }

    #[test]
    fn retro_profiles_start_isolated() {
        for p in builtin_profiles() {
            if matches!(p.category.as_str(), "retro-pc" | "retro-mac" | "console") {
                assert_eq!(p.network.mode, NetworkMode::Off, "{}: Retro-Profil mit Netz", p.id);
                assert!(p.isolation.read_only_shares, "{}: Ordner nicht schreibgeschützt", p.id);
            }
        }
    }

    #[test]
    fn user_profile_overrides_builtin() {
        let mut p = builtin_profiles().remove(0);
        p.name = "eigen".into();
        let all = load_profiles(&[serde_json::to_string(&p).unwrap()]);
        assert_eq!(all.iter().filter(|x| x.id == p.id).count(), 1);
        assert_eq!(all.iter().find(|x| x.id == p.id).unwrap().name, "eigen");
    }

    #[test]
    fn instantiate_copies_hardware() {
        let p = builtin_profiles().into_iter().find(|p| p.id == "pc-1996-pentium133").unwrap();
        let m = p.instantiate("m1", "Win98", "2026-09-10T00:00:00Z");
        assert_eq!(m.machine_type, "pc");
        assert_eq!(m.cpu.model, "pentium");
        assert_eq!(m.network.mode, NetworkMode::Off);
        assert!(m.devices.floppy);
    }
}
