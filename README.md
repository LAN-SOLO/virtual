# virtual.

Virtuelle Maschinen für heute und für damals — lokal, ohne Konto, ohne Telemetrie.

- **Virtualisierung:** aktuelle Windows-, Linux- und BSD-Gäste in nativer Geschwindigkeit über
  die Hardware-Virtualisierung des Hosts (HVF / WHPX / KVM), UEFI, virtio, TPM 2.0, Secure Boot.
- **Emulation:** alte Systeme von DOS über Windows 95 bis Mac OS 9 auf der Hardware ihrer Zeit —
  passender Chipsatz, ISA-Sound, VGA, Diskette, gedrosselter Takt; auch für Architekturen, die
  der Host nicht hat (68k, PowerPC, MIPS, RISC-V).
- **Maschinenprofile:** ein JSON beschreibt einen kompletten Rechner („PC 1996 — Pentium 133,
  Sound Blaster 16, S3"). Betriebssystem wählen, Profil bestätigen, installieren.
- **Sicher isoliert:** ungepatchte Alt-Systeme starten ohne Netz, mit schreibgeschützten
  Ordnern und Wegwerf-Snapshot; isoliertes Gast-Netz mit Proxy, NAT und Bridged bewusst zuschaltbar.
- **Free** bleibt kostenlos; **nested** (12 €/Jahr) bringt die komplette Hardware-Bibliothek,
  Snapshot-Bäume, Klone, Headless-Betrieb und CLI.

virtual ist ein Orchestrator: Maschinen, Profile, Medien, Snapshots, Isolation, UI und CLI sind
eigener Code; die Ausführung übernehmen bewährte Engines (QEMU, später 86Box und Apples
Virtualization.framework) als separate Prozesse. Details, Datenmodell und Roadmap: `VIRTUAL_PLAN.md`.

- **Konsolen:** Spielkonsolen und Handhelds (NES bis Dreamcast, Arcade, MSX) als dritte
  Maschinenklasse über RetroArch/libretro-Cores — Save States werden zu Snapshots.

Status: Version 0.1 in Arbeit (Vorabzugang, keine Downloads) — Website: https://lan-solo.com/de/tools/virtual/

## Voraussetzungen

virtual bringt in 0.1 noch keine Engines mit, sondern nutzt installierte:

| Engine | macOS | Windows | Linux |
| --- | --- | --- | --- |
| QEMU (Pflicht für PCs/Macs) | `brew install qemu` | `winget install SoftwareFreedomConservancy.QEMU` | `apt install qemu-system` / `dnf install qemu` |
| RetroArch (für Konsolen) | `brew install --cask retroarch-metal` | `winget install Libretro.RetroArch` | `apt install retroarch` / Flatpak |
| swtpm (optional, TPM 2.0) | `brew install swtpm` | — | `apt install swtpm` |

Cores lädt man in RetroArch unter „Online-Updater → Core herunterladen“; BIOS-Dateien gehören
in den System-Ordner (Einstellungen → Engines zeigt die Pfade).

## Entwicklung

```sh
pnpm install
pnpm tauri dev
cargo test --workspace
```

Der Rust-Kern (`core/`, Crate `virtual-core`) ist Tauri-frei und testbar: Datenmodell,
Maschinenprofile (`profiles/*.json`, eingebettet), QEMU-Kommandozeile als reine Funktion,
QMP-Nachrichten, RetroArch-Kommandozeile und -Konfiguration, Engine-Erkennung. Die Kommandos
in `src-tauri/src/commands.rs` folgen dem Vertrag in `src/api.ts`.

## Release-Build (lokal)

```sh
TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/virtual-updater.key)" \
TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" \
pnpm tauri build --bundles app,dmg
```

Details und Roadmap: `VIRTUAL_PLAN.md`.
