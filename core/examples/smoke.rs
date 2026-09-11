//! Smoke-Helfer: Maschine aus Profil anlegen (machine.json) und Argv drucken.
//! Aufruf: smoke make <profile> <dir> <name> [rom|floppy|cdrom=<path>] [boot=<auto|disk|cdrom|floppy>]
//!         smoke argv <dir> <qmp|port> [<cores> <system>]
//!         smoke convert <abbild> <ziel.iso>
use std::path::{Path, PathBuf};
use virtual_core::argv::{self, HostContext};
use virtual_core::libretro::{self, LibretroContext};
use virtual_core::profile::builtin_profiles;
use virtual_core::*;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    match a[1].as_str() {
        "make" => {
            let p = builtin_profiles().into_iter().find(|p| p.id == a[2]).expect("profil");
            let dir = PathBuf::from(&a[3]);
            let mut m = p.instantiate(&format!("smoke-{}", a[2]), &a[4], "2026-09-11T16:00:00Z");
            if p.default_disk_gb > 0 && dir.join("disk-0.qcow2").exists() {
                m.disks.push(DiskRef { id: "d0".into(), size_gb: p.default_disk_gb, ..DiskRef::default() });
            }
            for extra in &a[5..] {
                let (kind, path) = extra.split_once('=').unwrap();
                if kind == "boot" {
                    m.boot = path.to_string();
                    continue;
                }
                let kind = match kind { "rom" => MediaKind::Rom, "floppy" => MediaKind::Floppy, _ => MediaKind::Cdrom };
                m.media.push(MediaRef { id: format!("m{}", m.media.len()), kind, path: path.into(), slot: 0 });
            }
            std::fs::write(dir.join("machine.json"), serde_json::to_string_pretty(&m).unwrap()).unwrap();
            println!("{}", dir.join("machine.json").display());
        }
        "argv" => {
            let dir = PathBuf::from(&a[2]);
            let m: Machine = serde_json::from_str(&std::fs::read_to_string(dir.join("machine.json")).unwrap()).unwrap();
            if m.engine == Engine::Qemu {
                let ctx = HostContext {
                    machine_dir: dir.clone(),
                    accel: None,
                    qmp: a[3].clone(),
                    uefi_code: None,
                    uefi_vars: None,
                    tpm_socket: None,
                    display_backend: "cocoa".into(),
                    audio_backend: "coreaudio".into(),
                    process_names: false,
                };
                for x in argv::qemu_argv(&m, &ctx).unwrap() { println!("{x}"); }
            } else {
                let ctx = LibretroContext { machine_dir: dir.clone(), cores_dir: a[4].clone().into(), system_dir: a[5].clone().into(), cmd_port: a[3].parse().unwrap(), core_ext: "dylib".into() };
                let cfg = m.libretro.as_ref().unwrap();
                let core = libretro::resolve_core(cfg, &ctx, |p: &Path| p.exists()).unwrap();
                let cfg_path = dir.join("retroarch.cfg");
                std::fs::write(&cfg_path, libretro::machine_config(&m, &ctx)).unwrap();
                for x in libretro::retroarch_argv(&m, &ctx, &core, &cfg_path).unwrap() { println!("{x}"); }
            }
        }
        "convert" => {
            let a1 = virtual_core::image::analyze(Path::new(&a[2])).unwrap();
            println!("{:?}", a1);
            if a1.layout.is_some() {
                let r = virtual_core::image::write_iso(&a1, Path::new(&a[3])).unwrap();
                println!("{} Sektoren aus {} geschrieben", r.sectors, r.kind);
            }
        }
        _ => panic!("make|argv|convert"),
    }
}
