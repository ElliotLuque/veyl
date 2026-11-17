//! La cuenta de Google: para leer y escribir en sus calendarios (el enlace
//! iCal solo deja leer, y con horas de retraso).
//!
//! El cliente OAuth es el de Veyl, verificado por Google, y va dentro del
//! programa: se fija al compilar con `VEYL_GOOGLE_CLIENT_ID` y
//! `VEYL_GOOGLE_CLIENT_SECRET` (en una aplicación de escritorio el secreto no
//! es secreto: lo que protege es PKCE). Sin él (una compilación propia), el
//! usuario puede usar uno suyo de Google Cloud. Al conectar, veyld abre el navegador,
//! espera la vuelta en 127.0.0.1 (con PKCE) y guarda el token que no caduca
//! en `~/.config/veyl/keys.json`, solo para ti.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Local, NaiveDate, TimeZone};
use serde_json::{Value, json};

use crate::brain;

const AUTH: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN: &str = "https://oauth2.googleapis.com/token";
const API: &str = "https://www.googleapis.com/calendar/v3";
/// Lo justo: ver la lista de calendarios y leer y escribir sus eventos (no
/// crear ni borrar calendarios, ni compartirlos). Son permisos «sensibles»:
/// piden verificación, no la auditoría de los «restringidos».
const SCOPE: &str = "https://www.googleapis.com/auth/calendar.calendarlist.readonly https://www.googleapis.com/auth/calendar.events";
/// El cliente de Veyl, si se compiló con él.
const BUILT_IN: Option<(&str, &str)> = match (option_env!("VEYL_GOOGLE_CLIENT_ID"), option_env!("VEYL_GOOGLE_CLIENT_SECRET")) {
    (Some(id), Some(secret)) => Some((id, secret)),
    (Some(id), None) => Some((id, "")),
    _ => None,
};
/// Lo que se espera a que se dé permiso en el navegador.
const WAIT: Duration = Duration::from_secs(300);

/// El token de acceso, que dura una hora, y cuándo caduca.
static ACCESS: Mutex<Option<(String, Instant)>> = Mutex::new(None);

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(Duration::from_secs(30)).build()
}

/// Si hay una cuenta conectada: su correo.
pub fn account() -> Option<String> {
    brain::stored("google_refresh")?;
    Some(brain::stored("google_account").unwrap_or_default())
}

/// El cliente OAuth guardado (ID, secreto).
/// El cliente OAuth: el propio del usuario, si puso uno, o el de Veyl.
pub fn client() -> Option<(String, String)> {
    match brain::stored("google_client_id") {
        Some(id) => Some((id, brain::stored("google_client_secret").unwrap_or_default())),
        None => BUILT_IN.filter(|(id, _)| !id.is_empty()).map(|(id, secret)| (id.to_string(), secret.to_string())),
    }
}

/// Cómo está la cuenta, para los ajustes: `{ account, client, calendars?, error? }`.
pub fn status() -> Value {
    let mut out = json!({ "account": account(), "client": client().is_some(), "builtin": BUILT_IN.is_some() });
    if account().is_some() {
        match calendars() {
            Ok(list) => out["calendars"] = json!(list),
            Err(why) => out["error"] = json!(why),
        }
    }
    out
}

pub fn logout() {
    for key in ["google_refresh", "google_account"] {
        let _ = brain::forget_key(key);
    }
    *ACCESS.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

fn random(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    let _ = ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

/// Para meter algo en una dirección.
pub fn escape(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn unescape(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = |b: u8| (b as char).to_digit(16);
                match (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                    (Some(high), Some(low)) => out.push((high * 16 + low) as u8),
                    _ => out.extend_from_slice(&bytes[i..i + 3]),
                }
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Conecta la cuenta: abre el navegador y espera el permiso. Devuelve el
/// correo de la cuenta. Sin cliente, el de Veyl.
pub fn login(client_id: &str, client_secret: &str) -> Result<String, String> {
    let (client_id, client_secret) = match (client_id.trim(), client_secret.trim()) {
        ("", _) => {
            // Con el de Veyl a mano, el propio que hubiera ya no se usa.
            if BUILT_IN.is_some() {
                let _ = brain::forget_key("google_client_id");
                let _ = brain::forget_key("google_client_secret");
            }
            client().ok_or("esta compilación de Veyl no trae cliente de Google: pon uno tuyo")?
        }
        (id, secret) => {
            brain::save_key("google_client_id", id)?;
            brain::save_key("google_client_secret", secret)?;
            (id.to_string(), secret.to_string())
        }
    };
    let (client_id, client_secret) = (client_id.as_str(), client_secret.as_str());
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("no se puede esperar la vuelta: {e}"))?;
    let redirect = format!("http://127.0.0.1:{}", listener.local_addr().map_err(|e| e.to_string())?.port());
    let verifier = random(48);
    let challenge = URL_SAFE_NO_PAD.encode(ring::digest::digest(&ring::digest::SHA256, verifier.as_bytes()));
    let state = random(16);
    let url = format!(
        "{AUTH}?client_id={}&redirect_uri={}&response_type=code&scope={}&code_challenge={challenge}&code_challenge_method=S256&access_type=offline&prompt=consent&state={state}",
        escape(client_id),
        escape(&redirect),
        escape(SCOPE)
    );
    Command::new("xdg-open").arg(&url).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|e| format!("xdg-open: {e}"))?;

    // La vuelta del navegador: «GET /?state=…&code=… HTTP/1.1».
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let start = Instant::now();
    let code = loop {
        if start.elapsed() > WAIT {
            return Err("no se dio permiso a tiempo (5 minutos)".into());
        }
        let mut stream = match listener.accept() {
            Ok((stream, _)) => stream,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(200));
                continue;
            }
            Err(e) => return Err(e.to_string()),
        };
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let mut line = String::new();
        let _ = BufReader::new(&stream).read_line(&mut line);
        let query = line.split_whitespace().nth(1).and_then(|path| path.split_once('?')).map(|(_, q)| q.to_string()).unwrap_or_default();
        let param = |key: &str| query.split('&').filter_map(|p| p.split_once('=')).find(|(k, _)| *k == key).map(|(_, v)| unescape(v));
        let (code, error) = (param("code"), param("error"));
        if code.is_none() && error.is_none() {
            // El navegador pide otra cosa (el icono): no es la vuelta.
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            continue;
        }
        let ok = code.is_some() && param("state").as_deref() == Some(state.as_str());
        let page = if ok {
            "Listo: Veyl ya puede usar tu calendario. Puedes cerrar esta pestaña."
        } else {
            "No se ha conectado. Puedes cerrar esta pestaña y volver a intentarlo."
        };
        let body = format!("<!doctype html><meta charset=utf-8><title>Veyl</title><body style=\"font:16px sans-serif;margin:3em\">{page}</body>");
        let _ = stream.write_all(
            format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len())
                .as_bytes(),
        );
        match (code, error) {
            (Some(code), _) if ok => break code,
            (_, Some(error)) if error == "access_denied" => return Err("no se dio permiso".into()),
            (_, Some(error)) => return Err(format!("Google dice: {error}")),
            _ => return Err("la vuelta no era la esperada".into()),
        }
    };

    let reply: Value = agent()
        .post(TOKEN)
        .send_form(&[
            ("code", code.as_str()),
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("redirect_uri", redirect.as_str()),
            ("grant_type", "authorization_code"),
            ("code_verifier", verifier.as_str()),
        ])
        .map_err(failure)?
        .into_json()
        .map_err(|e| e.to_string())?;
    let refresh = reply["refresh_token"].as_str().ok_or("Google no ha dado un token que dure")?;
    brain::save_key("google_refresh", refresh)?;
    if let (Some(access), Some(expires)) = (reply["access_token"].as_str(), reply["expires_in"].as_u64()) {
        *ACCESS.lock().unwrap_or_else(|e| e.into_inner()) = Some((access.to_string(), Instant::now() + Duration::from_secs(expires.saturating_sub(60))));
    }
    // El correo de la cuenta es el ID de su calendario principal.
    let email = calendars()?.iter().find(|c| c["primary"].as_bool() == Some(true)).and_then(|c| c["id"].as_str()).unwrap_or("").to_string();
    brain::save_key("google_account", &email)?;
    Ok(email)
}

/// Lo que dice Google cuando algo falla, en cristiano.
fn failure(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, response) => {
            let body: Value = response.into_json().unwrap_or(Value::Null);
            let said =
                body["error"]["message"].as_str().or_else(|| body["error_description"].as_str()).or_else(|| body["error"].as_str()).unwrap_or("").to_string();
            match code {
                400 if said.contains("invalid_grant") || said.contains("expired or revoked") => "el permiso ha caducado: vuelve a conectar la cuenta".into(),
                401 => "el permiso ya no vale: vuelve a conectar la cuenta".into(),
                403 if said.contains("has not been used") || said.contains("disabled") => {
                    "la API de Google Calendar no está activada en tu proyecto de Google Cloud".into()
                }
                403 => format!("Google no deja hacerlo: {said}"),
                404 => "ese calendario no existe o no es tuyo".into(),
                _ if said.is_empty() => format!("Google contesta {code}"),
                _ => format!("Google contesta {code}: {said}"),
            }
        }
        ureq::Error::Transport(t) => format!("sin conexión ({})", t.kind()),
    }
}

/// Un token de acceso válido: el guardado o uno nuevo.
fn access() -> Result<String, String> {
    if let Some((token, until)) = ACCESS.lock().unwrap_or_else(|e| e.into_inner()).as_ref()
        && Instant::now() < *until
    {
        return Ok(token.clone());
    }
    let refresh = brain::stored("google_refresh").ok_or("no hay ninguna cuenta de Google conectada")?;
    let (id, secret) = client().ok_or("falta el cliente OAuth de Google")?;
    let reply: Value = agent()
        .post(TOKEN)
        .send_form(&[("refresh_token", refresh.as_str()), ("client_id", id.as_str()), ("client_secret", secret.as_str()), ("grant_type", "refresh_token")])
        .map_err(failure)?
        .into_json()
        .map_err(|e| e.to_string())?;
    let token = reply["access_token"].as_str().ok_or("Google no ha dado acceso")?.to_string();
    let expires = reply["expires_in"].as_u64().unwrap_or(3600);
    *ACCESS.lock().unwrap_or_else(|e| e.into_inner()) = Some((token.clone(), Instant::now() + Duration::from_secs(expires.saturating_sub(60))));
    Ok(token)
}

fn get(url: &str) -> Result<Value, String> {
    agent().get(url).set("Authorization", &format!("Bearer {}", access()?)).call().map_err(failure)?.into_json().map_err(|e| e.to_string())
}

/// Los calendarios de la cuenta: `{ id, summary, color, writable, primary }`.
pub fn calendars() -> Result<Vec<Value>, String> {
    let mut out = Vec::new();
    let mut page = String::new();
    loop {
        let reply = get(&format!("{API}/users/me/calendarList?maxResults=250{page}"))?;
        for c in reply["items"].as_array().into_iter().flatten() {
            let role = c["accessRole"].as_str().unwrap_or("");
            if role == "freeBusyReader" {
                continue;
            }
            out.push(json!({
                "id": c["id"],
                "summary": c["summaryOverride"].as_str().or_else(|| c["summary"].as_str()).unwrap_or(""),
                "color": c["backgroundColor"],
                "writable": role == "owner" || role == "writer",
                "primary": c["primary"].as_bool().unwrap_or(false),
            }));
        }
        match reply["nextPageToken"].as_str() {
            Some(token) => page = format!("&pageToken={}", escape(token)),
            None => break,
        }
    }
    Ok(out)
}

/// Los eventos de un calendario entre dos días, como los de iCal
/// (`{ title, day, time?, end?, where? }`) y con su `id`.
pub fn events(calendar: &str, from: NaiveDate, to: NaiveDate) -> Result<Vec<Value>, String> {
    let stamp = |d: NaiveDate| Local.from_local_datetime(&d.and_hms_opt(0, 0, 0).unwrap_or_default()).earliest().map(|t| t.to_rfc3339()).unwrap_or_default();
    let base = format!(
        "{API}/calendars/{}/events?singleEvents=true&orderBy=startTime&maxResults=2500&timeMin={}&timeMax={}",
        escape(calendar),
        escape(&stamp(from)),
        escape(&stamp(to + chrono::Duration::days(1)))
    );
    let mut out = Vec::new();
    let mut page = String::new();
    loop {
        let reply = get(&format!("{base}{page}"))?;
        for e in reply["items"].as_array().into_iter().flatten() {
            if e["status"].as_str() == Some("cancelled") {
                continue;
            }
            out.extend(event(e, from, to));
        }
        match reply["nextPageToken"].as_str() {
            Some(token) => page = format!("&pageToken={}", escape(token)),
            None => break,
        }
    }
    Ok(out)
}

/// Un evento de la API, en una o varias entradas (uno de todo el día que dura
/// varios, una por día).
fn event(e: &Value, from: NaiveDate, to: NaiveDate) -> Vec<Value> {
    let title = e["summary"].as_str().filter(|t| !t.trim().is_empty()).unwrap_or("(sin título)");
    let id = e["id"].as_str().unwrap_or("");
    let place = e["location"].as_str().unwrap_or("");
    let with_place = |mut v: Value| {
        if !place.is_empty() {
            v["where"] = json!(place);
        }
        v["id"] = json!(id);
        v
    };
    if let Some(day) = e["start"]["date"].as_str().and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()) {
        let end = e["end"]["date"].as_str().and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()).unwrap_or(day + chrono::Duration::days(1));
        let days = (end - day).num_days().clamp(1, 62);
        return (0..days)
            .map(|n| day + chrono::Duration::days(n))
            .filter(|d| *d >= from && *d <= to)
            .map(|d| with_place(json!({ "title": title, "day": d.format("%Y-%m-%d").to_string() })))
            .collect();
    }
    let parse = |v: &Value| v.as_str().and_then(|t| DateTime::parse_from_rfc3339(t).ok()).map(|t| t.with_timezone(&Local).naive_local());
    let Some(start) = parse(&e["start"]["dateTime"]) else {
        return Vec::new();
    };
    if start.date() < from || start.date() > to {
        return Vec::new();
    }
    let mut v = json!({ "title": title, "day": start.format("%Y-%m-%d").to_string(), "time": start.format("%H:%M").to_string() });
    if let Some(end) = parse(&e["end"]["dateTime"]).filter(|end| *end > start) {
        v["end"] = json!(if end.date() == start.date() { end.format("%H:%M").to_string() } else { end.format("%Y-%m-%d %H:%M").to_string() });
    }
    vec![with_place(v)]
}

/// Apunta un evento: de todo el día (sin hora) o de una hora.
pub fn add(calendar: &str, title: &str, day: NaiveDate, time: Option<chrono::NaiveTime>) -> Result<String, String> {
    let body = match time {
        None => json!({
            "summary": title,
            "start": { "date": day.format("%Y-%m-%d").to_string() },
            "end": { "date": (day + chrono::Duration::days(1)).format("%Y-%m-%d").to_string() },
        }),
        Some(time) => {
            let start = Local.from_local_datetime(&day.and_time(time)).earliest().ok_or("esa hora no existe (cambio de hora)")?;
            let end = start + chrono::Duration::hours(1);
            json!({ "summary": title, "start": { "dateTime": start.to_rfc3339() }, "end": { "dateTime": end.to_rfc3339() } })
        }
    };
    let reply: Value = agent()
        .post(&format!("{API}/calendars/{}/events", escape(calendar)))
        .set("Authorization", &format!("Bearer {}", access()?))
        .send_json(body)
        .map_err(failure)?
        .into_json()
        .map_err(|e| e.to_string())?;
    Ok(reply["id"].as_str().unwrap_or("").to_string())
}

pub fn remove(calendar: &str, id: &str) -> Result<(), String> {
    match agent().delete(&format!("{API}/calendars/{}/events/{}", escape(calendar), escape(id))).set("Authorization", &format!("Bearer {}", access()?)).call() {
        // Ya no estaba: da igual.
        Ok(_) | Err(ureq::Error::Status(404 | 410, _)) => Ok(()),
        Err(e) => Err(failure(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_calendar_ids() {
        assert_eq!(escape("es.spain#holiday@group.v.calendar.google.com"), "es.spain%23holiday%40group.v.calendar.google.com");
        assert_eq!(unescape("4%2F0Ab_x-y+z"), "4/0Ab_x-y z");
        assert_eq!(unescape("50%"), "50%");
    }

    #[test]
    fn api_events_become_agenda_entries() {
        let (from, to) = (NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(), NaiveDate::from_ymd_opt(2026, 12, 31).unwrap());
        let whole = event(&json!({ "id": "a", "summary": "Puente", "start": { "date": "2026-12-07" }, "end": { "date": "2026-12-09" } }), from, to);
        assert_eq!(whole.iter().map(|e| e["day"].as_str().unwrap()).collect::<Vec<_>>(), ["2026-12-07", "2026-12-08"]);
        assert_eq!(whole[0]["id"], "a");
        let timed = event(
            &json!({ "id": "b", "summary": "Clase", "location": "Aula 3", "start": { "dateTime": "2026-11-23T13:00:00Z" }, "end": { "dateTime": "2026-11-23T14:30:00Z" } }),
            from,
            to,
        );
        let start = Local
            .with_ymd_and_hms(2026, 11, 23, 0, 0, 0)
            .unwrap()
            .timezone()
            .from_utc_datetime(&NaiveDate::from_ymd_opt(2026, 11, 23).unwrap().and_hms_opt(13, 0, 0).unwrap());
        assert_eq!(timed[0]["time"], json!(start.format("%H:%M").to_string()));
        assert_eq!(timed[0]["where"], "Aula 3");
        assert!(timed[0]["end"].as_str().is_some());
    }
}
