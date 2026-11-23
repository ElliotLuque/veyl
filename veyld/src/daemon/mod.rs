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
//! {"stop": 7}                            deja de pensar la pregunta 7 (la `n` de `thinking`)
//! {"agent_answer": {"id": "claude:123", "key": "1"}}   contesta a lo que pregunta un agente
//!                                        (una de las opciones de su `ask`, con una tecla en su terminal)
//! {"id": 5, "agents_demo": 2}            agentes de ejemplo: uno trabajando y otro preguntando lo
//!                                        de la pantalla 2 de `agents::SAMPLES` ("next": la siguiente; "off": fuera)
//! ```
//!
//! Lo que manda: `{"id", …}` en respuesta a lo que lo llevaba, `{"agents":
//! […]}` y, a Veyl, `{"call": "timer.start", "args": [25]}` y las
//! notificaciones: todas al saludar (`{"notes": […]}`, la más reciente
//! primero), y luego cada una que llega o cambia (`{"note": {…}}`) y cada
//! una que se va (`{"note_gone": 7}`); lo que piensa el cerebro (`{"thinking":
//! {"said", "since", "n", "stop", "steps": [{"icono", "texto",
//! "at"}], "sources": [{"titulo", "host", "url"}]}}`, otra vez con cada paso, y
//! `null` al acabar), su tarjeta en cuanto la compone (`{"card": {…, "n",
//! "live": true}}`), luego la definitiva (`{"card": {…, "n", "say"}}`) y lo
//! que le llega después (`{"card_patch": {"n", "imagen"}}`); lo que pide que no se bloquee
//! (`{"awake": [{"app", "reason"}]}`), al saludar y cada vez que cambia; y mientras escucha, `{"voice": {"level":
//! 0.4}}` cada 50 ms, `{"voice": {"speaking": true}}` al empezar a hablar y
//! `{"voice": {"heard": "…"}}` con lo dicho.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::agents::{self, Agent};
use crate::apps::{self, App};
use crate::awake::{self, Hold};
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

mod sense;
mod talk;

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
    /// Lo que pide ahora que no se bloquee (un vídeo en el navegador).
    Held(Vec<Hold>),
    Tick,
    Join(u64, UnixStream),
    Said(u64, Value),
    Left(u64),
    /// Lo que contestó el modelo a la pregunta `n`.
    Answered {
        client: u64,
        id: Value,
        said: String,
        source: String,
        outcome: Outcome,
        n: u64,
    },
    /// Lo que va haciendo el cerebro con la pregunta `n`.
    Step {
        n: u64,
        step: brain::Step,
    },
    /// La portada de la tarjeta de la pregunta `n`, sacada de uno de sus enlaces.
    Cover {
        n: u64,
        url: String,
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
    /// La pantalla de ejemplo de la demo de agentes que se enseña, si hay demo.
    demo: Option<usize>,
    pub split: u32,
    clients: HashMap<u64, Client>,
    /// La ventana con el foco, se esté o no.
    current: Option<Window>,
    idle: bool,
    /// Lo que pide que no se bloquee: mientras haya algo, se está.
    held: Vec<Hold>,
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
    /// Cuántas preguntas está pensando el cerebro, y la última (para Veyl),
    /// con su número: el último que se ha dado.
    pondering: usize,
    pondered: Value,
    thought: u64,
    /// Las preguntas al cerebro que aún no han contestado, por su número: lo
    /// que las para y lo preguntado. Una que ya no está se paró.
    asking: HashMap<u64, (Arc<AtomicBool>, String)>,
    /// La tarjeta que el cerebro ha compuesto para lo que piensa ahora, y si
    /// la respuesta va en ella (entonces, sin notificación).
    card: Option<Value>,
    carded: bool,
    /// La tarjeta que se acaba de enseñar, para el historial de la respuesta.
    shown_card: Option<Value>,
    /// El día que se mira en la vista del diario, si la pregunta viene de ahí.
    diary_day: String,
    /// La portada que se busca para la tarjeta de la pregunta `n` en sus
    /// enlaces, mientras el modelo acaba de escribir o después: entonces, la
    /// tarjeta tal como se apuntó en el diario y su fila, para ponérsela.
    cover: Option<(u64, Option<(Value, i64)>)>,
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
        let mut daemon = Daemon {
            split: hypr.split(),
            hypr,
            journal,
            grammar,
            apps: apps::load(),
            apps_read: Instant::now(),
            settings: config::settings(),
            shell: Shell::default(),
            agents: Vec::new(),
            demo: None,
            clients: HashMap::new(),
            current: None,
            idle: false,
            held: Vec::new(),
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
            // Los números de las preguntas no se repiten aunque veyld se
            // reinicie: Veyl recuerda el de la tarjeta que cerraste, y una
            // del historial lleva el suyo.
            thought: now().max(0) as u64,
            asking: HashMap::new(),
            card: None,
            carded: false,
            shown_card: None,
            diary_day: String::new(),
            cover: None,
            views: Views::load(),
            calendars: None,
        };
        daemon.recall_talk();
        daemon
    }

    /// Lo hablado con el cerebro en los últimos diez minutos, del diario: una
    /// pregunta de seguimiento («¿y en Madrid?») sigue la conversación aunque
    /// veyld se haya reiniciado entre medias.
    fn recall_talk(&mut self) {
        let Some(journal) = self.journal.as_ref().filter(|_| self.settings.journal) else {
            return;
        };
        let at = now();
        self.talk = journal
            .talk(at - 600_000)
            .into_iter()
            .filter_map(|(time, said, answer)| Some((Instant::now().checked_sub(Duration::from_millis((at - time).max(0) as u64))?, said, answer)))
            .collect();
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
        let message = json!({ "agents": self.agents_view() });
        self.broadcast(&message, |c| c.agents);
    }

    /// Los agentes como los ve Veyl: los de verdad y, si hay demo, los de ejemplo.
    fn agents_view(&self) -> Vec<Agent> {
        let mut out = self.agents.clone();
        out.extend(self.demo.map(agents::demo).unwrap_or_default());
        out
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
            let agents = json!({ "agents": self.agents_view() });
            self.send(client, &agents);
            let notes = json!({ "notes": self.notes.all() });
            self.send(client, &notes);
            let thinking = json!({ "thinking": self.pondered });
            self.send(client, &thinking);
            let views = json!({ "views": self.views_listing() });
            self.send(client, &views);
            let awake = self.awake();
            self.send(client, &awake);
        }
        if text("watch").as_deref() == Some("agents") {
            if let Some(c) = self.clients.get_mut(&client) {
                c.agents = true;
            }
            let agents = json!({ "agents": self.agents_view() });
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
        if let Some(n) = message.get("stop").and_then(Value::as_u64) {
            self.stop_thinking(n);
        }
        if let Some(demo) = message.get("agents_demo") {
            self.demo = match demo.as_str() {
                Some("off") => None,
                Some(_) => Some(self.demo.map_or(0, |n| (n + 1) % agents::SAMPLES.len())),
                None => demo.as_u64().map(|n| n as usize % agents::SAMPLES.len()),
            };
            let message = json!({ "agents": self.agents_view() });
            self.broadcast(&message, |c| c.agents);
            let shown = self.demo.map(|n| json!({ "n": n + 1, "of": agents::SAMPLES.len(), "name": agents::SAMPLES[n].2, "agent": agents::SAMPLES[n].1 }));
            self.send(client, &json!({ "id": id, "demo": shown }));
        }
        // Contestar a un agente: solo con una de las opciones que enseña ahora.
        // Al de ejemplo, contestarle lo quita.
        if let Some(answer) = message.get("agent_answer") {
            let field = |key: &str| answer.get(key).and_then(Value::as_str).unwrap_or("");
            if field("id").starts_with("demo:") {
                self.demo = None;
                let message = json!({ "agents": self.agents_view() });
                self.broadcast(&message, |c| c.agents);
            } else if let Some(agent) = self.agents.iter().find(|a| a.id == field("id"))
                && !agent.keys.is_empty()
                && agent.ask.as_ref().is_some_and(|ask| ask.options.iter().any(|o| o.key == field("key")))
            {
                // Una letra («Y») se pulsa como la pulsarías tú: en minúscula.
                let key = field("key");
                let key = if key.len() == 1 && key.chars().all(|c| c.is_ascii_alphabetic()) { key.to_ascii_lowercase() } else { key.to_string() };
                let mut command: Vec<&str> = agent.keys.iter().map(String::as_str).collect();
                command.push(&key);
                orders::detach(&command);
            }
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
            let agents = json!({ "id": id, "agents": self.agents_view() });
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
            Event::Held(held) => {
                let before = self.absence();
                self.held = held;
                self.presence(now(), before);
                let awake = self.awake();
                self.broadcast(&awake, |c| c.shell);
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
            Event::Answered { client, id, said, source, outcome, n } => self.answered(client, id, &said, &source, outcome, n),
            Event::Step { n, step } => self.stepped(n, step),
            Event::Cover { n, url } => self.covered(n, url),
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
            let why = awake::watch(tx);
            eprintln!("veyld: sin atender lo que pide no bloquear: {why}");
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
    fn thinking_tells_its_steps_and_can_be_stopped() {
        let mut d = daemon();
        d.settings.journal = true;
        let said = "¿cuándo sale GTA 6?";
        let n = d.ponder_as(said, json!({ "stop": true }));
        d.asking.insert(n, (Arc::default(), said.into()));
        let search = brain::Step::Doing { icon: "magnifying-glass", text: "Busca «GTA 6»".into() };
        d.stepped(n, search.clone());
        d.stepped(n, search.clone());
        let page = |url: &str| brain::Source { title: String::new(), url: url.into() };
        d.stepped(n, brain::Step::Found(vec![page("https://www.ign.com/a"), page("https://ign.com/b"), page("https://rockstargames.com/VI")]));
        // Lo de otra pregunta no se mezcla.
        d.stepped(n + 1, brain::Step::Doing { icon: "globe", text: "Lee otra.com".into() });
        assert_eq!(d.pondered["steps"].as_array().unwrap().len(), 1);
        assert_eq!(d.pondered["steps"][0]["texto"], "Busca «GTA 6»");
        let hosts: Vec<&str> = d.pondered["sources"].as_array().unwrap().iter().map(|s| s["host"].as_str().unwrap()).collect();
        assert_eq!(hosts, ["ign.com", "rockstargames.com"]);
        // La tarjeta se guarda para cuando conteste.
        d.tool("mostrar", &json!({ "titulo": "GTA VI", "bloques": [{ "tipo": "texto", "texto": "Sale en 2026" }] }), "launcher");
        assert!(d.card.is_some());
        // Se para: deja de enseñarse ya, y lo que conteste después se tira.
        let stop = d.asking[&n].0.clone();
        d.stop_thinking(n);
        assert!(stop.load(Ordering::Relaxed));
        assert!(d.pondered.is_null() && d.card.is_none());
        d.answered(0, json!(1), said, "launcher", Outcome { ok: true, say: "Sale el 19".into(), answer: None }, n);
        assert_eq!(d.pondering, 0);
        assert!(d.talk.is_empty());
        assert!(d.journal.as_ref().unwrap().asked(5, None).is_empty());
    }

    #[test]
    fn talk_survives_a_restart() {
        let mut journal = Journal::memory();
        let at = now();
        journal.order(at - 20 * 60_000, "launcher", "algo de hace rato", None, "", "brain:claude-code", true, "Viejo", "");
        journal.order(at - 60_000, "launcher", "fotos de sastrerías", None, "", "brain:claude-code", true, "Te dejo fotos\nDe Savile Row", "");
        journal.order(at - 30_000, "launcher", "sube el volumen", Some("volume.up"), "", "grammar", true, "Volumen 40", "");
        journal.order(at - 10_000, "launcher", "¿qué es esto?", None, "", "brain:claude-code", false, "Claude ha fallado", "");
        let talk = journal.talk(at - 600_000);
        assert_eq!(talk.len(), 1);
        assert_eq!((talk[0].1.as_str(), talk[0].2.as_str()), ("fotos de sastrerías", "Te dejo fotos\nDe Savile Row"));
        let mut d = daemon();
        d.settings.journal = true;
        d.journal = Some(journal);
        d.recall_talk();
        assert_eq!(d.talk.len(), 1);
        assert!(d.talk[0].0.elapsed() >= Duration::from_secs(59));
        // Sin diario no se recuerda nada.
        d.settings.journal = false;
        d.talk.clear();
        d.recall_talk();
        assert!(d.talk.is_empty());
    }

    #[test]
    fn a_late_cover_reaches_the_journal() {
        let mut d = daemon();
        d.settings.journal = true;
        let n = d.ponder_as("¿qué es Veyl?", json!({}));
        d.asking.insert(n, (Arc::default(), "¿qué es Veyl?".into()));
        d.card =
            crate::card::clean(&json!({ "titulo": "Veyl", "bloques": [{ "tipo": "enlaces", "items": [{ "titulo": "Web", "url": "https://veyl.dev" }] }] }));
        d.cover = Some((n, None));
        d.answered(0, json!(1), "¿qué es Veyl?", "diary", Outcome { ok: true, say: "Un escritorio".into(), answer: None }, n);
        assert!(matches!(&d.cover, Some((at, Some(_))) if *at == n));
        // Otra pregunta no se la lleva; la suya, sí, y una segunda ya no cuenta.
        d.covered(n + 1, "https://otra/b.jpg".into());
        d.covered(n, "https://veyl.dev/og.jpg".into());
        d.covered(n, "https://veyl.dev/otra.jpg".into());
        let rows = d.journal.as_ref().unwrap().asked(1, None);
        assert_eq!(rows[0]["card"]["imagen"], "https://veyl.dev/og.jpg");
        assert_eq!(rows[0]["card"]["say"], "Un escritorio");
        assert!(d.cover.is_none());
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
