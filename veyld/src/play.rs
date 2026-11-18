//! Buscar y poner: «busca recetas de lentejas», «busca gatos en YouTube»,
//! «pon Bohemian Rhapsody en Spotify», «abre Spotify y pon Estopa».
//!
//! Buscar es abrir la página de resultados del buscador que toca. Poner algo
//! en YouTube es abrir su primer vídeo (sale de la página de resultados, sin
//! clave). En Spotify, sin su API, la canción, el disco o el artista salen de
//! preguntar a su API, si le has dado una clave (`veyld spotify login`: una
//! app gratuita de developer.spotify.com, sin entrar con tu cuenta), o de
//! buscar `site:open.spotify.com …` en Brave o en DuckDuckGo, que se cansan
//! pronto; si no sale nada, se abre la búsqueda en Spotify.

use std::process::{Command, ExitCode, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use crate::words::fold;

const AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0";

/// Un texto para una URL.
pub fn encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.trim().bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Lo que se busca, sin lo que lo envuelve: «el vídeo de gatos» → «gatos».
pub fn tidy(query: &str) -> String {
    const LEADS: &[&str] = &[
        "el video de ",
        "un video de ",
        "videos de ",
        "la cancion de ",
        "la cancion ",
        "el disco de ",
        "el disco ",
        "el album de ",
        "el album ",
        "la playlist de ",
        "la playlist ",
        "la lista de ",
        "musica de ",
        "canciones de ",
        "algo de ",
        "a ",
    ];
    let mut q = query.trim().to_string();
    loop {
        let folded = fold(&q);
        match LEADS.iter().find(|l| folded.starts_with(*l) && q.len() > l.len()) {
            // Las tildes no cambian cuántas palabras hay: se cortan tantas como tenga la entrada.
            Some(lead) => q = q.split_whitespace().skip(lead.split_whitespace().count()).collect::<Vec<_>>().join(" "),
            None => break,
        }
    }
    q
}

/// La página de resultados de un buscador; `None` si no se conoce.
pub fn search_url(engine: &str, query: &str) -> Option<String> {
    let q = encode(query);
    Some(match engine {
        "" | "google" => format!("https://www.google.com/search?q={q}"),
        "images" => format!("https://www.google.com/search?tbm=isch&q={q}"),
        "youtube" => format!("https://www.youtube.com/results?search_query={q}"),
        "wikipedia" => format!("https://es.wikipedia.org/w/index.php?search={q}"),
        "amazon" => format!("https://www.amazon.es/s?k={q}"),
        "github" => format!("https://github.com/search?q={q}"),
        "maps" => format!("https://www.google.com/maps/search/{q}"),
        "reddit" => format!("https://www.reddit.com/search/?q={q}"),
        _ => return None,
    })
}

/// Cómo se llama cada buscador al decirlo.
pub fn engine_name(engine: &str) -> &'static str {
    match engine {
        "images" => "imágenes",
        "youtube" => "YouTube",
        "wikipedia" => "la Wikipedia",
        "amazon" => "Amazon",
        "github" => "GitHub",
        "maps" => "Google Maps",
        "reddit" => "Reddit",
        "spotify" => "Spotify",
        _ => "Google",
    }
}

/// Las webs que se conocen por su nombre: (cómo se dice, cómo se escribe junto, dirección).
const SITES: &[(&str, &str, &str)] = &[
    ("YouTube", "youtube", "https://www.youtube.com"),
    ("Google", "google", "https://www.google.com"),
    ("Gmail", "gmail", "https://mail.google.com"),
    ("Google Drive", "googledrive drive", "https://drive.google.com"),
    ("Google Calendar", "googlecalendar", "https://calendar.google.com"),
    ("Google Maps", "googlemaps maps", "https://www.google.com/maps"),
    ("la Wikipedia", "wikipedia lawikipedia", "https://es.wikipedia.org"),
    ("GitHub", "github", "https://github.com"),
    ("Twitch", "twitch", "https://www.twitch.tv"),
    ("Prime Video", "primevideo amazonprimevideo amazonprime prime", "https://www.primevideo.com"),
    ("Netflix", "netflix", "https://www.netflix.com"),
    ("Disney+", "disneyplus disney disneyplus+ disney+", "https://www.disneyplus.com"),
    ("Max", "hbomax hbo max", "https://www.max.com"),
    ("Movistar Plus+", "movistarplus movistar", "https://ver.movistarplus.es"),
    ("Filmin", "filmin", "https://www.filmin.es"),
    ("Crunchyroll", "crunchyroll", "https://www.crunchyroll.com"),
    ("Amazon", "amazon", "https://www.amazon.es"),
    ("X", "twitter x", "https://x.com"),
    ("Instagram", "instagram", "https://www.instagram.com"),
    ("Facebook", "facebook", "https://www.facebook.com"),
    ("TikTok", "tiktok", "https://www.tiktok.com"),
    ("Reddit", "reddit", "https://www.reddit.com"),
    ("LinkedIn", "linkedin", "https://www.linkedin.com"),
    ("WhatsApp Web", "whatsapp whatsappweb", "https://web.whatsapp.com"),
    ("Telegram Web", "telegram telegramweb", "https://web.telegram.org"),
    ("ChatGPT", "chatgpt", "https://chatgpt.com"),
    ("Claude", "claude claudeai", "https://claude.ai"),
    ("Outlook", "outlook hotmail", "https://outlook.live.com"),
];

/// Una web por su nombre («Prime Video», «la web de Netflix») o su
/// dirección («primevideo.com»): (cómo se llama, dirección).
pub fn site(name: &str) -> Option<(String, String)> {
    let folded = fold(name.trim());
    let bare = folded.trim_start_matches("la web de ").trim_start_matches("la pagina de ").trim_start_matches("la web ").trim();
    let joined: String = bare.split_whitespace().collect();
    if let Some((label, _, url)) = SITES.iter().find(|(_, keys, _)| keys.split(' ').any(|k| k == joined)) {
        return Some((label.to_string(), url.to_string()));
    }
    // Una dirección: «primevideo.com», «www.marca.com», «https://…».
    let address = name.trim().trim_start_matches("https://").trim_start_matches("http://");
    let host = address.split('/').next().unwrap_or("");
    let tld = host.rsplit('.').next().unwrap_or("");
    if !address.contains(char::is_whitespace) && host.contains('.') && tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic()) {
        return Some((host.trim_start_matches("www.").to_string(), format!("https://{address}")));
    }
    None
}

/// Una web que no se conoce: la primera que da DuckDuckGo («\\», «voy a tener suerte»).
pub fn lucky(name: &str) -> (String, String) {
    (name.trim().to_string(), format!("https://duckduckgo.com/?q=%5C{}", encode(name)))
}

/// Lo que se habla con «pon»: Spotify, YouTube o un navegador.
#[derive(Clone, Debug, PartialEq)]
pub enum Player {
    Spotify,
    YouTube,
}

/// Si lo de antes («abre Spotify», «abre YouTube») dice dónde poner algo.
pub fn player(name: &str) -> Option<Player> {
    let n = fold(name);
    if n.contains("spotify") {
        Some(Player::Spotify)
    } else if n.contains("youtube") {
        Some(Player::YouTube)
    } else {
        None
    }
}

/// Los navegadores que se reconocen por su nombre.
pub fn is_browser(name: &str) -> bool {
    let n = fold(name);
    ["firefox", "chrome", "chromium", "brave", "zen", "librewolf", "vivaldi", "opera", "edge", "navegador", "epiphany", "qutebrowser", "floorp"]
        .iter()
        .any(|b| n.contains(b))
}

fn get(url: &str) -> Option<String> {
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(4)).build();
    let response = agent.get(url).set("User-Agent", AGENT).set("Accept-Language", "es-ES,es;q=0.9").call().ok()?;
    if response.status() != 200 {
        return None;
    }
    response.into_string().ok()
}

/// El primer vídeo de una página de resultados de YouTube: (id, título).
pub fn first_video(html: &str) -> Option<(String, String)> {
    let at = html.find("\"videoRenderer\":{\"videoId\":\"")?;
    let rest = &html[at + 28..];
    let id: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').collect();
    if id.len() != 11 {
        return None;
    }
    let title = rest
        .find("\"title\":{\"runs\":[{\"text\":\"")
        .map(|t| &rest[t + 26..])
        .and_then(|t| serde_json::from_str::<String>(&format!("\"{}\"", t.split("\"}").next()?)).ok())
        .unwrap_or_default();
    Some((id, title))
}

/// El primer vídeo de YouTube para lo buscado.
pub fn youtube(query: &str) -> Option<(String, String)> {
    first_video(&get(&search_url("youtube", query)?)?)
}

/// La primera canción, disco, lista o artista de Spotify de una página de resultados.
pub fn first_spotify(html: &str) -> Option<String> {
    let mut best: Option<(usize, String)> = None;
    for kind in ["track", "album", "playlist", "artist"] {
        let needle = format!("open.spotify.com/{kind}/");
        let mut from = 0;
        while let Some(at) = html[from..].find(&needle) {
            let start = from + at + needle.len();
            let id: String = html[start..].chars().take_while(char::is_ascii_alphanumeric).collect();
            if id.len() == 22 {
                if best.as_ref().is_none_or(|(b, _)| start < *b) {
                    best = Some((start, format!("spotify:{kind}:{id}")));
                }
                break;
            }
            from = start;
        }
    }
    best.map(|(_, uri)| uri)
}

/// El pase de la API de Spotify (dura una hora), guardado mientras vale.
static TOKEN: Mutex<Option<(String, Instant)>> = Mutex::new(None);

fn spotify_token() -> Option<String> {
    let mut saved = TOKEN.lock().ok()?;
    if let Some((token, until)) = saved.as_ref()
        && Instant::now() < *until
    {
        return Some(token.clone());
    }
    let (id, secret) = (crate::brain::stored("spotify_client_id")?, crate::brain::stored("spotify_client_secret")?);
    let basic = STANDARD.encode(format!("{id}:{secret}"));
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(4)).build();
    let reply: serde_json::Value = agent
        .post("https://accounts.spotify.com/api/token")
        .set("Authorization", &format!("Basic {basic}"))
        .send_form(&[("grant_type", "client_credentials")])
        .ok()?
        .into_json()
        .ok()?;
    let token = reply.get("access_token")?.as_str()?.to_string();
    let lasts = reply.get("expires_in").and_then(serde_json::Value::as_u64).unwrap_or(3600).saturating_sub(60);
    *saved = Some((token.clone(), Instant::now() + Duration::from_secs(lasts)));
    Some(token)
}

/// De lo que contesta la búsqueda de la API, lo primero: la canción, si la
/// hay; si no, el disco, el artista o la lista.
pub fn first_found(reply: &serde_json::Value) -> Option<String> {
    ["tracks", "albums", "artists", "playlists"]
        .iter()
        .find_map(|kind| reply.get(kind)?.get("items")?.as_array()?.iter().find_map(|item| item.get("uri")?.as_str().map(str::to_string)))
}

/// Lo de Spotify que mejor casa con lo buscado, por su API.
fn spotify_api(query: &str) -> Option<String> {
    let token = spotify_token()?;
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(4)).build();
    let reply: serde_json::Value = agent
        .get("https://api.spotify.com/v1/search")
        .query("q", query)
        .query("type", "track,artist,album,playlist")
        .query("limit", "1")
        .set("Authorization", &format!("Bearer {token}"))
        .call()
        .ok()?
        .into_json()
        .ok()?;
    first_found(&reply)
}

/// Guarda la clave de la API de Spotify (la de una app de developer.spotify.com).
pub fn login() -> ExitCode {
    eprintln!(
        "En https://developer.spotify.com/dashboard crea una app (gratis; de «Redirect URI» vale http://127.0.0.1) y pega aquí su Client ID y su \
         Client secret. Se guardan en ~/.config/veyl/keys.json, solo para ti; Veyl solo busca con ella, no entra en tu cuenta."
    );
    let ask = |what: &str| {
        eprint!("{what}: ");
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).ok().map(|_| line.trim().to_string()).filter(|l| !l.is_empty())
    };
    let (Some(id), Some(secret)) = (ask("Client ID"), ask("Client secret")) else {
        eprintln!("sin clave");
        return ExitCode::FAILURE;
    };
    let saved = crate::brain::save_key("spotify_client_id", &id).and_then(|_| crate::brain::save_key("spotify_client_secret", &secret));
    if let Err(why) = saved {
        eprintln!("{why}");
        return ExitCode::FAILURE;
    }
    match spotify_token() {
        Some(_) => {
            eprintln!("Hecho: Spotify contesta.");
            ExitCode::SUCCESS
        }
        None => {
            eprintln!("Guardada, pero Spotify no la acepta: revisa el ID y el secreto.");
            ExitCode::FAILURE
        }
    }
}

/// Lo de Spotify que mejor casa con lo buscado, como URI: por la API si hay
/// clave; si no, por un buscador.
pub fn spotify(query: &str) -> Option<String> {
    if let Some(uri) = spotify_api(query) {
        return Some(uri);
    }
    let q = encode(&format!("site:open.spotify.com {query}"));
    [format!("https://search.brave.com/search?q={q}"), format!("https://lite.duckduckgo.com/lite/?q={q}")].iter().find_map(|url| first_spotify(&get(url)?))
}

/// El título de algo de Spotify (por su oEmbed público).
pub fn spotify_title(uri: &str) -> Option<String> {
    let mut parts = uri.split(':').skip(1);
    let (kind, id) = (parts.next()?, parts.next()?);
    let body = get(&format!("https://open.spotify.com/oembed?url=https://open.spotify.com/{kind}/{id}"))?;
    let value: serde_json::Value = serde_json::from_str(&body).ok()?;
    value.get("title")?.as_str().map(str::to_string)
}

/// Si Spotify está en marcha (se le puede hablar por MPRIS).
pub fn spotify_running() -> bool {
    Command::new("playerctl")
        .arg("-l")
        .stderr(Stdio::null())
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).lines().any(|l| l.starts_with("spotify")))
}

/// Que Spotify ponga una URI («spotify:track:…», «spotify:search:…»).
pub fn spotify_open(uri: &str) -> Result<(), String> {
    let out = Command::new("playerctl").args(["-p", "spotify", "open", uri]).stderr(Stdio::piped()).output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    // Un disco o un artista a veces se abre sin sonar: si no suena, que suene.
    if !uri.starts_with("spotify:search:") {
        std::thread::sleep(Duration::from_millis(800));
        let status = Command::new("playerctl").args(["-p", "spotify", "status"]).stderr(Stdio::null()).output();
        if status.is_ok_and(|s| !String::from_utf8_lossy(&s.stdout).starts_with("Playing")) {
            let _ = Command::new("playerctl").args(["-p", "spotify", "play"]).stderr(Stdio::null()).status();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries_and_urls() {
        assert_eq!(encode("gatos & perros ñ"), "gatos+%26+perros+%C3%B1");
        assert_eq!(tidy("el vídeo de gatos graciosos"), "gatos graciosos");
        assert_eq!(tidy("música de Estopa"), "Estopa");
        assert_eq!(tidy("a Estopa"), "Estopa");
        assert_eq!(tidy("Bohemian Rhapsody"), "Bohemian Rhapsody");
        assert_eq!(search_url("youtube", "lo-fi").unwrap(), "https://www.youtube.com/results?search_query=lo-fi");
        assert!(search_url("altavista", "x").is_none());
        assert_eq!(player("Spotify"), Some(Player::Spotify));
        assert!(is_browser("firefox") && !is_browser("kitty"));
        assert_eq!(site("YouTube").unwrap().1, "https://www.youtube.com");
        assert_eq!(site("Prime Video").unwrap(), ("Prime Video".to_string(), "https://www.primevideo.com".to_string()));
        assert_eq!(site("primevideo").unwrap().0, "Prime Video");
        assert_eq!(site("la web de Netflix").unwrap().0, "Netflix");
        assert_eq!(site("marca.com").unwrap(), ("marca.com".to_string(), "https://marca.com".to_string()));
        assert_eq!(site("www.elpais.com/espana").unwrap().1, "https://www.elpais.com/espana");
        assert!(site("bohemian rhapsody").is_none());
        assert!(site("versión 2.0").is_none());
        assert_eq!(lucky("el corte inglés").1, "https://duckduckgo.com/?q=%5Cel+corte+ingl%C3%A9s");
    }

    #[test]
    fn results_pages_are_read() {
        let yt = r#"..."videoRenderer":{"videoId":"fJ9rUzIMcZQ","thumbnail":{},"title":{"runs":[{"text":"Queen – Bohemian Rhapsody & más"}]..."#;
        assert_eq!(first_video(yt), Some(("fJ9rUzIMcZQ".to_string(), "Queen – Bohemian Rhapsody & más".to_string())));
        assert_eq!(first_video("nada"), None);
        let page = r#"<a href="https://open.spotify.com/artist/5ZqnEfVdEGmoPxtELhN7ai">x</a> <a href="https://open.spotify.com/track/3z8h0TU7ReDPLIbEnYhWZb">"#;
        assert_eq!(first_spotify(page).unwrap(), "spotify:artist:5ZqnEfVdEGmoPxtELhN7ai");
        assert_eq!(first_spotify("open.spotify.com/track/corto"), None);
        let api = serde_json::json!({ "tracks": { "items": [] }, "artists": { "items": [{ "uri": "spotify:artist:5ZqnEfVdEGmoPxtELhN7ai" }] } });
        assert_eq!(first_found(&api).unwrap(), "spotify:artist:5ZqnEfVdEGmoPxtELhN7ai");
    }

    /// Con red: `cargo test -- --ignored online`.
    #[test]
    #[ignore]
    fn online() {
        let (id, title) = youtube("bohemian rhapsody queen").expect("YouTube da un vídeo");
        println!("YouTube: {id} «{title}»");
        // Sin clave, los buscadores se cansan: puede no salir nada.
        match spotify("bohemian rhapsody queen") {
            Some(uri) => println!("Spotify: {uri} «{}»", spotify_title(&uri).unwrap_or_default()),
            None => println!("Spotify: nada (sin clave y con los buscadores cansados)"),
        }
    }
}
