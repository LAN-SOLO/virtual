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
}

/// Maschinen, deren Grafik/Netz/Audio fest verdrahtet sind — kein `-device`.
fn integrated_board(machine_type: &str) -> bool {
    matches!(machine_type, "q800" | "mac99" | "g3beige" | "next-cube" | "malta")
}

fn resolve(dir: &Path, p: &str) -> String {
    let path = Path::new(p);
    if path.is_absolute() {
        p.to_string()
    } else {
        dir.join(path).to_string_lossy().into_owned()
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
    } else {
        machine.push_str(",accel=tcg");
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
            push(&mut a, "-drive");
            a.push(format!(
                "if=pflash,format=raw,readonly=on,file={}",
                code.to_string_lossy()
            ));
            if let Some(vars) = &ctx.uefi_vars {
                push(&mut a, "-drive");
                a.push(format!("if=pflash,format=raw,file={}", vars.to_string_lossy()));
            }
        }
    }
    if m.tpm {
        if let Some(sock) = &ctx.tpm_socket {
            push(&mut a, "-chardev");
            a.push(format!("socket,id=chrtpm,path={}", sock.to_string_lossy()));
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
    for (i, d) in m.disks.iter().enumerate() {
        let file = resolve(&ctx.machine_dir, &d.path);
        match bus {
            "virtio" => {
                push(&mut a, "-drive");
                a.push(format!("file={file},format={},if=none,id=disk{i},cache=writeback,discard=unmap", d.format));
                push(&mut a, "-device");
                a.push(format!("virtio-blk-pci,drive=disk{i},bootindex={}", i + 1));
            }
            "sata" => {
                push(&mut a, "-drive");
                a.push(format!("file={file},format={},if=none,id=disk{i}", d.format));
                push(&mut a, "-device");
                a.push(format!("ide-hd,drive=disk{i},bus=ide.{i},bootindex={}", i + 1));
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
                    "scsi" => "scsi".to_string(),
                    _ => "ide".to_string(),
                };
                if ifc == "none" {
                    a.push(format!("file={file},format=raw,if=none,id=cd{},media=cdrom,readonly=on", med.slot));
                    push(&mut a, "-device");
                    a.push(format!("ide-cd,drive=cd{},bootindex={}", med.slot, 10 + med.slot));
                } else {
                    a.push(format!("file={file},format=raw,if={ifc},index={cd_index},media=cdrom,readonly=on"));
                    cd_index += 1;
                }
            }
            MediaKind::Floppy => {
                let flag = if med.slot == 0 { "-fda" } else { "-fdb" };
                push(&mut a, flag);
                a.push(file);
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
        push(&mut a, "if=floppy,index=0,media=disk,format=raw,file.driver=null-co,file.read-zeroes=on,file.size=1474560");
    }
    if !m.media.iter().any(|x| x.kind == MediaKind::Cdrom) && m.disks.is_empty() {
        // Nichts zum Booten — QEMU soll wenigstens sauber starten
    }
    // Boot-Reihenfolge klassischer Boards: Diskette, CD, Platte
    if matches!(bus, "ide" | "scsi") && !integrated_board(&m.machine_type) {
        push(&mut a, "-boot");
        let order = if m.media.iter().any(|x| x.kind == MediaKind::Cdrom) { "order=dc,menu=on" } else { "order=c,menu=on" };
        push(&mut a, order);
    }

    // --- Grafik, Eingabe, Audio --------------------------------------------
    if !integrated_board(&m.machine_type) {
        let gfx = if m.devices.graphics.is_empty() { "std" } else { m.devices.graphics.as_str() };
        match gfx {
            "std" | "VGA" => { push(&mut a, "-vga"); push(&mut a, "std"); }
            "cirrus-vga" => { push(&mut a, "-vga"); push(&mut a, "cirrus"); }
            "none" => { push(&mut a, "-vga"); push(&mut a, "none"); }
            dev => {
                push(&mut a, "-vga"); push(&mut a, "none");
                push(&mut a, "-device");
                let mut d = dev.to_string();
                if m.display.gl && !d.ends_with("-gl") && d.starts_with("virtio") { d.push_str("-gl"); }
                a.push(d);
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
                push(&mut a, "-device");
                a.push(format!("{dev},audiodev=snd0"));
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
                net.push_str(&format!(",hostfwd={}::{}-:{}", f.proto, f.host_port, f.guest_port));
            }
            for s in &m.shared_folders {
                // SMB-Freigabe über SLIRP: für alte Gäste ohne virtiofs — nur erste Freigabe (SLIRP kann eine)
                net.push_str(&format!(",smb={}", s.host_path));
                break;
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
    if m.devices.storage_bus == "virtio" {
        for (i, s) in m.shared_folders.iter().enumerate() {
            let ro = s.read_only || m.isolation.read_only_shares;
            push(&mut a, "-fsdev");
            a.push(format!(
                "local,id=fs{i},path={},security_model=mapped-xattr{}",
                s.host_path,
                if ro { ",readonly=on" } else { "" }
            ));
            push(&mut a, "-device");
            a.push(format!("virtio-9p-pci,fsdev=fs{i},mount_tag={}", s.name));
        }
    }

    // --- Anzeige, Steuerung, Isolation ------------------------------------
    match m.display.kind.as_str() {
        "headless" => { push(&mut a, "-display"); push(&mut a, "none"); }
        "vnc" => { push(&mut a, "-display"); push(&mut a, "none"); push(&mut a, "-vnc"); push(&mut a, "127.0.0.1:0"); }
        _ => {
            push(&mut a, "-display");
            let mut d = ctx.display_backend.clone();
            if m.display.gl && matches!(ctx.display_backend.as_str(), "gtk" | "sdl" | "cocoa") { d.push_str(",gl=on"); }
            if m.display.fullscreen { d.push_str(",full-screen=on"); }
            a.push(d);
        }
    }
    push(&mut a, "-name");
    a.push(format!("{},process=virtual-{}", m.name, m.id));
    push(&mut a, "-qmp");
    if ctx.qmp.starts_with("tcp:") {
        a.push(format!("{},server=on,wait=off", ctx.qmp));
    } else {
        a.push(format!("unix:{},server=on,wait=off", ctx.qmp));
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
        assert!(has_pair(&a, "-netdev", "user,id=net0"));
        assert!(!a.contains(&"-icount".to_string()), "kein icount mit Beschleuniger");
        assert!(!a.contains(&"-snapshot".to_string()));
    }

    #[test]
    fn retro_pc_is_isolated_throttled_and_has_floppy() {
        let m = machine("pc-1996-pentium133");
        let a = qemu_argv(&m, &ctx(None)).unwrap();
        assert!(has_pair(&a, "-M", "pc,accel=tcg"));
        assert!(has_pair(&a, "-cpu", "pentium"));
        assert!(has_pair(&a, "-icount", "shift=1,align=off,sleep=on"));
        assert!(has_pair(&a, "-nic", "none"), "Retro-Profil ohne Netz");
        assert!(has_pair(&a, "-vga", "cirrus"));
        assert!(a.iter().any(|x| x == "sb16,audiodev=snd0"));
        assert!(a.iter().any(|x| x == "adlib,audiodev=snd0"));
        assert!(a.iter().any(|x| x.starts_with("if=floppy")), "leeres Diskettenlaufwerk");
        assert!(a.iter().any(|x| x.contains("/vm/test/disk-0.qcow2") && x.contains("if=ide")));
    }

    #[test]
    fn throwaway_adds_snapshot_flag_and_isolated_restricts() {
        let mut m = machine("pc-1999-pentium3");
        m.isolation.throwaway = true;
        m.network.mode = NetworkMode::Isolated;
        m.network.forwards.push(PortForward { proto: "tcp".into(), host_port: 2222, guest_port: 22 });
        let a = qemu_argv(&m, &ctx(None)).unwrap();
        assert!(a.contains(&"-snapshot".to_string()));
        assert!(has_pair(&a, "-netdev", "user,id=net0,restrict=on,hostfwd=tcp::2222-:22"));
        assert!(a.iter().any(|x| x == "rtl8139,netdev=net0"));
    }

    #[test]
    fn quadra_uses_rom_and_no_device_flags() {
        let mut m = machine("mac-1994-quadra");
        m.media.push(MediaRef { id: "r".into(), kind: MediaKind::Rom, path: "Q800.ROM".into(), slot: 0 });
        let a = qemu_argv(&m, &ctx(None)).unwrap();
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
