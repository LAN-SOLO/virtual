//! Maschine → QEMU-Kommandozeile als reine Funktion. Alles, was vom Host
//! abhängt (Beschleuniger, Firmware-Pfade, Sockets), kommt über `HostContext`
//! herein — damit ist jedes Profil hier deterministisch testbar.

use crate::model::*;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct HostContext {
    /// Ordner der Maschine (für relative Platten-/Medienpfade).
    pub machine_dir: PathBuf,
    /// Verfügbarer Beschleuniger für Gast-Arch = Host-Arch (`hvf`/`whpx`/`kvm`), sonst None.
    pub accel: Option<String>,
    /// QMP-Endpunkt: Unix-Socket-Pfad (macOS/Linux) oder `tcp:127.0.0.1:PORT` (Windows).
    pub qmp: String,
    /// UEFI-Firmware (Code) für x86_64 bzw. aarch64, falls gefunden.
    pub uefi_code: Option<PathBuf>,
    /// Beschreibbare NVRAM-Kopie im Maschinenordner.
    pub uefi_vars: Option<PathBuf>,
    /// swtpm-Socket, falls TPM gewünscht und swtpm vorhanden.
    pub tpm_socket: Option<PathBuf>,
    /// Anzeige-Backend des Hosts: `cocoa` | `gtk` | `sdl`
    pub display_backend: String,
    /// Audio-Backend des Hosts: `coreaudio` | `dsound` | `pa` | `pipewire` | `none`
    pub audio_backend: String,
    /// Host kann den Prozessnamen setzen (`-name …,process=…`) — nur Linux; macOS und Windows brechen sonst ab.
    pub process_names: bool,
    /// QEMU-Build kann OpenGL-Anzeige (`gl=on`, `virtio-vga-gl`) — Homebrew/cocoa nicht.
    pub gl: bool,
    /// SLIRP-SMB-Freigabe nutzbar (Samba-`smbd` vorhanden; unter Windows nie).
    pub smb: bool,
    /// virtio-9p (`-fsdev`) verfügbar — nicht in QEMU für Windows.
    pub virtfs: bool,
}

/// Wert für eine komma-getrennte QEMU-Option: Kommas werden verdoppelt, damit ein Pfad
/// oder Name wie „Win 3.11, DOS“ keine weiteren Optionen einschleusen oder den Start brechen kann.
pub fn qv(value: &str) -> String {
    value.replace(',', ",,")
}

/// Grafikgerät, wie es auf diesem Host tatsächlich nutzbar ist (ohne GL fällt `-gl` weg).
fn effective_graphics(m: &Machine, ctx: &HostContext) -> String {
    let gfx = if m.devices.graphics.is_empty() { "std" } else { m.devices.graphics.as_str() };
    let mut d = gfx.to_string();
    if ctx.gl {
        if m.display.gl && !d.ends_with("-gl") && (d == "virtio-vga" || d == "virtio-gpu-pci") {
            d = if d == "virtio-vga" { "virtio-vga-gl".into() } else { "virtio-gpu-gl-pci".into() };
        }
    } else {
        d = match d.as_str() {
            "virtio-vga-gl" => "virtio-vga".into(),
            "virtio-gpu-gl-pci" => "virtio-gpu-pci".into(),
            _ => d,
        };
    }
    d
}

/// Secure-Boot-Firmware ist nur auf q35 mit TCG oder KVM verlässlich (SMM nötig).
pub fn secure_boot_active(m: &Machine, ctx: &HostContext) -> bool {
    m.firmware == Firmware::UefiSecure
        && m.machine_type == "q35"
        && matches!(m.arch, Arch::X86_64 | Arch::I386)
        && matches!(ctx.accel.as_deref(), None | Some("kvm"))
}

/// Ordner für die SLIRP-SMB-Freigabe: nur der erste, nur mit Netz und Samba — und nie,
/// wenn er schreibgeschützt sein soll. QEMU schreibt `read only=no` in die smb.conf;
/// eine schreibgeschützte SMB-Freigabe kann es nicht. Die Freigabe entfällt dann (Hinweis im Log).
fn smb_folder<'a>(m: &'a Machine, ctx: &HostContext) -> Option<&'a SharedFolder> {
    if m.network.mode == NetworkMode::Off || !ctx.smb {
        return None;
    }
    m.shared_folders.first().filter(|s| !(s.read_only || m.isolation.read_only_shares))
}

/// Hinweise zu Einstellungen, die dieser Host nicht umsetzen kann — der Start
/// protokolliert sie, statt still etwas anderes zu tun.
pub fn host_notes(m: &Machine, ctx: &HostContext) -> Vec<String> {
    let mut n = Vec::new();
    if m.engine != Engine::Qemu {
        return n;
    }
    if m.tpm && ctx.tpm_socket.is_none() {
        n.push("TPM 2.0: swtpm nicht gefunden — die Maschine startet ohne TPM.".into());
    }
    if m.firmware == Firmware::UefiSecure && !secure_boot_active(m, ctx) {
        n.push("Secure Boot: mit diesem Beschleuniger bzw. Board nicht verfügbar — Start mit UEFI ohne Secure Boot.".into());
    }
    if (m.display.gl || m.devices.graphics.ends_with("-gl")) && !ctx.gl {
        n.push("OpenGL: von diesem QEMU-Build nicht unterstützt — Anzeige ohne GL.".into());
    }
    if !m.shared_folders.is_empty() {
        let ninep = m.devices.storage_bus == "virtio" && ctx.virtfs;
        let smb = smb_folder(m, ctx).is_some();
        if m.devices.storage_bus == "virtio" && !ctx.virtfs {
            n.push("Gemeinsame Ordner: virtio-9p gibt es in QEMU für Windows nicht.".into());
        }
        if m.network.mode != NetworkMode::Off && !ctx.smb {
            n.push("Gemeinsame Ordner: SMB-Freigabe übersprungen — Samba (smbd) fehlt bzw. Windows-Host.".into());
        } else if m.network.mode != NetworkMode::Off && !smb && !ninep {
            n.push("Gemeinsame Ordner: SMB-Freigaben sind immer beschreibbar — der Ordner ist schreibgeschützt eingestellt und wird nicht freigegeben (zum Freigeben unter Hardware „Gemeinsame Ordner schreibgeschützt“ und am Ordner „nur lesen“ abschalten).".into());
        } else if m.network.mode != NetworkMode::Off && !smb {
            // 9p hängt den Ordner schreibgeschützt ein — Gäste ohne 9p (Windows) sehen ihn aber nicht mehr
            n.push("Gemeinsame Ordner: keine SMB-Freigabe für den schreibgeschützten Ordner (SMB-Freigaben sind immer beschreibbar) — virtio-9p hängt ihn schreibgeschützt ein; Gäste ohne 9p (z. B. Windows) sehen ihn nicht.".into());
        }
        if !ninep && !smb {
            n.push("Gemeinsame Ordner sind für diese Maschine nicht eingehängt (virtio-9p braucht den virtio-Bus, SMB ein Netz).".into());
        }
        if smb && m.shared_folders.len() > 1 {
            n.push("SMB-Freigabe: nur der erste Ordner (SLIRP kann eine Freigabe).".into());
        }
    }
    n
}

/// Maschinen, deren Grafik/Netz/Audio fest verdrahtet sind — kein `-device`.
fn integrated_board(machine_type: &str) -> bool {
    matches!(machine_type, "q800" | "mac99" | "g3beige" | "next-cube" | "malta")
}

/// Pfad relativ zum Maschinenordner auflösen — bereits für QEMU-Optionen maskiert (Kommas).
fn resolve(dir: &Path, p: &str) -> String {
    let path = Path::new(p);
    let full = if path.is_absolute() { p.to_string() } else { dir.join(path).to_string_lossy().into_owned() };
    qv(&full)
}

/// Bootgerät der Maschine: `auto` | `disk` | `cdrom` | `floppy` (leer/unbekannt = auto).
pub fn boot_device(m: &Machine) -> String {
    match m.boot.as_str() {
        b @ ("disk" | "cdrom" | "floppy") => b.to_string(),
        _ => "auto".to_string(),
    }
}

/// `-icount shift=N`: 100 % = aus; 75 % ≈ shift 1, 60 % ≈ 2, ≤ 40 % = 3.
pub fn icount_shift(clock_percent: u32) -> Option<u32> {
    match clock_percent {
        p if p >= 100 => None,
        p if p >= 70 => Some(1),
        p if p >= 50 => Some(2),
        _ => Some(3),
    }
}

/// Baut die Argumentliste (ohne das Binary selbst).
pub fn qemu_argv(m: &Machine, ctx: &HostContext) -> Result<Vec<String>, String> {
    if m.engine != Engine::Qemu {
        return Err("Keine QEMU-Maschine".into());
    }
    let mut a: Vec<String> = Vec::new();
    let push = |a: &mut Vec<String>, s: &str| a.push(s.to_string());

    // --- Maschine, CPU, Speicher ------------------------------------------
    let mut machine = m.machine_type.clone();
    if m.machine_type == "mac99" {
        machine.push_str(",via=pmu");
    }
    if m.tpm && ctx.tpm_socket.is_some() && m.machine_type == "q35" {
        // TPM-Gerät kommt unten; q35 braucht nichts Extra am Board.
    }
    if let Some(accel) = ctx.accel.as_deref() {
        machine.push_str(&format!(",accel={accel}"));
        if accel == "whpx" {
            // WHPX ohne Kernel-irqchip: sonst scheitern viele Gäste an der Interrupt-Zustellung
            machine.push_str(",kernel-irqchip=off");
        }
    } else {
        machine.push_str(",accel=tcg");
    }
    if secure_boot_active(m, ctx) {
        machine.push_str(",smm=on");
    }
    push(&mut a, "-M");
    a.push(machine);

    let mut cpu = m.cpu.model.clone();
    if cpu.is_empty() || (cpu == "host" && ctx.accel.is_none()) {
        // Ohne Beschleuniger gibt es kein `host`-Modell — passendes Standardmodell.
        cpu = match m.arch {
            Arch::X86_64 | Arch::I386 => "max".into(),
            Arch::Aarch64 => "cortex-a72".into(),
            Arch::M68k => "m68040".into(),
            Arch::Ppc => "g4".into(),
            Arch::Riscv64 => "rv64".into(),
            Arch::Mips => "24Kf".into(),
            Arch::None => String::new(),
        };
    }
    if !cpu.is_empty() {
        push(&mut a, "-cpu");
        a.push(cpu);
    }
    push(&mut a, "-smp");
    a.push(m.cpu.cores.max(1).to_string());
    push(&mut a, "-m");
    a.push(format!("{}M", m.memory_mb.max(1)));

    // Taktbremse nur ohne Beschleuniger sinnvoll (icount ist TCG-only).
    if ctx.accel.is_none() {
        if let Some(shift) = icount_shift(m.cpu.clock_percent) {
            push(&mut a, "-icount");
            a.push(format!("shift={shift},align=off,sleep=on"));
        }
    }

    // --- Firmware -----------------------------------------------------------
    match m.firmware {
        Firmware::Bios => {}
        Firmware::Uefi | Firmware::UefiSecure => {
            let code = ctx
                .uefi_code
                .as_ref()
                .ok_or("UEFI-Firmware (OVMF/AAVMF) nicht gefunden — im QEMU-Paket enthalten?")?;
            if secure_boot_active(m, ctx) {
                push(&mut a, "-global");
                push(&mut a, "driver=cfi.pflash01,property=secure,value=on");
            }
            push(&mut a, "-drive");
            a.push(format!("if=pflash,format=raw,readonly=on,file={}", qv(&code.to_string_lossy())));
            if let Some(vars) = &ctx.uefi_vars {
                push(&mut a, "-drive");
                a.push(format!("if=pflash,format=raw,file={}", qv(&vars.to_string_lossy())));
            }
        }
    }
    if m.tpm {
        if let Some(sock) = &ctx.tpm_socket {
            push(&mut a, "-chardev");
            a.push(format!("socket,id=chrtpm,path={}", qv(&sock.to_string_lossy())));
            push(&mut a, "-tpmdev");
            push(&mut a, "emulator,id=tpm0,chardev=chrtpm");
            push(&mut a, "-device");
            let dev = if m.arch == Arch::Aarch64 { "tpm-tis-device,tpmdev=tpm0" } else { "tpm-tis,tpmdev=tpm0" };
            push(&mut a, dev);
        }
    }

    // --- Platten ------------------------------------------------------------
    let bus = m.devices.storage_bus.as_str();
    if bus == "scsi" && !integrated_board(&m.machine_type) {
        push(&mut a, "-device");
        push(&mut a, "virtio-scsi-pci,id=scsi0");
    }
    // Boot-Reihenfolge über bootindex: die gewünschte Klasse bekommt die kleinen Indizes.
    // `auto`: CD zuerst (wenn eingelegt), dann Platte, dann Diskette. SeaBIOS/OVMF probieren
    // ungelistete Geräte danach — mit `strict=on` (ausdrückliche Wahl) halten sie stattdessen an.
    // Ausdrückliche Wahl: nur diese Klasse bekommt einen bootindex, alles andere keinen —
    // zusammen mit `strict=on` hält die Firmware dann an, statt auf die CD zurückzufallen.
    let has_cd = m.media.iter().any(|x| x.kind == MediaKind::Cdrom);
    let (disk_base, cd_base, floppy_base): (Option<usize>, Option<usize>, Option<usize>) = match boot_device(m).as_str() {
        "disk" => (Some(1), None, None),
        "cdrom" => (None, Some(1), None),
        "floppy" => (None, None, Some(1)),
        _ if has_cd => (Some(10), Some(1), Some(20)),
        _ => (Some(1), Some(10), Some(20)),
    };
    let bi = |base: Option<usize>, n: usize| base.map(|b| format!(",bootindex={}", b + n)).unwrap_or_default();
    // Klassische PC-Boards (pc, isapc): IDE-Geräte einzeln, damit sie einen bootindex tragen.
    let pc_ide = bus == "ide" && !integrated_board(&m.machine_type);
    for (i, d) in m.disks.iter().enumerate() {
        let file = resolve(&ctx.machine_dir, &d.path);
        match bus {
            "ide" if pc_ide => {
                push(&mut a, "-drive");
                a.push(format!("file={file},format={},if=none,id=disk{i}", d.format));
                push(&mut a, "-device");
                a.push(format!("ide-hd,drive=disk{i},bus=ide.{},unit={}{}", i / 2, i % 2, bi(disk_base, i)));
            }
            "virtio" => {
                push(&mut a, "-drive");
                a.push(format!("file={file},format={},if=none,id=disk{i},cache=writeback,discard=unmap", d.format));
                push(&mut a, "-device");
                a.push(format!("virtio-blk-pci,drive=disk{i}{}", bi(disk_base, i)));
            }
            "sata" => {
                push(&mut a, "-drive");
                a.push(format!("file={file},format={},if=none,id=disk{i}", d.format));
                push(&mut a, "-device");
                a.push(format!("ide-hd,drive=disk{i},bus=ide.{i}{}", bi(disk_base, i)));
            }
            "scsi" => {
                push(&mut a, "-drive");
                a.push(format!("file={file},format={},if=scsi,index={i},media=disk", d.format));
            }
            _ => {
                // ide: klassisch, ohne -device — bleibt auch bei isapc/q800-losen Boards gültig
                push(&mut a, "-drive");
                a.push(format!("file={file},format={},if=ide,index={i},media=disk", d.format));
            }
        }
    }

    // --- Medien -------------------------------------------------------------
    let mut cd_index = m.disks.len().max(2); // CD hinter den Platten (IDE: index 2 = Secondary Master)
    for med in &m.media {
        let file = resolve(&ctx.machine_dir, &med.path);
        match med.kind {
            MediaKind::Cdrom => {
                push(&mut a, "-drive");
                let ifc = match bus {
                    "virtio" | "sata" => "none".to_string(),
                    "ide" if pc_ide => "none".to_string(),
                    "scsi" => "scsi".to_string(),
                    _ => "ide".to_string(),
                };
                if ifc == "none" {
                    a.push(format!("file={file},format=raw,if=none,id=cd{},media=cdrom,readonly=on", med.slot));
                    push(&mut a, "-device");
                    if pc_ide {
                        // Sekundärer IDE-Kanal, wie beim echten PC der Zeit
                        a.push(format!("ide-cd,drive=cd{},bus=ide.1,unit={}{}", med.slot, med.slot.min(1), bi(cd_base, med.slot as usize)));
                    } else {
                        a.push(format!("ide-cd,drive=cd{}{}", med.slot, bi(cd_base, med.slot as usize)));
                    }
                } else {
                    a.push(format!("file={file},format=raw,if={ifc},index={cd_index},media=cdrom,readonly=on"));
                    cd_index += 1;
                }
            }
            MediaKind::Floppy => {
                // Explizit raw (sonst warnt QEMU und sperrt Block 0) und schreibgeschützt:
                // beschreibbare Disketten-Images kennen keine internen Snapshots und
                // würden `savevm` für die ganze Maschine blockieren.
                let unit = med.slot.min(1);
                push(&mut a, "-drive");
                a.push(format!("if=floppy,index={unit},format=raw,readonly=on,file={file}"));
                if pc_ide {
                    // Der Board-Controller trägt den bootindex der Diskette (A/B)
                    if let Some(b) = floppy_base {
                        push(&mut a, "-global");
                        a.push(format!("isa-fdc.bootindex{}={}", if unit == 0 { "A" } else { "B" }, b + unit as usize));
                    }
                }
            }
            MediaKind::Rom => {
                push(&mut a, "-bios");
                a.push(file);
            }
            MediaKind::Physical => {
                push(&mut a, "-drive");
                a.push(format!("file={file},format=raw,if={},media=cdrom,readonly=on", if bus == "virtio" { "ide" } else { bus }));
            }
        }
    }
    if m.devices.floppy && !m.media.iter().any(|x| x.kind == MediaKind::Floppy) && !integrated_board(&m.machine_type) {
        // Leeres Laufwerk, damit der Gast eines sieht (Disketten später einlegen).
        push(&mut a, "-drive");
        push(&mut a, "if=floppy,index=0,media=disk,format=raw,readonly=on,file.driver=null-co,file.read-zeroes=on,file.size=1474560");
    }
    if !m.media.iter().any(|x| x.kind == MediaKind::Cdrom) && m.disks.is_empty() {
        // Nichts zum Booten — QEMU soll wenigstens sauber starten
    }
    // Boot: Reihenfolge steckt in den bootindex-Werten; F12-Menü immer, ausdrückliche Wahl strikt
    // (sonst fällt SeaBIOS nach den gelisteten Geräten auf alle übrigen zurück).
    if !integrated_board(&m.machine_type) {
        push(&mut a, "-boot");
        if bus == "scsi" {
            // Legacy-SCSI ohne bootindex: klassische Reihenfolge
            let order = match boot_device(m).as_str() {
                "disk" => "c",
                "cdrom" => "dc",
                "floppy" => "ac",
                _ if has_cd => "dc",
                _ => "c",
            };
            a.push(format!("order={order},menu=on"));
        } else if boot_device(m) != "auto" {
            push(&mut a, "menu=on,strict=on");
        } else {
            push(&mut a, "menu=on");
        }
    }

    // --- Grafik, Eingabe, Audio --------------------------------------------
    if !integrated_board(&m.machine_type) {
        let gfx = effective_graphics(m, ctx);
        match gfx.as_str() {
            "std" | "VGA" => { push(&mut a, "-vga"); push(&mut a, "std"); }
            "cirrus-vga" => { push(&mut a, "-vga"); push(&mut a, "cirrus"); }
            "none" => { push(&mut a, "-vga"); push(&mut a, "none"); }
            dev => {
                push(&mut a, "-vga"); push(&mut a, "none");
                push(&mut a, "-device");
                a.push(dev.to_string());
            }
        }
        match m.devices.input.as_str() {
            "usb" => { push(&mut a, "-device"); push(&mut a, "qemu-xhci,id=xhci"); push(&mut a, "-device"); push(&mut a, "usb-tablet"); push(&mut a, "-device"); push(&mut a, "usb-kbd"); }
            "virtio" => { push(&mut a, "-device"); push(&mut a, "virtio-keyboard-pci"); push(&mut a, "-device"); push(&mut a, "virtio-tablet-pci"); }
            _ => {} // ps2 ist am PC-Board eingebaut
        }
        if !m.devices.audio.is_empty() && ctx.audio_backend != "none" {
            push(&mut a, "-audiodev");
            a.push(format!("{},id=snd0", ctx.audio_backend));
            for dev in &m.devices.audio {
                match dev.as_str() {
                    // HD-Audio ist nur der Controller — hören lässt er sich erst mit einem Codec
                    "intel-hda" | "ich9-intel-hda" => {
                        push(&mut a, "-device");
                        a.push(dev.clone());
                        push(&mut a, "-device");
                        push(&mut a, "hda-duplex,audiodev=snd0");
                    }
                    _ => {
                        push(&mut a, "-device");
                        a.push(format!("{dev},audiodev=snd0"));
                    }
                }
            }
        }
    } else if !m.devices.audio.is_empty() && ctx.audio_backend != "none" {
        // Eingebaute Audio-Hardware (z. B. Screamer beim mac99) will nur ein Backend
        push(&mut a, "-audiodev");
        a.push(format!("{},id=snd0", ctx.audio_backend));
    }

    // --- Netzwerk -----------------------------------------------------------
    match m.network.mode {
        NetworkMode::Off => { push(&mut a, "-nic"); push(&mut a, "none"); }
        mode => {
            let mut net = String::from("user,id=net0");
            if mode == NetworkMode::Isolated { net.push_str(",restrict=on"); }
            for f in &m.network.forwards {
                if f.host_port == 0 || f.guest_port == 0 {
                    continue;
                }
                let proto = if f.proto == "udp" { "udp" } else { "tcp" };
                // Nur an localhost binden: eine Weiterleitung öffnet den Gast nie ins LAN
                net.push_str(&format!(",hostfwd={proto}:127.0.0.1:{}-:{}", f.host_port, f.guest_port));
            }
            if let Some(s) = smb_folder(m, ctx) {
                // SMB-Freigabe über SLIRP: für alte Gäste ohne virtio-9p — nur eine (SLIRP kann eine)
                net.push_str(&format!(",smb={}", qv(&s.host_path)));
            }
            push(&mut a, "-netdev");
            a.push(net);
            if integrated_board(&m.machine_type) {
                push(&mut a, "-nic"); // Board-NIC an netdev hängen
                push(&mut a, "user,model=none");
            } else {
                push(&mut a, "-device");
                let nic = if m.devices.nic.is_empty() { "e1000" } else { m.devices.nic.as_str() };
                a.push(format!("{nic},netdev=net0"));
            }
        }
    }

    // --- Gemeinsame Ordner (virtio-9p; virtiofs folgt in 0.2) -------------
    if m.devices.storage_bus == "virtio" && ctx.virtfs {
        for (i, s) in m.shared_folders.iter().enumerate() {
            let ro = s.read_only || m.isolation.read_only_shares;
            push(&mut a, "-fsdev");
            a.push(format!(
                "local,id=fs{i},path={},security_model=mapped-xattr{}",
                qv(&s.host_path),
                if ro { ",readonly=on" } else { "" }
            ));
            let tag: String = s.name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').take(31).collect();
            let tag = if tag.is_empty() { format!("share{i}") } else { tag };
            push(&mut a, "-device");
            a.push(format!("virtio-9p-pci,fsdev=fs{i},mount_tag={tag}"));
        }
    }

    // --- Anzeige, Steuerung, Isolation ------------------------------------
    match m.display.kind.as_str() {
        "headless" => { push(&mut a, "-display"); push(&mut a, "none"); }
        // Nur localhost; ab Port 5900 die nächste freie Anzeige (mehrere Maschinen gleichzeitig)
        "vnc" => { push(&mut a, "-display"); push(&mut a, "none"); push(&mut a, "-vnc"); push(&mut a, "127.0.0.1:0,to=99"); }
        _ => {
            push(&mut a, "-display");
            let mut d = ctx.display_backend.clone();
            if m.display.gl && ctx.gl && matches!(ctx.display_backend.as_str(), "gtk" | "sdl") { d.push_str(",gl=on"); }
            if m.display.fullscreen { d.push_str(",full-screen=on"); }
            a.push(d);
        }
    }
    push(&mut a, "-name");
    if ctx.process_names {
        a.push(format!("{},process=virtual-{}", qv(&m.name), qv(&m.id)));
    } else {
        a.push(qv(&m.name));
    }
    push(&mut a, "-qmp");
    if ctx.qmp.starts_with("tcp:") {
        a.push(format!("{},server=on,wait=off", ctx.qmp));
    } else {
        a.push(format!("unix:{},server=on,wait=off", qv(&ctx.qmp)));
    }
    push(&mut a, "-rtc");
    push(&mut a, "base=localtime,clock=host");
    if m.isolation.throwaway {
        push(&mut a, "-snapshot");
    }
    push(&mut a, "-no-user-config");
    for x in &m.extra_args {
        a.push(x.clone());
    }
    Ok(a)
}

/// Kommandozeile für `qemu-img create`.
pub fn qemu_img_create(path: &Path, format: &str, size_gb: u32) -> Vec<String> {
    vec![
        "create".into(),
        "-f".into(),
        format.into(),
        path.to_string_lossy().into_owned(),
        format!("{}G", size_gb.max(1)),
    ]
}

/// Menschenlesbare Darstellung (Shell-Quoting nur wo nötig).
pub fn render(binary: &str, argv: &[String]) -> String {
    let mut out = String::from(binary);
    for a in argv {
        out.push(' ');
        if a.contains(' ') || a.contains('"') {
            out.push('\'');
            out.push_str(&a.replace('\'', "'\\''"));
            out.push('\'');
        } else {
            out.push_str(a);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::builtin_profiles;

    fn ctx(accel: Option<&str>) -> HostContext {
        HostContext {
            machine_dir: PathBuf::from("/vm/test"),
            accel: accel.map(String::from),
            qmp: "/vm/test/qmp.sock".into(),
            uefi_code: Some(PathBuf::from("/fw/OVMF_CODE.fd")),
            uefi_vars: Some(PathBuf::from("/vm/test/nvram.fd")),
            tpm_socket: Some(PathBuf::from("/vm/test/tpm/sock")),
            display_backend: "cocoa".into(),
            audio_backend: "coreaudio".into(),
            process_names: false,
            gl: false,
            smb: true,
            virtfs: true,
        }
    }

    fn machine(profile: &str) -> Machine {
        let p = builtin_profiles().into_iter().find(|p| p.id == profile).expect("Profil");
        let mut m = p.instantiate("m1", "Test", "2026-09-10T00:00:00Z");
        if p.default_disk_gb > 0 {
            m.disks.push(DiskRef { id: "d1".into(), size_gb: p.default_disk_gb, ..DiskRef::default() });
        }
        m
    }

    /// Pfadtrenner plattformneutral (Windows baut `\vm\test`).
    fn norm(a: &[String]) -> Vec<String> {
        a.iter().map(|x| x.replace('\\', "/")).collect()
    }

    fn has_pair(a: &[String], flag: &str, value: &str) -> bool {
        a.windows(2).any(|w| w[0] == flag && w[1] == value)
    }

    #[test]
    fn modern_uefi_uses_accel_virtio_and_tpm() {
        let mut m = machine("modern-x86-uefi");
        m.media.push(MediaRef { id: "c".into(), kind: MediaKind::Cdrom, path: "/iso/win11.iso".into(), slot: 0 });
        let a = qemu_argv(&m, &ctx(Some("hvf"))).unwrap();
        assert!(has_pair(&a, "-M", "q35,accel=hvf"));
        assert!(has_pair(&a, "-cpu", "host"));
        assert!(a.iter().any(|x| x.starts_with("if=pflash") && x.contains("OVMF_CODE")));
        assert!(a.iter().any(|x| x.starts_with("virtio-blk-pci,drive=disk0")));
        assert!(a.iter().any(|x| x.contains("/iso/win11.iso") && x.contains("media=cdrom")));
        assert!(a.iter().any(|x| x == "tpm-tis,tpmdev=tpm0"));
        assert!(has_pair(&a, "-device", "intel-hda") && has_pair(&a, "-device", "hda-duplex,audiodev=snd0"), "HDA braucht einen Codec");
        assert!(!a.iter().any(|x| x == "intel-hda,audiodev=snd0"), "intel-hda kennt kein audiodev");
        assert!(has_pair(&a, "-netdev", "user,id=net0"));
        assert!(!a.contains(&"-icount".to_string()), "kein icount mit Beschleuniger");
        assert!(!a.contains(&"-snapshot".to_string()));
    }

    #[test]
    fn retro_pc_is_isolated_throttled_and_has_floppy() {
        let m = machine("pc-1996-pentium133");
        let a = qemu_argv(&m, &ctx(None)).unwrap();
        let a = norm(&a);
        assert!(has_pair(&a, "-M", "pc,accel=tcg"));
        assert!(has_pair(&a, "-cpu", "pentium"));
        assert!(has_pair(&a, "-icount", "shift=1,align=off,sleep=on"));
        assert!(has_pair(&a, "-nic", "none"), "Retro-Profil ohne Netz");
        assert!(has_pair(&a, "-vga", "cirrus"));
        assert!(a.iter().any(|x| x == "sb16,audiodev=snd0"));
        assert!(a.iter().any(|x| x == "adlib,audiodev=snd0"));
        assert!(a.iter().any(|x| x.starts_with("if=floppy") && x.contains("readonly=on")), "leeres Diskettenlaufwerk, snapshot-fähig");
        assert!(a.iter().any(|x| x.contains("/vm/test/disk-0.qcow2") && x.contains("if=none,id=disk0")));
        assert!(a.iter().any(|x| x == "ide-hd,drive=disk0,bus=ide.0,unit=0,bootindex=1"), "Platte am primären IDE-Kanal mit bootindex");
        assert!(has_pair(&a, "-boot", "menu=on"));
    }

    #[test]
    fn throwaway_adds_snapshot_flag_and_isolated_restricts() {
        let mut m = machine("pc-1999-pentium3");
        m.isolation.throwaway = true;
        m.network.mode = NetworkMode::Isolated;
        m.network.forwards.push(PortForward { proto: "tcp".into(), host_port: 2222, guest_port: 22 });
        let a = qemu_argv(&m, &ctx(None)).unwrap();
        assert!(a.contains(&"-snapshot".to_string()));
        assert!(has_pair(&a, "-netdev", "user,id=net0,restrict=on,hostfwd=tcp:127.0.0.1:2222-:22"));
        assert!(a.iter().any(|x| x == "rtl8139,netdev=net0"));
    }

    #[test]
    fn quadra_uses_rom_and_no_device_flags() {
        let mut m = machine("mac-1994-quadra");
        m.media.push(MediaRef { id: "r".into(), kind: MediaKind::Rom, path: "Q800.ROM".into(), slot: 0 });
        let a = qemu_argv(&m, &ctx(None)).unwrap();
        let a = norm(&a);
        assert!(has_pair(&a, "-M", "q800,accel=tcg"));
        assert!(has_pair(&a, "-bios", "/vm/test/Q800.ROM"));
        assert!(!a.iter().any(|x| x.contains("nubus-macfb")), "Board-Grafik nicht als -device");
        assert!(a.iter().any(|x| x.contains("if=scsi") && x.contains("media=disk")));
    }

    #[test]
    fn uefi_without_firmware_is_an_error() {
        let m = machine("modern-x86-uefi");
        let mut c = ctx(Some("kvm"));
        c.uefi_code = None;
        assert!(qemu_argv(&m, &c).is_err());
    }

    #[test]
    fn every_qemu_profile_builds() {
        for p in builtin_profiles().into_iter().filter(|p| p.engine == Engine::Qemu) {
            let m = machine(&p.id);
            let a = qemu_argv(&m, &ctx(None)).unwrap_or_else(|e| panic!("{}: {e}", p.id));
            assert!(a.iter().any(|x| x.starts_with("unix:/vm/test/qmp.sock")));
            assert!(a.contains(&"-no-user-config".to_string()));
        }
    }

    #[test]
    fn boot_device_controls_order_and_bootindex() {
        // klassisches PC-Board: IDE-Geräte einzeln mit bootindex
        let mut m = machine("pc-1996-pentium133");
        m.media.push(MediaRef { id: "c".into(), kind: MediaKind::Cdrom, path: "win98.iso".into(), slot: 0 });
        m.media.push(MediaRef { id: "f".into(), kind: MediaKind::Floppy, path: "boot.img".into(), slot: 0 });
        let a = qemu_argv(&m, &ctx(None)).unwrap();
        assert!(a.iter().any(|x| x == "ide-cd,drive=cd0,bus=ide.1,unit=0,bootindex=1"), "auto + CD → CD zuerst");
        assert!(a.iter().any(|x| x == "ide-hd,drive=disk0,bus=ide.0,unit=0,bootindex=10"));
        assert!(has_pair(&a, "-global", "isa-fdc.bootindexA=20"));
        assert!(has_pair(&a, "-boot", "menu=on"), "auto: nicht strikt");
        m.boot = "disk".into();
        let a = qemu_argv(&m, &ctx(None)).unwrap();
        assert!(a.iter().any(|x| x == "ide-hd,drive=disk0,bus=ide.0,unit=0,bootindex=1"), "nach der Installation: Platte");
        assert!(a.iter().any(|x| x == "ide-cd,drive=cd0,bus=ide.1,unit=0"), "CD ohne bootindex → wird bei strict übersprungen");
        assert!(!a.iter().any(|x| x.starts_with("isa-fdc.bootindex")));
        assert!(has_pair(&a, "-boot", "menu=on,strict=on"), "ausdrückliche Wahl: strikt");
        m.boot = "floppy".into();
        let a = qemu_argv(&m, &ctx(None)).unwrap();
        assert!(has_pair(&a, "-global", "isa-fdc.bootindexA=1"));
        assert!(a.iter().any(|x| x == "ide-hd,drive=disk0,bus=ide.0,unit=0"));
        m.boot = "unsinn".into();
        assert!(has_pair(&qemu_argv(&m, &ctx(None)).unwrap(), "-boot", "menu=on"), "unbekannt = auto");
        // Legacy-SCSI-Board ohne Integration bleibt bei -boot order=
        let mut q = machine("pc-1999-pentium3");
        q.devices.storage_bus = "scsi".into();
        q.boot = "cdrom".into();
        assert!(has_pair(&qemu_argv(&q, &ctx(None)).unwrap(), "-boot", "order=dc,menu=on"));
        // bootindex-Board
        let mut q = machine("modern-x86-uefi");
        q.media.push(MediaRef { id: "c".into(), kind: MediaKind::Cdrom, path: "/iso/x.iso".into(), slot: 0 });
        let a = qemu_argv(&q, &ctx(Some("hvf"))).unwrap();
        assert!(a.iter().any(|x| x.starts_with("virtio-blk-pci") && x.ends_with("bootindex=10")), "auto + CD: Platte nach der CD");
        assert!(a.iter().any(|x| x.starts_with("ide-cd") && x.ends_with("bootindex=1")));
        assert!(has_pair(&a, "-boot", "menu=on"));
        q.boot = "disk".into();
        let a = qemu_argv(&q, &ctx(Some("hvf"))).unwrap();
        assert!(a.iter().any(|x| x.starts_with("virtio-blk-pci") && x.ends_with("bootindex=1")));
        assert!(a.iter().any(|x| x == "ide-cd,drive=cd0"), "nur die Platte ist gelistet");
        assert!(has_pair(&a, "-boot", "menu=on,strict=on"));
    }

    #[test]
    fn floppy_is_raw_and_read_only() {
        let mut m = machine("pc-1996-pentium133");
        m.media.push(MediaRef { id: "f".into(), kind: MediaKind::Floppy, path: "boot.img".into(), slot: 0 });
        let a = qemu_argv(&m, &ctx(None)).unwrap();
        let a = norm(&a);
        assert!(has_pair(&a, "-drive", "if=floppy,index=0,format=raw,readonly=on,file=/vm/test/boot.img"));
        assert!(has_pair(&a, "-global", "isa-fdc.bootindexA=20"));
        assert!(!a.contains(&"-fda".to_string()));
        assert_eq!(a.iter().filter(|x| x.starts_with("if=floppy")).count(), 1, "kein zweites leeres Laufwerk");
    }

    #[test]
    fn process_name_only_where_supported() {
        let m = machine("pc-1996-pentium133");
        let a = qemu_argv(&m, &ctx(None)).unwrap();
        assert!(has_pair(&a, "-name", "Test"), "macOS: kein process=");
        let mut c = ctx(None);
        c.process_names = true;
        let a = qemu_argv(&m, &c).unwrap();
        assert!(has_pair(&a, "-name", "Test,process=virtual-m1"));
    }

    #[test]
    fn commas_in_names_and_paths_are_escaped() {
        let mut m = machine("pc-1996-pentium133");
        m.name = "Win 3.11, DOS".into();
        m.media.push(MediaRef { id: "c".into(), kind: MediaKind::Cdrom, path: "/iso/a,b.iso".into(), slot: 0 });
        let a = norm(&qemu_argv(&m, &ctx(None)).unwrap());
        assert!(has_pair(&a, "-name", "Win 3.11,, DOS"));
        assert!(a.iter().any(|x| x.contains("file=/iso/a,,b.iso,format=raw")), "{a:?}");
        let mut c = ctx(None);
        c.process_names = true;
        assert!(has_pair(&qemu_argv(&m, &c).unwrap(), "-name", "Win 3.11,, DOS,process=virtual-m1"));
    }

    #[test]
    fn forwards_bind_localhost_and_skip_invalid() {
        let mut m = machine("modern-x86-bios");
        m.network.mode = NetworkMode::Nat;
        m.network.forwards.push(PortForward { proto: "udp".into(), host_port: 5000, guest_port: 5000 });
        m.network.forwards.push(PortForward { proto: "tcp".into(), host_port: 0, guest_port: 22 });
        m.network.forwards.push(PortForward { proto: "tcp;x".into(), host_port: 8080, guest_port: 80 });
        let a = qemu_argv(&m, &ctx(None)).unwrap();
        let net = a.iter().find(|x| x.starts_with("user,id=net0")).unwrap();
        assert_eq!(net, "user,id=net0,hostfwd=udp:127.0.0.1:5000-:5000,hostfwd=tcp:127.0.0.1:8080-:80");
    }

    #[test]
    fn shares_follow_host_capabilities() {
        let mut m = machine("modern-x86-bios");
        m.devices.storage_bus = "virtio".into();
        m.network.mode = NetworkMode::Nat;
        m.shared_folders.push(SharedFolder { host_path: "/data,x".into(), name: "da ta".into(), read_only: false });
        let a = qemu_argv(&m, &ctx(None)).unwrap();
        assert!(a.iter().any(|x| x == "local,id=fs0,path=/data,,x,security_model=mapped-xattr"));
        assert!(a.iter().any(|x| x == "virtio-9p-pci,fsdev=fs0,mount_tag=data"));
        assert!(a.iter().any(|x| x.ends_with(",smb=/data,,x")));
        assert!(host_notes(&m, &ctx(None)).is_empty());
        // Windows-Host: weder 9p noch SMB — QEMU startet trotzdem, der Start protokolliert es
        let mut c = ctx(None);
        c.smb = false;
        c.virtfs = false;
        let a = qemu_argv(&m, &c).unwrap();
        assert!(!a.iter().any(|x| x.contains("smb=") || x == "-fsdev"));
        assert!(host_notes(&m, &c).len() >= 2);
    }

    #[test]
    fn smb_share_never_writable_when_read_only_is_wanted() {
        // Retro-Profil: IDE-Bus (kein 9p), Ordner laut Isolation schreibgeschützt
        let mut m = machine("pc-1996-pentium133");
        assert!(m.isolation.read_only_shares);
        m.network.mode = NetworkMode::Isolated;
        m.shared_folders.push(SharedFolder { host_path: "/data".into(), name: "data".into(), read_only: false });
        let a = qemu_argv(&m, &ctx(None)).unwrap();
        assert!(!a.iter().any(|x| x.contains("smb=")), "SMB kann nicht schreibschützen: {a:?}");
        let notes = host_notes(&m, &ctx(None));
        assert!(notes.iter().any(|n| n.contains("immer beschreibbar")), "{notes:?}");
        // Häkchen am Ordner allein genügt ebenfalls
        m.isolation.read_only_shares = false;
        m.shared_folders[0].read_only = true;
        assert!(!qemu_argv(&m, &ctx(None)).unwrap().iter().any(|x| x.contains("smb=")));
        // Ausdrücklich beschreibbar: Freigabe wie bisher, kein Hinweis
        m.shared_folders[0].read_only = false;
        assert!(qemu_argv(&m, &ctx(None)).unwrap().iter().any(|x| x.ends_with(",smb=/data")));
        assert!(host_notes(&m, &ctx(None)).is_empty());
    }

    /// Aktuelles Profil (virtio-Bus, z. B. Windows 11) mit Netz und Samba: bis Runde 3 bekam auch
    /// ein „nur lesen“-Ordner die SMB-Freigabe (Windows-Gäste haben kein 9p). Sie entfällt jetzt —
    /// das Log muss es sagen, sonst verschwindet der Ordner für Windows-Gäste kommentarlos.
    #[test]
    fn dropped_smb_share_is_noted_even_with_9p() {
        let mut m = machine("modern-x86-uefi");
        m.tpm = false;
        m.network.mode = NetworkMode::Nat;
        m.shared_folders.push(SharedFolder { host_path: "/data".into(), name: "data".into(), read_only: true });
        let a = qemu_argv(&m, &ctx(Some("hvf"))).unwrap();
        assert!(!a.iter().any(|x| x.contains("smb=")));
        assert!(a.iter().any(|x| x.contains("virtio-9p-pci")), "9p bleibt: {a:?}");
        let notes = host_notes(&m, &ctx(Some("hvf")));
        assert!(notes.iter().any(|n| n.contains("keine SMB-Freigabe")), "{notes:?}");
        // Beschreibbarer Ordner: SMB wie bisher, kein Hinweis
        m.shared_folders[0].read_only = false;
        assert!(qemu_argv(&m, &ctx(Some("hvf"))).unwrap().iter().any(|x| x.ends_with(",smb=/data")));
        assert!(host_notes(&m, &ctx(Some("hvf"))).is_empty());
    }

    #[test]
    fn gl_only_where_the_build_supports_it() {
        let mut m = machine("modern-x86-uefi");
        m.display.gl = true;
        m.devices.graphics = "virtio-vga-gl".into();
        let a = qemu_argv(&m, &ctx(Some("hvf"))).unwrap();
        assert!(has_pair(&a, "-device", "virtio-vga"), "cocoa ohne GL: Gerät ohne -gl");
        assert!(has_pair(&a, "-display", "cocoa"));
        assert!(!host_notes(&m, &ctx(Some("hvf"))).is_empty());
        let mut c = ctx(Some("kvm"));
        c.gl = true;
        c.display_backend = "gtk".into();
        m.devices.graphics = "virtio-vga".into();
        let a = qemu_argv(&m, &c).unwrap();
        assert!(has_pair(&a, "-device", "virtio-vga-gl"));
        assert!(has_pair(&a, "-display", "gtk,gl=on"));
    }

    #[test]
    fn whpx_and_secure_boot_machine_flags() {
        let mut m = machine("modern-x86-uefi");
        let a = qemu_argv(&m, &ctx(Some("whpx"))).unwrap();
        assert!(has_pair(&a, "-M", "q35,accel=whpx,kernel-irqchip=off"));
        m.firmware = Firmware::UefiSecure;
        let a = qemu_argv(&m, &ctx(None)).unwrap();
        assert!(has_pair(&a, "-M", "q35,accel=tcg,smm=on"));
        assert!(has_pair(&a, "-global", "driver=cfi.pflash01,property=secure,value=on"));
        let a = qemu_argv(&m, &ctx(Some("hvf"))).unwrap();
        assert!(has_pair(&a, "-M", "q35,accel=hvf"), "HVF: kein SMM");
        assert!(!host_notes(&m, &ctx(Some("hvf"))).is_empty());
    }

    #[test]
    fn vnc_is_local_and_finds_a_free_display() {
        let mut m = machine("modern-x86-bios");
        m.display.kind = "vnc".into();
        assert!(has_pair(&qemu_argv(&m, &ctx(None)).unwrap(), "-vnc", "127.0.0.1:0,to=99"));
    }

    #[test]
    fn icount_mapping() {
        assert_eq!(icount_shift(100), None);
        assert_eq!(icount_shift(75), Some(1));
        assert_eq!(icount_shift(60), Some(2));
        assert_eq!(icount_shift(30), Some(3));
    }

    #[test]
    fn render_quotes_spaces() {
        let s = render("qemu-system-i386", &["-name".into(), "Win 98".into()]);
        assert_eq!(s, "qemu-system-i386 -name 'Win 98'");
    }
}
