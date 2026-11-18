//! El daemon: un hilo por cada cosa que escuchar (los avisos de Hyprland, los
//! agentes, la inactividad, el reloj, las señales y el socket) y este, que lo
//! junta todo, contesta y es el único que toca el diario.
//!
//! Por el socket ($XDG_RUNTIME_DIR/veyl/veyld-<sesión>.sock) se habla en
//! líneas JSON. Lo que se le manda:
//!
//! ```text
//! {"hello": "shell", "root": "…"}        Veyl: recibe los agentes y las llamadas
//! {"watch": "agents"}                    recibe los agentes cada vez que cambian
//! {"state": {"locked", "timer", "quiet", "scheme", "mic", "camera", "screen"}}   lo que sabe Veyl
//! {"id": 1, "say": "sube el volumen a 85", "source": "voice"}   una orden
//! {"id": 1, "say": "¿qué es un quark?", "ask": true}            directa al cerebro, sin gramática
//! {"id": 2, "parse": "…"}                qué entiende la gramática, sin hacerlo
//! {"id": 3, "recall": "el jueves por la tarde"}   (con "full": true, todo; con "json": true, en datos)
//! {"id": 3, "timeline": "ayer"}          lo mismo, rato a rato
//! {"id": 3, "find": "fallout"}           cuándo salió algo en el diario (con "json": true, en datos)
//! {"id": 3, "diary": "2026-11-19", "days": 14}   un día entero, en datos, para la vista del diario
//! {"id": 4, "tool": "veyl", "input": {"orden": "…"}}   una herramienta del cerebro (para `veyld mcp`)
//! {"id": 4, "agents": true}
//! {"view": "open", "name": "Estudio de uni"}   una vista, por su nombre: create (con
//!                                        "level": true, lo de este nivel), open, again, undo, save, forget
//! {"note": 7, "act": "default"}          pulsa una acción de una notificación
//! {"note": 7, "reply": "vale"}           responde desde ella
//! {"note": 7, "dismiss": true}           la descarta (y la borra)
//! {"notes": "clear"}                     las borra todas
//! {"calendars": "sync"}                  vuelve a traer los calendarios de fuera
//! {"calendar": {"add": {"calendar", "title", "day", "time"}}}   apunta en uno de Google
//! {"calendar": {"remove": {"calendar", "id"}}}                   borra en uno de Google
//! {"google": "status"}                   cómo está la cuenta (y sus calendarios)
//! {"google": "login", "client_id", "client_secret"}   conecta la cuenta (abre el navegador)
//! {"google": "logout"}                   la desconecta
//! {"hold": true}                         los avisos no caducan (el puntero está encima)
//! {"listen": true, "hold": true}         escucha una frase por el micro; hold: con la tecla
//!                                        mantenida, hasta {"finish": true} (false: deja de escuchar)
//! ```
//!
//! Lo que manda: `{"id", …}` en respuesta a lo que lo llevaba, `{"agents":
//! […]}` y, a Veyl, `{"call": "timer.start", "args": [25]}` y las
//! notificaciones: todas al saludar (`{"notes": […]}`, la más reciente
//! primero), y luego cada una que llega o cambia (`{"note": {…}}`) y cada
//! una que se va (`{"note_gone": 7}`); y mientras escucha, `{"voice": {"level":
//! 0.4}}` cada 50 ms, `{"voice": {"speaking": true}}` al empezar a hablar y
//! `{"voice": {"heard": "…"}}` con lo dicho.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::agents::{self, Agent};
use crate::apps::{self, App};
use crate::brain;
use crate::bus::{self, Bus, Incoming};
use crate::calendars;
use crate::config::{self, Settings};
use crate::google;
use crate::grammar::{self, Grammar, Parse};
use crate::hypr::{Hypr, Window};
use crate::idle;
use crate::journal::{self, Journal, Seen};
use crate::media::{self, Playing};
use crate::notes::{Notes, Out};
use crate::orders::{self, Outcome};
use crate::procs;
use crate::recent;
use crate::tts;
use crate::views::{self, Candidate, Views};
use crate::voice::{self, Voice};
use crate::web::Web;

/// Cada cuánto se apunta en el diario hasta dónde llega lo abierto.
const BEAT: Duration = Duration::from_secs(15);
/// Lo que espera una orden peligrosa a que se confirme.
const PATIENCE: Duration = Duration::from_secs(20);

pub enum Event {
    Hypr(String, String),
    HyprLost,
    Agents(Vec<Agent>),
    /// Lo que suena ahora.
    Media(Vec<Playing>),
    /// El cerebro pide una herramienta (mirar el contexto, el diario, dar una orden).
    Tool {
        name: String,
        input: Value,
        source: String,
        reply: Sender<String>,
    },
    /// Lo que oye el micro mientras escucha.
    Voice(Voice),
    Idle(bool),
    Tick,
    Join(u64, UnixStream),
    Said(u64, Value),
    Left(u64),
    /// Lo que contestó el modelo.
    Answered {
        client: u64,
        id: Value,
        said: String,
        source: String,
        outcome: Outcome,
    },
    /// Lo medido del ordenador (system.*), con su tarjeta.
    Sensed {
        client: u64,
        id: Value,
        said: String,
        source: String,
        parse: Parse,
        outcome: Outcome,
        card: Option<Value>,
    },
    /// Una notificación que llega por D-Bus.
    Notified(Box<Incoming>),
    /// Su programa la retira.
    Withdrawn(u32),
    /// Caduca algún aviso.
    Due,
    /// El cerebro ha elegido qué ventanas van en una vista nueva.
    ViewChosen {
        name: String,
        candidates: Vec<Candidate>,
        here: i64,
        source: String,
        choice: Result<views::Choice, String>,
    },
    /// Lo que cuentan los calendarios (lo apuntado, lo borrado, la cuenta), para Veyl.
    Calendar(Value),
    Reload,
    Stop,
}

pub fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Lo que Veyl le cuenta.
#[derive(Clone, Debug, Default)]
pub struct Shell {
    pub locked: bool,
    /// Cuándo acaba el temporizador (ms); 0, parado.
    pub timer: i64,
    pub quiet: bool,
    pub scheme: String,
    /// El micro, la cámara y la pantalla compartida en uso.
    pub mic: bool,
    pub camera: bool,
    pub screen: bool,
    /// Dónde está Veyl (sus esquemas).
    pub root: String,
}

struct Client {
    stream: UnixStream,
    shell: bool,
    agents: bool,
}

pub struct Daemon {
    pub hypr: Hypr,
    pub journal: Option<Journal>,
    pub grammar: Grammar,
    pub apps: Vec<App>,
    apps_read: Instant,
    pub settings: Settings,
    pub shell: Shell,
    pub agents: Vec<Agent>,
    pub split: u32,
    clients: HashMap<u64, Client>,
    /// La ventana con el foco, se esté o no.
    current: Option<Window>,
    idle: bool,
    /// Lo que espera un «sí»: la orden peligrosa y las que iban detrás.
    pending: Option<(Vec<Parse>, Instant, String)>,
    /// De qué se está hablando, para «muévelo» o «ve allí».
    pub context: orders::Context,
    tx: Sender<Event>,
    pruned: i64,
    processes: HashMap<i32, procs::Stat>,
    children: HashMap<i32, Vec<i32>>,
    processes_read: Option<Instant>,
    pub notes: Notes,
    bus: Option<Bus>,
    /// Dónde se guardan las imágenes que llegan en datos.
    pictures: PathBuf,
    /// Las páginas de las pestañas del navegador.
    web: Web,
    /// Lo que suena.
    playing: Vec<Playing>,
    /// Cuándo cambió por última vez la lista de archivos recientes que se leyó.
    recent: Option<SystemTime>,
    /// Los relojes cuando se miraron por última vez (arranque, monótono, pared):
    /// si el de arranque avanza más que el monótono, la máquina durmió.
    clock: Option<(i64, i64, i64)>,
    /// El hilo que escucha, y si está escuchando (entonces el micro abierto es el suyo).
    ear: Option<Sender<voice::Order>>,
    listening: bool,
    /// El hilo que habla.
    mouth: Option<Sender<tts::Say>>,
    /// Lo hablado con el cerebro hace poco (cuándo, lo dicho, lo que contestó),
    /// y de dónde vino lo último (lo que pida confirmar, lo confirma quien habló).
    talk: Vec<(Instant, String, String)>,
    talk_source: String,
    /// Cuántas preguntas está pensando el cerebro, y la última (para Veyl).
    pondering: usize,
    pondered: Value,
    /// La tarjeta que el cerebro ha compuesto para lo que piensa ahora, y si
    /// la respuesta va en ella (entonces, sin notificación).
    card: Option<Value>,
    carded: bool,
    /// La tarjeta que se acaba de enseñar, para el historial de la respuesta.
    shown_card: Option<Value>,
    /// El día que se mira en la vista del diario, si la pregunta viene de ahí.
    diary_day: String,
    /// La imagen de la tarjeta, que se busca en su primer enlace mientras el
    /// modelo acaba de escribir.
    picture: Option<(std::sync::mpsc::Receiver<String>, Instant)>,
    /// Las vistas guardadas.
    pub views: Views,
    /// El hilo que trae los calendarios de fuera: se le despierta para que lo haga ya.
    calendars: Option<Sender<calendars::Job>>,
}

/// Un reloj del sistema, en ms. CLOCK_BOOTTIME cuenta lo dormido; CLOCK_MONOTONIC, no.
fn clock(id: libc::clockid_t) -> i64 {
    let mut t = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: `t` es un timespec válido y suyo; clock_gettime solo escribe en él.
    if unsafe { libc::clock_gettime(id, &mut t) } != 0 {
        return 0;
    }
    t.tv_sec * 1000 + t.tv_nsec / 1_000_000
}

/// Las clases de las pantallas o micros: «micro», «cámara», «pantalla».
const DEVICES: [&str; 3] = ["micro", "cámara", "pantalla"];

/// La gramática de fábrica y, delante (para que gane a igualdad), la del usuario.
pub fn grammar() -> (Grammar, Option<String>) {
    let mut g = Grammar::default();
    let own = config::config().join("grammar.txt");
    let mut error = None;
    if let Ok(text) = fs::read_to_string(&own)
        && let Err(e) = g.load(&text, &own.display().to_string())
    {
        error = Some(e);
    }
    g.load(grammar::SPANISH, "es.txt").expect("la gramática de fábrica se lee");
    (g, error)
}

/// Lo que se apunta de una ventana con un título que no es de nadie más.
fn private(class: &str, title: &str) -> bool {
    let class = class.to_lowercase();
    let title = crate::words::fold(title);
    ["keepassxc", "1password", "bitwarden", "seahorse"].iter().any(|k| class.contains(k))
        || ["private browsing", "navegacion privada", "incognito", "modo privado", "inprivate"].iter().any(|k| title.contains(k))
}

impl Daemon {
    fn new(hypr: Hypr, journal: Option<Journal>, tx: Sender<Event>) -> Daemon {
        let (grammar, error) = grammar();
        if let Some(error) = error {
            eprintln!("veyld: {error}");
        }
        Daemon {
            split: hypr.split(),
            hypr,
            journal,
            grammar,
            apps: apps::load(),
            apps_read: Instant::now(),
            settings: config::settings(),
            shell: Shell::default(),
            agents: Vec::new(),
            clients: HashMap::new(),
            current: None,
            idle: false,
            pending: None,
            context: orders::Context::default(),
            tx,
            pruned: 0,
            processes: HashMap::new(),
            children: HashMap::new(),
            processes_read: None,
            notes: Notes::memory(),
            bus: None,
            pictures: config::cache().join("notifications"),
            web: Web::default(),
            playing: Vec::new(),
            recent: None,
            clock: None,
            ear: None,
            listening: false,
            mouth: None,
            talk: Vec::new(),
            talk_source: String::new(),
            pondering: 0,
            pondered: Value::Null,
            card: None,
            carded: false,
            shown_card: None,
            diary_day: String::new(),
            picture: None,
            views: Views::load(),
            calendars: None,
        }
    }

    fn start(&mut self) {
        if !self.settings.journal {
            return;
        }
        let now = now();
        let open: Vec<_> = self
            .hypr
            .clients()
            .into_iter()
            .map(|mut w| {
                if private(&w.class, &w.title) {
                    w.title.clear();
                }
                (w.address, w.class, w.title, w.workspace)
            })
            .collect();
        if let Some(journal) = &mut self.journal {
            journal.windows_now(now, &open);
            journal.prune(now);
        }
        self.pruned = now;
        self.current = self.hypr.active();
        self.presence(now, None);
        self.look(now);
        self.feed(now);
        self.read_recent(now);
        // Las páginas de lo de hoy que se apuntaron sin ella: las que siguen
        // en una pestaña (o en su historial) se saben aún.
        if let Some(journal) = &mut self.journal {
            let web = &mut self.web;
            journal.pages(now - 24 * 3600 * 1000, |app, title| web.url(app, title));
        }
    }

    /// Lo que está pasando ahora y no avisa cuando el diario se pone en marcha:
    /// los agentes, lo que suena, los micros.
    fn feed(&mut self, now: i64) {
        let devices = self.devices();
        let playing = self.audible();
        if let Some(journal) = &mut self.journal {
            journal.agents(now, &self.agents);
            journal.media(now, &playing);
            for (kind, on) in devices {
                journal.device(now, kind, on);
            }
        }
    }

    fn devices(&self) -> [(&'static str, bool); 3] {
        [(DEVICES[0], self.shell.mic && !self.listening), (DEVICES[1], self.shell.camera), (DEVICES[2], self.shell.screen)]
    }

    /// Lo que suena y se puede apuntar: si hay una ventana privada de un
    /// navegador, lo que suene en él puede ser de ella.
    fn audible(&self) -> Vec<Playing> {
        if self.playing.is_empty() {
            return Vec::new();
        }
        let hidden: Vec<&'static str> =
            self.hypr.clients().iter().filter(|w| private(&w.class, &w.title)).filter_map(|w| crate::web::browser(&w.class)).collect();
        self.playing
            .iter()
            .filter(|p| {
                let player = format!("{} {}", p.player, p.bus).to_lowercase();
                !hidden.iter().any(|b| player.contains(b))
            })
            .cloned()
            .collect()
    }

    fn media_changed(&mut self, playing: Vec<Playing>) {
        self.playing = playing;
        if !self.settings.journal {
            return;
        }
        let audible = self.audible();
        if let Some(journal) = &mut self.journal {
            journal.media(now(), &audible);
        }
    }

    /// Los archivos que han abierto las aplicaciones, si la lista cambió.
    fn read_recent(&mut self, now: i64) {
        let path = recent::path();
        let Ok(modified) = fs::metadata(&path).and_then(|m| m.modified()) else {
            return;
        };
        if self.recent == Some(modified) {
            return;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            return;
        };
        self.recent = Some(modified);
        if let Some(journal) = &mut self.journal {
            journal.files(&recent::read(&text), now - journal::KEEP);
        }
    }

    /// Si la máquina ha dormido desde lo último que pasó, lo apunta: lo
    /// abierto acaba al dormirse y vuelve a empezar al despertar.
    fn woke(&mut self) {
        let (boot, mono, wall) = (clock(libc::CLOCK_BOOTTIME), clock(libc::CLOCK_MONOTONIC), now());
        let before = self.clock.replace((boot, mono, wall));
        let Some((b, m, w)) = before else {
            return;
        };
        let slept = (boot - b) - (mono - m);
        if slept < 20_000 || !self.settings.journal {
            return;
        }
        let reason = self.absence();
        let Some(journal) = &mut self.journal else {
            return;
        };
        let (from, to) = (w, (w + slept).min(wall));
        journal.slept(from, to);
        if let Some(reason) = reason {
            journal.away(to, Some(reason));
        }
        self.feed(wall);
        self.look(wall);
    }

    /// Por qué no estás, si no estás.
    fn absence(&self) -> Option<&'static str> {
        if self.shell.locked {
            Some("bloqueado")
        } else if self.idle {
            Some("inactivo")
        } else {
            None
        }
    }

    /// Lo que se apunta de la ventana con el foco.
    fn seen(&mut self, window: &Window) -> Seen {
        let private = private(&window.class, &window.title);
        let place = if window.pid > 0 && !private {
            // Una terminal puede cambiar el título muchas veces por segundo.
            // Reutiliza el árbol, pero sigue leyendo la carpeta actual.
            if self.processes_read.is_none_or(|at| at.elapsed() >= Duration::from_secs(1)) {
                self.processes = procs::all();
                self.children = procs::children(&self.processes);
                self.processes_read = Some(Instant::now());
            }
            procs::place(window.pid, &self.processes, &self.children)
        } else {
            procs::Place::default()
        };
        Seen {
            window: window.address.clone(),
            app: window.class.clone(),
            title: if private { String::new() } else { window.title.clone() },
            folder: place.folder,
            command: place.command,
            workspace: window.workspace.clone(),
            pid: window.pid,
        }
    }

    /// Apunta lo que tiene el foco, si estás y se lleva el diario.
    fn look(&mut self, now: i64) {
        if !self.settings.journal || self.journal.is_none() || self.absence().is_some() {
            return;
        }
        let seen = self.current.clone().map(|w| self.seen(&w));
        if let Some(journal) = &mut self.journal {
            journal.focus(now, seen);
        }
    }

    /// Algo cambió en si estás (`before` es cómo estaba).
    fn presence(&mut self, now: i64, before: Option<&'static str>) {
        if !self.settings.journal {
            return;
        }
        let after = self.absence();
        let Some(journal) = &mut self.journal else {
            return;
        };
        match (before, after) {
            (None, Some(reason)) => {
                // La inactividad se nota cuando ya ha pasado un rato.
                let since = if reason == "inactivo" { now - idle::AWAY.as_millis() as i64 } else { now };
                journal.away(since, Some(reason));
            }
            (Some(_), None) => {
                journal.away(now, None);
                self.look(now);
            }
            _ => {}
        }
    }

    fn hypr_event(&mut self, name: &str, data: &str) {
        let now = now();
        let address = |hex: &str| format!("0x{}", hex.trim_start_matches("0x"));
        match name {
            "activewindowv2" => {
                self.current = if data.trim().is_empty() || data == "," { None } else { self.hypr.active() };
                self.look(now);
            }
            "windowtitlev2" => {
                if let Some((hex, title)) = data.split_once(',')
                    && let Some(current) = &mut self.current
                    && current.address == address(hex)
                {
                    current.title = title.to_string();
                    self.look(now);
                }
            }
            "openwindow" if self.settings.journal => {
                let parts: Vec<&str> = data.splitn(4, ',').collect();
                if let ([hex, workspace, class, title], Some(journal)) = (parts.as_slice(), &mut self.journal) {
                    journal.opened(now, &address(hex), class, if private(class, title) { "" } else { title }, workspace);
                }
            }
            "closewindow" if self.settings.journal => {
                if let Some(journal) = &mut self.journal {
                    journal.closed(now, &address(data));
                }
            }
            "movewindowv2" => {
                let parts: Vec<&str> = data.splitn(3, ',').collect();
                if let ([hex, _, workspace], Some(current)) = (parts.as_slice(), &mut self.current)
                    && current.address == address(hex)
                {
                    current.workspace = workspace.to_string();
                    self.look(now);
                }
            }
            "configreloaded" => self.split = self.hypr.split(),
            // Una vista abre o cierra con su espacio.
            "createworkspacev2" | "destroyworkspacev2" | "renameworkspace" => self.views_changed(),
            _ => {}
        }
    }

    /// Borra las notificaciones más viejas que los días que se guardan.
    fn clean_notes(&mut self) {
        let days = self.settings.notification_days;
        if days > 0 {
            let out = self.notes.prune(now() - days as i64 * 24 * 3600 * 1000);
            self.notes_out(out);
        }
    }

    fn tick(&mut self) {
        let now = now();
        let recording = self.settings.journal;
        self.settings = config::settings();
        self.clean_notes();
        if recording && !self.settings.journal {
            if let Some(journal) = &mut self.journal {
                journal.pause(now);
            }
        } else if !recording && self.settings.journal {
            self.start();
            if let Some(journal) = &mut self.journal {
                journal.agents(now, &self.agents);
            }
        }
        // Lo que no avisa: el título o la carpeta de una terminal en la que se cambia de directorio.
        self.current = self.hypr.active();
        self.look(now);
        if self.settings.journal
            && let Some(journal) = &mut self.journal
        {
            journal.beat(now);
            // Las páginas de lo mirado en el navegador, que las guarda tarde.
            let web = &mut self.web;
            journal.pages(now - 10 * 60 * 1000, |app, title| web.url(app, title));
            if now - self.pruned > 24 * 3600 * 1000 {
                journal.prune(now);
                self.pruned = now;
            }
        }
        if self.settings.journal {
            self.read_recent(now);
        }
        if self.apps_read.elapsed() > Duration::from_secs(300) {
            self.reload_apps();
        }
        if self.pending.as_ref().is_some_and(|(_, at, _)| at.elapsed() > PATIENCE) {
            self.pending = None;
        }
    }

    pub fn reload_apps(&mut self) {
        self.apps = apps::load();
        self.apps_read = Instant::now();
    }

    fn send(&mut self, client: u64, message: &Value) {
        let line = format!("{message}\n");
        let failed = match self.clients.get_mut(&client) {
            Some(c) => c.stream.write_all(line.as_bytes()).is_err(),
            None => false,
        };
        if failed && let Some(client) = self.clients.remove(&client) {
            // Despierta también al hilo lector que conserva un clon del socket.
            let _ = client.stream.shutdown(std::net::Shutdown::Both);
        }
    }

    fn broadcast(&mut self, message: &Value, to: impl Fn(&Client) -> bool) -> bool {
        let targets: Vec<u64> = self.clients.iter().filter(|(_, c)| to(c)).map(|(&id, _)| id).collect();
        let mut reached = false;
        for id in targets {
            self.send(id, message);
            reached |= self.clients.contains_key(&id);
        }
        reached
    }

    /// Le pide algo a Veyl; `false` si Veyl no está.
    pub fn call(&mut self, name: &str, args: Value) -> bool {
        self.broadcast(&json!({ "call": name, "args": args }), |c| c.shell)
    }

    fn agents_changed(&mut self, agents: Vec<Agent>) {
        if self.settings.journal
            && let Some(journal) = &mut self.journal
        {
            journal.agents(now(), &agents);
        }
        self.agents = agents;
        let message = json!({ "agents": self.agents });
        self.broadcast(&message, |c| c.agents);
    }

    fn shell_state(&mut self, state: &Value) {
        let before = self.absence();
        if let Some(locked) = state.get("locked").and_then(Value::as_bool) {
            self.shell.locked = locked;
        }
        if let Some(timer) = state.get("timer").and_then(Value::as_f64) {
            self.shell.timer = timer as i64;
        }
        if let Some(quiet) = state.get("quiet").and_then(Value::as_bool) {
            self.shell.quiet = quiet;
        }
        if let Some(scheme) = state.get("scheme").and_then(Value::as_str) {
            self.shell.scheme = scheme.to_string();
        }
        for (key, flag) in [("mic", &mut self.shell.mic), ("camera", &mut self.shell.camera), ("screen", &mut self.shell.screen)] {
            if let Some(on) = state.get(key).and_then(Value::as_bool) {
                *flag = on;
            }
        }
        if self.settings.journal {
            let devices = self.devices();
            if let Some(journal) = &mut self.journal {
                for (kind, on) in devices {
                    journal.device(now(), kind, on);
                }
            }
        }
        self.presence(now(), before);
    }

    fn message(&mut self, client: u64, message: Value) {
        let id = message.get("id").cloned().unwrap_or(Value::Null);
        let text = |key: &str| message.get(key).and_then(Value::as_str).map(str::to_string);
        if text("hello").as_deref() == Some("shell") {
            if let Some(c) = self.clients.get_mut(&client) {
                (c.shell, c.agents) = (true, true);
            }
            if let Some(root) = text("root") {
                self.shell.root = root;
            }
            let agents = json!({ "agents": self.agents });
            self.send(client, &agents);
            let notes = json!({ "notes": self.notes.all() });
            self.send(client, &notes);
            let thinking = json!({ "thinking": self.pondered });
            self.send(client, &thinking);
            let views = json!({ "views": self.views_listing() });
            self.send(client, &views);
        }
        if text("watch").as_deref() == Some("agents") {
            if let Some(c) = self.clients.get_mut(&client) {
                c.agents = true;
            }
            let agents = json!({ "agents": self.agents });
            self.send(client, &agents);
        }
        if let Some(state) = message.get("state") {
            self.shell_state(state);
        }
        let flag = |key: &str| message.get(key).and_then(Value::as_bool).unwrap_or(false);
        match message.get("listen").and_then(Value::as_bool) {
            Some(true) => self.listen(flag("demo"), flag("hold")),
            Some(false) => self.hush(),
            None => {}
        }
        if flag("finish")
            && let Some(ear) = &self.ear
        {
            let _ = ear.send(voice::Order::Finish);
        }
        if text("calendars").as_deref() == Some("sync")
            && let Some(jobs) = &self.calendars
        {
            let _ = jobs.send(calendars::Job::Sync);
        }
        if let Some(asked) = message.get("calendar")
            && let Some(jobs) = &self.calendars
        {
            let field = |v: &Value, key: &str| v.get(key).and_then(Value::as_str).unwrap_or("").to_string();
            if let Some(a) = asked.get("add") {
                let _ =
                    jobs.send(calendars::Job::Add { calendar: field(a, "calendar"), title: field(a, "title"), day: field(a, "day"), time: field(a, "time") });
            }
            if let Some(r) = asked.get("remove") {
                let _ = jobs.send(calendars::Job::Remove { calendar: field(r, "calendar"), id: field(r, "id") });
            }
        }
        if let Some(op) = text("google") {
            let tx = self.tx.clone();
            let (id, secret) = (text("client_id").unwrap_or_default(), text("client_secret").unwrap_or_default());
            // Lo de la cuenta va por la red (y conectar espera al navegador): en otro hilo.
            thread::spawn(move || {
                let mut told = match op.as_str() {
                    "login" => match google::login(&id, &secret) {
                        Ok(_) => json!({ "login": "ok" }),
                        Err(why) => json!({ "login": why }),
                    },
                    "logout" => {
                        google::logout();
                        json!({})
                    }
                    _ => json!({}),
                };
                let status = google::status();
                if let (Some(told), Some(status)) = (told.as_object_mut(), status.as_object()) {
                    told.extend(status.clone());
                }
                let _ = tx.send(Event::Calendar(json!({ "google": told })));
            });
        }
        if let Some(said) = text("say") {
            let source = text("source").unwrap_or_else(|| "cli".into());
            let confirmed = message.get("confirmed").and_then(Value::as_bool).unwrap_or(false);
            // Lo que se le pregunta a Veyl a propósito no pasa por la gramática.
            // Desde la vista del diario, el día que se está mirando.
            self.diary_day = text("day").unwrap_or_default();
            if flag("ask") && self.thinks() {
                self.pending = None;
                self.ask_model(client, id.clone(), &said, &source);
            } else {
                self.say(client, id.clone(), &said, &source, confirmed);
            }
        }
        // Algo con una vista, desde la entrada: por su nombre tal cual.
        if let Some(op) = text("view") {
            let name = text("name").unwrap_or_default();
            let (intent, extra) = match op.as_str() {
                "create" if flag("level") => ("view.create", Some(("from", json!("level")))),
                "create" => ("view.create", None),
                "again" => ("view.open", Some(("again", json!(true)))),
                "open" => ("view.open", None),
                "undo" => ("view.undo", None),
                "save" => ("view.save", None),
                "forget" => ("view.forget", None),
                _ => ("view.list", None),
            };
            let mut args = serde_json::Map::new();
            args.insert("name".into(), json!(name));
            if let Some((key, value)) = extra {
                args.insert(key.into(), value);
            }
            let parse = Parse { intent: intent.into(), args, heard: format!("{op} {name}") };
            let outcome = self.perform(std::slice::from_ref(&parse), "launcher", true);
            self.answer(client, id.clone(), &format!("vista {op}: {name}"), "launcher", Some(&parse), "views", outcome);
        }
        if let Some(said) = text("parse") {
            let preview = orders::plan(self, &said).map(|plan| {
                let shown = orders::describe_plan(self, &plan);
                let (intent, args, heard) = match plan.as_slice() {
                    [one] => (json!(one.intent), Value::Object(one.args.clone()), one.heard.clone()),
                    many => (json!("plan"), json!(many), many.iter().map(|p| p.heard.as_str()).collect::<Vec<_>>().join(" · ")),
                };
                json!({ "intent": intent, "args": args, "heard": heard, "title": shown.title, "glyph": shown.glyph, "confirm": shown.confirm, "ready": shown.ready })
            });
            self.send(client, &json!({ "id": id, "said": said, "parse": preview }));
        }
        // Lo que queda de cuota en las suscripciones de los agentes.
        if message.get("usage").and_then(Value::as_bool) == Some(true) {
            self.send(client, &json!({ "id": id, "usage": crate::usage::all() }));
        }
        // El historial de lo que le has pedido a Veyl, para el centro de control.
        if let Some(limit) = message.get("asked").and_then(Value::as_u64) {
            let list = match &self.journal {
                Some(journal) if self.settings.journal => journal.asked(limit.min(500) as usize, None),
                _ => Vec::new(),
            };
            let list: Vec<Value> = list.into_iter().filter(|row| row["source"] != "cli").collect();
            self.send(client, &json!({ "id": id, "asked": list, "journal": self.settings.journal }));
        }
        if let Some(row) = message.get("forget_asked").and_then(Value::as_i64)
            && let Some(journal) = &mut self.journal
            && journal.forget_order(row)
        {
            self.broadcast(&json!({ "asked_gone": row }), |c| c.shell);
        }
        if let Some(when) = text("recall") {
            let flag = |key: &str| message.get(key).and_then(Value::as_bool).unwrap_or(false);
            if flag("json") {
                let reply = match orders::recall_json(self, &when) {
                    Ok(report) => json!({ "id": id, "ok": true, "report": report }),
                    Err(outcome) => json!({ "id": id, "ok": false, "say": outcome.say }),
                };
                self.send(client, &reply);
            } else {
                let outcome = if flag("full") { orders::report(self, &when) } else { orders::recall(self, &when) };
                self.send(client, &json!({ "id": id, "ok": outcome.ok, "say": outcome.say, "answer": outcome.answer }));
            }
        }
        // Una herramienta, para el servidor MCP (Claude Code, Codex).
        if let Some(name) = text("tool") {
            let source = if self.talk_source.is_empty() { "cli".to_string() } else { self.talk_source.clone() };
            let result = self.tool(&name, message.get("input").unwrap_or(&Value::Null), &source);
            self.send(client, &json!({ "id": id, "result": result }));
        }
        if let Some(when) = text("timeline") {
            let outcome = orders::timeline(self, &when);
            self.send(client, &json!({ "id": id, "ok": outcome.ok, "say": outcome.say, "answer": outcome.answer }));
        }
        if let Some(what) = text("find") {
            if flag("json") {
                let reply = match orders::find_json(self, &what) {
                    Ok(hits) => json!({ "id": id, "ok": true, "find": what, "hits": hits }),
                    Err(outcome) => json!({ "id": id, "ok": false, "find": what, "say": outcome.say }),
                };
                self.send(client, &reply);
            } else {
                let outcome = orders::find(self, &what);
                self.send(client, &json!({ "id": id, "ok": outcome.ok, "say": outcome.say, "answer": outcome.answer }));
            }
        }
        if let Some(day) = message.get("diary").and_then(Value::as_str) {
            let days = message.get("days").and_then(Value::as_i64).unwrap_or(14);
            let reply = match orders::diary_json(self, day, days) {
                Ok(diary) => json!({ "id": id, "ok": true, "diary": diary }),
                Err(outcome) => json!({ "id": id, "ok": false, "diary": null, "say": outcome.say }),
            };
            self.send(client, &reply);
        }
        if message.get("agents").and_then(Value::as_bool) == Some(true) {
            let agents = json!({ "id": id, "agents": self.agents });
            self.send(client, &agents);
        }
        if let Some(note) = message.get("note").and_then(Value::as_u64) {
            self.note_message(note as u32, &message);
        }
        if text("notes").as_deref() == Some("clear") {
            let out = self.notes.clear();
            self.notes_out(out);
        }
        if let Some(on) = message.get("hold").and_then(Value::as_bool) {
            self.notes.hold(on, Instant::now());
        }
    }

    /// Lo que haces con una notificación desde Veyl.
    fn note_message(&mut self, id: u32, message: &Value) {
        if let Some(key) = message.get("act").and_then(Value::as_str) {
            let (heard, out) = self.notes.act(id);
            if heard && let Some(bus) = &self.bus {
                bus.invoked(id, key);
            }
            self.notes_out(out);
        } else if let Some(text) = message.get("reply").and_then(Value::as_str) {
            let (heard, out) = self.notes.act(id);
            if heard && let Some(bus) = &self.bus {
                bus.replied(id, text);
            }
            self.notes_out(out);
        } else if message.get("dismiss").and_then(Value::as_bool) == Some(true) {
            let out = self.notes.dismiss(id);
            self.notes_out(out);
        }
    }

    /// Cuenta lo que ha pasado: a Veyl, cómo quedan; a cada programa, las
    /// suyas que se han cerrado.
    fn notes_out(&mut self, out: Vec<Out>) {
        for change in out {
            match change {
                Out::Show(note) => {
                    self.broadcast(&json!({ "note": note }), |c| c.shell);
                }
                Out::Gone(id) => {
                    bus::forget_pictures(&self.pictures, id);
                    self.broadcast(&json!({ "note_gone": id }), |c| c.shell);
                }
                Out::Closed(id, reason) => {
                    if let Some(bus) = &self.bus {
                        bus.closed(id, reason as u32);
                    }
                }
            }
        }
    }

    /// Una orden: la gramática, y si no la entiende, el modelo (si hay).
    fn say(&mut self, client: u64, id: Value, said: &str, source: &str, confirmed: bool) {
        let plan = orders::plan(self, said);
        let first = plan.as_ref().and_then(|p| p.first()).map(|p| p.intent.clone());
        if first.as_deref().is_none_or(|i| i != "confirm" && i != "cancel") {
            self.pending = None;
        }
        let Some(plan) = plan else {
            if self.thinks() {
                self.ask_model(client, id, said, source);
                return;
            }
            let outcome = Outcome::fail("No sé hacer eso todavía");
            self.answer(client, id, said, source, None, "none", outcome);
            return;
        };
        // Lo que hay que medir tarda: en un hilo, con «Pensando» mientras.
        if let [one] = plan.as_slice()
            && one.intent.starts_with("system.")
        {
            self.sense(client, id, said, source, one.clone());
            return;
        }
        let outcome = match first.as_deref() {
            Some("confirm") => match self.pending.take() {
                Some((pending, at, owner)) if owner == source && at.elapsed() < PATIENCE => self.perform(&pending, source, true),
                _ => Outcome::fail("No hay nada que confirmar"),
            },
            Some("cancel") => match self.pending.take() {
                Some(_) => Outcome::done("Cancelado"),
                None => Outcome::fail("No hay nada que cancelar"),
            },
            _ => {
                // Casa con la gramática pero no hay a qué aplicarlo («abre la puerta del
                // garaje»): mejor que lo intente el modelo, si lo hay.
                let ready = orders::describe_plan(self, &plan).ready;
                if !ready && self.thinks() {
                    self.ask_model(client, id, said, source);
                    return;
                }
                // Sin modelo, una canción que no se ha pedido como tal no se pone.
                if !ready && plan.iter().any(|p| p.intent == "play.search") {
                    Outcome::fail("No sé qué poner: di «pon … en Spotify» o «pon la canción …»")
                } else {
                    self.perform(&plan, source, confirmed)
                }
            }
        };
        let logged = match plan.as_slice() {
            [one] => one.clone(),
            many => Parse {
                intent: many.iter().map(|p| p.intent.as_str()).collect::<Vec<_>>().join("+"),
                args: [("plan".to_string(), json!(many))].into_iter().collect(),
                heard: many.iter().map(|p| p.heard.as_str()).collect::<Vec<_>>().join(" · "),
            },
        };
        // Lo que contesta con varias líneas (la agenda, el diario, los agentes)
        // se lee mejor en una tarjeta que en una notificación.
        if outcome.ok
            && source != "cli"
            && let Some(body) = outcome.answer.as_deref().filter(|b| !b.trim().is_empty())
            && let Some(card) = crate::card::from_answer(&outcome.say, orders::describe_plan(self, &plan).glyph, body)
        {
            self.present(card, &outcome, said);
        }
        self.answer(client, id, said, source, Some(&logged), "grammar", outcome);
        self.carded = false;
    }

    /// Mide lo que se pregunta del ordenador en un hilo aparte.
    fn sense(&mut self, client: u64, id: Value, said: &str, source: &str, parse: Parse) {
        self.ponder(said);
        let windows = crate::system::seen(self);
        let (tx, said, source) = (self.tx.clone(), said.to_string(), source.to_string());
        thread::spawn(move || {
            let (outcome, card) = crate::system::run(&parse.intent, &windows);
            let _ = tx.send(Event::Sensed { client, id, said, source, parse, outcome, card });
        });
    }

    /// Enseña una tarjeta en la isla, en vez de la notificación, si hay un Veyl que la pinte.
    fn present(&mut self, mut card: Value, outcome: &Outcome, said: &str) {
        if !outcome.ok || !self.clients.values().any(|c| c.shell) {
            return;
        }
        card["say"] = json!(outcome.say);
        card["said"] = json!(said);
        self.broadcast(&json!({ "card": card }), |c| c.shell);
        self.shown_card = Some(card);
        self.carded = true;
    }

    /// Hace las órdenes una tras otra. Si una es peligrosa y no está
    /// confirmada, se para ahí y espera un «sí» para ella y las siguientes;
    /// si una falla, las siguientes no se hacen.
    fn perform(&mut self, plan: &[Parse], source: &str, confirmed: bool) -> Outcome {
        let mut said: Vec<String> = Vec::new();
        let mut bodies: Vec<String> = Vec::new();
        for (i, parse) in plan.iter().enumerate() {
            let shown = orders::describe(self, parse);
            if shown.confirm && !confirmed {
                self.pending = Some((plan[i..].to_vec(), Instant::now(), source.to_string()));
                said.push(format!("¿{}? Di «sí» para confirmar", shown.title));
                break;
            }
            let outcome = orders::run(self, parse, source);
            said.push(outcome.say);
            bodies.extend(outcome.answer);
            if !outcome.ok {
                let say = said.join(". ");
                return Outcome { ok: false, say, answer: (!bodies.is_empty()).then(|| bodies.join("\n")) };
            }
        }
        Outcome { ok: true, say: said.join(". "), answer: (!bodies.is_empty()).then(|| bodies.join("\n")) }
    }

    #[allow(clippy::too_many_arguments)]
    fn answer(&mut self, client: u64, id: Value, said: &str, source: &str, parse: Option<&Parse>, handler: &str, outcome: Outcome) {
        let shown = self.shown_card.take();
        let card = shown.as_ref().map(|c| c.to_string()).unwrap_or_default();
        let mut logged = None;
        if self.settings.journal
            && let Some(journal) = &mut self.journal
        {
            let args = parse.map(|p| Value::Object(p.args.clone()).to_string()).unwrap_or_default();
            let answer = match &outcome.answer {
                Some(body) => format!("{}\n{body}", outcome.say),
                None => outcome.say.clone(),
            };
            logged = journal
                .order(now(), source, said, parse.map(|p| p.intent.as_str()), &args, handler, outcome.ok, &answer, &card)
                .and_then(|row| journal.asked(1, Some(row)).pop());
        }
        // Al historial del centro de control; lo que se pide desde la línea de órdenes no es una consulta tuya.
        if let Some(one) = logged
            && source != "cli"
        {
            self.broadcast(&json!({ "asked_one": one }), |c| c.shell);
        }
        let reply = json!({
            "id": id,
            "ok": outcome.ok,
            "say": outcome.say,
            "answer": outcome.answer,
            "intent": parse.map(|p| p.intent.clone()),
            "handler": handler,
            "answer_card": shown,
        });
        self.send(client, &reply);
        // Desde la línea de órdenes la respuesta se lee ahí; por voz, todo se
        // dice; desde la entrada, solo lo que contesta algo o lo que falla.
        let show = match source {
            "cli" | "diary" => false,
            "voice" => true,
            _ => outcome.answer.is_some() || !outcome.ok,
        };
        if show && !self.carded {
            notify(&outcome);
        }
        // A lo dicho por voz, Veyl contesta en voz alta: lo corto; lo largo, en la notificación.
        if source == "voice"
            && self.settings.speak
            && !self.shell.quiet
            && let Some(mouth) = &self.mouth
        {
            let _ = mouth.send(tts::Say::Text { text: tts::spoken(&outcome.say), style: self.settings.style() });
        }
    }

    /// Lo que la gramática no entiende, al modelo: lo dicho por la entrada
    /// estándar de `llm`, y lo que escriba es la respuesta.
    /// Si hay alguien a quien preguntar lo que la gramática no entiende.
    pub fn thinks(&self) -> bool {
        !self.settings.brain.is_empty() || !self.settings.llm.is_empty()
    }

    /// Que Veyl enseñe que está pensando en algo.
    fn ponder(&mut self, said: &str) {
        self.pondering += 1;
        self.pondered = json!({ "said": said, "since": now() });
        let thinking = json!({ "thinking": self.pondered });
        self.broadcast(&thinking, |c| c.shell);
    }

    /// Ha acabado de pensar una; si no queda ninguna, Veyl deja de enseñarlo.
    fn pondered_one(&mut self) {
        self.pondering = self.pondering.saturating_sub(1);
        if self.pondering == 0 {
            self.pondered = Value::Null;
            self.broadcast(&json!({ "thinking": null }), |c| c.shell);
        }
    }

    /// Lo que acaba más tarde que la orden que lo pidió: siempre se avisa
    /// y, si se dijo por voz, se dice.
    fn later(&mut self, source: &str, outcome: &Outcome) {
        notify(outcome);
        if source == "voice"
            && self.settings.speak
            && !self.shell.quiet
            && let Some(mouth) = &self.mouth
        {
            let _ = mouth.send(tts::Say::Text { text: tts::spoken(&outcome.say), style: self.settings.style() });
        }
    }

    /// Las ventanas abiertas, con su carpeta (en una terminal), lo que tiene
    /// en marcha y su página (en un navegador): con lo que se elige una vista.
    /// Las privadas no salen.
    pub fn candidates(&mut self) -> Vec<Candidate> {
        self.processes = procs::all();
        self.children = procs::children(&self.processes);
        self.processes_read = Some(Instant::now());
        let level = |workspace: i64| if self.split > 0 && workspace > 0 { (workspace - 1) % self.split as i64 + 1 } else { workspace };
        let mut out = Vec::new();
        for window in self.hypr.clients() {
            if private(&window.class, &window.title) || window.workspace.starts_with("special:") {
                continue;
            }
            let place = if window.pid > 0 { procs::place(window.pid, &self.processes, &self.children) } else { procs::Place::default() };
            let url = self.web.url(&window.class, &window.title).unwrap_or_default();
            out.push(Candidate {
                app: apps::label(&self.apps, &window.class),
                folder: place.folder,
                command: place.command,
                url,
                level: level(window.workspace_id),
                window,
            });
        }
        out
    }

    /// Le pide al cerebro, aparte, qué ventanas van en la vista `name`; la
    /// vista se hace cuando contesta (`Event::ViewChosen`).
    pub fn think_view(&mut self, name: &str, candidates: Vec<Candidate>, here: i64, source: &str) {
        self.ponder(&format!("Qué va en la vista «{name}»"));
        let (tx, settings, name, source) = (self.tx.clone(), self.settings.clone(), name.to_string(), source.to_string());
        thread::spawn(move || {
            let asked = views::prompt(&name, &candidates);
            let choice = crate::brain::judge(&settings, views::SYSTEM, &asked)
                .and_then(|answer| views::read(&answer, &candidates).ok_or_else(|| format!("respuesta sin JSON: {answer}")));
            let _ = tx.send(Event::ViewChosen { name, candidates, here, source, choice });
        });
    }

    /// Las vistas, como las ve Veyl: con las que están abiertas ahora.
    pub fn views_listing(&self) -> Value {
        let open: HashMap<String, i64> = self
            .hypr
            .json("workspaces")
            .and_then(|w| w.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|w| Some((w.get("name")?.as_str()?.to_string(), w.get("windows")?.as_i64()?)))
            .collect();
        views::listing(&self.views, &open, &self.apps)
    }

    /// Cuenta a Veyl cómo quedan las vistas.
    pub fn views_changed(&mut self) {
        let views = json!({ "views": self.views_listing() });
        self.broadcast(&views, |c| c.shell);
    }

    fn ask_model(&mut self, client: u64, id: Value, said: &str, source: &str) {
        // Que Veyl enseñe que está pensando.
        self.pondering += 1;
        self.card = None;
        self.picture = None;
        self.pondered = json!({ "said": said, "since": now() });
        let thinking = json!({ "thinking": self.pondered });
        self.broadcast(&thinking, |c| c.shell);
        let (tx, said, source) = (self.tx.clone(), said.to_string(), source.to_string());
        if self.settings.brain.is_empty() {
            let command = self.settings.llm.clone();
            thread::spawn(move || {
                let outcome = model(&command, &said, &source);
                let _ = tx.send(Event::Answered { client, id, said, source, outcome });
            });
            return;
        }
        // Lo hablado en los últimos diez minutos, para poder seguir la conversación.
        self.talk.retain(|(at, _, _)| at.elapsed() < Duration::from_secs(600));
        let history: Vec<(String, String)> = self.talk.iter().rev().take(6).rev().map(|(_, said, answer)| (said.clone(), answer.clone())).collect();
        self.talk_source = source.clone();
        let (settings, socket) = (self.settings.clone(), config::socket());
        // Lo que hay delante va ya con la pregunta: casi nunca hace falta pedirlo (una vuelta menos).
        let mut context = self.context();
        if source == "diary" {
            context["pregunta_desde"] = json!("la vista del diario: casi seguro que es sobre lo que hizo; usa la herramienta `diario`");
            if !self.diary_day.is_empty() {
                context["dia_que_mira_en_el_diario"] = json!(format!("{} (así vale en `cuando`)", self.diary_day));
            }
        }
        let context = context.to_string();
        let windows = crate::system::seen(self);
        thread::spawn(move || {
            let ask = brain::Ask { said: said.clone(), history, context };
            let (tools, from) = (tx.clone(), source.clone());
            let tool = move |name: &str, input: &Value| -> String {
                // Medir tarda: se mide en este hilo, sin parar al daemon.
                if name == "sistema" {
                    return crate::system::told(input, &windows);
                }
                let (reply, wait) = mpsc::channel();
                let asked = Event::Tool { name: name.to_string(), input: input.clone(), source: from.clone(), reply };
                if tools.send(asked).is_err() {
                    return "veyld se ha ido".into();
                }
                wait.recv_timeout(Duration::from_secs(60)).unwrap_or_else(|_| "Veyl no contesta".into())
            };
            let outcome = match brain::ask(&settings, &ask, &tool, &socket) {
                Ok(text) if !brain::plain(&text).is_empty() => {
                    let text = brain::plain(&text);
                    let (say, rest) = text.split_once('\n').map(|(a, b)| (a.trim().to_string(), Some(b.trim().to_string()))).unwrap_or((text.clone(), None));
                    Outcome { ok: true, say, answer: rest.filter(|r| !r.is_empty()) }
                }
                // Sin frase: quizá lo dijo todo en la tarjeta (se mira al llegar).
                Ok(_) => Outcome { ok: true, say: String::new(), answer: None },
                Err(why) => Outcome::fail(why),
            };
            let _ = tx.send(Event::Answered { client, id, said, source, outcome });
        });
    }

    /// Lo que pide el cerebro: una orden, el contexto o el diario. Lo que
    /// contesta es texto para el modelo.
    pub fn tool(&mut self, name: &str, input: &Value, source: &str) -> String {
        let text = |key: &str| input.get(key).and_then(Value::as_str).unwrap_or("").trim().to_string();
        let told = |o: Outcome| match o.answer {
            Some(body) => format!("{}\n{body}", o.say),
            None => o.say,
        };
        match name {
            "veyl" => {
                let order = text("orden");
                match orders::plan(self, &order) {
                    Some(plan) => told(self.perform(&plan, source, false)),
                    None => format!("Veyl no entiende «{order}» como una orden suya."),
                }
            }
            "contexto" => self.context().to_string(),
            "mostrar" => match crate::card::clean(input) {
                Some(card) => {
                    self.picture = None;
                    // Se prueba en los tres primeros enlaces a la vez: vale el primero que llegue.
                    let pages = crate::card::links(&card);
                    if card["imagen"] == "" && !pages.is_empty() {
                        let (tx, rx) = std::sync::mpsc::channel();
                        for page in pages.into_iter().take(3) {
                            let tx = tx.clone();
                            thread::spawn(move || {
                                if let Some(url) = crate::card::picture(&page) {
                                    tx.send(url).ok();
                                }
                            });
                        }
                        self.picture = Some((rx, Instant::now()));
                    }
                    self.card = Some(card);
                    "Tarjeta lista. Ahora contesta con tu frase corta.".into()
                }
                // La de antes, si la hay, se queda: no se tapa con una vacía.
                None if self.card.is_some() => "Esa tarjeta no tiene ninguna pieza que Veyl sepa pintar: se queda la que ya enseñaste. No la vuelvas a mandar; contesta con tu frase corta.".into(),
                None => "Esa tarjeta no tiene ninguna pieza que Veyl sepa pintar: no se enseña. Mándala otra vez con sus `bloques`.".into(),
            },
            "diario" => {
                let when = text("cuando");
                told(match text("que").as_str() {
                    "detalle" => orders::report(self, &when),
                    "rato" => orders::timeline(self, &when),
                    "indice" => orders::index(self, &when),
                    "buscar" => orders::find(self, &text("texto")),
                    _ => orders::recall(self, &when),
                })
            }
            other => format!("No hay ninguna herramienta «{other}»"),
        }
    }

    /// Lo que hay ahora en el escritorio, para el cerebro.
    fn context(&self) -> Value {
        let level = |workspace: i64| if self.split > 0 && workspace > 0 { (workspace - 1) % self.split as i64 + 1 } else { workspace };
        let active = self.hypr.active();
        let windows: Vec<Value> = self
            .hypr
            .clients()
            .iter()
            .map(|w| json!({ "app": apps::label(&self.apps, &w.class), "titulo": w.title, "nivel": level(w.workspace_id), "foco": active.as_ref().is_some_and(|a| a.address == w.address) }))
            .collect();
        let agents: Vec<Value> = self.agents.iter().map(|a| json!({ "agente": a.label, "sesion": a.name, "carpeta": a.cwd, "estado": a.phase })).collect();
        let playing: Vec<Value> = self.playing.iter().map(|p| json!({ "reproductor": p.player, "titulo": p.title, "artista": p.artist })).collect();
        let today = chrono::Local::now().date_naive();
        let days = [today.format("%Y-%m-%d").to_string(), (today + chrono::Duration::days(1)).format("%Y-%m-%d").to_string()];
        let agenda: Vec<Value> =
            calendars::agenda().into_iter().filter(|e| e.get("day").and_then(Value::as_str).is_some_and(|d| days.iter().any(|x| x == d))).collect();
        let left = (self.shell.timer - now()) / 1000;
        json!({
            "ahora": chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
            "ventana_con_foco": active.map(|w| json!({ "app": apps::label(&self.apps, &w.class), "titulo": w.title, "nivel": level(w.workspace_id) })),
            "ventanas": windows,
            "agentes": agents,
            "suena": playing,
            "temporizador_segundos": if left > 0 { json!(left) } else { Value::Null },
            "no_molestar": self.shell.quiet,
            "agenda_hoy_y_manana": agenda,
        })
    }

    /// Escucha una frase.
    fn listen(&mut self, demo: bool, hold: bool) {
        // Si estaba hablando, se calla: le estás hablando tú.
        if let Some(mouth) = &self.mouth {
            let _ = mouth.send(tts::Say::Hush);
            // Y si va a contestar en voz alta, que la voz se vaya cargando.
            if self.settings.speak {
                let _ = mouth.send(tts::Say::Warm(self.settings.style()));
            }
        }
        let prompt = self.prompt();
        let (model, mic) = (self.settings.voice_model.clone(), self.settings.voice_mic.clone());
        if let Some(ear) = &self.ear {
            self.listening = ear.send(voice::Order::Listen { demo, hold, prompt, model, mic }).is_ok();
        }
    }

    fn hush(&mut self) {
        self.listening = false;
        if let Some(ear) = &self.ear {
            let _ = ear.send(voice::Order::Stop);
        }
    }

    /// Lo que se le dice a Whisper que puede oír: cómo suenan las órdenes y
    /// cómo se escriben las aplicaciones que hay abiertas, para que acierte
    /// con «kitty» o «Hyprland».
    fn prompt(&self) -> String {
        let mut names: Vec<String> = Vec::new();
        for w in self.hypr.clients() {
            let name = apps::label(&self.apps, &w.class);
            if !name.is_empty() && !names.contains(&name) {
                names.push(name);
            }
        }
        for agent in &self.agents {
            if !names.contains(&agent.label) {
                names.push(agent.label.clone());
            }
        }
        names.truncate(20);
        // Y las palabras del vocabulario, empezando por «Veyl».
        for word in crate::vocab::Vocabulary::load().hints() {
            if names.len() < 30 && !names.contains(&word) {
                names.push(word);
            }
        }
        let mut prompt = voice::PROMPT.to_string();
        if !names.is_empty() {
            prompt += &format!(" {}.", names.join(", "));
        }
        prompt
    }

    fn voice(&mut self, voice: Voice) {
        match voice {
            Voice::Level(level) => {
                let message = json!({ "voice": { "level": (level * 1000.0).round() / 1000.0 } });
                self.broadcast(&message, |c| c.shell);
            }
            Voice::Speaking => {
                self.broadcast(&json!({ "voice": { "speaking": true } }), |c| c.shell);
            }
            Voice::Heard(text) => {
                // Lo que Whisper oye mal («qué puede hacer bale»), con su palabra.
                let text = crate::vocab::Vocabulary::load().fix(&text);
                self.listening = false;
                self.broadcast(&json!({ "voice": { "heard": text } }), |c| c.shell);
                self.call("listen.set", json!([false]));
                self.say(0, Value::Null, &text, "voice", false);
            }
            Voice::Done(why) => {
                self.listening = false;
                self.call("listen.set", json!([false]));
                if !why.is_empty() {
                    notify(&Outcome::fail(why));
                }
            }
        }
    }

    fn handle(&mut self, event: Event) -> bool {
        self.woke();
        match event {
            Event::Hypr(name, data) => self.hypr_event(&name, &data),
            Event::HyprLost => return false,
            Event::Agents(agents) => self.agents_changed(agents),
            Event::Media(playing) => self.media_changed(playing),
            Event::Voice(voice) => self.voice(voice),
            Event::Idle(away) => {
                let before = self.absence();
                self.idle = away;
                self.presence(now(), before);
            }
            Event::Tick => self.tick(),
            Event::Join(id, stream) => {
                self.clients.insert(id, Client { stream, shell: false, agents: false });
            }
            Event::Said(id, message) => self.message(id, message),
            Event::Left(id) => {
                let was_shell = self.clients.remove(&id).is_some_and(|c| c.shell);
                // Sin Veyl no hay puntero encima de nada.
                if was_shell && !self.clients.values().any(|c| c.shell) {
                    self.notes.hold(false, Instant::now());
                    self.hush();
                }
            }
            Event::Notified(incoming) => {
                let Incoming { note, sound } = *incoming;
                let critical = note.critical();
                let out = self.notes.arrive(note, self.shell.quiet, Instant::now());
                if let Some(sound) = sound
                    && (!self.shell.quiet || critical)
                {
                    bus::play(&sound);
                }
                self.notes_out(out);
            }
            Event::Withdrawn(id) => {
                let out = self.notes.withdraw(id);
                self.notes_out(out);
            }
            Event::Due => {
                let out = self.notes.expire(Instant::now());
                self.notes_out(out);
            }
            Event::Answered { client, id, said, source, outcome } => {
                self.pondered_one();
                if outcome.ok && !self.settings.brain.is_empty() {
                    let answer = outcome.answer.as_ref().map_or(outcome.say.clone(), |a| format!("{}\n{a}", outcome.say));
                    self.talk.push((Instant::now(), said.clone(), answer));
                }
                let handler = if self.settings.brain.is_empty() { "llm".to_string() } else { format!("brain:{}", self.settings.brain) };
                let mut outcome = outcome;
                if outcome.ok && outcome.say.is_empty() {
                    match self.card.as_ref().and_then(|c| c["titulo"].as_str()).filter(|t| !t.is_empty()) {
                        Some(title) => outcome.say = title.to_string(),
                        None => outcome = Outcome::fail("El modelo no ha dicho nada"),
                    }
                }
                // Sin tarjeta del modelo, una respuesta larga no cabe en la
                // notificación: va en una tarjeta hecha con su texto.
                if self.card.is_none() && outcome.ok && (outcome.say.chars().count() > 110 || outcome.say.contains('\n')) {
                    self.card = Some(crate::card::from_text(&outcome.say));
                }
                // Con tarjeta, Veyl la enseña en la isla en vez de la notificación.
                let card = self.card.take().filter(|_| outcome.ok && (source == "diary" || self.clients.values().any(|c| c.shell)));
                let picture = self.picture.take();
                if let Some(mut card) = card {
                    if let Some(url) = picture.and_then(|(rx, since)| {
                        rx.recv_timeout(Duration::from_millis(3500).saturating_sub(since.elapsed()).max(Duration::from_millis(200))).ok()
                    }) {
                        card["imagen"] = json!(url);
                    }
                    if source == "diary" {
                        // Desde la vista del diario la tarjeta va con la respuesta, ahí mismo.
                        card["say"] = json!(outcome.say);
                        card["said"] = json!(said);
                        self.shown_card = Some(card);
                    } else {
                        self.present(card, &outcome, &said);
                    }
                }
                self.answer(client, id, &said, &source, None, &handler, outcome);
                self.carded = false;
            }
            Event::Sensed { client, id, said, source, parse, outcome, card } => {
                self.pondered_one();
                if let Some(card) = card
                    && source != "cli"
                {
                    self.present(card, &outcome, &said);
                }
                self.answer(client, id, &said, &source, Some(&parse), "grammar", outcome);
                self.carded = false;
            }
            Event::Tool { name, input, source, reply } => {
                let out = self.tool(&name, &input, &source);
                let _ = reply.send(out);
            }
            Event::ViewChosen { name, candidates, here, source, choice } => {
                self.pondered_one();
                let choice = choice.unwrap_or_else(|why| {
                    eprintln!("veyld: vista «{name}» sin cerebro: {why}");
                    views::guess(&name, &candidates, here)
                });
                let outcome = orders::make_view(self, &name, &candidates, choice);
                self.later(&source, &outcome);
            }
            Event::Calendar(told) => {
                if !told.is_null() {
                    self.broadcast(&json!({ "calendar": told }), |c| c.shell);
                }
            }
            Event::Reload => {
                let (grammar, error) = grammar();
                if let Some(error) = error {
                    eprintln!("veyld: {error}");
                }
                self.grammar = grammar;
                self.reload_apps();
            }
            Event::Stop => return false,
        }
        true
    }
}

/// Pregunta al modelo; dos minutos como mucho.
fn model(command: &str, said: &str, source: &str) -> Outcome {
    let output = crate::process::output(Command::new("sh").args(["-c", command]).env("VEYL_SOURCE", source), said.as_bytes(), Duration::from_secs(120));
    match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let (say, rest) = text.split_once('\n').map(|(a, b)| (a.to_string(), Some(b.trim().to_string()))).unwrap_or((text, None));
            if say.is_empty() { Outcome::fail("El modelo no dijo nada") } else { Outcome { ok: true, say, answer: rest.filter(|r| !r.is_empty()) } }
        }
        _ => Outcome::fail("El modelo no contestó"),
    }
}

/// Una notificación de Veyl (que es el servidor), si hay notify-send.
fn notify(outcome: &Outcome) {
    let body = outcome.answer.clone().unwrap_or_default();
    let time = if body.len() > 80 { "15000" } else { "5000" };
    let urgency = if outcome.ok { "low" } else { "normal" };
    orders::detach(&["notify-send", "-a", "Veyl", "-u", urgency, "-t", time, &outcome.say, &body]);
}

/// Una línea IPC no puede reservar memoria sin límite esperando un salto.
fn frame(reader: &mut impl BufRead) -> io::Result<Option<String>> {
    const LIMIT: u64 = 65536;
    let mut line = String::new();
    let count = reader.take(LIMIT + 1).read_line(&mut line)?;
    if count as u64 > LIMIT {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "mensaje IPC demasiado grande"));
    }
    Ok((count > 0).then_some(line))
}

fn listen(listener: UnixListener, tx: Sender<Event>) {
    let mut next = 0;
    for stream in listener.incoming() {
        let Ok(stream) = stream else {
            continue;
        };
        next += 1;
        let id = next;
        let Ok(writer) = stream.try_clone() else {
            continue;
        };
        // Quien no lee no frena a los demás.
        let _ = writer.set_write_timeout(Some(Duration::from_millis(500)));
        if tx.send(Event::Join(id, writer)).is_err() {
            return;
        }
        let tx = tx.clone();
        thread::spawn(move || {
            let mut reader = BufReader::new(stream);
            while let Ok(Some(line)) = frame(&mut reader) {
                if let Ok(message) = serde_json::from_str::<Value>(&line)
                    && tx.send(Event::Said(id, message)).is_err()
                {
                    return;
                }
            }
            let _ = tx.send(Event::Left(id));
        });
    }
}

/// Los avisos de Hyprland. Si el socket se corta y Hyprland no vuelve, la
/// sesión ha acabado, y veyld con ella.
fn hypr_events(hypr: Hypr, tx: Sender<Event>) {
    loop {
        if let Ok(reader) = hypr.events() {
            for line in reader.lines() {
                let Ok(line) = line else {
                    break;
                };
                if let Some((name, data)) = line.split_once(">>")
                    && tx.send(Event::Hypr(name.to_string(), data.to_string())).is_err()
                {
                    return;
                }
            }
        }
        let back = (0..10).any(|_| {
            thread::sleep(Duration::from_secs(1));
            hypr.alive()
        });
        if !back {
            let _ = tx.send(Event::HyprLost);
            return;
        }
    }
}

pub fn run() -> Result<(), String> {
    let socket = config::socket();
    let dir = socket.parent().ok_or("socket sin carpeta")?.to_path_buf();
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
    // Uno por sesión: el que llega tarde se va sin hacer nada.
    let lock = File::create(socket.with_extension("lock")).map_err(|e| format!("{}: {e}", socket.display()))?;
    match lock.try_lock() {
        Ok(()) => {}
        Err(fs::TryLockError::WouldBlock) => {
            eprintln!("veyld: ya hay uno en marcha ({})", socket.display());
            return Ok(());
        }
        Err(fs::TryLockError::Error(e)) => return Err(format!("{}: {e}", socket.display())),
    }
    let hypr = Hypr::find().ok_or("no hay ninguna sesión de Hyprland viva (HYPRLAND_INSTANCE_SIGNATURE)")?;
    let _ = fs::remove_file(&socket);
    let listener = UnixListener::bind(&socket).map_err(|e| format!("{}: {e}", socket.display()))?;

    let path = config::state().join("journal.db");
    let journal = match Journal::open(&path) {
        Ok(journal) => Some(journal),
        Err(e) => {
            eprintln!("veyld: sin diario ({}: {e})", path.display());
            None
        }
    };

    let (tx, rx) = mpsc::channel();
    {
        let tx = tx.clone();
        thread::spawn(move || listen(listener, tx));
    }
    {
        let (tx, hypr) = (tx.clone(), hypr.clone());
        thread::spawn(move || hypr_events(hypr, tx));
    }
    {
        let tx = tx.clone();
        thread::spawn(move || agents::watch(tx));
    }
    {
        let tx = tx.clone();
        thread::spawn(move || {
            let why = idle::watch(tx);
            eprintln!("veyld: sin aviso de inactividad: {why}");
        });
    }
    {
        let tx = tx.clone();
        thread::spawn(move || {
            let why = media::watch(tx);
            if !why.is_empty() {
                eprintln!("veyld: sin saber qué suena: {why}");
            }
        });
    }
    {
        let tx = tx.clone();
        thread::spawn(move || {
            while tx.send(Event::Tick).is_ok() {
                thread::sleep(BEAT);
            }
        });
    }
    {
        let tx = tx.clone();
        let mut signals = signal_hook::iterator::Signals::new([signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT, signal_hook::consts::SIGHUP])
            .map_err(|e| format!("señales: {e}"))?;
        thread::spawn(move || {
            for signal in signals.forever() {
                let event = if signal == signal_hook::consts::SIGHUP { Event::Reload } else { Event::Stop };
                if tx.send(event).is_err() {
                    return;
                }
            }
        });
    }

    let mut daemon = Daemon::new(hypr, journal, tx.clone());
    let path = config::state().join("notifications.db");
    match Notes::open(&path) {
        Ok(notes) => daemon.notes = notes,
        Err(e) => eprintln!("veyld: notificaciones sin guardar ({}: {e})", path.display()),
    }
    daemon.clean_notes();
    let kept: Vec<u32> = daemon.notes.all().iter().map(|n| n.id).collect();
    bus::sweep_pictures(&daemon.pictures, &kept);
    {
        let (jobs, taken) = mpsc::channel();
        let tx = tx.clone();
        thread::spawn(move || calendars::watch(taken, tx));
        daemon.calendars = Some(jobs);
    }
    daemon.ear = Some(voice::spawn(tx.clone()));
    daemon.mouth = Some(tts::spawn());
    // La voz se carga ya (tarda unos segundos): así la primera respuesta suena al momento.
    if daemon.settings.speak
        && let Some(mouth) = &daemon.mouth
    {
        let _ = mouth.send(tts::Say::Warm(daemon.settings.style()));
    }
    match Bus::start(tx, daemon.notes.next_id(), daemon.pictures.clone()) {
        Ok(bus) => daemon.bus = Some(bus),
        Err(e) => eprintln!("veyld: sin servidor de notificaciones: {e}"),
    }
    eprintln!("veyld: en marcha en {} ({} reglas)", socket.display(), daemon.grammar.rules());
    daemon.start();
    loop {
        // Se espera a lo próximo que pase o a que caduque un aviso.
        let event = match daemon.notes.next_due() {
            Some(due) => match rx.recv_timeout(due.saturating_duration_since(Instant::now())) {
                Ok(event) => event,
                Err(RecvTimeoutError::Timeout) => Event::Due,
                Err(RecvTimeoutError::Disconnected) => break,
            },
            None => match rx.recv() {
                Ok(event) => event,
                Err(_) => break,
            },
        };
        if !daemon.handle(event) {
            break;
        }
    }
    if let Some(journal) = &mut daemon.journal {
        journal.finish(now());
    }
    let _ = fs::remove_file(&socket);
    eprintln!("veyld: fuera");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn daemon() -> Daemon {
        let (tx, _) = mpsc::channel();
        let mut d = Daemon::new(Hypr::offline(), Some(Journal::memory()), tx);
        // Los ajustes son los del usuario: en los tests, nadie a quien preguntar.
        d.settings.brain.clear();
        d.settings.llm.clear();
        d
    }

    #[test]
    fn ipc_frames_are_bounded_and_preserve_following_messages() {
        let mut reader = io::Cursor::new(b"{}\n[]\n");
        assert_eq!(frame(&mut reader).unwrap().as_deref(), Some("{}\n"));
        assert_eq!(frame(&mut reader).unwrap().as_deref(), Some("[]\n"));
        assert!(frame(&mut reader).unwrap().is_none());
        assert!(frame(&mut io::Cursor::new(vec![b'x'; 65537])).is_err());
    }

    fn planned(d: &Daemon, said: &str) -> Vec<(String, Value)> {
        orders::plan(d, said).unwrap_or_else(|| panic!("no entiende «{said}»")).into_iter().map(|p| (p.intent, Value::Object(p.args))).collect()
    }

    #[test]
    fn search_and_play() {
        let d = daemon();
        let intents = |said: &str| planned(&d, said).into_iter().map(|(i, _)| i).collect::<Vec<_>>();
        let plan = planned(&d, "abre firefox y busca recetas de lentejas");
        assert_eq!(plan[0].0, "app.open");
        assert_eq!(plan[1], ("web.search".to_string(), json!({ "q": "recetas de lentejas", "whole": true, "via": "firefox" })));
        let plan = planned(&d, "abre spotify y pon bohemian rhapsody");
        assert_eq!(plan[1].0, "play.search");
        assert_eq!(plan[1].1["via"], "spotify");
        assert_eq!(plan[1].1["q"], "bohemian rhapsody");
        let plan = planned(&d, "abre youtube y pon música lofi");
        assert_eq!(intents("abre youtube y pon música lofi"), ["web.open", "play.search"]);
        assert_eq!(plan[1].1["via"], "youtube");
        assert_eq!(plan[0].1["then"], true);
        // Lo que es una web, la web: «abre firefox y pon primevideo».
        let plan = planned(&d, "abre firefox y pon primevideo");
        assert_eq!(plan[1], ("web.open".to_string(), json!({ "site": "primevideo", "via": "firefox" })));
        assert_eq!(planned(&d, "abre prime video en firefox"), [("web.open".to_string(), json!({ "site": "prime video", "via": "firefox", "lucky": true }))]);
        assert_eq!(intents("abre netflix"), ["web.open"]);
        assert_eq!(intents("abre la web de el corte inglés"), ["web.open"]);
        assert!(orders::describe_plan(&d, &orders::plan(&d, "abre la web de el corte inglés").unwrap()).ready);
        assert_eq!(intents("abre firefox"), ["app.open"]);
        // Lo libre no se parte por «y».
        assert_eq!(planned(&d, "busca pan y leche"), [("web.search".to_string(), json!({ "q": "pan y leche", "whole": true }))]);
        assert_eq!(planned(&d, "pon simon y garfunkel en spotify")[0].1["q"], "simon y garfunkel");
        assert_eq!(planned(&d, "busca gatos en youtube")[0].1["where"], "youtube");
        assert_eq!(planned(&d, "pon música de estopa")[0].1, json!({ "q": "estopa", "music": true, "whole": true }));
        // Pero «busca … y abre …» siguen siendo dos.
        assert_eq!(intents("busca gatos y abre kitty"), ["web.search", "app.open"]);
        // Lo de antes no cambia.
        assert_eq!(intents("pon el volumen al 50"), ["volume.set"]);
        assert_eq!(intents("pon un temporizador de 10 minutos"), ["timer.start"]);
        assert_eq!(intents("pon esto en el otro monitor"), ["window.monitor"]);
        assert_eq!(intents("busca en el diario gatos"), ["recall.find"]);
        // «Pon …» sin saber dónde no se da por hecho: mejor el modelo.
        assert!(!orders::describe_plan(&d, &orders::plan(&d, "pon una alarma a las siete").unwrap()).ready);
        assert!(orders::describe_plan(&d, &orders::plan(&d, "abre spotify y pon estopa").unwrap()).ready);
        // Solo es música dicha como tal: dónde o qué.
        assert_eq!(planned(&d, "pon en spotify la macarena")[0].1["where"], "spotify");
        assert_eq!(planned(&d, "pon la canción despacito")[0].1["music"], true);
        assert!(!orders::describe_plan(&d, &orders::plan(&d, "pon a cargar el móvil").unwrap()).ready);
        assert!(!orders::describe_plan(&d, &orders::plan(&d, "pon algo de ruido blanco").unwrap()).ready);
    }

    #[test]
    fn questions_about_the_computer() {
        let d = daemon();
        let cases = [
            ("qué está ocupando más en el ordenador", "system.disk"),
            ("cuánto espacio me queda", "system.disk"),
            ("porque va lento la red", "system.net"),
            ("por qué va tan lento internet", "system.net"),
            ("oye, cómo va el wifi", "system.net"),
            ("qué programa está usando más internet", "system.net"),
            ("qué proceso está ralentizando esto", "system.status"),
            ("por qué va tan lento el ordenador", "system.status"),
            ("cómo va el pc", "system.status"),
            ("qué está usando la cpu", "system.cpu"),
            ("por qué suena tanto el ventilador", "system.cpu"),
            ("qué se está comiendo la ram", "system.memory"),
            ("qué programa consume más memoria", "system.memory"),
            ("qué temperatura tiene la cpu", "system.temp"),
            ("qué está usando la gráfica", "system.gpu"),
            ("cuánta batería queda", "system.battery"),
        ];
        for (said, intent) in cases {
            assert_eq!(planned(&d, said)[0].0, intent, "«{said}»");
        }
        // Lo de antes sigue igual.
        assert_eq!(planned(&d, "usa el tema nord")[0].0, "scheme.set");
        assert_eq!(planned(&d, "cómo va claude")[0].0, "agents.ask");
    }

    #[test]
    fn context_resolves_pronouns_and_there() {
        let mut d = daemon();
        // Sin contexto, «muévelo» es la ventana que tienes delante.
        assert_eq!(planned(&d, "muévelo al nivel tres"), [("window.move".to_string(), json!({ "n": 3 }))]);
        d.context = orders::Context { app: "firefox".into(), window: "0x1".into(), at: Some(Instant::now()), ..Default::default() };
        assert_eq!(planned(&d, "muévelo al nivel tres"), [("window.move".to_string(), json!({ "n": 3, "app": "firefox", "window": "0x1" }))]);
        assert_eq!(
            planned(&d, "y ponlo a pantalla completa"),
            [("window.fullscreen".to_string(), json!({ "mode": "fullscreen", "on": true, "app": "firefox", "window": "0x1" }))]
        );
        // «Esto» es lo que tienes delante, no lo de antes.
        assert_eq!(planned(&d, "cierra esto"), [("window.close".to_string(), json!({}))]);
        // Dentro de la misma frase manda lo dicho en ella.
        assert_eq!(planned(&d, "mueve kitty al nivel 2 y ponlo a la izquierda")[1], ("window.place".to_string(), json!({ "side": "left", "app": "kitty" })));
        assert_eq!(planned(&d, "mueve kitty al nivel 2 y después ve allí")[1], ("level.go".to_string(), json!({ "n": 2 })));
        // «Ve allí» sin nivel de antes va a la aplicación.
        assert_eq!(planned(&d, "ve allí"), [("window.focus".to_string(), json!({ "app": "firefox", "window": "0x1" }))]);
        // Lo de hace más de cinco minutos ya no cuenta.
        d.context.at = Instant::now().checked_sub(Duration::from_secs(600));
        assert_eq!(planned(&d, "ciérralo"), [("window.close".to_string(), json!({}))]);
    }

    #[test]
    fn the_brain_cannot_skip_a_confirmation() {
        let mut d = daemon();
        let out = d.tool("veyl", &json!({ "orden": "cierra todas las ventanas" }), "voice");
        assert!(out.contains("Di «sí» para confirmar"), "{out}");
        let (pending, _, owner) = d.pending.as_ref().expect("espera un sí");
        assert_eq!((pending[0].intent.as_str(), owner.as_str()), ("window.others", "voice"));
        assert!(d.tool("veyl", &json!({ "orden": "haz una cosa rara" }), "voice").contains("no entiende"));
        assert!(d.tool("contexto", &json!({}), "voice").contains("ventanas"));
    }

    #[test]
    fn dangerous_step_waits_with_the_rest() {
        let mut d = daemon();
        d.settings.llm.clear();
        d.say(1, Value::Null, "cierra todas las ventanas y bloquea la pantalla", "cli", false);
        let (pending, _, _) = d.pending.as_ref().expect("espera un sí");
        assert_eq!(pending.iter().map(|p| p.intent.as_str()).collect::<Vec<_>>(), ["window.others", "session.lock"]);
    }

    #[test]
    fn unrelated_order_clears_pending_confirmation() {
        let mut d = daemon();
        d.settings.llm.clear();
        d.pending = Some((vec![Parse { intent: "session.off".into(), args: Default::default(), heard: "".into() }], Instant::now(), "cli".into()));
        d.say(1, Value::Null, "unrelated unknown command", "cli", false);
        assert!(d.pending.is_none());
    }

    #[test]
    fn disabled_journal_records_nothing() {
        let mut d = daemon();
        d.settings.journal = false;
        d.start();
        d.hypr_event("openwindow", "123,1,firefox,title");
        d.shell_state(&json!({"locked": true}));
        d.agents_changed(vec![]);
        d.answer(1, Value::Null, "secret", "cli", None, "none", Outcome::done("ok"));
        let db = d.journal.as_ref().unwrap().db();
        for table in ["focus", "windows", "agents", "away", "orders"] {
            let count: i64 = db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).unwrap();
            assert_eq!(count, 0, "{table}");
        }
    }

    #[test]
    fn private_windows_do_not_store_titles_or_folders() {
        let mut d = daemon();
        d.settings.journal = true;
        d.hypr_event("openwindow", "123,1,firefox,Secret — Navegación privada");
        d.hypr_event("openwindow", "456,1,org.keepassxc.KeePassXC,Passwords");
        let db = d.journal.as_ref().unwrap().db();
        let count: i64 = db.query_row("SELECT COUNT(*) FROM windows WHERE title != ''", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 0);
        let seen =
            d.seen(&Window { class: "firefox".into(), title: "Secret — Private Browsing".into(), pid: std::process::id() as i32, ..Window::default() });
        assert!(seen.title.is_empty());
        assert!(seen.folder.is_empty());
        assert!(!private("firefox", "ordinary page"));
    }
}
