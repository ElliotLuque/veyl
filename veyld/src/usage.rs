//! Lo que queda de cuota en las suscripciones de los agentes, para el centro
//! de control. Nada de pedírselo a nadie con sus credenciales: cada CLI ya lo
//! cuenta. Codex apunta sus límites en cada sesión (`~/.codex/sessions/…
//! .jsonl`, «rate_limits»); Claude Code se los pasa a su línea de estado, y
//! `veyld statusline` (puesto como `statusLine` en ~/.claude/settings.json)
//! los guarda en `$XDG_STATE_HOME/veyl/usage/claude.json`.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::config;

fn claude_file() -> PathBuf {
    config::state().join("usage").join("claude.json")
}

fn now() -> i64 {
    crate::daemon::now()
}

/// Un instante en segundos o milisegundos desde 1970, o en ISO 8601: en ms.
fn instant(value: &Value) -> Option<i64> {
    match value {
        Value::Number(n) => n.as_f64().map(|v| if v < 1e12 { (v * 1000.0) as i64 } else { v as i64 }),
        Value::String(s) => chrono::DateTime::parse_from_rfc3339(s).ok().map(|t| t.timestamp_millis()),
        _ => None,
    }
}

/// Una ventana de cuota: lo gastado (0–100) y cuándo se renueva. Si ya se
/// renovó desde que se apuntó, está entera.
fn window(label: &str, raw: &Value) -> Option<Value> {
    let used = raw.get("used_percentage").or_else(|| raw.get("used_percent")).and_then(Value::as_f64)?;
    let resets = raw.get("resets_at").and_then(instant);
    let renewed = resets.is_some_and(|at| at <= now());
    Some(json!({
        "label": label,
        "used": if renewed { 0.0 } else { used.clamp(0.0, 100.0) },
        "resets": if renewed { Value::Null } else { json!(resets) },
    }))
}

/// Lo que guarda la línea de estado de Claude Code.
fn claude() -> Option<Value> {
    let saved: Value = serde_json::from_str(&fs::read_to_string(claude_file()).ok()?).ok()?;
    let limits = saved.get("rate_limits")?;
    let windows: Vec<Value> =
        [("5 h", "five_hour"), ("Semana", "seven_day"), ("Opus", "seven_day_opus")].iter().filter_map(|(label, key)| window(label, limits.get(key)?)).collect();
    (!windows.is_empty()).then(|| json!({ "provider": "anthropic", "name": "Claude Code", "at": saved["at"], "windows": windows }))
}

/// Las sesiones de Codex, de la más reciente a la más antigua (año/mes/día/archivo).
fn codex_sessions(root: &Path) -> Vec<PathBuf> {
    fn sorted(dir: &Path) -> Vec<PathBuf> {
        let mut list: Vec<PathBuf> = fs::read_dir(dir).map(|it| it.flatten().map(|e| e.path()).collect()).unwrap_or_default();
        list.sort();
        list.reverse();
        list
    }
    let mut out = Vec::new();
    for year in sorted(root) {
        for month in sorted(&year) {
            for day in sorted(&month) {
                out.extend(sorted(&day).into_iter().filter(|f| f.extension().is_some_and(|e| e == "jsonl")));
                if out.len() >= 8 {
                    return out;
                }
            }
        }
    }
    out
}

/// Lo último que Codex apuntó de sus límites.
fn codex() -> Option<Value> {
    let root = config::home().join(".codex").join("sessions");
    for file in codex_sessions(&root) {
        let Ok(text) = fs::read_to_string(&file) else { continue };
        let Some(line) = text.lines().rev().find(|l| l.contains("\"rate_limits\"")) else { continue };
        let Ok(event) = serde_json::from_str::<Value>(line) else { continue };
        let limits = find(&event, "rate_limits")?;
        let label = |raw: &Value, fallback: &str| match raw.get("window_minutes").and_then(Value::as_i64) {
            Some(m) if m >= 7 * 24 * 60 => "Semana".to_string(),
            Some(m) if m >= 60 => format!("{} h", m / 60),
            Some(m) => format!("{m} min"),
            None => fallback.to_string(),
        };
        let windows: Vec<Value> = [("primary", "5 h"), ("secondary", "Semana")]
            .iter()
            .filter_map(|(key, fallback)| limits.get(key).and_then(|w| window(&label(w, fallback), w)))
            .collect();
        let at = fs::metadata(&file).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64);
        return (!windows.is_empty()).then(|| json!({ "provider": "openai", "name": "Codex", "at": at, "windows": windows }));
    }
    None
}

/// La primera clave `key` en cualquier nivel.
fn find<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(map) => map.get(key).or_else(|| map.values().find_map(|v| find(v, key))),
        Value::Array(list) => list.iter().find_map(|v| find(v, key)),
        _ => None,
    }
}

/// La cuota de cada suscripción que se sepa.
pub fn all() -> Value {
    json!([claude(), codex()].into_iter().flatten().collect::<Vec<_>>())
}

/// `veyld statusline`: la línea de estado de Claude Code. Guarda sus límites
/// y escribe una línea corta (el modelo y la carpeta).
pub fn statusline() {
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    let Ok(status) = serde_json::from_str::<Value>(&input) else {
        return;
    };
    if let Some(limits) = status.get("rate_limits").filter(|l| l.is_object()) {
        let file = claude_file();
        if let Some(dir) = file.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let saved = json!({ "at": now(), "rate_limits": limits });
        let partial = file.with_extension("json.part");
        if fs::write(&partial, saved.to_string()).is_ok() {
            let _ = fs::rename(&partial, &file);
        }
    }
    let model = status.pointer("/model/display_name").and_then(Value::as_str).unwrap_or("");
    let folder = status.pointer("/workspace/current_dir").or_else(|| status.get("cwd")).and_then(Value::as_str).map(config::tilde).unwrap_or_default();
    let left =
        status.pointer("/rate_limits/five_hour").and_then(|w| window("5 h", w)).map(|w| format!("{:.0} % libre", 100.0 - w["used"].as_f64().unwrap_or(0.0)));
    println!("{}", [model.to_string(), folder, left.unwrap_or_default()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · "));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_renew_and_take_any_instant() {
        let future = now() / 1000 + 3600;
        let w = window("5 h", &json!({ "used_percentage": 42.5, "resets_at": future })).unwrap();
        assert_eq!((w["used"].as_f64(), w["resets"].as_i64()), (Some(42.5), Some(future * 1000)));
        let gone = window("5 h", &json!({ "used_percent": 80, "resets_at": 1000 })).unwrap();
        assert_eq!((gone["used"].as_f64(), gone["resets"].is_null()), (Some(0.0), true));
        assert_eq!(instant(&json!("2026-01-01T00:00:00Z")), Some(1767225600000));
    }

    #[test]
    fn codex_limits_anywhere_in_the_event() {
        let event = json!({ "payload": { "type": "token_count", "rate_limits": { "primary": { "used_percent": 5.0, "window_minutes": 300 }, "secondary": { "used_percent": 69.0, "window_minutes": 10080 } } } });
        let limits = find(&event, "rate_limits").unwrap();
        assert_eq!(limits["secondary"]["used_percent"].as_f64(), Some(69.0));
    }
}
