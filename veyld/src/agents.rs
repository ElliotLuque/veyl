//! Qué agentes hay en marcha en esta máquina y en qué están.
//!
//! Mira los procesos cada segundo y reconoce los agentes conocidos (Claude
//! Code, Codex, Gemini, Antigravity, opencode, pi, Aider…), los lancen desde
//! una terminal, un editor o donde sea. Lo fino lo cuenta el propio agente
//! cuando lo publica (Claude Code en ~/.claude/sessions, Codex en su rollout,
//! Antigravity en ~/.gemini/antigravity-cli); si no, se deduce de cuánto
//! trabaja su árbol de procesos. Si vive en un panel de herdr, manda lo que
//! herdr ve en su pantalla (trabaja, espera, te necesita): es lo más fiable,
//! porque Claude Code deja su estado sin actualizar en las sesiones que pasan
//! a otro proceso. Y si te necesita, se lee de su pantalla qué te pregunta y
//! con qué opciones, para contestar desde Veyl con una tecla (herdr o tmux).

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use serde_json::Value;

use crate::config;
use crate::daemon::Event;
use crate::procs::{self, Stat};

const TICK: Duration = Duration::from_secs(1);
/// Fracción de CPU de su árbol a partir de la cual trabaja.
const BUSY: f64 = 0.04;
/// Segundos sin trabajar para darlo por parado.
const CALM: f64 = 4.0;

/// Nombre del programa → (proveedor, nombre visible).
const KNOWN: [(&str, &str, &str); 15] = [
    ("claude", "anthropic", "Claude"),
    ("codex", "openai", "Codex"),
    ("gemini", "google", "Gemini"),
    ("agy", "google", "Antigravity"),
    ("antigravity-cli", "google", "Antigravity"),
    ("opencode", "opencode", "opencode"),
    ("aider", "aider", "Aider"),
    ("cursor-agent", "cursor", "Cursor"),
    ("copilot", "github", "Copilot"),
    ("goose", "goose", "Goose"),
    ("amp", "amp", "Amp"),
    ("crush", "crush", "Crush"),
    ("qwen", "qwen", "Qwen"),
    ("droid", "factory", "Droid"),
    ("pi", "pi", "pi"),
];
/// Intérpretes: el agente es el guion que ejecutan.
const HOSTS: [&str; 5] = ["node", "bun", "deno", "python", "python3"];

/// Un agente, como lo espera Veyl (state/Agents.qml).
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct Agent {
    pub id: String,
    pub provider: String,
    pub label: String,
    pub name: String,
    pub cwd: String,
    /// working, needs o idle.
    pub phase: String,
    pub detail: String,
    pub model: String,
    /// La herramienta que está usando ahora.
    pub tool: String,
    /// Desde cuándo está en esa fase (ms).
    pub since: i64,
    /// Los procesos de los que cuelga, del agente hacia arriba: así Veyl
    /// encuentra su ventana.
    pub chain: Vec<i32>,
    /// Las órdenes que llevan, dentro de esa ventana, a su pestaña.
    pub focus: Vec<Vec<String>>,
    /// Lo que te pregunta, si te necesita y se ve en su terminal.
    pub ask: Option<Ask>,
    /// La orden que manda una tecla a su terminal (sin la tecla), si se puede.
    #[serde(skip)]
    pub keys: Vec<String>,
}

/// Lo que pregunta un agente que te necesita: de qué va (`title`, la
/// herramienta: «Bash command», «Edit file»), sobre qué (`subject`: la orden,
/// el archivo), lo que añade (`note`: su descripción), la pregunta y sus
/// opciones, cada una con su tecla.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct Ask {
    pub title: String,
    pub subject: String,
    pub note: String,
    pub question: String,
    pub options: Vec<Choice>,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct Choice {
    pub key: String,
    pub label: String,
    /// Lo que la explica, debajo, si la trae (en una pregunta de varias opciones).
    pub detail: String,
}

/// Lo que el agente cuenta de sí mismo.
#[derive(Clone, Debug, Default)]
struct Told {
    phase: String,
    detail: String,
    since: Option<i64>,
    name: String,
    cwd: String,
    model: String,
    tool: String,
}

struct Seen {
    busy: f64,
    phase: &'static str,
    since: f64,
    ticks: u64,
    at: f64,
    start: u64,
}

fn now() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

fn clock_ticks() -> f64 {
    // SAFETY: sysconf solo lee una constante del sistema.
    unsafe extern "C" {
        fn sysconf(name: i32) -> i64;
    }
    const SC_CLK_TCK: i32 = 2;
    match unsafe { sysconf(SC_CLK_TCK) } {
        n if n > 0 => n as f64,
        _ => 100.0,
    }
}

fn program_of(args: &[String]) -> Option<&'static (&'static str, &'static str, &'static str)> {
    for arg in args.iter().take(3) {
        // pi cambia el título de su proceso a «pi» relleno de espacios.
        let arg = arg.trim();
        let mut name = arg.rsplit('/').next().unwrap_or(arg).to_string();
        // NixOS envuelve los binarios: «.claude-wrapped» es «claude».
        if let Some(inner) = name.strip_prefix('.').and_then(|n| n.strip_suffix("-wrapped")) {
            name = inner.to_string();
        }
        let name = name.trim_end_matches(".js").trim_end_matches(".mjs");
        if let Some(known) = KNOWN.iter().find(|k| k.0 == name) {
            return Some(known);
        }
        if !HOSTS.contains(&name.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.')) {
            return None;
        }
    }
    None
}

/// El agente conocido que es este proceso.
fn program(pid: i32) -> Option<&'static (&'static str, &'static str, &'static str)> {
    program_of(&procs::argv(pid))
}

/// El valor de `--model x` o `--model=x`.
fn flag(args: &[String], names: &[&str]) -> String {
    for (i, arg) in args.iter().enumerate() {
        for name in names {
            if arg == name
                && let Some(next) = args.get(i + 1)
            {
                return next.clone();
            }
            if let Some(value) = arg.strip_prefix(&format!("{name}=")) {
                return value.to_string();
            }
        }
    }
    String::new()
}

/// Las últimas líneas JSON de un archivo, de la más nueva a la más vieja.
fn tail(path: &Path, size: u64) -> Vec<Value> {
    let Ok(mut file) = File::open(path) else {
        return Vec::new();
    };
    let end = file.seek(SeekFrom::End(0)).unwrap_or(0);
    let _ = file.seek(SeekFrom::Start(end.saturating_sub(size)));
    let mut bytes = Vec::new();
    let _ = file.take(size).read_to_end(&mut bytes);
    let text = String::from_utf8_lossy(&bytes);
    let mut lines: Vec<&str> = text.lines().collect();
    if end > size && !lines.is_empty() {
        lines.remove(0);
    }
    lines.iter().rev().filter_map(|line| serde_json::from_str(line).ok()).collect()
}

/// El panel de un multiplexor en el que vive un agente: para leer su
/// pantalla y mandarle teclas.
#[derive(Clone, Debug, PartialEq)]
enum Pane {
    Herdr { bin: String, socket: String, id: String },
    Tmux(String),
}

impl Pane {
    fn of(env: &HashMap<String, String>) -> Option<Pane> {
        let get = |key: &str| env.get(key).filter(|v| !v.is_empty()).cloned();
        if let Some(id) = get("HERDR_PANE_ID") {
            return Some(Pane::Herdr {
                bin: get("HERDR_BIN_PATH").unwrap_or_else(|| "herdr".into()),
                socket: get("HERDR_SOCKET_PATH").unwrap_or_default(),
                id,
            });
        }
        get("TMUX_PANE").map(Pane::Tmux)
    }

    fn command(&self, args: &[&str]) -> Command {
        match self {
            Pane::Herdr { bin, socket, .. } => {
                let mut cmd = Command::new(bin);
                cmd.args(args);
                if !socket.is_empty() {
                    cmd.env("HERDR_SOCKET_PATH", socket);
                }
                cmd
            }
            Pane::Tmux(_) => {
                let mut cmd = Command::new("tmux");
                cmd.args(args);
                cmd
            }
        }
    }

    /// Lo que se ve ahora en su pantalla.
    fn screen(&self) -> String {
        let mut cmd = match self {
            Pane::Herdr { id, .. } => self.command(&["pane", "read", id, "--source", "visible", "--format", "text"]),
            Pane::Tmux(id) => self.command(&["capture-pane", "-p", "-t", id]),
        };
        crate::process::output(&mut cmd, b"", Duration::from_secs(2)).map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default()
    }

    /// La orden que manda una tecla, sin la tecla.
    fn keys(&self) -> Vec<String> {
        match self {
            Pane::Herdr { bin, socket, id } => {
                let mut out = Vec::new();
                if !socket.is_empty() {
                    out.extend(["env".to_string(), format!("HERDR_SOCKET_PATH={socket}")]);
                }
                out.extend([bin.clone(), "pane".into(), "send-keys".into(), id.clone()]);
                out
            }
            Pane::Tmux(id) => vec!["tmux".into(), "send-keys".into(), "-t".into(), id.clone()],
        }
    }
}

/// El estado de cada panel de herdr con un agente: panel → idle, working,
/// blocked, done o unknown.
fn herdr_states(pane: &Pane) -> HashMap<String, String> {
    let Ok(out) = crate::process::output(&mut pane.command(&["agent", "list"]), b"", Duration::from_secs(2)) else {
        return HashMap::new();
    };
    let reply: Value = serde_json::from_slice(&out.stdout).unwrap_or(Value::Null);
    reply
        .pointer("/result/agents")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|a| Some((a.get("pane_id")?.as_str()?.to_string(), a.get("agent_status")?.as_str()?.to_string())))
        .collect()
}

/// Una opción de un diálogo: su tecla y lo que dice, de una línea como
/// «❯ 1. Yes», «2) No», «[y] Allow» o «(n) Deny». Una tecla por línea; si la
/// línea trae varias en fila («[Y] Permitir  [A] Toda la sesión  [N]
/// Denegar»), todas.
fn choices(line: &str) -> Vec<(String, String)> {
    let line = line.trim_start_matches(|c: char| "❯›>▶▸➜→•".contains(c) || c.is_whitespace());
    // En fila, con corchetes: «[Y] Permitir  [A] Toda la sesión».
    if line.starts_with('[') || line.starts_with('(') {
        let mut out = Vec::new();
        let mut rest = line;
        while let Some(open) = rest.find(['[', '(']) {
            let close = if rest[open..].starts_with('[') { ']' } else { ')' };
            let Some(end) = rest[open..].find(close).map(|e| open + e) else { break };
            let key = rest[open + 1..end].trim();
            let after = &rest[end + 1..];
            let next = after.find(['[', '(']).unwrap_or(after.len());
            let label = after[..next].trim();
            if key.chars().count() != 1 || !key.chars().all(|c| c.is_ascii_alphanumeric()) || label.is_empty() {
                return Vec::new();
            }
            out.push((key.to_string(), label.to_string()));
            rest = &after[next..];
        }
        return out;
    }
    // Una por línea: «1. Yes», «2) No».
    let mut chars = line.chars();
    let (Some(key), Some(mark)) = (chars.next(), chars.next()) else { return Vec::new() };
    let label = chars.as_str().trim();
    if key.is_ascii_digit() && (mark == '.' || mark == ')') && !label.is_empty() { vec![(key.to_string(), label.to_string())] } else { Vec::new() }
}

/// Lo que pregunta un agente en su pantalla: el último bloque de opciones
/// (numeradas, «❯ 1. Yes», o con su letra, «[y] Allow»), la pregunta encima
/// («Do you want to proceed?», «Allow command?») y de qué va: el título del
/// diálogo («Bash command», «Edit file»), sobre qué (la orden, el archivo) y
/// lo que añade (su descripción). `None` si no se ve ninguno.
pub fn asked(screen: &str) -> Option<Ask> {
    let raw: Vec<&str> = screen.lines().collect();
    let lines: Vec<String> = raw.iter().map(|l| l.trim_matches(|c: char| c.is_whitespace() || "│┃║╭╮╰╯▌▐".contains(c)).to_string()).collect();
    // Rayas: la del borde del diálogo, maciza; las de dentro (el diff), a trazos.
    let solid = |l: &str| !l.is_empty() && l.chars().all(|c| "─━═-—_".contains(c) || c.is_whitespace());
    let dashed = |l: &str| !l.is_empty() && l.chars().all(|c| "╌┄┈╍┅┉╴╶".contains(c) || c.is_whitespace());
    // Lo sangrada que va una línea, dentro del recuadro si lo hay.
    let indent = |i: usize| raw[i].trim_start_matches(|c: char| "│┃║▌".contains(c)).chars().take_while(|c| c.is_whitespace()).count();

    // El último bloque de opciones: sus líneas y dónde empieza y acaba.
    let mut end = (0..lines.len()).rev().find(|&i| !choices(&lines[i]).is_empty())?;
    let mut start = end;
    let mut options: Vec<Choice> = Vec::new();
    // Hacia arriba, por las opciones y por lo que sigue a una de ellas (sangrado).
    let mut j = end;
    while j > 0 {
        let prev = j - 1;
        if !choices(&lines[prev]).is_empty() {
            start = prev;
        } else if lines[prev].is_empty()
            || solid(&lines[prev])
            || dashed(&lines[prev])
            || indent(prev) < 2
            || !(prev.saturating_sub(2)..prev).any(|k| !choices(&lines[k]).is_empty())
        {
            break;
        }
        j = prev;
    }
    // Una opción larga sigue, sangrada, en la línea de abajo: si empieza en
    // minúscula o con un signo es el resto de su texto; en mayúscula, su descripción.
    for (i, line) in lines.iter().enumerate().skip(start) {
        let found = choices(line);
        if !found.is_empty() {
            options.extend(found.into_iter().map(|(key, label)| Choice { key, label, detail: String::new() }));
            end = i;
            continue;
        }
        if line.is_empty() || solid(line) || dashed(line) || indent(i) < 2 || i > end + 2 * options.len() {
            break;
        }
        let Some(last) = options.last_mut() else { break };
        if line.chars().next().is_some_and(|c| c.is_uppercase()) || !last.detail.is_empty() {
            last.detail = [last.detail.as_str(), line.as_str()].iter().filter(|t| !t.is_empty()).copied().collect::<Vec<_>>().join(" ");
        } else {
            last.label = format!("{} {line}", last.label);
        }
        end = i;
    }
    // Numeradas: 1, 2, 3… seguidas. Con letra: distintas.
    let numbered = options.iter().all(|o| o.key.chars().all(|c| c.is_ascii_digit()));
    let sequential = options.iter().enumerate().all(|(i, o)| o.key == (i + 1).to_string());
    let distinct = options.iter().enumerate().all(|(i, o)| !options[..i].iter().any(|p| p.key.eq_ignore_ascii_case(&o.key)));
    if options.len() < 2 || (numbered && !sequential) || !distinct {
        return None;
    }
    // Debajo solo caben pistas y la barra de estado: si no, es una lista del texto.
    if lines[end + 1..].iter().filter(|l| !l.is_empty() && !solid(l)).count() > 8 {
        return None;
    }

    // Lo de encima, hasta el borde del diálogo o un hueco grande: la
    // pregunta (la más cercana que acaba en «?») y, sobre ella, de qué va.
    let mut above: Vec<&String> = Vec::new();
    let mut blank = 0;
    let mut i = start;
    while i > 0 && start - i < 16 {
        i -= 1;
        let line = &lines[i];
        if solid(line) {
            break;
        }
        if line.is_empty() {
            blank += 1;
            if blank > 1 && above.iter().any(|l| !l.is_empty()) {
                break;
            }
        } else {
            blank = 0;
        }
        above.insert(0, line);
    }
    let asks = above.iter().rposition(|l| l.ends_with('?'))?;
    let question = above[asks].clone();
    let text = |part: &[&String]| part.iter().filter(|l| !l.is_empty()).map(|l| l.to_string()).collect::<Vec<_>>();
    // Entre la pregunta y las opciones (Codex: «$ npm install»): sobre qué.
    let between = text(&above[asks + 1..]);
    // Encima de la pregunta, hasta la primera raya a trazos (el diff no cuenta).
    let head: Vec<&String> = above[..asks].split(|l| dashed(l)).next().unwrap_or(&[]).to_vec();
    let head = text(&head);
    let subject_of = |l: &str| l.strip_prefix("$ ").unwrap_or(l).to_string();
    let (title, subject, note) = match (head.as_slice(), between.as_slice()) {
        (_, [one, rest @ ..]) => (head.first().cloned().unwrap_or_default(), subject_of(one), rest.first().or(head.get(1)).cloned().unwrap_or_default()),
        ([title, subject, note, ..], []) => (title.clone(), subject_of(subject), note.clone()),
        ([title, subject], []) => (title.clone(), subject_of(subject), String::new()),
        ([title], []) => (title.clone(), String::new(), String::new()),
        _ => Default::default(),
    };
    // Lo que adorna el título de una pregunta de varias opciones («☐ Base de datos»).
    let title = title.trim_start_matches(|c: char| "☐☑✔✓●○◆◇".contains(c) || c.is_whitespace()).to_string();
    for choice in &mut options {
        choice.label = crate::card::clip(&choice.label, 90);
        choice.detail = crate::card::clip(&choice.detail, 90);
    }
    Some(Ask {
        title: crate::card::clip(&title, 60),
        subject: crate::card::clip(&subject, 200),
        note: crate::card::clip(&note, 160),
        question: crate::card::clip(&question, 100),
        options,
    })
}

/// Pantallas de ejemplo de agentes que preguntan, como las enseñan de
/// verdad: para las pruebas y para `veyld agents demo`, que las pasa por el
/// mismo lector que a los agentes reales. (proveedor, nombre, sesión, carpeta, pantalla)
pub const SAMPLES: &[(&str, &str, &str, &str, &str)] = &[
    (
        "anthropic",
        "Claude",
        "Limpiar la compilación",
        "/home/elliot/dev/veyl",
        "● Voy a borrar la carpeta de compilación y volver a compilar.

────────────────────────────────────────────────────────────
 Bash command

   rm -rf build && cargo build --release
   Clean and rebuild in release mode

 Do you want to proceed?
 ❯ 1. Yes
   2. Yes, and don't ask again for rm commands in
   /home/elliot/dev/veyl
   3. No, and tell Claude what to do differently (esc)
",
    ),
    (
        "anthropic",
        "Claude",
        "Separar la orden",
        "/home/elliot/dev/veyl",
        "────────────────────────────────────────────────────────────
 Edit file
   veyld/src/agents.rs
╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌
  210 -    let detail = context.iter().skip(1);
  210 +    let subject = context.get(1).cloned();
╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌
 Do you want to make this edit to agents.rs?
 ❯ 1. Yes
   2. Yes, allow all edits during this session (shift+tab)
   3. No, and tell Claude what to do differently (esc)
",
    ),
    (
        "openai",
        "Codex",
        "Tests del frontend",
        "/home/elliot/dev/portfolio",
        "▌ Allow command?
▌
▌ $ npm install --save-dev vitest
▌
▌ ❯ 1. Yes, proceed
▌   2. Yes, and don't ask again for this command
▌   3. No, and tell Codex what to do differently  esc
",
    ),
    (
        "pi",
        "pi",
        "nixdots",
        "/home/elliot/dotfiles/nixdots",
        "────────────────────────────────────────────────────────────
 Bash

   nixos-rebuild build --flake .#nixos-laptop
   Construye el sistema sin activarlo

 ¿Ejecutar este comando?
 [Y] Permitir   [A] Toda la sesión   [N] Denegar
",
    ),
    (
        "anthropic",
        "Claude",
        "Nuevo proyecto",
        "/home/elliot/dev/lc",
        "────────────────────────────────────────────────────────────
 ☐ Base de datos

 ¿Qué base de datos uso para el proyecto?

 ❯ 1. PostgreSQL
      Robusta, con extensiones y réplicas
   2. SQLite
      Un archivo, sin servidor
   3. Type something.

 Enter to select · ↑/↓ to navigate · Esc to cancel
",
    ),
    (
        "anthropic",
        "Claude",
        "Documentación de serde",
        "/home/elliot/dev/veyl",
        "────────────────────────────────────────────────────────────
 Fetch

   https://docs.rs/serde_json/latest/serde_json/
   Claude wants to fetch content from docs.rs

 Do you want to allow Claude to fetch this content?
 ❯ 1. Yes
   2. Yes, and don't ask again for docs.rs
   3. No, and tell Claude what to do differently (esc)
",
    ),
    (
        "anthropic",
        "Claude",
        "Proyecto nuevo",
        "/home/elliot/dev/proyecto-nuevo",
        "────────────────────────────────────────────────────────────
 Do you trust the files in this folder?

 /home/elliot/dev/proyecto-nuevo

 Claude Code may read, write, or execute files contained in this directory.

 ❯ 1. Yes, proceed
   2. No, exit
",
    ),
];

/// Agentes de ejemplo para ver sus islas (`veyld agents demo`): uno que
/// trabaja desde hace un rato y otro que pregunta lo de la pantalla `n`
/// de `SAMPLES` (pasada por el lector de verdad). Sin terminal: contestar
/// solo los quita.
pub fn demo(n: usize) -> Vec<Agent> {
    let now = (now() * 1000.0) as i64;
    let (provider, label, name, cwd, screen) = SAMPLES[n % SAMPLES.len()];
    vec![
        Agent {
            id: "demo:working".into(),
            provider: "anthropic".into(),
            label: "Claude".into(),
            name: "Refactor del daemon".into(),
            phase: "working".into(),
            since: now - 252_000,
            ..Agent::default()
        },
        Agent {
            id: format!("demo:needs:{}", n % SAMPLES.len()),
            provider: provider.into(),
            label: label.into(),
            name: name.into(),
            cwd: cwd.into(),
            phase: "needs".into(),
            since: now - 38_000,
            ask: asked(screen),
            ..Agent::default()
        },
    ]
}

/// ISO 8601 en UTC («2026-11-17T10:00:00.123Z») → ms.
fn epoch(text: Option<&str>) -> Option<i64> {
    Some(chrono::DateTime::parse_from_rfc3339(text?).ok()?.timestamp_millis())
}

/// Modelo, título y herramienta en curso de una conversación de Claude Code.
fn transcript(entries: &[Value]) -> Told {
    let mut out = Told::default();
    let (mut title, mut model) = (None, None);
    let mut turn: Option<&Value> = None;
    for entry in entries {
        let kind = entry.get("type").and_then(Value::as_str).unwrap_or("");
        if kind == "ai-title" && title.is_none() {
            title = Some(entry.get("aiTitle").and_then(Value::as_str).unwrap_or("").to_string());
        }
        if kind != "assistant" && kind != "user" {
            continue;
        }
        let message = entry.get("message");
        turn.get_or_insert(entry);
        if kind == "assistant"
            && model.is_none()
            && let Some(m) = message.and_then(|m| m.get("model")).and_then(Value::as_str)
            && m != "<synthetic>"
        {
            model = Some(m.to_string());
        }
        if model.is_some() && title.is_some() {
            break;
        }
    }
    // Si lo último es una llamada a una herramienta, es lo que está haciendo.
    if let Some(turn) = turn
        && turn.get("type").and_then(Value::as_str) == Some("assistant")
        && let Some(Value::Array(content)) = turn.get("message").and_then(|m| m.get("content"))
        && let Some(call) = content.iter().rev().find(|c| c.get("type").and_then(Value::as_str) == Some("tool_use"))
    {
        out.tool = call.get("name").and_then(Value::as_str).unwrap_or("").to_string();
    }
    out.name = title.unwrap_or_default();
    out.model = model.unwrap_or_default();
    out
}

/// Fase, modelo y herramienta en curso de un rollout de Codex.
fn rollout(entries: &[Value]) -> Told {
    let mut out = Told::default();
    let (mut model, mut answered) = (None, false);
    for entry in entries {
        let payload = entry.get("payload").cloned().unwrap_or(Value::Null);
        let kind = payload.get("type").and_then(Value::as_str).unwrap_or("");
        let top = entry.get("type").and_then(Value::as_str).unwrap_or("");
        if top == "turn_context" && model.is_none() {
            model = Some(payload.get("model").and_then(Value::as_str).unwrap_or("").to_string());
        }
        if top == "response_item" && out.phase.is_empty() && out.tool.is_empty() {
            if kind.ends_with("_output") {
                answered = true;
            } else if kind.ends_with("call") && !answered {
                out.tool = payload.get("name").and_then(Value::as_str).unwrap_or("shell").to_string();
            }
        }
        if top == "event_msg" && out.phase.is_empty() {
            let when = epoch(entry.get("timestamp").and_then(Value::as_str));
            if kind.contains("approval") || kind == "request_user_input" {
                (out.phase, out.detail, out.since) = ("needs".into(), "aprobación".into(), when);
            } else if kind == "task_started" {
                (out.phase, out.since) = ("working".into(), when);
            } else if kind == "task_complete" || kind == "turn_aborted" {
                (out.phase, out.since) = ("idle".into(), when);
                if kind == "turn_aborted" {
                    out.detail = "interrumpido".into();
                }
            }
        }
        if !out.phase.is_empty() && model.is_some() {
            break;
        }
    }
    if out.phase != "working" {
        out.tool.clear();
    }
    out.model = model.unwrap_or_default();
    out
}

/// Fase, modelo y herramienta en curso de una conversación de Antigravity.
fn agy_transcript(entries: &[Value]) -> Told {
    let mut out = Told::default();
    if let Some(latest) = entries.first() {
        let status = latest.get("status").and_then(Value::as_str).unwrap_or("");
        if status == "WAITING" {
            out.phase = "needs".into();
            out.detail = "aprobación".into();
            out.since = epoch(latest.get("created_at").and_then(Value::as_str));
        } else if matches!(status, "RUNNING" | "GENERATING" | "PENDING" | "QUEUED") {
            out.phase = "working".into();
        }
        if latest.get("type").and_then(Value::as_str) == Some("PLANNER_RESPONSE")
            && let Some(calls) = latest.get("tool_calls").and_then(Value::as_array)
            && let Some(name) = calls.last().and_then(|c| c.get("name")).and_then(Value::as_str)
        {
            out.tool = name.to_string();
        }
    }
    for entry in entries {
        if let Some(model) = entry.get("model").and_then(Value::as_str).filter(|m| !m.is_empty()) {
            out.model = model.to_string();
            break;
        }
    }
    out
}

/// Título, fase y si es conversación raíz en `conversation_summaries.db`.
fn agy_summary(db: &Path, id: &str) -> Option<(String, &'static str, bool)> {
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX).ok()?;
    let _ = conn.busy_timeout(Duration::from_millis(50));
    let (title, status, parent): (String, String, String) = conn
        .query_row("SELECT title, status, parent_conversation_id FROM conversation_summaries WHERE conversation_id = ?1", [id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .ok()?;
    let phase = match status.as_str() {
        "CASCADE_RUN_STATUS_RUNNING" | "CASCADE_RUN_STATUS_BUSY" | "CASCADE_RUN_STATUS_CANCELING" => "working",
        "CASCADE_RUN_STATUS_IDLE" => "idle",
        _ => "",
    };
    Some((title, phase, parent.is_empty()))
}

pub struct Watcher {
    home: PathBuf,
    hz: f64,
    memory: HashMap<i32, Seen>,
    /// Lo leído de cada registro mientras no cambie: ruta → ((mtime, tamaño), resumen).
    read: HashMap<PathBuf, ((i128, u64), Told)>,
    /// Sesión de Claude → su conversación.
    talks: HashMap<String, PathBuf>,
    used_reads: HashSet<PathBuf>,
    used_talks: HashSet<String>,
    /// La fase en la que se vio a cada agente y desde cuándo (ms).
    phases: HashMap<String, (String, i64)>,
}

/// Las sesiones que publica Claude Code (~/.claude/sessions/<pid>.json), por pid.
fn claude_sessions(home: &Path) -> HashMap<i32, Value> {
    let Ok(dir) = fs::read_dir(home.join(".claude/sessions")) else {
        return HashMap::new();
    };
    dir.flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let pid = path.file_name()?.to_str()?.strip_suffix(".json")?.parse().ok()?;
            Some((pid, serde_json::from_str(&fs::read_to_string(&path).ok()?).ok()?))
        })
        .collect()
}

/// Una sesión de Claude Code que no hay que enseñar: la de reserva (un
/// proceso que Claude Code tiene listo por si lo necesita) o la que se lleva
/// en segundo plano el trabajo de otra que está a la vista.
fn hidden_session(pid: i32, sessions: &HashMap<i32, Value>, alive: impl Fn(i32) -> bool) -> bool {
    let Some(session) = sessions.get(&pid) else {
        return false;
    };
    if session.get("spare").and_then(Value::as_bool) == Some(true) {
        return true;
    }
    let job = session.get("jobId").and_then(Value::as_str).filter(|j| !j.is_empty());
    job.is_some_and(|job| {
        session.get("kind").and_then(Value::as_str) == Some("bg")
            && sessions.iter().any(|(&other, s)| other != pid && alive(other) && s.get("parkedJobId").and_then(Value::as_str) == Some(job))
    })
}

impl Watcher {
    pub fn new() -> Watcher {
        Watcher {
            home: config::home(),
            hz: clock_ticks(),
            memory: HashMap::new(),
            read: HashMap::new(),
            talks: HashMap::new(),
            used_reads: HashSet::new(),
            used_talks: HashSet::new(),
            phases: HashMap::new(),
        }
    }

    fn summary(&mut self, path: &Path, digest: fn(&[Value]) -> Told) -> Option<Told> {
        self.used_reads.insert(path.to_path_buf());
        let meta = fs::metadata(path).ok()?;
        let mtime = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_nanos() as i128;
        let key = (mtime, meta.len());
        if self.read.get(path).map(|(k, _)| *k) != Some(key) {
            let told = digest(&tail(path, 262_144));
            self.read.insert(path.to_path_buf(), (key, told));
        }
        self.read.get(path).map(|(_, told)| told.clone())
    }

    /// Lo que Claude Code publica de sí mismo. Si su trabajo pasó a otro
    /// proceso (`parkedJobId`), su propio estado se queda congelado: vale el
    /// de la sesión que se lo llevó.
    fn claude(&mut self, pid: i32, sessions: &HashMap<i32, Value>) -> Option<Told> {
        let own = sessions.get(&pid)?;
        let parked = own.get("parkedJobId").and_then(Value::as_str).filter(|j| !j.is_empty());
        let session = parked.and_then(|job| sessions.values().find(|s| s.get("jobId").and_then(Value::as_str) == Some(job))).unwrap_or(own);
        let text = |key: &str| session.get(key).and_then(Value::as_str).unwrap_or("").to_string();
        let phase = match session.get("status").and_then(Value::as_str)? {
            "busy" => "working",
            "waiting" => "needs",
            "idle" => "idle",
            _ => return None,
        };
        let mut talk = Told::default();
        let sid = text("sessionId");
        if !sid.is_empty() {
            self.used_talks.insert(sid.clone());
            if !self.talks.get(&sid).is_some_and(|p| p.exists())
                && let Ok(dirs) = fs::read_dir(self.home.join(".claude/projects"))
                && let Some(path) = dirs.flatten().map(|d| d.path().join(format!("{sid}.jsonl"))).find(|p| p.exists())
            {
                self.talks.insert(sid.clone(), path);
            }
            if let Some(path) = self.talks.get(&sid).cloned() {
                talk = self.summary(&path, transcript).unwrap_or_default();
            }
        }
        let detail = Some(text("waitingFor")).filter(|s| !s.is_empty()).unwrap_or_else(|| text("needs"));
        Some(Told {
            phase: phase.into(),
            detail,
            since: session.get("statusUpdatedAt").and_then(Value::as_i64),
            name: if talk.name.is_empty() { text("name") } else { talk.name },
            cwd: text("cwd"),
            model: talk.model,
            tool: if phase == "working" { talk.tool } else { String::new() },
        })
    }

    /// Lo que dice el rollout que Codex tiene abierto.
    fn codex(&mut self, tree: &[i32]) -> Option<Told> {
        for &p in tree {
            let Ok(fds) = fs::read_dir(format!("/proc/{p}/fd")) else {
                continue;
            };
            for fd in fds.flatten() {
                if let Ok(path) = fs::read_link(fd.path()) {
                    let shown = path.to_string_lossy();
                    if shown.contains("/.codex/sessions/") && shown.ends_with(".jsonl") {
                        return self.summary(&path, rollout).filter(|t| !t.phase.is_empty());
                    }
                }
            }
        }
        None
    }

    /// Lo que Antigravity publica en su base de resúmenes y su transcripción.
    fn agy(&mut self, tree: &[i32]) -> Option<Told> {
        let mut seen = HashSet::new();
        let mut convs = Vec::new();
        for &p in tree {
            let Ok(fds) = fs::read_dir(format!("/proc/{p}/fd")) else {
                continue;
            };
            for fd in fds.flatten() {
                let Ok(path) = fs::read_link(fd.path()) else {
                    continue;
                };
                let shown = path.to_string_lossy();
                if shown.contains("/conversations/")
                    && shown.ends_with(".db")
                    && let Some(id) = path.file_stem().and_then(|s| s.to_str())
                    && let Some(dir) = path.parent().and_then(Path::parent)
                    && seen.insert((dir.to_path_buf(), id.to_string()))
                {
                    convs.push((dir.to_path_buf(), id.to_string()));
                }
            }
        }
        // Si hay varias conversaciones abiertas (p. ej. un subagente), manda la raíz.
        let mut chosen = None;
        for (dir, id) in convs {
            let summary = agy_summary(&dir.join("conversation_summaries.db"), &id);
            let root = summary.as_ref().is_none_or(|(_, _, root)| *root);
            let candidate = (dir, id, summary);
            if root {
                chosen = Some(candidate);
                break;
            }
            chosen.get_or_insert(candidate);
        }
        let (dir, id, summary) = chosen?;
        let transcript_path = dir.join("brain").join(&id).join(".system_generated/logs/transcript.jsonl");
        let mut out = self.summary(&transcript_path, agy_transcript).unwrap_or_default();
        if let Some((title, phase, _)) = summary {
            if !title.is_empty() {
                out.name = title;
            }
            if out.phase != "needs" && !phase.is_empty() {
                out.phase = phase.into();
            }
        }
        if out.phase.is_empty() {
            return None;
        }
        if out.phase != "working" {
            out.tool.clear();
        }
        Some(out)
    }

    /// Una foto de los agentes en marcha.
    pub fn look(&mut self) -> Vec<Agent> {
        self.used_reads.clear();
        self.used_talks.clear();
        let now = now();
        let procs = procs::all();
        let children = procs::children(&procs);
        let kinds: HashMap<i32, &(&str, &str, &str)> = procs.keys().filter_map(|&pid| program(pid).map(|k| (pid, k))).collect();
        let sessions = claude_sessions(&self.home);
        let now_ms = (now * 1000.0) as i64;
        let mut tmux: Option<HashMap<i32, i32>> = None;
        let mut herdr: Option<HashMap<String, String>> = None;
        let mut agents = Vec::new();
        let mut pids: Vec<i32> = kinds.keys().copied().collect();
        pids.sort();
        for pid in pids {
            let kind = kinds[&pid];
            // Un agente que cuelga de otro igual es un ayudante suyo.
            let mut up = procs.get(&pid).map(|s| s.parent).unwrap_or(0);
            let mut inner = false;
            while up > 1 {
                if kinds.get(&up).is_some_and(|k| k.0 == kind.0) {
                    inner = true;
                    break;
                }
                up = procs.get(&up).map(|s| s.parent).unwrap_or(0);
            }
            if inner || (kind.0 == "claude" && hidden_session(pid, &sessions, |p| procs.contains_key(&p))) {
                continue;
            }

            let tree = procs::tree(pid, &children);
            let spent: u64 = tree.iter().filter_map(|p| procs.get(p)).map(|s| s.ticks).sum();
            let start = procs[&pid].start;
            if self.memory.get(&pid).is_some_and(|seen| seen.start != start) {
                self.memory.remove(&pid);
            }
            let seen = self.memory.entry(pid).or_insert(Seen { busy: 0.0, phase: "idle", since: now, ticks: spent, at: now, start });
            let load = spent.saturating_sub(seen.ticks) as f64 / self.hz / (now - seen.at).max(0.001);
            (seen.ticks, seen.at) = (spent, now);
            if load > BUSY {
                seen.busy = now;
            }
            let guess = if now - seen.busy < CALM { "working" } else { "idle" };
            if guess != seen.phase {
                (seen.phase, seen.since) = (guess, now);
            }
            let (guess, guessed_since) = (seen.phase, (seen.since * 1000.0) as i64);

            let told = match kind.0 {
                "claude" => self.claude(pid, &sessions),
                "codex" => self.codex(&tree),
                "agy" | "antigravity-cli" => self.agy(&tree),
                _ => None,
            };
            let told_ref = told.as_ref();
            let env = procs::environ(pid);
            let pane = Pane::of(&env);
            // Lo que herdr ve en su pantalla manda: es lo que de verdad pasa.
            let shown = match &pane {
                Some(p @ Pane::Herdr { id, .. }) => herdr.get_or_insert_with(|| herdr_states(p)).get(id).and_then(|state| match state.as_str() {
                    "working" => Some("working"),
                    "blocked" => Some("needs"),
                    "idle" | "done" => Some("idle"),
                    _ => None,
                }),
                _ => None,
            };
            let phase = shown.map(String::from).or_else(|| told_ref.map(|t| t.phase.clone())).unwrap_or_else(|| guess.into());
            let told_now = told_ref.filter(|t| t.phase == phase);
            // Desde cuándo: lo que cuenta el agente si dice lo mismo; si no, cuando se vio cambiar.
            let id = format!("{}:{pid}", kind.0);
            let first = told_now.and_then(|t| t.since).unwrap_or(if told_ref.is_none() && phase == guess { guessed_since } else { now_ms });
            let seen = self.phases.entry(id.clone()).or_insert_with(|| (phase.clone(), first));
            if seen.0 != phase {
                *seen = (phase.clone(), told_now.and_then(|t| t.since).unwrap_or(now_ms));
            } else if let Some(since) = told_now.and_then(|t| t.since) {
                seen.1 = since;
            }
            let since = seen.1;
            let own = procs::cwd(pid);
            let cwd = told_ref.map(|t| t.cwd.clone()).filter(|c| !c.is_empty()).unwrap_or(own);
            let name = told_ref
                .map(|t| t.name.clone())
                .filter(|n| !n.is_empty())
                .or_else(|| Path::new(&cwd).file_name().map(|n| n.to_string_lossy().into_owned()))
                .unwrap_or_else(|| kind.2.to_string());
            let model = told_ref.map(|t| t.model.clone()).filter(|m| !m.is_empty()).unwrap_or_else(|| flag(&procs::argv(pid), &["--model", "-m"]));
            let ask = pane.as_ref().filter(|_| phase == "needs").and_then(|p| asked(&p.screen()));
            agents.push(Agent {
                id,
                provider: kind.1.into(),
                label: kind.2.into(),
                name,
                cwd,
                detail: told_now.map(|t| t.detail.clone()).unwrap_or_default(),
                model,
                tool: told_now.filter(|_| phase == "working").map(|t| t.tool.clone()).unwrap_or_default(),
                phase,
                since,
                chain: chain(pid, &procs, &mut tmux),
                focus: inside(&env),
                ask,
                keys: pane.map(|p| p.keys()).unwrap_or_default(),
            });
        }
        self.memory.retain(|pid, _| kinds.contains_key(pid));
        self.phases.retain(|id, _| agents.iter().any(|a| &a.id == id));
        self.read.retain(|path, _| self.used_reads.contains(path));
        self.talks.retain(|sid, _| self.used_talks.contains(sid));
        agents
    }
}

/// Panel → cliente de tmux que lo enseña, para seguir hacia la terminal.
fn tmux_clients() -> HashMap<i32, i32> {
    let run = |args: &[&str]| {
        crate::process::output(Command::new("tmux").args(args), b"", Duration::from_secs(2))
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default()
    };
    let panes = run(&["list-panes", "-a", "-F", "#{pane_pid} #{session_id}"]);
    let clients = run(&["list-clients", "-F", "#{client_pid} #{session_id}"]);
    let mut shown = HashMap::new();
    for line in clients.lines() {
        if let Some((pid, session)) = line.split_once(' ')
            && let Ok(pid) = pid.parse::<i32>()
        {
            shown.entry(session.to_string()).or_insert(pid);
        }
    }
    let mut out = HashMap::new();
    for line in panes.lines() {
        if let Some((pid, session)) = line.split_once(' ')
            && let (Ok(pid), Some(&client)) = (pid.parse::<i32>(), shown.get(session))
        {
            out.insert(pid, client);
        }
    }
    out
}

/// El proceso y sus ancestros; a través de tmux, hasta la terminal.
fn chain(pid: i32, procs: &HashMap<i32, Stat>, tmux: &mut Option<HashMap<i32, i32>>) -> Vec<i32> {
    let mut out = Vec::new();
    let mut pid = pid;
    while pid > 1 && !out.contains(&pid) {
        out.push(pid);
        let parent = procs.get(&pid).map(|s| s.parent).unwrap_or(0);
        if parent > 0 && procs::comm(parent).starts_with("tmux") {
            let panes = tmux.get_or_insert_with(tmux_clients);
            pid = panes.get(&pid).copied().unwrap_or(parent);
        } else {
            pid = parent;
        }
    }
    out
}

/// Órdenes que llevan a la pestaña del agente dentro de su multiplexor.
fn inside(env: &HashMap<String, String>) -> Vec<Vec<String>> {
    if let Some(workspace) = env.get("HERDR_WORKSPACE_ID").filter(|w| !w.is_empty()) {
        let herdr = env.get("HERDR_BIN_PATH").filter(|b| !b.is_empty()).cloned().unwrap_or_else(|| "herdr".into());
        let mut out = vec![vec![herdr.clone(), "workspace".into(), "focus".into(), workspace.clone()]];
        if let Some(tab) = env.get("HERDR_TAB_ID").filter(|t| !t.is_empty()) {
            out.push(vec![herdr, "tab".into(), "focus".into(), tab.clone()]);
        }
        return out;
    }
    if let Some(pane) = env.get("TMUX_PANE").filter(|p| !p.is_empty()) {
        return ["switch-client", "select-window", "select-pane"].iter().map(|verb| vec!["tmux".into(), verb.to_string(), "-t".into(), pane.clone()]).collect();
    }
    Vec::new()
}

/// Mira cada segundo y avisa cuando algo cambia.
pub fn watch(tx: Sender<Event>) {
    let mut watcher = Watcher::new();
    let mut last = None;
    loop {
        let agents = watcher.look();
        if last.as_ref() != Some(&agents) {
            last = Some(agents.clone());
            if tx.send(Event::Agents(agents)).is_err() {
                return;
            }
        }
        thread::sleep(TICK);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLAUDE_ASKS: &str = "\
● Voy a borrar la carpeta de compilación.

────────────────────────────────────────────────────────
 Bash command

   rm -rf build
   Remove the build directory

 Do you want to proceed?
 ❯ 1. Yes
   2. Yes, and don't ask again for rm commands in
   /home/elliot/dev/veyl
   3. No, and tell Claude what to do differently (esc)

";

    #[test]
    fn every_sample_reads_as_its_agent_shows_it() {
        let read = |n: usize| asked(SAMPLES[n].4).unwrap_or_else(|| panic!("no se entiende {}", SAMPLES[n].2));
        let labels = |ask: &Ask| ask.options.iter().map(|o| format!("{} {}", o.key, o.label)).collect::<Vec<_>>();
        let bash = read(0);
        assert_eq!(
            (bash.title.as_str(), bash.subject.as_str(), bash.note.as_str()),
            ("Bash command", "rm -rf build && cargo build --release", "Clean and rebuild in release mode")
        );
        assert_eq!(bash.question, "Do you want to proceed?");
        assert_eq!(labels(&bash)[1], "2 Yes, and don't ask again for rm commands in /home/elliot/dev/veyl");
        let edit = read(1);
        assert_eq!((edit.title.as_str(), edit.subject.as_str(), edit.note.as_str()), ("Edit file", "veyld/src/agents.rs", ""));
        assert_eq!(labels(&edit)[1], "2 Yes, allow all edits during this session (shift+tab)");
        let codex = read(2);
        assert_eq!((codex.question.as_str(), codex.subject.as_str()), ("Allow command?", "npm install --save-dev vitest"));
        assert_eq!(labels(&codex), ["1 Yes, proceed", "2 Yes, and don't ask again for this command", "3 No, and tell Codex what to do differently esc"]);
        let pi = read(3);
        assert_eq!((pi.question.as_str(), pi.subject.as_str()), ("¿Ejecutar este comando?", "nixos-rebuild build --flake .#nixos-laptop"));
        assert_eq!(labels(&pi), ["Y Permitir", "A Toda la sesión", "N Denegar"]);
        let choice = read(4);
        assert_eq!((choice.title.as_str(), choice.question.as_str()), ("Base de datos", "¿Qué base de datos uso para el proyecto?"));
        assert_eq!(labels(&choice), ["1 PostgreSQL", "2 SQLite", "3 Type something."]);
        assert_eq!(choice.options[0].detail, "Robusta, con extensiones y réplicas");
        let fetch = read(5);
        assert_eq!((fetch.title.as_str(), fetch.subject.as_str()), ("Fetch", "https://docs.rs/serde_json/latest/serde_json/"));
        assert_eq!(fetch.options.len(), 3);
        let trust = read(6);
        assert_eq!((trust.question.as_str(), trust.subject.as_str()), ("Do you trust the files in this folder?", "/home/elliot/dev/proyecto-nuevo"));
        assert_eq!(trust.note, "Claude Code may read, write, or execute files contained in this directory.");
        assert_eq!(labels(&trust), ["1 Yes, proceed", "2 No, exit"]);
        // La demo pasa por el mismo lector, y da la vuelta.
        assert_eq!(demo(3)[1].ask, Some(pi));
        assert_eq!(demo(SAMPLES.len())[1].id, "demo:needs:0");
    }

    #[test]
    fn reads_what_an_agent_asks() {
        let ask = asked(CLAUDE_ASKS).unwrap();
        assert_eq!(ask.title, "Bash command");
        assert_eq!((ask.subject.as_str(), ask.note.as_str()), ("rm -rf build", "Remove the build directory"));
        assert_eq!(ask.question, "Do you want to proceed?");
        let keys: Vec<&str> = ask.options.iter().map(|o| o.key.as_str()).collect();
        assert_eq!(keys, ["1", "2", "3"]);
        assert_eq!(ask.options[1].label, "Yes, and don't ask again for rm commands in /home/elliot/dev/veyl");
        // En un recuadro también.
        let boxed = "╭───────────────╮\n│ Edit file     │\n│ src/main.rs   │\n│ Do you want to make this edit? │\n│ ❯ 1. Yes      │\n│   2. No (esc) │\n╰───────────────╯";
        let ask = asked(boxed).unwrap();
        assert_eq!((ask.title.as_str(), ask.question.as_str(), ask.options.len()), ("Edit file", "Do you want to make this edit?", 2));
        // Una lista numerada en mitad del texto no es una pregunta.
        let prose = format!("Pasos:\n1. Instala\n2. Configura\n{}", "más texto\n".repeat(12));
        assert!(asked(&prose).is_none());
        assert!(asked("❯ 1. Solo una").is_none());
    }

    #[test]
    fn parked_and_spare_sessions_are_not_agents() {
        let sessions: HashMap<i32, Value> = [
            (10, serde_json::json!({ "kind": "interactive", "status": "busy", "parkedJobId": "job1" })),
            (11, serde_json::json!({ "kind": "bg", "status": "idle", "jobId": "job1" })),
            (12, serde_json::json!({ "kind": "bg", "status": "idle", "jobId": "job2", "spare": true })),
            (13, serde_json::json!({ "kind": "bg", "status": "busy", "jobId": "job3" })),
        ]
        .into_iter()
        .collect();
        let alive = |_| true;
        assert!(!hidden_session(10, &sessions, alive));
        assert!(hidden_session(11, &sessions, alive), "su trabajo está a la vista en la 10");
        assert!(hidden_session(12, &sessions, alive), "de reserva");
        assert!(!hidden_session(13, &sessions, alive));
        // Si la que lo aparcó ya no está, la de segundo plano se enseña.
        assert!(!hidden_session(11, &sessions, |p| p != 10));
    }

    #[test]
    fn timestamps_preserve_milliseconds_and_timezone() {
        let utc = epoch(Some("2026-11-17T10:00:00.123Z")).unwrap();
        assert_eq!(epoch(Some("2026-11-17T11:00:00.123+01:00")), Some(utc));
        assert_eq!(utc % 1000, 123);
        assert!(epoch(Some("not a timestamp")).is_none());
    }

    #[test]
    fn recognizes_antigravity_and_google_programs() {
        let argv = |parts: &[&str]| parts.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(program_of(&argv(&["/etc/profiles/per-user/elliot/bin/agy", "acp"])), Some(&("agy", "google", "Antigravity")));
        assert_eq!(program_of(&argv(&["/nix/store/abc/bin/.agy-wrapped"])), Some(&("agy", "google", "Antigravity")));
        assert_eq!(program_of(&argv(&["antigravity-cli"])), Some(&("antigravity-cli", "google", "Antigravity")));
        assert_eq!(program_of(&argv(&["gemini"])), Some(&("gemini", "google", "Gemini")));
    }

    #[test]
    fn reads_antigravity_transcript_and_summary() {
        let working = agy_transcript(&[
            serde_json::json!({
                "type": "PLANNER_RESPONSE",
                "status": "DONE",
                "created_at": "2026-11-25T13:32:54Z",
                "model": "gemini-4-neon-medium",
                "tool_calls": [{"name": "read_file"}, {"name": "run_command"}]
            }),
            serde_json::json!({ "type": "GENERIC", "status": "DONE" }),
        ]);
        assert_eq!(working.model, "gemini-4-neon-medium");
        assert_eq!(working.tool, "run_command");
        assert_eq!(working.phase, "");

        let waiting = agy_transcript(&[
            serde_json::json!({
                "type": "GENERIC",
                "status": "WAITING",
                "created_at": "2026-11-25T13:33:00.500Z"
            }),
            serde_json::json!({
                "type": "PLANNER_RESPONSE",
                "status": "DONE",
                "model": "gemini-4-neon-xhigh"
            }),
        ]);
        assert_eq!(waiting.phase, "needs");
        assert_eq!(waiting.detail, "aprobación");
        assert_eq!(waiting.model, "gemini-4-neon-xhigh");
        assert_eq!(waiting.since, epoch(Some("2026-11-25T13:33:00.500Z")));

        let path = std::env::temp_dir().join(format!("veyl-agy-test-{}.db", std::process::id()));
        let _ = fs::remove_file(&path);
        let conn = Connection::open(&path).unwrap();
        conn.execute("CREATE TABLE conversation_summaries (conversation_id text, title text, status text, parent_conversation_id text)", []).unwrap();
        conn.execute(
            "INSERT INTO conversation_summaries VALUES ('c1', 'Añadir Antigravity', 'CASCADE_RUN_STATUS_RUNNING', ''), ('c2', 'Sub', 'CASCADE_RUN_STATUS_IDLE', 'c1')",
            [],
        )
        .unwrap();
        drop(conn);
        assert_eq!(agy_summary(&path, "c1"), Some(("Añadir Antigravity".into(), "working", true)));
        assert_eq!(agy_summary(&path, "c2"), Some(("Sub".into(), "idle", false)));
        let _ = fs::remove_file(&path);
    }
}
