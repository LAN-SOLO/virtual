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

Status: in Planung — Website: https://lan-solo.com/de/tools/virtual/
