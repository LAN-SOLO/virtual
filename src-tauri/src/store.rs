//! Maschinenordner: jede Maschine ist `<machinesDir>/<slug>/` mit `machine.json`,
//! `snapshots.json`, Platten, `nvram.fd`, `saves/`, `states/`, `screenshots/`
//! und `log.txt`. Kopieren = Backup, Verschieben = Umzug. JSON wird atomar
//! geschrieben (Temp-Datei + rename), damit ein Absturz nie einen halben
//! Bestand hinterlässt.

use std::path::{Path, PathBuf};
use virtual_core::{Machine, Snapshot};

pub const MACHINE_FILE: &str = "machine.json";
pub const SNAPSHOTS_FILE: &str = "snapshots.json";

pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Ordner anlegen fehlgeschlagen: {e}"))?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).map_err(|e| format!("Schreiben fehlgeschlagen: {e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("Umbenennen fehlgeschlagen: {e}"))
}

/// Ordnername aus dem Maschinennamen: klein, ASCII, Bindestriche.
pub fn slugify(name: &str) -> String {
    let mut out = String::new();
    let mut last_dash = true;
    for c in name.to_lowercase().chars() {
        let mapped: Option<char> = match c {
            'ä' => Some('a'),
            'ö' => Some('o'),
            'ü' => Some('u'),
            'ß' => Some('s'),
            c if c.is_ascii_alphanumeric() => Some(c.to_ascii_lowercase()),
            _ => None,
        };
        match mapped {
            Some(ch) => {
                out.push(ch);
                last_dash = false;
            }
            None if !last_dash => {
                out.push('-');
                last_dash = true;
            }
            None => {}
        }
    }
    let trimmed = out.trim_end_matches('-').to_string();
    if trimmed.is_empty() {
        "maschine".into()
    } else {
        trimmed
    }
}

/// Legt einen noch nicht belegten Ordner für den Namen an (`slug`, `slug-2`, …).
pub fn create_machine_dir(base: &Path, name: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(base).map_err(|e| format!("Maschinenordner anlegen fehlgeschlagen: {e}"))?;
    let slug = slugify(name);
    let mut candidate = base.join(&slug);
    let mut n = 2;
    while candidate.exists() {
        candidate = base.join(format!("{slug}-{n}"));
        n += 1;
    }
    std::fs::create_dir_all(&candidate).map_err(|e| format!("Ordner anlegen fehlgeschlagen: {e}"))?;
    for sub in ["saves", "states", "screenshots"] {
        let _ = std::fs::create_dir_all(candidate.join(sub));
    }
    Ok(candidate)
}

pub fn load_machine(dir: &Path) -> Option<Machine> {
    let text = std::fs::read_to_string(dir.join(MACHINE_FILE)).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save_machine(dir: &Path, m: &Machine) -> Result<(), String> {
    let json = serde_json::to_string_pretty(m).map_err(|e| e.to_string())?;
    write_atomic(&dir.join(MACHINE_FILE), json.as_bytes())
}

/// Alle Maschinen mit ihrem Ordner, sortiert nach Name.
pub fn list(base: &Path) -> Vec<(PathBuf, Machine)> {
    let mut out: Vec<(PathBuf, Machine)> = std::fs::read_dir(base)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .filter_map(|p| load_machine(&p).map(|m| (p, m)))
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| a.1.name.to_lowercase().cmp(&b.1.name.to_lowercase()));
    out
}

/// Ordner zur Maschinen-Id.
pub fn find_dir(base: &Path, id: &str) -> Option<PathBuf> {
    list(base).into_iter().find(|(_, m)| m.id == id).map(|(p, _)| p)
}

pub fn load_snapshots(dir: &Path) -> Vec<Snapshot> {
    std::fs::read_to_string(dir.join(SNAPSHOTS_FILE))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_snapshots(dir: &Path, snaps: &[Snapshot]) -> Result<(), String> {
    let json = serde_json::to_string_pretty(snaps).map_err(|e| e.to_string())?;
    write_atomic(&dir.join(SNAPSHOTS_FILE), json.as_bytes())
}

/// Maschine entfernen — mit `delete_files` samt Ordner, sonst nur `machine.json`
/// (der Ordner bleibt als Datenreste liegen, z. B. zum manuellen Sichern).
pub fn delete(dir: &Path, delete_files: bool) -> Result<(), String> {
    if delete_files {
        std::fs::remove_dir_all(dir).map_err(|e| format!("Ordner löschen fehlgeschlagen: {e}"))
    } else {
        std::fs::remove_file(dir.join(MACHINE_FILE)).map_err(|e| format!("Löschen fehlgeschlagen: {e}"))
    }
}

/// Logdatei der Maschine (Engine-Ausgabe).
pub fn append_log(dir: &Path, line: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("log.txt")) {
        let _ = writeln!(f, "{line}");
    }
}

pub fn truncate_log(dir: &Path) {
    let _ = std::fs::write(dir.join("log.txt"), b"");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir() -> PathBuf {
        let p = std::env::temp_dir().join(format!("virtual-store-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn slugs() {
        assert_eq!(slugify("Windows 98 SE"), "windows-98-se");
        assert_eq!(slugify("Äpfel & Öl"), "apfel-ol");
        assert_eq!(slugify("***"), "maschine");
    }

    #[test]
    fn create_save_list_delete() {
        let base = tempdir();
        let d1 = create_machine_dir(&base, "Test VM").unwrap();
        let d2 = create_machine_dir(&base, "Test VM").unwrap();
        assert!(d1.ends_with("test-vm"));
        assert!(d2.ends_with("test-vm-2"));
        assert!(d1.join("states").is_dir());

        let mut m = Machine::default();
        m.id = "abc".into();
        m.name = "Test VM".into();
        save_machine(&d1, &m).unwrap();
        assert_eq!(load_machine(&d1).unwrap().id, "abc");
        assert_eq!(list(&base).len(), 1, "Ordner ohne machine.json werden ignoriert");
        assert_eq!(find_dir(&base, "abc").unwrap(), d1);
        assert!(find_dir(&base, "nope").is_none());

        let snaps = vec![Snapshot { id: "s1".into(), tag: "vs-1".into(), ..Snapshot::default() }];
        save_snapshots(&d1, &snaps).unwrap();
        assert_eq!(load_snapshots(&d1)[0].tag, "vs-1");
        assert!(load_snapshots(&d2).is_empty());

        delete(&d1, true).unwrap();
        assert!(!d1.exists());
        let _ = std::fs::remove_dir_all(&base);
    }
}
