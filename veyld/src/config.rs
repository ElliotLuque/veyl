//! Dónde vive cada cosa, y lo que Veyl tiene apuntado en settings.json.

use std::env;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;

pub fn home() -> PathBuf {
    env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| "/".into())
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| home().join(fallback))
}

/// Lo que Veyl recuerda: el diario, los ajustes, la agenda.
pub fn state() -> PathBuf {
    xdg("XDG_STATE_HOME", ".local/state").join("veyl")
}

/// Lo que se puede volver a hacer: las imágenes de las notificaciones.
pub fn cache() -> PathBuf {
    xdg("XDG_CACHE_HOME", ".cache").join("veyl")
}

/// Lo que escribe el usuario: esquemas propios, su gramática.
pub fn config() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join("veyl")
}

pub fn runtime() -> PathBuf {
    env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(env::temp_dir)
}

/// La sesión de Hyprland en la que se está.
pub fn signature() -> Option<String> {
    env::var("HYPRLAND_INSTANCE_SIGNATURE").ok().filter(|s| !s.is_empty())
}

/// El socket de veyld: uno por sesión de Hyprland, para que un Hyprland
/// anidado tenga el suyo. `VEYLD_SOCKET` lo fija a mano.
pub fn socket() -> PathBuf {
    if let Some(path) = env::var_os("VEYLD_SOCKET").filter(|v| !v.is_empty()) {
        return path.into();
    }
    let name = match signature() {
        Some(sig) => format!("veyld-{sig}.sock"),
        None => "veyld.sock".into(),
    };
    runtime().join("veyl").join(name)
}

/// Lo que veyld lee de settings.json (el mismo archivo que Veyl).
#[derive(Clone, Debug)]
pub struct Settings {
    /// Si se lleva el diario.
    pub journal: bool,
    /// La orden que contesta lo que la gramática no entiende (lo dicho, por
    /// la entrada estándar); vacía, no se le pregunta a nadie.
    pub llm: String,
    pub terminal: String,
    /// Los días que se guardan las notificaciones; 0, siempre.
    pub notification_days: u32,
    /// El modelo de voz (`"voice": {"model": "small-q5_1"}`, o una ruta);
    /// vacío, el mejor de los que haya.
    pub voice_model: String,
    /// El micro con el que se escucha (`"voice": {"mic": "…"}`, el nombre del
    /// nodo de PipeWire, como lo da `veyld mic`); vacío, el de por defecto.
    pub voice_mic: String,
    /// Si Veyl contesta en voz alta a lo que se le dice por voz
    /// (`"voice": {"speak": false}` lo calla), con qué voz de Piper y cuánto
    /// robot (de 0 a 1).
    pub speak: bool,
    pub speaker: String,
    pub robot: f32,
    /// A qué volumen habla, de 0 a 1 (`"voice": {"volume": 0.6}`).
    pub loudness: f32,
    /// Con qué motor habla («xtts», «piper»; vacío, el mejor que haya) y,
    /// con XTTS, a qué velocidad.
    pub engine: String,
    pub pace: f32,
    /// El cerebro (`"brain": {"provider", "model", "url"}`): quién contesta lo
    /// que la gramática no entiende. Vacío, nadie (o la orden de `llm`).
    pub brain: String,
    pub brain_model: String,
    /// Minutos sin hablar tras los que se suelta la voz (`"voice": {"unload": 10}`); 0, nunca.
    pub unload: u32,
    pub brain_url: String,
}

pub fn settings() -> Settings {
    let text = fs::read_to_string(state().join("settings.json")).unwrap_or_default();
    let json: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    let text = |key: &str| json.get(key).and_then(Value::as_str).unwrap_or("").trim().to_string();
    let terminal =
        Some(text("terminal")).filter(|t| !t.is_empty()).or_else(|| env::var("TERMINAL").ok().filter(|t| !t.is_empty())).unwrap_or_else(|| "kitty".into());
    Settings {
        journal: json.get("journal").and_then(Value::as_bool).unwrap_or(true),
        llm: text("llm"),
        terminal,
        notification_days: json.get("notificationDays").and_then(Value::as_u64).map(|d| d.min(36500) as u32).unwrap_or(7),
        voice_model: json.pointer("/voice/model").and_then(Value::as_str).unwrap_or("").trim().to_string(),
        voice_mic: json.pointer("/voice/mic").and_then(Value::as_str).unwrap_or("").trim().to_string(),
        speak: json.pointer("/voice/speak").and_then(Value::as_bool).unwrap_or(true),
        speaker: json.pointer("/voice/voz").and_then(Value::as_str).unwrap_or("").trim().to_string(),
        robot: json.pointer("/voice/robot").and_then(Value::as_f64).map(|r| r.clamp(0.0, 1.0) as f32).unwrap_or(1.0),
        loudness: json.pointer("/voice/volume").and_then(Value::as_f64).map(|v| v.clamp(0.0, 1.0) as f32).unwrap_or(0.6),
        engine: json.pointer("/voice/engine").and_then(Value::as_str).unwrap_or("").trim().to_string(),
        pace: json.pointer("/voice/speed").and_then(Value::as_f64).map(|v| v.clamp(0.5, 2.0) as f32).unwrap_or(1.2),
        unload: json.pointer("/voice/unload").and_then(Value::as_u64).map(|m| m.min(1440) as u32).unwrap_or(0),
        brain: json.pointer("/brain/provider").and_then(Value::as_str).unwrap_or("").trim().to_string(),
        brain_model: json.pointer("/brain/model").and_then(Value::as_str).unwrap_or("").trim().to_string(),
        brain_url: json.pointer("/brain/url").and_then(Value::as_str).unwrap_or("").trim().to_string(),
    }
}

/// Una ruta como se escribe: «~/dev/veyl».
impl Settings {
    /// Cómo habla Veyl.
    pub fn style(&self) -> crate::tts::Style {
        crate::tts::Style {
            engine: self.engine.clone(),
            voice: self.speaker.clone(),
            speed: self.pace,
            robot: self.robot,
            volume: self.loudness,
            unload: self.unload,
        }
    }
}

pub fn tilde(path: &str) -> String {
    let home = home();
    let home = home.to_string_lossy();
    match path.strip_prefix(home.as_ref()) {
        Some("") => "~".into(),
        Some(rest) if rest.starts_with('/') => format!("~{rest}"),
        _ => path.to_string(),
    }
}
