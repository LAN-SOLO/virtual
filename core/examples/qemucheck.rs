//! Prüfstand gegen ein echtes QEMU: baut für jedes QEMU-Profil (plus Varianten)
//! die Kommandozeile, startet QEMU pausiert (`-S`, ohne Fenster), spricht QMP
//! (`qmp_capabilities`, `query-status`) und beendet per `quit`.
//! Aufruf (macOS/Linux, QEMU im PATH): `cargo run -p virtual-core --example qemucheck [filter]`
//! Ergebnis: eine Zeile je Fall, Exit-Code 1 bei Fehlern.

#[cfg(unix)]
fn main() {
    unix::run();
}

#[cfg(not(unix))]
fn main() {
    eprintln!("qemucheck braucht Unix-Sockets (macOS/Linux)");
}

#[cfg(unix)]
mod unix {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    use virtual_core::argv::{self, HostContext};
    use virtual_core::detect::{self, HostOs};
    use virtual_core::profile::builtin_profiles;
    use virtual_core::*;

    fn which(bin: &str) -> Option<PathBuf> {
        std::env::split_paths(&std::env::var_os("PATH")?).map(|d| d.join(bin)).find(|p| p.is_file())
    }

    fn qmp_roundtrip(sock: &Path) -> Result<String, String> {
        let t0 = Instant::now();
        let s = loop {
            match UnixStream::connect(sock) {
                Ok(s) => break s,
                Err(e) if t0.elapsed() < Duration::from_secs(10) => {
                    let _ = e;
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(e) => return Err(format!("QMP connect: {e}")),
            }
        };
        s.set_read_timeout(Some(Duration::from_secs(10))).ok();
        let mut w = s.try_clone().unwrap();
        let mut r = BufReader::new(s);
        let mut line = String::new();
        r.read_line(&mut line).map_err(|e| e.to_string())?;
        let mut send = |cmd: String| -> Result<serde_json::Value, String> {
            w.write_all(cmd.as_bytes()).and_then(|_| w.write_all(b"\n")).map_err(|e| e.to_string())?;
            loop {
                let mut l = String::new();
                if r.read_line(&mut l).map_err(|e| e.to_string())? == 0 {
                    return Err("QMP closed".into());
                }
                match qmp::parse_line(&l) {
                    Some(qmp::Message::Return { value, .. }) => return Ok(value),
                    Some(qmp::Message::Error { desc, .. }) => return Err(desc),
                    _ => {}
                }
            }
        };
        send(qmp::capabilities())?;
        let st = qmp::status_from_return(&send(qmp::query_status())?);
        let _ = send(qmp::quit());
        Ok(st)
    }

    fn make_iso(dir: &Path) -> PathBuf {
        // Minimales ISO 9660: 20 Sektoren, PVD in Sektor 16
        let mut d = vec![0u8; 20 * 2048];
        d[16 * 2048] = 1;
        d[16 * 2048 + 1..16 * 2048 + 6].copy_from_slice(b"CD001");
        let p = dir.join("install, cd.iso");
        std::fs::write(&p, d).unwrap();
        p
    }

    fn case(name: &str, m: &Machine, dir: &Path, filter: &str) -> Option<bool> {
        if !name.contains(filter) {
            return None;
        }
        let host = HostOs::current();
        let accel = detect::accel_for_machine(host, detect::host_arch(), m.arch, &m.category).map(String::from);
        let bin = which(m.arch.qemu_binary()?)?;
        let prefix = bin.parent().and_then(|p| p.parent()).unwrap().to_string_lossy().into_owned();
        let bin_dir = bin.parent().unwrap().to_string_lossy().into_owned();
        let secure = m.firmware == Firmware::UefiSecure && m.machine_type == "q35" && matches!(accel.as_deref(), None | Some("kvm"));
        let uefi_code = detect::uefi_candidates(&prefix, &bin_dir, m.arch, secure).into_iter().map(PathBuf::from).find(|p| p.is_file());
        let uefi_vars = if uefi_code.is_some() {
            let tmpl = detect::uefi_vars_template(&prefix, &bin_dir, m.arch).into_iter().map(PathBuf::from).find(|p| p.is_file());
            let nv = dir.join("nvram.fd");
            if let Some(t) = tmpl {
                std::fs::copy(t, &nv).unwrap();
            } else {
                std::fs::write(&nv, vec![0u8; 64 * 1024 * 1024]).unwrap();
            }
            Some(nv)
        } else {
            None
        };
        let sock = std::env::temp_dir().join(format!("vqc-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&sock);
        let ctx = HostContext {
            machine_dir: dir.to_path_buf(),
            accel,
            qmp: sock.to_string_lossy().into_owned(),
            uefi_code,
            uefi_vars,
            tpm_socket: None,
            display_backend: detect::display_backend(host).into(),
            audio_backend: detect::audio_backend(host).into(),
            process_names: host == HostOs::Linux,
            gl: host != HostOs::MacOs,
            smb: false,
            virtfs: host != HostOs::Windows,
        };
        let mut args = match argv::qemu_argv(m, &ctx) {
            Ok(a) => a,
            Err(e) => {
                println!("FAIL {name}: argv: {e}");
                return Some(false);
            }
        };
        // QEMUCHECK_RUN=<s>: nicht pausiert starten und s Sekunden laufen lassen (Firmware bootet wirklich)
        let run_secs: u64 = std::env::var("QEMUCHECK_RUN").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
        if run_secs == 0 {
            args.push("-S".into());
        }
        let mut child = Command::new(&bin).args(&args).current_dir(dir).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        std::thread::sleep(Duration::from_secs(run_secs));
        let res = qmp_roundtrip(&sock);
        let t0 = Instant::now();
        while matches!(child.try_wait(), Ok(None)) && t0.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        let out = child.wait_with_output().unwrap();
        let _ = std::fs::remove_file(&sock);
        match res {
            Ok(st) => {
                println!("ok   {name}: QMP status {st}");
                Some(true)
            }
            Err(e) => {
                let err = String::from_utf8_lossy(&out.stderr);
                println!("FAIL {name}: {e}\n     {}\n     $ {}", err.trim().replace('\n', "\n     "), argv::render(&bin.to_string_lossy(), &args));
                Some(false)
            }
        }
    }

    pub fn run() {
        let filter = std::env::args().nth(1).unwrap_or_default();
        let root = std::env::temp_dir().join(format!("virtual-qemucheck-{}", std::process::id()));
        let qemu_img = which("qemu-img").expect("qemu-img fehlt");
        let mut ok = 0;
        let mut fail = 0;
        for p in builtin_profiles().into_iter().filter(|p| p.engine == Engine::Qemu) {
            // Ordner mit Komma und Leerzeichen: prüft die Maskierung in -drive/-name
            let dir = root.join(format!("{}, test dir", p.id));
            std::fs::create_dir_all(&dir).unwrap();
            let mut base = p.instantiate("chk-1", "Check, Maschine", "2026-10-19T00:00:00Z");
            base.display.kind = "headless".into();
            if p.default_disk_gb > 0 {
                let disk = DiskRef { id: "d0".into(), size_gb: 1, ..DiskRef::default() };
                let st = Command::new(&qemu_img).args(argv::qemu_img_create(&dir.join(&disk.path), "qcow2", 1)).output().unwrap();
                assert!(st.status.success());
                base.disks.push(disk);
            }
            let mut variants: Vec<(String, Machine)> = vec![(format!("{} plain", p.id), base.clone())];
            if p.needs_rom {
                // Quadra 800 startet ohne echtes ROM nicht — Fall überspringen, Argv ist in den Unit-Tests geprüft
                variants.clear();
            }
            let iso = make_iso(&dir);
            if !p.needs_rom && p.machine_type != "virt" {
                let mut m = base.clone();
                m.media.push(MediaRef { id: "c".into(), kind: MediaKind::Cdrom, path: iso.to_string_lossy().into(), slot: 0 });
                if m.devices.floppy {
                    let fl = dir.join("boot.img");
                    std::fs::write(&fl, vec![0u8; 1474560]).unwrap();
                    m.media.push(MediaRef { id: "f".into(), kind: MediaKind::Floppy, path: "boot.img".into(), slot: 0 });
                }
                for boot in ["auto", "cdrom", "disk", "floppy"] {
                    if boot == "floppy" && !m.devices.floppy {
                        continue;
                    }
                    let mut v = m.clone();
                    v.boot = boot.into();
                    variants.push((format!("{} media boot={boot}", p.id), v));
                }
            }
            if !p.needs_rom {
                let mut m = base.clone();
                m.network.mode = NetworkMode::Isolated;
                m.network.forwards.push(PortForward { proto: "tcp".into(), host_port: 0, guest_port: 22 });
                m.network.forwards.push(PortForward { proto: "tcp".into(), host_port: 40000 + (ok + fail) as u16, guest_port: 22 });
                let share = dir.join("share, ro");
                std::fs::create_dir_all(&share).unwrap();
                m.shared_folders.push(SharedFolder { host_path: share.to_string_lossy().into(), name: "share".into(), read_only: true });
                m.isolation.throwaway = true;
                m.display.kind = "vnc".into();
                variants.push((format!("{} net+share+throwaway+vnc", p.id), m));
            }
            if p.machine_type == "q35" {
                let mut m = base.clone();
                m.firmware = Firmware::UefiSecure;
                variants.push((format!("{} uefi-secure", p.id), m));
            }
            for (name, m) in variants {
                match case(&name, &m, &dir, &filter) {
                    Some(true) => ok += 1,
                    Some(false) => fail += 1,
                    None => {}
                }
            }
        }
        let _ = std::fs::remove_dir_all(&root);
        println!("\n{ok} ok, {fail} fehlgeschlagen");
        if fail > 0 {
            std::process::exit(1);
        }
    }
}
