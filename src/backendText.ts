// Meldungen des Rust-Backends sind deutsch (Fehler, Log-Hinweise, Installationshinweise).
// Bei englischer Oberfläche werden sie hier übersetzt — bekannte Muster, eingebettete
// Teile (z. B. die Ursache hinter „…: “) werden rekursiv mitübersetzt. Unbekanntes
// (Ausgabe von QEMU/RetroArch, Betriebssystem-Fehlertexte) bleibt unverändert.
// tests/backendText.test.ts prüft, dass jede Meldung aus src-tauri/ und core/ hier ein Muster hat.

import type { Lang } from './i18n';

type Rule = [RegExp, string];

// Reihenfolge: spezifisch vor allgemein. $1, $2 … werden selbst wieder übersetzt.
const RULES: Rule[] = [
  // Maschinen & Ordner
  [/^Maschine nicht gefunden$/, 'Machine not found'],
  [/^machine\.json nicht lesbar$/, 'machine.json is not readable'],
  [/^Name fehlt$/, 'Name is missing'],
  [/^Profil nicht gefunden$/, 'Profile not found'],
  [/^Maschine läuft — erst stoppen, dann ändern$/, 'The machine is running — stop it before making changes'],
  [/^Maschine läuft — erst stoppen, dann löschen$/, 'The machine is running — stop it before deleting it'],
  [/^Maschine läuft bereits$/, 'The machine is already running'],
  [/^Maschine startet bereits$/, 'The machine is already starting'],
  [/^Maschine läuft nicht$/, 'The machine is not running'],
  [/^Maschine wird gerade gelöscht$/, 'The machine is being deleted'],
  [/^Maschine wird gerade gespeichert$/, 'The machine is being saved'],
  [/^Für diese Maschine läuft gerade ein Snapshot — bitte warten$/, 'A snapshot is in progress for this machine — please wait'],
  [/^Für diese Maschine wird gerade ein Medium vorbereitet — bitte warten$/, 'A medium is being prepared for this machine — please wait'],
  [/^Medienwechsel im laufenden Betrieb folgt in einer späteren Version — bitte erst stoppen$/, 'Changing media while running comes in a later version — please stop the machine first'],
  [/^Nur Links \(https\) und Maschinenordner lassen sich öffnen$/, 'Only links (https) and machine folders can be opened'],
  [/^Maschinenordner anlegen fehlgeschlagen: (.*)$/s, 'Could not create the machines folder: $1'],
  [/^Ordner anlegen fehlgeschlagen: (.*)$/s, 'Could not create the folder: $1'],
  [/^Ordner löschen fehlgeschlagen: (.*)$/s, 'Could not delete the folder: $1'],
  [/^Löschen fehlgeschlagen: (.*)$/s, 'Delete failed: $1'],
  [/^Schreiben fehlgeschlagen: (.*)$/s, 'Write failed: $1'],
  [/^Umbenennen fehlgeschlagen: (.*)$/s, 'Rename failed: $1'],
  [/^Platte anlegen fehlgeschlagen: (.*)$/s, 'Creating the disk failed: $1'],
  [/^NVRAM anlegen: (.*)$/s, 'Creating NVRAM: $1'],

  // Engines & Erkennung
  [/^(\S+) nicht gefunden — (.*)$/s, '$1 not found — $2'],
  [/^(\S+) nicht gefunden$/, '$1 not found'],
  [/^swtpm ist unter Windows nicht verfügbar — TPM entfällt$/, 'swtpm is not available on Windows — no TPM'],
  [/^Maschine ohne QEMU-Architektur$/, 'Machine without a QEMU architecture'],
  [/^Keine QEMU-Maschine$/, 'Not a QEMU machine'],
  [/^Keine Konsolen-Maschine$/, 'Not a console machine'],
  [/^Konsolen-Maschine ohne libretro-Konfiguration$/, 'Console machine without libretro configuration'],
  [/^QEMU starten fehlgeschlagen: (.*)$/s, 'Starting QEMU failed: $1'],
  [/^RetroArch starten fehlgeschlagen: (.*)$/s, 'Starting RetroArch failed: $1'],
  [/^Engine beendet mit (.*?): (.*)$/s, 'Engine exited with $1: $2'],
  [/^UEFI-Firmware \(OVMF\/AAVMF\) nicht gefunden — im QEMU-Paket enthalten\?$/, 'UEFI firmware (OVMF/AAVMF) not found — is it part of the QEMU package?'],
  [/^Core „(.*?)“ nicht gefunden in (.*?) — in RetroArch unter „Online-Updater → Core herunterladen“ laden oder den Cores-Ordner in den Einstellungen anpassen\.$/s,
    'Core “$1” not found in $2 — download it in RetroArch under “Online Updater → Core Downloader” or adjust the cores folder in Settings.'],
  [/^BIOS-Dateien fehlen im System-Ordner (.*?): (.*)$/s, 'BIOS files missing in the system folder $1: $2'],
  [/^Kein Spiel eingelegt — ROM oder Disc-Abbild unter „Medien“ wählen\.$/, 'No game inserted — choose a ROM or disc image under “Media”.'],
  [/^retroarch\.cfg schreiben: (.*)$/s, 'Writing retroarch.cfg: $1'],
  [/^RetroArch-Kommando: (.*)$/s, 'RetroArch command: $1'],

  // QMP
  [/^Engine beschäftigt \(Snapshot läuft\) — bitte warten$/, 'Engine busy (snapshot in progress) — please wait'],
  [/^QMP-Steuerung \(noch\) nicht verbunden$/, 'QMP control not connected (yet)'],
  [/^Keine QMP-Steuerung: (.*)$/s, 'No QMP control: $1'],
  [/^QMP-Verbindung fehlgeschlagen: (.*)$/s, 'QMP connection failed: $1'],
  [/^QMP-Verbindung geschlossen$/, 'QMP connection closed'],
  [/^QMP-Begrüßung: (.*)$/s, 'QMP greeting: $1'],
  [/^QMP-Kommando: (.*)$/s, 'QMP command: $1'],
  [/^QMP senden: (.*)$/s, 'QMP send: $1'],
  [/^QMP lesen: (.*)$/s, 'QMP read: $1'],
  [/^Unix-Socket nicht verfügbar: (.*)$/s, 'Unix socket not available: $1'],

  // Snapshots
  [/^Snapshot nicht gefunden$/, 'Snapshot not found'],
  [/^Maschine hat keine Platte — Snapshots brauchen eine qcow2-Platte$/, 'The machine has no disk — snapshots need a qcow2 disk'],
  [/^Snapshots brauchen qcow2, die Platte ist (.*)$/, 'Snapshots need qcow2, the disk is $1'],
  [/^Wegwerf-Modus: Änderungen und Snapshots im laufenden Betrieb verfallen beim Beenden — Snapshot bitte bei gestoppter Maschine anlegen oder den Wegwerf-Modus abschalten\.$/,
    'Throwaway mode: changes and snapshots taken while running are discarded on exit — take the snapshot with the machine stopped or turn throwaway mode off.'],
  [/^Konsole muss laufen, um einen Spielstand zu sichern$/, 'The console must be running to save a state'],
  [/^Konsole muss laufen, um einen Spielstand zu laden$/, 'The console must be running to load a state'],
  [/^Save State wurde nicht geschrieben \((.*)\) — läuft ein Spiel\?$/s, 'The save state was not written ($1) — is a game running?'],
  [/^Ungültiger Slot$/, 'Invalid slot'],

  // CD-Abbilder
  [/^Abbild nicht lesbar: (.*)$/s, 'Image not readable: $1'],
  [/^Begleitdatei (.*?) fehlt neben (.*)$/s, 'Companion file $1 is missing next to $2'],
  [/^erste Spur ist eine Audiospur — kein Datenabbild$/, 'the first track is an audio track — not a data image'],
  [/^CUE: Spurmodus „(.*)“ nicht unterstützt$/, 'CUE: track mode “$1” not supported'],
  [/^CUE: keine FILE-Zeile$/, 'CUE: no FILE line'],
  [/^CUE: Datendatei (.*) fehlt$/s, 'CUE: data file $1 is missing'],
  [/^CUE: keine TRACK-Zeile$/, 'CUE: no TRACK line'],
  [/^NRG: Datei zu kurz$/, 'NRG: file too short'],
  [/^NRG: Nero-Kennung \(NERO\/NER5\) fehlt$/, 'NRG: Nero signature (NERO/NER5) missing'],
  [/^NRG: Chunk-Liste liegt außerhalb der Datei$/, 'NRG: chunk list lies outside the file'],
  [/^NRG: beschädigte Chunk-Liste$/, 'NRG: damaged chunk list'],
  [/^NRG: keine Spurinfo \(DAO\/ETN\) und Sektorformat nicht erkennbar$/, 'NRG: no track info (DAO/ETN) and sector format not recognizable'],
  [/^NRG: Spurgrenzen unplausibel$/, 'NRG: implausible track boundaries'],
  [/^NRG: rohe Sektoren ohne Sync-Muster$/, 'NRG: raw sectors without sync pattern'],
  [/^Abbild ist bereits ein ISO — keine Wandlung nötig$/, 'The image already is an ISO — no conversion needed'],
  [/^Abbild enthält keine vollständigen Sektoren$/, 'The image contains no complete sectors'],
  [/^(.*): Sektorformat nicht erkannt — kein ISO 9660 und keine rohen 2352\/2336-Byte-Sektoren$/s,
    '$1: sector format not recognized — neither ISO 9660 nor raw 2352/2336-byte sectors'],

  // Updates
  [/^Update-Prüfung fehlgeschlagen: (.*)$/s, 'Update check failed: $1'],
  [/^Update fehlgeschlagen: (.*)$/s, 'Update failed: $1'],
  [/^Kein Update verfügbar$/, 'No update available'],

  // Hinweise beim Start (Log, Kommandozeilen-Vorschau)
  [/^TPM 2\.0: swtpm nicht gefunden — die Maschine startet ohne TPM\.$/, 'TPM 2.0: swtpm not found — the machine starts without a TPM.'],
  [/^Secure Boot: mit diesem Beschleuniger bzw\. Board nicht verfügbar — Start mit UEFI ohne Secure Boot\.$/,
    'Secure Boot: not available with this accelerator or board — starting with UEFI without Secure Boot.'],
  [/^OpenGL: von diesem QEMU-Build nicht unterstützt — Anzeige ohne GL\.$/, 'OpenGL: not supported by this QEMU build — display without GL.'],
  [/^Gemeinsame Ordner: virtio-9p gibt es in QEMU für Windows nicht\.$/, 'Shared folders: QEMU for Windows has no virtio-9p.'],
  [/^Gemeinsame Ordner: SMB-Freigabe übersprungen — Samba \(smbd\) fehlt bzw\. Windows-Host\.$/,
    'Shared folders: SMB share skipped — Samba (smbd) is missing or the host is Windows.'],
  [/^Gemeinsame Ordner sind für diese Maschine nicht eingehängt \(virtio-9p braucht den virtio-Bus, SMB ein Netz\)\.$/,
    'Shared folders are not mounted for this machine (virtio-9p needs the virtio bus, SMB a network).'],
  [/^Gemeinsame Ordner: SMB-Freigaben sind immer beschreibbar — der Ordner ist schreibgeschützt eingestellt und wird nicht freigegeben \(zum Freigeben unter Hardware „Gemeinsame Ordner schreibgeschützt“ und am Ordner „nur lesen“ abschalten\)\.$/,
    'Shared folders: SMB shares are always writable — the folder is set to read-only and is not shared (to share it, turn off “Shared folders read-only” under Hardware and “read-only” at the folder).'],
  [/^SMB-Freigabe: nur der erste Ordner \(SLIRP kann eine Freigabe\)\.$/, 'SMB share: only the first folder (SLIRP supports one share).'],

  // Präfix „qemu-img: …“ zuletzt (die Ursache selbst ist Engine-Text)
  [/^qemu-img: (.*)$/s, 'qemu-img: $1'],
];

/** Backend-Meldung in die Sprache der Oberfläche bringen. */
export function trBackend(msg: unknown, lang: Lang): string {
  const text = typeof msg === 'string' ? msg : msg instanceof Error ? msg.message : String(msg);
  if (lang === 'de') return text;
  return translate(text, 0);
}

function translate(text: string, depth: number): string {
  if (depth > 4) return text;
  for (const [re, tpl] of RULES) {
    const m = re.exec(text);
    if (!m) continue;
    return tpl.replace(/\$(\d)/g, (_, i: string) => translate(m[Number(i)] ?? '', depth + 1));
  }
  return text;
}

/** Log-/Vorschauzeile: Hinweiszeilen „# …“ von virtual übersetzen, Engine-Ausgabe nicht. */
export function trLogLine(line: string, lang: Lang): string {
  if (lang === 'de' || !line.startsWith('# ')) return line;
  return `# ${translate(line.slice(2), 0)}`;
}

/** Mehrzeiliger Text (Kommandozeilen-Vorschau): jede Hinweiszeile einzeln. */
export function trLines(text: string, lang: Lang): string {
  if (lang === 'de') return text;
  return text
    .split('\n')
    .map((l) => trLogLine(l, lang))
    .join('\n');
}
