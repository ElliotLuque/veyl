//! Lo que Veyl hace cuando entiende algo: cada intención de la gramática,
//! hecha, y cómo se llama para enseñarla antes de hacerla (la entrada la
//! enseña como una fila; lo peligroso pide confirmación).
//!
//! Lo del sistema lo hace veyld (el sonido con wpctl, la música con
//! playerctl, las ventanas y los niveles con Hyprland, las radios con nmcli y
//! bluetoothctl); lo que es de Veyl (bloquear, el temporizador, la agenda, el
//! esquema, no molestar, la entrada) se lo pide a Veyl.

use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use chrono::{Datelike, Local, NaiveDate, NaiveTime, Timelike};
use serde_json::{Value, json};

use crate::apps::{self, App};
use crate::config;
use crate::daemon::Daemon;
use crate::grammar::{Clause, Parse};
use crate::hypr::{Window, lua};
use crate::journal;
use crate::play::{self, Player};
use crate::views::{self, Candidate, Choice, View};
use crate::when::{self, day_name};
use crate::words::{fold, spoken, tokens};

/// Lo que salió: si se hizo, qué decir en corto y, si hay, la respuesta larga.
#[derive(Clone, Debug)]
pub struct Outcome {
    pub ok: bool,
    pub say: String,
    pub answer: Option<String>,
}

impl Outcome {
    pub fn done(say: impl Into<String>) -> Outcome {
        Outcome { ok: true, say: say.into(), answer: None }
    }

    pub fn fail(say: impl Into<String>) -> Outcome {
        Outcome { ok: false, say: say.into(), answer: None }
    }

    pub fn tell(say: impl Into<String>, answer: impl Into<String>) -> Outcome {
        Outcome { ok: true, say: say.into(), answer: Some(answer.into()) }
    }

    fn from(result: Result<String, String>) -> Outcome {
        match result {
            Ok(say) => Outcome::done(say),
            Err(why) => Outcome::fail(why),
        }
    }
}

/// Cómo se enseña una orden antes de hacerla.
pub struct Preview {
    pub title: String,
    pub glyph: &'static str,
    pub confirm: bool,
    /// Si se puede hacer: la aplicación, la ventana o el esquema existen.
    pub ready: bool,
}

const SINK: &str = "@DEFAULT_AUDIO_SINK@";
const SOURCE: &str = "@DEFAULT_AUDIO_SOURCE@";
/// Puntos que sube o baja el volumen de una vez, como en Veyl.
const STEP: u32 = 5;

pub fn has(tool: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(tool).is_file()))
}

/// Una orden corta: lo que escribe, o por qué falló.
fn exec(command: &[&str]) -> Result<String, String> {
    let out = crate::process::output(Command::new(command[0]).args(&command[1..]), b"", std::time::Duration::from_secs(5))
        .map_err(|e| format!("{}: {e}", command[0]))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        let why = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if why.is_empty() { format!("{} falló", command[0]) } else { why })
    }
}

/// Una orden que tarda o espera algo (slurp espera a que elijas): se lanza y
/// se recoge cuando acabe.
pub fn detach(command: &[&str]) {
    let Ok(mut child) = Command::new(command[0]).args(&command[1..]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() else {
        return;
    };
    thread::spawn(move || child.wait());
}

fn need(tool: &str) -> Result<(), String> {
    if has(tool) { Ok(()) } else { Err(format!("Falta {tool}")) }
}

// --- Sonido --------------------------------------------------------------

/// El volumen (0-100+) y si está silenciado.
fn level(target: &str) -> Option<(u32, bool)> {
    // «Volume: 0.40 [MUTED]»
    let out = exec(&["wpctl", "get-volume", target]).ok()?;
    let value: f64 = out.split_whitespace().nth(1)?.parse().ok()?;
    Some(((value * 100.0).round() as u32, out.contains("MUTED")))
}

fn set_level(target: &str, percent: u32) -> Result<(), String> {
    exec(&["wpctl", "set-volume", target, &format!("{:.2}", percent.min(100) as f64 / 100.0)])?;
    exec(&["wpctl", "set-mute", target, "0"])?;
    Ok(())
}

/// Sube o baja: `by` puntos o, sin decir cuántos, al siguiente múltiplo de 5.
fn step(now: u32, way: i32, by: Option<u32>) -> u32 {
    let next = match by {
        Some(by) => now as i32 + way * by as i32,
        None if way > 0 => (now / STEP * STEP + STEP) as i32,
        None => (now.div_ceil(STEP) * STEP) as i32 - STEP as i32,
    };
    next.clamp(0, 100) as u32
}

fn sound(p: &Parse, target: &str, what: &str) -> Result<String, String> {
    need("wpctl")?;
    let (now, _) = level(target).ok_or(format!("No hay {}", if target == SINK { "salida de sonido" } else { "micrófono" }))?;
    let by = p.number("n").map(|n| n.clamp(0, 150) as u32);
    let intent = p.intent.split('.').nth(1).unwrap_or("");
    match intent {
        "set" | "up" | "down" => {
            let next = match intent {
                "set" => by.unwrap_or(now).min(100),
                "up" => step(now, 1, by),
                _ => step(now, -1, by),
            };
            set_level(target, next)?;
            Ok(format!("{what} al {next} %"))
        }
        "mute" => {
            let on = p.flag("on").unwrap_or(true);
            exec(&["wpctl", "set-mute", target, if on { "1" } else { "0" }])?;
            Ok(match (target == SINK, on) {
                (true, true) => "Sonido silenciado".into(),
                (true, false) => "Vuelve el sonido".into(),
                (false, true) => "Micro silenciado".into(),
                (false, false) => "Micro abierto".into(),
            })
        }
        _ => Err(format!("No sé hacer {}", p.intent)),
    }
}

fn media(p: &Parse) -> Result<String, String> {
    need("playerctl")?;
    let silent = |e: String| if e.contains("No players") { "No suena nada".to_string() } else { e };
    match p.intent.as_str() {
        "media.pause" => exec(&["playerctl", "-a", "pause"]).map(|_| "En pausa".into()).map_err(silent),
        "media.play" => exec(&["playerctl", "play"]).map(|_| "Suena".into()).map_err(silent),
        "media.next" => exec(&["playerctl", "next"]).map(|_| "Siguiente".into()).map_err(silent),
        "media.previous" => exec(&["playerctl", "previous"]).map(|_| "Anterior".into()).map_err(silent),
        _ => {
            let out = exec(&["playerctl", "metadata", "--format", "{{title}}\t{{artist}}"]).map_err(silent)?;
            let (title, artist) = out.split_once('\t').unwrap_or((&out, ""));
            if title.is_empty() {
                return Err("No suena nada".into());
            }
            Ok(if artist.is_empty() { format!("Suena «{title}»") } else { format!("Suena «{title}», de {artist}") })
        }
    }
}

// --- Contexto y frases compuestas -----------------------------------------------

/// De qué se está hablando: la última aplicación (y su ventana) y el último
/// nivel que salieron en una orden, para que «muévelo» o «ve allí» sepan a
/// qué se refieren. Caduca a los cinco minutos.
#[derive(Clone, Debug, Default)]
pub struct Context {
    pub app: String,
    pub window: String,
    pub level: Option<u32>,
    pub at: Option<Instant>,
    /// Se acaba de abrir `app`: cuándo, y las ventanas que ya tenía, para
    /// esperar a la nueva.
    pub launched: Option<(Instant, Vec<String>)>,
}

const MEMORY: Duration = Duration::from_secs(300);

impl Context {
    fn fresh(&self) -> bool {
        self.at.is_some_and(|at| at.elapsed() < MEMORY)
    }
}

/// Lo que actúa sobre una ventana y, sin decir cuál, puede señalar la de antes.
const TARGETED: &[&str] = &["window.move", "window.close", "window.fullscreen", "window.float", "window.place", "window.shift", "window.monitor"];

/// Lo dicho, como las órdenes que hay que hacer, en orden y con los
/// pronombres resueltos: «abre Firefox y muévelo al nivel 2» son
/// `app.open app=firefox` y `window.move app=firefox n=2`. Si partida no se
/// puede hacer y entera sí («apunta pan y leche mañana»), entera.
pub fn plan(d: &Daemon, said: &str) -> Option<Vec<Parse>> {
    let clauses = d.grammar.clauses(said)?;
    let whole = if clauses.len() > 1 { d.grammar.parse(said) } else { None };
    let parts = resolve(d, clauses);
    if let Some(whole) = whole
        && !doable(d, &parts)
    {
        let whole = resolve(d, vec![Clause { parse: whole, said: said.to_string() }]);
        if doable(d, &whole) {
            return Some(whole);
        }
    }
    Some(parts)
}

/// Si todas se pueden hacer: lo que se abre en una cuenta para las siguientes.
fn doable(d: &Daemon, plan: &[Parse]) -> bool {
    let mut opened: Vec<String> = Vec::new();
    plan.iter().all(|p| {
        let app = fold(p.text("app"));
        let ok = describe(d, p).ready || (!app.is_empty() && opened.contains(&app));
        if matches!(p.intent.as_str(), "app.open" | "window.focus") {
            opened.push(app);
        }
        ok
    })
}

fn resolve(d: &Daemon, clauses: Vec<Clause>) -> Vec<Parse> {
    let (mut app, mut window, mut level) =
        if d.context.fresh() { (d.context.app.clone(), d.context.window.clone(), d.context.level) } else { (String::new(), String::new(), None) };
    // Lo nombrado en esta misma frase, no en la de antes.
    let mut here = false;
    let mut out: Vec<Parse> = Vec::with_capacity(clauses.len());
    for Clause { mut parse, said } in clauses {
        // «Abre Prime Video», «abre Firefox y pon Netflix»: lo que no es una
        // aplicación pero sí una web, la web.
        let named = match parse.intent.as_str() {
            "app.open" if !terminal_asked(parse.text("app")) && find_app(d, parse.text("app")).is_none() => parse.text("app").to_string(),
            "play.search" if parse.text("where").is_empty() => parse.text("q").to_string(),
            _ => String::new(),
        };
        if !named.is_empty() && play::site(&named).is_some() {
            parse.intent = "web.open".into();
            parse.args.clear();
            parse.args.insert("site".into(), json!(named));
        }
        if parse.intent == "context.go" {
            if let Some(n) = level {
                parse.intent = "level.go".into();
                parse.args.insert("n".into(), json!(n));
            } else if !app.is_empty() {
                parse.intent = "window.focus".into();
                parse.args.insert("app".into(), json!(app));
                if !window.is_empty() {
                    parse.args.insert("window".into(), json!(window));
                }
            }
        } else if matches!(parse.intent.as_str(), "play.search" | "web.search" | "web.open")
            && !app.is_empty()
            && parse.text("via").is_empty()
            // Poner música solo con lo abierto ahora: «pon …» tras «abre Spotify»
            // en otra frase no es una canción.
            && (here || parse.intent != "play.search")
        {
            // «Abre Spotify y pon…», «abre Firefox y busca…»: dónde, lo de antes.
            parse.args.insert("via".into(), json!(app));
            // «Abre YouTube y pon…»: basta con lo segundo, no hacen falta dos pestañas.
            if let Some(last) = out.last_mut()
                && last.intent == "web.open"
                && fold(last.text("site")) == fold(&app)
                && parse.intent != "web.open"
            {
                last.args.insert("then".into(), json!(true));
            }
        } else if TARGETED.contains(&parse.intent.as_str()) && parse.text("app").is_empty() && !app.is_empty() && d.grammar.pointing(&said) {
            parse.args.insert("app".into(), json!(app));
            if !window.is_empty() {
                parse.args.insert("window".into(), json!(window));
            }
        }
        // Lo que queda dicho para las siguientes.
        let named = parse.text("app").to_string();
        if !named.is_empty() && (parse.intent.starts_with("window.") || parse.intent == "app.open") && parse.intent != "window.others" {
            if fold(&named) != fold(&app) {
                window.clear();
            }
            app = named;
            here = true;
            if !parse.text("window").is_empty() {
                window = parse.text("window").to_string();
            }
        }
        // «Abre YouTube y pon…»: la web cuenta como lo abierto.
        if parse.intent == "web.open" {
            app = parse.text("site").to_string();
            here = true;
            window.clear();
        }
        if matches!(parse.intent.as_str(), "window.move" | "level.go")
            && let Some(n) = parse.number("n")
        {
            level = Some(n.clamp(1, 99) as u32);
        }
        out.push(parse);
    }
    out
}

/// Apunta de qué se ha hablado.
fn remember(d: &mut Daemon, app: &str, window: Option<&Window>) {
    let c = &mut d.context;
    if !app.is_empty() {
        if fold(app) != fold(&c.app) {
            c.window.clear();
            c.launched = None;
        }
        c.app = app.to_string();
    }
    if let Some(w) = window {
        c.window = w.address.clone();
    }
    c.at = Some(Instant::now());
}

fn remember_level(d: &mut Daemon, n: u32) {
    d.context.level = Some(n);
    d.context.at = Some(Instant::now());
}

// --- Ventanas ----------------------------------------------------------------

/// «la terminal» es el terminal de Veyl.
fn terminal_asked(query: &str) -> bool {
    matches!(fold(query.trim()).as_str(), "terminal" | "la terminal" | "una terminal" | "consola" | "la consola" | "shell")
}

fn terminal_program(d: &Daemon) -> String {
    let first = d.settings.terminal.split_whitespace().next().unwrap_or("kitty");
    first.rsplit('/').next().unwrap_or(first).to_string()
}

/// La ventana que mejor casa con lo dicho: por su aplicación, primero, y
/// por su título; a igualdad, la que tuvo el foco más hace poco.
fn pick(d: &Daemon, query: &str) -> Option<Window> {
    let q = fold(query.trim());
    let app = if terminal_asked(query) { None } else { find_app(d, query) };
    let terminal = terminal_asked(query).then(|| terminal_program(d));
    let mut best: Option<(f32, Window)> = None;
    for window in d.hypr.clients() {
        let named = apps::label(&d.apps, &window.class);
        let mut score = apps::likeness(&q, window.app()).max(apps::likeness(&q, &named)).max(apps::likeness(&q, &window.title) * 0.6);
        if app.is_some_and(|a| a.owns(&window.class)) {
            score = score.max(90.0);
        }
        if terminal.as_deref().is_some_and(|t| fold(window.app()) == fold(t)) {
            score = 100.0;
        }
        let better = match &best {
            None => score > 0.0,
            Some((top, old)) => score > *top || (score == *top && window.recency < old.recency),
        };
        if better {
            best = Some((score, window));
        }
    }
    best.map(|(_, w)| w)
}

fn find_app<'a>(d: &'a Daemon, query: &str) -> Option<&'a App> {
    let open: Vec<String> = d.hypr.clients().into_iter().map(|w| w.class).collect();
    apps::find(&d.apps, query, &open)
}

fn launch(d: &Daemon, command: &str) -> Result<(), String> {
    d.hypr.dispatch(&format!("hl.dsp.exec_cmd({})", lua(command)))
}

fn open(d: &mut Daemon, query: &str) -> Result<String, String> {
    let before: Vec<String> = d.hypr.clients().into_iter().filter(|w| owns(d, query, w)).map(|w| w.address).collect();
    let said = if terminal_asked(query) {
        launch(d, &d.settings.terminal)?;
        "Abro el terminal".to_string()
    } else {
        let app = find_app(d, query).ok_or_else(|| format!("No encuentro «{query}»"))?;
        launch(d, &app.command(&d.settings.terminal))?;
        format!("Abro {}", app.name)
    };
    remember(d, query, None);
    d.context.launched = Some((Instant::now(), before));
    Ok(said)
}

/// Si la ventana es de la aplicación de la que se habla.
fn owns(d: &Daemon, query: &str, window: &Window) -> bool {
    if terminal_asked(query) {
        return fold(window.app()) == fold(&terminal_program(d));
    }
    find_app(d, query).is_some_and(|a| a.owns(&window.class)) || apps::likeness(&fold(query), window.app()) >= 80.0
}

/// La ventana de la que se habla: la que señala el contexto, la de la
/// aplicación nombrada o ninguna (entonces, la que tiene el foco). Si la
/// aplicación se acaba de abrir, se espera a que salga su ventana nueva.
fn target(d: &mut Daemon, p: &Parse) -> Result<Option<Window>, String> {
    let app = p.text("app").to_string();
    let address = p.text("window");
    if !address.is_empty()
        && let Some(w) = d.hypr.clients().into_iter().find(|w| w.address == address)
    {
        remember(d, &app, Some(&w));
        return Ok(Some(w));
    }
    if app.is_empty() {
        return Ok(None);
    }
    let window = match fresh_window(d, &app) {
        Some(w) => w,
        None => pick(d, &app).ok_or_else(|| format!("No hay ninguna ventana de «{app}»"))?,
    };
    remember(d, &app, Some(&window));
    Ok(Some(window))
}

/// Cuánto se espera a que salga la ventana de lo que se acaba de abrir.
const ARRIVAL: Duration = Duration::from_secs(8);

fn fresh_window(d: &Daemon, app: &str) -> Option<Window> {
    let (at, before) = d.context.launched.clone()?;
    if fold(&d.context.app) != fold(app) || at.elapsed() > Duration::from_secs(30) {
        return None;
    }
    loop {
        let new = d.hypr.clients().into_iter().filter(|w| !before.contains(&w.address) && owns(d, app, w)).min_by_key(|w| w.recency);
        if new.is_some() || at.elapsed() > ARRIVAL {
            return new;
        }
        thread::sleep(Duration::from_millis(150));
    }
}

/// De los clientes de Hyprland, lo que no lleva `Window`: el sitio en la
/// tira, si flota y si está a pantalla completa.
fn client(d: &Daemon, address: &str) -> Option<Value> {
    d.hypr.json("clients")?.as_array()?.iter().find(|c| c.get("address").and_then(Value::as_str) == Some(address)).cloned()
}

/// Las ventanas de un espacio, de izquierda a derecha (sin las flotantes).
fn strip(d: &Daemon, workspace: i64) -> Vec<String> {
    let Some(all) = d.hypr.json("clients") else {
        return Vec::new();
    };
    let mut windows: Vec<(i64, i64, String)> = all
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c.pointer("/workspace/id").and_then(Value::as_i64) == Some(workspace) && c.get("floating").and_then(Value::as_bool) != Some(true))
        .filter_map(|c| {
            let x = c.pointer("/at/0").and_then(Value::as_i64)?;
            let y = c.pointer("/at/1").and_then(Value::as_i64)?;
            Some((x, y, c.get("address")?.as_str()?.to_string()))
        })
        .collect();
    windows.sort();
    windows.into_iter().map(|(_, _, a)| a).collect()
}

fn active_workspace(d: &Daemon) -> Option<i64> {
    d.hypr.json("activeworkspace")?.get("id")?.as_i64()
}

/// Las ventanas que cierra «cierra todas menos esta»: todas (o las del
/// nivel) menos la que tiene el foco o las de la aplicación nombrada.
fn others(d: &Daemon, p: &Parse) -> Vec<Window> {
    let keep = p.text("keep").trim().to_string();
    let here = active_workspace(d);
    let active = d.hypr.active().map(|w| w.address).unwrap_or_default();
    let mine = keep.is_empty() || matches!(fold(&keep).as_str(), "esta" | "esto" | "esta ventana" | "la actual" | "la de ahora" | "la que tengo delante");
    d.hypr
        .clients()
        .into_iter()
        .filter(|w| p.text("scope") != "level" || Some(w.workspace_id) == here)
        .filter(|w| match () {
            _ if p.flag("all") == Some(true) => true,
            _ if mine => w.address != active,
            _ => !owns(d, &keep, w),
        })
        .collect()
}

fn focus(d: &Daemon, window: &Window) -> Result<(), String> {
    d.hypr.dispatch(&format!("hl.dsp.focus({{ window = {} }})", lua(&format!("address:{}", window.address))))
}

/// El nivel `n` del monitor con el foco, en Lua: con hyprsplit, el suyo.
fn level_lua(n: u32) -> String {
    format!(r#"local ok, hs = pcall(require, "hyprsplit") local target = ok and hs.get_workspace_string("{n}") or "{n}""#)
}

fn levels(d: &Daemon) -> u32 {
    if d.split > 0 { d.split } else { 10 }
}

/// El nivel en el que estás.
fn current_level(d: &Daemon) -> Option<u32> {
    let id = d.hypr.json("activeworkspace")?.get("id")?.as_i64()? as u32;
    Some(if d.split > 0 { (id.max(1) - 1) % d.split + 1 } else { id })
}

fn go_level(d: &Daemon, n: u32) -> Result<String, String> {
    d.hypr.eval(&format!(
        r#"local ok, hs = pcall(require, "hyprsplit") if ok then hl.dispatch(hs.dsp.focus({{ workspace = {n} }})) else hl.dispatch(hl.dsp.focus({{ workspace = "{n}" }})) end"#
    ))?;
    Ok(format!("Nivel {n}"))
}

/// Un monitor: su id, su centro y el espacio que enseña.
struct Screen {
    id: i64,
    center: (i64, i64),
    workspace: i64,
    focused: bool,
}

/// Los monitores, de izquierda a derecha (y de arriba abajo).
fn screens(d: &Daemon) -> Vec<Screen> {
    let mut all: Vec<Screen> = d
        .hypr
        .json("monitors")
        .and_then(|m| m.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|m| {
            let num = |k: &str| m.get(k).and_then(Value::as_i64);
            let scale = m.get("scale").and_then(Value::as_f64).unwrap_or(1.0).max(0.1);
            let (w, h) = ((num("width")? as f64 / scale) as i64, (num("height")? as f64 / scale) as i64);
            Some(Screen {
                id: num("id")?,
                center: (num("x")? + w / 2, num("y")? + h / 2),
                workspace: m.pointer("/activeWorkspace/id")?.as_i64()?,
                focused: m.get("focused").and_then(Value::as_bool) == Some(true),
            })
        })
        .collect();
    all.sort_by_key(|m| (m.center.0, m.center.1));
    all
}

/// El monitor más cercano hacia un lado; «other», el siguiente (y vuelta al primero).
fn beside<'a>(all: &'a [Screen], from: &Screen, dir: &str) -> Option<&'a Screen> {
    let (fx, fy) = from.center;
    let toward = |m: &&Screen| {
        let (dx, dy) = (m.center.0 - fx, m.center.1 - fy);
        match dir {
            "left" => dx < 0,
            "right" => dx > 0,
            "up" => dy < 0,
            "down" => dy > 0,
            _ => false,
        }
    };
    let distance = |m: &&Screen| {
        let (dx, dy) = ((m.center.0 - fx).abs(), (m.center.1 - fy).abs());
        if matches!(dir, "left" | "right") { (dx, dy) } else { (dy, dx) }
    };
    match dir {
        "left" | "right" | "up" | "down" => all.iter().filter(toward).min_by_key(distance),
        _ => {
            let at = all.iter().position(|m| m.id == from.id)?;
            all.get((at + 1) % all.len())
        }
    }
}

fn side_name(dir: &str) -> &'static str {
    match dir {
        "left" => "de la izquierda",
        "right" => "de la derecha",
        "up" => "de arriba",
        "down" => "de abajo",
        _ => "más",
    }
}

fn windows(d: &mut Daemon, p: &Parse) -> Result<String, String> {
    let app = p.text("app").to_string();
    match p.intent.as_str() {
        "window.focus" => {
            let window = if p.text("window").is_empty() { pick(d, &app) } else { target(d, p).ok().flatten() };
            if let Some(window) = window {
                focus(d, &window)?;
                remember(d, &app, Some(&window));
                return Ok(apps::label(&d.apps, &window.class));
            }
            if find_app(d, &app).is_none() && !terminal_asked(&app) {
                // Quizá se instaló algo después de leerlas.
                d.reload_apps();
            }
            open(d, &app)
        }
        "app.open" => {
            if find_app(d, &app).is_none() && !terminal_asked(&app) {
                d.reload_apps();
            }
            open(d, &app)
        }
        "window.close" => {
            let Some(window) = target(d, p)? else {
                d.hypr.dispatch("hl.dsp.window.close()")?;
                return Ok("Ventana cerrada".into());
            };
            d.hypr.dispatch(&format!("hl.dsp.window.close({{ window = {} }})", lua(&format!("address:{}", window.address))))?;
            Ok(format!("Cierro {}", apps::label(&d.apps, &window.class)))
        }
        "window.fullscreen" => {
            let maximized = p.text("mode") == "maximized";
            let window = target(d, p)?;
            if let Some(w) = &window {
                focus(d, w)?;
            }
            let address = window.map(|w| w.address).or_else(|| d.hypr.active().map(|w| w.address)).ok_or("No hay ninguna ventana")?;
            // «Ponlo a pantalla completa» la pone, no la alterna: si ya lo está, se queda.
            let state = client(d, &address).and_then(|c| c.get("fullscreen")?.as_i64()).unwrap_or(0);
            let is = state == if maximized { 1 } else { 2 };
            let (done, undone) = if maximized { ("Maximizada", "Ya no está maximizada") } else { ("Pantalla completa", "Sin pantalla completa") };
            if p.flag("on").is_some_and(|on| on == is) {
                return Ok(if is { format!("Ya estaba: {}", done.to_lowercase()) } else { undone.to_string() });
            }
            let mode = if maximized { "maximized" } else { "fullscreen" };
            d.hypr.dispatch(&format!(r#"hl.dsp.window.fullscreen({{ mode = "{mode}", action = "toggle" }})"#))?;
            Ok(if is { undone.into() } else { done.into() })
        }
        "window.float" => {
            if let Some(w) = target(d, p)? {
                focus(d, &w)?;
            }
            d.hypr.dispatch(r#"hl.dsp.window.float({ action = "toggle" })"#)?;
            Ok("Flotante".into())
        }
        "window.place" => {
            let left = p.text("side") == "left";
            let level = current_level(d).ok_or("No sé en qué nivel estás")?;
            let here = active_workspace(d).ok_or("No sé en qué nivel estás")?;
            let window = match target(d, p)? {
                Some(w) => w,
                None => d.hypr.active().ok_or("No hay ninguna ventana")?,
            };
            let name = apps::label(&d.apps, &window.class);
            let address = lua(&format!("address:{}", window.address));
            // Primero, a este nivel (el nivel se calcula antes de que el foco cambie de monitor).
            if window.workspace_id != here {
                d.hypr.eval(&format!(
                    "{} hl.dispatch(hl.dsp.focus({{ window = {address} }})) hl.dispatch(hl.dsp.window.move({{ workspace = target, follow = true }}))",
                    level_lua(level)
                ))?;
            } else {
                focus(d, &window)?;
            }
            // Luego, tantos pasos como haya hasta el borde: ni uno más, que
            // en el borde se iría al otro monitor.
            let row = strip(d, here);
            let at = row.iter().position(|a| *a == window.address).unwrap_or(0);
            let steps = if left { at } else { row.len().saturating_sub(at + 1) };
            let dir = if left { "left" } else { "right" };
            if steps > 0 {
                d.hypr.eval(&format!(r#"for _ = 1, {steps} do hl.dispatch(hl.dsp.window.move({{ direction = "{dir}" }})) end"#))?;
            }
            Ok(format!("{name}, a la {}", if left { "izquierda" } else { "derecha" }))
        }
        "window.others" => {
            let close = others(d, p);
            if close.is_empty() {
                return Ok("No hay nada más que cerrar".into());
            }
            for w in &close {
                d.hypr.dispatch(&format!("hl.dsp.window.close({{ window = {} }})", lua(&format!("address:{}", w.address))))?;
            }
            Ok(format!("Cierro {} ventana{}", close.len(), if close.len() == 1 { "" } else { "s" }))
        }
        "window.direction" | "window.shift" => {
            let dir = if p.text("dir") == "left" { "left" } else { "right" };
            let verb = if p.intent == "window.shift" { "hl.dsp.window.move" } else { "hl.dsp.focus" };
            if p.intent == "window.shift"
                && let Some(w) = target(d, p)?
            {
                focus(d, &w)?;
            }
            d.hypr.dispatch(&format!(r#"{verb}({{ direction = "{dir}" }})"#))?;
            Ok(if dir == "left" { "A la izquierda".into() } else { "A la derecha".into() })
        }
        "window.monitor" => {
            let window = match target(d, p)? {
                Some(w) => w,
                None => d.hypr.active().ok_or("No hay ninguna ventana")?,
            };
            let mine = d.hypr.active().is_some_and(|a| a.address == window.address);
            let monitors = screens(d);
            if monitors.len() < 2 {
                return Err("Solo hay un monitor".into());
            }
            let from = client(d, &window.address).and_then(|c| c.get("monitor")?.as_i64());
            let from = monitors.iter().find(|m| Some(m.id) == from).or_else(|| monitors.iter().find(|m| m.focused)).ok_or("No sé en qué monitor está")?;
            let to = match p.number("n") {
                Some(n) => monitors.get((n.max(1) - 1) as usize).ok_or_else(|| format!("No hay monitor {n}: hay {}", monitors.len()))?,
                None => beside(&monitors, from, p.text("dir")).ok_or_else(|| match p.text("dir") {
                    "left" => "No hay ningún monitor más a la izquierda".to_string(),
                    "right" => "No hay ningún monitor más a la derecha".to_string(),
                    "up" => "No hay ningún monitor más arriba".to_string(),
                    _ => "No hay ningún monitor más abajo".to_string(),
                })?,
            };
            let name = apps::label(&d.apps, &window.class);
            if to.id == from.id {
                return Ok(format!("{name} ya está en ese monitor"));
            }
            // Al nivel que enseña el otro monitor; la tuya se lleva contigo.
            let address = lua(&format!("address:{}", window.address));
            let mut lua =
                format!("hl.dispatch(hl.dsp.window.move({{ window = {address}, workspace = {}, follow = false }}))", self::lua(&to.workspace.to_string()));
            if mine {
                lua += &format!(" hl.dispatch(hl.dsp.focus({{ window = {address} }}))");
            }
            d.hypr.eval(&lua)?;
            let (dx, dy) = (to.center.0 - from.center.0, to.center.1 - from.center.1);
            let dir = match () {
                _ if dx.abs() >= dy.abs() && dx < 0 => "left",
                _ if dx.abs() >= dy.abs() => "right",
                _ if dy < 0 => "up",
                _ => "down",
            };
            Ok(format!("{name}, al monitor {}", side_name(dir)))
        }
        "window.move" => {
            let n = (p.number("n").unwrap_or(1).max(1) as u32).min(levels(d));
            // Otra ventana se manda y se queda allí; la tuya se lleva contigo.
            let window = target(d, p)?;
            let follow = window.is_none();
            let mut name = "La ventana".to_string();
            if let Some(window) = &window {
                focus(d, window)?;
                name = apps::label(&d.apps, &window.class);
            }
            remember_level(d, n);
            let mut lua = format!("{} hl.dispatch(hl.dsp.window.move({{ workspace = target, follow = false }}))", level_lua(n));
            if follow {
                lua += " hl.dispatch(hl.dsp.focus({ workspace = target, on_current_monitor = true }))";
            }
            d.hypr.eval(&lua)?;
            Ok(format!("{name}, al nivel {n}"))
        }
        "level.go" => {
            let n = (p.number("n").unwrap_or(1).max(1) as u32).min(levels(d));
            remember_level(d, n);
            go_level(d, n)
        }
        "level.step" => {
            let now = current_level(d).ok_or("No sé en qué nivel estás")?;
            let by = p.number("by").unwrap_or(1);
            let next = (now as i64 + by).clamp(1, levels(d) as i64) as u32;
            if next == now {
                return Ok(if by < 0 { format!("Ya estás arriba, en el nivel {now}") } else { format!("Ya estás abajo, en el nivel {now}") });
            }
            go_level(d, next)
        }
        _ => Err(format!("No sé hacer {}", p.intent)),
    }
}

// --- Vistas --------------------------------------------------------------------

/// Un nombre de espacio en Lua.
fn workspace_lua(view: &View) -> String {
    lua(&view.workspace())
}

/// Lleva ventanas al espacio de una vista, sin seguirlas.
fn bring(d: &Daemon, view: &View, addresses: &[String]) -> Result<(), String> {
    if addresses.is_empty() {
        return Ok(());
    }
    let target = workspace_lua(view);
    let lua: Vec<String> = addresses
        .iter()
        .map(|a| format!("hl.dispatch(hl.dsp.window.move({{ window = {}, workspace = {target}, follow = false }}))", lua(&format!("address:{a}"))))
        .collect();
    d.hypr.eval(&lua.join(" "))
}

/// Va al espacio de una vista: si no existe, nace en el monitor con el foco.
fn enter(d: &Daemon, view: &View) -> Result<(), String> {
    d.hypr.dispatch(&format!("hl.dsp.focus({{ workspace = {} }})", workspace_lua(view)))
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// La vista de la que se habla: la nombrada o, sin nombre, la del espacio en el que estás.
fn view_named(d: &Daemon, name: &str) -> Result<usize, String> {
    if name.trim().is_empty() {
        let here = d.hypr.json("activeworkspace").and_then(|w| Some(w.get("name")?.as_str()?.to_string())).unwrap_or_default();
        return d.views.of_workspace(&here).ok_or_else(|| "Aquí no hay ninguna vista: ¿cuál?".to_string());
    }
    d.views.find(name).ok_or_else(|| format!("No hay ninguna vista «{}»", views::tidy(name)))
}

/// Hace la vista con lo elegido: la guarda, abre su espacio y le lleva las ventanas.
pub fn make_view(d: &mut Daemon, name: &str, candidates: &[Candidate], choice: Choice) -> Outcome {
    if choice.is_empty() {
        return Outcome::fail(format!("No encuentro nada abierto que sea de «{name}»"));
    }
    let now = crate::daemon::now();
    let mut view = View { name: name.to_string(), created: now, used: now, ..Default::default() };
    let mut moved = Vec::new();
    let mut lines = Vec::new();
    for (i, keys) in choice {
        let c = &candidates[i];
        view.members.push(views::member(c, keys));
        view.origin.insert(c.window.address.clone(), c.window.workspace_id);
        moved.push(c.window.address.clone());
        lines.push(format!("- {}: {}", c.app, short(&tidy(&c.window.title, &c.app), 60)));
    }
    let done = enter(d, &view).and_then(|_| bring(d, &view, &moved));
    d.views.put(view);
    let saved = d.views.save();
    d.views_changed();
    if let Err(why) = done.and(saved) {
        return Outcome::fail(why);
    }
    remember(d, "", None);
    Outcome::tell(format!("Vista «{name}», con {}", plural(moved.len(), "ventana", "ventanas")), lines.join("\n"))
}

/// Abre una vista. Si su espacio ya existe, va allí; si no (o si se pide
/// recrearla), junta las ventanas que casan y abre las que faltan.
fn open_view(d: &mut Daemon, name: &str, again: bool) -> Result<String, String> {
    let at = view_named(d, name)?;
    let view = d.views.list[at].clone();
    let alive = d
        .hypr
        .json("workspaces")
        .and_then(|w| w.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .any(|w| w.get("name").and_then(Value::as_str) == Some(view.name.as_str()) && w.get("windows").and_then(Value::as_i64).unwrap_or(0) > 0);
    d.views.list[at].used = crate::daemon::now();
    let _ = d.views.save();
    if alive && !again {
        enter(d, &view)?;
        return Ok(format!("Vista «{}»", view.name));
    }
    let candidates = d.candidates();
    let (pairs, missing) = views::gather(&view, &candidates);
    let brought: Vec<String> =
        pairs.iter().map(|(_, i)| &candidates[*i]).filter(|c| c.window.workspace != view.name).map(|c| c.window.address.clone()).collect();
    enter(d, &view)?;
    bring(d, &view, &brought)?;
    for (_, i) in &pairs {
        let c = &candidates[*i];
        if c.window.workspace != view.name {
            d.views.list[at].origin.insert(c.window.address.clone(), c.window.workspace_id);
        }
    }
    let mut opened = 0;
    for m in &missing {
        if let Some(command) = views::relaunch(&view.members[*m], &d.apps, &d.settings.terminal) {
            // Al espacio de la vista aunque mientras tanto te vayas a otro.
            d.hypr.eval(&format!("hl.exec_cmd({}, {{ workspace = {} }})", lua(&command), lua(&format!("{} silent", view.workspace()))))?;
            opened += 1;
        }
    }
    let _ = d.views.save();
    let mut said = format!("Vista «{}»", view.name);
    if !brought.is_empty() {
        said += &format!(": traigo {}", plural(brought.len(), "ventana", "ventanas"));
    }
    if opened > 0 {
        let what = if opened == 1 { "la que faltaba".to_string() } else { format!("las {opened} que faltaban") };
        said += &format!("{} abro {what}", if brought.is_empty() { ":" } else { " y" });
    }
    Ok(said)
}

/// Deshace una vista: cada ventana vuelve al nivel del que vino (si ya no
/// existe, al primero) y la vista queda guardada para recrearla.
fn undo_view(d: &mut Daemon, name: &str) -> Result<String, String> {
    let at = view_named(d, name)?;
    let view = d.views.list[at].clone();
    let inside: Vec<Window> = d.hypr.clients().into_iter().filter(|w| w.workspace == view.name).collect();
    if inside.is_empty() {
        return Ok(format!("La vista «{}» no está abierta", view.name));
    }
    let lua: Vec<String> = inside
        .iter()
        .map(|w| {
            let back = view.origin.get(&w.address).copied().filter(|id| *id > 0).unwrap_or(1);
            format!("hl.dispatch(hl.dsp.window.move({{ window = {}, workspace = {back}, follow = false }}))", lua(&format!("address:{}", w.address)))
        })
        .collect();
    let first = view.origin.get(&inside[0].address).copied().filter(|id| *id > 0).unwrap_or(1);
    d.hypr.eval(&format!("{} hl.dispatch(hl.dsp.focus({{ workspace = {first} }}))", lua.join(" ")))?;
    d.views.list[at].origin.clear();
    let _ = d.views.save();
    Ok(format!("Vista «{}» recogida: cada ventana, a su nivel", view.name))
}

/// Guarda en una vista lo que tiene ahora su espacio (o, si no es de
/// ninguna, el nivel en el que estás, como una vista nueva).
fn save_view(d: &mut Daemon, name: &str) -> Result<String, String> {
    let here = d.hypr.json("activeworkspace").ok_or("No sé dónde estás")?;
    let here_name = here.get("name").and_then(Value::as_str).unwrap_or("").to_string();
    let mut here_id = here.get("id").and_then(Value::as_i64).unwrap_or(0);
    let name = if name.trim().is_empty() {
        d.views.of_workspace(&here_name).map(|i| d.views.list[i].name.clone()).ok_or("¿Con qué nombre?")?
    } else {
        d.views.find(name).map(|i| d.views.list[i].name.clone()).unwrap_or_else(|| views::tidy(name))
    };
    // Si la vista está abierta, lo de su espacio, estés donde estés.
    let spaces = d.hypr.json("workspaces").and_then(|w| w.as_array().cloned()).unwrap_or_default();
    if let Some(id) = spaces.iter().find(|w| w.get("name").and_then(Value::as_str) == Some(name.as_str())).and_then(|w| w.get("id")?.as_i64()) {
        here_id = id;
    }
    let candidates = d.candidates();
    let choice = views::level(&candidates, here_id);
    if choice.is_empty() {
        return Err("En este nivel no hay nada que guardar".into());
    }
    let now = crate::daemon::now();
    let mut view = d.views.find(&name).filter(|i| fold(&d.views.list[*i].name) == fold(&name)).map(|i| d.views.list[i].clone()).unwrap_or_default();
    view.name = name.clone();
    view.created = if view.created == 0 { now } else { view.created };
    view.used = now;
    // Las que ya eran suyas conservan sus palabras.
    let old = std::mem::take(&mut view.members);
    view.members = choice
        .into_iter()
        .map(|(i, keys)| {
            let c = &candidates[i];
            let keys = old.iter().find(|m| !m.keys.is_empty() && views::owns(m, c)).map_or(keys, |m| m.keys.clone());
            views::member(c, keys)
        })
        .collect();
    let count = view.members.len();
    // Un nivel numerado se queda como está: la vista nace la próxima vez que se abra.
    d.views.put(view);
    d.views.save()?;
    d.views_changed();
    Ok(format!("Vista «{name}» guardada, con {}", plural(count, "ventana", "ventanas")))
}

fn views_order(d: &mut Daemon, p: &Parse, source: &str) -> Outcome {
    let name = p.text("name").to_string();
    match p.intent.as_str() {
        "view.create" => {
            let name = views::tidy(&name);
            if name.is_empty() {
                return Outcome::fail("¿Cómo se llama la vista?");
            }
            let here = active_workspace(d).unwrap_or(0);
            let candidates = d.candidates();
            if candidates.is_empty() {
                return Outcome::fail("No hay ninguna ventana abierta");
            }
            if p.text("from") == "level" {
                let choice = views::level(&candidates, here);
                return make_view(d, &name, &candidates, choice);
            }
            if !d.thinks() {
                let choice = views::guess(&name, &candidates, here);
                return make_view(d, &name, &candidates, choice);
            }
            d.think_view(&name, candidates, here, source);
            Outcome::done(format!("Preparo la vista «{name}»"))
        }
        "view.open" => Outcome::from(open_view(d, &name, p.flag("again") == Some(true))),
        "view.undo" => Outcome::from(undo_view(d, &name)),
        "view.save" => Outcome::from(save_view(d, &name)),
        "view.forget" => Outcome::from(view_named(d, &name).and_then(|at| {
            let gone = d.views.list.remove(at);
            d.views.save()?;
            d.views_changed();
            Ok(format!("Olvido la vista «{}»", gone.name))
        })),
        _ => Outcome::from(veyl(d, "launcher.open", json!(["views"])).map(|_| String::new())),
    }
}

// --- Sesión, radios y Veyl -----------------------------------------------------

fn session(d: &mut Daemon, p: &Parse) -> Result<String, String> {
    match p.intent.as_str() {
        "session.lock" => {
            if !d.call("lock", json!([])) {
                exec(&["loginctl", "lock-session"])?;
            }
            Ok("Bloqueada".into())
        }
        "session.suspend" => {
            need("systemctl")?;
            detach(&["systemctl", "suspend"]);
            Ok("Suspendo".into())
        }
        "session.poweroff" => {
            need("systemctl")?;
            detach(&["systemctl", "poweroff"]);
            Ok("Apago".into())
        }
        "session.reboot" => {
            need("systemctl")?;
            detach(&["systemctl", "reboot"]);
            Ok("Reinicio".into())
        }
        "session.logout" => {
            d.hypr.dispatch("hl.dsp.exit()")?;
            Ok("Cierro la sesión".into())
        }
        _ => {
            veyl(d, "reload", json!([]))?;
            Ok("Recargo Veyl".into())
        }
    }
}

fn radio(p: &Parse) -> Result<String, String> {
    let on = p.flag("on").unwrap_or(true);
    if p.text("device") == "bluetooth" {
        need("bluetoothctl")?;
        exec(&["timeout", "5", "bluetoothctl", "power", if on { "on" } else { "off" }])?;
        return Ok(if on { "Bluetooth encendido".into() } else { "Bluetooth apagado".into() });
    }
    need("nmcli")?;
    exec(&["nmcli", "radio", "wifi", if on { "on" } else { "off" }])?;
    Ok(if on { "Wi-Fi encendido".into() } else { "Wi-Fi apagado".into() })
}

/// Algo que hace Veyl.
fn veyl(d: &mut Daemon, name: &str, args: Value) -> Result<(), String> {
    if d.call(name, args) { Ok(()) } else { Err("Veyl no está en marcha".into()) }
}

fn timer(d: &mut Daemon, p: &Parse) -> Result<String, String> {
    match p.intent.as_str() {
        "timer.start" => {
            let secs = p.number("d").unwrap_or(0);
            if secs <= 0 {
                return Err("¿De cuánto?".into());
            }
            let minutes = (secs + 59) / 60;
            veyl(d, "timer.start", json!([minutes]))?;
            Ok(format!("Temporizador de {}", spoken(minutes * 60_000)))
        }
        "timer.stop" => {
            if d.shell.timer <= 0 {
                return Err("No hay ningún temporizador en marcha".into());
            }
            veyl(d, "timer.stop", json!([]))?;
            Ok("Temporizador parado".into())
        }
        _ => {
            let left = d.shell.timer - crate::daemon::now();
            if d.shell.timer <= 0 || left <= 0 {
                return Err("No hay ningún temporizador en marcha".into());
            }
            Ok(format!("Quedan {}", spoken(left)))
        }
    }
}

/// El esquema que más se parece a un nombre: (id, nombre visible).
fn find_scheme(d: &Daemon, name: &str) -> Option<(String, String)> {
    let q = fold(name.trim());
    let mut dirs = vec![config::config().join("schemes")];
    if !d.shell.root.is_empty() {
        dirs.push(Path::new(&d.shell.root).join("schemes"));
    }
    let mut best: Option<(f32, String, String)> = None;
    for dir in dirs {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(id) = path.file_stem().and_then(|s| s.to_str()).filter(|_| path.extension().is_some_and(|e| e == "json")).map(str::to_string) else {
                continue;
            };
            let shown = fs::read_to_string(&path)
                .ok()
                .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                .and_then(|v| v.get("name")?.as_str().map(str::to_string))
                .unwrap_or_else(|| id.clone());
            let score = apps::likeness(&q, &shown).max(apps::likeness(&q, &id.replace('-', " ")));
            if score > 0.0 && best.as_ref().is_none_or(|(top, ..)| score > *top) {
                best = Some((score, id, shown));
            }
        }
    }
    best.map(|(_, id, shown)| (id, shown))
}

fn scheme(d: &mut Daemon, name: &str) -> Result<String, String> {
    let (id, shown) = find_scheme(d, name).ok_or_else(|| format!("No hay ningún esquema «{name}»"))?;
    veyl(d, "scheme.set", json!([id]))?;
    Ok(format!("Esquema {shown}"))
}

fn shot(d: &Daemon, p: &Parse) -> Result<String, String> {
    need("grim")?;
    need("wl-copy")?;
    if p.text("what") == "area" {
        need("slurp")?;
        detach(&["sh", "-c", r#"area=$(slurp) && grim -g "$area" - | wl-copy --type image/png"#]);
        return Ok("Elige la zona".into());
    }
    let monitor = d.hypr.monitor().unwrap_or_default();
    exec(&["sh", "-c", r#"grim ${1:+-o "$1"} - | wl-copy --type image/png"#, "sh", &monitor])?;
    Ok("Captura copiada".into())
}

// --- Preguntas -------------------------------------------------------------------

fn weekday(date: NaiveDate) -> &'static str {
    ["lunes", "martes", "miércoles", "jueves", "viernes", "sábado", "domingo"][date.weekday().num_days_from_monday() as usize]
}

fn month(date: NaiveDate) -> &'static str {
    ["enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre"][date.month0() as usize]
}

fn capital(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// «mañana a las 17:00», «el jueves 19 de noviembre».
fn moment_name(date: NaiveDate, time: Option<NaiveTime>) -> String {
    let today = Local::now().date_naive();
    let day = day_name(date, today);
    match time {
        Some(t) => format!("{day} a las {}", t.format("%H:%M")),
        None => day,
    }
}

fn event(d: &mut Daemon, p: &Parse) -> Result<String, String> {
    let title = p.text("title").trim().to_string();
    let w = when::parse(&tokens(p.text("w"))).ok_or("¿Cuándo?")?;
    let (date, time) = when::ahead(&w, Local::now()).ok_or("Eso ya ha pasado")?;
    let hour = time.map(|t| t.format("%H:%M").to_string()).unwrap_or_default();
    veyl(d, "agenda.add", json!([title, date.format("%Y-%m-%d").to_string(), hour]))?;
    Ok(format!("Apuntado: «{title}», {}", moment_name(date, time)))
}

fn agenda(p: &Parse) -> Outcome {
    let now = Local::now();
    let date = match p.text("w") {
        "" => now.date_naive(),
        w => match when::parse(&tokens(w)).and_then(|w| when::ahead(&w, now)) {
            Some((date, _)) => date,
            None => return Outcome::fail("¿Qué día?"),
        },
    };
    let entries = crate::calendars::agenda();
    let day = date.format("%Y-%m-%d").to_string();
    let mut found: Vec<(String, String)> = entries
        .iter()
        .filter(|e| e.get("day").and_then(Value::as_str) == Some(day.as_str()))
        .map(|e| (e.get("time").and_then(Value::as_str).unwrap_or("").to_string(), e.get("title").and_then(Value::as_str).unwrap_or("").to_string()))
        .collect();
    found.sort();
    let named = capital(&day_name(date, now.date_naive()));
    if found.is_empty() {
        return Outcome::done(format!("{named}, nada apuntado"));
    }
    let lines: Vec<String> =
        found.iter().map(|(time, title)| if time.is_empty() { format!("Todo el día · {title}") } else { format!("{time} · {title}") }).collect();
    Outcome::tell(format!("{named}: {}", if found.len() == 1 { "una cosa".to_string() } else { format!("{} cosas", found.len()) }), lines.join("\n"))
}

// --- El diario ---------------------------------------------------------------------

/// El tramo del que se habla: lo dicho («el jueves por la tarde») o, sin nada, hoy.
fn span_of(said: &str) -> Result<when::Span, Outcome> {
    let now = Local::now();
    let span = if said.trim().is_empty() {
        when::past(&when::When { day: Some(when::Day::Offset(0)), ..when::When::default() }, now)
    } else {
        match when::parse(&tokens(said)) {
            Some(w) => when::past(&w, now),
            None => return Err(Outcome::fail(format!("No entiendo cuándo es «{said}»"))),
        }
    };
    span.ok_or_else(|| Outcome::fail("Eso aún no ha pasado"))
}

/// El diario, con lo abierto apuntado hasta ahora.
fn diary(d: &mut Daemon) -> Result<&rusqlite::Connection, Outcome> {
    let Some(journal) = &mut d.journal else {
        return Err(Outcome::fail("No hay diario"));
    };
    journal.beat(crate::daemon::now());
    Ok(journal.db())
}

fn failed(e: rusqlite::Error) -> Outcome {
    Outcome::fail(format!("El diario no contesta: {e}"))
}

/// «09:05».
fn hm(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms).map(|t| t.with_timezone(&Local).format("%H:%M").to_string()).unwrap_or_default()
}

fn date(ms: i64) -> NaiveDate {
    chrono::DateTime::from_timestamp_millis(ms).map(|t| t.with_timezone(&Local).date_naive()).unwrap_or_default()
}

/// Un texto largo, cortado: «We're Decompiling Fallout 3…».
fn short(text: &str, max: usize) -> String {
    let text = text.trim();
    if text.chars().count() > max { format!("{}…", text.chars().take(max - 1).collect::<String>().trim_end()) } else { text.to_string() }
}

/// «a, b y c».
fn and(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} y {last}", rest.join(", ")),
    }
}

/// Quita del título el nombre de la aplicación: «Rust by Example — Mozilla Firefox».
fn tidy(title: &str, app: &str) -> String {
    let app = fold(app);
    for sep in [" — ", " – ", " - ", " · ", " | "] {
        if let Some((head, tail)) = title.rsplit_once(sep)
            && !head.is_empty()
            && !app.is_empty()
            && (fold(tail).contains(&app) || app.contains(&fold(tail)))
        {
            return tidy(head, &app);
        }
    }
    short(title, 60)
}

/// El título de una página sin el navegador ni el sitio: «… - YouTube».
fn page(title: &str, browser: &str, site: &str) -> String {
    let name = site.rsplit_once('.').map(|(head, _)| head.rsplit('.').next().unwrap_or(head)).unwrap_or(site);
    tidy(&tidy(title, browser), name)
}

/// Por qué no estabas, dicho después de un tiempo: «40 min con la pantalla bloqueada».
fn away_name(reason: &str) -> &str {
    match reason {
        "inactivo" => "sin tocar nada",
        "bloqueado" => "con la pantalla bloqueada",
        "suspendido" => "con el ordenador suspendido",
        other => other,
    }
}

/// Lo mismo, solo: «pantalla bloqueada».
fn away_alone(reason: &str) -> &str {
    match reason {
        "" => "sin diario",
        "inactivo" => "sin tocar nada",
        "bloqueado" => "pantalla bloqueada",
        "suspendido" => "ordenador suspendido",
        other => other,
    }
}

/// Un día en una lista: «hoy», «ayer», «sábado 14», «9 de noviembre».
fn day_short(day: NaiveDate, today: NaiveDate) -> String {
    match (today - day).num_days() {
        0..=2 => day_name(day, today),
        3..=6 => format!("{} {}", weekday(day), day.day()),
        _ if day.year() == today.year() => format!("{} de {}", day.day(), month(day)),
        _ => format!("{} de {} de {}", day.day(), month(day), day.year()),
    }
}

/// Lo que dice el título de una terminal, si dice algo más que su carpeta o su shell.
fn telling(title: &str, folder: &str) -> bool {
    let (title, base) = (fold(title), fold(Path::new(folder).file_name().map(|f| f.to_string_lossy()).unwrap_or_default().as_ref()));
    !(base.is_empty() || title.contains(&base) || ["fish", "bash", "zsh", "sh", "nu", "~"].contains(&title.trim()))
}

/// Lo que se busca, en palabras que cuentan: sin artículos ni «el vídeo de».
fn significant(text: &str) -> Vec<String> {
    const EMPTY: &[&str] = &[
        "el", "la", "los", "las", "lo", "un", "una", "unos", "unas", "de", "del", "al", "a", "en", "con", "por", "para", "y", "o", "que", "mi", "mis", "su",
        "sus", "tu", "tus", "sobre", "este", "esta", "esto", "ese", "esa", "eso", "aquel", "aquella", "algo", "cosa", "cosas", "video", "videos", "pagina",
        "paginas", "web", "rollo", "tema", "vez",
    ];
    let folded = fold(text);
    let words: Vec<String> = folded.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty() && !EMPTY.contains(w)).map(str::to_string).collect();
    words
}

/// «¿Qué estuve mirando el jueves por la tarde?»
pub fn recall(d: &mut Daemon, said: &str) -> Outcome {
    let (span, report) = match gather(d, said) {
        Ok(found) => found,
        Err(why) => return why,
    };
    let mut out = describe_recall(&d.apps, &span.label, &report.recall);
    // Además del resumen: los ratos seguidos y en qué proyectos.
    let mut extra = Vec::new();
    let long = report.sessions.iter().filter(|(a, b)| b - a >= 3 * 60_000).count();
    if long > 1 {
        let longest = report.sessions.iter().map(|(a, b)| b - a).max().unwrap_or(0);
        extra.push(format!("{long} ratos seguidos; el más largo, {}; cambiaste de ventana {} veces", spoken(longest), report.switches));
    }
    let projects: Vec<String> = report
        .projects
        .iter()
        .filter(|p| p.ms + p.agents >= 60_000)
        .take(3)
        .map(|p| match (p.ms >= 60_000, p.agents >= 60_000) {
            (true, true) => format!("{} ({}; los agentes, {})", project_name(&p.path), spoken(p.ms), spoken(p.agents)),
            (false, true) => format!("{} (los agentes, {})", project_name(&p.path), spoken(p.agents)),
            _ => format!("{} ({})", project_name(&p.path), spoken(p.ms.max(60_000))),
        })
        .collect();
    if !projects.is_empty() {
        extra.push(format!("Proyectos: {}", and(&projects)));
    }
    if !extra.is_empty() && out.answer.is_some() {
        let body = out.answer.take().unwrap_or_default();
        let mut lines: Vec<String> = body.lines().map(str::to_string).collect();
        let at = usize::from(lines.first().is_some_and(|l| l.starts_with("De ")));
        lines.splice(at..at, extra);
        out.answer = Some(lines.join("\n"));
    }
    out
}

/// El tramo de lo dicho y todo lo que hubo en él (por horas si es un día o
/// menos; por días si es más).
fn gather(d: &mut Daemon, said: &str) -> Result<(when::Span, journal::Report), Outcome> {
    let span = span_of(said)?;
    let (from, to) = (span.from.timestamp_millis(), span.to.timestamp_millis());
    let slice = if to - from <= 26 * 3600 * 1000 { 3600 * 1000 } else { 24 * 3600 * 1000 };
    let midnight =
        span.from.date_naive().and_hms_opt(0, 0, 0).and_then(|t| t.and_local_timezone(Local).earliest()).map(|t| t.timestamp_millis()).unwrap_or(from);
    let home = config::home().to_string_lossy().into_owned();
    let report = diary(d).and_then(|db| journal::report(db, from, to, slice, midnight, &home).map_err(failed))?;
    Ok((span, report))
}

/// Lo mismo que `recall`, para otro programa: todo, en JSON.
pub fn recall_json(d: &mut Daemon, said: &str) -> Result<Value, Outcome> {
    let (span, report) = gather(d, said)?;
    let mut json = serde_json::to_value(&report).map_err(|e| Outcome::fail(e.to_string()))?;
    json["label"] = json!(span.label);
    Ok(json)
}

/// Un día entero del diario, para su vista (base/Diary.qml): todo lo de
/// `report`, la línea del tiempo, el nombre de cada aplicación y lo que hubo
/// delante cada uno de los `days` días hasta hoy. `day` es «2026-11-19»; hoy
/// llega hasta ahora.
pub fn diary_json(d: &mut Daemon, day: &str, days: i64) -> Result<Value, Outcome> {
    let today = Local::now().date_naive();
    let day = if day.is_empty() { today } else { NaiveDate::parse_from_str(day, "%Y-%m-%d").map_err(|_| Outcome::fail(format!("No es un día: «{day}»")))? };
    let midnight = |day: NaiveDate| day.and_hms_opt(0, 0, 0).and_then(|t| t.and_local_timezone(Local).earliest()).map(|t| t.timestamp_millis()).unwrap_or(0);
    let from = midnight(day);
    let to = if day == today { crate::daemon::now() } else { midnight(day + chrono::Duration::days(1)) };
    let home = config::home().to_string_lossy().into_owned();
    let apps_known = d.apps.clone();
    let db = diary(d)?;
    let report = journal::report(db, from, to, 3600 * 1000, from, &home).map_err(failed)?;
    let blocks = journal::timeline(db, from, to, 5 * 60 * 1000, from).map_err(failed)?;
    let mut strip = Vec::new();
    for back in (0..days.clamp(1, 60)).rev() {
        let one = today - chrono::Duration::days(back);
        let (start, end) = (midnight(one), if one == today { crate::daemon::now() } else { midnight(one + chrono::Duration::days(1)) });
        strip.push(json!({ "day": one.format("%Y-%m-%d").to_string(), "ms": journal::screen(db, start, end).map_err(failed)? }));
    }
    let mut labels = serde_json::Map::new();
    for class in report.recall.apps.iter().map(|a| &a.app).chain(blocks.iter().map(|b| &b.app)).chain(report.opened.iter().map(|(a, _)| a)) {
        if !class.is_empty() && !labels.contains_key(class) {
            labels.insert(class.clone(), json!(apps::label(&apps_known, class)));
        }
    }
    let mut json = serde_json::to_value(&report).map_err(|e| Outcome::fail(e.to_string()))?;
    json["day"] = json!(day.format("%Y-%m-%d").to_string());
    json["label"] = json!(capital(&day_name(day, today)));
    json["blocks"] = serde_json::to_value(&blocks).map_err(|e| Outcome::fail(e.to_string()))?;
    json["labels"] = Value::Object(labels);
    json["days"] = json!(strip);
    json["home"] = json!(home);
    Ok(json)
}

/// Lo que encuentra `find`, en datos, para la vista del diario.
pub fn find_json(d: &mut Daemon, what: &str) -> Result<Value, Outcome> {
    let words = significant(what);
    if words.is_empty() {
        return Ok(json!([]));
    }
    let apps_known = d.apps.clone();
    let hits = diary(d).and_then(|db| journal::find(db, &words, |app| apps::label(&apps_known, app), 40).map_err(failed))?;
    let hits: Vec<Value> = hits
        .iter()
        .map(|hit| {
            let mut one = serde_json::to_value(hit).unwrap_or_default();
            one["label"] = json!(if hit.kind == "focus" { apps::label(&apps_known, &hit.app) } else { hit.app.clone() });
            one
        })
        .collect();
    Ok(json!(hits))
}

/// Todo lo distinto de un tramo, para que el cerebro busque por lo que
/// significa («¿cuándo miré lo de la beca?») y no solo por las palabras.
pub fn index(d: &mut Daemon, said: &str) -> Outcome {
    let span = match span_of(said) {
        Ok(span) => span,
        Err(why) => return why,
    };
    let (from, to) = (span.from.timestamp_millis(), span.to.timestamp_millis());
    let apps_known = d.apps.clone();
    let found = match diary(d).and_then(|db| journal::index(db, from, to, 300).map_err(failed)) {
        Ok(found) => found,
        Err(why) => return why,
    };
    let label = capital(&span.label);
    if found.is_empty() {
        return Outcome::done(format!("{label}: nada en el diario"));
    }
    let today = Local::now().date_naive();
    let several = date(from) != date(to - 1);
    let lines: Vec<String> = found
        .iter()
        .map(|e| {
            let app = if e.kind == "focus" { apps::label(&apps_known, &e.app) } else { e.app.clone() };
            let when = if e.last - e.first >= 60_000 { format!("{}–{}", hm(e.first), hm(e.last)) } else { hm(e.first) };
            let when = if several { format!("{}, {when}", capital(&day_short(date(e.first), today))) } else { when };
            let mut parts = vec![
                when,
                match e.kind {
                    "media" => format!("sonó en {app}"),
                    "file" => format!("archivo, con {app}"),
                    _ => app.clone(),
                },
            ];
            if !e.place.is_empty() {
                parts.push(if e.place.starts_with('/') { config::tilde(&e.place) } else { e.place.clone() });
            }
            let title = page(&e.title, &app, &e.place);
            // Un título que solo repite la carpeta o la aplicación no dice nada.
            let says =
                !title.is_empty() && title != "~" && title != app && config::tilde(&e.place) != title && !e.place.ends_with(title.trim_start_matches('~'));
            if says && e.kind != "file" {
                parts.push(format!("«{}»", short(&title, 90)));
            }
            if !e.url.is_empty() {
                parts.push(short(&e.url, 100));
            }
            if e.kind != "file" {
                parts.push(if e.times > 1 { format!("{} en {} veces", spoken(e.ms.max(60_000)), e.times) } else { spoken(e.ms.max(60_000)) });
            }
            parts.join(" · ")
        })
        .collect();
    Outcome::tell(format!("{label}: {} cosas distintas, por orden", found.len()), lines.join("\n"))
}

/// «veyl» de «/home/u/dev/veyl».
fn project_name(path: &str) -> String {
    Path::new(path).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_else(|| config::tilde(path))
}

/// Una barra de 10 de lo lleno que está algo: «███████░░░».
fn bar(part: i64, whole: i64) -> String {
    let filled = if whole > 0 { ((part * 10 + whole / 2) / whole).clamp(0, 10) as usize } else { 0 };
    format!("{}{}", "█".repeat(filled), "░".repeat(10 - filled))
}

/// «(a 25 min, b 10 min)», lo que pasa del minuto.
fn timed(list: &[(String, i64)], max: usize, name: impl Fn(&str) -> String) -> String {
    let parts: Vec<String> =
        list.iter().filter(|(_, ms)| *ms >= 30_000).take(max).map(|(k, ms)| format!("{} {}", name(k), spoken((*ms).max(60_000)))).collect();
    parts.join(", ")
}

/// «¿Qué hice ayer?», con todo: ritmo, horas, proyectos, aplicaciones con
/// sus páginas, carpetas y órdenes, niveles, agentes, lo que sonó, llamadas,
/// archivos, ventanas y lo que se le pidió a Veyl.
pub fn report(d: &mut Daemon, said: &str) -> Outcome {
    let (span, r) = match gather(d, said) {
        Ok(found) => found,
        Err(why) => return why,
    };
    let label = capital(&span.label);
    let rc = &r.recall;
    let heard: i64 = rc.media.iter().map(|m| m.ms).sum();
    if rc.total < 60_000 && r.agents.is_empty() && heard < 60_000 && rc.files.is_empty() {
        return Outcome::done(format!("{label}: nada en el diario"));
    }
    let today = Local::now().date_naive();
    let several = date(r.from) != date(r.to - 1);
    let at = |ms: i64| if several { format!("{}, {}", capital(&day_short(date(ms), today)), hm(ms)) } else { hm(ms) };
    let name = |app: &str| apps::label(&d.apps, app);
    let mut out: Vec<String> = Vec::new();
    let mut section = |title: &str, lines: Vec<String>| {
        if !lines.is_empty() {
            out.push(String::new());
            out.push(title.to_string());
            out.extend(lines.into_iter().map(|l| format!("  {l}")));
        }
    };

    // Ritmo.
    let mut rhythm = Vec::new();
    if !r.sessions.is_empty() {
        // Los ratos de verdad; los vistazos sueltos (menos de 3 min), solo contados.
        let long: Vec<&(i64, i64)> = r.sessions.iter().filter(|(a, b)| b - a >= 3 * 60_000).collect();
        let short_ones = r.sessions.len() - long.len();
        // Con varios días, el día solo cuando cambia.
        let mut day = None;
        let shown: Vec<String> = long
            .iter()
            .take(10)
            .map(|(a, b)| {
                let first = several && day != Some(date(*a));
                day = Some(date(*a));
                let from = if first { format!("{} {}", day_short(date(*a), today), hm(*a)) } else { hm(*a) };
                format!("{from}–{} ({})", hm(*b), spoken(b - a))
            })
            .collect();
        let mut line = match long.len() {
            0 => String::new(),
            1 => format!("Un rato seguido: {}", shown[0]),
            n => format!("{n} ratos seguidos: {}", shown.join(", ")),
        };
        if long.len() > 10 {
            line += &format!(" y {} más", long.len() - 10);
        }
        if short_ones > 0 {
            let glance = format!("{} vistazo{} suelto{}", short_ones, if short_ones == 1 { "" } else { "s" }, if short_ones == 1 { "" } else { "s" });
            line = if line.is_empty() { capital(&glance) } else { format!("{line}; y {glance}") };
        }
        let longest = r.sessions.iter().map(|(a, b)| b - a).max().unwrap_or(0);
        if long.len() > 1 {
            line += &format!(". El más largo, {}", spoken(longest));
        }
        rhythm.push(line);
    }
    let pauses: Vec<String> = rc.away.iter().filter(|(_, ms)| *ms >= 60_000).map(|(why, ms)| format!("{} {}", spoken(*ms), away_name(why))).collect();
    if !pauses.is_empty() {
        rhythm.push(format!("Pausas: {}", and(&pauses)));
    }
    if r.switches > 0 && rc.total > 0 {
        rhythm.push(format!("Cambiaste de ventana {} veces (una cada {} de media)", r.switches, spoken(rc.total / (r.switches as i64 + 1))));
    }
    section("Ritmo", rhythm);

    // Por horas o por días.
    let busiest = r.slices.iter().map(|s| s.ms).max().unwrap_or(0);
    let hourly = r.slices.first().is_some_and(|s| s.end - s.start <= 3600 * 1000);
    let mut slices = Vec::new();
    let first = r.slices.iter().position(|s| s.ms > 0).unwrap_or(r.slices.len());
    let last = r.slices.iter().rposition(|s| s.ms > 0).map_or(0, |i| i + 1);
    let mut empty: Option<(i64, i64)> = None;
    for s in r.slices.iter().take(last).skip(first) {
        // Las horas sin nada, juntas en una línea.
        if s.ms < 60_000 {
            empty = Some(empty.map_or((s.start, s.end), |(a, _)| (a, s.end)));
            continue;
        }
        if let Some((a, b)) = empty.take() {
            let span = if hourly { format!("{}–{}", hm(a), hm(b)) } else { format!("{}–{}", day_short(date(a), today), day_short(date(b - 1), today)) };
            slices.push(format!("{span:<11}  ·"));
        }
        let when = if hourly { hm(s.start) } else { capital(&day_short(date(s.start), today)) };
        let whole = if hourly { 3600 * 1000 } else { busiest };
        let mut what = format!("  {:>9}  {}", spoken(s.ms), name(&s.app));
        if !s.place.is_empty() {
            what += &format!(" · {}", if s.place.starts_with('/') { config::tilde(&s.place) } else { s.place.clone() });
        }
        slices.push(format!("{when:<11} {}{what}", bar(s.ms, whole)));
    }
    section(if hourly { "Por horas" } else { "Por días" }, slices);

    // Proyectos.
    let projects: Vec<String> = r
        .projects
        .iter()
        .filter(|p| p.ms + p.agents >= 60_000)
        .take(8)
        .map(|p| {
            let mut line = format!("{}  {}", config::tilde(&p.path), spoken(p.ms.max(60_000)));
            let apps = timed(&p.apps, 3, |a| name(a));
            if !apps.is_empty() {
                line += &format!(" ({apps})");
            }
            let commands = timed(&p.commands, 4, str::to_string);
            if !commands.is_empty() {
                line += &format!(" · {commands}");
            }
            if p.agents >= 60_000 {
                line += &format!(" · los agentes trabajaron {}", spoken(p.agents));
            }
            line
        })
        .collect();
    section("Proyectos", projects);

    // Aplicaciones, con todo lo que hubo dentro.
    let mut apps_lines = Vec::new();
    for app in rc.apps.iter().filter(|a| a.ms >= 60_000).take(10) {
        let label = name(&app.app);
        apps_lines.push(format!("{label}  {}", spoken(app.ms)));
        let mut covered = 0;
        for site in app.sites.iter().filter(|p| p.ms >= 30_000).take(6) {
            covered += site.ms;
            let pages = timed(&site.within, 3, |t| format!("«{}»", short(&page(t, &label, &site.name), 45)));
            apps_lines.push(format!("  {}  {}{}", site.name, spoken(site.ms.max(60_000)), if pages.is_empty() { String::new() } else { format!(": {pages}") }));
        }
        for folder in app.folders.iter().filter(|p| p.ms >= 30_000).take(4) {
            covered += folder.ms;
            let commands = timed(&folder.within, 4, str::to_string);
            apps_lines.push(format!(
                "  {}  {}{}",
                config::tilde(&folder.name),
                spoken(folder.ms.max(60_000)),
                if commands.is_empty() { String::new() } else { format!(": {commands}") }
            ));
        }
        // Lo que no cae en ningún sitio ni carpeta, por sus títulos.
        let placed: Vec<&str> = app.sites.iter().flat_map(|p| p.within.iter().map(|(t, _)| t.as_str())).collect();
        if app.ms - covered >= 60_000 {
            let mut seen: Vec<(String, i64)> = Vec::new();
            for (title, ms) in app.titles.iter().filter(|(t, _)| covered == 0 || !placed.contains(&t.as_str())) {
                let clean = tidy(title, &label);
                match seen.iter_mut().find(|(t, _)| *t == clean) {
                    Some((_, total)) => *total += ms,
                    None if !clean.is_empty() => seen.push((clean, *ms)),
                    None => {}
                }
            }
            let titles = timed(&seen, 6, |t| format!("«{}»", short(t, 50)));
            if !titles.is_empty() {
                apps_lines.push(if covered > 0 { format!("  otras páginas  {}: {titles}", spoken(app.ms - covered)) } else { format!("  {titles}") });
            }
        }
    }
    section("Aplicaciones", apps_lines);

    // Niveles (con hyprsplit, el mismo nivel de cada monitor se junta).
    let mut levels: Vec<(String, i64)> = Vec::new();
    for (workspace, ms) in &r.levels {
        let n = workspace.parse::<u32>().map(|id| if d.split > 0 { (id.max(1) - 1) % d.split + 1 } else { id });
        let key = n.map(|n| format!("nivel {n}")).unwrap_or_else(|_| workspace.clone());
        match levels.iter_mut().find(|(k, _)| *k == key) {
            Some((_, total)) => *total += ms,
            None => levels.push((key, *ms)),
        }
    }
    levels.sort_by_key(|(_, ms)| std::cmp::Reverse(*ms));
    let levels = timed(&levels, 10, str::to_string);
    section("Niveles", if levels.is_empty() { vec![] } else { vec![levels] });

    // Agentes.
    let agents: Vec<String> = r
        .agents
        .iter()
        .take(10)
        .map(|a| {
            let mut line = a.label.clone();
            if !a.name.is_empty() {
                line += &format!(" «{}»", short(&a.name, 50));
            }
            if !a.folder.is_empty() {
                line += &format!(" en {}", config::tilde(&a.folder));
            }
            if !a.model.is_empty() {
                line += &format!(" ({})", a.model);
            }
            line += &format!(": trabajó {}, de {} a {}", spoken(a.working.max(60_000)), at(a.first), hm(a.last));
            if a.waits > 0 {
                let total = if a.waiting < 60_000 { "menos de un minuto".to_string() } else { spoken(a.waiting) };
                line += &format!("; te esperó {} {}, {total} en total", a.waits, if a.waits == 1 { "vez" } else { "veces" });
            }
            line
        })
        .collect();
    section("Agentes", agents);

    // Lo que sonó.
    let media: Vec<String> = rc.media.iter().take(15).map(|m| format!("{} ({})  {}", track(m), m.player, spoken(m.ms.max(60_000)))).collect();
    section(&format!("Sonó{}", if heard >= 60_000 { format!(", {}", spoken(heard)) } else { String::new() }), media);

    // Micro, cámara y pantalla compartida.
    let calls: Vec<String> = r
        .calls
        .iter()
        .filter(|c| c.end - c.start >= 30_000)
        .take(15)
        .map(|c| {
            let mut line = format!("{}–{}  {} ({})", at(c.start), hm(c.end), device_name(&c.kind), spoken((c.end - c.start).max(60_000)));
            if !c.app.is_empty() {
                line += &format!(", en {}", name(&c.app));
                if !c.site.is_empty() {
                    line += &format!(" · {}", c.site);
                }
            }
            line
        })
        .collect();
    section("Micro, cámara y pantalla", calls);

    // Archivos.
    let files: Vec<String> = rc
        .files
        .iter()
        .take(20)
        .map(|f| {
            let folder = Path::new(&f.path).parent().map(|p| config::tilde(&p.to_string_lossy())).unwrap_or_default();
            format!("{}  {} · {} · {folder}", at(f.time), file_name(&f.path), f.app)
        })
        .collect();
    section("Archivos", files);

    // Ventanas.
    let opened: i64 = r.opened.iter().map(|(_, n)| n).sum();
    if opened > 0 || r.closed > 0 {
        let by: Vec<String> = r.opened.iter().take(6).map(|(app, n)| format!("{} {n}", name(app))).collect();
        section(
            "Ventanas",
            vec![format!("Abriste {opened}{}; cerraste {}", if by.is_empty() { String::new() } else { format!(" ({})", by.join(", ")) }, r.closed)],
        );
    }

    // Lo que se le pidió a Veyl.
    let asked: Vec<String> = r
        .orders
        .iter()
        .rev()
        .take(15)
        .rev()
        .map(|o| {
            let how = match o.source.as_str() {
                "voice" => " (voz)",
                "launcher" => " (entrada)",
                _ => "",
            };
            format!("{}  «{}»{how} → {}{}", at(o.time), short(&o.said, 60), if o.ok { "" } else { "✗ " }, short(&o.answer, 60))
        })
        .collect();
    section("Le pediste a Veyl", asked);

    let mut say = format!("{label}: {} de pantalla", spoken(rc.total));
    if rc.total >= 60_000 && date(rc.first) == date(rc.last) {
        say += &format!(", de {} a {}", hm(rc.first), hm(rc.last));
    }
    let body = out.into_iter().skip_while(String::is_empty).collect::<Vec<_>>().join("\n");
    Outcome::tell(say, body)
}

pub fn describe_recall(apps_known: &[App], label: &str, found: &journal::Recall) -> Outcome {
    let label = capital(label);
    let heard: i64 = found.media.iter().map(|m| m.ms).sum();
    if found.total < 60_000 && found.agents.is_empty() && heard < 60_000 {
        return Outcome::done(format!("{label}: nada en el diario"));
    }
    let mut lines = Vec::new();
    if found.total >= 60_000 {
        let pauses: Vec<String> = found.away.iter().filter(|(_, ms)| *ms >= 60_000).map(|(why, ms)| format!("{} {}", spoken(*ms), away_name(why))).collect();
        let mut line = if date(found.first) == date(found.last) { format!("De {} a {}", hm(found.first), hm(found.last)) } else { String::new() };
        if !pauses.is_empty() {
            line = if line.is_empty() { capital(&and(&pauses)) } else { format!("{line}, con {}", and(&pauses)) };
        }
        if !line.is_empty() {
            lines.push(line);
        }
    }
    for app in found.apps.iter().filter(|a| a.ms >= 60_000).take(6) {
        let name = apps::label(apps_known, &app.app);
        let sites: i64 = app.sites.iter().map(|p| p.ms).sum();
        let folders: i64 = app.folders.iter().map(|p| p.ms).sum();
        let detail = if sites * 2 >= app.ms {
            // En el navegador: los sitios, cada uno con su página más vista.
            let top: Vec<String> = app
                .sites
                .iter()
                .filter(|p| p.ms >= 30_000)
                .take(3)
                .map(|p| {
                    let title = p.within.first().map(|(t, _)| page(t, &name, &p.name)).unwrap_or_default();
                    let bare = p.name.split('.').next().unwrap_or(&p.name);
                    if title.is_empty() || fold(&title) == fold(bare) { p.name.clone() } else { format!("{} («{}»)", p.name, short(&title, 50)) }
                })
                .collect();
            if top.is_empty() { String::new() } else { format!(": {}", top.join(", ")) }
        } else if folders * 2 >= app.ms {
            // En una terminal: las carpetas, con lo que corrió en ellas.
            let top: Vec<String> = app
                .folders
                .iter()
                .take(2)
                .map(|p| {
                    let commands: Vec<&str> = p.within.iter().filter(|(_, ms)| *ms >= 30_000).take(3).map(|(c, _)| c.as_str()).collect();
                    if commands.is_empty() { config::tilde(&p.name) } else { format!("{} ({})", config::tilde(&p.name), commands.join(", ")) }
                })
                .collect();
            format!(", en {}", top.join(" y "))
        } else {
            let mut seen: Vec<String> = Vec::new();
            for (title, _) in &app.titles {
                let clean = tidy(title, &name);
                if !clean.is_empty() && !seen.contains(&clean) {
                    seen.push(clean);
                }
                if seen.len() == 2 {
                    break;
                }
            }
            if seen.is_empty() { String::new() } else { format!(": {}", seen.iter().map(|t| format!("«{t}»")).collect::<Vec<_>>().join(", ")) }
        };
        lines.push(format!("{name}, {}{detail}", spoken(app.ms)));
    }
    for agent in found.agents.iter().filter(|a| a.ms >= 60_000).take(3) {
        let mut place = if agent.folder.is_empty() { String::new() } else { format!(" en {}", config::tilde(&agent.folder)) };
        // El nombre de la sesión, si dice algo más que la carpeta.
        if !agent.name.is_empty() && Path::new(&agent.folder).file_name().is_none_or(|f| f.to_string_lossy() != agent.name) {
            place += &format!(" («{}»)", agent.name);
        }
        lines.push(format!("{} trabajó {}{place}", agent.label, spoken(agent.ms)));
    }
    if heard >= 60_000 {
        let top: Vec<String> = found.media.iter().take(3).map(track).collect();
        lines.push(format!("Sonó {}: {}", spoken(heard), top.join(", ")));
    }
    for device in found.devices.iter().filter(|d| d.ms >= 60_000) {
        let mut line = format!("{} {}", device_name(&device.kind), spoken(device.ms));
        if device.times > 1 {
            line += &format!(" en {} veces", device.times);
        }
        if !device.app.is_empty() {
            line += &format!(", sobre todo en {}", apps::label(apps_known, &device.app));
            if !device.site.is_empty() {
                line += &format!(" ({})", device.site);
            }
        }
        lines.push(line);
    }
    if !found.files.is_empty() {
        let mut names: Vec<String> = found.files.iter().take(4).map(|f| file_name(&f.path)).collect();
        if found.files.len() > 4 {
            names.push(format!("{} más", found.files.len() - 4));
        }
        lines.push(format!("Archivos: {}", and(&names)));
    }
    let say = if found.total >= 60_000 { format!("{label}: {} de pantalla", spoken(found.total)) } else { format!("{label}: casi nada de pantalla") };
    if lines.is_empty() { Outcome::done(say) } else { Outcome::tell(say, lines.join("\n")) }
}

/// «Micro abierto», «Cámara encendida».
fn device_name(kind: &str) -> &str {
    match kind {
        "micro" => "Micro abierto",
        "cámara" => "Cámara encendida",
        "pantalla" => "Pantalla compartida",
        other => other,
    }
}

/// «Song» de Artist.
fn track(m: &journal::MediaTime) -> String {
    if m.artist.is_empty() { format!("«{}»", short(&m.title, 50)) } else { format!("«{}» de {}", short(&m.title, 50), short(&m.artist, 30)) }
}

fn file_name(path: &str) -> String {
    Path::new(path).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string())
}

/// El tamaño de los ratos de la línea del tiempo: más largos cuanto más tiempo.
fn slot_for(length: i64) -> i64 {
    const MIN: i64 = 60_000;
    if length <= 4 * 60 * MIN {
        5 * MIN
    } else if length <= 26 * 60 * MIN {
        15 * MIN
    } else {
        60 * MIN
    }
}

/// «¿Qué hice ayer, hora a hora?»: lo que más tuvo el foco en cada rato.
pub fn timeline(d: &mut Daemon, said: &str) -> Outcome {
    let span = match span_of(said) {
        Ok(span) => span,
        Err(why) => return why,
    };
    let (from, to) = (span.from.timestamp_millis(), span.to.timestamp_millis());
    let midnight =
        span.from.date_naive().and_hms_opt(0, 0, 0).and_then(|t| t.and_local_timezone(Local).earliest()).map(|t| t.timestamp_millis()).unwrap_or(from);
    let blocks = match diary(d).and_then(|db| journal::timeline(db, from, to, slot_for(to - from), midnight).map_err(failed)) {
        Ok(blocks) => blocks,
        Err(why) => return why,
    };
    let label = capital(&span.label);
    if blocks.iter().all(|b| b.app.is_empty()) {
        return Outcome::done(format!("{label}: nada en el diario"));
    }
    let today = Local::now().date_naive();
    let several = date(from) != date(to - 1);
    let mut lines = Vec::new();
    let mut day = None;
    for block in &blocks {
        if several && day != Some(date(block.start)) {
            day = Some(date(block.start));
            lines.push(capital(&day_short(date(block.start), today)));
        }
        let what = if block.app.is_empty() {
            away_alone(&block.away).to_string()
        } else {
            let name = apps::label(&d.apps, &block.app);
            let mut parts = vec![name.clone()];
            if !block.place.is_empty() {
                parts.push(if block.place.starts_with('/') { config::tilde(&block.place) } else { block.place.clone() });
            }
            let folder = block.place.starts_with('/');
            if block.command {
                parts.push(block.detail.clone());
            } else if !block.detail.is_empty() && (!folder || telling(&block.detail, &block.place)) {
                parts.push(format!("«{}»", short(&page(&block.detail, &name, &block.place), 50)));
            }
            parts.join(" · ")
        };
        lines.push(format!("{}–{}  {what}", hm(block.start), hm(block.end)));
    }
    Outcome::tell(format!("{label}, rato a rato"), lines.join("\n"))
}

/// «¿Cuándo vi lo de Fallout?»: dónde sale en el diario, lo último primero.
pub fn find(d: &mut Daemon, what: &str) -> Outcome {
    let words = significant(what);
    if words.is_empty() {
        return Outcome::fail("¿Qué busco?");
    }
    let apps_known = d.apps.clone();
    let hits = match diary(d).and_then(|db| journal::find(db, &words, |app| apps::label(&apps_known, app), 8).map_err(failed)) {
        Ok(hits) => hits,
        Err(why) => return why,
    };
    let what = what.trim();
    let Some(last) = hits.first() else {
        return Outcome::done(format!("«{what}» no sale en el diario"));
    };
    let today = Local::now().date_naive();
    let name = |hit: &journal::Hit| if hit.kind == "focus" { apps::label(&apps_known, &hit.app) } else { hit.app.clone() };
    let doing = match last.kind {
        "media" => format!("sonando en {}", name(last)),
        "file" => format!("lo abrió {}", name(last)),
        _ => format!("en {}", name(last)),
    };
    let say = format!("«{what}»: la última vez, {} a las {}, {doing}", day_name(date(last.start), today), hm(last.start));
    let lines: Vec<String> = hits
        .iter()
        .map(|hit| {
            let when = if hit.end - hit.start >= 60_000 { format!("{}–{}", hm(hit.start), hm(hit.end)) } else { hm(hit.start) };
            let app = name(hit);
            let mut parts = vec![app.clone()];
            match hit.kind {
                "file" => parts.push(config::tilde(&hit.place)),
                _ if !hit.place.is_empty() => parts.push(if hit.place.starts_with('/') { config::tilde(&hit.place) } else { hit.place.clone() }),
                _ => {}
            }
            if hit.kind != "file" && !hit.title.is_empty() {
                parts.push(format!("«{}»", short(&page(&hit.title, &app, &hit.place), 50)));
            }
            format!("{}, {when}: {}", capital(&day_short(date(hit.start), today)), parts.join(" · "))
        })
        .collect();
    Outcome::tell(say, lines.join("\n"))
}

/// «¿Cuánto rato estuve en YouTube esta semana?»; sin nada que buscar, todo lo de pantalla.
fn spent(d: &mut Daemon, what: &str, said: &str) -> Outcome {
    let span = match span_of(said) {
        Ok(span) => span,
        Err(why) => return why,
    };
    let (from, to) = (span.from.timestamp_millis(), span.to.timestamp_millis());
    let words = significant(what);
    let apps_known = d.apps.clone();
    let rows = match diary(d).and_then(|db| journal::rows(db, from, to).map_err(failed)) {
        Ok(rows) => rows,
        Err(why) => return why,
    };
    let need = if words.len() >= 3 { words.len() - 1 } else { words.len() };
    let mut labels: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut total = 0;
    let mut days: Vec<(NaiveDate, i64)> = Vec::new();
    let mut titles: Vec<(String, i64)> = Vec::new();
    for row in &rows {
        let name = labels.entry(row.app.clone()).or_insert_with(|| apps::label(&apps_known, &row.app)).clone();
        if !words.is_empty() && journal::matches(&row.hay(&name), &words) < need {
            continue;
        }
        let ms = row.end - row.start;
        total += ms;
        match days.iter_mut().find(|(day, _)| *day == date(row.start)) {
            Some((_, t)) => *t += ms,
            None => days.push((date(row.start), ms)),
        }
        let title = if row.title.is_empty() { name.clone() } else { format!("{name} · {}", page(&row.title, &name, &row.site)) };
        match titles.iter_mut().find(|(t, _)| *t == title) {
            Some((_, t)) => *t += ms,
            None => titles.push((title, ms)),
        }
    }
    let label = capital(&span.label);
    let what = what.trim();
    let on = if words.is_empty() { "de pantalla".to_string() } else { format!("en «{what}»") };
    if total < 60_000 {
        return Outcome::done(if words.is_empty() { format!("{label}: nada en el diario") } else { format!("{label}: nada de «{what}»") });
    }
    let mut lines = Vec::new();
    let today = Local::now().date_naive();
    if days.len() > 1 {
        for (day, ms) in &days {
            lines.push(format!("{}: {}", capital(&day_short(*day, today)), spoken(*ms)));
        }
    }
    titles.sort_by_key(|(_, ms)| std::cmp::Reverse(*ms));
    for (title, ms) in titles.iter().filter(|(_, ms)| *ms >= 60_000).take(4) {
        lines.push(format!("{}, {}", short(title, 70), spoken(*ms)));
    }
    let say = format!("{label}: {} {on}", spoken(total));
    if lines.is_empty() { Outcome::done(say) } else { Outcome::tell(say, lines.join("\n")) }
}

/// «¿Qué sonó ayer?»
fn heard(d: &mut Daemon, said: &str) -> Outcome {
    let span = match span_of(said) {
        Ok(span) => span,
        Err(why) => return why,
    };
    let found = match diary(d).and_then(|db| journal::recall(db, span.from.timestamp_millis(), span.to.timestamp_millis()).map_err(failed)) {
        Ok(found) => found,
        Err(why) => return why,
    };
    let label = capital(&span.label);
    let total: i64 = found.media.iter().map(|m| m.ms).sum();
    if found.media.is_empty() {
        return Outcome::done(format!("{label} no sonó nada"));
    }
    let lines: Vec<String> = found.media.iter().take(10).map(|m| format!("{} ({}), {}", track(m), m.player, spoken(m.ms.max(60_000)))).collect();
    Outcome::tell(format!("{label} sonó {}", spoken(total.max(60_000))), lines.join("\n"))
}

/// «¿Qué archivos abrí ayer?»
fn opened(d: &mut Daemon, said: &str) -> Outcome {
    let span = match span_of(said) {
        Ok(span) => span,
        Err(why) => return why,
    };
    let found = match diary(d).and_then(|db| journal::recall(db, span.from.timestamp_millis(), span.to.timestamp_millis()).map_err(failed)) {
        Ok(found) => found,
        Err(why) => return why,
    };
    let label = capital(&span.label);
    if found.files.is_empty() {
        return Outcome::done(format!("{label}: ningún archivo en el diario"));
    }
    let several = found.files.iter().any(|f| date(f.time) != date(found.files[0].time));
    let today = Local::now().date_naive();
    let lines: Vec<String> = found
        .files
        .iter()
        .take(12)
        .map(|f| {
            let when = if several { format!("{}, {}", capital(&day_short(date(f.time), today)), hm(f.time)) } else { hm(f.time) };
            let folder = Path::new(&f.path).parent().map(|p| config::tilde(&p.to_string_lossy())).unwrap_or_default();
            format!("{when} · {} · {} · {folder}", file_name(&f.path), f.app)
        })
        .collect();
    let count = found.files.len();
    Outcome::tell(format!("{label}: {count} archivo{}", if count == 1 { "" } else { "s" }), lines.join("\n"))
}

/// Las preguntas al diario.
fn diary_order(d: &mut Daemon, p: &Parse) -> Outcome {
    let w = p.text("w").to_string();
    let q = p.text("q").to_string();
    match p.intent.as_str() {
        "recall.find" => find(d, &q),
        "recall.time" => spent(d, &q, &w),
        "recall.timeline" => timeline(d, &w),
        "recall.media" => heard(d, &w),
        "recall.files" => opened(d, &w),
        "recall.full" => report(d, &w),
        "recall.close" => Outcome::from(veyl(d, "diary.close", json!([])).map(|_| String::new())),
        "recall.open" => {
            let day = match span_of(&w) {
                Ok(span) => span.from.date_naive().format("%Y-%m-%d").to_string(),
                Err(why) => return why,
            };
            Outcome::from(veyl(d, "diary.open", json!([day, q])).map(|_| String::new()))
        }
        _ => recall(d, &w),
    }
}

fn agents(d: &Daemon, who: &str) -> Outcome {
    let q = fold(who.trim());
    let busy: Vec<_> = d
        .agents
        .iter()
        .filter(|a| a.phase != "idle")
        .filter(|a| q.is_empty() || [&a.label, &a.provider, &a.name].iter().any(|t| apps::likeness(&q, t) > 0.0))
        .collect();
    if busy.is_empty() {
        return Outcome::done(if q.is_empty() { "Ningún agente trabajando".to_string() } else { format!("{} no está trabajando", capital(who.trim())) });
    }
    let lines: Vec<String> = busy
        .iter()
        .map(|a| {
            let state = if a.phase == "needs" { "te necesita".to_string() } else { format!("trabaja desde hace {}", spoken(crate::daemon::now() - a.since)) };
            let place = if a.name.is_empty() { String::new() } else { format!(" en «{}»", a.name) };
            format!("{}{place}: {state}", a.label)
        })
        .collect();
    let needing = busy.iter().filter(|a| a.phase == "needs").count();
    let say = match (busy.len(), needing) {
        (1, 1) => format!("{} te necesita", busy[0].label),
        (1, _) => format!("{} está trabajando", busy[0].label),
        (n, 0) => format!("{n} agentes trabajando"),
        (n, k) => format!("{n} agentes; {k} te necesita{}", if k > 1 { "n" } else { "" }),
    };
    Outcome::tell(say, lines.join("\n"))
}

fn clock(p: &Parse) -> Outcome {
    let now = Local::now();
    if p.intent == "clock.time" {
        let time = now.format("%H:%M");
        return Outcome::done(if now.hour() == 1 { format!("Es la {time}") } else { format!("Son las {time}") });
    }
    let date = now.date_naive();
    Outcome::done(format!("Hoy es {} {} de {} de {}", weekday(date), date.day(), month(date), date.year()))
}

// --- Buscar y poner -------------------------------------------------------------

/// Si el nombre es el de un navegador (o el navegador de siempre).
fn browser_named(d: &Daemon, name: &str) -> bool {
    !name.is_empty() && (play::is_browser(name) || find_app(d, name).zip(find_app(d, "navegador")).is_some_and(|(a, b)| a.id == b.id))
}

/// Abre una web: en el navegador del que se habla, si es uno (y, si se
/// acaba de abrir, cuando salga su ventana); si no, en el de siempre.
fn browse(d: &mut Daemon, browser: &str, url: &str) -> Result<(), String> {
    if browser_named(d, browser)
        && let Some(app) = find_app(d, browser)
    {
        let _ = fresh_window(d, browser);
        // La dirección va codificada: no lleva comillas.
        let command = format!("{} '{url}'", app.command(&d.settings.terminal));
        launch(d, &command)?;
        remember(d, browser, None);
        return Ok(());
    }
    detach(&["xdg-open", url]);
    Ok(())
}

/// El buscador de «busca …»: el que se dice o, si no, el de lo de antes
/// (Spotify, YouTube) o Google.
fn engine(p: &Parse) -> String {
    let named = p.text("where");
    if !named.is_empty() {
        return named.to_string();
    }
    match play::player(p.text("via")) {
        Some(Player::Spotify) => "spotify".into(),
        Some(Player::YouTube) => "youtube".into(),
        None => String::new(),
    }
}

/// Que Spotify ponga algo. Si no está en marcha, se abre (o, si se acaba de
/// abrir, se le espera) y se le pide por MPRIS en cuanto contesta.
fn spotify_put(d: &mut Daemon, uri: &str) -> Result<(), String> {
    if !play::spotify_running() {
        let launching = fold(&d.context.app).contains("spotify") && d.context.launched.as_ref().is_some_and(|(at, _)| at.elapsed() < Duration::from_secs(30));
        if !launching {
            let app = find_app(d, "spotify").ok_or("No tienes Spotify")?;
            launch(d, &app.command(&d.settings.terminal))?;
        }
        let start = Instant::now();
        while !play::spotify_running() {
            if start.elapsed() > Duration::from_secs(15) {
                return Err("Spotify no contesta".into());
            }
            thread::sleep(Duration::from_millis(300));
        }
        // Recién despierto, no hace caso a la primera.
        thread::sleep(Duration::from_millis(1500));
    }
    play::spotify_open(uri).map_err(|e| format!("Spotify no lo pone: {e}"))?;
    remember(d, "spotify", None);
    Ok(())
}

fn web(d: &mut Daemon, p: &Parse) -> Result<String, String> {
    let via = p.text("via").to_string();
    if p.intent == "web.open" {
        let site = p.text("site");
        let (name, url) =
            play::site(site).or_else(|| (p.flag("lucky") == Some(true)).then(|| play::lucky(site))).ok_or_else(|| format!("No conozco la web «{site}»"))?;
        if p.flag("then") != Some(true) {
            browse(d, &via, &url)?;
        }
        remember(d, p.text("site"), None);
        return Ok(format!("Abro {name}"));
    }
    let query = p.text("q").trim().to_string();
    if query.is_empty() {
        return Err("¿Qué busco?".into());
    }
    let engine = engine(p);
    if engine == "spotify" {
        spotify_put(d, &format!("spotify:search:{}", play::encode(&query)))?;
        return Ok(format!("Busco «{query}» en Spotify"));
    }
    let url = play::search_url(&engine, &query).ok_or_else(|| format!("No sé buscar en {engine}"))?;
    browse(d, &via, &url)?;
    Ok(format!("Busco «{query}» en {}", play::engine_name(&engine)))
}

/// «Pon …»: en Spotify, su canción, disco o artista; en YouTube, su primer vídeo.
fn play_order(d: &mut Daemon, p: &Parse) -> Result<String, String> {
    let query = play::tidy(p.text("q"));
    if query.is_empty() {
        return Err("¿Qué pongo?".into());
    }
    let via = p.text("via").to_string();
    let target = match p.text("where") {
        "spotify" => Player::Spotify,
        "youtube" => Player::YouTube,
        _ => play::player(&via).or_else(|| browser_named(d, &via).then_some(Player::YouTube)).unwrap_or(if find_app(d, "spotify").is_some() {
            Player::Spotify
        } else {
            Player::YouTube
        }),
    };
    match target {
        Player::YouTube => {
            let browser = if browser_named(d, &via) { via.clone() } else { String::new() };
            let (url, title) = match play::youtube(&query) {
                Some((id, title)) => (format!("https://www.youtube.com/watch?v={id}"), title),
                None => (play::search_url("youtube", &query).unwrap_or_default(), String::new()),
            };
            browse(d, &browser, &url)?;
            remember(d, "youtube", None);
            Ok(if title.is_empty() { format!("Busco «{query}» en YouTube") } else { format!("Pongo «{title}»") })
        }
        Player::Spotify => match play::spotify(&query) {
            Some(uri) => {
                spotify_put(d, &uri)?;
                let title = play::spotify_title(&uri).unwrap_or(query);
                Ok(format!("Pongo «{title}» en Spotify"))
            }
            None => {
                spotify_put(d, &format!("spotify:search:{}", play::encode(&query)))?;
                Ok("No lo encuentro tal cual: te lo busco en Spotify".into())
            }
        },
    }
}

/// Lo dicho, hecho.
/// Quién lo dijo (`source`) importa a lo que acaba más tarde: se le cuenta a él.
pub fn run(d: &mut Daemon, p: &Parse, source: &str) -> Outcome {
    let group = p.intent.split('.').next().unwrap_or("");
    match group {
        "volume" => Outcome::from(sound(p, SINK, "Volumen")),
        "mic" => Outcome::from(sound(p, SOURCE, "Micro")),
        "media" => Outcome::from(media(p)),
        "window" | "app" | "level" => Outcome::from(windows(d, p)),
        "session" | "veyl" => Outcome::from(session(d, p)),
        "radio" => Outcome::from(radio(p)),
        "quiet" => {
            let on = p.flag("on").unwrap_or(true);
            Outcome::from(veyl(d, "quiet.set", json!([on])).map(|_| if on { "No molestar".into() } else { "Se acabó el no molestar".into() }))
        }
        "listen" => Outcome::from(veyl(d, "listen.set", json!([p.flag("on").unwrap_or(false)])).map(|_| "Dejo de escuchar".into())),
        "timer" => Outcome::from(timer(d, p)),
        "event" => Outcome::from(event(d, p)),
        "agenda" => agenda(p),
        "recall" => diary_order(d, p),
        "context" => Outcome::fail("¿Adónde? No sé de qué hablábamos"),
        "clock" => clock(p),
        "agents" => agents(d, p.text("who")),
        "scheme" => {
            let name = p.text("name").to_string();
            Outcome::from(scheme(d, &name))
        }
        "launcher" => {
            let view = p.text("view").to_string();
            Outcome::from(veyl(d, "launcher.open", json!([view])).map(|_| String::new()))
        }
        "shot" => Outcome::from(shot(d, p)),
        "view" => views_order(d, p, source),
        // Lo normal es que veyld lo mida en un hilo aparte (daemon::sense); aquí, dentro de un plan.
        "system" => crate::system::run(&p.intent, &crate::system::seen(d)).0,
        "web" => Outcome::from(web(d, p)),
        "play" => Outcome::from(play_order(d, p)),
        _ => Outcome::fail(format!("No sé hacer {}", p.intent)),
    }
}

/// Cómo se llama lo dicho, para enseñarlo antes de hacerlo, y si se puede hacer.
/// Cómo se enseña un plan: una orden, como ella; varias, juntas («Abrir
/// Firefox · Llevar Firefox al nivel 2»), listas si lo están todas y
/// peligrosas si lo es alguna.
pub fn describe_plan(d: &Daemon, plan: &[Parse]) -> Preview {
    if let [one] = plan {
        return describe(d, one);
    }
    let shown: Vec<Preview> = plan.iter().map(|p| describe(d, p)).collect();
    Preview {
        title: shown.iter().map(|s| s.title.as_str()).collect::<Vec<_>>().join(" · "),
        glyph: shown.first().map(|s| s.glyph).unwrap_or("list-checks"),
        confirm: shown.iter().any(|s| s.confirm),
        ready: doable(d, plan),
    }
}

pub fn describe(d: &Daemon, p: &Parse) -> Preview {
    let mut preview = name(d, p);
    let app = p.text("app");
    let found = |query: &str| terminal_asked(query) || find_app(d, query).is_some();
    preview.ready &= match p.intent.as_str() {
        "app.open" => found(app),
        "window.focus" => found(app) || pick(d, app).is_some(),
        "window.others" => !others(d, p).is_empty(),
        "web.open" => p.flag("lucky") == Some(true) || play::site(p.text("site")).is_some(),
        // «pon …» a secas es música solo si se sabe dónde: si no, mejor el modelo.
        "play.search" => {
            !p.text("where").is_empty() || p.flag("music") == Some(true) || play::player(p.text("via")).is_some() || browser_named(d, p.text("via"))
        }
        "context.go" => false,
        intent if TARGETED.contains(&intent) && !app.is_empty() => pick(d, app).is_some() || !p.text("window").is_empty(),
        _ => true,
    };
    preview
}

fn name(d: &Daemon, p: &Parse) -> Preview {
    let n = p.number("n");
    let app = p.text("app");
    let shown = |title: String, glyph: &'static str| Preview { title, glyph, confirm: false, ready: true };
    let on = p.flag("on").unwrap_or(true);
    let app_name_of = |query: &str| find_app(d, query).map(|a| a.name.clone()).unwrap_or_else(|| capital(query));
    let app_name = || app_name_of(app);
    let window_name = || if app.is_empty() { "la ventana".to_string() } else { app_name() };
    let of = || if app.is_empty() { String::new() } else { format!(": {}", app_name()) };
    match p.intent.as_str() {
        "volume.set" => shown(format!("Poner el volumen al {} %", n.unwrap_or(0).min(100)), "speaker-high"),
        "volume.up" => shown(n.map(|n| format!("Subir el volumen {n} puntos")).unwrap_or("Subir el volumen".into()), "speaker-high"),
        "volume.down" => shown(n.map(|n| format!("Bajar el volumen {n} puntos")).unwrap_or("Bajar el volumen".into()), "speaker-low"),
        "volume.mute" if on => shown("Silenciar el sonido".into(), "speaker-x"),
        "volume.mute" => shown("Quitar el silencio".into(), "speaker-high"),
        "mic.set" => shown(format!("Poner el micro al {} %", n.unwrap_or(0).min(100)), "microphone"),
        "mic.up" => shown("Subir el micro".into(), "microphone"),
        "mic.down" => shown("Bajar el micro".into(), "microphone"),
        "mic.mute" if on => shown("Silenciar el micro".into(), "microphone-slash"),
        "mic.mute" => shown("Abrir el micro".into(), "microphone"),
        "media.pause" => shown("Pausar".into(), "pause"),
        "media.play" => shown("Reproducir".into(), "play"),
        "media.next" => shown("Pista siguiente".into(), "skip-forward"),
        "media.previous" => shown("Pista anterior".into(), "skip-back"),
        "media.what" => shown("Qué suena".into(), "music-notes"),
        "window.focus" => shown(format!("Ir a {}", app_name()), "browsers"),
        "app.open" => shown(format!("Abrir {}", app_name()), "app-window"),
        "window.close" if app.is_empty() => shown("Cerrar la ventana".into(), "x-circle"),
        "window.close" => shown(format!("Cerrar {}", app_name()), "x-circle"),
        "window.fullscreen" if p.flag("on") == Some(false) => shown(format!("Quitar la pantalla completa{}", of()), "corners-in"),
        "window.fullscreen" if p.text("mode") == "maximized" => shown(format!("Maximizar {}", window_name()), "arrows-out"),
        "window.fullscreen" => shown(format!("Pantalla completa{}", of()), "corners-out"),
        "window.float" => shown(format!("Hacer flotante {}", window_name()), "picture-in-picture"),
        "window.place" if p.text("side") == "left" => shown(format!("Poner {} a la izquierda", window_name()), "arrow-line-left"),
        "window.place" => shown(format!("Poner {} a la derecha", window_name()), "arrow-line-right"),
        "window.monitor" if n.is_some() => shown(format!("Llevar {} al monitor {}", window_name(), n.unwrap_or(1)), "monitor"),
        "window.monitor" if p.text("dir") == "other" => shown(format!("Llevar {} al otro monitor", window_name()), "monitor"),
        "window.monitor" => shown(format!("Llevar {} al monitor {}", window_name(), side_name(p.text("dir"))), "monitor"),
        "context.go" => shown("Ir allí".into(), "arrow-bend-up-right"),
        "window.others" => {
            let count = others(d, p).len();
            let keep = p.text("keep");
            let but = match () {
                _ if p.flag("all") == Some(true) => String::new(),
                _ if keep.is_empty() || matches!(fold(keep).as_str(), "esta" | "esto" | "esta ventana" | "la actual") => ", menos esta".into(),
                _ => format!(", menos {}", app_name_of(keep)),
            };
            let place = if p.text("scope") == "level" { " de este nivel" } else { "" };
            Preview { confirm: true, ..shown(format!("Cerrar {count} ventana{}{place}{but}", if count == 1 { "" } else { "s" }), "x-square") }
        }
        "window.direction" if p.text("dir") == "left" => shown("Ventana de la izquierda".into(), "arrow-left"),
        "window.direction" => shown("Ventana de la derecha".into(), "arrow-right"),
        "window.shift" if p.text("dir") == "left" => shown("Mover la ventana a la izquierda".into(), "arrow-line-left"),
        "window.shift" => shown("Mover la ventana a la derecha".into(), "arrow-line-right"),
        "window.move" if app.is_empty() => shown(format!("Llevar la ventana al nivel {}", n.unwrap_or(1)), "stack"),
        "window.move" => shown(format!("Llevar {} al nivel {}", app_name(), n.unwrap_or(1)), "stack"),
        "level.go" if n == Some(99) => shown("Ir al último nivel".into(), "stack"),
        "level.go" => shown(format!("Ir al nivel {}", n.unwrap_or(1)), "stack"),
        "level.step" if p.number("by").unwrap_or(1) < 0 => shown("Subir un nivel".into(), "arrow-up"),
        "level.step" => shown("Bajar un nivel".into(), "arrow-down"),
        "session.lock" => shown("Bloquear la pantalla".into(), "lock"),
        "session.suspend" => shown("Suspender".into(), "moon"),
        "session.poweroff" => Preview { title: "Apagar el ordenador".into(), glyph: "power", confirm: true, ready: true },
        "session.reboot" => Preview { title: "Reiniciar el ordenador".into(), glyph: "arrow-clockwise", confirm: true, ready: true },
        "session.logout" => Preview { title: "Cerrar la sesión".into(), glyph: "sign-out", confirm: true, ready: true },
        "veyl.reload" => shown("Recargar Veyl".into(), "arrows-clockwise"),
        "radio" => {
            let device = if p.text("device") == "bluetooth" { "el Bluetooth" } else { "el Wi-Fi" };
            let glyph = match (p.text("device") == "bluetooth", on) {
                (true, true) => "bluetooth",
                (true, false) => "bluetooth-slash",
                (false, true) => "wifi-high",
                (false, false) => "wifi-slash",
            };
            shown(format!("{} {device}", if on { "Encender" } else { "Apagar" }), glyph)
        }
        "quiet" if on => shown("Activar no molestar".into(), "bell-slash"),
        "quiet" => shown("Quitar no molestar".into(), "bell"),
        "listen" => shown("Dejar de escuchar".into(), "waveform"),
        "timer.start" => shown(format!("Temporizador de {}", spoken(((p.number("d").unwrap_or(60) + 59) / 60) * 60_000)), "timer"),
        "timer.stop" => shown("Parar el temporizador".into(), "timer"),
        "timer.left" => shown("Lo que queda del temporizador".into(), "timer"),
        "event.add" => {
            let at = when::parse(&tokens(p.text("w"))).and_then(|w| when::ahead(&w, Local::now()));
            let at = at.map(|(date, time)| format!(", {}", moment_name(date, time))).unwrap_or_default();
            shown(format!("Apuntar «{}»{at}", p.text("title")), "calendar-plus")
        }
        "agenda.ask" => shown("Lo que tienes apuntado".into(), "calendar"),
        intent if intent.starts_with("recall") => {
            let span = when::parse(&tokens(p.text("w"))).and_then(|w| when::past(&w, Local::now()));
            let label = span.map(|s| s.label).unwrap_or_else(|| "hoy".into());
            let q = p.text("q").trim();
            match intent {
                "recall.find" => shown(format!("Buscar «{q}» en el diario"), "magnifying-glass"),
                "recall.time" if q.is_empty() => shown(format!("Cuánto rato de pantalla {label}"), "hourglass-medium"),
                "recall.time" => shown(format!("Cuánto rato en «{q}» {label}"), "hourglass-medium"),
                "recall.timeline" => shown(format!("Lo que hiciste {label}, rato a rato"), "list-bullets"),
                "recall.media" => shown(format!("Lo que sonó {label}"), "music-notes"),
                "recall.files" => shown(format!("Los archivos que abriste {label}"), "files"),
                "recall.full" => shown(format!("Todo lo que hiciste {label}"), "list-checks"),
                "recall.close" => shown("Cerrar el diario".into(), "notebook"),
                "recall.open" if !q.is_empty() => shown(format!("Buscar «{q}» en el diario"), "notebook"),
                "recall.open" => shown(format!("Abrir el diario {label}"), "notebook"),
                _ => shown(format!("Lo que estuviste mirando {label}"), "clock-counter-clockwise"),
            }
        }
        "clock.time" => shown("Qué hora es".into(), "clock"),
        "clock.date" => shown("Qué día es".into(), "calendar-blank"),
        "agents.ask" => shown("Lo que hacen los agentes".into(), "robot"),
        "scheme.set" => match find_scheme(d, p.text("name")) {
            Some((_, name)) => shown(format!("Esquema {name}"), "palette"),
            None => Preview { ready: false, ..shown(format!("Esquema «{}»", p.text("name")), "palette") },
        },
        "view.create" if p.text("from") == "level" => shown(format!("Guardar este nivel como la vista «{}»", views::tidy(p.text("name"))), "squares-four"),
        "view.create" => shown(format!("Crear la vista «{}» con lo que tienes abierto", views::tidy(p.text("name"))), "squares-four"),
        "view.open" | "view.undo" | "view.save" | "view.forget" => {
            let asked = p.text("name");
            let found = if asked.trim().is_empty() {
                let here = d.hypr.json("activeworkspace").and_then(|w| Some(w.get("name")?.as_str()?.to_string())).unwrap_or_default();
                d.views.of_workspace(&here)
            } else {
                d.views.find(asked)
            };
            let called = found.map(|i| d.views.list[i].name.clone()).unwrap_or_else(|| views::tidy(asked));
            let named = if called.is_empty() { "esta vista".to_string() } else { format!("la vista «{called}»") };
            let ready = found.is_some() || (p.intent == "view.save" && !asked.trim().is_empty());
            let (title, glyph, confirm) = match p.intent.as_str() {
                "view.open" if p.flag("again") == Some(true) => (format!("Recrear {named}"), "arrows-clockwise", false),
                "view.open" => (format!("Abrir {named}"), "squares-four", false),
                "view.undo" => (format!("Deshacer {named}"), "arrow-u-up-left", false),
                "view.save" => (format!("Guardar este nivel en {named}"), "floppy-disk", false),
                _ => (format!("Olvidar {named}"), "trash", true),
            };
            Preview { title, glyph, confirm, ready }
        }
        "view.list" => shown("Las vistas".into(), "squares-four"),
        "web.search" => shown(format!("Buscar «{}» en {}", p.text("q"), play::engine_name(&engine(p))), "magnifying-glass"),
        "web.open" => shown(format!("Abrir {}", play::site(p.text("site")).map_or(p.text("site").to_string(), |s| s.0)), "globe"),
        "play.search" => shown(format!("Poner «{}»", play::tidy(p.text("q"))), "play"),
        intent if intent.starts_with("system.") => {
            let (title, glyph) = crate::system::title(intent);
            shown(title.into(), glyph)
        }
        "launcher" if p.text("view") == "clipboard" => shown("Abrir el portapapeles".into(), "clipboard-text"),
        "launcher" => shown("Abrir la entrada".into(), "magnifying-glass"),
        "shot" if p.text("what") == "area" => shown("Capturar una zona".into(), "selection"),
        "shot" => shown("Capturar la pantalla".into(), "monitor"),
        "confirm" => shown("Confirmar".into(), "check"),
        "cancel" => shown("Cancelar".into(), "x-circle"),
        other => shown(other.into(), "question"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn steps() {
        assert_eq!(step(42, 1, None), 45);
        assert_eq!(step(45, 1, None), 50);
        assert_eq!(step(42, -1, None), 40);
        assert_eq!(step(40, -1, None), 35);
        assert_eq!(step(97, 1, Some(10)), 100);
        assert_eq!(step(3, -1, Some(10)), 0);
    }

    #[test]
    fn titles() {
        assert_eq!(tidy("Rust by Example — Mozilla Firefox", "Firefox"), "Rust by Example");
        assert_eq!(tidy("main.rs - veyld - Visual Studio Code", "Visual Studio Code"), "main.rs - veyld");
        assert_eq!(tidy("nixos-pc: veyl", "kitty"), "nixos-pc: veyl");
    }

    #[test]
    fn recall_text() {
        let part = |name: &str, ms: i64, within: &[(&str, i64)]| journal::Part {
            name: name.into(),
            ms,
            within: within.iter().map(|(w, ms)| (w.to_string(), *ms)).collect(),
        };
        let at = |h: u32, m: u32| Local.with_ymd_and_hms(2026, 11, 12, h, m, 0).unwrap().timestamp_millis();
        let found = journal::Recall {
            total: 13_200_000,
            first: at(15, 4),
            last: at(20, 41),
            apps: vec![
                journal::AppTime {
                    app: "firefox".into(),
                    ms: 5_100_000,
                    titles: vec![("Rust by Example — Mozilla Firefox".into(), 3_000_000), ("YouTube — Mozilla Firefox".into(), 2_100_000)],
                    sites: vec![
                        part("doc.rust-lang.org", 3_000_000, &[("Rust by Example — Mozilla Firefox", 3_000_000)]),
                        part("youtube.com", 2_100_000, &[("Fallout 3 - YouTube — Mozilla Firefox", 1_500_000)]),
                    ],
                    ..Default::default()
                },
                journal::AppTime {
                    app: "kitty".into(),
                    ms: 3_300_000,
                    titles: vec![("fish".into(), 3_300_000)],
                    folders: vec![part("/x/dev/veyl", 3_300_000, &[("claude", 2_000_000), ("cargo", 900_000), ("ls", 10_000)])],
                    ..Default::default()
                },
                journal::AppTime { app: "zed".into(), ms: 600_000, titles: vec![("main.rs — veyld".into(), 600_000)], ..Default::default() },
            ],
            agents: vec![journal::AgentTime { label: "Claude".into(), name: "veyld".into(), folder: "/x/dev/veyl".into(), ms: 4_200_000 }],
            media: vec![journal::MediaTime { player: "Spotify".into(), title: "Song".into(), artist: "Band".into(), ms: 2_400_000 }],
            devices: vec![journal::DeviceTime { kind: "micro".into(), ms: 2_700_000, times: 2, app: "firefox".into(), site: "meet.google.com".into() }],
            files: vec![crate::recent::Used { path: "/x/informe.pdf".into(), app: "Papers".into(), time: at(16, 0) }],
            away: vec![("bloqueado".into(), 2_400_000), ("inactivo".into(), 30_000)],
        };
        let out = describe_recall(&[], "el jueves 12 de noviembre por la tarde (15:00–21:00)", &found);
        assert_eq!(out.say, "El jueves 12 de noviembre por la tarde (15:00–21:00): 3 h 40 min de pantalla");
        assert_eq!(
            out.answer.unwrap(),
            "De 15:04 a 20:41, con 40 min con la pantalla bloqueada
firefox, 1 h 25 min: doc.rust-lang.org («Rust by Example»), youtube.com («Fallout 3»)
kitty, 55 min, en /x/dev/veyl (claude, cargo)
zed, 10 min: «main.rs — veyld»
Claude trabajó 1 h 10 min en /x/dev/veyl («veyld»)
Sonó 40 min: «Song» de Band
Micro abierto 45 min en 2 veces, sobre todo en firefox (meet.google.com)
Archivos: informe.pdf"
        );
    }

    #[test]
    fn search_words() {
        assert_eq!(significant("el vídeo de Fallout 3"), vec!["fallout", "3"]);
        assert_eq!(significant("lo del dentista"), vec!["dentista"]);
        assert_eq!(page("Fallout 3 - YouTube — Mozilla Firefox", "Firefox", "youtube.com"), "Fallout 3");
    }
}
