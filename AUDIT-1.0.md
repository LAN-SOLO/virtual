# virtual. — Audit 1.0

Stand: 2026-10-19 · Basis `de77524` · Ziel: Testfassung 1.0.0 für macOS und Windows.

„Getestet“ heißt hier: tatsächlich ausgeführt (Unit-/Integrationstest, Prüfstand gegen echtes
QEMU/RetroArch auf macOS 26 / Apple Silicon, oder Oberfläche im Browser mit gemocktem Backend).
„Statisch“ heißt: nur Code-Review — vor allem alles, was Windows oder ein natives Fenster braucht.

## Testlauf

| Was | Befehl | Ergebnis |
| --- | --- | --- |
| Rust-Kern + App | `cargo test --workspace` | 39 (core) + 7 (App) Tests grün, 1 ignored = Live-Test |
| QMP + Snapshots live | `cargo test -p virtual -- --ignored qemu_live` | grün (QEMU 11.1.1, i386) |
| Alle QEMU-Profile live | `cargo run -p virtual-core --example qemucheck` | 49/49 Fälle starten, QMP antwortet |
| Firmware bootet wirklich | `QEMUCHECK_RUN=8 … qemucheck plain` / `uefi-secure` | 10/10 + Secure Boot „running“ |
| RetroArch live | `cargo run -p virtual-core --example retrocheck …` | Slot 2/0, Save/Load, Pause, Reset, Quit grün (RetroArch 1.22.2, snes9x) |
| Frontend | `pnpm build` (tsc + vite) | grün |
| Oberfläche | headless Chrome + `mockIPC` (nicht eingecheckt) | 40 Prüfungen grün, keine Konsolenfehler |
| Abhängigkeiten | `cargo audit`, `pnpm audit` | siehe Sicherheit |

## Funktionsinventar

| Bereich | Funktion | Status |
| --- | --- | --- |
| Assistent | Kategorie/Suche, Profil, Name, Plattengröße, Medium/Spiel, Firmware-ROM, Zusammenfassung, Anlegen | UI getestet (Mock); Anlegen + qemu-img statisch/Live-Test |
| Profile | 27 eingebettet, eigene JSON überlagern per id | Unit-Tests |
| QEMU-Profile | 11 Profile × Varianten (Medien, Bootgerät auto/cdrom/disk/floppy, Netz isoliert + Weiterleitung + Ordner + Wegwerf + VNC, Secure Boot) | Live gegen QEMU 11 (Quadra 800 nur Argv-Test: braucht Original-ROM) |
| Steuerung | Start, Pause, Fortsetzen, Reset, Stopp (ACPI → quit → kill), Stopp erzwingen | Live-Test (QMP) + UI-Mock |
| Snapshots QEMU | anlegen/wiederherstellen/löschen, laufend (savevm/loadvm/delvm) und gestoppt (qemu-img) | Live-Test |
| Konsolen | RetroArch-Start, Core/Fallback, BIOS-Prüfung, Save States als Snapshots, Pause/Reset/Quit | Unit-Tests + retrocheck live |
| Medien | CD (ISO; NRG/BIN-CUE/MDF-MDS/CCD/raw → ISO), Diskette A/B, Firmware-ROM, Entfernen | Unit-Tests (synthetische Abbilder) + UI-Mock |
| Hardware | CPU, Kerne, RAM, Takt, Firmware, TPM, Grafik, Audio, NIC, Bus, Eingabe, Bootgerät, Diskette, Anzeige, GL, Vollbild, Isolation, Zusatzargumente (nested) | Argv-Tests + UI-Mock |
| Netzwerk | aus / isoliert / NAT / bridged(=NAT), Weiterleitungen, gemeinsame Ordner (9p, SMB) | Argv-Tests + Live (9p, Weiterleitung) |
| Übersicht | Fakten, Notizen, Ordner öffnen, Kommandozeilen-Vorschau inkl. Host-Hinweise | UI-Mock; Vorschau statisch |
| Log | Live-Ausgabe, log.txt nach Neustart, Leeren | statisch |
| Einstellungen | Sprache, Theme hell/dunkel, Akzent, Nachfragen, nested, Engines + Pfade + Erneut suchen, Maschinenordner, Updates, Links | UI-Mock (Sprache, Hell-Modus, Speichern) |
| App | Tutorial, Handbuch DE/EN, Kürzel n ? ⌘/Strg+, Esc, Schließen-Nachfrage, Updater | UI-Mock; Updater statisch |

## Behobene Fehler

1. **Aktuelle Profile starteten nicht** (`intel-hda,audiodev=` gibt es nicht) → Controller + `hda-duplex`.
2. Komma in Maschinenname oder Pfad (`Win 3.11, DOS`) brach den QEMU-Start → Werte maskiert (`,,`).
3. OpenGL-Option auf macOS (`cocoa,gl=on`, `virtio-vga-gl`) ließ QEMU abbrechen → nur wo der Build GL kann.
4. Gemeinsamer Ordner + Netz: ohne Samba (macOS/Linux) bzw. unter Windows brach QEMU ab; 9p unter Windows ebenso → nur verfügbare Wege, Hinweise im Log.
5. Lange Maschinennamen: QMP-Socket > 104 Byte → Socket im Benutzer-Temp-Ordner, Ordnernamen auf 40 Zeichen begrenzt.
6. Synchrone Tauri-Kommandos (Snapshot, Start, Stopp, Engine-Erkennung) liefen im UI-Thread und froren das Fenster ein; der Runtime-Lock wurde über QMP-Aufrufe gehalten.
7. Windows: jeder Kindprozess öffnete ein Konsolenfenster → `CREATE_NO_WINDOW`.
8. Windows: UEFI-Firmware im QEMU-Installer-Layout (`<qemu>\share`) wurde nicht gefunden.
9. Windows: RetroArch-Cores/System nur unter `C:\RetroArch-Win64` gesucht → neben `retroarch.exe`.
10. Zwei gleichnamige CD-Abbilder aus verschiedenen Ordnern: das zweite nutzte die ISO des ersten.
11. RetroArch-Slot-Zählung lief nach Neustart auseinander (Slot aus der globalen RetroArch-Config) → `state_slot = "0"`.
12. Save State: feste 400 ms Wartezeit → Warten auf geänderte Datei (bis 5 s).
13. Audio-/Zusatzargument-Felder verschluckten Komma bzw. Zeilenumbruch beim Tippen.
14. Ungespeicherte Hardware-Änderungen gingen beim Einlegen eines Mediums verloren; ein alter Entwurf konnte Medien/Slot zurückdrehen.
15. „UEFI + Secure Boot“ nutzte gar keine Secure-Boot-Firmware → secure-code + SMM (q35, TCG/KVM).
16. Snapshots im Wegwerf-Modus während des Laufs „gelangen“, verfielen aber → abgelehnt mit Erklärung.
17. App schließen beendete laufende Gäste kommentarlos hart → Nachfrage.
18. Status-Änderungen aus dem Engine-Fenster (QEMU-Menü „Pause“) kamen nie in der Oberfläche an → Polling.
19. WHPX ohne `kernel-irqchip=off`; VNC-Port-Konflikt bei mehreren Maschinen (`to=99`).
20. React-Warnung: setState der App während SettingsModal rendert.

## Sicherheit

| Schwere | Befund | Status |
| --- | --- | --- |
| Mittel | Port-Weiterleitungen banden auf alle Interfaces (`hostfwd=tcp::P`) — ungepatchte Alt-Systeme waren aus dem LAN erreichbar | behoben: `127.0.0.1` |
| Mittel | `open_path` öffnete beliebige Pfade (öffnen = ausführen bei .app/.exe/.command) | behoben: nur https und Ordner im Maschinenordner (siehe Verifikation: `.app`-Bundles sind Ordner) |
| Mittel | Option-Injektion über Kommas in Namen/Pfaden in QEMU-Optionen | behoben: Maskierung |
| Mittel | rustls 0.23.44 (RUSTSEC-2026-0285/0346–0349) | behoben: 0.23.46 |
| Hoch (dev) | source-map-js < 1.2.2 (nur Build-Werkzeug) | behoben: 1.2.2 |
| Niedrig | Capabilities `dialog:default` + `opener:default` | reduziert auf `core:default` + `dialog:allow-open` |
| Niedrig | Sockets im geteilten `/tmp` vermieden (Benutzer-Temp bzw. `$XDG_RUNTIME_DIR`) | umgesetzt |
| Offen (niedrig) | `zip` 4.6.1 im Updater (RUSTSEC-2026-0367/0368/0369, Fix erst ab 9.x). Der Updater entpackt nur minisign-signierte Pakete — ausnutzbar nur mit dem privaten Signaturschlüssel | offen, Upgrade erst mit tauri-plugin-updater, das zip ≥ 9 nutzt |
| Offen (niedrig) | Windows: QMP über TCP 127.0.0.1 ohne Authentifizierung — lokale Prozesse desselben Rechners könnten die VM steuern | offen (Rust-std ohne AF_UNIX unter Windows) |
| Info | unmaintained-Warnungen (gtk-rs/unic/proc-macro-error) nur im Linux-Zweig von Tauri | keine Aktion |

Weiter geprüft, ohne Befund: CSP (`default-src 'self'`), kein `dangerouslySetInnerHTML`, Updater
(Pubkey gesetzt, Endpoint https), keine Shell-Aufrufe (nur `Command` mit Argumentliste),
Abbild-Parser (Chunk-Größen begrenzt, Spurgrenzen geprüft, atomares Schreiben), JSON atomar
geschrieben, `remove_dir_all` folgt keinen Symlinks, localStorage nur für „Tutorial gesehen“.
Zusatzargumente (nested) gehen bewusst ungefiltert an QEMU — sie stehen in der eigenen machine.json.

## Bekannte Einschränkungen

- Engines werden nicht mitgeliefert (QEMU, RetroArch, optional swtpm/Samba).
- Fehlermeldungen des Backends sind deutsch; die englische Oberfläche übersetzt sie (`src/backendText.ts`, seit Runde 2). Betriebssystem-Fehlertexte (`os error …`) kommen in der Sprache des Systems.
- Tarif „nested“ ohne Lizenzprüfung (Vorabzugang); Bridged = NAT; kein Medienwechsel im Betrieb; Anzeige im Engine-Fenster.
- Secure Boot: Schlüssel nicht vorinstalliert; mit HVF/WHPX normales UEFI.
- Windows: kein TPM (kein swtpm), keine gemeinsamen Ordner (QEMU ohne 9p/SMB), QMP über localhost-TCP.
- RetroArch `GET_STATUS` stürzt in 1.22.2 ab — Pausenzustand wird lokal geführt (Pause im RetroArch-Menü wird nicht erkannt).
- Homebrew-Cask `retroarch` (x86_64) hängt unter Rosetta nach QUIT → App beendet nach 5 s hart; `retroarch-metal` empfohlen.

## Manuelle Testliste

### macOS (Apple Silicon und Intel)

- [ ] DMG öffnen, App startet ohne Gatekeeper-Warnung; Tutorial erscheint einmal.
- [ ] Einstellungen → Engines: qemu, qemu-img, retroarch gefunden, Beschleuniger hvf.
- [ ] „PC — aktuell (UEFI)“ mit Linux-ISO anlegen, starten, Installer erscheint im QEMU-Fenster; Ton hörbar.
- [ ] „ARM — aktuell (UEFI)“ (Apple Silicon) mit ARM-ISO: bootet beschleunigt.
- [ ] „PC 1996“: Boot-Diskette + CD, Booten von Diskette, dann „Platte“ — Startreihenfolge stimmt.
- [ ] NRG/BIN-CUE einlegen → ISO im Maschinenordner; gleichnamiges zweites Abbild → `-2.iso`.
- [ ] Pause im QEMU-Menü → Oberfläche zeigt nach ≤ 4 s „pausiert“.
- [ ] Snapshot laufend + gestoppt, wiederherstellen, löschen; im Wegwerf-Modus laufend abgelehnt.
- [ ] Weiterleitung 2222→22: `nc -z 127.0.0.1 2222` geht, von einem anderen Rechner nicht.
- [ ] Gemeinsamer Ordner ohne Samba: Maschine startet trotzdem, Hinweis im Log.
- [ ] Maschine mit sehr langem Namen und mit Komma im Namen starten.
- [ ] Snapshot einer laufenden Maschine mit 4–8 GB RAM (dauert > 20 s): meldet Erfolg, Liste stimmt, danach Pause/Fortsetzen reagieren normal.
- [ ] Start-Knopf doppelt klicken: nur ein QEMU-Prozess, zweiter Klick meldet „startet bereits“.
- [ ] SNES-Maschine: Spiel startet, Snapshot (Save State), Wiederherstellen, Stopp.
- [ ] virtual schließen (rotes X und ⌘Q) mit laufender Maschine → Nachfrage; Abbrechen lässt alles laufen.
- [ ] Hell-Modus, Akzentfarben, Englisch, Handbuch DE/EN.

### Windows 10/11

- [ ] MSI und NSIS installieren; App startet, kein schwarzes Konsolenfenster beim Start, bei „Erneut suchen“, beim Maschinenstart oder Anlegen einer Platte.
- [ ] Engines: QEMU unter `C:\Program Files\qemu` gefunden, RetroArch per winget gefunden, Cores-Ordner neben retroarch.exe.
- [ ] „PC — aktuell (UEFI)“: UEFI-Firmware gefunden (kein „UEFI-Firmware nicht gefunden“), Start mit WHPX; ohne Hypervisorplattform verständlicher Fehler im Log.
- [ ] Windows-11-ISO: Log nennt „TPM: swtpm nicht gefunden“ (erwartet).
- [ ] QMP-Steuerung: Pause/Fortsetzen/Reset/Stopp; Snapshot laufend.
- [ ] Gemeinsamer Ordner: Maschine startet, Hinweis „virtio-9p gibt es in QEMU für Windows nicht“ im Log.
- [ ] Pfade mit Leerzeichen/Umlauten (`C:\Users\Jürgen\ISOs\Win 98, DE.iso`) als CD einlegen und starten.
- [ ] Anderer Maschinenordner auf zweitem Laufwerk (`D:\VMs`).
- [ ] Konsole (SNES/GBA) mit RetroArch: Start, Save State, Stopp (QUIT).
- [ ] Schließen mit laufender Maschine → Nachfrage; „OK“ beendet QEMU-Prozess (Task-Manager prüfen).
- [ ] Updater-Prüfung in Einstellungen → App meldet „aktuell“ bzw. Fehler ohne Absturz.

## Verifikation (unabhängige Zweitprüfung, 2026-10-19)

Zweiter Prüfer, Ziel: die Aussagen oben widerlegen. Alles unten selbst ausgeführt, außer wo „statisch“ steht.

### Nachgemessen

| Was | Ergebnis |
| --- | --- |
| `cargo test --workspace` | core 39 grün; App 9 grün (vorher 7, +2 neue QMP-Tests), 1 ignored |
| `cargo test -p virtual -- --ignored qemu_live --nocapture` | grün, wirklich gegen `/opt/homebrew/bin/qemu-system-i386` gelaufen (Test kehrt sonst still zurück — Ausgabe geprüft) |
| `cargo run -p virtual-core --example qemucheck` | 49 ok, 0 fehlgeschlagen |
| `pnpm build` (tsc + vite) | grün |
| `pnpm audit` | keine Funde |
| `cargo audit` | nur `zip` 4.6.1 (Updater) + unmaintained-Warnungen, wie oben |
| `cargo clippy --workspace --all-targets` | nur Stil-Warnungen |
| `cargo check --target x86_64-pc-windows-msvc -p virtual-core --all-targets` | grün; die App selbst lässt sich auf dem Mac nicht für MSVC prüfen (`ring` braucht den MSVC-C-Compiler) — Windows-Zweige der App nur statisch |
| i18n DE/EN | 244/244 Schlüssel, Handbuch-Abschnitte paarweise vorhanden |
| IPC | alle 31 `invoke`-Namen und Argumente in `src/api.ts` passen zu den registrierten Rust-Kommandos (camelCase → snake_case) |
| Version | 1.0.0 in package.json, tauri.conf.json, beiden Cargo.toml, Cargo.lock, `APP_VERSION` |
| RetroArch (`retrocheck`) | nicht erneut ausgeführt (öffnet ein RetroArch-Fenster); Code-Änderungen dort nur gelesen |

### Widerlegt bzw. unvollständig

1. **`open_path` ließ weiterhin Programme starten.** Die Prüfung „nur vorhandene Ordner“ greift unter macOS
   nicht: ein `.app`-Bundle *ist* ein Ordner, `open` startet es. Jetzt nur noch Ordner innerhalb des
   Maschinenordners (kanonisch, `..` kann nicht ausbrechen), ohne Bundle-Endungen (`.app`, `.bundle`, …).
   Die Oberfläche öffnet ohnehin nur Maschinenordner. Unit-Test erweitert (außerhalb, `.app` innen und außen, `..`).
2. **Lange Snapshots liefen in den 20-s-Lese-Timeout.** `savevm`/`loadvm` schreiben bzw. lesen den gesamten
   Gast-RAM; bei 4–8 GB (Windows-11-Profil) dauert das länger als 20 s. Folge: Fehlermeldung, obwohl QEMU
   weiter sichert — und weil jede HMP-Anfrage dieselbe id `hmp` hatte, wäre die verspätete Antwort dem
   *nächsten* Aufruf zugeordnet worden (falscher Erfolg/Fehler). Jetzt: HMP wartet bis 15 min, jede Anfrage
   bekommt eine eindeutige id, verspätete Antworten werden übersprungen. Zwei Unit-Tests mit simuliertem Strom.
3. **Doppelklick auf Start.** Der zweite Start löschte den QMP-Socket des ersten und startete einen zweiten
   QEMU, bevor die Kollision bemerkt wurde. Jetzt sperrt eine „startet gerade“-Menge den zweiten Start sofort.
4. Fehlertext „Medienwechsel … folgt in 0.2“ in einer 1.0 → „in einer späteren Version“.
5. Handbuch (DE/EN) nachgezogen: Öffnen nur https-Links und Maschinenordner, keine `.app`-Bundles.

### Bestätigt

HDA-Codec-Fix, Komma-Maskierung, GL-Abschaltung auf macOS, Host-Fähigkeiten für 9p/SMB, localhost-Weiterleitungen,
VNC `to=99`, kurze Socket-Pfade, `spawn_blocking` ohne gehaltene Runtime-Sperre, `CREATE_NO_WINDOW`, UEFI-Suche im
Windows-Installer-Layout, Secure-Boot-Logik, Wegwerf-Snapshot-Ablehnung, Schließen-Nachfrage (eigener Dialog statt
`window.confirm`), `save_machine` behält Platten/Medien/Slot (die Oberfläche bearbeitet Platten nirgends über den Entwurf).

### Weiterhin offen / nur statisch

- Windows-Laufzeit komplett (WHPX, SDL, dsound, QMP über TCP, MSI/NSIS) — siehe manuelle Testliste.
- ⌘Q-Abfangen (`RunEvent::ExitRequested` mit `code: None`) nur im Code geprüft; ob macOS-„Beenden“ aus dem
  App-Menü wirklich dort ankommt und `prevent_exit` greift, muss am Gerät getestet werden.
- OpenGL unter Windows: `ctx.gl` gilt für jeden Nicht-macOS-Host. Ob der QEMU-Windows-Build `sdl,gl=on` und
  `virtio-vga-gl` kann, ist ungeprüft — bei Abbruch die GL-Option abwählen (Standardprofile haben GL aus).
- Windows-11-Gäste: ohne swtpm kein TPM — der Windows-11-Installer verweigert dann die Installation.
- `quit_app` räumt Sockets im Temp-Ordner nicht auf (werden beim nächsten Start der Maschine ersetzt).

## Runde 2 (2026-10-20)

Basis `716fbc4`. Ziel: Reste aus Runde 1 im Code beheben und die App gegen die Fehlermuster L1–L13
aus den anderen Tools prüfen. „Getestet“ heißt ausgeführt; was nur am Gerät geht, steht unten.

### Testlauf

| Was | Befehl | Ergebnis |
| --- | --- | --- |
| Rust-Kern + App | `cargo test --workspace` | core 47 (vorher 39) + App 12 (vorher 9) grün, 1 ignored = Live-Test |
| QMP + Snapshots live | `cargo test -p virtual -- --ignored qemu_live --nocapture` | grün gegen `/opt/homebrew/bin/qemu-system-i386` (Pause/Weiter/Reset laufen jetzt über die neue Warte-Logik) |
| Backend-Übersetzung | `pnpm test:ui` | 4/4 grün; der Abdeckungstest findet alle deutschen Meldungen in `src-tauri/` und `core/` und verlangt je ein Muster |
| Windows-Typecheck | `cargo check --target x86_64-pc-windows-msvc --workspace --tests` (MSVC-Header/`llvm-rc` gestubbt, kein Linken) | grün, App und Kern — die `cfg(windows)`-Zweige kompilieren jetzt tatsächlich (Runde 1: nur Kern) |
| Frontend | `pnpm build` (tsc + vite) | grün |
| Oberfläche | headless Chromium + gemocktes IPC (Dialog-Plugin wie 2.7: `window.confirm` immer truthy), 960×600 = Mindestgröße, DE/EN × dunkel/hell × Windows-/macOS-Plattform | 4 × 15 Schritte grün: keine Überläufe, keine fremdsprachigen Texte, keine Konsolenfehler |
| Abhängigkeiten | `cargo audit`, `pnpm audit` | unverändert nur `zip` 4.6.1 im Updater (s. u.); pnpm ohne Funde |

### Behoben

1. **L2 — Backend-Meldungen deutsch in der englischen Oberfläche.** `src/backendText.ts` übersetzt alle
   Fehler, Start-Hinweise (Log, Kommandozeilen-Vorschau) und Installationshinweise; eingebettete Ursachen
   („Abbild nicht lesbar: CUE: …“) werden rekursiv mitübersetzt, Engine-Ausgabe bleibt unverändert.
   Gilt für Toasts, Fehlerleiste, Log, Vorschau, Engine-Liste und Assistent.
2. **L12 — Profile nur deutsch.** Alle 27 Profile haben einen Block `en` (Name, Familie, Versionen,
   Hinweis, Notizen); Assistent, Kopfzeile und Namensvorschlag folgen der Sprache, die Suche findet beide.
   Test `every_builtin_profile_has_english_texts`.
3. **L9 — gleichzeitige Vorgänge.** Neue Belegung je Maschine (`AppState.busy`, RAII-Guard): Start,
   Snapshot (anlegen/wiederherstellen/löschen), Medium einlegen (Wandlung) und Löschen schließen sich
   gegenseitig aus. Vorher konnte man nach einem Wechsel der Auswahl (die Oberfläche vergaß ihren
   „busy“-Zustand) während eines langen Snapshots starten oder die Maschine löschen; eine Wandlung schrieb
   dann in einen gelöschten Ordner. Löschen prüft jetzt auch „startet gerade“.
4. **L9 — Steuerung hing hinter langen Snapshots.** `savevm` hält die QMP-Verbindung bis 15 min. Pause,
   Weiter, Reset und Stopp warteten so lange (Knopf hängt), die 4-s-Statusabfrage stapelte blockierte
   Threads, und **„Stopp erzwingen“ wartete ebenfalls** auf das Ende des Snapshots. Jetzt: Steuerung wartet
   max. 3 s und meldet „Engine beschäftigt“, die Statusabfrage überspringt, „Stopp erzwingen“ beendet sofort.
   Unit-Test mit gehaltener Verbindung.
5. **L11/L13 — Update-Fehler verschluckt; Update beendete Gäste ohne Nachfrage.** Installationsfehler
   (Banner, Einstellungen, Auto-Update) erscheinen als Meldung in der UI-Sprache, der Prüffehler zeigt die
   Ursache. Laufen Maschinen, fragt virtual vorher; das Backend lädt erst, beendet dann die Engines und
   installiert (der Windows-Installer beendet virtual sofort — QEMU lief sonst verwaist weiter).
6. **L9 — Beenden während eines Vorgangs.** Schließen/⌘Q fragt auch, wenn nur ein Vorgang läuft
   (z. B. qemu-img-Snapshot einer gestoppten Maschine), mit eigenem Text.
7. **L4/L5 — CUE aus Windows.** CUE mit UTF-8-BOM scheiterte („keine FILE-Zeile“), Windows-1252-Umlaute
   im Dateinamen machten die Datei unlesbar, `Unterordner\Datei.bin` und abweichende Schreibweise wurden
   unter macOS nicht gefunden. Alle vier Fälle behoben und getestet.
8. **L4 — BOM in JSON.** Eigene Profile, `machine.json`, `snapshots.json` und `settings.json` aus dem
   Windows-Editor (UTF-8 mit BOM) wurden still ignoriert bzw. auf Standard zurückgesetzt.
9. **L5 — Windows-Dateisystem.** Atomares Umbenennen (JSON, ISO-Wandlung) wiederholt bei Freigabe-/Sperr-
   verletzungen (Virenscanner, OneDrive) bis ~3 s; reservierte Namen (`CON`, `NUL`, `COM1` …) als
   Maschinenname ergaben einen Ordner, den Windows nicht anlegen kann → `con-vm`; Pfade aus den
   Einstellungen ohne Anführungszeichen („Als Pfad kopieren“), nacktes `D:` → `D:\` (sonst `D:vm`
   relativ zum Laufwerks-Arbeitsordner); RetroArch-Pfad als Datei ohne Rücksicht auf Groß-/Kleinschreibung;
   gewandelte ISO-Namen auf 80 Zeichen gekürzt (MAX_PATH unter AppData), ohne Punkt/Leerzeichen am Ende;
   abgebrochene Wandlung lässt keine `.iso.part` liegen.
10. **L8 — Log nach Neustart.** `machine_log` las die ganze `log.txt` in den Speicher; jetzt nur die
    letzten 512 KiB. Löschen einer Maschine (große Platten, Virenscanner) läuft nicht mehr im UI-Thread.
11. **L6 — Tastenbeschriftung.** „⌘/Strg“ fest verdrahtet → macOS „⌘ ,“, Windows „Strg + ,“ (DE) bzw.
    „Ctrl + ,“ (EN) in Einstellungen und Handbuch.
12. **L3 — Sprache beim ersten Start.** Das Backend nutzt `sys-locale` (macOS: CFLocale, Windows:
    GetUserDefaultLocaleName — nicht `LANG`), war also korrekt; fällt das Laden der Einstellungen aus,
    nimmt die Oberfläche jetzt `navigator.language` statt fest Deutsch.
13. Kleinigkeiten: Esc schließt bei offener Nachfrage nur die Nachfrage (nicht auch die Einstellungen
    darunter); `n`/`?` lösen hinter dem offenen Handbuch nichts aus; Snapshot-Dialog schließt mit Esc;
    Medien-Knöpfe und „Maschine löschen“ sind während einer Wandlung gesperrt; Snapshot löschen sperrt
    die Knöpfe bis zum Ende.

### L1–L13 geprüft

| Muster | Ergebnis |
| --- | --- |
| L1 `window.confirm/alert` | kein Treffer — eigener Promise-Dialog; im Mock liefert `window.confirm` wie Plugin 2.7 immer truthy, Abbrechen ruft trotzdem nichts auf (Wiederherstellen, Löschen, Beenden, Update geprüft) |
| L2 Backend-Sprache | behoben (1) |
| L3 Erststart-Sprache | in Ordnung (sys-locale), Fallback ergänzt (12) |
| L4 BOM | behoben (7, 8); CSV gibt es nicht |
| L5 Windows-Dateisystem | behoben (9); Namen aus fremden Quellen: die einzige ist der `FILE`-Eintrag einer CUE — er wird nur gelesen, nie als Zielname benutzt |
| L6 Kürzel | behoben (11) |
| L7 Fremdinhalte | kein gerendertes HTML, kein `dangerouslySetInnerHTML`, CSP `default-src 'self'`; Update-Notizen werden nicht angezeigt |
| L8 schwere Arbeit | alle langen Kommandos `spawn_blocking`; Log-Tail und Löschen ergänzt (10) |
| L9 lange Vorgänge | behoben (3, 4, 5, 6) |
| L10 Zeit | Zeitstempel UTC (`…Z`), Anzeige lokal (geprüft: 00:30Z am 29.03.2026 = 01:30 MEZ, Tag der Zeitumstellung); keine Datumsauswahl, keine Wochen |
| L11 Bestätigung/Fehler | Löschen, harter Stopp, Wiederherstellen, Snapshot löschen, Beenden, Update mit laufenden Maschinen fragen; Update-Fehler werden angezeigt (5) |
| L12 Texte | i18n DE/EN vollständig (tsc erzwingt gleiche Schlüssel), Profile (2), Handbuch DE/EN ergänzt |
| L13 Updater | Version 1.0.0, Endpoint https, Pubkey gesetzt; Vergleich macht das Plugin (semver); Fehler in UI-Sprache (5) |

### Sicherheit

- `zip` 4.6.1 (RUSTSEC-2026-0367/0368/0369) bleibt: kein semver-kompatibles Update von
  tauri-plugin-updater. Der Updater entpackt nur minisign-signierte Pakete; die Windows-Artefakte
  (`createUpdaterArtifacts: true`, nicht v1-kompatibel) sind `.exe`/`.msi` und laufen gar nicht über zip.
- Windows-QMP über TCP 127.0.0.1 ohne Authentifizierung: unverändert offen (Rust-std ohne AF_UNIX).
- Neu: keine. Die Belegung je Maschine verhindert zusätzlich, dass zwei Prozesse gleichzeitig dieselbe
  qcow2 öffnen.

### Nur am Gerät prüfbar (zusätzlich zur Liste oben)

- [ ] Windows: Update mit laufender Maschine → Nachfrage; nach „OK“ kein qemu-system-Prozess mehr im Task-Manager, Installer startet.
- [ ] Windows: Maschine „CON“ anlegen → Ordner `con-vm`; Maschinenordner `D:` (ohne Backslash) in den Einstellungen → Maschinen landen in `D:\<name>`.
- [ ] Windows: QEMU-Ordner per Explorer „Als Pfad kopieren“ (mit Anführungszeichen) einfügen → „Erneut suchen“ findet QEMU.
- [ ] Windows: CUE aus einem alten Brennprogramm (Windows-1252, Umlaut im BIN-Namen) einlegen → ISO entsteht.
- [ ] Windows mit aktivem Defender/OneDrive-Ordner als Maschinenordner: Maschine anlegen, Hardware speichern, Snapshot — keine „Umbenennen fehlgeschlagen“-Meldung.
- [ ] Beide: Snapshot einer laufenden Maschine mit 4–8 GB RAM; währenddessen Auswahl wechseln und zurück, Pause drücken (→ „Engine beschäftigt“ nach 3 s), „Stopp erzwingen“ (→ sofort aus).
- [ ] Beide: Schließen während eines Snapshots einer gestoppten Maschine → eigener Hinweistext.
- [ ] Beide: englische Oberfläche — Start ohne QEMU zeigt „qemu-system-… not found — …“.

## Runde 2 – Verifikation (unabhängige Zweitprüfung, 2026-10-20)

Geprüft wurde der Diff `716fbc4..369b032` sowie L1–L13 an Stellen, die Runde 2 nicht genannt hatte.

### Nachgemessen (auf dem Stand dieser Prüfung)

- `cargo test --workspace`: core 47/47, App 12/12 (Belegungstest um Op::Edit erweitert); Live-Test `qemu_live` gegen das echte qemu-system-i386 erneut grün.
- `pnpm test:ui` 4/4 — die Abdeckung wurde zusätzlich mit einer eigenen, breiteren Suche über *alle*
  deutschen String-Literale in `src-tauri/` und `core/` gegengeprüft: ohne Übersetzung bleiben nur
  ein Testausgabetext und ein `expect` für eingebettete Profile (beide nie in der Oberfläche).
- `pnpm build` grün. Windows-Typecheck (`cargo check --target x86_64-pc-windows-msvc --workspace --tests`,
  gestubbte llvm-rc/MSVC-Header, nichts gelinkt) ohne Warnungen, nach erzwungenem Neu-Check beider Crates.
- `cargo audit`: nur `zip` 4.6.1 über tauri-plugin-updater (kein semver-kompatibles Update, `cargo update -p zip`
  ändert nichts) plus Unmaintained-Warnungen.
- Headless Chromium mit gemockter IPC (Dialog-Plugin-2.7-Verhalten, `window.confirm` liefert ein Promise):
  17 Abläufe × 4 Konfigurationen (DE/EN × dunkel/hell × Windows/macOS) bei 960×600 — 68/68 grün,
  keine Konsolenfehler, kein Überlauf, keine falsche Sprache. Die zwei neuen Abläufe schlagen auf dem
  Stand vor dieser Prüfung nachweislich fehl.

### Gefunden und behoben

1. **Verlorene Änderung während einer Wandlung (L9).** `save_machine` und `detach_media` belegten die
   Maschine nicht. Wurde während einer minutenlangen NRG/BIN-Wandlung die Hardware gespeichert oder ein
   anderes Medium entfernt, schrieb die Wandlung am Ende ihren vorher geladenen Stand zurück — die
   Änderung war still weg (die Oberfläche hatte „Gespeichert“ gemeldet), ebenso konnte `machine.json`
   in einen gerade gelöschten Ordner geschrieben werden. Beide Kommandos belegen jetzt kurz (`Op::Edit`)
   und werden während eines Vorgangs mit Hinweis abgelehnt; in der Oberfläche sind „Speichern“ und
   „Entfernen“ währenddessen gesperrt. Neue Meldung „Maschine wird gerade gespeichert“ mit Übersetzung.
2. **„Stopp erzwingen“ war während eines Snapshots gesperrt (L9/L12).** Das Backend beendet die Engine
   seit Runde 2 sofort, der Knopf war aber an denselben `busy`-Zustand gebunden wie der Snapshot — er
   ließ sich nur nach Wechsel der Auswahl erreichen, entgegen Handbuch. Er bleibt jetzt beim Anlegen und
   Zurückspielen eines Snapshots bedienbar (mit Nachfrage); Pause/Stopp bleiben gesperrt.

Handbuch (DE/EN, Abschnitt Version 1.0) und README sind ergänzt.

### Bestätigt (Stichproben über die Angaben von Runde 2 hinaus)

- L1: kein `window.confirm/alert/prompt` im Code; Capability nur `core:default` + `dialog:allow-open`.
- L3: Rust-Standard über `sys-locale`, Oberfläche fällt ohne Einstellungen auf `navigator.language` zurück.
- L5: `slugify` liefert nur `[a-z0-9-]`, reservierte Namen → `-vm`; CUE-`FILE` wird nur gelesen.
- L9: Doppelstart — `start_machine_inner` prüft unter der Belegung erneut, die Laufzeit-Map verwirft
  einen zweiten Prozess; Löschen prüft „läuft“ erst unter der Belegung.
- L10: Zeitstempel UTC mit `Z`, Anzeige über `toLocaleString`.
- L13: Version 1.0.0, https-Endpoint, Pubkey; Signaturprüfung im Plugin beim Download, vor dem Beenden der Engines.

### Weiterhin offen

- Alles aus „Weiterhin offen“ und „Nur am Gerät prüfbar“ von Runde 2 gilt unverändert; zusätzlich am Gerät:
  - [ ] Beide: Snapshot einer laufenden Maschine mit viel RAM, Dialog schließen, „Stopp erzwingen“ in derselben Ansicht → Engine sofort aus, Snapshot-Fehler wird angezeigt.
  - [ ] Beide: große NRG einlegen, während der Wandlung Hardware-Tab öffnen → „Speichern“ und „Entfernen“ gesperrt; nach Abschluss wieder frei.
- Ein Update mit laufendem `qemu-img`-Snapshot einer gestoppten Maschine fragt nicht nach (nur laufende
  Maschinen werden gezählt); der Windows-Installer beendet virtual, `qemu-img` läuft weiter — wie beim Beenden.

## Runde 3 (2026-10-20)

Basis `f9e6258`. Ziel: jede Aussage in Handbuch (DE/EN), README und Oberflächentexten gegen den Code
prüfen, Nutzerwege inklusive Fehlerfällen durchspielen, die Reste aus Runde 2 (Windows-QMP,
`qemu-img` beim Beenden/Update) erledigen. „Ausgeführt“ = selbst laufen lassen; „gelesen“ = nur Code.

### Testlauf

| Was | Befehl | Ergebnis |
| --- | --- | --- |
| Rust-Kern + App | `cargo test --workspace` | core 49 (vorher 47) + App 15 (vorher 12) grün, 1 ignored = Live-Test |
| QMP + Snapshots live | `cargo test -p virtual -- --ignored qemu_live --nocapture` | grün gegen `/opt/homebrew/bin/qemu-system-i386`; neu: gesperrte Platte, schreibgeschützter Ordner, verwaister Listeneintrag |
| Volle Platte | `VIRTUAL_FULL_DIR=/Volumes/vfull cargo test … write_iso_into / save_into` auf einer 4-MB-RAM-Disk (`hdiutil attach -nomount ram://8192`) | grün: „No space left on device (os error 28)“, keine `.iso.part`/`.tmp`, altes `machine.json` unverändert |
| Alle QEMU-Profile live | `cargo run -p virtual-core --example qemucheck` | 49 ok, 0 fehlgeschlagen |
| Backend-Übersetzung | `pnpm test:ui` | 4/4 grün (neuer Log-Hinweis hat sein Muster) |
| Windows-Typecheck | `cargo check --target x86_64-pc-windows-msvc --workspace --tests` (gestubbte MSVC-Header/`llvm-rc`, nichts gelinkt) | grün, ohne Warnungen |
| Frontend | `pnpm build` | grün |
| Clippy | `cargo clippy --workspace --all-targets` | nur die bekannten Stil-Warnungen, keine neue aus dieser Runde |
| Oberfläche | headless Chromium + gemocktes IPC (Dialog-Plugin 2.7: `window.confirm` = Promise), 960×600, DE/EN × dunkel/hell × Windows/macOS | 23 Abläufe × 4 = 92/92 grün (6 neu), keine Konsolenfehler, kein Überlauf, keine fremdsprachigen Texte |

### Falsche Aussagen — behoben

1. **„Gemeinsame Ordner sind für Retro-Profile schreibgeschützt“ stimmte für alte Gäste nicht.** Alte
   Gäste (IDE-Bus) bekommen Ordner nur per SLIRP-SMB, und QEMU schreibt dafür `read only=no` in die
   smb.conf — das Häkchen „Gemeinsame Ordner schreibgeschützt“ (Standard aller Retro-Profile) und
   „nur lesen“ am Ordner wurden ignoriert, der Gast konnte auf den Host-Ordner schreiben. Jetzt wird
   ein schreibgeschützter Ordner nicht per SMB freigegeben, das Log nennt den Grund; wer schreiben
   lassen will, schaltet den Schutz bewusst ab. Test `smb_share_never_writable_when_read_only_is_wanted`.
2. **„Retro-Profile laufen ohne Beschleuniger (TCG)“ stimmte auf Windows und Intel-Macs nicht.** Der
   Beschleuniger hing nur an der Architektur: i386-Retro-Profile (486, Pentium 133, P III, XP) liefen
   dort mit WHPX/HVF — die Taktbremse (`-icount`, TCG-only) wurde still übergangen, Windows 95 droht auf
   schnellen CPUs am Timer zu scheitern. Jetzt virtualisiert nur die Kategorie „modern“
   (`detect::accel_for_machine`, Test in `accel_matches_host`). Auf Apple Silicon ändert sich nichts.
3. **RetroArch-Steuerung „UDP, nur localhost“.** Ausgeführt: RetroArch 1.22.2 mit
   `network_cmd_enable` → `lsof` zeigt `UDP *:<port>` — RetroArch lauscht auf **allen** Schnittstellen,
   ohne Anmeldung; andere Rechner im LAN könnten Beenden/Spielstand laden schicken. virtual sendet nur
   an 127.0.0.1, kann die Bindung aber nicht einschränken (RetroArch hat keine Option dafür). Text in
   Handbuch (Konsolen, Isolation) und README korrigiert, Firewall-Hinweis; Port bleibt zufällig je Start.
4. **Netz „isoliert — nur der Host“.** `restrict=on` trennt den Gast auch vom Host; erreichbar sind nur
   Weiterleitungen und Freigaben. Auswahltext, Tutorial (DE/EN) korrigiert.
5. **Tarif nested „bringt Snapshot-Bäume, Klone, Headless, CLI“** (Einstellungen, Handbuch, README):
   in 1.0 schaltet er nur die freien Engine-Argumente frei; Headless gibt es für alle. Als geplant
   gekennzeichnet; README nannte außerdem eine CLI als vorhanden und ein „Gast-Netz mit Proxy“ — beides gibt es nicht.
6. **Profil-Angaben:** „Retro-PC (486 bis Pentium 4)“ → bis Pentium III (XP-Profil = pentium3);
   „PC 1996 — … S3“ → Cirrus-VGA; PC-Profile „Cirrus-VGA, SB16“ → Standard-VGA oder Cirrus, XP mit AC97;
   „beta“-Liste ohne Dreamcast/Saturn; Mega-CD-/PC-Engine-CD-BIOS als optional gekennzeichnet (die
   Profile verlangen sie nicht, „Medien“ meldet sie nicht).
7. **Kleinere:** „Hardware → Takt“ heißt „Taktbremse“; Windows 9x „von A: starten“ braucht
   „Booten von → Diskette“ (automatisch bootet die CD zuerst); Einstellung „Vor Löschen und hartem
   Stopp nachfragen“ gilt auch für Wiederherstellen/Snapshot löschen (Text ergänzt); RetroArch-Menü-Pause
   wird nicht erkannt (jetzt im Handbuch); `mount -t 9p` ohne `-o trans=virtio`; Assistent bot `.vfd`
   nicht an, Medien-Hinweis nannte nur IMG/IMA.

### Weitere Fehler — behoben

8. **Beenden/Update schnitten `qemu-img` ab bzw. fragten beim Update nicht (Rest aus Runde 2).**
   `quit_app` und `install_update` beenden die Engines und warten dann bis zu 60 s auf laufende
   Vorgänge (`AppState::wait_idle`) — ein Offline-Snapshot/-Wiederherstellen schreibt die qcow2 zu Ende
   (ein abgebrochenes `snapshot -a` kann die aktive L1-Tabelle halb überschreiben), die Liste stimmt.
   Hängt ein Vorgang länger, beendet sich virtual trotzdem; `qemu-img` wird bewusst *nicht* getötet und
   läuft dann allein zu Ende. Das Update fragt jetzt auch, wenn nur ein Vorgang läuft (`busy_ops`);
   die Oberfläche meldet beim Beenden „wartet auf Vorgang“. Anlegen einer Maschine (qemu-img create,
   ggf. CD-Wandlung) zählt als Vorgang. Test `quit_waits_for_running_operations`, UI-Abläufe
   `quitWhileBusy`, `updateWhileBusy`.
9. **Lesefehler beim Wandeln ergaben ein halbes ISO als Erfolg** (`read_exact` → `break` bei jedem
   Fehler, Markierung „aktuell“ geschrieben, beim nächsten Einlegen wiederverwendet). Jetzt bricht nur
   ein Dateiende ab, echte Lesefehler (USB getrennt, gesperrter Bereich) melden einen Fehler.
10. **Volle Platte hinterließ `machine.tmp`/`snapshots.tmp`** (`write_atomic` räumte nur beim
    Umbenennen auf). Behoben, auf RAM-Disk geprüft.
11. **Snapshot ohne schreibbare Liste blieb unsichtbar in der Platte** (Ordner schreibgeschützt, Platte
    voll): wird jetzt zurückgenommen (`qemu-img snapshot -d` bzw. `delvm`). **Listeneintrag ohne
    Snapshot in der Platte ließ sich nie löschen** („snapshot not found“): wird jetzt entfernt. Beides live.
12. **Konsolen-Slot nach Fehler verschoben:** Scheiterte ein Save State (Timeout), hatte RetroArch den
    Slot schon gewechselt, `machine.json` aber nicht — die nächsten Slots zählten falsch. Der Slot wird
    jetzt auch im Fehlerfall gespeichert (gelesen, kein RetroArch-Lauf).
13. **Assistent: `.img` war immer Diskette** — ein CloneCD-IMG oder roher CD-Abzug wurde als Diskette
    eingelegt. Über 2,88 MB gilt `.img` jetzt als CD (wird wie gewohnt gewandelt). Test.

### Windows-QMP (Rest aus Runde 1/2) — bewertet, nicht umgebaut

- `-chardev pipe` (Named Pipe): QEMU legt die Pipe mit einer Instanz an und nimmt danach keinen
  neuen Client an; Rust-std kann auf einer Pipe-Datei keinen Lese-Timeout setzen (die 3-s-/15-min-
  Fristen der Steuerung hingen daran). Gewinn gegenüber TCP gering (die Standard-DACL einer Pipe gibt
  allen Konten Lesezugriff), Risiko hoch und vom Mac aus nicht prüfbar → nicht umgesetzt.
- Token je Start: QMP kennt keine Anmeldung; TLS-PSK (`tls-creds-psk`) kann rustls nicht → nicht möglich.
- Was schützt: Port zufällig je Start (`free_port`), und virtual hält die eine Verbindung, die QEMU
  bedient. Ausgeführt gegen QEMU 11.1.1: Ein zweiter Client bekommt keine Begrüßung und keine Antwort,
  solange der erste verbunden ist (erst nach dessen Trennung). Restrisiko: ein lokales Programm, das
  in den Millisekunden zwischen QEMU-Start und Verbindung zuvorkommt — virtual zeigt dann „Keine
  QMP-Steuerung“. Bleibt „Offen (niedrig)“; im Handbuch so beschrieben.

### Nutzerwege (ausgeführt)

| Weg | Wie | Ergebnis |
| --- | --- | --- |
| Erster Start, Übersicht, alle Reiter | UI-Mock `start`, `overview`, `tabs` (DE/EN) | grün |
| Maschine im Assistenten mit Medium anlegen | UI `wizardCreateWithMedium` (Filter mit `.vfd`, Zusammenfassung, `create_machine`-Argumente); Rust `wizard_media_kind_by_extension_and_size` | grün |
| Medium einlegen: Fehler, abgebrochener Dialog, Sperren während Wandlung | UI `attachError`, `cancelledDialog`, `attachLocksEditing` | grün; Abbruch ruft nichts auf |
| Start, Pause, Engine beschäftigt | UI `startAndPauseBusy`; live `qemu_live` (stop/cont/reset) | grün |
| Snapshot anlegen/zurück/löschen, offline und laufend | live `qemu_live`; UI `snapshotCreate`, `snapshotRestoreConfirm`, `forceStopDuringSnapshot` | grün |
| Gesperrte Platte (App neu gestartet, QEMU läuft verwaist) | live: Offline-Snapshot → „Failed to get "write" lock“, Liste unverändert | grün |
| Schreibgeschützter Ordner | Rust: Wandlung, `machine.json`, Snapshot-Liste (live mit Rücknahme) | grün |
| Volle Platte | RAM-Disk: Wandlung und `machine.json` | grün |
| Beenden während Maschine/Vorgang, Update während Vorgang | UI `closeRequested`, `quitWhileBusy`, `updateWhileRunning`, `updateWhileBusy`; Rust `quit_waits_for_running_operations` | grün |
| Einstellungen, Kürzel, Handbuch DE/EN | UI `settings`, `help`, `helpRound3` (Suche, gleiche Abschnitte DE/EN) | grün |

Nur gelesen: Neustart mitten in einer Wandlung (`.iso.part` bleibt liegen, wird beim nächsten
Einlegen überschrieben, ohne Herkunftsmarkierung nie wiederverwendet); RetroArch-Fehlerpfade.

### Nur am Gerät prüfbar (zusätzlich)

- [ ] Windows: Retro-PC-Profil (Pentium 133) starten → Kommandozeilen-Vorschau zeigt `accel=tcg` und `-icount`; Windows 95 bootet ohne Schutzfehler.
- [ ] Windows: Windows-Firewall fragt beim ersten Konsolenstart nach RetroArch → „Abbrechen“; Steuerung aus virtual funktioniert trotzdem (localhost).
- [ ] Beide: Snapshot einer gestoppten Maschine mit großer Platte, währenddessen schließen → Hinweis „wartet bis zu 60 s“, App beendet sich nach dem Snapshot, Snapshot steht nach Neustart in der Liste.
- [ ] Beide: Update-Banner, während nur ein Offline-Snapshot läuft → eigene Nachfrage.
- [ ] macOS mit Samba: Retro-Profil mit Ordner und Netz „isoliert“ → kein SMB, Hinweis im Log; „schreibgeschützt“ abschalten → `\\10.0.2.4\qemu` im Gast beschreibbar.

## Runde 3 – Verifikation (unabhängige Prüfung, 2026-10-20)

Geprüft wurde der Diff `f9e6258..15f900f` (Regressionen plattformübergreifend, Nebenläufigkeit, i18n,
Handbuch DE/EN) sowie zehn Handbuch-Aussagen, die Runde 3 nicht genannt hatte.

### Nachgemessen (selbst ausgeführt)

- `cargo test --workspace`: core 49/49, App 15/15, 1 ignoriert; Live-Test `qemu_live` (`--ignored`) gegen
  das echte qemu-system-i386/qemu-img 11.1.1 grün (inkl. Geister-Eintrag löschen, Rücknahme bei
  schreibgeschütztem Ordner, gesperrte Platte bei verwaistem QEMU).
- `pnpm test:ui` 4/4, `pnpm build` (tsc + vite) grün; eigene Suche über alle deutschen Literale in
  `core/` und `src-tauri/`: ohne Übersetzung nur Testcode und das `expect` für eingebettete Profile.
- UI-Harness (headless Chromium, Dialog-Plugin-2.7-Mock) erneut: 23 Abläufe × 4 Konfigurationen = 92/92,
  keine Konsolenfehler, kein Überlauf, keine falsche Sprache.
- Windows-Typecheck (`cargo check --target x86_64-pc-windows-msvc --workspace --tests`, gestubbte
  MSVC-Header/llvm-rc, nichts gelinkt) grün, ohne Warnungen.
- `cargo clippy --workspace --all-targets`: nur ältere Stil-Warnungen, keine in den Änderungen von Runde 3.
- `cargo audit`: unverändert `zip` 4.6.1 (RUSTSEC-2026-0367/0368/0369) über tauri-plugin-updater plus
  Unmaintained-Hinweise (unic-*, proc-macro-error, zbus_names).
- Nicht wiederholt: volle Platte auf RAM-Disk (`VIRTUAL_FULL_DIR`), `qemucheck` über alle Profile.

### Diff-Review — keine Regression gefunden

- `accel_for_machine`: Alle eingebauten Profile tragen eine Kategorie; `machine.json` übernimmt sie beim
  Anlegen. Nur `modern` wird beschleunigt — Retro-Mac/andere Architekturen hatten ohnehin keinen
  Beschleuniger. Die Host-Anzeige (Einstellungen → Engines) nutzt weiterhin `accel_for` (richtig).
- Beenden: `quit_app` wartet in einem eigenen Thread; `app.exit(0)` löst `ExitRequested` mit Code aus und
  wird nicht erneut abgefangen. Ein während der Wartezeit gestarteter Gast wird über `RunEvent::Exit`
  (`stop_all`) beendet. `Op::Start` ist nur während des Hochfahrens belegt — eine laufende Maschine
  verlängert das Warten nicht.
- `write_iso`, `write_atomic`, Snapshot-Rücknahme, Löschen eines Geister-Eintrags und die Slot-Sicherung
  bei Konsolen sind korrekt; das Speichern nach einem Konsolen-Fehler kann die ursprüngliche Meldung nur
  verdecken, wenn zusätzlich `machine.json` nicht schreibbar ist (hingenommen).

### Gefunden und behoben

1. **Falscher Bedienweg zum SMB-Schreibschutz.** Log-Hinweis (DE/EN) und Handbuch (Isolation & Sicherheit,
   DE/EN) verwiesen auf „Hardware → Isolation“ — einen solchen Abschnitt gibt es im Hardware-Tab nicht;
   das Häkchen heißt „Gemeinsame Ordner schreibgeschützt“. Außerdem genügte „den Schreibschutz abschalten“
   nicht: Ordner, die bei aktivem Schreibschutz hinzugefügt wurden, tragen selbst „nur lesen“, und SMB
   entfällt, solange eines der beiden gesetzt ist. Hinweis, Übersetzung und Handbuch nennen jetzt beide
   Häkchen. Der Gerätetest „macOS mit Samba“ aus Runde 3 gilt entsprechend: beide Häkchen abschalten.

### Stichproben im Handbuch (gegen Code/Profile geprüft, alle zutreffend)

1. Port-Weiterleitungen nur auf 127.0.0.1 (`hostfwd=…:127.0.0.1:…`).
2. VNC nur lokal ab 5900 (`-vnc 127.0.0.1:0,to=99`).
3. Wegwerf-Modus = `-snapshot`.
4. CPU „host“ ohne Beschleuniger → automatisch passendes Modell (`argv.rs`, Zeile mit `cpu == "host" && accel.is_none()`).
5. Secure Boot mit HVF/WHPX → normales UEFI mit Log-Hinweis (`secure_boot_active`, `host_notes`).
6. NVRAM als `nvram.fd` im Maschinenordner.
7. Disketten schreibgeschützt eingelegt (`if=floppy,…,readonly=on`).
8. Konsolen: `saves/`, `states/`, `screenshots/` im Maschinenordner; Netplay und Achievements aus (`netplay_enable`/`cheevos_enable = false`).
9. BIOS-Listen der Profile (PSX, Saturn, Dreamcast, NDS, Neo Geo; Mega-CD/PC Engine ohne Pflicht-BIOS) und Geräte der PC-Profile (SB16+AdLib, XP: AC97, NE2000/RTL8139, Diskette).
10. Nacktes „D:“ wird unter Windows zu „D:\“ (`normalize_user_path`), Cores-Ordner im RetroArch.app-Bundle wird gesucht.

Nebenbefund ohne Änderung: Das Profil „Power Mac G3“ nutzt `mac99` mit CPU-Modell `g4`; das Handbuch
nennt nur „startet mit OpenBIOS ohne ROM“, was zutrifft.

### Weiterhin offen

- Alles aus „Nur am Gerät prüfbar“ der Runden 1–3 gilt unverändert (WHPX/TCG + `-icount` unter Windows,
  Windows-95-Start, Firewall-Abfrage für RetroArch, Beenden/Update während eines langen Offline-Snapshots,
  SMB mit Samba, MSI/NSIS, SDL).
- Windows-QMP über TCP ohne Anmeldung (Startfenster), RetroArch-Kommandoport im LAN, `zip` im Updater — wie Runde 3.
