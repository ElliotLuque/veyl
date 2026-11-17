//! Los calendarios de fuera: los que el usuario apunta en
//! `~/.config/veyl/calendars.json` y que veyld trae cada 15 minutos.
//!
//! No hay cuentas ni contraseñas: cada calendario es una dirección iCalendar
//! (lo que Google llama «dirección secreta en formato iCal», Outlook
//! «publicar calendario», Nextcloud «enlace de suscripción»), un archivo
//! `.ics` o una carpeta de ellos (la de vdirsyncer, que sí sincroniza en los
//! dos sentidos con CalDAV). Es de solo lectura: lo que se apunta en Veyl va a
//! su agenda, no a esos calendarios.
//!
//! ```json
//! [
//!   { "name": "Uni", "url": "https://calendar.google.com/calendar/ical/…/basic.ics", "color": "#7aa2f7" },
//!   { "name": "Casa", "url": "~/.local/share/calendars/casa" },
//!   { "name": "Festivos", "url": "webcal://…", "on": false }
//! ]
//! ```
//!
//! Con la cuenta de Google conectada (ver `google.rs`), un calendario puede
//! ser suyo: `{ "name": "Elliot", "google": "primary" }` (o el ID del
//! calendario). Esos se leen por la API, sin retraso, y en los que se puede
//! escribir veyld apunta y borra lo que se le pida.
//!
//! Lo traído va a `$XDG_STATE_HOME/veyl/events.json` (sin las direcciones,
//! que son secretas), con los eventos ya desplegados (los que se repiten, uno
//! por vez) entre dos meses atrás y un año adelante; Veyl lo junta con su
//! agenda. Si un calendario no se puede traer, se queda con lo de la última vez.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant, SystemTime};

use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc, Weekday};
use chrono_tz::Tz;
use serde_json::{Value, json};

use crate::config;
use crate::daemon::Event;
use crate::google;

/// Cada cuánto se vuelven a traer.
const EVERY: Duration = Duration::from_secs(15 * 60);
/// Cada cuánto se mira si ha cambiado la lista.
const GLANCE: Duration = Duration::from_secs(15);
/// Lo que se despliega: desde tantos días atrás hasta tantos adelante.
const BEHIND: i64 = 62;
const AHEAD: i64 = 400;
/// Lo más que se lee de un calendario.
const LIMIT: u64 = 32 << 20;

#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    pub name: String,
    pub url: String,
    /// El ID del calendario en la cuenta de Google; vacío, es de iCal.
    pub google: String,
    pub color: String,
    pub on: bool,
}

/// Dónde se apuntan: en la configuración, solo para ti (las direcciones son
/// secretas).
pub fn path() -> PathBuf {
    config::config().join("calendars.json")
}

/// Dónde queda lo traído.
pub fn output() -> PathBuf {
    config::state().join("events.json")
}

pub fn sources() -> Vec<Source> {
    let path = path();
    let Ok(text) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    // Lleva direcciones secretas: que no las lea nadie más.
    if let Ok(meta) = fs::metadata(&path)
        && meta.permissions().mode() & 0o077 != 0
    {
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    let json: Value = match serde_json::from_str(&text) {
        Ok(json) => json,
        Err(e) => {
            eprintln!("veyld: {} no es JSON válido: {e}", path.display());
            return Vec::new();
        }
    };
    let list = json.as_array().cloned().unwrap_or_default();
    let mut seen = HashSet::new();
    list.iter()
        .filter_map(|c| {
            let text = |key: &str| c.get(key).and_then(Value::as_str).unwrap_or("").trim().to_string();
            let (url, google) = (text("url"), text("google"));
            if url.is_empty() && google.is_empty() {
                return None;
            }
            let mut name = Some(text("name")).filter(|n| !n.is_empty()).unwrap_or_else(|| if url.is_empty() { google.clone() } else { guess_name(&url) });
            // Dos con el mismo nombre se confundirían al guardar lo de la última vez.
            let base = name.clone();
            let mut n = 2;
            while !seen.insert(name.clone()) {
                name = format!("{base} {n}");
                n += 1;
            }
            Some(Source { name, url, google, color: text("color"), on: c.get("on").and_then(Value::as_bool).unwrap_or(true) })
        })
        .collect()
}

/// Un nombre para el que no lo trae: el del archivo o el del sitio.
fn guess_name(url: &str) -> String {
    let tail = url.trim_end_matches('/').rsplit('/').next().unwrap_or(url);
    let tail = tail.trim_end_matches(".ics");
    if url.contains("://") && (tail.len() > 24 || tail == "basic") {
        url.split("://").nth(1).and_then(|r| r.split('/').next()).unwrap_or("Calendario").to_string()
    } else {
        tail.to_string()
    }
}

/// El hilo: los trae al arrancar, cada 15 minutos, cuando cambia la lista y
/// cuando se le pide (`{"calendars": "sync"}` por el socket).
pub fn watch(jobs: Receiver<Job>, tx: Sender<Event>) {
    // Que Veyl tenga dónde escribir la lista desde los ajustes.
    let _ = fs::create_dir_all(config::config());
    let mut last: Option<Instant> = None;
    let mut stamp = modified();
    loop {
        if last.is_none_or(|at| at.elapsed() >= EVERY) {
            let report = sync();
            if !report.is_empty() {
                eprintln!("veyld: calendarios: {report}");
            }
            last = Some(Instant::now());
        }
        match jobs.recv_timeout(GLANCE) {
            Ok(Job::Sync) => last = None,
            Ok(job) => {
                let told = work(job);
                if tx.send(Event::Calendar(told)).is_err() {
                    return;
                }
                last = None;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        let now = modified();
        if now != stamp {
            stamp = now;
            last = None;
        }
    }
}

/// Lo que se le pide al hilo de los calendarios.
pub enum Job {
    Sync,
    /// Apuntar en un calendario (por su nombre): día AAAA-MM-DD y hora HH:MM o nada.
    Add {
        calendar: String,
        title: String,
        day: String,
        time: String,
    },
    /// Borrar un evento (por su `id` en events.json).
    Remove {
        calendar: String,
        id: String,
    },
}

/// Hace lo pedido y dice cómo fue, para Veyl. Si no se pudo apuntar, avisa
/// (Veyl lo guarda en su agenda para no perderlo).
fn work(job: Job) -> Value {
    let find = |name: &str| sources().into_iter().find(|s| s.name == name && !s.google.is_empty());
    match job {
        Job::Sync => Value::Null,
        Job::Add { calendar, title, day, time } => {
            let done = (|| {
                let source = find(&calendar).ok_or(format!("{calendar} no es un calendario de tu cuenta de Google"))?;
                let date = NaiveDate::parse_from_str(&day, "%Y-%m-%d").map_err(|_| format!("día no válido: {day}"))?;
                let hour = match time.as_str() {
                    "" => None,
                    t => Some(NaiveTime::parse_from_str(t, "%H:%M").map_err(|_| format!("hora no válida: {t}"))?),
                };
                google::add(&source.google, &title, date, hour)
            })();
            match done {
                Ok(_) => json!({ "added": { "calendar": calendar, "title": title } }),
                Err(why) => {
                    eprintln!("veyld: no se pudo apuntar «{title}» en {calendar}: {why}");
                    crate::orders::detach(&[
                        "notify-send",
                        "-a",
                        "Veyl",
                        "-u",
                        "normal",
                        &format!("No se ha podido apuntar en {calendar}"),
                        &format!("«{title}» se queda en Veyl: {why}"),
                    ]);
                    json!({ "failed": { "calendar": calendar, "title": title, "day": day, "time": time, "error": why } })
                }
            }
        }
        Job::Remove { calendar, id } => {
            let done =
                find(&calendar).ok_or(format!("{calendar} no es un calendario de tu cuenta de Google")).and_then(|source| google::remove(&source.google, &id));
            match done {
                Ok(()) => json!({ "removed": { "calendar": calendar, "id": id } }),
                Err(why) => {
                    crate::orders::detach(&["notify-send", "-a", "Veyl", "-u", "normal", &format!("No se ha podido borrar en {calendar}"), &why]);
                    json!({ "unremoved": { "calendar": calendar, "id": id, "error": why } })
                }
            }
        }
    }
}

fn modified() -> Option<SystemTime> {
    fs::metadata(path()).and_then(|m| m.modified()).ok()
}

/// Trae todos y escribe lo traído. Devuelve los que fallaron, uno por línea.
pub fn sync() -> String {
    let sources = sources();
    let before: Value = fs::read_to_string(output()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
    let old_events = before.get("events").and_then(Value::as_array).cloned().unwrap_or_default();
    let old_status: HashMap<String, Value> = before
        .get("calendars")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|c| Some((c.get("name")?.as_str()?.to_string(), c.clone())))
        .collect();
    let today = Local::now().date_naive();
    let (from, to) = (today - chrono::Duration::days(BEHIND), today + chrono::Duration::days(AHEAD));
    let now = Local::now().timestamp_millis();

    // En cuáles de la cuenta se puede escribir.
    let writable: HashMap<String, bool> = if sources.iter().any(|s| s.on && !s.google.is_empty()) {
        google::calendars()
            .unwrap_or_default()
            .iter()
            .filter_map(|c| Some((c["id"].as_str()?.to_string(), c["writable"].as_bool().unwrap_or(false))))
            .chain(google::account().map(|_| ("primary".to_string(), true)))
            .collect()
    } else {
        HashMap::new()
    };
    let mut calendars = Vec::new();
    let mut events: Vec<Value> = Vec::new();
    let mut failed = Vec::new();
    for source in &sources {
        let google = !source.google.is_empty();
        let mut status = json!({ "name": source.name, "color": source.color, "on": source.on, "kind": if google { "google" } else { kind(&source.url) } });
        if google {
            status["writable"] = json!(writable.get(&source.google).copied().unwrap_or(false));
        }
        if !source.on {
            calendars.push(status);
            continue;
        }
        let found = if google { google::events(&source.google, from, to) } else { fetch(&source.url).map(|text| expand(&text, from, to)) };
        match found {
            Ok(found) => {
                status["count"] = json!(found.len());
                status["synced"] = json!(now);
                for mut event in found {
                    event["calendar"] = json!(source.name);
                    events.push(event);
                }
            }
            Err(why) => {
                // Lo de la última vez, si lo hay.
                let kept: Vec<Value> = old_events.iter().filter(|e| e.get("calendar").and_then(Value::as_str) == Some(source.name.as_str())).cloned().collect();
                status["count"] = json!(kept.len());
                if let Some(at) = old_status.get(&source.name).and_then(|s| s.get("synced")) {
                    status["synced"] = at.clone();
                }
                status["error"] = json!(why);
                failed.push(format!("{}: {why}", source.name));
                events.extend(kept);
            }
        }
        calendars.push(status);
    }
    events.sort_by(|a, b| {
        let key = |e: &Value| (e["day"].as_str().unwrap_or("").to_string(), e.get("time").and_then(Value::as_str).unwrap_or("").to_string());
        key(a).cmp(&key(b))
    });
    let out = json!({ "synced": now, "calendars": calendars, "events": events });
    let path = output();
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let partial = path.with_extension("json.part");
    let text = serde_json::to_string_pretty(&out).unwrap_or_default() + "\n";
    if let Err(e) = fs::write(&partial, text).and_then(|()| fs::rename(&partial, &path)) {
        failed.push(format!("{}: {e}", path.display()));
    }
    failed.join("\n")
}

fn kind(url: &str) -> &'static str {
    if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("webcal://") || url.starts_with("webcals://") { "web" } else { "file" }
}

/// El texto de un calendario: de la red, de un archivo o de una carpeta.
fn fetch(url: &str) -> Result<String, String> {
    if kind(url) == "web" {
        let url = match url.split_once("://") {
            Some(("webcal" | "webcals", rest)) => format!("https://{rest}"),
            _ => url.to_string(),
        };
        let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(30)).user_agent("veyld (calendario)").build();
        let response = agent.get(&url).call().map_err(|e| match e {
            ureq::Error::Status(404, _) => "no existe (404): ¿la dirección está bien copiada?".to_string(),
            ureq::Error::Status(401 | 403, _) => "no deja leerlo: ¿es la dirección secreta o pública?".to_string(),
            ureq::Error::Status(code, _) => format!("el servidor contesta {code}"),
            ureq::Error::Transport(t) => format!("sin conexión ({})", t.kind()),
        })?;
        let mut text = String::new();
        response.into_reader().take(LIMIT).read_to_string(&mut text).map_err(|e| format!("al leerlo: {e}"))?;
        if !text.contains("BEGIN:VCALENDAR") {
            return Err("eso no es un calendario iCalendar (.ics)".into());
        }
        return Ok(text);
    }
    let path = match url.strip_prefix("file://").unwrap_or(url) {
        "~" => config::home(),
        p => p.strip_prefix("~/").map(|rest| config::home().join(rest)).unwrap_or_else(|| PathBuf::from(p)),
    };
    if path.is_dir() {
        let mut text = String::new();
        read_dir(&path, 0, &mut text);
        if text.is_empty() {
            return Err(format!("{} no tiene ningún .ics", config::tilde(&path.to_string_lossy())));
        }
        return Ok(text);
    }
    fs::read_to_string(&path).map_err(|e| format!("{}: {e}", config::tilde(&path.to_string_lossy())))
}

/// Los `.ics` de una carpeta (vdirsyncer guarda uno por evento), y de sus
/// subcarpetas (una por calendario).
fn read_dir(dir: &Path, depth: u32, text: &mut String) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && depth < 2 && !entry.file_name().to_string_lossy().starts_with('.') {
            read_dir(&path, depth + 1, text);
        } else if path.extension().is_some_and(|e| e == "ics")
            && let Ok(more) = fs::read_to_string(&path)
        {
            text.push_str(&more);
            text.push('\n');
        }
    }
}

// --- iCalendar (RFC 5545) ----------------------------------------------------------

struct Prop {
    name: String,
    params: Vec<(String, String)>,
    value: String,
}

impl Prop {
    fn param(&self, key: &str) -> Option<&str> {
        self.params.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
}

/// Las líneas, ya desplegadas (una que empieza por espacio sigue a la de antes).
fn unfold(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for line in text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        match (line.strip_prefix(' ').or_else(|| line.strip_prefix('\t')), lines.last_mut()) {
            (Some(rest), Some(last)) => last.push_str(rest),
            _ => lines.push(line.to_string()),
        }
    }
    lines
}

/// «DTSTART;TZID=Europe/Madrid:20261120T163000»
fn prop(line: &str) -> Option<Prop> {
    let mut quoted = false;
    let colon = line.char_indices().find(|&(_, c)| {
        if c == '"' {
            quoted = !quoted;
        }
        c == ':' && !quoted
    })?;
    let (head, value) = (&line[..colon.0], &line[colon.0 + 1..]);
    let mut parts = head.split(';');
    let name = parts.next()?.trim().to_ascii_uppercase();
    let params = parts.filter_map(|p| p.split_once('=')).map(|(k, v)| (k.trim().to_ascii_uppercase(), v.trim().trim_matches('"').to_string())).collect();
    Some(Prop { name, params, value: value.to_string() })
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n' | 'N') => out.push(' '),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// En qué zona va una hora: la de un TZID, UTC («Z») o flotante (la de aquí).
#[derive(Clone, Copy, Debug)]
enum Zone {
    Here,
    Utc,
    Named(Tz),
}

impl Zone {
    /// Una hora de esa zona, en la de aquí.
    fn here(self, at: NaiveDateTime) -> NaiveDateTime {
        match self {
            Zone::Here => at,
            Zone::Utc => Utc.from_utc_datetime(&at).with_timezone(&Local).naive_local(),
            Zone::Named(tz) => match tz.from_local_datetime(&at).earliest() {
                Some(t) => t.with_timezone(&Local).naive_local(),
                // La hora que se salta al cambiar al horario de verano.
                None => tz.from_local_datetime(&(at + chrono::Duration::hours(1))).earliest().map(|t| t.with_timezone(&Local).naive_local()).unwrap_or(at),
            },
        }
    }
}

/// Un momento: su hora en su zona y si es de todo el día.
#[derive(Clone, Copy, Debug)]
struct Stamp {
    at: NaiveDateTime,
    zone: Zone,
    all_day: bool,
}

fn zone_of(tzid: Option<&str>) -> Zone {
    let Some(tzid) = tzid.map(|t| t.trim_start_matches('/')) else {
        return Zone::Here;
    };
    if let Ok(tz) = Tz::from_str(tzid) {
        return Zone::Named(tz);
    }
    // Outlook usa los nombres de Windows; los más corrientes.
    let windows = [
        ("Romance Standard Time", "Europe/Madrid"),
        ("W. Europe Standard Time", "Europe/Berlin"),
        ("Central Europe Standard Time", "Europe/Budapest"),
        ("GMT Standard Time", "Europe/London"),
        ("UTC", "UTC"),
        ("Eastern Standard Time", "America/New_York"),
        ("Central Standard Time", "America/Chicago"),
        ("Pacific Standard Time", "America/Los_Angeles"),
        ("Mountain Standard Time", "America/Denver"),
        ("SA Pacific Standard Time", "America/Bogota"),
        ("Central Standard Time (Mexico)", "America/Mexico_City"),
        ("Argentina Standard Time", "America/Argentina/Buenos_Aires"),
    ];
    windows.iter().find(|(w, _)| *w == tzid).and_then(|(_, iana)| Tz::from_str(iana).ok()).map(Zone::Named).unwrap_or(Zone::Here)
}

fn parse_stamp(value: &str, tzid: Option<&str>, date_only: bool) -> Option<Stamp> {
    let value = value.trim();
    if date_only || value.len() == 8 {
        let date = NaiveDate::parse_from_str(value.get(..8)?, "%Y%m%d").ok()?;
        return Some(Stamp { at: date.and_time(NaiveTime::MIN), zone: Zone::Here, all_day: true });
    }
    let (body, utc) = match value.strip_suffix('Z') {
        Some(body) => (body, true),
        None => (value, false),
    };
    let at = NaiveDateTime::parse_from_str(body, "%Y%m%dT%H%M%S").or_else(|_| NaiveDateTime::parse_from_str(body, "%Y%m%dT%H%M")).ok()?;
    Some(Stamp { at, zone: if utc { Zone::Utc } else { zone_of(tzid) }, all_day: false })
}

/// Un DTSTART, DTEND, EXDATE…: puede llevar varios separados por comas.
fn stamps(p: &Prop) -> Vec<Stamp> {
    let date_only = p.param("VALUE").is_some_and(|v| v.eq_ignore_ascii_case("DATE"));
    p.value.split(',').filter_map(|v| parse_stamp(v, p.param("TZID"), date_only)).collect()
}

/// «PT1H30M», «P1D»
fn duration(text: &str) -> Option<chrono::Duration> {
    let (negative, text) = match text.trim().strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.trim().trim_start_matches('+')),
    };
    let text = text.strip_prefix('P')?;
    let mut total = chrono::Duration::zero();
    let mut number = String::new();
    for c in text.chars() {
        match c {
            '0'..='9' => number.push(c),
            'T' => {}
            unit => {
                let n: i64 = number.parse().ok()?;
                number.clear();
                total += match unit {
                    'W' => chrono::Duration::weeks(n),
                    'D' => chrono::Duration::days(n),
                    'H' => chrono::Duration::hours(n),
                    'M' => chrono::Duration::minutes(n),
                    'S' => chrono::Duration::seconds(n),
                    _ => return None,
                };
            }
        }
    }
    Some(if negative { -total } else { total })
}

#[derive(Default)]
struct Vevent {
    uid: String,
    title: String,
    place: String,
    start: Option<Stamp>,
    end: Option<Stamp>,
    length: Option<chrono::Duration>,
    rule: Option<String>,
    extra: Vec<Stamp>,
    except: Vec<Stamp>,
    instead_of: Option<Stamp>,
    cancelled: bool,
}

fn events(text: &str) -> Vec<Vevent> {
    let mut out = Vec::new();
    let mut current: Option<Vevent> = None;
    // Lo que va dentro de un evento (sus alarmas) no es del evento.
    let mut inner = 0;
    for line in unfold(text) {
        let Some(p) = prop(&line) else {
            continue;
        };
        match (p.name.as_str(), p.value.trim().to_ascii_uppercase().as_str()) {
            ("BEGIN", "VEVENT") if current.is_none() => {
                current = Some(Vevent::default());
                inner = 0;
                continue;
            }
            ("END", "VEVENT") if inner == 0 => {
                out.extend(current.take());
                continue;
            }
            ("BEGIN", _) if current.is_some() => inner += 1,
            ("END", _) if current.is_some() => inner -= 1,
            _ => {}
        }
        let Some(e) = current.as_mut().filter(|_| inner == 0) else {
            continue;
        };
        match p.name.as_str() {
            "UID" => e.uid = p.value.trim().to_string(),
            "SUMMARY" => e.title = unescape(&p.value),
            "LOCATION" => e.place = unescape(&p.value),
            "DTSTART" => e.start = stamps(&p).into_iter().next(),
            "DTEND" => e.end = stamps(&p).into_iter().next(),
            "DURATION" => e.length = duration(&p.value),
            "RRULE" => e.rule = Some(p.value.trim().to_string()),
            "RDATE" => e.extra.extend(stamps(&p)),
            "EXDATE" => e.except.extend(stamps(&p)),
            "RECURRENCE-ID" => e.instead_of = stamps(&p).into_iter().next(),
            "STATUS" => e.cancelled = p.value.trim().eq_ignore_ascii_case("CANCELLED"),
            _ => {}
        }
    }
    out
}

#[derive(Clone, Copy, PartialEq)]
enum Freq {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

struct Rule {
    freq: Freq,
    interval: i64,
    count: Option<u32>,
    until: Option<Stamp>,
    by_day: Vec<(i32, Weekday)>,
    by_month_day: Vec<i32>,
    by_month: Vec<u32>,
    week_start: Weekday,
}

fn weekday(text: &str) -> Option<Weekday> {
    Some(match text {
        "MO" => Weekday::Mon,
        "TU" => Weekday::Tue,
        "WE" => Weekday::Wed,
        "TH" => Weekday::Thu,
        "FR" => Weekday::Fri,
        "SA" => Weekday::Sat,
        "SU" => Weekday::Sun,
        _ => return None,
    })
}

/// «FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE;UNTIL=20270115T000000Z». Lo que no se
/// sabe desplegar (por horas o minutos) no se despliega.
fn rule(text: &str) -> Option<Rule> {
    let mut r = Rule {
        freq: Freq::Daily,
        interval: 1,
        count: None,
        until: None,
        by_day: Vec::new(),
        by_month_day: Vec::new(),
        by_month: Vec::new(),
        week_start: Weekday::Mon,
    };
    let mut freq = None;
    for part in text.split(';') {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        let value = value.trim().to_ascii_uppercase();
        match key.trim().to_ascii_uppercase().as_str() {
            "FREQ" => {
                freq = Some(match value.as_str() {
                    "DAILY" => Freq::Daily,
                    "WEEKLY" => Freq::Weekly,
                    "MONTHLY" => Freq::Monthly,
                    "YEARLY" => Freq::Yearly,
                    _ => return None,
                })
            }
            "INTERVAL" => r.interval = value.parse::<i64>().ok().filter(|&n| n > 0).unwrap_or(1),
            "COUNT" => r.count = value.parse().ok(),
            "UNTIL" => r.until = parse_stamp(&value, None, false),
            "BYDAY" => {
                r.by_day = value
                    .split(',')
                    .filter_map(|d| {
                        let split = d.len().checked_sub(2)?;
                        let ordinal = if split == 0 { 0 } else { d[..split].parse().ok()? };
                        Some((ordinal, weekday(&d[split..])?))
                    })
                    .collect()
            }
            "BYMONTHDAY" => r.by_month_day = value.split(',').filter_map(|d| d.parse().ok()).collect(),
            "BYMONTH" => r.by_month = value.split(',').filter_map(|d| d.parse().ok()).collect(),
            "WKST" => r.week_start = weekday(&value).unwrap_or(Weekday::Mon),
            _ => {}
        }
    }
    r.freq = freq?;
    Some(r)
}

fn days_in_month(date: NaiveDate) -> u32 {
    let (y, m) = if date.month() == 12 { (date.year() + 1, 1) } else { (date.year(), date.month() + 1) };
    NaiveDate::from_ymd_opt(y, m, 1).map(|next| next.pred_opt().map_or(31, |d| d.day())).unwrap_or(31)
}

impl Rule {
    /// Si un día va en la serie que empieza en `start`.
    fn matches(&self, start: NaiveDate, d: NaiveDate) -> bool {
        if !self.by_month.is_empty() && !self.by_month.contains(&d.month()) {
            return false;
        }
        let months = (d.year() - start.year()) as i64 * 12 + d.month() as i64 - start.month() as i64;
        let in_month = |d: NaiveDate| {
            let day_ok = |ord: i32, wd: Weekday| {
                if d.weekday() != wd {
                    return false;
                }
                match ord {
                    0 => true,
                    n if n > 0 => (d.day() as i32 - 1) / 7 + 1 == n,
                    n => -((days_in_month(d) as i32 - d.day() as i32) / 7 + 1) == n,
                }
            };
            let month_day_ok = |n: i32| if n > 0 { d.day() as i32 == n } else { days_in_month(d) as i32 + 1 + n == d.day() as i32 };
            match (self.by_day.is_empty(), self.by_month_day.is_empty()) {
                (true, true) => d.day() == start.day(),
                (false, true) => self.by_day.iter().any(|&(o, w)| day_ok(o, w)),
                (true, false) => self.by_month_day.iter().any(|&n| month_day_ok(n)),
                (false, false) => self.by_day.iter().any(|&(o, w)| day_ok(o, w)) && self.by_month_day.iter().any(|&n| month_day_ok(n)),
            }
        };
        match self.freq {
            Freq::Daily => {
                (d - start).num_days() % self.interval == 0
                    && (self.by_day.is_empty() || self.by_day.iter().any(|&(_, w)| w == d.weekday()))
                    && (self.by_month_day.is_empty() || self.by_month_day.contains(&(d.day() as i32)))
            }
            Freq::Weekly => {
                let week = |x: NaiveDate| x - chrono::Duration::days(x.weekday().days_since(self.week_start) as i64);
                let weeks = (week(d) - week(start)).num_days() / 7;
                weeks % self.interval == 0
                    && if self.by_day.is_empty() { d.weekday() == start.weekday() } else { self.by_day.iter().any(|&(_, w)| w == d.weekday()) }
            }
            Freq::Monthly => months % self.interval == 0 && in_month(d),
            Freq::Yearly => {
                (d.year() - start.year()) as i64 % self.interval == 0
                    && (!self.by_month.is_empty()
                        || d.month() == start.month()
                        || (!self.by_day.is_empty() && self.by_month_day.is_empty() && self.by_day.iter().all(|&(o, _)| o == 0)))
                    && in_month(d)
            }
        }
    }
}

/// Los eventos de un calendario entre dos días, uno por vez que pasan:
/// `{ title, day, time?, end?, where? }`, con la hora de aquí.
pub fn expand(text: &str, from: NaiveDate, to: NaiveDate) -> Vec<Value> {
    let all = events(text);
    // Las veces cambiadas de una serie (llevan RECURRENCE-ID): la original no sale.
    let moved: HashSet<(String, NaiveDateTime)> = all.iter().filter_map(|e| Some((e.uid.clone(), here_of(e.instead_of?)))).collect();
    let mut out: Vec<Value> = Vec::new();
    let mut seen = HashSet::new();
    for e in &all {
        let title = if e.title.is_empty() { "(sin título)".to_string() } else { e.title.clone() };
        let Some(start) = e.start else {
            continue;
        };
        if e.cancelled {
            continue;
        }
        // Lo que dura, en su zona.
        let length = match (e.end, e.length) {
            (Some(end), _) if end.all_day == start.all_day => end.at - start.at,
            (_, Some(length)) => length,
            _ if start.all_day => chrono::Duration::days(1),
            _ => chrono::Duration::zero(),
        };
        let except: HashSet<NaiveDateTime> = e.except.iter().map(|&s| here_of(s)).collect();
        let mut times: Vec<NaiveDateTime> = Vec::new();
        match e.rule.as_deref().filter(|_| e.instead_of.is_none()).and_then(rule) {
            Some(r) => {
                let first = start.at.date();
                let until = r.until.map(|u| if u.all_day { u.at.date() } else { u.zone.here(u.at).date() });
                let last = until.map_or(to, |u| u.min(to));
                // Sin COUNT no hace falta contar desde el principio.
                let mut day = if r.count.is_none() { first.max(from - chrono::Duration::days(length.num_days() + 1)) } else { first };
                let mut count = 0u32;
                while day <= last {
                    if day == first || r.matches(first, day) {
                        let at = day.and_time(start.at.time());
                        if let Some(u) = r.until.filter(|u| !u.all_day && !start.all_day)
                            && start.zone.here(at) > u.zone.here(u.at)
                        {
                            break;
                        }
                        count += 1;
                        if r.count.is_some_and(|c| count > c) {
                            break;
                        }
                        times.push(at);
                    }
                    let Some(next) = day.succ_opt() else {
                        break;
                    };
                    day = next;
                }
            }
            None => times.push(start.at),
        }
        times.extend(e.extra.iter().map(|s| s.at));
        for at in times {
            let stamp = Stamp { at, ..start };
            let here = here_of(stamp);
            if except.contains(&here) || (e.instead_of.is_none() && moved.contains(&(e.uid.clone(), here))) {
                continue;
            }
            if start.all_day {
                // Uno por día que dura (DTEND no cuenta), hasta dos meses.
                let days = length.num_days().clamp(1, 62);
                for n in 0..days {
                    let day = at.date() + chrono::Duration::days(n);
                    if day >= from && day <= to && seen.insert((e.uid.clone(), day.and_time(NaiveTime::MIN), e.title.clone())) {
                        let mut event = json!({ "title": title, "day": day.format("%Y-%m-%d").to_string() });
                        if !e.place.is_empty() {
                            event["where"] = json!(e.place);
                        }
                        out.push(event);
                    }
                }
                continue;
            }
            let day = here.date();
            if day < from || day > to || !seen.insert((e.uid.clone(), here, e.title.clone())) {
                continue;
            }
            let mut event = json!({ "title": title, "day": day.format("%Y-%m-%d").to_string(), "time": here.format("%H:%M").to_string() });
            if length > chrono::Duration::zero() {
                let end = start.zone.here(at + length);
                event["end"] = json!(if end.date() == day { end.format("%H:%M").to_string() } else { end.format("%Y-%m-%d %H:%M").to_string() });
            }
            if !e.place.is_empty() {
                event["where"] = json!(e.place);
            }
            out.push(event);
        }
    }
    out.sort_by_key(|e| (e["day"].as_str().unwrap_or("").to_string(), e.get("time").and_then(Value::as_str).unwrap_or("").to_string()));
    out
}

fn here_of(s: Stamp) -> NaiveDateTime {
    if s.all_day { s.at } else { s.zone.here(s.at) }
}

/// Lo que hay apuntado en Veyl y en los calendarios (`{ title, day, time?,
/// calendar? }`), para el cerebro y para «¿qué tengo mañana?».
pub fn agenda() -> Vec<Value> {
    let read = |path: PathBuf| fs::read_to_string(path).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).unwrap_or(Value::Null);
    let mut all: Vec<Value> = read(config::state().join("agenda.json")).as_array().cloned().unwrap_or_default();
    all.extend(read(output()).get("events").and_then(Value::as_array).cloned().unwrap_or_default());
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    fn days(events: &[Value]) -> Vec<String> {
        events.iter().map(|e| format!("{} {}", e["day"].as_str().unwrap(), e.get("time").and_then(Value::as_str).unwrap_or("--"))).collect()
    }

    const CAL: &str = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\n";

    #[test]
    fn weekly_series_with_exception_and_moved_instance() {
        let text = format!(
            "{CAL}BEGIN:VEVENT\r\nUID:clase\r\nSUMMARY:Cálculo\\, aula 3\r\nDTSTART:20261102T090000\r\nDTEND:20261102T110000\r\nRRULE:FREQ=WEEKLY;BYDAY=MO,WE;COUNT=5\r\nEXDATE:20261104T090000\r\nBEGIN:VALARM\r\nSUMMARY:no\r\nEND:VALARM\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:clase\r\nRECURRENCE-ID:20261109T090000\r\nSUMMARY:Cálculo (cambiada)\r\nDTSTART:20261109T120000\r\nDTEND:20261109T130000\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
        );
        let found = expand(&text, day("2026-10-01"), day("2026-12-31"));
        assert_eq!(days(&found), ["2026-11-02 09:00", "2026-11-09 12:00", "2026-11-11 09:00", "2026-11-16 09:00"]);
        assert_eq!(found[0]["title"], "Cálculo, aula 3");
        assert_eq!(found[0]["end"], "11:00");
        assert_eq!(found[1]["title"], "Cálculo (cambiada)");
    }

    #[test]
    fn all_day_events_span_their_days() {
        let text = format!(
            "{CAL}BEGIN:VEVENT\r\nUID:v\r\nSUMMARY:Vacaciones\r\nDTSTART;VALUE=DATE:20261223\r\nDTEND;VALUE=DATE:20261226\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
        );
        assert_eq!(days(&expand(&text, day("2026-12-01"), day("2026-12-31"))), ["2026-12-23 --", "2026-12-24 --", "2026-12-25 --"]);
    }

    #[test]
    fn monthly_by_ordinal_weekday_and_yearly_birthdays() {
        let text = format!(
            "{CAL}BEGIN:VEVENT\r\nUID:m\r\nSUMMARY:Reunión\r\nDTSTART:20260101T180000\r\nRRULE:FREQ=MONTHLY;BYDAY=-1FR;UNTIL=20260430T000000\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:b\r\nSUMMARY:Cumple\r\nDTSTART;VALUE=DATE:19900315\r\nRRULE:FREQ=YEARLY\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
        );
        let found = expand(&text, day("2026-01-01"), day("2027-12-31"));
        assert_eq!(
            days(&found),
            ["2026-01-01 18:00", "2026-01-30 18:00", "2026-02-27 18:00", "2026-03-15 --", "2026-03-27 18:00", "2026-04-24 18:00", "2027-03-15 --"]
        );
    }

    #[test]
    fn times_in_another_zone_are_converted() {
        let text = format!("{CAL}BEGIN:VEVENT\r\nUID:u\r\nSUMMARY:Llamada\r\nDTSTART:20260710T120000Z\r\nDURATION:PT30M\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n");
        let found = expand(&text, day("2026-07-01"), day("2026-07-31"));
        let expected = Utc.with_ymd_and_hms(2026, 7, 10, 12, 0, 0).unwrap().with_timezone(&Local).format("%H:%M").to_string();
        assert_eq!(found[0]["time"], json!(expected));
    }

    #[test]
    fn folded_lines_and_cancelled_events() {
        let text = format!(
            "{CAL}BEGIN:VEVENT\r\nUID:f\r\nSUMMARY:Una línea\r\n  muy larga\r\nDTSTART;VALUE=DATE:20260105\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nUID:c\r\nSUMMARY:Anulado\r\nSTATUS:CANCELLED\r\nDTSTART;VALUE=DATE:20260106\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
        );
        let found = expand(&text, day("2026-01-01"), day("2026-01-31"));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0]["title"], "Una línea muy larga");
    }
}
