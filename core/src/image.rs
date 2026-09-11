//! CD-Abbilder nach ISO (2048-Byte-Sektoren) wandeln. QEMU liest nur ISO bzw. rohe
//! 2048er-Abbilder; Nero NRG, BIN/CUE, Alcohol MDF/MDS, CloneCD CCD/IMG und rohe
//! 2352-Byte-Abbilder werden hier auf ihre Datenspur reduziert. Reine Byte-Logik,
//! streamend (Abbilder sind hunderte MB), mit synthetischen Abbildern getestet.

use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub const ISO_SECTOR: u64 = 2048;
const SYNC: [u8; 12] = [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00];

/// Wo im Quellabbild die 2048 Nutzbytes je Sektor liegen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    /// 2048 | 2336 | 2352 | 2448
    pub sector_size: u32,
    /// Versatz der Nutzdaten im Sektor (0, 8, 16 oder 24).
    pub data_offset: u32,
    /// Byte-Offset des ersten Sektors der Datenspur.
    pub start: u64,
    /// Byte-Offset hinter dem letzten Sektor.
    pub end: u64,
}

impl Layout {
    pub fn sectors(&self) -> u64 {
        (self.end.saturating_sub(self.start)) / self.sector_size as u64
    }
}

/// Ergebnis der Analyse: welche Datei die Sektoren enthält und ob gewandelt werden muss.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Analysis {
    /// `iso` | `nrg` | `cue` | `mdf` | `ccd` | `raw`
    pub kind: &'static str,
    /// Datei mit den Sektordaten (bei CUE/MDS/CCD die Begleitdatei).
    pub data_file: PathBuf,
    /// `None`: direkt als ISO nutzbar, keine Wandlung nötig.
    pub layout: Option<Layout>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub kind: &'static str,
    pub sectors: u64,
    pub layout: Layout,
}

fn ext(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// Endungen, die als CD-Abbild gelten und unter „CD einlegen“ angeboten werden.
pub const CD_EXTENSIONS: &[&str] = &["iso", "nrg", "bin", "cue", "mdf", "mds", "ccd", "img", "cdr", "toast", "dmg"];

/// Ziel-Dateiname der Wandlung im Maschinenordner: `<stem>.iso`.
pub fn iso_name_for(src: &Path) -> String {
    let stem = src.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "image".into());
    format!("{stem}.iso")
}

/// Analysiert ein Abbild ohne es zu wandeln.
pub fn analyze(path: &Path) -> Result<Analysis, String> {
    match ext(path).as_str() {
        "cue" => parse_cue(path),
        "mds" => sibling(path, "mdf", "mdf"),
        "ccd" => sibling(path, "img", "ccd"),
        "nrg" => parse_nrg(path),
        _ => analyze_raw(path, "raw"),
    }
}

fn sibling(path: &Path, sibling_ext: &str, kind: &'static str) -> Result<Analysis, String> {
    let data = path.with_extension(sibling_ext);
    if !data.is_file() {
        return Err(format!("Begleitdatei {} fehlt neben {}", data.display(), path.display()));
    }
    let mut a = analyze_raw(&data, kind)?;
    a.kind = kind;
    Ok(a)
}

/// Sektorformat aus den ersten Bytes erkennen: (Sektorgröße, Datenversatz).
pub fn sniff_layout(head: &[u8]) -> Option<(u32, u32)> {
    if head.len() >= 16 && head[..12] == SYNC {
        let offset = match head[15] {
            1 => 16,
            2 => 24,
            _ => return None,
        };
        // 2448 = 2352 + 96 Byte Subchannel: der zweite Sync verrät die Sektorgröße
        let size = if head.len() >= 2448 + 12 && head[2448..2460] == SYNC && head[2352..2364] != SYNC { 2448 } else { 2352 };
        return Some((size, offset));
    }
    // Plain ISO 9660 / HFS-Hybrid: Primary Volume Descriptor in Sektor 16
    if head.len() >= 0x8006 && &head[0x8001..0x8006] == b"CD001" {
        return Some((2048, 0));
    }
    // Mode 2 / 2336 (Subheader + 2048 + 280): Descriptor bei 16·2336 + 8
    let m2 = 16 * 2336 + 8 + 1;
    if head.len() >= m2 + 5 && &head[m2..m2 + 5] == b"CD001" {
        return Some((2336, 8));
    }
    None
}

fn read_head(path: &Path, max: usize) -> Result<(Vec<u8>, u64), String> {
    let mut f = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let len = f.metadata().map_err(|e| e.to_string())?.len();
    let mut buf = vec![0u8; max.min(len as usize)];
    f.read_exact(&mut buf).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok((buf, len))
}

fn analyze_raw(path: &Path, kind: &'static str) -> Result<Analysis, String> {
    let (head, len) = read_head(path, 16 * 2352 + 2448 + 64)?;
    match sniff_layout(&head) {
        Some((2048, 0)) => Ok(Analysis { kind: "iso", data_file: path.to_path_buf(), layout: None }),
        Some((size, off)) => Ok(Analysis {
            kind,
            data_file: path.to_path_buf(),
            layout: Some(Layout { sector_size: size, data_offset: off, start: 0, end: len - len % size as u64 }),
        }),
        None => Err(format!(
            "{}: Sektorformat nicht erkannt — kein ISO 9660 und keine rohen 2352/2336-Byte-Sektoren",
            path.display()
        )),
    }
}

// ---------------------------------------------------------------------------
// CUE
// ---------------------------------------------------------------------------

fn cue_mode(mode: &str) -> Result<(u32, u32), String> {
    Ok(match mode.to_uppercase().as_str() {
        "MODE1/2048" | "MODE2/2048" => (2048, 0),
        "MODE1/2352" => (2352, 16),
        "MODE2/2352" | "CDI/2352" => (2352, 24),
        "MODE2/2336" | "CDI/2336" => (2336, 8),
        "AUDIO" => return Err("erste Spur ist eine Audiospur — kein Datenabbild".into()),
        other => return Err(format!("CUE: Spurmodus „{other}“ nicht unterstützt")),
    })
}

fn msf_frames(s: &str) -> Option<u64> {
    let mut it = s.split(':').map(|p| p.trim().parse::<u64>());
    let m = it.next()?.ok()?;
    let sec = it.next()?.ok()?;
    let f = it.next()?.ok()?;
    Some(m * 60 * 75 + sec * 75 + f)
}

fn parse_cue(path: &Path) -> Result<Analysis, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut file: Option<PathBuf> = None;
    let mut mode: Option<(u32, u32)> = None;
    let mut index1: Option<u64> = None;
    let mut next_track_start: Option<u64> = None;
    let mut in_first = false;
    let mut tracks = 0;
    for raw in text.lines() {
        let line = raw.trim();
        let mut words = line.splitn(2, ' ');
        let key = words.next().unwrap_or("").to_uppercase();
        let rest = words.next().unwrap_or("").trim();
        match key.as_str() {
            "FILE" => {
                if file.is_none() {
                    let name = rest.rsplit_once(' ').map(|(n, _)| n).unwrap_or(rest).trim().trim_matches('"');
                    let p = Path::new(name);
                    file = Some(if p.is_absolute() { p.to_path_buf() } else { dir.join(p) });
                }
            }
            "TRACK" => {
                tracks += 1;
                if tracks == 1 {
                    in_first = true;
                    let m = rest.split_whitespace().nth(1).unwrap_or("");
                    mode = Some(cue_mode(m)?);
                } else {
                    in_first = false;
                }
            }
            "INDEX" => {
                let mut p = rest.split_whitespace();
                let n = p.next().unwrap_or("");
                let frames = p.next().and_then(msf_frames);
                if in_first && n == "01" {
                    index1 = frames;
                } else if !in_first && tracks == 2 && next_track_start.is_none() {
                    next_track_start = frames;
                }
            }
            _ => {}
        }
    }
    let data_file = file.ok_or("CUE: keine FILE-Zeile")?;
    if !data_file.is_file() {
        return Err(format!("CUE: Datendatei {} fehlt", data_file.display()));
    }
    let (size, off) = mode.ok_or("CUE: keine TRACK-Zeile")?;
    let len = std::fs::metadata(&data_file).map_err(|e| e.to_string())?.len();
    let start = index1.unwrap_or(0) * size as u64;
    let end = next_track_start.map(|f| f * size as u64).unwrap_or(len).min(len);
    Ok(Analysis { kind: "cue", data_file, layout: Some(Layout { sector_size: size, data_offset: off, start, end }) })
}

// ---------------------------------------------------------------------------
// Nero NRG
// ---------------------------------------------------------------------------

fn be16(b: &[u8]) -> u16 {
    u16::from_be_bytes([b[0], b[1]])
}
fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}
fn be64(b: &[u8]) -> u64 {
    u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
}

/// Sektorgröße aus dem Nero-Moduscode (Spurinfo ohne explizite Größe).
fn nrg_mode_sector_size(mode: u32) -> Option<u32> {
    Some(match mode {
        0x00 | 0x02 => 2048,
        0x03 => 2336,
        0x05 | 0x06 | 0x07 => 2352,
        0x0F | 0x10 | 0x11 => 2448,
        _ => return None,
    })
}

fn is_audio_mode(mode: u32) -> bool {
    matches!(mode, 0x07 | 0x10)
}

/// Fußzeile: (Offset der Chunk-Liste, Version 2?)
pub fn nrg_footer(tail12: &[u8]) -> Option<(u64, bool)> {
    if tail12.len() < 12 {
        return None;
    }
    if &tail12[0..4] == b"NER5" {
        return Some((be64(&tail12[4..12]), true));
    }
    if &tail12[4..8] == b"NERO" {
        return Some((be32(&tail12[8..12]) as u64, false));
    }
    None
}

/// Spurinfo aus DAOX/DAOI (Disc-at-once) — erste Datenspur.
fn nrg_dao_track(payload: &[u8], v2: bool) -> Option<(u32, u32, u64, u64)> {
    let entry = if v2 { 42 } else { 30 };
    let header = [22usize, 20, 24, 18].into_iter().find(|h| payload.len() > *h && (payload.len() - h) % entry == 0)?;
    let mut off = header;
    while off + entry <= payload.len() {
        let e = &payload[off..off + entry];
        let sector_size = be16(&e[12..14]) as u32;
        let mode = e[14] as u32;
        let (start, end) = if v2 { (be64(&e[26..34]), be64(&e[34..42])) } else { (be32(&e[22..26]) as u64, be32(&e[26..30]) as u64) };
        if !is_audio_mode(mode) && matches!(sector_size, 2048 | 2336 | 2352 | 2448) && end > start {
            return Some((sector_size, mode, start, end));
        }
        off += entry;
    }
    None
}

/// Spurinfo aus ETN2/ETNF (Track-at-once) — erste Datenspur.
fn nrg_etn_track(payload: &[u8], v2: bool) -> Option<(u32, u32, u64, u64)> {
    let entry = if v2 { 32 } else { 20 };
    let mut off = 0;
    while off + entry <= payload.len() {
        let e = &payload[off..off + entry];
        let (start, size, mode) = if v2 {
            (be64(&e[0..8]), be64(&e[8..16]), be32(&e[16..20]))
        } else {
            (be32(&e[0..4]) as u64, be32(&e[4..8]) as u64, be32(&e[8..12]))
        };
        if !is_audio_mode(mode) && size > 0 {
            let sector_size = nrg_mode_sector_size(mode)?;
            return Some((sector_size, mode, start, start + size));
        }
        off += entry;
    }
    None
}

fn parse_nrg(path: &Path) -> Result<Analysis, String> {
    let mut f = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let len = f.metadata().map_err(|e| e.to_string())?.len();
    if len < 12 {
        return Err("NRG: Datei zu kurz".into());
    }
    let mut tail = [0u8; 12];
    f.seek(SeekFrom::Start(len - 12)).map_err(|e| e.to_string())?;
    f.read_exact(&mut tail).map_err(|e| e.to_string())?;
    let (chunks_at, v2) = nrg_footer(&tail).ok_or("NRG: Nero-Kennung (NERO/NER5) fehlt")?;
    if chunks_at >= len {
        return Err("NRG: Chunk-Liste liegt außerhalb der Datei".into());
    }
    f.seek(SeekFrom::Start(chunks_at)).map_err(|e| e.to_string())?;
    let mut track: Option<(u32, u32, u64, u64)> = None;
    let mut etn: Option<(u32, u32, u64, u64)> = None;
    let mut pos = chunks_at;
    loop {
        let mut hdr = [0u8; 8];
        if pos + 8 > len || f.read_exact(&mut hdr).is_err() {
            break;
        }
        let id = &hdr[0..4];
        let size = be32(&hdr[4..8]) as u64;
        if id == b"END!" {
            break;
        }
        if pos + 8 + size > len || size > 16 * 1024 * 1024 {
            return Err("NRG: beschädigte Chunk-Liste".into());
        }
        let mut payload = vec![0u8; size as usize];
        f.read_exact(&mut payload).map_err(|e| e.to_string())?;
        match id {
            b"DAOX" => track = track.or_else(|| nrg_dao_track(&payload, true)),
            b"DAOI" => track = track.or_else(|| nrg_dao_track(&payload, false)),
            b"ETN2" => etn = etn.or_else(|| nrg_etn_track(&payload, true)),
            b"ETNF" => etn = etn.or_else(|| nrg_etn_track(&payload, false)),
            _ => {}
        }
        pos += 8 + size;
    }
    let _ = v2;
    let (sector_size, start, end) = match track.or(etn) {
        Some((s, _, a, b)) => (s, a, b.min(chunks_at)),
        None => {
            // Kein Spur-Chunk lesbar: Sektorformat am Anfang erschnüffeln, Daten bis zur Chunk-Liste
            let (head, _) = read_head(path, 16 * 2352 + 2448 + 64)?;
            let (s, off) = sniff_layout(&head).ok_or("NRG: keine Spurinfo (DAO/ETN) und Sektorformat nicht erkennbar")?;
            let end = chunks_at - chunks_at % s as u64;
            return Ok(Analysis { kind: "nrg", data_file: path.to_path_buf(), layout: Some(Layout { sector_size: s, data_offset: off, start: 0, end }) });
        }
    };
    if start >= end || end > len {
        return Err("NRG: Spurgrenzen unplausibel".into());
    }
    // Datenversatz im Sektor am ersten Sektor der Spur bestimmen
    let data_offset = match sector_size {
        2048 => 0,
        2336 => 8,
        _ => {
            let mut first = [0u8; 16];
            f.seek(SeekFrom::Start(start)).map_err(|e| e.to_string())?;
            f.read_exact(&mut first).map_err(|e| e.to_string())?;
            match sniff_layout(&first) {
                Some((_, off)) => off,
                None => return Err("NRG: rohe Sektoren ohne Sync-Muster".into()),
            }
        }
    };
    Ok(Analysis {
        kind: "nrg",
        data_file: path.to_path_buf(),
        layout: Some(Layout { sector_size, data_offset, start, end }),
    })
}

// ---------------------------------------------------------------------------
// Wandlung
// ---------------------------------------------------------------------------

/// Schreibt die Datenspur aus `analysis` als ISO nach `dst` (atomar über Temp-Datei).
pub fn write_iso(analysis: &Analysis, dst: &Path) -> Result<Report, String> {
    let layout = analysis.layout.ok_or("Abbild ist bereits ein ISO — keine Wandlung nötig")?;
    let mut src = BufReader::with_capacity(1 << 20, File::open(&analysis.data_file).map_err(|e| e.to_string())?);
    src.seek(SeekFrom::Start(layout.start)).map_err(|e| e.to_string())?;
    let tmp = dst.with_extension("iso.part");
    let mut out = BufWriter::with_capacity(1 << 20, File::create(&tmp).map_err(|e| format!("{}: {e}", tmp.display()))?);
    let mut sector = vec![0u8; layout.sector_size as usize];
    let off = layout.data_offset as usize;
    let mut written = 0u64;
    for _ in 0..layout.sectors() {
        if src.read_exact(&mut sector).is_err() {
            break;
        }
        out.write_all(&sector[off..off + ISO_SECTOR as usize]).map_err(|e| e.to_string())?;
        written += 1;
    }
    out.flush().map_err(|e| e.to_string())?;
    drop(out);
    if written == 0 {
        let _ = std::fs::remove_file(&tmp);
        return Err("Abbild enthält keine vollständigen Sektoren".into());
    }
    std::fs::rename(&tmp, dst).map_err(|e| format!("{}: {e}", dst.display()))?;
    Ok(Report { kind: analysis.kind, sectors: written, layout })
}

/// Bequem: analysieren und — falls nötig — nach `dst` wandeln. `Ok(None)` = direkt nutzbar.
pub fn convert_if_needed(src: &Path, dst: &Path) -> Result<Option<Report>, String> {
    let a = analyze(src)?;
    if a.layout.is_none() {
        return Ok(None);
    }
    write_iso(&a, dst).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("virtual-image-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// 20 ISO-Sektoren mit PVD in Sektor 16 und erkennbarem Inhalt.
    fn iso_data() -> Vec<u8> {
        let mut d = vec![0u8; 20 * 2048];
        for (i, chunk) in d.chunks_mut(2048).enumerate() {
            chunk[0] = i as u8;
            chunk[2047] = 0xEE;
        }
        d[16 * 2048] = 1;
        d[16 * 2048 + 1..16 * 2048 + 6].copy_from_slice(b"CD001");
        d
    }

    fn raw_sectors(data: &[u8], mode: u8) -> Vec<u8> {
        let mut out = Vec::new();
        for chunk in data.chunks(2048) {
            out.extend_from_slice(&SYNC);
            out.extend_from_slice(&[0, 2, 0, mode]);
            if mode == 2 {
                out.extend_from_slice(&[0, 0, 8, 0, 0, 0, 8, 0]); // Subheader Form 1
            }
            out.extend_from_slice(chunk);
            out.resize(out.len() + if mode == 2 { 280 } else { 288 }, 0);
        }
        out
    }

    fn nrg_v2_dao(raw: &[u8], sector_size: u16, mode: u8, pregap_sectors: u64) -> Vec<u8> {
        let mut f = vec![0u8; (pregap_sectors * sector_size as u64) as usize];
        let start = f.len() as u64;
        f.extend_from_slice(raw);
        let end = f.len() as u64;
        let chunks_at = f.len() as u64;
        let mut payload = vec![0u8; 22];
        payload[4..17].copy_from_slice(b"1234567890123");
        payload[20] = 1;
        payload[21] = 1;
        let mut e = vec![0u8; 42];
        e[12..14].copy_from_slice(&sector_size.to_be_bytes());
        e[14] = mode;
        e[18..26].copy_from_slice(&0u64.to_be_bytes());
        e[26..34].copy_from_slice(&start.to_be_bytes());
        e[34..42].copy_from_slice(&end.to_be_bytes());
        payload.extend_from_slice(&e);
        f.extend_from_slice(b"DAOX");
        f.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        f.extend_from_slice(&payload);
        f.extend_from_slice(b"END!");
        f.extend_from_slice(&0u32.to_be_bytes());
        f.extend_from_slice(b"NER5");
        f.extend_from_slice(&chunks_at.to_be_bytes());
        f
    }

    fn nrg_v1_etnf(data: &[u8], mode: u32) -> Vec<u8> {
        let mut f = data.to_vec();
        let chunks_at = f.len() as u32;
        let mut payload = Vec::new();
        payload.extend_from_slice(&0u32.to_be_bytes());
        payload.extend_from_slice(&(data.len() as u32).to_be_bytes());
        payload.extend_from_slice(&mode.to_be_bytes());
        payload.extend_from_slice(&0u32.to_be_bytes());
        payload.extend_from_slice(&0u32.to_be_bytes());
        f.extend_from_slice(b"ETNF");
        f.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        f.extend_from_slice(&payload);
        f.extend_from_slice(b"END!");
        f.extend_from_slice(&0u32.to_be_bytes());
        f.extend_from_slice(&0u32.to_be_bytes()); // Padding, damit NERO bei len-8 steht
        f.extend_from_slice(b"NERO");
        f.extend_from_slice(&chunks_at.to_be_bytes());
        f
    }

    #[test]
    fn plain_iso_needs_no_conversion() {
        let d = tempdir("plain_iso_needs_no_conversion");
        let p = d.join("plain.iso");
        std::fs::write(&p, iso_data()).unwrap();
        let a = analyze(&p).unwrap();
        assert_eq!(a.kind, "iso");
        assert!(a.layout.is_none());
        assert!(convert_if_needed(&p, &d.join("x.iso")).unwrap().is_none());
    }

    #[test]
    fn nrg_v2_dao_mode1_with_pregap() {
        let d = tempdir("nrg_v2_dao_mode1_with_pregap");
        let data = iso_data();
        let p = d.join("disc.nrg");
        std::fs::write(&p, nrg_v2_dao(&raw_sectors(&data, 1), 2352, 0x05, 150)).unwrap();
        let a = analyze(&p).unwrap();
        assert_eq!(a.kind, "nrg");
        let l = a.layout.unwrap();
        assert_eq!((l.sector_size, l.data_offset, l.start), (2352, 16, 150 * 2352));
        assert_eq!(l.sectors(), 20);
        let out = d.join(iso_name_for(&p));
        assert_eq!(iso_name_for(&p), "disc.iso");
        let r = write_iso(&a, &out).unwrap();
        assert_eq!(r.sectors, 20);
        assert_eq!(std::fs::read(&out).unwrap(), data);
    }

    #[test]
    fn nrg_v2_dao_mode2_form1() {
        let d = tempdir("nrg_v2_dao_mode2_form1");
        let data = iso_data();
        let p = d.join("m2.nrg");
        std::fs::write(&p, nrg_v2_dao(&raw_sectors(&data, 2), 2352, 0x06, 0)).unwrap();
        let a = analyze(&p).unwrap();
        assert_eq!(a.layout.unwrap().data_offset, 24);
        let out = d.join("m2.iso");
        write_iso(&a, &out).unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), data);
    }

    #[test]
    fn nrg_v1_etnf_plain_sectors() {
        let d = tempdir("nrg_v1_etnf_plain_sectors");
        let data = iso_data();
        let p = d.join("tao.nrg");
        std::fs::write(&p, nrg_v1_etnf(&data, 0x00)).unwrap();
        let a = analyze(&p).unwrap();
        let l = a.layout.unwrap();
        assert_eq!((l.sector_size, l.data_offset, l.start, l.end), (2048, 0, 0, data.len() as u64));
        let out = d.join("tao.iso");
        write_iso(&a, &out).unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), data);
    }

    #[test]
    fn bin_cue_mode1_2352() {
        let d = tempdir("bin_cue_mode1_2352");
        let data = iso_data();
        std::fs::write(d.join("game.bin"), raw_sectors(&data, 1)).unwrap();
        let cue = d.join("game.cue");
        std::fs::write(&cue, "FILE \"game.bin\" BINARY\n  TRACK 01 MODE1/2352\n    INDEX 01 00:00:00\n").unwrap();
        let a = analyze(&cue).unwrap();
        assert_eq!(a.kind, "cue");
        assert!(a.data_file.ends_with("game.bin"));
        let out = d.join("game.iso");
        let r = write_iso(&a, &out).unwrap();
        assert_eq!(r.sectors, 20);
        assert_eq!(std::fs::read(&out).unwrap(), data);
        // Audio zuerst → Fehler
        std::fs::write(&cue, "FILE \"game.bin\" BINARY\n  TRACK 01 AUDIO\n    INDEX 01 00:00:00\n").unwrap();
        assert!(analyze(&cue).is_err());
    }

    #[test]
    fn raw_bin_and_mdf_by_sniffing() {
        let d = tempdir("raw_bin_and_mdf_by_sniffing");
        let data = iso_data();
        let bin = d.join("loose.bin");
        std::fs::write(&bin, raw_sectors(&data, 2)).unwrap();
        let a = analyze(&bin).unwrap();
        assert_eq!(a.kind, "raw");
        assert_eq!(a.layout.unwrap().data_offset, 24);
        let mdf = d.join("alc.mdf");
        std::fs::write(&mdf, raw_sectors(&data, 1)).unwrap();
        std::fs::write(d.join("alc.mds"), b"MEDIA DESCRIPTOR").unwrap();
        let a = analyze(&d.join("alc.mds")).unwrap();
        assert_eq!(a.kind, "mdf");
        assert!(a.data_file.ends_with("alc.mdf"));
        let out = d.join("alc.iso");
        write_iso(&a, &out).unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), data);
    }

    #[test]
    fn garbage_is_rejected() {
        let d = tempdir("garbage_is_rejected");
        let p = d.join("noise.img");
        std::fs::write(&p, vec![0x5Au8; 100_000]).unwrap();
        assert!(analyze(&p).is_err());
        let n = d.join("noise.nrg");
        std::fs::write(&n, vec![0x5Au8; 100_000]).unwrap();
        assert!(analyze(&n).unwrap_err().contains("Nero-Kennung"));
    }
}
