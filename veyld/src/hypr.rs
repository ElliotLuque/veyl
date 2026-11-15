//! Hyprland, por sus dos sockets: el de las preguntas y órdenes
//! (`.socket.sock`, lo mismo que usa hyprctl) y el de los avisos
//! (`.socket2.sock`, una línea `evento>>datos` por cosa que pasa).

use std::io::{self, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;

use crate::config;

#[derive(Clone, Debug)]
pub struct Hypr {
    dir: PathBuf,
}

/// Una ventana, lo que de ella interesa.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Window {
    /// «0x…»
    pub address: String,
    pub class: String,
    pub title: String,
    pub pid: i32,
    pub workspace: String,
    pub workspace_id: i64,
    pub monitor: i64,
    /// 0 es la que tiene el foco; cuanto mayor, más tiempo hace.
    pub recency: i64,
}

impl Window {
    fn from(json: &Value) -> Option<Window> {
        let text = |key: &str| json.get(key).and_then(Value::as_str).unwrap_or("").to_string();
        let address = text("address");
        if address.is_empty() || address == "0x0" {
            return None;
        }
        let workspace = json.get("workspace");
        Some(Window {
            address,
            class: text("class"),
            title: text("title"),
            pid: json.get("pid").and_then(Value::as_i64).unwrap_or(0) as i32,
            workspace: workspace.and_then(|w| w.get("name")).and_then(Value::as_str).unwrap_or("").to_string(),
            workspace_id: workspace.and_then(|w| w.get("id")).and_then(Value::as_i64).unwrap_or(0),
            monitor: json.get("monitor").and_then(Value::as_i64).unwrap_or(-1),
            recency: json.get("focusHistoryID").and_then(Value::as_i64).unwrap_or(i64::MAX),
        })
    }

    /// La aplicación, como se nombra en corto: «org.mozilla.firefox» es «firefox».
    pub fn app(&self) -> &str {
        self.class.rsplit('.').next().unwrap_or(&self.class)
    }
}

impl Hypr {
    #[cfg(test)]
    pub fn offline() -> Hypr {
        Hypr { dir: PathBuf::from("/nonexistent/veyl-test") }
    }

    /// La sesión de Hyprland de este entorno, si está viva.
    pub fn find() -> Option<Hypr> {
        let dir = config::runtime().join("hypr").join(config::signature()?);
        let hypr = Hypr { dir };
        hypr.alive().then_some(hypr)
    }

    pub fn alive(&self) -> bool {
        UnixStream::connect(self.dir.join(".socket.sock")).is_ok()
    }

    pub fn ask(&self, request: &str) -> io::Result<String> {
        let mut socket = UnixStream::connect(self.dir.join(".socket.sock"))?;
        socket.set_read_timeout(Some(Duration::from_secs(3)))?;
        socket.set_write_timeout(Some(Duration::from_secs(3)))?;
        socket.write_all(request.as_bytes())?;
        let mut answer = String::new();
        socket.read_to_string(&mut answer)?;
        Ok(answer)
    }

    pub fn json(&self, what: &str) -> Option<Value> {
        serde_json::from_str(&self.ask(&format!("j/{what}")).ok()?).ok()
    }

    /// Lua en la configuración viva; Hyprland contesta «ok» o el error.
    pub fn eval(&self, lua: &str) -> Result<(), String> {
        let answer = self.ask(&format!("eval {lua}")).map_err(|e| format!("Hyprland no contesta: {e}"))?;
        match answer.trim() {
            "ok" => Ok(()),
            error => Err(format!("Hyprland: {error}")),
        }
    }

    /// Un despachador de `hl.dsp`, ya escrito: `hl.dsp.window.close()`.
    pub fn dispatch(&self, dispatcher: &str) -> Result<(), String> {
        self.eval(&format!("hl.dispatch({dispatcher})"))
    }

    /// El valor de una expresión Lua.
    pub fn repl(&self, lua: &str) -> Option<String> {
        self.ask(&format!("repl {lua}")).ok().map(|s| s.trim().to_string())
    }

    pub fn events(&self) -> io::Result<BufReader<UnixStream>> {
        Ok(BufReader::new(UnixStream::connect(self.dir.join(".socket2.sock"))?))
    }

    pub fn clients(&self) -> Vec<Window> {
        let Some(Value::Array(all)) = self.json("clients") else {
            return Vec::new();
        };
        all.iter().filter_map(Window::from).collect()
    }

    pub fn active(&self) -> Option<Window> {
        Window::from(&self.json("activewindow")?)
    }

    /// El nombre del monitor con el foco.
    pub fn monitor(&self) -> Option<String> {
        let Value::Array(all) = self.json("monitors")? else {
            return None;
        };
        let focused = all.iter().find(|m| m.get("focused").and_then(Value::as_bool) == Some(true))?;
        Some(focused.get("name")?.as_str()?.to_string())
    }

    /// Cuántos niveles tiene cada monitor con hyprsplit (0 = sin él).
    pub fn split(&self) -> u32 {
        self.repl(r#"local ok, h = pcall(require, "hyprsplit") return ok and h._config.num_workspaces or 0"#).and_then(|s| s.parse().ok()).unwrap_or(0)
    }
}

/// Un texto como cadena de Lua.
pub fn lua(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
