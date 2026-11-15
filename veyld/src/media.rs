//! Lo que suena: los reproductores MPRIS del bus de sesión, mirados cada
//! pocos segundos. Solo cuenta lo que está sonando.

use std::collections::HashMap;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use zbus::blocking::Connection;
use zbus::blocking::fdo::DBusProxy;
use zbus::zvariant::OwnedValue;

use crate::daemon::Event;

const PREFIX: &str = "org.mpris.MediaPlayer2.";
const PATH: &str = "/org/mpris/MediaPlayer2";
const EVERY: Duration = Duration::from_secs(3);

/// Una pista que suena.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Playing {
    /// Su nombre en el bus: dos que suenan a la vez son dos filas.
    pub bus: String,
    /// El reproductor: «Spotify», «Mozilla Firefox».
    pub player: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub url: String,
}

fn all(conn: &Connection, name: &str, interface: &str) -> Option<HashMap<String, OwnedValue>> {
    let reply = conn.call_method(Some(name), PATH, Some("org.freedesktop.DBus.Properties"), "GetAll", &(interface,)).ok()?;
    reply.body().deserialize().ok()
}

fn text(value: Option<&OwnedValue>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    if let Ok(text) = value.downcast_ref::<&str>() {
        return text.trim().to_string();
    }
    // Los artistas van en una lista.
    value.try_clone().ok().and_then(|v| Vec::<String>::try_from(v).ok()).map(|list| list.join(", ").trim().to_string()).unwrap_or_default()
}

fn read(conn: &Connection, name: &str, players: &mut HashMap<String, String>) -> Option<Playing> {
    let props = all(conn, name, "org.mpris.MediaPlayer2.Player")?;
    if text(props.get("PlaybackStatus")) != "Playing" {
        return None;
    }
    let meta: HashMap<String, OwnedValue> = props.get("Metadata").and_then(|m| m.try_clone().ok()).and_then(|m| HashMap::try_from(m).ok())?;
    let title = text(meta.get("xesam:title"));
    if title.is_empty() {
        return None;
    }
    let player = players
        .entry(name.to_string())
        .or_insert_with(|| {
            let identity = all(conn, name, "org.mpris.MediaPlayer2").map(|p| text(p.get("Identity"))).unwrap_or_default();
            if identity.is_empty() { name.trim_start_matches(PREFIX).split('.').next().unwrap_or(name).to_string() } else { identity }
        })
        .clone();
    Some(Playing {
        bus: name.to_string(),
        player,
        title,
        artist: text(meta.get("xesam:artist")),
        album: text(meta.get("xesam:album")),
        url: crate::web::clean(&text(meta.get("xesam:url"))),
    })
}

/// Manda lo que suena cada vez que cambia. Vuelve solo si no hay bus.
pub fn watch(tx: Sender<Event>) -> String {
    let conn = match Connection::session() {
        Ok(conn) => conn,
        Err(e) => return e.to_string(),
    };
    let dbus = match DBusProxy::new(&conn) {
        Ok(dbus) => dbus,
        Err(e) => return e.to_string(),
    };
    let mut players: HashMap<String, String> = HashMap::new();
    let mut last: Vec<Playing> = Vec::new();
    loop {
        let names: Vec<String> = match dbus.list_names() {
            Ok(names) => names.into_iter().map(|n| n.to_string()).filter(|n| n.starts_with(PREFIX) && !n.contains("playerctld")).collect(),
            Err(e) => return e.to_string(),
        };
        players.retain(|name, _| names.contains(name));
        let mut playing: Vec<Playing> = names.iter().filter_map(|name| read(&conn, name, &mut players)).collect();
        playing.sort_by(|a, b| a.bus.cmp(&b.bus));
        if playing != last {
            if tx.send(Event::Media(playing.clone())).is_err() {
                return String::new();
            }
            last = playing;
        }
        thread::sleep(EVERY);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::Value;

    struct Player;

    #[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
    impl Player {
        #[zbus(property)]
        fn playback_status(&self) -> String {
            "Playing".into()
        }

        #[zbus(property)]
        fn metadata(&self) -> HashMap<String, OwnedValue> {
            HashMap::from([
                ("xesam:title".to_string(), OwnedValue::try_from(Value::from("Canción")).unwrap()),
                ("xesam:artist".to_string(), OwnedValue::try_from(Value::from(vec!["Grupo", "Otro"])).unwrap()),
                ("xesam:url".to_string(), OwnedValue::try_from(Value::from("https://x.example/a?token=1")).unwrap()),
            ])
        }
    }

    struct Root;

    #[zbus::interface(name = "org.mpris.MediaPlayer2")]
    impl Root {
        #[zbus(property)]
        fn identity(&self) -> String {
            "Prueba".into()
        }
    }

    /// Con un bus propio: `dbus-run-session -- cargo test -- --ignored`.
    #[test]
    #[ignore = "necesita un bus de sesión"]
    fn reads_a_player() {
        let name = "org.mpris.MediaPlayer2.prueba";
        let _server = zbus::blocking::connection::Builder::session()
            .unwrap()
            .name(name)
            .unwrap()
            .serve_at(PATH, Player)
            .unwrap()
            .serve_at(PATH, Root)
            .unwrap()
            .build()
            .unwrap();
        let conn = Connection::session().unwrap();
        let got = read(&conn, name, &mut HashMap::new()).unwrap();
        assert_eq!(
            got,
            Playing {
                bus: name.into(),
                player: "Prueba".into(),
                title: "Canción".into(),
                artist: "Grupo, Otro".into(),
                album: String::new(),
                url: "https://x.example/a".into()
            }
        );
    }
}
