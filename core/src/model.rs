//! Datenmodell — jede Maschine ist ein Ordner mit `machine.json`; die Typen hier
//! sind der Vertrag zwischen Rust (core, Tauri) und der React-Oberfläche
//! (`src/api.ts`, camelCase über serde).

use serde::{Deserialize, Serialize};

/// Welche Engine die Maschine ausführt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    #[default]
    Qemu,
    Libretro,
}

/// Gast-Architektur — bestimmt das QEMU-Binary (`qemu-system-<arch>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Arch {
    #[default]
    #[serde(rename = "x86_64")]
    X86_64,
    I386,
    Aarch64,
    M68k,
    Ppc,
    Riscv64,
    Mips,
    /// Konsolen: keine CPU-Architektur im QEMU-Sinn.
    None,
}

impl Arch {
    pub fn qemu_binary(self) -> Option<&'static str> {
        Some(match self {
            Arch::X86_64 => "qemu-system-x86_64",
            Arch::I386 => "qemu-system-i386",
            Arch::Aarch64 => "qemu-system-aarch64",
            Arch::M68k => "qemu-system-m68k",
            Arch::Ppc => "qemu-system-ppc",
            Arch::Riscv64 => "qemu-system-riscv64",
            Arch::Mips => "qemu-system-mips",
            Arch::None => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Firmware {
    #[default]
    Bios,
    Uefi,
    UefiSecure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum NetworkMode {
    /// Kein Netzwerkgerät — Standard für Retro-Profile.
    #[default]
    Off,
    /// SLIRP mit `restrict=on`: Gast sieht nur den Host-Proxy, nie das LAN.
    Isolated,
    /// SLIRP-NAT wie üblich.
    Nat,
    /// Bridged (0.4) — bis dahin wie NAT behandelt.
    Bridged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortForward {
    /// "tcp" | "udp"
    pub proto: String,
    pub host_port: u16,
    pub guest_port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Network {
    pub mode: NetworkMode,
    pub forwards: Vec<PortForward>,
    /// Isoliert: Proxy zum Host anbieten (0.2) — 0.1 nur gespeichert.
    pub proxy: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Cpu {
    /// QEMU-CPU-Modell (`host`, `pentium`, `486`, `m68040`, `g4`, …).
    pub model: String,
    pub cores: u32,
    /// 100 = voller Takt; darunter wird per `-icount` gebremst (nur TCG).
    pub clock_percent: u32,
}

impl Default for Cpu {
    fn default() -> Self {
        Cpu { model: "host".into(), cores: 2, clock_percent: 100 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Devices {
    /// QEMU-Grafikgerät: `virtio-vga`, `virtio-vga-gl`, `cirrus-vga`, `VGA`, `std`, …
    pub graphics: String,
    /// Audio-Geräte: `sb16`, `adlib`, `ac97`, `intel-hda`, `es1370`, `gus`
    pub audio: Vec<String>,
    /// Netzwerkkarte: `virtio-net-pci`, `e1000`, `rtl8139`, `ne2k_isa`, `ne2k_pci`, `pcnet`
    pub nic: String,
    /// Bus der Systemplatte: `virtio` | `ide` | `sata` | `scsi`
    pub storage_bus: String,
    /// Diskettenlaufwerk(e) vorhanden.
    pub floppy: bool,
    /// Eingabe: `ps2` | `usb` | `virtio`
    pub input: String,
}

impl Default for Devices {
    fn default() -> Self {
        Devices {
            graphics: "virtio-vga".into(),
            audio: vec!["intel-hda".into()],
            nic: "virtio-net-pci".into(),
            storage_bus: "virtio".into(),
            floppy: false,
            input: "usb".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DiskRef {
    pub id: String,
    /// Relativ zum Maschinenordner oder absolut.
    pub path: String,
    /// `qcow2` | `raw` | `vmdk` | `vhdx` | `vdi`
    pub format: String,
    pub size_gb: u32,
    pub encrypted: bool,
    pub backing: Option<String>,
}

impl Default for DiskRef {
    fn default() -> Self {
        DiskRef {
            id: String::new(),
            path: "disk-0.qcow2".into(),
            format: "qcow2".into(),
            size_gb: 32,
            encrypted: false,
            backing: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    #[default]
    Cdrom,
    Floppy,
    /// Konsolen-ROM / Disc-Abbild für libretro; für QEMU-Maschinen das Firmware-ROM (q800).
    Rom,
    /// Physisches Laufwerk (0.4).
    Physical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct MediaRef {
    pub id: String,
    pub kind: MediaKind,
    pub path: String,
    /// Laufwerksplatz: Diskette 0/1, CD 0…
    pub slot: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct SharedFolder {
    pub host_path: String,
    pub name: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Isolation {
    /// `-snapshot`: Änderungen verfallen beim Beenden.
    pub throwaway: bool,
    /// Gemeinsame Ordner nur lesend einhängen.
    pub read_only_shares: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Display {
    /// `window` (Engine-eigenes Fenster) | `headless` | `vnc` | `spice`
    pub kind: String,
    pub gl: bool,
    pub hidpi: bool,
    /// Vollbild beim Start (libretro).
    pub fullscreen: bool,
}

impl Default for Display {
    fn default() -> Self {
        Display { kind: "window".into(), gl: false, hidpi: true, fullscreen: false }
    }
}

/// Konsolen-Maschine: welcher libretro-Core, welche BIOS-Dateien.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct LibretroConfig {
    pub core: String,
    pub fallback_core: Option<String>,
    /// Erwartete Dateien im System-/BIOS-Ordner (relativ).
    pub bios: Vec<String>,
    /// Aktueller Save-State-Slot (virtual verwaltet Slots, RetroArch kennt nur +/-).
    pub state_slot: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Machine {
    pub id: String,
    pub name: String,
    pub profile_id: String,
    /// `modern` | `retro-pc` | `retro-mac` | `other-arch` | `console`
    pub category: String,
    pub engine: Engine,
    pub arch: Arch,
    /// QEMU `-M` (q35, pc, virt, q800, mac99 …); bei libretro leer.
    pub machine_type: String,
    pub cpu: Cpu,
    pub memory_mb: u32,
    pub firmware: Firmware,
    pub tpm: bool,
    pub disks: Vec<DiskRef>,
    pub media: Vec<MediaRef>,
    pub network: Network,
    pub display: Display,
    pub devices: Devices,
    pub usb: Vec<String>,
    pub shared_folders: Vec<SharedFolder>,
    pub isolation: Isolation,
    pub libretro: Option<LibretroConfig>,
    /// Freie QEMU-Zusatzargumente (nested; Vorabzugang frei).
    pub extra_args: Vec<String>,
    pub notes: String,
    pub created_at: String,
    pub updated_at: String,
}

impl Machine {
    pub fn is_console(&self) -> bool {
        self.engine == Engine::Libretro
    }

    /// Medium eines Typs im Slot (erstes gefunden).
    pub fn media_of(&self, kind: MediaKind, slot: u32) -> Option<&MediaRef> {
        self.media.iter().find(|m| m.kind == kind && m.slot == slot)
    }
}

/// Snapshot-Eintrag in `snapshots.json` — bei QEMU ein interner qcow2-Snapshot
/// mit `tag`, bei libretro ein Save-State-Slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Snapshot {
    pub id: String,
    pub machine_id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub note: String,
    pub created_at: String,
    /// `internal` (qcow2-Tag) | `state` (libretro-Slot) | `overlay` (0.3)
    pub kind: String,
    /// qcow2-Tag bzw. Save-State-Slot als Text.
    pub tag: String,
}

/// Laufzeitzustand einer Maschine (nicht persistiert).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MachineStatus {
    pub id: String,
    /// `stopped` | `starting` | `running` | `paused` | `error`
    pub state: String,
    pub pid: Option<u32>,
    pub since: Option<String>,
    pub error: Option<String>,
}

/// Erkannte Engine auf dem Host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EngineInfo {
    /// `qemu` | `qemu-img` | `retroarch`
    pub engine: String,
    /// Binary-Name, z. B. `qemu-system-x86_64`
    pub binary: String,
    pub path: Option<String>,
    pub version: Option<String>,
    pub ok: bool,
    /// Nutzbarer Beschleuniger für die Host-Architektur (hvf/whpx/kvm) — nur bei qemu.
    pub accel: Option<String>,
    /// Installationshinweis, wenn nicht gefunden.
    pub hint: String,
}
