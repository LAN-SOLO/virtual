# virtual. — Plan

VM-Software mit zwei Engines: Hardware-Virtualisierung für aktuelle Gäste, Emulation für alte
Systeme auf der Hardware ihrer Zeit. Tauri 2 + React (Vite, TypeScript), Rust-Workspace mit
Tauri-freiem Kern. Terminal-Look wie die anderen LAN-SOLO-Apps (Mono-UI 13 px, `--radius: 4px`,
Uppercase-Mono-Labels, `//`-Hinweiszeilen), Dunkel/Hell, DE/EN, signierte In-App-Updates.
Kein Server, kein Konto, keine Telemetrie. Website: https://lan-solo.com/de/tools/virtual/

## Grundsatzentscheidung: Orchestrator statt eigener Hypervisor

Ein eigener Hypervisor oder ein eigener zyklengenauer PC-Emulator ist kein realistisches Ziel.
virtual ist deshalb ein **Orchestrator**: Die App besitzt Maschinen, Profile, Medien, Snapshots,
Isolation, UI und CLI — die Ausführung übernehmen bewährte Engines als **separate Prozesse**,
gesteuert über ihre Steuerprotokolle. So bleibt der Rust-/TypeScript-Code proprietär, die
GPL-Engines laufen unverändert als eigenständige Binaries daneben (Prozessgrenze, kein Linken).

| Engine | Lizenz | Rolle | Ab |
| --- | --- | --- | --- |
| **QEMU** (`qemu-system-*`) | GPLv2 | Arbeitstier: Virtualisierung per HVF (macOS), WHPX (Windows), KVM (Linux); TCG-Emulation für i386/x86_64 ohne Beschleuniger und für fremde Architekturen (m68k, ppc, mips, riscv, aarch64); qcow2, Snapshots, SLIRP-Netz, QMP | 0.1 |
| **86Box** | GPLv2 | Zyklengenaue Retro-PCs: echte Mainboards/Chipsätze, ISA-Sound (SB16, AdLib, OPL3), S3/Voodoo, exakte Taktraten — das Versprechen „Hardware ihrer Zeit" im Wortsinn | 0.3 |
| **Apple Virtualization.framework** | System | macOS-Gäste auf Apple Silicon (einziger legitimer Weg), Rosetta-Integration für x86-Linux-Binaries | 0.4 |
| **libretro / RetroArch** | GPLv3 | Spielkonsolen und Handhelds über libretro-Cores (NES, SNES, Mega Drive, Game Boy/GBA, N64, PS1, PSP, DS, Dreamcast, Arcade …) — ein Prozess, eine CLI, ein Steuerprotokoll | 0.2 |
| Standalone-Konsolen-Emulatoren (GameCube/Wii, PS2) | GPL | Adapter über deren CLI, sobald libretro-Cores dafür nicht reichen | später |
| Weitere Adapter (FS-UAE/vAmiga für Amiga, RPCEmu für RISC OS) | GPL | Über dieselbe Engine-Schnittstelle, Community-Profile | später |

**Engine-Beschaffung:** 0.1 nutzt eine installierte QEMU (Homebrew / winget / Distro-Paket) und
prüft beim Start Version und Pfad (Einstellungen → Engines, Hinweis mit Install-Befehl). Ab 0.2
liegt QEMU als Bundle im App-Paket (`engines/fetch-qemu.sh` pro Plattform, nicht im Git),
Systeminstallation bleibt als Fallback wählbar. GPL-Quellen und Lizenztexte werden in
„Über → Engines" verlinkt und im Bundle mitgeliefert.

## Zwei Engines im UI — der Nutzer wählt keine

Beim Anlegen wählt der Nutzer ein **Betriebssystem** (Liste aus `profiles/`), virtual wählt
Engine, Maschine, Beschleuniger und Geräte:

- **Virtualisierung** (Gast-Arch = Host-Arch): `-accel hvf|whpx|kvm`, `-M q35` (x86) bzw.
  `-M virt` (ARM), virtio-Geräte (Disk, Netz, GPU, Balloon, Tastatur/Maus), UEFI (OVMF/AAVMF),
  optional TPM 2.0 (swtpm, mitgeliefert) und Secure Boot, mehrere Kerne, `-device virtio-vga-gl`
  bzw. `virtio-gpu-gl` für beschleunigte Grafik, SPICE-Agent für Zwischenablage/Auflösung.
- **Emulation** (alte x86-Gäste): `-M pc` (i440FX + PIIX3), CPU-Modell passend zur Epoche
  (`486`, `pentium`, `pentium2`, `pentium3`), `-icount` als Taktregler, Cirrus-VGA/VESA,
  `sb16` + `adlib`, Diskette (`-drive if=floppy`, raw, schreibgeschützt), IDE-CD, RTL8139/NE2000-ISA, PS/2. Ohne Beschleuniger,
  reines TCG — absichtlich, damit Windows 95 nicht am Timer verschluckt.
- **Emulation** (fremde Architekturen): `-M q800` (68040, Mac OS 7.1–8.1, Quadra-800-ROM),
  `-M mac99`/`g3beige` (PowerPC, Mac OS 8.5–9.2, OpenBIOS), `-M next-cube` (NeXTSTEP 68k,
  experimentell), `-M virt` für Linux/BSD auf aarch64/riscv64/mips/ppc64.

## Tarif „nested" — was Free kann und was nicht

**Free (0 €):**
- Virtualisierung aktueller Gäste (mehrere Kerne, beschleunigte Grafik, TPM, Secure Boot)
- Emulation mit den **Standard-Profilen** (eine Maschine je Epoche, nicht einzeln konfigurierbar)
- Lineare Snapshots (qcow2 intern), Wegwerf-Modus (`-snapshot`), verschlüsselte Maschinen
  (LUKS-qcow2, Passwort im Schlüsselbund)
- Gemeinsame Ordner (virtiofs/9p bzw. SMB-Freigabe per SLIRP für alte Gäste), Zwischenablage,
  Drag-and-drop von Dateien, USB-Durchreichen, HiDPI
- Netz: **aus** (Standard für Retro-Profile), isoliert (`user,restrict=on` + Proxy zum Host),
  NAT, Bridged
- Formate: VMDK/VHDX/QCOW2/VDI/Raw öffnen und konvertieren (`qemu-img`), OVF/OVA-Import,
  IMG/ISO/BIN-CUE einlegen, physische Datenträger

**nested (12 €/Jahr):**
- Komplette Hardware-Bibliothek: Chipsatz, Sound- und Grafikkarte, Controller einzeln; Taktregler
  frei; ab 0.3 zusätzlich die 86Box-Maschinen
- Snapshot-Bäume mit Zweigen (externe qcow2-Overlays), verknüpfte Klone (Backing-File)
- Gast-Programme als eigene Fenster (SPICE-Agent + Fensterliste, erst 0.4)
- Headless-Betrieb, CLI `virtual` mit JSON-Ausgabe, verschachtelte Virtualisierung
  (`-cpu host,vmx|svm`), Port-Weiterleitungen, serielle Konsole, SSH/VNC-Zugriff
- Eigene Maschinenprofile exportieren, importieren, teilen

Im Code gibt es (wie bei allen bisherigen Apps) noch keine Lizenzprüfung: nested ist
umschaltbar und trägt das Badge mit dem Hinweis „im Vorabzugang freigeschaltet".

## Gäste-Liste der Website ↔ Engine (ehrlich)

| Gast | Engine / Maschine | Stand |
| --- | --- | --- |
| Windows 10/11, Linux, BSDs, Windows Server, Haiku, ReactOS | QEMU virtualisiert (q35/virt) | 0.1 |
| Windows 11 ARM | QEMU virt + HVF/WHPX auf ARM-Host; auf x86-Host TCG (langsam, Hinweis) | 0.1 |
| macOS (Gast auf Apple Silicon) | Virtualization.framework | 0.4 |
| MS-DOS, FreeDOS, Windows 3.1/95/98/ME, NT 4/2000/XP, OS/2 Warp, BeOS, QNX, Solaris x86, Minix, frühe Linux | QEMU `pc` (TCG); 86Box für echte Chipsatz-Treue | 0.1 / 0.3 |
| Mac OS 7–8.1 (68k) | QEMU `q800` (ROM vom Nutzer) | 0.2 |
| Mac OS 8.5–9.2 (PowerPC), A/UX (68k) | QEMU `mac99` / `q800` | 0.2 |
| NeXTSTEP x86 | QEMU `pc` | 0.2 |
| NeXTSTEP 68k | QEMU `next-cube` — experimentell, Profil als „beta" markiert | 0.3 |
| Plan 9, Linux/BSD für ARM/RISC-V/MIPS/PowerPC | QEMU `virt`/`malta` | 0.2 |
| AmigaOS | Engine-Adapter FS-UAE (Kickstart-ROM vom Nutzer) — **noch nicht in QEMU** | später |
| RISC OS | Engine-Adapter RPCEmu — **nicht in QEMU** | später |
| IRIX (MIPS) | Kein tauglicher Emulator in QEMU — auf der Website später relativieren oder streichen | offen |

Cray OS / UNICOS (Batmans Beispiel) hat keinen lauffähigen Emulator; bleibt unter
„Exoten per eigenem Maschinenprofil" und bedeutet: Wer eine Engine hat, kann sie anbinden.

## Spielkonsolen — dritte Gästeklasse

Konsolen und Handhelds sind Maschinen wie alle anderen: Profil, Medien (ROM/Disc-Abbild),
Snapshots (= Save States), Isolation (kein Netz), Ordner (Speicherstände). Engine ist
**RetroArch mit libretro-Cores** als separater Prozess — eine Kommandozeile für alle Systeme:

- Start: `retroarch -L <core> <rom> --appendconfig <machine.cfg>`; Konfiguration je Maschine
  (Vollbild, Shader, `savefile_directory`, `savestate_directory`, `system_directory` für BIOS,
  `network_cmd_enable = true`, `network_cmd_port`).
- Steuerung über das Netzwerk-Kommando-Interface (UDP, localhost): `PAUSE_TOGGLE`, `RESET`,
  `SAVE_STATE`, `LOAD_STATE`, `STATE_SLOT_PLUS/MINUS`, `SCREENSHOT`, `FULLSCREEN_TOGGLE`,
  `FAST_FORWARD`, `MENU_TOGGLE`, `QUIT` — damit werden Save States zu virtual-Snapshots mit
  Name und Notiz (Slot-Verwaltung liegt bei virtual, Datei = `<rom>.state<slot>`).
- Cores kommen vom libretro-Buildbot (`buildbot.libretro.com/nightly/<os>/<arch>/latest/<core>.zip`),
  virtual lädt sie auf Nutzerklick in den Core-Ordner; RetroArch selbst 0.2 als
  Systeminstallation (Homebrew / winget / Distro), ab 0.3 im Bundle.
- BIOS-Dateien liefert der Nutzer in den System-Ordner; das Profil nennt die erwarteten
  Dateinamen und prüft sie vor dem Start (Hinweis statt Absturz).

| System | Core | BIOS nötig |
| --- | --- | --- |
| NES / Famicom | mesen (Fallback nestopia) | nein |
| SNES | snes9x | nein |
| Game Boy / Color | gambatte | nein |
| Game Boy Advance | mgba | optional `gba_bios.bin` |
| Master System / Game Gear / Mega Drive | genesis_plus_gx | nein (Mega-CD: `bios_CD_E/U/J.bin`) |
| PC Engine / TurboGrafx | mednafen_pce_fast | CD: `syscard3.pce` |
| Nintendo 64 | mupen64plus_next | nein |
| PlayStation | swanstation (Fallback pcsx_rearmed) | `scph5500/5501/5502.bin` |
| PSP | ppsspp | nein |
| Nintendo DS | melonds | `bios7.bin`, `bios9.bin`, `firmware.bin` |
| Saturn | mednafen_saturn | `sega_101.bin`, `mpr-17933.bin` |
| Dreamcast | flycast | `dc/dc_boot.bin`, `dc/dc_flash.bin` |
| Neo Geo / Arcade | fbneo | `neogeo.zip` bzw. je Spiel |
| Atari 2600 | stella | nein |
| Atari Lynx / WonderSwan / NGPC | handy / mednafen_wswan / mednafen_ngp | Lynx: `lynxboot.img` |
| MSX | bluemsx | Machines-Datenbank |
| DOS-Spiele (alternativ zu QEMU) | dosbox_pure | nein |

GameCube/Wii und PlayStation 2 haben zwar libretro-Cores (dolphin, pcsx2), die aber schlechter
gepflegt sind als die Standalone-Emulatoren — Adapter über deren CLI kommen später. Spiele,
ROMs und BIOS-Dateien bringt der Nutzer mit; virtual lädt keine und verlinkt keine Quellen.

## Datenmodell

Machine{id,name,profileId,engine('qemu'|'libretro'|'box86'|'avf'),arch,machineType,cpu{model,cores,clockPercent},
memoryMb,firmware('bios'|'uefi'|'uefi-secure'),tpm,disks[DiskRef],media[MediaRef],
network{mode('off'|'isolated'|'nat'|'bridged'),forwards[{host,guest,proto}],proxy},display{type,gl,hidpi},
audio,usb[],sharedFolders[{hostPath,name,readOnly}],isolation{throwaway,readOnlyShares},
notes,createdAt,updatedAt} ·
DiskRef{id,path,format,sizeGb,backing?,encrypted} ·
MediaRef{id,kind('floppy'|'cdrom'|'physical'),path,slot} ·
Snapshot{id,machineId,parentId?,name,note,createdAt,kind('internal'|'overlay')} ·
Profile{id,name,epoch,os{family,versions[]},engine,arch,machineType,cpu,memoryMb,devices{...},
network,isolation,tier('free'|'nested'),beta,notes} ·
Settings{engines{qemuPath,mode('bundled'|'system')},machinesDir,defaultNetwork,theme,lang,nested}

Jede Maschine ist ein Ordner `<machinesDir>/<name>/` mit `machine.json`, `disk-*.qcow2`,
`snapshots/`, `nvram.fd`, `tpm/`. Kopieren = Backup, Verschieben = Umzug. `machine.json`
wird atomar geschrieben (Temp-Datei + rename). Profile liegen als JSON in `profiles/` im
Bundle; eigene Profile im Konfigurationsordner überlagern sie per `id`.

## Architektur

- `core/` (`virtual-core`): reine Logik ohne Tauri — `model`, `profile` (Laden, Überlagern,
  Validierung), `argv` (Machine → QEMU-Kommandozeile als reine Funktion; **hier sitzen die
  Unit-Tests**: jedes Profil erzeugt eine bekannte argv), `qmp` (JSON-Protokoll: Capabilities,
  `query-status`, `stop/cont`, `system_powerdown`, `snapshot-save/load/delete`, `blockdev-*`,
  `device_add` für USB, `screendump`), `qemu_img` (Format-Erkennung, convert, create, info),
  `ovf` (OVF/OVA lesen → Machine + Disks), `snapshot` (Baum, Overlays, Klone), `isolation`
  (Regeln: Retro-Profil ⇒ Netz aus + RO-Shares + throwaway-Vorschlag).
- `src-tauri/`: `engine/` (Trait `Engine { spawn, control, stop, display }`, Implementierung
  `qemu` mit Prozess-Supervisor, QMP-Unix-Socket/Named-Pipe, Log-Ringpuffer, Crash-Erkennung),
  `store` (Maschinen-Ordner, Settings), `media` (physische Laufwerke aufzählen), `shares`
  (virtiofsd-Start bzw. SMB-Export für alte Gäste), `commands` laut `src/api.ts`, Updater.
- `src/`: React-UI — `App.tsx` (Sidebar mit Maschinen und Status-LEDs, Detail, Konsole,
  Update-Banner), `components/` (MachineList, MachineDetail mit Tabs Hardware/Medien/Netz/
  Ordner/Snapshots, NewMachineWizard: OS wählen → Profil → Name/Disk → fertig, SnapshotTree,
  Console, Settings, Help), `api.ts` (Vertrag), `i18n.ts` (DE Sie-Form / EN), `styles.css`.
- `cli/` (`virtual-cli`, Rust-Binary, nested): `virtual list|create|start|stop|snapshot|clone|
  export|import --json`, spricht mit dem laufenden App-Prozess über Unix-Socket/Named-Pipe,
  sonst direkt mit `core` + Engine (headless).
- `profiles/`: JSON-Profile, z. B. `pc-1992-486.json`, `pc-1996-pentium133.json`,
  `pc-1999-pentium3.json`, `mac-1994-quadra.json`, `mac-1999-g3.json`, `modern-x86-uefi.json`,
  `modern-arm-uefi.json`, `linux-riscv.json`.
- `engines/`: Fetch-/Bundle-Skripte für QEMU, swtpm, virtiofsd und RetroArch je Plattform + Lizenztexte.
- `src-tauri/engine/libretro`: RetroArch-Prozess, UDP-Kommandos, Core-Download, BIOS-Prüfung.

**Anzeige:** 0.1 öffnet QEMU mit eigenem Fenster (`-display cocoa|gtk|sdl`) — schnell, robust,
Grafik hardwarebeschleunigt. 0.2 bettet die Anzeige in die App ein: SPICE-Client in Rust
(Framebuffer → Tauri-Channel → Canvas, Tastatur/Maus zurück), damit Tabs, Snapshots und
Konsole in einem Fenster wohnen. Zwischenablage/Auflösung über den SPICE-Agent im Gast; für
Retro-Gäste ohne Agent bleibt es beim Framebuffer.

**Sicherheit:** Retro-Profile starten ohne Netz; „isoliert" heißt `user,restrict=on` plus
ein kleiner HTTP/SOCKS-Proxy in Rust auf dem Host, den der Gast per fester IP erreicht —
kein Bridged, kein Zugriff aufs LAN. Gemeinsame Ordner für Retro-Gäste standardmäßig
schreibgeschützt. Wegwerf-Snapshot = `-snapshot`. QEMU läuft ohne Root, USB-Durchreichen
nur auf Nutzerklick je Gerät.

## Roadmap

- 0.1: Maschinen-Ordner, Standard-Profile, Wizard, QEMU-Erkennung (System), Virtualisierung
  x86/ARM mit UEFI + virtio, Retro-PC-Emulation (`pc`, TCG, `-icount`), Medien (ISO/IMG/CD),
  Netz aus/isoliert/NAT, lineare Snapshots, Wegwerf-Modus, QMP-Steuerung (Start/Stop/Pause/
  ACPI-Aus), QEMU-Fenster, Log-Ansicht, Handbuch, Updater.
- 0.2: Eingebettete Anzeige (SPICE), Zwischenablage/Drag-and-drop, gemeinsame Ordner (virtiofs +
  SMB für alte Gäste), QEMU im Bundle, `qemu-img`-Konvertierung, OVF/OVA-Import, USB,
  Mac-68k/PPC- und ARM/RISC-V-Profile, verschlüsselte Maschinen; Konsolen über RetroArch
  (Profile, Core-Download, Save States als Snapshots, BIOS-Prüfung).
- 0.3: nested-Umfang: Hardware-Bibliothek mit Einzelgeräten, Snapshot-Bäume, Klone, CLI mit JSON,
  Headless, Port-Weiterleitungen, serielle Konsole, Nested-Virt; 86Box-Engine mit eigenen
  Profilen; Profil-Export/-Import; Lizenzprüfung „nested".
- 0.4: Virtualization.framework für macOS-Gäste, Gast-Programme als eigene Fenster, TPM/Secure-
  Boot-Komfort (Schlüssel-Enrollment), Bridged-Netz mit Adapterwahl, physische Datenträger.
- später: Engine-Adapter für Amiga und RISC OS, Community-Profile, Übergabe an all/backed
  (Maschinen-Ordner als Backup-Quelle).

## Smoke-Test 0.1 auf macOS (2026-09-11) — Befunde

Getestet mit QEMU 11.1.1 (Homebrew) und RetroArch 1.22.2 auf Apple Silicon; Profile
„PC 1996 — Pentium 133" (FreeDOS-1.3-Boot-Diskette) und „Super Nintendo" (Homebrew-ROM).

- QEMU: `-name …,process=…` bricht außerhalb von Linux ab → Prozessname nur unter Linux.
- QEMU: Disketten werden als `-drive if=floppy,format=raw,readonly=on` eingelegt. Beschreibbare
  Disketten-Images (und das leere `null-co`-Laufwerk) blockieren sonst `savevm` für die ganze
  Maschine („Device 'floppy0' is writable but does not support snapshots").
- QEMU: Pause/Weiter, Reset, `savevm` im Lauf, ACPI-Aus (ohne ACPI-Gast wirkungslos → `quit`
  nach 10 s) und `quit` funktionieren; Boot von Diskette trotz `-boot order=c` (SeaBIOS fällt durch).
- RetroArch: `GET_STATUS` über UDP stürzt in 1.22.2 mit SIGSEGV ab — im x86_64- **und** im
  nativen Metal-Build. Der Pausenzustand wird deshalb lokal geführt; `GET_STATUS` bleibt tabu,
  bis ein Upstream-Fix da ist.
- RetroArch: Save States landen standardmäßig in `states/<Core>/…` → `sort_savestates_enable`
  und Verwandte aus; ebenso `history_list_enable` (kein Verlauf in ~/Documents/RetroArch) und
  `quit_press_twice` (sonst braucht QUIT zwei Kommandos).
- RetroArch: der Homebrew-Cask `retroarch` ist nur x86_64 und hängt unter Rosetta nach QUIT im
  Beenden (Daten sind gesichert, die App killt nach 5 s). Der Cask `retroarch-metal` ist universal,
  beendet sauber → Installationshinweis geändert. Cores müssen zur Binary-Architektur passen.
- Hilfsprogramm: `cargo run -p virtual-core --example smoke` legt Maschinen aus Profilen an,
  druckt die erzeugte Kommandozeile und wandelt Abbilder (`convert`) — Grundlage der Smoke-Skripte.

## Installieren von CD-Abbild (2026-09-11)

- `core/src/image.rs`: NRG (Nero v1/v2, DAO und TAO), BIN/CUE, MDF/MDS, CCD/IMG und rohe
  2352/2336-Byte-Abbilder werden beim Einlegen als `<stem>.iso` in den Maschinenordner gewandelt
  (erste Datenspur, streamend). ISO 9660 bleibt unangetastet; Unbekanntes (z. B. reine HFS-CD)
  wird QEMU roh gereicht. `attach_media`/`create_machine` laufen dafür asynchron (spawn_blocking).
- `Machine.boot`: `auto` (CD zuerst, wenn eingelegt; dann Platte, dann Diskette) | `disk` |
  `cdrom` | `floppy`. Auf PC-Boards hängen Platten/CDs jetzt als `-device ide-hd/ide-cd` mit
  bootindex (primärer/sekundärer Kanal), Disketten über `-global isa-fdc.bootindexA/B`.
  Ausdrückliche Wahl = nur diese Klasse gelistet + `-boot strict=on` → SeaBIOS hält an, statt
  auf die CD zurückzufallen (`-boot order=c,strict=on` allein reicht nicht: ohne bootindex-
  Geräte hängt QEMU kein HALT an). Geprüft: TinyCore-NRG → ISO identisch, bootet; `disk` strikt
  → „No bootable device“; Diskette auto und strikt → FreeDOS.
- Offen: Medienwechsel im Lauf (0.2), Audio-/Multi-Track-CDs, Fortschrittsanzeige bei großen
  Abbildern (Wandlung eines 700-MB-Abbilds dauert wenige Sekunden von SSD).
