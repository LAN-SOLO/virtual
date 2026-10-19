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
- Fehlermeldungen des Backends sind deutsch, auch bei englischer Oberfläche.
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
