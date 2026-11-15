//! Lo que se mira en el navegador: la dirección de cada pestaña, de la sesión
//! que Firefox (y los que son como él) guarda cada 15 s en
//! `sessionstore-backups/recovery.jsonlz4`. De una ventana solo se sabe su
//! título, así que la página es la de ese título; las ventanas privadas no van
//! a ese archivo.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use serde::Deserialize;

use crate::config;

/// Los navegadores que se saben leer y dónde tienen sus perfiles.
const BROWSERS: &[(&str, &[&str])] = &[
    ("firefox", &[".config/mozilla/firefox", ".mozilla/firefox"]),
    ("librewolf", &[".config/librewolf/librewolf", ".librewolf"]),
    ("zen", &[".config/zen", ".zen"]),
    ("floorp", &[".floorp"]),
];

/// Lo que no se guarda de una dirección: lo que puede abrir una sesión.
const SECRET: &[&str] = &["token", "key", "code", "auth", "session", "sid", "sig", "pass", "secret", "otp", "nonce", "state", "ticket", "jwt", "credential"];

/// Cuál de los navegadores que se saben leer es esta clase de ventana.
pub fn browser(app: &str) -> Option<&'static str> {
    let app = app.to_lowercase();
    if app.contains("firefox") {
        Some("firefox")
    } else if app.contains("librewolf") {
        Some("librewolf")
    } else if app == "zen" || app.starts_with("zen-") || app.contains("zen_browser") || app.contains("zen-browser") {
        Some("zen")
    } else if app.contains("floorp") {
        Some("floorp")
    } else {
        None
    }
}

/// El título de la página, sin el nombre del navegador: «Rust — Mozilla Firefox».
fn page(title: &str) -> Option<&str> {
    let brand = |tail: &str| {
        let tail = tail.to_lowercase();
        ["firefox", "librewolf", "zen", "floorp"].iter().any(|b| tail.contains(b))
    };
    let head = [" — ", " – ", " - "].iter().find_map(|sep| title.rsplit_once(sep).filter(|(_, tail)| brand(tail)).map(|(head, _)| head));
    match head {
        Some(head) if !head.trim().is_empty() => Some(head.trim()),
        Some(_) => None,
        None if brand(title) => None,
        None => Some(title.trim()).filter(|t| !t.is_empty()),
    }
}

/// La dirección sin lo de después de `#` ni lo que parezca una clave.
pub fn clean(url: &str) -> String {
    if url.starts_with("data:") || url.starts_with("blob:") {
        return String::new();
    }
    let url = url.split('#').next().unwrap_or(url);
    let url = match url.split_once('?') {
        Some((base, query)) => {
            let kept: Vec<&str> = query
                .split('&')
                .filter(|pair| {
                    let key = pair.split('=').next().unwrap_or("").to_lowercase();
                    !key.is_empty() && !SECRET.iter().any(|s| key.contains(s))
                })
                .collect();
            if kept.is_empty() { base.to_string() } else { format!("{base}?{}", kept.join("&")) }
        }
        None => url.to_string(),
    };
    if url.chars().count() > 500 { url.chars().take(500).collect() } else { url }
}

/// El sitio de una dirección web, sin «www.»: «github.com».
pub fn site(url: &str) -> String {
    let Some(rest) = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")) else {
        return String::new();
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit('@').next().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host).to_lowercase();
    host.strip_prefix("www.").map(str::to_string).unwrap_or(host)
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Session {
    windows: Vec<Win>,
    #[serde(rename = "_closedWindows")]
    closed: Vec<Win>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Win {
    tabs: Vec<Tab>,
    selected: usize,
    #[serde(rename = "_closedTabs")]
    closed: Vec<Closed>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Closed {
    state: Tab,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Tab {
    entries: Vec<Entry>,
    /// La entrada que enseña, desde 1.
    index: usize,
    #[serde(rename = "lastAccessed")]
    last: i64,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Entry {
    url: String,
    title: String,
}

/// De cada título, la dirección más probable: antes la de la pestaña elegida
/// de una ventana, luego la de otra pestaña abierta, luego la de una pestaña
/// o una ventana ya cerradas, y por último lo que quedó atrás en una pestaña.
type Pages = HashMap<String, ((u8, i64), String)>;

fn pages(session: &Session) -> Pages {
    let mut out: Pages = HashMap::new();
    for win in &session.windows {
        for (i, tab) in win.tabs.iter().enumerate() {
            index(&mut out, tab, if i + 1 == win.selected { 4 } else { 3 });
        }
    }
    for win in session.windows.iter().chain(&session.closed) {
        for closed in &win.closed {
            index(&mut out, &closed.state, 2);
        }
    }
    for win in &session.closed {
        for tab in &win.tabs {
            index(&mut out, tab, 2);
        }
    }
    out
}

/// Las páginas de una pestaña: la que enseña, con `current`; las de atrás y
/// adelante, con lo mínimo.
fn index(out: &mut Pages, tab: &Tab, current: u8) {
    let shown = if tab.index == 0 { tab.entries.len() } else { tab.index };
    for (i, entry) in tab.entries.iter().enumerate() {
        let title = entry.title.trim();
        if title.is_empty() || entry.url.is_empty() {
            continue;
        }
        let rank = (if i + 1 == shown { current } else { 1 }, tab.last);
        if out.get(title).is_none_or(|(best, _)| rank > *best) {
            out.insert(title.to_string(), (rank, entry.url.clone()));
        }
    }
}

/// Lee `recovery.jsonlz4`: «mozLz40\0», el tamaño y un bloque LZ4.
fn read(path: &PathBuf) -> Option<Session> {
    let bytes = fs::read(path).ok()?;
    let body = bytes.strip_prefix(b"mozLz40\0")?;
    let json = lz4_flex::block::decompress_size_prepended(body).ok()?;
    serde_json::from_slice(&json).ok()
}

/// La sesión más reciente entre los perfiles de un navegador.
fn newest(kind: &str) -> Option<(PathBuf, SystemTime)> {
    let home = config::home();
    let dirs = BROWSERS.iter().find(|(k, _)| *k == kind)?.1;
    let mut best: Option<(PathBuf, SystemTime)> = None;
    for dir in dirs {
        let Ok(profiles) = fs::read_dir(home.join(dir)) else {
            continue;
        };
        for profile in profiles.flatten() {
            let path = profile.path().join("sessionstore-backups/recovery.jsonlz4");
            if let Ok(modified) = fs::metadata(&path).and_then(|m| m.modified())
                && best.as_ref().is_none_or(|(_, b)| modified > *b)
            {
                best = Some((path, modified));
            }
        }
    }
    best
}

/// Lo leído de cada navegador, que se vuelve a leer solo si su sesión cambia.
#[derive(Default)]
pub struct Web {
    read: HashMap<&'static str, (PathBuf, SystemTime, Pages)>,
}

impl Web {
    /// La dirección de lo que enseña una ventana del navegador `app` con este título.
    pub fn url(&mut self, app: &str, title: &str) -> Option<String> {
        let kind = browser(app)?;
        let page = page(title)?;
        let newest = newest(kind)?;
        let stale = self.read.get(kind).is_none_or(|(path, modified, _)| *path != newest.0 || *modified != newest.1);
        if stale {
            let pages = read(&newest.0).map(|s| pages(&s)).unwrap_or_default();
            self.read.insert(kind, (newest.0, newest.1, pages));
        }
        let (_, url) = self.read.get(kind)?.2.get(page)?;
        Some(clean(url)).filter(|u| !u.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_and_urls() {
        assert_eq!(page("Rust by Example — Mozilla Firefox"), Some("Rust by Example"));
        assert_eq!(page("Mozilla Firefox"), None);
        assert_eq!(page("Sin marca"), Some("Sin marca"));
        assert_eq!(site("https://www.youtube.com/watch?v=9Wk9b105YPk"), "youtube.com");
        assert_eq!(site("http://user@localhost:8080/x"), "localhost");
        assert_eq!(site("about:blank"), "");
        assert_eq!(clean("https://x.com/a?v=1&access_token=s3cr3t#top"), "https://x.com/a?v=1");
        assert_eq!(clean("https://x.com/callback?code=abc&state=xyz"), "https://x.com/callback");
        assert_eq!(browser("firefox"), Some("firefox"));
        assert_eq!(browser("zenity"), None);
    }

    #[test]
    fn the_shown_tab_wins() {
        let session: Session = serde_json::from_str(
            r#"{"windows": [{"selected": 2, "tabs": [
                {"index": 1, "lastAccessed": 5, "entries": [{"url": "https://old.example/", "title": "Inicio"}]},
                {"index": 2, "lastAccessed": 1, "entries": [{"url": "https://a.example/", "title": "A"}, {"url": "https://new.example/", "title": "Inicio"}]}
            ], "_closedTabs": [{"state": {"index": 1, "entries": [{"url": "https://gone.example/", "title": "Cerrada"}]}}]}],
            "_closedWindows": [{"tabs": [{"index": 1, "entries": [{"url": "https://w.example/", "title": "De otra ventana"}]}]}]}"#,
        )
        .unwrap();
        let pages = pages(&session);
        assert_eq!(pages["Inicio"].1, "https://new.example/");
        assert_eq!(pages["A"].1, "https://a.example/");
        assert_eq!(pages["Cerrada"].1, "https://gone.example/");
        assert_eq!(pages["De otra ventana"].1, "https://w.example/");
    }
}
