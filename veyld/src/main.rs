//! veyld · el daemon de Veyl.
//!
//! Lleva el diario de lo que miras (qué ventana tenía el foco, con su
//! título, su carpeta o su página; lo que sonó, los archivos; en SQLite),
//! encuentra los agentes que corren en la máquina y entiende las órdenes: lo
//! que cumple la gramática lo hace él, sin preguntar a ningún modelo. Y es el
//! servidor de notificaciones: las recibe, las guarda y decide cuándo salen;
//! Veyl solo las pinta.

mod agents;
mod apps;
mod brain;
mod bus;
mod calendars;
mod card;
mod config;
mod daemon;
mod google;
mod grammar;
mod hypr;
mod idle;
mod journal;
mod media;
mod nix;
mod notes;
mod orders;
mod play;
mod process;
mod procs;
mod recent;
mod system;
mod tts;
mod usage;
mod views;
mod vocab;
mod voice;
mod web;
mod when;
mod words;

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use serde_json::{Value, json};

const USAGE: &str = "uso:
  veyld                      arranca el daemon (si ya hay uno en esta sesión, sale)
  veyld say <texto…>         lo dice como una orden: lo hace si la gramática lo entiende
  veyld parse <texto…>       qué entiende la gramática, sin hacerlo
  veyld recall [--full|--json] [<cuándo…>]
                             qué estuviste mirando (sin nada, hoy); --full, todo con
                             detalle; --json, todo en datos
  veyld timeline [<cuándo…>] lo mismo, rato a rato
  veyld find <texto…>        cuándo salió algo en el diario
  veyld hear <audio.wav>     lo que se oye en un WAV, y qué entiende la gramática
  veyld model [get <nombre>] los modelos de voz que hay; get trae uno (y el de Silero)
  veyld mic                  los micros que hay y cuál se usa
  veyld brain [use <proveedor> [<modelo>] | login <anthropic|openai> | off]
                             quién contesta lo que la gramática no entiende: anthropic
                             (clave o ant auth login), openai (clave), claude-code (tu
                             suscripción de Claude) o codex (la de ChatGPT)
  veyld mcp                  las herramientas de Veyl por MCP (para Claude Code y Codex)
  veyld spotify login        la clave de la API de Spotify, para que «pon …» acierte siempre
  veyld speak [--wav <f>] <texto…>
                             lo dice con la voz de Veyl (o lo guarda en un WAV)
  veyld agents               los agentes en marcha
  veyld calendars            trae ya los calendarios de ~/.config/veyl/calendars.json y
                             dice cómo ha ido
  veyld google [login [<id> <secreto>] | logout]
                             la cuenta de Google y sus calendarios; login la conecta (abre
                             el navegador), con el cliente de Veyl o con uno tuyo
  veyld grammar              comprueba la gramática (la de fábrica y ~/.config/veyl/grammar.txt)
  veyld nix [status | write <cambios> | show <ajustes>]
                             Veyl en tu configuración de Nix: dónde está programs.veyl,
                             escribir en ella lo cambiado en los ajustes (JSON: [{path, value}])
                             o cómo se escribirían unos ajustes";

/// Pregunta al daemon y espera su respuesta.
fn ask(mut message: Value) -> Result<Value, String> {
    let path = config::socket();
    let mut socket = UnixStream::connect(&path).map_err(|_| "veyld no está en marcha".to_string())?;
    message["id"] = json!(1);
    socket.set_read_timeout(Some(Duration::from_secs(150))).ok();
    writeln!(socket, "{message}").map_err(|e| e.to_string())?;
    for line in BufReader::new(socket).lines() {
        let line = line.map_err(|e| e.to_string())?;
        let Ok(reply) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if reply.get("id") == Some(&json!(1)) {
            return Ok(reply);
        }
    }
    Err("veyld cerró sin contestar".into())
}

/// Lo que contesta el daemon, como se lee: la frase y, debajo, el resto.
fn show(reply: &Value) -> ExitCode {
    if let Some(say) = reply.get("say").and_then(Value::as_str).filter(|s| !s.is_empty()) {
        println!("{say}");
    }
    if let Some(answer) = reply.get("answer").and_then(Value::as_str) {
        println!("{answer}");
    }
    if reply.get("ok").and_then(Value::as_bool) == Some(true) { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

/// Lo que se oye en un WAV, con el mismo modelo que usaría veyld.
fn hear(path: &str) -> ExitCode {
    let audio = match voice::wav(std::path::Path::new(path)) {
        Ok(audio) => audio,
        Err(why) => return fail(why),
    };
    let settings = config::settings();
    let Some(model) = voice::choose(&settings.voice_model) else {
        return fail("no hay modelo de voz: veyl voice get small-q5_1");
    };
    whisper_rs::install_logging_hooks();
    let mut params = whisper_rs::WhisperContextParameters::default();
    params.use_gpu(voice::GPU);
    let whisper = match whisper_rs::WhisperContext::new_with_params(&model, params) {
        Ok(whisper) => whisper,
        Err(e) => return fail(format!("{}: {e}", model.display())),
    };
    let started = std::time::Instant::now();
    // Como la voz de verdad: con las palabras del vocabulario en la pista, y lo oído mal corregido.
    let vocabulary = vocab::Vocabulary::load();
    let prompt = format!("{} {}.", voice::PROMPT, vocabulary.hints().join(", "));
    match voice::transcribe(&whisper, &audio, &prompt, voice::trims(&model)) {
        Ok(heard) => {
            let text = vocabulary.fix(&heard);
            if text != heard {
                eprintln!("(se oyó «{heard}»)");
            }
            println!("{text}");
            eprintln!("({}, {:.2} s de audio en {:.2} s)", model.display(), audio.len() as f32 / voice::RATE as f32, started.elapsed().as_secs_f32());
            let (grammar, _) = daemon::grammar();
            println!("{}", serde_json::to_string_pretty(&grammar.parse(&text)).unwrap_or_default());
            ExitCode::SUCCESS
        }
        Err(why) => fail(why),
    }
}

/// `veyld brain …`: ver, elegir y dar la clave del cerebro.
fn brain_cli(args: &[String]) -> ExitCode {
    let settings_path = config::state().join("settings.json");
    let mut json: Value = std::fs::read_to_string(&settings_path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_else(|| json!({}));
    let save = |json: &Value| std::fs::write(&settings_path, serde_json::to_string_pretty(json).unwrap_or_default()).map_err(|e| e.to_string());
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        [] => {
            let s = config::settings();
            match s.brain.as_str() {
                "" if s.llm.is_empty() => println!(
                    "Sin cerebro: lo que la gramática no entiende se queda sin contestar.\nveyld brain use claude-code (tu suscripción de Claude), codex, anthropic u openai."
                ),
                "" => println!("Contesta la orden de llm: {}", s.llm),
                provider => println!("Contesta {provider}{}", if s.brain_model.is_empty() { String::new() } else { format!(" con {}", s.brain_model) }),
            }
            ExitCode::SUCCESS
        }
        ["use", provider, rest @ ..] if ["anthropic", "openai", "claude-code", "codex"].contains(provider) => {
            json["brain"] =
                json!({ "provider": provider, "model": rest.first().copied().unwrap_or(""), "url": json.pointer("/brain/url").cloned().unwrap_or(json!("")) });
            match save(&json) {
                Ok(()) => {
                    println!("Contesta {provider}. Pruébalo: veyl say ¿qué tengo abierto?");
                    ExitCode::SUCCESS
                }
                Err(why) => fail(why),
            }
        }
        ["off"] => {
            if let Some(o) = json.as_object_mut() {
                o.remove("brain");
            }
            match save(&json) {
                Ok(()) => ExitCode::SUCCESS,
                Err(why) => fail(why),
            }
        }
        ["login", provider @ ("anthropic" | "openai")] => {
            eprintln!("Pega la clave de {provider} y pulsa Intro (se guarda en ~/.config/veyl/keys.json, solo para ti):");
            let mut key = String::new();
            if std::io::stdin().read_line(&mut key).is_err() || key.trim().is_empty() {
                return fail("sin clave");
            }
            match brain::save_key(provider, &key) {
                Ok(()) => ExitCode::SUCCESS,
                Err(why) => fail(why),
            }
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}

/// Los micros (fuentes de PipeWire que no son el monitor de una salida) y
/// cuál se usa: el de `"voice": {"mic": …}` o, sin él, el de por defecto.
fn mics() {
    let dump = std::process::Command::new("pw-dump").output().map(|o| o.stdout).unwrap_or_default();
    let objects: Vec<Value> = serde_json::from_slice(&dump).unwrap_or_default();
    let default = std::process::Command::new("wpctl")
        .args(["inspect", "@DEFAULT_AUDIO_SOURCE@"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
        .lines()
        .find_map(|l| l.trim().trim_start_matches("* ").strip_prefix("node.name = ").map(|n| n.trim_matches('"').to_string()))
        .unwrap_or_default();
    let wanted = config::settings().voice_mic;
    let chosen = if wanted.is_empty() { default.clone() } else { wanted.clone() };
    let mut found = false;
    for o in &objects {
        let props = o.pointer("/info/props").cloned().unwrap_or(Value::Null);
        let text = |k: &str| props.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        if text("media.class") != "Audio/Source" {
            continue;
        }
        found = true;
        let name = text("node.name");
        let mark = if name == chosen { "●" } else { " " };
        let mut notes = Vec::new();
        if name == default {
            notes.push("el de por defecto");
        }
        if name.contains("bluez") || text("device.api") == "bluez5" {
            notes.push("Bluetooth: al usarlo, los auriculares pasan a manos libres");
        }
        println!("{mark} {}\n    {name}{}", text("node.description"), if notes.is_empty() { String::new() } else { format!("  ({})", notes.join("; ")) });
    }
    if !found {
        println!("No hay ningún micro.");
    }
    if !wanted.is_empty() && !objects.iter().any(|o| o.pointer("/info/props/node.name").and_then(Value::as_str) == Some(wanted.as_str())) {
        println!("\nEl de settings.json («{wanted}») no está: se usa el de por defecto.");
    }
    println!("\nPara elegir uno: \"voice\": {{\"mic\": \"<nombre>\"}} en settings.json.");
}

/// Lo dice con la voz de Veyl, como lo diría el daemon.
fn speak(args: &[String]) -> ExitCode {
    let (copy, words) = match args {
        [flag, path, rest @ ..] if flag == "--wav" => (Some(path.clone()), rest),
        rest => (None, rest),
    };
    let settings = config::settings();
    let wav = match tts::render(&tts::spoken(&words.join(" ")), &settings.style()) {
        Ok(wav) => wav,
        Err(why) => return fail(why),
    };
    if let Some(path) = copy {
        return match std::fs::copy(&wav, &path) {
            Ok(_) => ExitCode::SUCCESS,
            Err(e) => fail(format!("{path}: {e}")),
        };
    }
    let played = std::process::Command::new("pw-play").args(["--volume", &format!("{:.2}", settings.loudness)]).arg(&wav).status();
    match played {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => fail(format!("pw-play: {e}")),
    }
}

/// Las voces de Piper que se pueden traer: nombre, ruta en rhasspy/piper-voices y cómo es.
const VOICES: &[(&str, &str, &str)] = &[
    ("voz-davefx", "es/es_ES/davefx/medium/es_ES-davefx-medium", "63 MB, España, grave (la de fábrica)"),
    ("voz-sharvard", "es/es_ES/sharvard/medium/es_ES-sharvard-medium", "77 MB, España"),
    ("voz-ald", "es/es_MX/ald/medium/es_MX-ald-medium", "63 MB, México"),
    ("voz-claude", "es/es_MX/claude/high/es_MX-claude-high", "114 MB, México, más fina"),
];

/// Los modelos que se pueden traer, con lo que pesan, y si están.
const MODELS: &[(&str, &str)] = &[
    ("tiny-q5_1", "31 MB, para máquinas muy justas"),
    ("base-q5_1", "57 MB, órdenes en una CPU modesta"),
    ("small-q5_1", "190 MB, el bueno sin GPU"),
    ("medium-q5_0", "514 MB"),
    ("large-v3-turbo-q5_0", "547 MB, el mejor con GPU"),
];

fn models() {
    let chosen = voice::choose(&config::settings().voice_model);
    println!("En {}{}:", voice::models().display(), if voice::GPU { " (con GPU)" } else { " (sin GPU)" });
    for (name, about) in MODELS {
        let path = voice::model_path(name);
        let mark = if chosen.as_ref() == Some(&path) {
            "●"
        } else if path.exists() {
            "○"
        } else {
            " "
        };
        println!("  {mark} {name:<22} {about}");
    }
    let vad = voice::model_path(voice::VAD).exists();
    println!("  {} {:<22} Silero, para saber cuándo hablas", if vad { "○" } else { " " }, voice::VAD);
    let xtts = voice::models().join("xtts-v2/model.pth").is_file();
    println!("\nVoz de Veyl:");
    println!("  {} {:<22} XTTS-v2, «Damien Black» con tratamiento de cine (~1,8 GB; licencia no comercial)", if xtts { "●" } else { " " }, "xtts");
    let speaker = tts::choose(&config::settings().speaker);
    println!("\nSi no está XTTS, Piper con filtro de robot:");
    for (name, path, about) in VOICES {
        let file = tts::voice_path(path.rsplit('/').next().unwrap_or(path));
        let mark = if speaker.as_ref() == Some(&file) {
            "●"
        } else if file.exists() {
            "○"
        } else {
            " "
        };
        println!("  {mark} {name:<22} {about}");
    }
}

/// Trae un modelo de whisper.cpp (de Hugging Face) y el de Silero si falta.
fn fetch(name: &str) -> Result<(), String> {
    if name == "xtts" {
        // La voz de película: entorno de Python, PyTorch y el modelo (~1,8 GB); acepta la licencia CPML.
        let script = std::env::current_exe()
            .ok()
            .and_then(|e| Some(e.parent()?.parent()?.parent()?.join("speech/install.sh")))
            .filter(|p| p.is_file())
            .ok_or("no encuentro veyld/speech/install.sh")?;
        let ok = std::process::Command::new("sh").arg(script).status().map_err(|e| e.to_string())?.success();
        return if ok { Ok(()) } else { Err("no se pudo instalar XTTS".into()) };
    }
    let dir = voice::models();
    let speaker = VOICES.iter().find(|(n, _, _)| *n == name);
    if !MODELS.iter().any(|(n, _)| *n == name) && speaker.is_none() {
        let all: Vec<&str> = MODELS.iter().map(|(n, _)| *n).chain(VOICES.iter().map(|(n, _, _)| *n)).collect();
        return Err(format!("no conozco «{name}»: {}", all.join(", ")));
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut wanted = Vec::new();
    if let Some((_, path, _)) = speaker {
        let file = tts::voice_path(path.rsplit('/').next().unwrap_or(path));
        let url = format!("https://huggingface.co/rhasspy/piper-voices/resolve/main/{path}.onnx");
        let mut config = file.as_os_str().to_os_string();
        config.push(".json");
        wanted.push((format!("{url}.json"), PathBuf::from(config)));
        wanted.push((url, file));
    } else {
        wanted.push((format!("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{name}.bin"), voice::model_path(name)));
    }
    if speaker.is_none() && !voice::model_path(voice::VAD).exists() {
        wanted.push((format!("https://huggingface.co/ggml-org/whisper-vad/resolve/main/ggml-{}.bin", voice::VAD), voice::model_path(voice::VAD)));
    }
    for (url, path) in wanted {
        if path.exists() {
            println!("{} ya está", path.display());
            continue;
        }
        let partial = path.with_extension("part");
        println!("Trayendo {url}");
        let ok = std::process::Command::new("curl")
            .args(["-L", "--fail", "--progress-bar", "-o"])
            .arg(&partial)
            .arg(&url)
            .status()
            .map_err(|e| format!("curl: {e}"))?
            .success();
        if !ok {
            let _ = std::fs::remove_file(&partial);
            return Err(format!("no se pudo traer {url}"));
        }
        std::fs::rename(&partial, &path).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

fn fail(why: impl std::fmt::Display) -> ExitCode {
    eprintln!("veyld: {why}");
    ExitCode::FAILURE
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = args.get(1..).unwrap_or_default().join(" ");
    match args.first().map(String::as_str) {
        None | Some("run") => match daemon::run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(why) => fail(why),
        },
        Some("say") if !rest.is_empty() => match ask(json!({ "say": rest, "source": "cli" })) {
            Ok(reply) => show(&reply),
            Err(why) => fail(why),
        },
        Some("parse") if !rest.is_empty() => {
            // La gramática no necesita al daemon; los títulos, sí (las aplicaciones, Hyprland).
            match ask(json!({ "parse": rest })) {
                Ok(reply) => println!("{}", serde_json::to_string_pretty(&reply["parse"]).unwrap_or_default()),
                Err(_) => {
                    let (grammar, _) = daemon::grammar();
                    println!("{}", serde_json::to_string_pretty(&grammar.parse(&rest)).unwrap_or_default());
                }
            }
            ExitCode::SUCCESS
        }
        Some("google") => match args.get(1..).unwrap_or_default() {
            [op, rest @ ..] if op == "login" && rest.len() != 1 && rest.len() <= 2 => {
                match google::login(rest.first().map_or("", |s| s.as_str()), rest.get(1).map_or("", |s| s.as_str())) {
                    Ok(account) => {
                        println!("Conectada: {account}");
                        ExitCode::SUCCESS
                    }
                    Err(why) => fail(why),
                }
            }
            [op] if op == "logout" => {
                google::logout();
                ExitCode::SUCCESS
            }
            [] => {
                let status = google::status();
                match status["account"].as_str() {
                    None => println!("No hay ninguna cuenta de Google conectada"),
                    Some(account) => println!("Conectada: {account}"),
                }
                if let Some(why) = status["error"].as_str() {
                    println!("Falla: {why}");
                }
                for c in status["calendars"].as_array().into_iter().flatten() {
                    println!(
                        "{}\t{}\t{}",
                        c["id"].as_str().unwrap_or(""),
                        if c["writable"].as_bool() == Some(true) { "escribible" } else { "solo lectura" },
                        c["summary"].as_str().unwrap_or("")
                    );
                }
                ExitCode::SUCCESS
            }
            _ => fail(USAGE),
        },
        Some("calendars") => {
            let sources = calendars::sources();
            if sources.is_empty() {
                println!("No hay calendarios: apúntalos en {}", config::tilde(&calendars::path().to_string_lossy()));
                return ExitCode::SUCCESS;
            }
            let failed = calendars::sync();
            let out: Value = std::fs::read_to_string(calendars::output()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
            for c in out["calendars"].as_array().into_iter().flatten() {
                let name = c["name"].as_str().unwrap_or("");
                match (c["on"].as_bool(), c["error"].as_str()) {
                    (Some(false), _) => println!("{name}: apagado"),
                    (_, Some(why)) => println!("{name}: {why} (se queda con {} de la última vez)", c["count"]),
                    _ => println!("{name}: {} eventos", c["count"]),
                }
            }
            if failed.is_empty() { ExitCode::SUCCESS } else { ExitCode::FAILURE }
        }
        Some("recall") => {
            // --full, todo; --json, todo en datos.
            let flags: Vec<&String> = args[1..].iter().filter(|a| a.starts_with("--")).collect();
            let when = args[1..].iter().filter(|a| !a.starts_with("--")).cloned().collect::<Vec<_>>().join(" ");
            let full = flags.iter().any(|f| *f == "--full" || *f == "--todo");
            let json = flags.iter().any(|f| *f == "--json");
            match ask(json!({ "recall": when, "full": full, "json": json })) {
                Ok(reply) if json => {
                    println!("{}", serde_json::to_string_pretty(reply.get("report").unwrap_or(&reply)).unwrap_or_default());
                    if reply["ok"] == true { ExitCode::SUCCESS } else { ExitCode::FAILURE }
                }
                Ok(reply) => show(&reply),
                Err(why) => fail(why),
            }
        }
        Some("timeline") => match ask(json!({ "timeline": rest })) {
            Ok(reply) => show(&reply),
            Err(why) => fail(why),
        },
        Some("find") if !rest.is_empty() => match ask(json!({ "find": rest })) {
            Ok(reply) => show(&reply),
            Err(why) => fail(why),
        },
        Some("agents") => match ask(json!({ "agents": true })) {
            Ok(reply) => {
                println!("{}", serde_json::to_string_pretty(&reply["agents"]).unwrap_or_default());
                ExitCode::SUCCESS
            }
            Err(why) => fail(why),
        },
        Some("hear") if args.len() == 2 => hear(&args[1]),
        Some("speak") if args.len() > 1 => speak(&args[1..]),
        Some("mcp") => brain::mcp(),
        Some("spotify") if args.get(1).map(String::as_str) == Some("login") => play::login(),
        Some("statusline") => {
            usage::statusline();
            ExitCode::SUCCESS
        }
        Some("brain") => brain_cli(&args[1..]),
        Some("mic") => {
            mics();
            ExitCode::SUCCESS
        }
        Some("model") if args.len() == 1 => {
            models();
            ExitCode::SUCCESS
        }
        Some("model") if args.len() == 3 && args[1] == "get" => match fetch(&args[2]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(why) => fail(why),
        },
        Some("nix") => match nix::cli(&args[1..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(why) => fail(why),
        },
        Some("grammar") => match daemon::grammar() {
            (grammar, None) => {
                println!("{} reglas", grammar.rules());
                ExitCode::SUCCESS
            }
            (_, Some(error)) => fail(error),
        },
        Some("-h" | "--help" | "help") => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}
