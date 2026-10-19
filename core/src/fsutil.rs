//! Datei-Helfer, die unter Windows und macOS gleich verlässlich arbeiten:
//! Umbenennen mit Wiederholung (Virenscanner, OneDrive, Indexdienst halten
//! frisch geschriebene Dateien kurz offen), BOM-tolerantes Lesen von JSON/CUE
//! aus dem Windows-Editor, reservierte Windows-Namen und Laufwerkswurzeln.

use std::path::Path;
use std::time::Duration;

/// Fehler, die unter Windows typischerweise nur kurz bestehen: Freigabe- oder
/// Sperrverletzung (32/33) und „Zugriff verweigert“ (5), solange ein Scanner die
/// Datei offen hält. Unter macOS/Linux gibt es das nicht — dort wird nicht wiederholt.
fn transient(e: &std::io::Error) -> bool {
    cfg!(windows) && (matches!(e.raw_os_error(), Some(5) | Some(32) | Some(33)) || e.kind() == std::io::ErrorKind::PermissionDenied)
}

/// `rename` mit Wiederholung bei vorübergehenden Sperren (insgesamt bis ~3 s).
pub fn rename_retry(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut wait = 25u64;
    let mut last = None;
    for _ in 0..10 {
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) if transient(&e) => {
                last = Some(e);
                std::thread::sleep(Duration::from_millis(wait));
                wait = (wait * 2).min(600);
            }
            Err(e) => return Err(e),
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("rename")))
}

/// UTF-8-BOM am Anfang entfernen (Windows-Editor speichert „UTF-8 mit BOM“).
pub fn strip_bom(s: &str) -> &str {
    s.strip_prefix('\u{feff}').unwrap_or(s)
}

/// Textdatei lesen: UTF-8 (mit oder ohne BOM), sonst als Windows-1252/Latin-1 —
/// CUE-Dateien alter Brennprogramme sind selten UTF-8, Umlaute in Dateinamen
/// würden sonst das ganze Abbild unlesbar machen.
pub fn read_text_lossless(path: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(decode_text(&bytes))
}

pub fn decode_text(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

/// Unter Windows reservierte Gerätenamen (auch mit Endung, in jeder Schreibweise).
pub fn is_windows_reserved(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or("").trim_end_matches([' ', '.']).to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}

/// Ordnerangabe aus den Einstellungen bereinigen: Leerraum und Anführungszeichen
/// (Explorer „Als Pfad kopieren“ setzt sie) entfernen; unter Windows wird eine
/// nackte Laufwerksangabe `D:` zur Wurzel `D:\` — `D:` allein meint das aktuelle
/// Verzeichnis des Laufwerks, `D:` + `vm` ergäbe `D:vm`.
pub fn normalize_user_path(s: &str, windows: bool) -> String {
    let t = s.trim();
    let t = t.strip_prefix('"').and_then(|x| x.strip_suffix('"')).unwrap_or(t).trim();
    let b = t.as_bytes();
    if windows && b.len() == 2 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        return format!("{t}\\");
    }
    t.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bom_and_codepages() {
        assert_eq!(strip_bom("\u{feff}{\"a\":1}"), "{\"a\":1}");
        assert_eq!(strip_bom("{}"), "{}");
        assert_eq!(decode_text(b"\xEF\xBB\xBFFILE \"x.bin\" BINARY"), "FILE \"x.bin\" BINARY");
        // Windows-1252: „Jürgen“ mit 0xFC
        assert_eq!(decode_text(b"FILE \"J\xFCrgen.bin\" BINARY"), "FILE \"Jürgen.bin\" BINARY");
        assert_eq!(decode_text("Größe".as_bytes()), "Größe");
    }

    #[test]
    fn reserved_windows_names() {
        for n in ["con", "CON", "nul", "Aux", "prn", "com1", "COM9", "lpt3", "con.txt", "nul.iso"] {
            assert!(is_windows_reserved(n), "{n}");
        }
        for n in ["console", "com", "com0", "lpt10", "nul-vm", "windows-98", ""] {
            assert!(!is_windows_reserved(n), "{n}");
        }
    }

    #[test]
    fn user_paths() {
        assert_eq!(normalize_user_path("D:", true), "D:\\");
        assert_eq!(normalize_user_path(" \"C:\\Program Files\\qemu\" ", true), "C:\\Program Files\\qemu");
        assert_eq!(normalize_user_path("D:", false), "D:");
        assert_eq!(normalize_user_path("/opt/homebrew/bin ", false), "/opt/homebrew/bin");
        assert_eq!(normalize_user_path("", true), "");
    }

    #[test]
    fn rename_replaces_existing_target() {
        let d = std::env::temp_dir().join(format!("virtual-fsutil-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("a.tmp"), b"neu").unwrap();
        std::fs::write(d.join("a.json"), b"alt").unwrap();
        rename_retry(&d.join("a.tmp"), &d.join("a.json")).unwrap();
        assert_eq!(std::fs::read(d.join("a.json")).unwrap(), b"neu");
        assert!(rename_retry(&d.join("fehlt"), &d.join("x")).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }
}
