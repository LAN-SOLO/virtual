//! Konsolen über RetroArch + libretro-Cores: Kommandozeile, Konfiguration je
//! Maschine und die Kommandos des Netzwerk-Kommando-Interfaces (UDP).

use crate::model::*;
use std::path::{Path, PathBuf};

/// Kommandos, die RetroArch per UDP entgegennimmt (`network_cmd_enable = true`).
pub mod cmd {
    pub const PAUSE_TOGGLE: &str = "PAUSE_TOGGLE";
    pub const RESET: &str = "RESET";
    pub const SAVE_STATE: &str = "SAVE_STATE";
    pub const LOAD_STATE: &str = "LOAD_STATE";
    pub const STATE_SLOT_PLUS: &str = "STATE_SLOT_PLUS";
    pub const STATE_SLOT_MINUS: &str = "STATE_SLOT_MINUS";
    pub const SCREENSHOT: &str = "SCREENSHOT";
    pub const FULLSCREEN_TOGGLE: &str = "FULLSCREEN_TOGGLE";
    pub const FAST_FORWARD: &str = "FAST_FORWARD";
    pub const MENU_TOGGLE: &str = "MENU_TOGGLE";
    pub const QUIT: &str = "QUIT";
    /// Antwortet mit `GET_STATUS <PAUSED|PLAYING|CONTENTLESS> <core>,<content>,crc32=…`.
    /// **Nicht im laufenden Betrieb senden:** RetroArch 1.22.2 (macOS) stürzt beim
    /// Beantworten mit SIGSEGV ab. Der Pausenzustand wird deshalb lokal geführt.
    pub const GET_STATUS: &str = "GET_STATUS";
}

#[derive(Debug, Clone, Default)]
pub struct LibretroContext {
    pub machine_dir: PathBuf,
    /// Ordner mit den Cores (`<name>_libretro.<dylib|so|dll>`).
    pub cores_dir: PathBuf,
    /// System-/BIOS-Ordner.
    pub system_dir: PathBuf,
    /// UDP-Port für Kommandos (je Maschine eindeutig).
    pub cmd_port: u16,
    /// `dylib` | `so` | `dll`
    pub core_ext: String,
}

/// Dateiname eines Cores auf dieser Plattform.
pub fn core_filename(core: &str, ext: &str) -> String {
    format!("{core}_libretro.{ext}")
}

/// Kern-Datei im Cores-Ordner: erst der bevorzugte Core, sonst der Fallback.
pub fn resolve_core(cfg: &LibretroConfig, ctx: &LibretroContext, exists: impl Fn(&Path) -> bool) -> Result<PathBuf, String> {
    let primary = ctx.cores_dir.join(core_filename(&cfg.core, &ctx.core_ext));
    if exists(&primary) {
        return Ok(primary);
    }
    if let Some(fb) = &cfg.fallback_core {
        let p = ctx.cores_dir.join(core_filename(fb, &ctx.core_ext));
        if exists(&p) {
            return Ok(p);
        }
    }
    Err(format!(
        "Core „{}“ nicht gefunden in {} — in RetroArch unter „Online-Updater → Core herunterladen“ laden oder den Cores-Ordner in den Einstellungen anpassen.",
        cfg.core,
        ctx.cores_dir.to_string_lossy()
    ))
}

/// Fehlende BIOS-Dateien (relativ zum System-Ordner).
pub fn missing_bios(cfg: &LibretroConfig, ctx: &LibretroContext, exists: impl Fn(&Path) -> bool) -> Vec<String> {
    cfg.bios
        .iter()
        .filter(|b| !exists(&ctx.system_dir.join(b)))
        .cloned()
        .collect()
}

/// Konfigurations-Overlay je Maschine (`--appendconfig`).
pub fn machine_config(m: &Machine, ctx: &LibretroContext) -> String {
    let saves = ctx.machine_dir.join("saves");
    let states = ctx.machine_dir.join("states");
    let shots = ctx.machine_dir.join("screenshots");
    let mut s = String::new();
    s.push_str(&format!("savefile_directory = \"{}\"\n", saves.to_string_lossy()));
    s.push_str(&format!("savestate_directory = \"{}\"\n", states.to_string_lossy()));
    s.push_str(&format!("screenshot_directory = \"{}\"\n", shots.to_string_lossy()));
    s.push_str(&format!("system_directory = \"{}\"\n", ctx.system_dir.to_string_lossy()));
    s.push_str("network_cmd_enable = \"true\"\n");
    s.push_str(&format!("network_cmd_port = \"{}\"\n", ctx.cmd_port));
    s.push_str(&format!("video_fullscreen = \"{}\"\n", if m.display.fullscreen { "true" } else { "false" }));
    // Save States und SRAM flach in die Maschinenordner — ohne Core-/Inhalts-Unterordner,
    // sonst findet die App `states/<rom>.state` nicht.
    s.push_str("sort_savefiles_enable = \"false\"\n");
    s.push_str("sort_savestates_enable = \"false\"\n");
    s.push_str("sort_savefiles_by_content_enable = \"false\"\n");
    s.push_str("sort_savestates_by_content_enable = \"false\"\n");
    // Kein Verlauf in ~/Documents/RetroArch — die Maschine bleibt in ihrem Ordner
    s.push_str("history_list_enable = \"false\"\n");
    s.push_str("savestate_auto_index = \"false\"\n");
    s.push_str("savestate_auto_save = \"false\"\n");
    s.push_str("savestate_auto_load = \"false\"\n");
    s.push_str("pause_nonactive = \"false\"\n");
    // Sonst verlangt RetroArch das QUIT-Kommando zweimal und das sanfte Beenden verpufft
    s.push_str("quit_press_twice = \"false\"\n");
    s.push_str("config_save_on_exit = \"false\"\n");
    // Konsolen sind offline: kein Netplay, keine Cheevos, keine Updates aus dem Core-Menü
    s.push_str("netplay_enable = \"false\"\n");
    s.push_str("cheevos_enable = \"false\"\n");
    s.push_str("core_updater_auto_backup = \"false\"\n");
    s
}

/// `retroarch`-Argumente (ohne Binary).
pub fn retroarch_argv(m: &Machine, ctx: &LibretroContext, core_path: &Path, config_path: &Path) -> Result<Vec<String>, String> {
    let cfg = m.libretro.as_ref().ok_or("Keine Konsolen-Maschine")?;
    let _ = cfg;
    let rom = m
        .media
        .iter()
        .find(|x| x.kind == MediaKind::Rom)
        .ok_or("Kein Spiel eingelegt — ROM oder Disc-Abbild unter „Medien“ wählen.")?;
    let rom_path = {
        let p = Path::new(&rom.path);
        if p.is_absolute() { p.to_path_buf() } else { ctx.machine_dir.join(p) }
    };
    let mut a = vec![
        "-L".to_string(),
        core_path.to_string_lossy().into_owned(),
        "--appendconfig".to_string(),
        config_path.to_string_lossy().into_owned(),
        "--verbose".to_string(),
    ];
    if m.display.fullscreen {
        a.push("--fullscreen".into());
    }
    a.push(rom_path.to_string_lossy().into_owned());
    Ok(a)
}

/// Save-State-Slot wechseln: RetroArch kennt nur +/− — Anzahl und Richtung ab `current`.
pub fn slot_commands(current: u32, target: u32) -> Vec<&'static str> {
    if target >= current {
        vec![cmd::STATE_SLOT_PLUS; (target - current) as usize]
    } else {
        vec![cmd::STATE_SLOT_MINUS; (current - target) as usize]
    }
}

/// Dateiname des Save-States eines Slots (RetroArch: `.state`, `.state1`, `.state2` …).
pub fn state_filename(rom_stem: &str, slot: u32) -> String {
    if slot == 0 {
        format!("{rom_stem}.state")
    } else {
        format!("{rom_stem}.state{slot}")
    }
}

/// Antwort auf `GET_STATUS` deuten → `paused` | `running` | `contentless`.
pub fn parse_status(reply: &str) -> Option<&'static str> {
    let mut it = reply.trim().split_whitespace();
    if it.next()? != "GET_STATUS" {
        return None;
    }
    Some(match it.next()? {
        "PAUSED" => "paused",
        "PLAYING" => "running",
        "CONTENTLESS" => "contentless",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::builtin_profiles;

    fn ctx() -> LibretroContext {
        LibretroContext {
            machine_dir: PathBuf::from("/vm/snes"),
            cores_dir: PathBuf::from("/cores"),
            system_dir: PathBuf::from("/system"),
            cmd_port: 55400,
            core_ext: "dylib".into(),
        }
    }

    fn machine() -> Machine {
        let p = builtin_profiles().into_iter().find(|p| p.id == "console-psx").unwrap();
        let mut m = p.instantiate("c1", "PSX", "2026-09-10T00:00:00Z");
        m.media.push(MediaRef { id: "r".into(), kind: MediaKind::Rom, path: "/games/game.cue".into(), slot: 0 });
        m
    }

    #[test]
    fn resolves_primary_then_fallback() {
        let m = machine();
        let c = ctx();
        let cfg = m.libretro.as_ref().unwrap();
        let only_fallback = |p: &Path| p.ends_with("pcsx_rearmed_libretro.dylib");
        assert_eq!(resolve_core(cfg, &c, only_fallback).unwrap(), PathBuf::from("/cores/pcsx_rearmed_libretro.dylib"));
        assert_eq!(resolve_core(cfg, &c, |_| true).unwrap(), PathBuf::from("/cores/swanstation_libretro.dylib"));
        assert!(resolve_core(cfg, &c, |_| false).is_err());
    }

    #[test]
    fn reports_missing_bios() {
        let m = machine();
        let c = ctx();
        let have_us = |p: &Path| p.ends_with("scph5501.bin");
        let missing = missing_bios(m.libretro.as_ref().unwrap(), &c, have_us);
        assert_eq!(missing, vec!["scph5500.bin".to_string(), "scph5502.bin".to_string()]);
    }

    #[test]
    fn builds_argv_and_config() {
        let m = machine();
        let c = ctx();
        let a = retroarch_argv(&m, &c, Path::new("/cores/swanstation_libretro.dylib"), Path::new("/vm/snes/retroarch.cfg")).unwrap();
        assert_eq!(a[0], "-L");
        assert!(a.contains(&"--fullscreen".to_string()));
        assert_eq!(a.last().unwrap(), "/games/game.cue");
        let cfg = machine_config(&m, &c);
        assert!(cfg.contains("network_cmd_port = \"55400\""));
        assert!(cfg.contains("system_directory = \"/system\""));
        assert!(cfg.contains("savestate_directory = \"/vm/snes/states\""));
        assert!(cfg.contains("sort_savestates_enable = \"false\""), "States flach im Ordner");
        assert!(cfg.contains("history_list_enable = \"false\""));
        assert!(cfg.contains("quit_press_twice = \"false\""), "QUIT muss beim ersten Mal wirken");
    }

    #[test]
    fn no_rom_is_an_error() {
        let mut m = machine();
        m.media.clear();
        assert!(retroarch_argv(&m, &ctx(), Path::new("/c"), Path::new("/cfg")).is_err());
    }

    #[test]
    fn slots_and_states() {
        assert_eq!(slot_commands(0, 2), vec!["STATE_SLOT_PLUS", "STATE_SLOT_PLUS"]);
        assert_eq!(slot_commands(3, 1), vec!["STATE_SLOT_MINUS", "STATE_SLOT_MINUS"]);
        assert!(slot_commands(2, 2).is_empty());
        assert_eq!(state_filename("game", 0), "game.state");
        assert_eq!(state_filename("game", 3), "game.state3");
        assert_eq!(parse_status("GET_STATUS PLAYING snes9x,game,crc32=abcd"), Some("running"));
        assert_eq!(parse_status("GET_STATUS PAUSED x"), Some("paused"));
        assert_eq!(parse_status("nope"), None);
    }
}
