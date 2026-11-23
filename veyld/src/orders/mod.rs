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

mod diary;
mod windows;

use diary::*;
pub use diary::{diary_json, find, find_json, index, recall, recall_json, report, timeline};
pub use windows::make_view;
use windows::*;

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

fn shot(d: &mut Daemon, p: &Parse) -> Result<String, String> {
    need("grim")?;
    need("wl-copy")?;
    let area = p.text("what") == "area";
    if area {
        need("slurp")?;
    }
    let monitor = d.hypr.monitor().unwrap_or_default();
    // Veyl la guarda, la avisa con su imagen y destella; sin él, solo al portapapeles.
    if veyl(d, "shot.take", json!([monitor, area, 0])).is_ok() {
        return Ok(if area { "Elige la zona" } else { "Captura copiada" }.into());
    }
    if area {
        detach(&["sh", "-c", r#"area=$(slurp) && grim -g "$area" - | wl-copy --type image/png"#]);
        return Ok("Elige la zona".into());
    }
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

    #[test]
    fn steps() {
        assert_eq!(step(42, 1, None), 45);
        assert_eq!(step(45, 1, None), 50);
        assert_eq!(step(42, -1, None), 40);
        assert_eq!(step(40, -1, None), 35);
        assert_eq!(step(97, 1, Some(10)), 100);
        assert_eq!(step(3, -1, Some(10)), 0);
    }
}
