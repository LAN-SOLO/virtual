//! QMP (QEMU Machine Protocol) — reine Nachrichtenlogik: Kommandos bauen,
//! Zeilen parsen. Der Transport (Unix-Socket / TCP) liegt im Tauri-Teil.

use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    /// Begrüßung nach dem Verbinden — danach muss `qmp_capabilities` folgen.
    Greeting { version: String },
    /// Antwort auf ein Kommando (Feld `return`), mit optionaler `id`.
    Return { id: Option<String>, value: Value },
    /// Fehlerantwort (`error.class`, `error.desc`).
    Error { id: Option<String>, class: String, desc: String },
    /// Asynchrones Ereignis (`SHUTDOWN`, `STOP`, `RESUME`, `RESET`, `POWERDOWN`, `JOB_STATUS_CHANGE` …).
    Event { name: String, data: Value },
}

/// Kommandozeile als JSON-Text (eine Zeile, ohne Zeilenumbruch).
pub fn command(name: &str, args: Option<Value>, id: Option<&str>) -> String {
    let mut v = json!({ "execute": name });
    if let Some(a) = args {
        v["arguments"] = a;
    }
    if let Some(i) = id {
        v["id"] = Value::String(i.into());
    }
    v.to_string()
}

pub fn capabilities() -> String {
    command("qmp_capabilities", None, Some("caps"))
}

pub fn query_status() -> String {
    command("query-status", None, Some("status"))
}

pub fn stop() -> String {
    command("stop", None, Some("stop"))
}

pub fn cont() -> String {
    command("cont", None, Some("cont"))
}

pub fn system_reset() -> String {
    command("system_reset", None, Some("reset"))
}

/// ACPI-Ausschalten — der Gast darf sauber herunterfahren.
pub fn system_powerdown() -> String {
    command("system_powerdown", None, Some("powerdown"))
}

pub fn quit() -> String {
    command("quit", None, Some("quit"))
}

/// Interner qcow2-Snapshot im laufenden Betrieb (QEMU ≥ 6.0, Job-basiert).
pub fn snapshot_save(job_id: &str, tag: &str, vmstate_node: &str, devices: &[String]) -> String {
    command(
        "snapshot-save",
        Some(json!({ "job-id": job_id, "tag": tag, "vmstate": vmstate_node, "devices": devices })),
        Some(job_id),
    )
}

pub fn snapshot_load(job_id: &str, tag: &str, vmstate_node: &str, devices: &[String]) -> String {
    command(
        "snapshot-load",
        Some(json!({ "job-id": job_id, "tag": tag, "vmstate": vmstate_node, "devices": devices })),
        Some(job_id),
    )
}

pub fn snapshot_delete(job_id: &str, tag: &str, devices: &[String]) -> String {
    command(
        "snapshot-delete",
        Some(json!({ "job-id": job_id, "tag": tag, "devices": devices })),
        Some(job_id),
    )
}

pub fn query_jobs() -> String {
    command("query-jobs", None, Some("jobs"))
}

pub fn job_dismiss(job_id: &str) -> String {
    command("job-dismiss", Some(json!({ "id": job_id })), Some("dismiss"))
}

/// Blockgeräte mit Node-Namen — nötig, um `vmstate`/`devices` für snapshot-save zu finden.
pub fn query_named_block_nodes() -> String {
    command("query-named-block-nodes", None, Some("nodes"))
}

pub fn query_block() -> String {
    command("query-block", None, Some("block"))
}

/// Medium wechseln (CD einlegen) — `blockdev-change-medium` mit Dateipfad.
pub fn change_medium(device: &str, file: &str) -> String {
    command(
        "blockdev-change-medium",
        Some(json!({ "id": device, "filename": file, "format": "raw", "read-only-mode": "read-only" })),
        Some("medium"),
    )
}

pub fn eject(device: &str) -> String {
    command("eject", Some(json!({ "id": device, "force": true })), Some("eject"))
}

pub fn screendump(path: &str) -> String {
    command("screendump", Some(json!({ "filename": path })), Some("screendump"))
}

/// Eine empfangene Zeile deuten.
pub fn parse_line(line: &str) -> Option<Message> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    if let Some(q) = v.get("QMP") {
        let version = q
            .get("version")
            .and_then(|x| x.get("qemu"))
            .map(|x| {
                format!(
                    "{}.{}.{}",
                    x.get("major").and_then(Value::as_u64).unwrap_or(0),
                    x.get("minor").and_then(Value::as_u64).unwrap_or(0),
                    x.get("micro").and_then(Value::as_u64).unwrap_or(0)
                )
            })
            .unwrap_or_default();
        return Some(Message::Greeting { version });
    }
    let id = v.get("id").and_then(Value::as_str).map(String::from);
    if let Some(r) = v.get("return") {
        return Some(Message::Return { id, value: r.clone() });
    }
    if let Some(e) = v.get("error") {
        return Some(Message::Error {
            id,
            class: e.get("class").and_then(Value::as_str).unwrap_or("").into(),
            desc: e.get("desc").and_then(Value::as_str).unwrap_or("").into(),
        });
    }
    if let Some(ev) = v.get("event").and_then(Value::as_str) {
        return Some(Message::Event { name: ev.into(), data: v.get("data").cloned().unwrap_or(Value::Null) });
    }
    None
}

/// Aus `query-status`: `running` | `paused` | `shutdown` | `prelaunch` …
pub fn status_from_return(value: &Value) -> String {
    value.get("status").and_then(Value::as_str).unwrap_or("unknown").to_string()
}

/// Aus `query-named-block-nodes`: alle qcow2-Format-Nodes (für vmstate/devices).
pub fn qcow2_nodes(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter(|n| n.get("drv").and_then(Value::as_str) == Some("qcow2"))
                .filter_map(|n| n.get("node-name").and_then(Value::as_str).map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// Aus `query-jobs`: Status eines Jobs (`concluded`, `running`, …) und Fehlertext.
pub fn job_state(value: &Value, job_id: &str) -> Option<(String, Option<String>)> {
    value.as_array()?.iter().find_map(|j| {
        (j.get("id").and_then(Value::as_str) == Some(job_id)).then(|| {
            (
                j.get("status").and_then(Value::as_str).unwrap_or("").to_string(),
                j.get("error").and_then(Value::as_str).map(String::from),
            )
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_commands() {
        assert_eq!(capabilities(), r#"{"execute":"qmp_capabilities","id":"caps"}"#);
        let s = snapshot_save("snap1", "clean", "disk0", &["disk0".into()]);
        let v: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["execute"], "snapshot-save");
        assert_eq!(v["arguments"]["tag"], "clean");
        assert_eq!(v["arguments"]["devices"][0], "disk0");
    }

    #[test]
    fn parses_greeting_return_error_event() {
        let g = parse_line(r#"{"QMP": {"version": {"qemu": {"micro": 0, "minor": 2, "major": 9}, "package": ""}, "capabilities": ["oob"]}}"#).unwrap();
        assert_eq!(g, Message::Greeting { version: "9.2.0".into() });
        let r = parse_line(r#"{"return": {"status": "running", "running": true}, "id": "status"}"#).unwrap();
        match r {
            Message::Return { id, value } => {
                assert_eq!(id.as_deref(), Some("status"));
                assert_eq!(status_from_return(&value), "running");
            }
            _ => panic!(),
        }
        let e = parse_line(r#"{"error": {"class": "GenericError", "desc": "nope"}, "id": "x"}"#).unwrap();
        assert!(matches!(e, Message::Error { class, .. } if class == "GenericError"));
        let ev = parse_line(r#"{"timestamp": {"seconds": 1, "microseconds": 2}, "event": "SHUTDOWN", "data": {"guest": true}}"#).unwrap();
        assert!(matches!(ev, Message::Event { name, .. } if name == "SHUTDOWN"));
        assert!(parse_line("garbage").is_none());
    }

    #[test]
    fn extracts_qcow2_nodes_and_job_state() {
        let nodes: Value = serde_json::from_str(r##"[{"node-name":"disk0","drv":"qcow2"},{"node-name":"#block123","drv":"file"}]"##).unwrap();
        assert_eq!(qcow2_nodes(&nodes), vec!["disk0".to_string()]);
        let jobs: Value = serde_json::from_str(r#"[{"id":"snap1","status":"concluded","type":"snapshot-save"}]"#).unwrap();
        assert_eq!(job_state(&jobs, "snap1"), Some(("concluded".into(), None)));
        assert_eq!(job_state(&jobs, "other"), None);
    }
}
