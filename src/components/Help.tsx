import { useEffect, useState } from 'react';
import { Lang } from '../i18n';
import { shortcut } from '../platform';

// Selbstständiges Hilfe-System: schwebender ?-Button, First-Run-Tutorial
// und durchsuchbares Handbuch. Inhalte liegen bewusst hier, nicht in i18n.ts.

const SEEN_KEY = 'virtual.tutorialSeen';

interface Step {
  title: string;
  body: string[];
}

interface Section {
  id: string;
  title: string;
  body: string[];
}

interface Content {
  labels: {
    fab: string;
    tutorial: string;
    manual: string;
    search: string;
    next: string;
    back: string;
    skip: string;
    done: string;
    stepOf: (n: number, total: number) => string;
    noResults: string;
  };
  tutorial: Step[];
  sections: Section[];
}

const de: Content = {
  labels: {
    fab: 'Hilfe & Handbuch',
    tutorial: 'Tutorial',
    manual: 'Handbuch',
    search: 'Handbuch durchsuchen …',
    next: 'Weiter',
    back: 'Zurück',
    skip: 'Überspringen',
    done: 'Los geht’s',
    stepOf: (n, total) => `Schritt ${n} von ${total}`,
    noResults: 'Keine Treffer',
  },
  tutorial: [
    {
      title: 'Willkommen bei virtual.',
      body: [
        'virtual führt virtuelle Maschinen aus — für heute und für damals: aktuelle Windows-, Linux- und BSD-Gäste in nativer Geschwindigkeit, alte Systeme von DOS über Windows 95 bis Mac OS 9 auf der Hardware ihrer Zeit, dazu Spielkonsolen.',
        'virtual ist ein Orchestrator: Maschinen, Profile, Medien, Snapshots und Isolation sind seine Sache — die Ausführung übernehmen QEMU und RetroArch als separate Prozesse. Beide müssen installiert sein (Einstellungen → Engines zeigt, was gefunden wurde).',
        'Alles bleibt auf Ihrem Rechner: Jede Maschine ist ein Ordner mit machine.json, Platten und Snapshots. Kein Konto, keine Cloud, keine Telemetrie.',
        'Dieses Tutorial dauert zwei Minuten. Sie finden es jederzeit wieder über den ?-Knopf unten rechts.',
      ],
    },
    {
      title: 'Maschine anlegen',
      body: [
        '„+ Maschine“ in der Seitenleiste öffnet den Assistenten. Sie wählen kein Gerät, sondern ein Betriebssystem — virtual bringt das passende Maschinenprofil mit.',
        '• Ein Profil beschreibt einen kompletten Rechner: „PC 1996 — Pentium 133, Sound Blaster 16, S3“ oder „Mac 1994 — Quadra 800“. Chipsatz, Takt, Grafik, Sound und Laufwerke sind damit gesetzt.',
        '• Schritt 2: Name, Größe der Systemplatte und optional das Installationsmedium (ISO oder Diskettenabbild). Manche Profile brauchen ein Firmware-ROM — das liefern Sie selbst.',
        '• Schritt 3 zeigt die Zusammenfassung. „Anlegen“ erzeugt Ordner und Platte. Danach: „Start“.',
      ],
    },
    {
      title: 'Zwei Engines, ein Fenster',
      body: [
        'Aktuelle Gäste laufen virtualisiert: Der Prozessor führt sie direkt aus (HVF auf macOS, WHPX auf Windows, KVM auf Linux) — mit mehreren Kernen, UEFI, TPM 2.0 und beschleunigter Grafik.',
        'Alte Gäste laufen emuliert: Prozessor, Chipsatz, ISA-Bus und Soundkarte werden nachgebildet, der Takt gebremst, damit Windows 95 nicht am Timer verschluckt. Auch fremde Architekturen (68k, PowerPC, MIPS, RISC-V) laufen so.',
        '• Sie müssen sich nicht entscheiden: Das Profil legt die Engine fest. Unter „Übersicht“ sehen Sie die exakte Kommandozeile, die virtual ausführt.',
        '• Die Anzeige öffnet sich als eigenes Engine-Fenster (QEMU bzw. RetroArch); eine in virtual eingebettete Anzeige ist für eine spätere Version geplant.',
      ],
    },
    {
      title: 'Alte Systeme sicher',
      body: [
        'Systeme, die seit Jahrzehnten keine Updates bekommen, gehören nicht ins Netz. virtual startet Retro-Profile deshalb ohne Netzwerkgerät, mit schreibgeschützten Ordnern und auf Wunsch im Wegwerf-Modus.',
        '• Netzwerk „isoliert“: Der Gast erreicht nur den Host — nie das LAN. „NAT“ lässt ihn ins Internet, „aus“ ist der sicherste Zustand.',
        '• Wegwerf-Modus: Alles, was der Gast schreibt, verfällt beim Beenden. Ideal zum Ausprobieren unbekannter Software.',
        '• Snapshots: Zustand einfrieren, bevor Sie etwas installieren — und in Sekunden zurück, wenn es schiefging.',
      ],
    },
    {
      title: 'Konsolen',
      body: [
        'Spielkonsolen und Handhelds sind Maschinen wie alle anderen — die Engine ist RetroArch mit libretro-Cores. Das Profil nennt den Core (z. B. snes9x, mgba, swanstation) und die BIOS-Dateien, die das System braucht.',
        '• Cores laden Sie einmalig in RetroArch unter „Online-Updater → Core herunterladen“; virtual findet sie im Cores-Ordner (Einstellungen → Engines).',
        '• BIOS-Dateien gehören in den System-Ordner. Unter „Medien“ zeigt virtual, was fehlt. Spiele, ROMs und BIOS-Dateien bringen Sie selbst mit — virtual lädt keine und verlinkt keine Quellen.',
        '• Snapshots einer Konsole sind Save-States: nur möglich, während das Spiel läuft. Speicherstände liegen im Maschinenordner unter saves/ und states/.',
      ],
    },
    {
      title: 'Medien, Ordner, Netzwerk',
      body: [
        '• „Medien“: CDs (ISO, NRG, BIN/CUE, MDF), Disketten (IMG) und Firmware-ROMs einlegen oder entfernen — wirkt beim nächsten Start. Diskette A: und B: für die Installation von 13 Disketten.',
        '• Installieren von CD: Abbild einlegen, „Start“ — die Maschine bootet vom Medium. Nach der Installation unter „Hardware“ → „Booten von“ auf „Platte“ stellen oder die CD auswerfen, sonst startet wieder das Installationsmedium. Windows 9x: Boot-Diskette einlegen, CD dazu, von A: starten und setup von der CD aufrufen.',
        '• „Netzwerk“: Modus, Port-Weiterleitungen (z. B. Host 2222 → Gast 22 für SSH, nur auf 127.0.0.1) und gemeinsame Ordner. Aktuelle Gäste hängen Ordner per virtio-9p ein, alte Gäste über die SMB-Freigabe des Gast-Netzes (braucht Samba auf dem Host).',
        '• „Hardware“: Kerne, Speicher, Taktbremse, Firmware, Grafik, Audio, Netzkarte, Platten-Bus, Diskettenlaufwerk, Eingabe und Anzeige — änderbar, solange die Maschine aus ist.',
        '• „Log“ zeigt die Ausgabe der Engine — der erste Blick, wenn ein Start fehlschlägt.',
      ],
    },
  ],
  sections: [
    {
      id: 'v1',
      title: 'Version 1.0',
      body: [
        'Version 1.0 ist die erste Testfassung für macOS und Windows nach einem vollständigen Funktions- und Sicherheits-Audit.',
        '• Aktuelle Profile (UEFI x86, ARM) starten wieder: HD-Audio bekommt seinen Codec.',
        '• Namen und Pfade mit Komma brechen die Kommandozeile nicht mehr; lange Maschinennamen ebenso wenig.',
        '• Port-Weiterleitungen und VNC nur auf 127.0.0.1; geöffnet werden nur https-Links und Maschinenordner, nie Dateien oder Programme.',
        '• Fenster friert bei Snapshots und Start nicht mehr ein; Windows öffnet keine Konsolenfenster mehr.',
        '• Schließen mit laufenden Maschinen fragt nach; was der Host nicht kann (TPM, OpenGL, Ordner), steht im Log.',
        '• Zweite Prüfrunde: Meldungen der Engine-Steuerung erscheinen in der Sprache der Oberfläche, Profile haben englische Texte.',
        '• Pro Maschine läuft immer nur ein Vorgang (Start, Snapshot, Medium, Löschen) — ein zweiter wird mit Hinweis abgelehnt. Während eines langen Snapshots melden Pause/Stopp „Engine beschäftigt“, „Stopp erzwingen“ greift sofort.',
        '• Update mit laufenden Maschinen fragt nach und beendet sie vor der Installation; Fehler beim Update werden angezeigt.',
        '• Windows: CUE-Dateien mit BOM, Windows-1252-Umlauten und \\-Pfaden, Profile/JSON aus dem Editor (BOM), reservierte Namen wie „CON“, Laufwerksangabe „D:“ und Pfade in Anführungszeichen werden vertragen.',
      ],
    },
    {
      id: 'engines',
      title: 'Engines installieren',
      body: [
        'virtual bringt keine Engines mit, sondern nutzt installierte. Einstellungen → Engines zeigt, was gefunden wurde, und den Installationsbefehl.',
        '• macOS: brew install qemu · brew install --cask retroarch-metal · brew install swtpm (für TPM 2.0).',
        '• Windows: winget install SoftwareFreedomConservancy.QEMU · winget install Libretro.RetroArch. Für die Beschleunigung (WHPX) in den Windows-Features „Windows-Hypervisorplattform“ aktivieren. swtpm gibt es unter Windows nicht — TPM entfällt dort; Cores und System-Ordner sucht virtual neben retroarch.exe.',
        '• Linux: sudo apt install qemu-system retroarch swtpm (Debian/Ubuntu) bzw. sudo dnf install qemu retroarch swtpm (Fedora).',
        '• Liegt eine Engine an einem ungewöhnlichen Ort, tragen Sie den Pfad in den Einstellungen ein. „Erneut suchen“ prüft sofort. Anführungszeichen (Explorer „Als Pfad kopieren“) entfernt virtual; ein nacktes „D:“ wird zu „D:\\“.',
        '• QEMU (GPLv2) und RetroArch (GPLv3) laufen als eigenständige Prozesse und werden nicht in virtual gelinkt.',
      ],
    },
    {
      id: 'profiles',
      title: 'Maschinenprofile',
      body: [
        'Ein Profil ist eine JSON-Datei, die einen kompletten Rechner beschreibt: Engine, Architektur, Maschinentyp, CPU, Speicher, Firmware, Geräte, Netzwerk-Standard und Isolation.',
        '• Kategorien: Aktuell (virtualisiert), Retro-PC (486 bis Pentium 4), Retro-Mac (68k und PowerPC), andere Architekturen (RISC-V, MIPS) und Konsolen.',
        '• Profile mit „beta“ sind experimentell — die Emulation der Maschine ist in QEMU noch nicht ausgereift (Quadra 800, Power Mac G3, Malta).',
        '• Profile aus dem Tarif „nested“ sind im Vorabzugang ebenfalls nutzbar.',
        '• Eigene Profile: JSON-Dateien im Profile-Ordner des Konfigurationsverzeichnisses überlagern die eingebauten per id (Export/Import in der App ist für eine spätere Version geplant). UTF-8 mit oder ohne BOM; englische Anzeigetexte optional im Block „en“ (name, family, versions, hint, notes).',
      ],
    },
    {
      id: 'virtualization',
      title: 'Virtualisierung (aktuelle Gäste)',
      body: [
        'Gast-Architektur = Host-Architektur → virtual nutzt den Beschleuniger des Hosts: HVF (macOS), WHPX (Windows), KVM (Linux). Auf Apple Silicon laufen ARM-Gäste nativ, x86-Gäste emuliert — und entsprechend langsam.',
        '• UEFI-Firmware (OVMF/AAVMF) kommt aus dem QEMU-Paket; die NVRAM-Kopie liegt als nvram.fd im Maschinenordner.',
        '• „UEFI + Secure Boot“ startet die Secure-Boot-Firmware mit SMM — auf q35 mit TCG oder KVM. Mit HVF/WHPX startet die Maschine mit normalem UEFI (Hinweis im Log). Schlüssel sind nicht vorinstalliert: Secure Boot bei Bedarf im UEFI-Setup einrichten.',
        '• TPM 2.0 braucht swtpm — ohne läuft die Maschine, aber ohne TPM (Windows 11 verlangt es bei der Installation).',
        '• Grafik: virtio-vga bzw. virtio-gpu; „OpenGL-Beschleunigung“ nutzt virtio-vga-gl, wo der QEMU-Build OpenGL kann (Linux, Windows). Homebrew-QEMU auf macOS hat kein OpenGL — dort läuft die Anzeige ohne GL. Eingabe per USB-Tablet, damit die Maus nicht gefangen wird.',
        '• CPU-Modell „host“ gibt es nur mit Beschleuniger; ohne setzt virtual automatisch ein passendes Modell.',
      ],
    },
    {
      id: 'emulation',
      title: 'Emulation (alte Gäste)',
      body: [
        'Retro-Profile laufen ohne Beschleuniger (TCG), damit Takt und Timer der alten Kernel stimmen. Die Taktbremse (Hardware → Takt) bildet 60–75 % einer damaligen Maschine nach.',
        '• PC-Profile: i440FX/PIIX3 oder reine ISA-Maschine, Cirrus-VGA mit VESA, Sound Blaster 16 + AdLib, NE2000 oder RTL8139, Diskettenlaufwerk A: und B:.',
        '• Mac-Profile: Quadra 800 (68040) braucht das Original-ROM als „Firmware-ROM“; Power Mac G3 startet mit OpenBIOS ohne ROM.',
        '• Andere Architekturen: RISC-V „virt“ und MIPS „Malta“ für Linux/BSD-Kernel.',
        '• Zyklengenaue Chipsatz-Treue (86Box) ist für eine spätere Version im Tarif nested geplant.',
      ],
    },
    {
      id: 'consoles',
      title: 'Konsolen (RetroArch / libretro)',
      body: [
        'Konsolen-Profile starten RetroArch mit einem libretro-Core: -L <core> <spiel> --appendconfig <maschine.cfg>. Die Steuerung läuft über das Netzwerk-Kommando-Interface von RetroArch (UDP, nur localhost).',
        '• Cores: RetroArch → Online-Updater → Core herunterladen. Der Cores-Ordner steht in den Einstellungen; unter macOS auch RetroArch.app/Contents/Resources/cores.',
        '• BIOS: PlayStation scph5500/5501/5502.bin, Saturn sega_101.bin + mpr-17933.bin, Dreamcast dc/dc_boot.bin + dc/dc_flash.bin, Nintendo DS bios7.bin + bios9.bin + firmware.bin, Mega-CD bios_CD_*.bin, PC Engine CD syscard3.pce, Neo Geo neogeo.zip. Alles in den System-Ordner.',
        '• Save-States: „Snapshot erstellen“ speichert einen State im nächsten freien Slot; „Wiederherstellen“ lädt ihn. Nur während das Spiel läuft.',
        '• Speicherstände (Batterie-RAM) liegen in saves/, Screenshots in screenshots/ — alles im Maschinenordner.',
        '• Pause/Fortsetzen, Reset und Stopp funktionieren wie bei PC-Maschinen. Netplay und Achievements sind deaktiviert.',
      ],
    },
    {
      id: 'isolation',
      title: 'Isolation & Sicherheit',
      body: [
        '• Netzwerk „aus“: kein Netzwerkgerät. Standard für Retro-Profile und Konsolen.',
        '• „isoliert“: SLIRP mit restrict=on — der Gast sieht nur Weiterleitungen und Freigaben, nie das LAN.',
        '• „NAT“: Der Gast kommt ins Internet, von außen kommt nichts hinein (außer Weiterleitungen). „Bridged“ folgt in einer späteren Version und verhält sich bis dahin wie NAT.',
        '• Port-Weiterleitungen lauschen nur auf 127.0.0.1 — andere Rechner im LAN erreichen den Gast darüber nicht. VNC ebenso (127.0.0.1:5900, weitere Maschinen ab 5901).',
        '• Gemeinsame Ordner sind für Retro-Profile schreibgeschützt (Hardware → Isolation).',
        '• Wegwerf-Modus startet QEMU mit -snapshot: Schreibzugriffe landen in einer temporären Datei und verfallen beim Beenden.',
        '• Engines laufen ohne Root-Rechte. Die QMP-Steuerung läuft über einen Unix-Socket im Benutzerkontext (Windows: TCP nur auf 127.0.0.1). USB-Durchreichung ist für eine spätere Version geplant.',
        '• virtual öffnet aus der Oberfläche nur https-Links und Ordner innerhalb des Maschinenordners — nie Dateien oder Programme (auch keine .app-Bundles).',
      ],
    },
    {
      id: 'snapshots',
      title: 'Snapshots',
      body: [
        '• PC- und Mac-Maschinen: interne qcow2-Snapshots. Läuft die Maschine, wird der Speicherzustand mitgesichert (savevm über QMP); ist sie aus, nur die Platte (qemu-img snapshot).',
        '• Im Wegwerf-Modus lehnt virtual Snapshots im laufenden Betrieb ab — sie würden beim Beenden mit verfallen. Bei gestoppter Maschine geht es.',
        '• Wiederherstellen ersetzt den aktuellen Zustand — deshalb die Nachfrage.',
        '• Konsolen: Save-States, nur im laufenden Spiel.',
        '• Snapshot-Bäume mit Zweigen und verknüpfte Klone sind für eine spätere Version geplant (nested).',
        '• Die Liste liegt als snapshots.json im Maschinenordner.',
        '• Ein Snapshot mit viel Gast-RAM dauert. Solange er läuft, lehnt virtual Start, Löschen und einen zweiten Snapshot derselben Maschine ab; Pause und Stopp melden „Engine beschäftigt“, „Stopp erzwingen“ beendet sofort (der Snapshot ist dann verloren).',
      ],
    },
    {
      id: 'media',
      title: 'Medien & Ordner',
      body: [
        '• CDs: ISO direkt; NRG (Nero), BIN/CUE, MDF/MDS (Alcohol), CCD/IMG (CloneCD) und rohe 2352-Byte-Abbilder wandelt virtual beim Einlegen in ein ISO im Maschinenordner (nur die erste Datenspur, Audio-CDs nicht). Disketten: IMG, IMA, DSK, VFD (schreibgeschützt). Firmware-ROM: beliebige Datei (Quadra 800: 1 MB ROM).',
        '• „Booten von“ (Hardware): automatisch = CD zuerst, wenn eingelegt; sonst Platte, CD/DVD oder Diskette fest wählen. Esc im BIOS-Startbild öffnet das Bootmenü.',
        '• Medien wirken beim nächsten Start; ein Wechsel im laufenden Betrieb ist für eine spätere Version geplant. Zwei gleichnamige Abbilder aus verschiedenen Ordnern werden als <name>.iso und <name>-2.iso gewandelt.',
        '• Gemeinsame Ordner: virtio-9p (mount -t 9p -o trans=virtio <name> /mnt) für Gäste am virtio-Bus; alte Gäste über die SMB-Freigabe im Gast-Netz (\\\\10.0.2.4\\qemu, nur der erste Ordner). SMB braucht Samba auf dem Host (macOS: brew install samba, Linux: apt install samba). QEMU für Windows kann weder 9p noch SMB — was der Host nicht kann, steht beim Start im Log.',
        '• „Ordner öffnen“ in der Übersicht zeigt den Maschinenordner: machine.json, disk-0.qcow2, nvram.fd, snapshots.json, log.txt.',
      ],
    },
    {
      id: 'nested',
      title: 'Tarif nested',
      body: [
        'Free bleibt kostenlos — dauerhaft: Virtualisierung, Emulation mit Standard-Profilen, lineare Snapshots, Wegwerf-Modus, Ordner, Netzwerk, alle Formate.',
        '• nested (12 € im Jahr): komplette Hardware-Bibliothek mit Einzelgeräten und freiem Taktregler, Snapshot-Bäume und Klone, Gast-Programme als eigene Fenster, Headless-Betrieb, CLI mit JSON-Ausgabe, verschachtelte Virtualisierung, freie Engine-Argumente, eigene Profile teilen.',
        '• Im Vorabzugang gibt es keine Lizenzprüfung: Einstellungen → Allgemein → „Tarif nested aktivieren“.',
      ],
    },
    {
      id: 'shortcuts',
      title: 'Tastaturkürzel',
      body: ['• n — neue Maschine', '• ? — Handbuch', `• ${shortcut('de', ',')} — Einstellungen`, '• Esc — Dialog schließen'],
    },
    {
      id: 'trouble',
      title: 'Fehlerbehebung',
      body: [
        '• Die macOS-Builds sind signiert und notarisiert. Nur bei selbst gebauten, unsignierten Builds meldet macOS „beschädigt“ — dann einmal: xattr -dr com.apple.quarantine /Applications/virtual.app',
        '• Windows: „WHPX: No accelerator found“ im Log → in den Windows-Features „Windows-Hypervisorplattform“ aktivieren und neu starten.',
        '• virtual schließen, während Maschinen laufen: virtual fragt nach und beendet die Gäste dann hart — vorher im Gast herunterfahren.',
        '• „Engine nicht gefunden“: Installation laut Einstellungen → Engines, oder Pfad von Hand setzen und „Erneut suchen“.',
        '• „UEFI-Firmware nicht gefunden“: Das QEMU-Paket enthält edk2-x86_64-code.fd bzw. edk2-aarch64-code.fd unter share/qemu. Bei Distro-Paketen zusätzlich ovmf/edk2-ovmf installieren.',
        '• „Core nicht gefunden“: In RetroArch den Core herunterladen; Cores-Ordner in den Einstellungen prüfen.',
        '• „Fehlende BIOS-Dateien“: Dateien mit exakt dem genannten Namen in den System-Ordner legen.',
        '• Windows 95/98 friert beim Start ein oder meldet Schutzfehler: Taktbremse auf 60–75 % und nur einen Kern.',
        '• Der Start schlägt sofort fehl: „Log“ zeigt die Ausgabe der Engine; „Übersicht“ die Kommandozeile zum Nachstellen im Terminal.',
        '• „CUE: Datendatei … fehlt“: Die BIN muss neben der CUE liegen; virtual sucht auch ohne Rücksicht auf Groß-/Kleinschreibung und versteht Windows-Pfade (Unterordner\\Datei.bin).',
        '• Update mit laufenden Maschinen: virtual fragt nach, lädt das Update, beendet die Maschinen und installiert dann — vorher im Gast herunterfahren.',
      ],
    },
  ],
};

const en: Content = {
  labels: {
    fab: 'Help & manual',
    tutorial: 'Tutorial',
    manual: 'Manual',
    search: 'Search the manual …',
    next: 'Next',
    back: 'Back',
    skip: 'Skip',
    done: 'Let’s go',
    stepOf: (n, total) => `Step ${n} of ${total}`,
    noResults: 'No results',
  },
  tutorial: [
    {
      title: 'Welcome to virtual.',
      body: [
        'virtual runs virtual machines — for today and for back then: current Windows, Linux and BSD guests at native speed, old systems from DOS to Windows 95 to Mac OS 9 on the hardware of their era, plus game consoles.',
        'virtual is an orchestrator: machines, profiles, media, snapshots and isolation are its job — execution is done by QEMU and RetroArch as separate processes. Both must be installed (Settings → Engines shows what was found).',
        'Everything stays on your computer: every machine is a folder with machine.json, disks and snapshots. No account, no cloud, no telemetry.',
        'This tutorial takes two minutes. You can always reopen it with the ? button at the bottom right.',
      ],
    },
    {
      title: 'Creating a machine',
      body: [
        '“+ Machine” in the sidebar opens the wizard. You don’t pick a device, you pick an operating system — virtual brings the matching machine profile.',
        '• A profile describes a complete computer: “PC 1996 — Pentium 133, Sound Blaster 16, S3” or “Mac 1994 — Quadra 800”. Chipset, clock, graphics, sound and drives are set.',
        '• Step 2: name, system disk size and optionally the install medium (ISO or floppy image). Some profiles need a firmware ROM — that is yours to bring.',
        '• Step 3 shows the summary. “Create” makes the folder and disk. Then: “Start”.',
      ],
    },
    {
      title: 'Two engines, one window',
      body: [
        'Current guests run virtualized: the processor executes them directly (HVF on macOS, WHPX on Windows, KVM on Linux) — with multiple cores, UEFI, TPM 2.0 and accelerated graphics.',
        'Old guests run emulated: CPU, chipset, ISA bus and sound card are reproduced, the clock throttled so Windows 95 doesn’t choke on its timer. Foreign architectures (68k, PowerPC, MIPS, RISC-V) run this way too.',
        '• You don’t have to choose: the profile decides the engine. “Overview” shows the exact command line virtual runs.',
        '• The display opens as the engine’s own window (QEMU or RetroArch); a display embedded in virtual is planned for a later version.',
      ],
    },
    {
      title: 'Old systems, safely',
      body: [
        'Systems that haven’t seen updates in decades don’t belong on a network. So virtual starts retro profiles with no network device, read-only folders and, if you like, in throwaway mode.',
        '• Network “isolated”: the guest reaches only the host — never the LAN. “NAT” lets it reach the internet, “off” is the safest state.',
        '• Throwaway mode: everything the guest writes is discarded on shutdown. Ideal for trying unknown software.',
        '• Snapshots: freeze the state before you install something — and be back in seconds if it went wrong.',
      ],
    },
    {
      title: 'Consoles',
      body: [
        'Game consoles and handhelds are machines like any other — the engine is RetroArch with libretro cores. The profile names the core (e.g. snes9x, mgba, swanstation) and the BIOS files the system needs.',
        '• Download cores once in RetroArch under “Online Updater → Core Downloader”; virtual finds them in the cores folder (Settings → Engines).',
        '• BIOS files go into the system folder. “Media” shows what is missing. Games, ROMs and BIOS files are yours to bring — virtual downloads none and links no sources.',
        '• Snapshots of a console are save states: only possible while the game is running. Saves live in the machine folder under saves/ and states/.',
      ],
    },
    {
      title: 'Media, folders, network',
      body: [
        '• “Media”: insert or remove CDs (ISO, NRG, BIN/CUE, MDF), floppies (IMG) and firmware ROMs — takes effect on the next start. Floppy A: and B: for those 13-disk installs.',
        '• Installing from CD: insert the image, “Start” — the machine boots from the medium. After the install set “Hardware” → “Boot from” to “Disk” or eject the CD, otherwise the install medium boots again. Windows 9x: insert the boot floppy plus the CD, boot from A: and run setup from the CD.',
        '• “Network”: mode, port forwards (e.g. host 2222 → guest 22 for SSH, on 127.0.0.1 only) and shared folders. Current guests mount folders via virtio-9p, old guests via the SMB share of the guest network (needs Samba on the host).',
        '• “Hardware”: cores, memory, clock throttle, firmware, graphics, audio, NIC, disk bus, floppy drive, input and display — editable while the machine is off.',
        '• “Log” shows the engine output — the first place to look when a start fails.',
      ],
    },
  ],
  sections: [
    {
      id: 'v1',
      title: 'Version 1.0',
      body: [
        'Version 1.0 is the first test release for macOS and Windows after a complete functional and security audit.',
        '• Current profiles (UEFI x86, ARM) start again: HD audio gets its codec.',
        '• Names and paths containing commas no longer break the command line; neither do long machine names.',
        '• Port forwards and VNC on 127.0.0.1 only; only https links and machine folders are opened, never files or programs.',
        '• The window no longer freezes during snapshots and start; Windows no longer opens console windows.',
        '• Closing with machines running asks first; whatever the host cannot do (TPM, OpenGL, folders) is noted in the log.',
        '• Second review round: messages from the engine control appear in the UI language, profiles have English texts.',
        '• Only one operation runs per machine at a time (start, snapshot, medium, delete) — a second one is refused with a note. During a long snapshot, pause/stop report “engine busy”; “Force stop” acts immediately.',
        '• Updating with machines running asks first and stops them before installing; update errors are shown.',
        '• Windows: CUE files with a BOM, Windows-1252 umlauts and \\ paths, profiles/JSON saved in Notepad (BOM), reserved names like “CON”, a bare drive “D:” and quoted paths are handled.',
      ],
    },
    {
      id: 'engines',
      title: 'Installing engines',
      body: [
        'virtual ships no engines and uses installed ones. Settings → Engines shows what was found and the install command.',
        '• macOS: brew install qemu · brew install --cask retroarch-metal · brew install swtpm (for TPM 2.0).',
        '• Windows: winget install SoftwareFreedomConservancy.QEMU · winget install Libretro.RetroArch. For acceleration (WHPX) enable “Windows Hypervisor Platform” in Windows Features. swtpm is not available on Windows — no TPM there; virtual looks for cores and the system folder next to retroarch.exe.',
        '• Linux: sudo apt install qemu-system retroarch swtpm (Debian/Ubuntu) or sudo dnf install qemu retroarch swtpm (Fedora).',
        '• If an engine lives somewhere unusual, enter its path in Settings. “Search again” checks immediately. virtual strips quotes (Explorer “Copy as path”); a bare “D:” becomes “D:\\”.',
        '• QEMU (GPLv2) and RetroArch (GPLv3) run as standalone processes and are not linked into virtual.',
      ],
    },
    {
      id: 'profiles',
      title: 'Machine profiles',
      body: [
        'A profile is a JSON file describing a complete computer: engine, architecture, machine type, CPU, memory, firmware, devices, network default and isolation.',
        '• Categories: Current (virtualized), Retro PC (486 to Pentium 4), Retro Mac (68k and PowerPC), other architectures (RISC-V, MIPS) and consoles.',
        '• Profiles marked “beta” are experimental — QEMU’s emulation of that machine is not yet mature (Quadra 800, Power Mac G3, Malta).',
        '• Profiles from the “nested” plan are usable during early access as well.',
        '• Own profiles: JSON files in the profiles folder of the config directory override the built-in ones by id (export/import in the app is planned for a later version). UTF-8 with or without BOM; English display texts optionally in an “en” block (name, family, versions, hint, notes).',
      ],
    },
    {
      id: 'virtualization',
      title: 'Virtualization (current guests)',
      body: [
        'Guest architecture = host architecture → virtual uses the host accelerator: HVF (macOS), WHPX (Windows), KVM (Linux). On Apple Silicon, ARM guests run natively, x86 guests emulated — and slowly.',
        '• UEFI firmware (OVMF/AAVMF) comes from the QEMU package; the NVRAM copy lives as nvram.fd in the machine folder.',
        '• “UEFI + Secure Boot” starts the Secure Boot firmware with SMM — on q35 with TCG or KVM. With HVF/WHPX the machine starts with plain UEFI (noted in the log). No keys are pre-enrolled: set up Secure Boot in the UEFI setup if needed.',
        '• TPM 2.0 needs swtpm — without it the machine runs, but without TPM (Windows 11 requires it during setup).',
        '• Graphics: virtio-vga or virtio-gpu; “OpenGL acceleration” uses virtio-vga-gl where the QEMU build supports OpenGL (Linux, Windows). Homebrew QEMU on macOS has no OpenGL — the display runs without GL there. Input via USB tablet so the mouse isn’t captured.',
        '• CPU model “host” exists only with an accelerator; without one virtual picks a suitable model automatically.',
      ],
    },
    {
      id: 'emulation',
      title: 'Emulation (old guests)',
      body: [
        'Retro profiles run without an accelerator (TCG) so that clock and timers of old kernels behave. The clock throttle (Hardware → Clock) approximates 60–75 % of a period machine.',
        '• PC profiles: i440FX/PIIX3 or a pure ISA machine, Cirrus VGA with VESA, Sound Blaster 16 + AdLib, NE2000 or RTL8139, floppy drives A: and B:.',
        '• Mac profiles: Quadra 800 (68040) needs the original ROM as “firmware ROM”; Power Mac G3 boots with OpenBIOS without a ROM.',
        '• Other architectures: RISC-V “virt” and MIPS “Malta” for Linux/BSD kernels.',
        '• Cycle-accurate chipset fidelity (86Box) is planned for a later version in the nested plan.',
      ],
    },
    {
      id: 'consoles',
      title: 'Consoles (RetroArch / libretro)',
      body: [
        'Console profiles start RetroArch with a libretro core: -L <core> <game> --appendconfig <machine.cfg>. Control goes through RetroArch’s network command interface (UDP, localhost only).',
        '• Cores: RetroArch → Online Updater → Core Downloader. The cores folder is set in Settings; on macOS also RetroArch.app/Contents/Resources/cores.',
        '• BIOS: PlayStation scph5500/5501/5502.bin, Saturn sega_101.bin + mpr-17933.bin, Dreamcast dc/dc_boot.bin + dc/dc_flash.bin, Nintendo DS bios7.bin + bios9.bin + firmware.bin, Mega-CD bios_CD_*.bin, PC Engine CD syscard3.pce, Neo Geo neogeo.zip. All into the system folder.',
        '• Save states: “Create snapshot” stores a state in the next free slot; “Restore” loads it. Only while the game is running.',
        '• Battery saves live in saves/, screenshots in screenshots/ — all inside the machine folder.',
        '• Pause/Resume, Reset and Stop work as for PC machines. Netplay and achievements are disabled.',
      ],
    },
    {
      id: 'isolation',
      title: 'Isolation & security',
      body: [
        '• Network “off”: no network device. Default for retro profiles and consoles.',
        '• “isolated”: SLIRP with restrict=on — the guest sees only forwards and shares, never the LAN.',
        '• “NAT”: the guest reaches the internet, nothing gets in from outside (except forwards). “Bridged” arrives in a later version and behaves like NAT until then.',
        '• Port forwards listen on 127.0.0.1 only — other computers on the LAN cannot reach the guest through them. VNC likewise (127.0.0.1:5900, further machines from 5901).',
        '• Shared folders are read-only for retro profiles (Hardware → Isolation).',
        '• Throwaway mode starts QEMU with -snapshot: writes go to a temporary file and are discarded on shutdown.',
        '• Engines run without root. QMP control uses a Unix socket in the user context (Windows: TCP on 127.0.0.1 only). USB passthrough is planned for a later version.',
        '• From the interface, virtual opens only https links and folders inside the machines folder — never files or programs (no .app bundles either).',
      ],
    },
    {
      id: 'snapshots',
      title: 'Snapshots',
      body: [
        '• PC and Mac machines: internal qcow2 snapshots. While running, memory state is included (savevm via QMP); while stopped, disk only (qemu-img snapshot).',
        '• In throwaway mode virtual refuses snapshots while running — they would be discarded on shutdown too. With the machine stopped it works.',
        '• Restoring replaces the current state — hence the confirmation.',
        '• Consoles: save states, only in a running game.',
        '• Snapshot trees with branches and linked clones are planned for a later version (nested).',
        '• The list lives as snapshots.json in the machine folder.',
        '• A snapshot with a lot of guest RAM takes a while. While it runs, virtual refuses start, delete and a second snapshot of the same machine; pause and stop report “engine busy”, “Force stop” ends it immediately (the snapshot is then lost).',
      ],
    },
    {
      id: 'media',
      title: 'Media & folders',
      body: [
        '• CDs: ISO directly; NRG (Nero), BIN/CUE, MDF/MDS (Alcohol), CCD/IMG (CloneCD) and raw 2352-byte images are converted to an ISO in the machine folder on insert (first data track only, no audio CDs). Floppies: IMG, IMA, DSK, VFD (read-only). Firmware ROM: any file (Quadra 800: 1 MB ROM).',
        '• “Boot from” (Hardware): automatic = CD first when inserted; or fix it to disk, CD/DVD or floppy. Esc at the BIOS splash opens the boot menu.',
        '• Media take effect on the next start; hot-swap is planned for a later version. Two images with the same name from different folders are converted to <name>.iso and <name>-2.iso.',
        '• Shared folders: virtio-9p (mount -t 9p -o trans=virtio <name> /mnt) for guests on the virtio bus; old guests via the SMB share on the guest network (\\\\10.0.2.4\\qemu, first folder only). SMB needs Samba on the host (macOS: brew install samba, Linux: apt install samba). QEMU for Windows supports neither 9p nor SMB — whatever the host cannot do is noted in the log at start.',
        '• “Open folder” in Overview shows the machine folder: machine.json, disk-0.qcow2, nvram.fd, snapshots.json, log.txt.',
      ],
    },
    {
      id: 'nested',
      title: 'The nested plan',
      body: [
        'Free stays free — for good: virtualization, emulation with standard profiles, linear snapshots, throwaway mode, folders, network, every format.',
        '• nested (€12 a year): full hardware library with individual devices and a free clock dial, snapshot trees and clones, guest programs as their own windows, headless operation, CLI with JSON output, nested virtualization, free engine arguments, sharing your own profiles.',
        '• No license check during early access: Settings → General → “Enable the nested plan”.',
      ],
    },
    {
      id: 'shortcuts',
      title: 'Keyboard shortcuts',
      body: ['• n — new machine', '• ? — manual', `• ${shortcut('en', ',')} — settings`, '• Esc — close dialog'],
    },
    {
      id: 'trouble',
      title: 'Troubleshooting',
      body: [
        '• The macOS builds are signed and notarized. Only self-built, unsigned builds make macOS say “damaged” — then once: xattr -dr com.apple.quarantine /Applications/virtual.app',
        '• Windows: “WHPX: No accelerator found” in the log → enable “Windows Hypervisor Platform” in Windows Features and reboot.',
        '• Closing virtual while machines run: virtual asks first and then force-stops the guests — shut them down inside the guest beforehand.',
        '• “Engine not found”: install as shown under Settings → Engines, or set the path manually and “Search again”.',
        '• “UEFI firmware not found”: the QEMU package contains edk2-x86_64-code.fd or edk2-aarch64-code.fd under share/qemu. With distro packages also install ovmf/edk2-ovmf.',
        '• “Core not found”: download the core in RetroArch; check the cores folder in Settings.',
        '• “Missing BIOS files”: put files with exactly the listed names into the system folder.',
        '• Windows 95/98 freezes at boot or reports protection errors: clock throttle 60–75 % and a single core.',
        '• Start fails immediately: “Log” shows the engine output; “Overview” the command line to reproduce in a terminal.',
        '• “CUE: data file … is missing”: the BIN must sit next to the CUE; virtual also matches regardless of upper/lower case and understands Windows paths (subfolder\\file.bin).',
        '• Updating with machines running: virtual asks, downloads the update, stops the machines and then installs — shut the guests down beforehand.',
      ],
    },
  ],
};

export function Help({ lang, openSignal = 0 }: { lang: Lang; openSignal?: number }) {
  // Sprache der Hilfe: folgt der App-Sprache, lässt sich aber im Kopf der
  // Hilfe jederzeit zwischen DE und EN umschalten
  const [helpLang, setHelpLang] = useState<'de' | 'en'>(lang === 'de' ? 'de' : 'en');
  useEffect(() => {
    setHelpLang(lang === 'de' ? 'de' : 'en');
  }, [lang]);
  const c = helpLang === 'de' ? de : en;
  const [mode, setMode] = useState<'closed' | 'tutorial' | 'manual'>(() => {
    try {
      return localStorage.getItem(SEEN_KEY) ? 'closed' : 'tutorial';
    } catch {
      return 'closed';
    }
  });
  const [step, setStep] = useState(0);
  const [sel, setSel] = useState(c.sections[0].id);
  const [q, setQ] = useState('');

  useEffect(() => {
    if (openSignal > 0) setMode('manual');
  }, [openSignal]);

  const close = () => {
    try {
      localStorage.setItem(SEEN_KEY, '1');
    } catch {
      /* Speicher nicht verfügbar */
    }
    setMode('closed');
    setStep(0);
  };

  useEffect(() => {
    if (mode === 'closed') return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') close();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [mode]);

  const query = q.trim().toLowerCase();
  const filtered = query
    ? c.sections.filter((s) => s.title.toLowerCase().includes(query) || s.body.some((p) => p.toLowerCase().includes(query)))
    : c.sections;
  const current = filtered.find((s) => s.id === sel) ?? filtered[0] ?? null;

  const para = (p: string, i: number) =>
    p.startsWith('• ') ? (
      <div key={i} className="hlp-li">
        {p.slice(2)}
      </div>
    ) : (
      <p key={i}>{p}</p>
    );

  return (
    <>
      <button className="hlp-fab" title={c.labels.fab} onClick={() => setMode('manual')}>
        ?
      </button>
      {mode !== 'closed' && (
        <div className="hlp-overlay" onClick={close}>
          <div className="hlp-modal" onClick={(e) => e.stopPropagation()}>
            <div className="hlp-head">
              <span className="hlp-brand">
                <span className="hlp-name">virtual</span>
                <span className="hlp-dot">.</span>
              </span>
              <button
                className={`hlp-tab ${mode === 'tutorial' ? 'active' : ''}`}
                onClick={() => {
                  setMode('tutorial');
                  setStep(0);
                }}
              >
                {c.labels.tutorial}
              </button>
              <button className={`hlp-tab ${mode === 'manual' ? 'active' : ''}`} onClick={() => setMode('manual')}>
                {c.labels.manual}
              </button>
              <span className="hlp-spacer" />
              <span className="hlp-lang" role="group" aria-label="Sprache / Language">
                <button className={`hlp-lang-btn ${helpLang === 'de' ? 'active' : ''}`} onClick={() => setHelpLang('de')}>
                  DE
                </button>
                <button className={`hlp-lang-btn ${helpLang === 'en' ? 'active' : ''}`} onClick={() => setHelpLang('en')}>
                  EN
                </button>
              </span>
              <button className="hlp-close" onClick={close}>
                ✕
              </button>
            </div>

            {mode === 'tutorial' && (
              <div className="hlp-tut">
                <div className="hlp-step-count">{c.labels.stepOf(step + 1, c.tutorial.length)}</div>
                <h2>{c.tutorial[step].title}</h2>
                {c.tutorial[step].body.map(para)}
                <div className="hlp-tut-nav">
                  <button className="hlp-ghost" onClick={close}>
                    {c.labels.skip}
                  </button>
                  <span className="hlp-dots">
                    {c.tutorial.map((_, i) => (
                      <span key={i} className={i === step ? 'on' : ''} />
                    ))}
                  </span>
                  {step > 0 && <button onClick={() => setStep(step - 1)}>{c.labels.back}</button>}
                  {step < c.tutorial.length - 1 ? (
                    <button className="hlp-primary" onClick={() => setStep(step + 1)}>
                      {c.labels.next}
                    </button>
                  ) : (
                    <button className="hlp-primary" onClick={close}>
                      {c.labels.done}
                    </button>
                  )}
                </div>
              </div>
            )}

            {mode === 'manual' && (
              <div className="hlp-body">
                <div className="hlp-toc">
                  <input type="text" placeholder={c.labels.search} value={q} onChange={(e) => setQ(e.target.value)} />
                  {filtered.length === 0 && <div className="hlp-empty">{c.labels.noResults}</div>}
                  {filtered.map((s) => (
                    <button key={s.id} className={`hlp-toc-item ${current?.id === s.id ? 'active' : ''}`} onClick={() => setSel(s.id)}>
                      {s.title}
                    </button>
                  ))}
                </div>
                <div className="hlp-content">
                  {current && (
                    <>
                      <h2>{current.title}</h2>
                      {current.body.map(para)}
                    </>
                  )}
                </div>
              </div>
            )}
          </div>
        </div>
      )}
    </>
  );
}
