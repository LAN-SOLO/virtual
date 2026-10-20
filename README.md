# virtual.

Virtuelle Maschinen für heute und für damals — lokal, ohne Konto, ohne Telemetrie.

- **Virtualisierung:** aktuelle Windows-, Linux- und BSD-Gäste in nativer Geschwindigkeit über
  die Hardware-Virtualisierung des Hosts (HVF / WHPX / KVM), UEFI, virtio, TPM 2.0, Secure Boot.
- **Emulation:** alte Systeme von DOS über Windows 95 bis Mac OS 9 auf der Hardware ihrer Zeit —
  passender Chipsatz, ISA-Sound, VGA, Diskette, gedrosselter Takt; auch für Architekturen, die
  der Host nicht hat (68k, PowerPC, MIPS, RISC-V).
- **Maschinenprofile:** ein JSON beschreibt einen kompletten Rechner („PC 1996 — Pentium 133“:
  Sound Blaster 16, Cirrus-VGA). Betriebssystem wählen, Profil bestätigen, installieren.
- **Sicher isoliert:** ungepatchte Alt-Systeme starten ohne Netz und mit schreibgeschützten
  Ordnern (eine SMB-Freigabe gibt es dann nicht — sie wäre immer beschreibbar), Wegwerf-Modus auf
  Wunsch; isoliertes Gast-Netz (nur Weiterleitungen/Freigaben) und NAT bewusst zuschaltbar
  (Bridged folgt später und verhält sich bis dahin wie NAT).
- **Free** bleibt kostenlos; **nested** (12 €/Jahr) soll die komplette Hardware-Bibliothek,
  Snapshot-Bäume, Klone, Headless-Betrieb und eine CLI bringen (in 1.0 noch nicht enthalten;
  im Vorabzugang schaltet der Tarif nur die freien Engine-Argumente frei).

virtual ist ein Orchestrator: Maschinen, Profile, Medien, Snapshots, Isolation und UI sind
eigener Code; die Ausführung übernehmen bewährte Engines (QEMU, später 86Box und Apples
Virtualization.framework) als separate Prozesse. Details, Datenmodell und Roadmap: `VIRTUAL_PLAN.md`.

- **Konsolen:** Spielkonsolen und Handhelds (NES bis Dreamcast, Arcade, MSX) als dritte
  Maschinenklasse über RetroArch/libretro-Cores — Save States werden zu Snapshots.

Status: Version 1.0 — Testfassung für macOS und Windows (Audit: `AUDIT-1.0.md`) — Website: https://lan-solo.com/de/tools/virtual/

## Voraussetzungen

virtual bringt keine Engines mit, sondern nutzt installierte:

| Engine | macOS | Windows | Linux |
| --- | --- | --- | --- |
| QEMU (Pflicht für PCs/Macs) | `brew install qemu` | `winget install SoftwareFreedomConservancy.QEMU` | `apt install qemu-system` / `dnf install qemu` |
| RetroArch (für Konsolen) | `brew install --cask retroarch-metal` | `winget install Libretro.RetroArch` | `apt install retroarch` / Flatpak |
| swtpm (optional, TPM 2.0) | `brew install swtpm` | — | `apt install swtpm` |
| Samba (optional, SMB-Ordner für alte Gäste) | `brew install samba` | — | `apt install samba` |

Windows: Für die Beschleunigung (WHPX) das Windows-Feature „Windows-Hypervisorplattform“ aktivieren.

Cores lädt man in RetroArch unter „Online-Updater → Core herunterladen“; BIOS-Dateien gehören
in den System-Ordner (Einstellungen → Engines zeigt die Pfade).

## Entwicklung

```sh
pnpm install
pnpm tauri dev
cargo test --workspace
pnpm test:ui          # Übersetzung der Backend-Meldungen (Node ≥ 22.6, ohne Abhängigkeiten)
# gegen echte Engines (macOS/Linux, QEMU bzw. RetroArch installiert):
cargo test -p virtual -- --ignored qemu_live
cargo run -p virtual-core --example qemucheck
cargo run -p virtual-core --example retrocheck <retroarch> <cores-ordner> <rom>
```

Backend-Meldungen sind deutsch; `src/backendText.ts` übersetzt sie für die englische
Oberfläche. Neue Fehlermeldungen brauchen dort ein Muster — `pnpm test:ui` liest alle
Meldungen aus `src-tauri/` und `core/` und schlägt sonst fehl. Profile tragen ihre englischen
Anzeigetexte im Block `en` (Test: `every_builtin_profile_has_english_texts`).

Vorgänge je Maschine (Start, Snapshot, Medium wandeln, Löschen, Speichern/Medium entfernen)
belegen die Maschine über `AppState::begin` (`src-tauri/src/state.rs`); ein zweiter Vorgang
wird abgelehnt. Steuerkommandos warten höchstens 3 s auf QMP, „Stopp erzwingen“ gar nicht.
Beenden (`quit_app`) und Update (`install_update`) beenden die Engines und warten dann über
`AppState::wait_idle` bis zu 60 s auf laufende Vorgänge (z. B. `qemu-img snapshot`), statt sie
abzuschneiden; `busy_ops` sagt der Oberfläche, ob sie dafür nachfragen muss.

Sicherheitshinweise: RetroArch lauscht mit seinem Kommando-Interface (UDP) auf allen
Schnittstellen — virtual kann das nicht auf localhost beschränken (siehe `AUDIT-1.0.md`, Runde 3).
Unter Windows läuft QMP über TCP 127.0.0.1 ohne Anmeldung; virtual hält die einzige Verbindung.

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
