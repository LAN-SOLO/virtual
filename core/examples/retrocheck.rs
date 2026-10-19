//! Prüfstand gegen ein echtes RetroArch: Konfiguration + Kommandozeile aus
//! virtual-core, dann UDP-Kommandos wie die App (Slot +/−, SAVE_STATE,
//! LOAD_STATE, PAUSE_TOGGLE, RESET, QUIT). Öffnet kurz ein RetroArch-Fenster.
//! Aufruf: `cargo run -p virtual-core --example retrocheck <retroarch> <cores-dir> <rom>`

use std::net::UdpSocket;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use virtual_core::libretro::{self, cmd, LibretroContext};
use virtual_core::profile::builtin_profiles;
use virtual_core::*;

fn send(port: u16, c: &str) {
    let s = UdpSocket::bind("127.0.0.1:0").unwrap();
    s.send_to(c.as_bytes(), ("127.0.0.1", port)).unwrap();
    std::thread::sleep(Duration::from_millis(60));
}

fn wait_for(f: &Path, before: Option<std::time::SystemTime>) -> bool {
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(8) {
        let now = f.metadata().and_then(|m| m.modified()).ok();
        if now.is_some() && now != before {
            return true;
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    false
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (bin, cores, rom) = (PathBuf::from(&a[1]), PathBuf::from(&a[2]), PathBuf::from(&a[3]));
    let dir = std::env::temp_dir().join(format!("virtual-retrocheck-{}", std::process::id()));
    for sub in ["saves", "states", "screenshots"] {
        std::fs::create_dir_all(dir.join(sub)).unwrap();
    }
    std::fs::copy(&rom, dir.join(rom.file_name().unwrap())).unwrap();
    let p = builtin_profiles().into_iter().find(|p| p.id == "console-snes").unwrap();
    let mut m = p.instantiate("rc", "SNES", "2026-10-19T00:00:00Z");
    m.display.fullscreen = false;
    m.media.push(MediaRef { id: "r".into(), kind: MediaKind::Rom, path: rom.file_name().unwrap().to_string_lossy().into(), slot: 0 });
    let port = UdpSocket::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let ctx = LibretroContext { machine_dir: dir.clone(), cores_dir: cores, system_dir: dir.join("system"), cmd_port: port, core_ext: "dylib".into() };
    let core = libretro::resolve_core(m.libretro.as_ref().unwrap(), &ctx, |p| p.exists()).expect("core");
    let cfg = dir.join("retroarch.cfg");
    std::fs::write(&cfg, libretro::machine_config(&m, &ctx)).unwrap();
    let args = libretro::retroarch_argv(&m, &ctx, &core, &cfg).unwrap();
    let mut child = Command::new(&bin).args(&args).current_dir(&dir).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    std::thread::sleep(Duration::from_secs(6));
    let stem = rom.file_stem().unwrap().to_string_lossy().into_owned();
    let mut ok = true;
    // Slot 2 ansteuern wie die App (Start = Slot 0 dank state_slot = "0")
    for c in libretro::slot_commands(0, 2) {
        send(port, c);
    }
    let f2 = dir.join("states").join(libretro::state_filename(&stem, 2));
    send(port, cmd::SAVE_STATE);
    let r = wait_for(&f2, None);
    println!("{} SAVE_STATE Slot 2 → {}", if r { "ok  " } else { "FAIL" }, f2.display());
    ok &= r;
    // zurück auf Slot 0, speichern, laden
    for c in libretro::slot_commands(2, 0) {
        send(port, c);
    }
    let f0 = dir.join("states").join(libretro::state_filename(&stem, 0));
    send(port, cmd::SAVE_STATE);
    let r = wait_for(&f0, None);
    println!("{} SAVE_STATE Slot 0 → {}", if r { "ok  " } else { "FAIL" }, f0.display());
    ok &= r;
    let before = f0.metadata().and_then(|m| m.modified()).ok();
    std::thread::sleep(Duration::from_millis(1100));
    send(port, cmd::SAVE_STATE);
    let r = wait_for(&f0, before);
    println!("{} SAVE_STATE überschreibt Slot 0 (Änderungszeit erkannt)", if r { "ok  " } else { "FAIL" });
    ok &= r;
    send(port, cmd::LOAD_STATE);
    send(port, cmd::PAUSE_TOGGLE);
    send(port, cmd::PAUSE_TOGGLE);
    send(port, cmd::RESET);
    let alive = matches!(child.try_wait(), Ok(None));
    println!("{} RetroArch läuft nach LOAD_STATE/PAUSE/RESET", if alive { "ok  " } else { "FAIL" });
    ok &= alive;
    send(port, cmd::QUIT);
    let t0 = Instant::now();
    while matches!(child.try_wait(), Ok(None)) && t0.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(100));
    }
    let quit = !matches!(child.try_wait(), Ok(None));
    println!("{} QUIT beendet RetroArch (sonst kill nach 5 s wie in der App)", if quit { "ok  " } else { "info" });
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&dir);
    std::process::exit(if ok { 0 } else { 1 });
}
