//! Qué agentes hay en marcha en esta máquina y en qué están.
//!
//! Mira los procesos cada segundo y reconoce los agentes conocidos (Claude
//! Code, Codex, Gemini, opencode, Aider…), los lancen desde una terminal, un
//! editor o donde sea. Lo fino lo cuenta el propio agente cuando lo publica
//! (Claude Code en ~/.claude/sessions, Codex en su rollout); si no, se
//! deduce de cuánto trabaja su árbol de procesos.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
const KNOWN: [(&str, &str, &str); 12] = [
    ("claude", "anthropic", "Claude"),
    ("codex", "openai", "Codex"),
    ("gemini", "google", "Gemini"),
    ("opencode", "opencode", "opencode"),
    ("aider", "aider", "Aider"),
    ("cursor-agent", "cursor", "Cursor"),
    ("copilot", "github", "Copilot"),
    ("goose", "goose", "Goose"),
    ("amp", "amp", "Amp"),
    ("crush", "crush", "Crush"),
    ("qwen", "qwen", "Qwen"),
    ("droid", "factory", "Droid"),
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

/// El agente conocido que es este proceso.
fn program(pid: i32) -> Option<&'static (&'static str, &'static str, &'static str)> {
    for arg in procs::argv(pid).iter().take(3) {
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

    /// Lo que Claude Code publica de sí mismo.
    fn claude(&mut self, pid: i32) -> Option<Told> {
        let text = fs::read_to_string(self.home.join(format!(".claude/sessions/{pid}.json"))).ok()?;
        let session: Value = serde_json::from_str(&text).ok()?;
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

    /// Una foto de los agentes en marcha.
    pub fn look(&mut self) -> Vec<Agent> {
        self.used_reads.clear();
        self.used_talks.clear();
        let now = now();
        let procs = procs::all();
        let children = procs::children(&procs);
        let kinds: HashMap<i32, &(&str, &str, &str)> = procs.keys().filter_map(|&pid| program(pid).map(|k| (pid, k))).collect();
        let mut tmux: Option<HashMap<i32, i32>> = None;
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
            if inner {
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
                "claude" => self.claude(pid),
                "codex" => self.codex(&tree),
                _ => None,
            };
            let told_ref = told.as_ref();
            let own = procs::cwd(pid);
            let cwd = told_ref.map(|t| t.cwd.clone()).filter(|c| !c.is_empty()).unwrap_or(own);
            let name = told_ref
                .map(|t| t.name.clone())
                .filter(|n| !n.is_empty())
                .or_else(|| Path::new(&cwd).file_name().map(|n| n.to_string_lossy().into_owned()))
                .unwrap_or_else(|| kind.2.to_string());
            let model = told_ref.map(|t| t.model.clone()).filter(|m| !m.is_empty()).unwrap_or_else(|| flag(&procs::argv(pid), &["--model", "-m"]));
            agents.push(Agent {
                id: format!("{}:{pid}", kind.0),
                provider: kind.1.into(),
                label: kind.2.into(),
                name,
                cwd,
                phase: told_ref.map(|t| t.phase.clone()).unwrap_or_else(|| guess.into()),
                detail: told_ref.map(|t| t.detail.clone()).unwrap_or_default(),
                model,
                tool: told_ref.map(|t| t.tool.clone()).unwrap_or_default(),
                since: told_ref.and_then(|t| t.since).unwrap_or(guessed_since),
                chain: chain(pid, &procs, &mut tmux),
                focus: inside(pid),
            });
        }
        self.memory.retain(|pid, _| kinds.contains_key(pid));
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
fn inside(pid: i32) -> Vec<Vec<String>> {
    let env = procs::environ(pid);
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

    #[test]
    fn timestamps_preserve_milliseconds_and_timezone() {
        let utc = epoch(Some("2026-11-17T10:00:00.123Z")).unwrap();
        assert_eq!(epoch(Some("2026-11-17T11:00:00.123+01:00")), Some(utc));
        assert_eq!(utc % 1000, 123);
        assert!(epoch(Some("not a timestamp")).is_none());
    }
}
