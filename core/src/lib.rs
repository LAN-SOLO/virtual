//! virtual-core — die Tauri-freie Logik von virtual.:
//! Datenmodell, Maschinenprofile, Erzeugung der Engine-Kommandozeilen
//! (QEMU, RetroArch), QMP-Protokoll und Engine-Erkennung als reine Funktionen.

pub mod argv;
pub mod detect;
pub mod libretro;
pub mod model;
pub mod profile;
pub mod qmp;

pub use model::*;
pub use profile::{builtin_profiles, load_profiles, Profile, ProfileLibretro, ProfileOs};
