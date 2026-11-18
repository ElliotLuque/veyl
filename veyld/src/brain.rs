//! El cerebro de Veyl: lo que la gramática no entiende se lo pregunta a un
//! modelo, que puede mirar qué tienes delante, consultar el diario y dar
//! órdenes a Veyl con las mismas palabras que tú (las que entiende la
//! gramática). Lo peligroso sigue pidiendo un «sí».
//!
//! Proveedores (`"brain": {"provider": …}` en settings.json):
//!
//! - `anthropic`: la API de Claude, con una clave (`veyld brain login
//!   anthropic`, o `ANTHROPIC_API_KEY`) o con el perfil OAuth de `ant auth
//!   login` (el programa oficial de Anthropic).
//! - `openai`: cualquier API compatible con la de OpenAI (OpenAI, OpenRouter,
//!   Ollama…), con su clave y, si no es OpenAI, `"url"`.
//! - `claude-code`: tu suscripción de Claude, por Claude Code (`claude -p`),
//!   con la sesión que ya tengas iniciada en él.
//! - `codex`: tu suscripción de ChatGPT, por Codex (`codex exec`).
//!
//! Con las dos últimas las herramientas llegan por MCP: Claude Code o Codex
//! arrancan `veyld mcp`, que habla con el daemon por su socket.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use serde_json::{Value, json};

use crate::config;

/// El modelo de Claude que se usa si no se pide otro: Haiku 5.5, el más rápido
/// y barato (se pidió la máxima rapidez y el menor coste). Claude Code lo llama «haiku».
pub const CLAUDE: &str = "claude-haiku-5-5";
const CLAUDE_CODE: &str = "haiku";
/// Cuántas vueltas de herramientas, como mucho, por pregunta.
const TURNS: usize = 8;

/// Las herramientas: nombre, descripción y esquema de lo que reciben.
pub fn tools() -> Vec<(&'static str, String, Value)> {
    let tools: Vec<(&'static str, &'static str, Value)> = vec![
        (
            "veyl",
            "Da una orden a Veyl, el escritorio, en español y como la diría una persona: «sube el volumen a 30», «abre Firefox y muévelo al nivel 2», \
             «pon un temporizador de 10 minutos», «apunta dentista el jueves a las 5», «pausa la música», «ve al nivel 3», «cierra esta ventana», \
             «activa no molestar». Música solo si el usuario la pide como tal, diciendo dónde o qué: «pon Estopa en Spotify», «pon la canción \
             Bohemian Rhapsody»; nunca para algo que no sea poner música. Devuelve lo que contestó Veyl. Si la orden es peligrosa (apagar, reiniciar, cerrar todas las ventanas) Veyl no \
             la hace: pide que el usuario diga «sí», y tú debes decírselo así.",
            json!({
                "type": "object",
                "properties": { "orden": { "type": "string", "description": "La orden, en español, como se le diría a Veyl" } },
                "required": ["orden"],
                "additionalProperties": false
            }),
        ),
        (
            "contexto",
            "Lo que hay ahora en el escritorio: la hora y la fecha, la ventana con el foco, las ventanas abiertas por nivel, los agentes de IA en \
             marcha, lo que suena, el temporizador y lo apuntado para hoy y mañana.",
            json!({ "type": "object", "properties": {}, "required": [], "additionalProperties": false }),
        ),
        (
            "diario",
            "El diario de lo que el usuario ha hecho en el ordenador: qué aplicaciones, páginas, carpetas, órdenes de terminal, música, archivos y \
             agentes, y cuándo. `que`: «resumen» (lo de un tramo), «detalle» (todo, con horas y proyectos), «rato» (rato a rato), «indice» (cada \
             página, título, orden, pista y archivo distinto del tramo, con sus horas y su rato: para buscar por tema o por lo que \
             significa) o «buscar» (cuándo salió una palabra, en los 90 días; pon lo buscado en `texto`; solo casa palabras literales). `cuando`, en español: «hoy», «ayer por la tarde», «el jueves», «esta semana», «hace dos horas».",
            json!({
                "type": "object",
                "properties": {
                    "que": { "type": "string", "enum": ["resumen", "detalle", "rato", "indice", "buscar"] },
                    "cuando": { "type": "string", "description": "Cuándo, en español; vacío es hoy" },
                    "texto": { "type": "string", "description": "Lo que se busca, con «buscar»" }
                },
                "required": ["que", "cuando", "texto"],
                "additionalProperties": false
            }),
        ),
    ];
    let mut tools: Vec<(&'static str, String, Value)> = tools.into_iter().map(|(n, d, s)| (n, d.to_string(), s)).collect();
    tools.push((
        "sistema",
        "Mide el ordenador ahora: `que` es cpu (qué programas tiran de ella), memoria, disco (lo que más ocupa en la carpeta del usuario y \
         lo libre), red (router, internet, DNS, señal del wifi y qué programa mueve más), grafica, temperatura, bateria o todo (por qué va \
         lento: lo que más espera y quién tiene la culpa). Devuelve el diagnóstico en una frase y la tarjeta de Veyl con las cifras."
            .to_string(),
        json!({
            "type": "object",
            "properties": { "que": { "type": "string", "enum": ["todo", "cpu", "memoria", "disco", "red", "grafica", "temperatura", "bateria"] } },
            "required": ["que"],
            "additionalProperties": false
        }),
    ));
    tools.push(("mostrar", crate::card::description(), crate::card::schema()));
    tools
}

/// Cómo se le habla al modelo.
pub fn system() -> String {
    format!(
        "Eres Veyl, el escritorio del usuario (un shell para Hyprland en Linux) y su asistente. Contestas siempre en español, de tú.\n\
         \n\
         La primera línea de tu respuesta se dice en voz alta: una sola frase corta y natural, sin formato, sin emojis y sin listas. Si hace falta \
         más detalle (pasos, una lista, enlaces), va en las líneas siguientes, que se leen en una notificación: texto plano, sin Markdown \
         (nada de asteriscos ni almohadillas); para una lista, una línea por cosa empezando por «- ».\n\
         Lo dicho por voz pasa por un reconocedor y a veces llega mal escrito: entiéndelo por cómo suena. Si una palabra suena a Veyl \
         («bale», «vail», «beil»), es Veyl, o sea tú, no un actor, una marca ni otra cosa: «¿qué puede hacer bale?» pregunta qué puedes hacer tú.\n\
         Lo que sabes hacer, por si te lo preguntan: ventanas y niveles (abrir, cerrar, mover, colocar, pantalla completa), vistas con nombre \
         que juntan lo abierto, volumen, micro y música, wifi y bluetooth, no molestar, temporizadores, la agenda y los calendarios, el diario \
         de lo que hizo el usuario («¿qué hice ayer?», «¿cuándo vi…?»), los agentes de IA en marcha, capturas, el portapapeles, el esquema de \
         colores, medir el ordenador (qué lo ralentiza, la memoria, el disco, la red, la temperatura), buscar en la web y contestar con \
         tarjetas. Todo por voz o escribiendo.\n\
         \n\
         Para actuar sobre el escritorio usa la herramienta `veyl` con órdenes en español, una por llamada o varias unidas con «y». Para saber \
         qué hay abierto o qué pasa ahora, `contexto`; para lo que hizo antes, `diario` (si pregunta por un tema \
         o por algo que no recuerda con sus palabras exactas, «¿cuándo miré lo de la beca?», «¿qué leí de nutrición?», pide el `indice` del \
         tramo y busca tú por lo que significa, en cualquier idioma; prueba `buscar` con palabras sueltas solo como ayuda, y si no sabes \
         cuándo fue, el índice de «esta semana» o «este mes»; di siempre cuándo fue y en qué, con la página o la carpeta); para lo que pasa dentro del ordenador (qué lo \
         ralentiza, qué ocupa la memoria o el disco, por qué va mal la red, la temperatura, la batería), `sistema`, y contesta con lo medido: \
         su tarjeta ya trae las cifras, úsala de base en `mostrar` y añade lo que sepas (qué es ese proceso, si se puede cerrar). No inventes lo que no sepas: si no puedes hacerlo, dilo.\n\
         Lo peligroso (apagar, reiniciar, cerrar todas las ventanas) Veyl lo deja pendiente de un «sí»: dile al usuario que lo confirme diciendo «sí».\n\
         \n\
         Contesta siempre con una tarjeta (`mostrar`), salvo cuando solo hayas hecho una orden (abrir, mover, el volumen…). La tarjeta es la \
         respuesta que el usuario ve: hazla rica y a la medida de lo preguntado, como la ficha de un buscador. Elige un icono que encaje con \
         el tema (los colores son los del tema del usuario), pon la imagen si la hay (portada, cartel, producto, lugar), y elige la pieza que mejor \
         cuente cada cosa; la lista es el último recurso, solo si no encaja ninguna otra:\n\
         - el tiempo o una previsión de varios días u horas → `tira` (un día por ficha, con icono y máxima/mínima), y la cifra de ahora;\n\
         - comparar dos o más cosas → `tabla` (una columna por cosa, `mejor` en cada fila) y `pros` si hay que decidir;\n\
         - un precio, una cotización o algo que cambia en el tiempo → `grafica` de `lineas` con los puntos que tengas, y `metricas` (actual, \
         variación, máximo, mínimo);\n\
         - un reparto (gastos, votos, cuota, tiempo por app) → `grafica` de `anillo`; un ranking o unas cantidades por categoría → `barras`;\n\
         - varias cifras clave → `metricas`; una sola → `cifra`;\n\
         - cómo hacer algo → `pasos`; una historia o lo que pasó → `cronologia`; una nota o crítica → `valoracion`; una frase de alguien → \
         `cita`; código o una orden de terminal → `codigo`; varias fotos → `galeria`;\n\
         - una fecha que llega → `cuenta`; plataformas, géneros o estados → `etiquetas`.\n\
         Siempre, al final, los enlaces de donde lo sacaste y los botones de lo que probablemente quiera hacer después. Con la tarjeta, la notificación no hace falta: tu respuesta escrita queda en la \
         frase corta que se dice en voz alta. No añadas una lista de fuentes al final: si buscas en la web, pon los enlaces en la tarjeta.\n\
         \n\
         Ahora es {}.",
        now()
    )
}

/// «miércoles 18 de noviembre de 2026, 18:20».
fn now() -> String {
    use chrono::{Datelike, Timelike};
    let t = chrono::Local::now();
    let days = ["lunes", "martes", "miércoles", "jueves", "viernes", "sábado", "domingo"];
    let months = ["enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre"];
    format!(
        "{} {} de {} de {}, {:02}:{:02}",
        days[t.weekday().num_days_from_monday() as usize],
        t.day(),
        months[t.month0() as usize],
        t.year(),
        t.hour(),
        t.minute()
    )
}

// --- Claves ---------------------------------------------------------------------

/// Donde se guardan las claves: `~/.config/veyl/keys.json`, solo tuyo.
fn keys_path() -> PathBuf {
    config::config().join("keys.json")
}

fn keys() -> HashMap<String, String> {
    std::fs::read_to_string(keys_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Guarda la clave de un proveedor (0600).
pub fn save_key(provider: &str, key: &str) -> Result<(), String> {
    let mut all = keys();
    all.insert(provider.to_string(), key.trim().to_string());
    let path = keys_path();
    std::fs::create_dir_all(path.parent().ok_or("sin carpeta")?).map_err(|e| e.to_string())?;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file =
        std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    file.write_all(serde_json::to_string_pretty(&all).unwrap_or_default().as_bytes()).map_err(|e| e.to_string())
}

/// Lo guardado con ese nombre, si lo hay.
pub fn stored(name: &str) -> Option<String> {
    keys().get(name).cloned().filter(|k| !k.is_empty())
}

/// Lo olvida.
pub fn forget_key(name: &str) -> Result<(), String> {
    let mut all = keys();
    if all.remove(name).is_none() {
        return Ok(());
    }
    let path = keys_path();
    use std::os::unix::fs::OpenOptionsExt;
    let mut file =
        std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    file.write_all(serde_json::to_string_pretty(&all).unwrap_or_default().as_bytes()).map_err(|e| e.to_string())
}

/// La clave de un proveedor: la del entorno o la guardada.
fn key(provider: &str, env: &str) -> Option<String> {
    std::env::var(env).ok().filter(|k| !k.is_empty()).or_else(|| keys().get(provider).cloned()).filter(|k| !k.is_empty())
}

/// Cómo se entra en la API de Claude: con clave, o con el token OAuth del perfil de `ant`.
enum Pass {
    Key(String),
    Bearer(String),
}

fn anthropic_pass() -> Result<Pass, String> {
    if let Some(key) = key("anthropic", "ANTHROPIC_API_KEY") {
        return Ok(Pass::Key(key));
    }
    let out = Command::new("ant")
        .args(["auth", "print-credentials", "--access-token"])
        .output()
        .map_err(|_| "Sin clave de Anthropic: veyld brain login anthropic (o ant auth login para entrar con tu cuenta)".to_string())?;
    let token = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if out.status.success() && !token.is_empty() { Ok(Pass::Bearer(token)) } else { Err("El perfil de ant no da token: ant auth login".into()) }
}

// --- Las preguntas ----------------------------------------------------------------

/// Lo que se pregunta: lo dicho y, antes, lo hablado hace poco.
pub struct Ask {
    pub said: String,
    pub history: Vec<(String, String)>,
    /// Lo que hay delante (la herramienta `contexto`), ya mirado: una vuelta menos.
    pub context: String,
}

/// Lo que se le dice al modelo: el contexto de ahora y lo dicho.
fn said(question: &Ask) -> String {
    if question.context.is_empty() { question.said.clone() } else { format!("[Contexto ahora: {}]\n\n{}", question.context, question.said) }
}

/// La herramienta que pide el modelo, hecha por el daemon.
pub type Tool<'a> = &'a dyn Fn(&str, &Value) -> String;

/// Pregunta al cerebro configurado. Devuelve su respuesta en texto.
pub fn ask(settings: &config::Settings, question: &Ask, tool: Tool, socket: &std::path::Path) -> Result<String, String> {
    let model = settings.brain_model.as_str();
    match settings.brain.as_str() {
        "anthropic" => anthropic(if model.is_empty() { CLAUDE } else { model }, question, tool),
        "openai" => openai(model, &settings.brain_url, question, tool),
        "claude-code" => claude_code(if model.is_empty() { CLAUDE_CODE } else { model }, question, socket),
        "codex" => codex(model, question, socket),
        other => Err(format!("No conozco el proveedor «{other}»")),
    }
}

/// La API de Claude: el bucle de herramientas a mano (HTTP; no hay SDK de Rust).
fn anthropic(model: &str, question: &Ask, tool: Tool) -> Result<String, String> {
    let pass = anthropic_pass()?;
    let tools: Vec<Value> = tools()
        .into_iter()
        .map(|(name, description, schema)| json!({ "name": name, "description": description, "input_schema": schema, "strict": name != "mostrar" }))
        .collect();
    let mut messages: Vec<Value> = Vec::new();
    for (said, answer) in &question.history {
        messages.push(json!({ "role": "user", "content": said }));
        messages.push(json!({ "role": "assistant", "content": answer }));
    }
    messages.push(json!({ "role": "user", "content": said(question) }));
    let system = system();
    // Si un clasificador de seguridad declina, que lo intente el modelo que toque
    // (Haiku no tiene a quién pasárselo).
    let fallbacks = !model.contains("haiku");
    for _ in 0..TURNS {
        let mut body = json!({
            "model": model,
            "max_tokens": 16000,
            // Conversación corta: poco esfuerzo, para contestar pronto y gastar poco.
            "output_config": { "effort": "low" },
            "system": system,
            "tools": tools,
            "messages": messages,
        });
        let mut betas = Vec::new();
        if fallbacks {
            body["fallbacks"] = json!("default");
            betas.push("server-side-fallback-2026-07-01");
        }
        let request = ureq::post("https://api.anthropic.com/v1/messages")
            .timeout(Duration::from_secs(120))
            .set("content-type", "application/json")
            .set("anthropic-version", "2023-06-01");
        let request = match &pass {
            Pass::Key(key) => request.set("x-api-key", key),
            Pass::Bearer(token) => {
                betas.push("oauth-2025-04-20");
                request.set("authorization", &format!("Bearer {token}"))
            }
        };
        let request = if betas.is_empty() { request } else { request.set("anthropic-beta", &betas.join(",")) };
        let response: Value = match request.send_json(body) {
            Ok(response) => response.into_json().map_err(|e| format!("Claude: {e}"))?,
            Err(ureq::Error::Status(code, response)) => {
                let detail: Value = response.into_json().unwrap_or(Value::Null);
                let why = detail.pointer("/error/message").and_then(Value::as_str).unwrap_or("").to_string();
                return Err(match code {
                    401 | 403 => format!("Claude no acepta la clave ({code}): {why}"),
                    429 => "Claude está saturado o se acabó el cupo; prueba en un rato".into(),
                    _ => format!("Claude ha fallado ({code}): {why}"),
                });
            }
            Err(e) => return Err(format!("No llego a Claude: {e}")),
        };
        let content = response.get("content").cloned().unwrap_or(json!([]));
        match response.get("stop_reason").and_then(Value::as_str) {
            Some("refusal") => return Err("Claude no ha querido contestar a eso".into()),
            Some("tool_use") => {
                // Todas las herramientas pedidas, y todas sus respuestas en un solo mensaje.
                let results: Vec<Value> = content
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_use"))
                    .map(|b| {
                        let name = b.get("name").and_then(Value::as_str).unwrap_or("");
                        let out = tool(name, b.get("input").unwrap_or(&Value::Null));
                        json!({ "type": "tool_result", "tool_use_id": b.get("id").cloned().unwrap_or(Value::Null), "content": out })
                    })
                    .collect();
                messages.push(json!({ "role": "assistant", "content": content }));
                messages.push(json!({ "role": "user", "content": results }));
            }
            _ => return Ok(text_of(&content)),
        }
    }
    Err("Claude no ha terminado a tiempo".into())
}

/// Lo que contesta el modelo, sin el Markdown que la notificación no pinta y
/// sin la lista de fuentes que añade al buscar en la web (sus enlaces van en la
/// tarjeta): de «Sources:» o «Fuentes:» en adelante, fuera.
pub fn plain(text: &str) -> String {
    let mut lines = Vec::new();
    for line in text.lines() {
        let clean = line.trim_start_matches('#').trim_start().replace("**", "").replace("__", "");
        let head = clean.trim().trim_end_matches(':').to_lowercase();
        if matches!(head.as_str(), "sources" | "fuentes" | "references" | "referencias") {
            break;
        }
        lines.push(clean);
    }
    lines.join("\n").trim().to_string()
}

/// El texto de los bloques `text` de una respuesta de Claude.
fn text_of(content: &Value) -> String {
    let parts: Vec<&str> = content
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|b| b.get("text")?.as_str())
        .collect();
    parts.join("\n").trim().to_string()
}

/// Una API compatible con la de OpenAI (chat completions con herramientas).
fn openai(model: &str, url: &str, question: &Ask, tool: Tool) -> Result<String, String> {
    if model.is_empty() {
        return Err("Falta el modelo: \"brain\": {\"provider\": \"openai\", \"model\": \"…\"}".into());
    }
    let base = if url.is_empty() { "https://api.openai.com/v1" } else { url.trim_end_matches('/') };
    let key = key("openai", "OPENAI_API_KEY");
    if key.is_none() && url.is_empty() {
        return Err("Sin clave de OpenAI: veyld brain login openai".into());
    }
    let tools: Vec<Value> = tools()
        .into_iter()
        .map(|(name, description, schema)| json!({ "type": "function", "function": { "name": name, "description": description, "parameters": schema } }))
        .collect();
    let mut messages = vec![json!({ "role": "system", "content": system() })];
    for (said, answer) in &question.history {
        messages.push(json!({ "role": "user", "content": said }));
        messages.push(json!({ "role": "assistant", "content": answer }));
    }
    messages.push(json!({ "role": "user", "content": said(question) }));
    for _ in 0..TURNS {
        let mut request = ureq::post(&format!("{base}/chat/completions")).timeout(Duration::from_secs(120)).set("content-type", "application/json");
        if let Some(key) = &key {
            request = request.set("authorization", &format!("Bearer {key}"));
        }
        let response: Value = match request.send_json(json!({ "model": model, "messages": messages, "tools": tools })) {
            Ok(response) => response.into_json().map_err(|e| format!("{model}: {e}"))?,
            Err(ureq::Error::Status(code, response)) => {
                let detail: Value = response.into_json().unwrap_or(Value::Null);
                return Err(format!("{model} ha fallado ({code}): {}", detail.pointer("/error/message").and_then(Value::as_str).unwrap_or("")));
            }
            Err(e) => return Err(format!("No llego a {base}: {e}")),
        };
        let message = response.pointer("/choices/0/message").cloned().unwrap_or(Value::Null);
        let calls = message.get("tool_calls").and_then(Value::as_array).cloned().unwrap_or_default();
        if calls.is_empty() {
            return Ok(message.get("content").and_then(Value::as_str).unwrap_or("").trim().to_string());
        }
        messages.push(message);
        for call in calls {
            let name = call.pointer("/function/name").and_then(Value::as_str).unwrap_or("");
            let input: Value = call.pointer("/function/arguments").and_then(Value::as_str).and_then(|a| serde_json::from_str(a).ok()).unwrap_or(json!({}));
            messages.push(json!({ "role": "tool", "tool_call_id": call.get("id").cloned().unwrap_or(Value::Null), "content": tool(name, &input) }));
        }
    }
    Err(format!("{model} no ha terminado a tiempo"))
}

/// Lo dicho, con lo hablado antes, para los que no llevan conversación propia.
fn prompt(question: &Ask) -> String {
    let mut text = String::new();
    if !question.history.is_empty() {
        text.push_str("Lo que hablamos hace un momento:\n");
        for (said, answer) in &question.history {
            text.push_str(&format!("- Yo: {said}\n- Tú: {answer}\n"));
        }
        text.push('\n');
    }
    text.push_str(&said(question));
    text
}

/// El servidor MCP de veyld, para Claude Code y Codex.
fn mcp_config(socket: &std::path::Path) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let path = config::runtime().join("veyl/mcp.json");
    let config = json!({ "mcpServers": { "veyl": { "command": exe, "args": ["mcp"], "env": { "VEYLD_SOCKET": socket } } } });
    std::fs::write(&path, config.to_string()).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// Tu suscripción de Claude, por Claude Code: sin sus herramientas, solo las de Veyl.
fn claude_code(model: &str, question: &Ask, socket: &std::path::Path) -> Result<String, String> {
    let mcp = mcp_config(socket)?;
    let mut cmd = Command::new("claude");
    cmd.arg("-p")
        .arg(prompt(question))
        // De lo suyo, solo buscar en la web: nada de terminal ni de archivos.
        .args(["--output-format", "json", "--tools", "WebSearch,WebFetch", "--strict-mcp-config", "--no-session-persistence"])
        .arg("--mcp-config")
        .arg(&mcp)
        .args(["--allowedTools", "mcp__veyl__veyl,mcp__veyl__contexto,mcp__veyl__diario,mcp__veyl__sistema,mcp__veyl__mostrar,WebSearch,WebFetch"])
        .arg("--system-prompt")
        .arg(system())
        .current_dir(config::state());
    if !model.is_empty() {
        cmd.args(["--model", model]);
    }
    let out = crate::process::output(&mut cmd, b"", Duration::from_secs(180)).map_err(|e| format!("Claude Code: {e}"))?;
    let reply: Value = serde_json::from_slice(&out.stdout).map_err(|_| {
        let why = String::from_utf8_lossy(&out.stderr);
        format!("Claude Code no contesta: {}", why.lines().last().unwrap_or("¿has iniciado sesión? (claude, /login)"))
    })?;
    if reply.get("is_error").and_then(Value::as_bool) == Some(true) {
        return Err(format!("Claude Code: {}", reply.get("result").and_then(Value::as_str).unwrap_or("ha fallado")));
    }
    Ok(reply.get("result").and_then(Value::as_str).unwrap_or("").trim().to_string())
}

/// Tu suscripción de ChatGPT, por Codex, en solo lectura y con las herramientas de Veyl.
fn codex(model: &str, question: &Ask, socket: &std::path::Path) -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let last = config::runtime().join("veyl/codex.txt");
    let _ = std::fs::remove_file(&last);
    let mut cmd = Command::new("codex");
    cmd.args(["exec", "--skip-git-repo-check", "--ephemeral", "-s", "read-only"])
        .arg("-c")
        .arg(format!("mcp_servers.veyl.command={}", json!(exe)))
        .arg("-c")
        .arg("mcp_servers.veyl.args=[\"mcp\"]")
        .arg("-c")
        .arg(format!("mcp_servers.veyl.env={{VEYLD_SOCKET={}}}", json!(socket)))
        .arg("-o")
        .arg(&last)
        .current_dir(config::state());
    if !model.is_empty() {
        cmd.args(["-m", model]);
    }
    cmd.arg(format!("{}\n\n{}", system(), prompt(question)));
    let out = crate::process::output(&mut cmd, b"", Duration::from_secs(180)).map_err(|e| format!("Codex: {e}"))?;
    match std::fs::read_to_string(&last) {
        Ok(text) if !text.trim().is_empty() => Ok(text.trim().to_string()),
        _ => Err(format!("Codex no contesta: {}", String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("¿has iniciado sesión? (codex login)"))),
    }
}

// --- Una consulta suelta -----------------------------------------------------------

/// Una pregunta de una vuelta, sin herramientas ni conversación: para lo que
/// veyld necesita decidir (qué ventanas van en una vista). Contesta el texto.
pub fn judge(settings: &config::Settings, system: &str, prompt: &str) -> Result<String, String> {
    let model = settings.brain_model.as_str();
    match settings.brain.as_str() {
        "claude-code" => {
            let mut cmd = Command::new("claude");
            cmd.arg("-p")
                .arg(prompt)
                .args(["--output-format", "json", "--tools", "", "--strict-mcp-config", "--no-session-persistence"])
                .arg("--system-prompt")
                .arg(system)
                .args(["--model", if model.is_empty() { CLAUDE_CODE } else { model }])
                .current_dir(config::state());
            let out = crate::process::output(&mut cmd, b"", Duration::from_secs(90)).map_err(|e| format!("Claude Code: {e}"))?;
            let reply: Value = serde_json::from_slice(&out.stdout).map_err(|_| "Claude Code no contesta".to_string())?;
            if reply.get("is_error").and_then(Value::as_bool) == Some(true) {
                return Err(format!("Claude Code: {}", reply.get("result").and_then(Value::as_str).unwrap_or("ha fallado")));
            }
            Ok(reply.get("result").and_then(Value::as_str).unwrap_or("").trim().to_string())
        }
        "anthropic" => {
            let pass = anthropic_pass()?;
            let body = json!({
                "model": if model.is_empty() { CLAUDE } else { model },
                "max_tokens": 4000,
                "output_config": { "effort": "low" },
                "system": system,
                "messages": [{ "role": "user", "content": prompt }],
            });
            let request = ureq::post("https://api.anthropic.com/v1/messages")
                .timeout(Duration::from_secs(90))
                .set("content-type", "application/json")
                .set("anthropic-version", "2023-06-01");
            let request = match &pass {
                Pass::Key(key) => request.set("x-api-key", key),
                Pass::Bearer(token) => request.set("anthropic-beta", "oauth-2025-04-20").set("authorization", &format!("Bearer {token}")),
            };
            let response: Value = request.send_json(body).map_err(|e| format!("Claude: {e}"))?.into_json().map_err(|e| format!("Claude: {e}"))?;
            Ok(text_of(response.get("content").unwrap_or(&json!([]))))
        }
        "openai" => {
            if model.is_empty() {
                return Err("Falta el modelo del cerebro".into());
            }
            let url = settings.brain_url.trim_end_matches('/');
            let base = if url.is_empty() { "https://api.openai.com/v1" } else { url };
            let mut request = ureq::post(&format!("{base}/chat/completions")).timeout(Duration::from_secs(90)).set("content-type", "application/json");
            if let Some(key) = key("openai", "OPENAI_API_KEY") {
                request = request.set("authorization", &format!("Bearer {key}"));
            }
            let messages = json!([{ "role": "system", "content": system }, { "role": "user", "content": prompt }]);
            let response: Value = request
                .send_json(json!({ "model": model, "messages": messages }))
                .map_err(|e| format!("{model}: {e}"))?
                .into_json()
                .map_err(|e| format!("{model}: {e}"))?;
            Ok(response.pointer("/choices/0/message/content").and_then(Value::as_str).unwrap_or("").trim().to_string())
        }
        "codex" => {
            let last = config::runtime().join("veyl/codex-judge.txt");
            let _ = std::fs::remove_file(&last);
            let mut cmd = Command::new("codex");
            cmd.args(["exec", "--skip-git-repo-check", "--ephemeral", "-s", "read-only", "-o"]).arg(&last).current_dir(config::state());
            if !model.is_empty() {
                cmd.args(["-m", model]);
            }
            cmd.arg(format!("{system}\n\n{prompt}"));
            crate::process::output(&mut cmd, b"", Duration::from_secs(120)).map_err(|e| format!("Codex: {e}"))?;
            std::fs::read_to_string(&last).map(|t| t.trim().to_string()).map_err(|_| "Codex no contesta".into())
        }
        "" if !settings.llm.is_empty() => {
            let out =
                crate::process::output(Command::new("sh").args(["-c", &settings.llm]), format!("{system}\n\n{prompt}").as_bytes(), Duration::from_secs(90))
                    .map_err(|e| format!("El modelo: {e}"))?;
            Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
        }
        "" => Err("No hay cerebro".into()),
        other => Err(format!("No conozco el proveedor «{other}»")),
    }
}

/// El primer objeto JSON de una respuesta (los modelos a veces lo envuelven en texto o en ```).
pub fn json_in(text: &str) -> Option<Value> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    serde_json::from_str(text.get(start..=end)?).ok()
}

// --- MCP ------------------------------------------------------------------------

/// `veyld mcp`: un servidor MCP por la entrada y la salida estándar (JSON-RPC,
/// una línea por mensaje) con las herramientas de Veyl; cada llamada la hace
/// el daemon (por su socket).
pub fn mcp() -> std::process::ExitCode {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines().map_while(Result::ok) {
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some(id) = message.get("id").cloned() else {
            continue; // una notificación: no se contesta
        };
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let result = match message.get("method").and_then(Value::as_str).unwrap_or("") {
            "initialize" => json!({
                "protocolVersion": params.get("protocolVersion").cloned().unwrap_or(json!("2025-06-18")),
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "veyl", "version": env!("CARGO_PKG_VERSION") }
            }),
            "tools/list" => json!({
                "tools": tools().into_iter().map(|(name, description, schema)| json!({ "name": name, "description": description, "inputSchema": schema })).collect::<Vec<_>>()
            }),
            "tools/call" => {
                let name = params.get("name").and_then(Value::as_str).unwrap_or("");
                match call_daemon(name, params.get("arguments").unwrap_or(&json!({}))) {
                    Ok(text) => json!({ "content": [{ "type": "text", "text": text }], "isError": false }),
                    Err(why) => json!({ "content": [{ "type": "text", "text": why }], "isError": true }),
                }
            }
            "ping" => json!({}),
            other => {
                let error = json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": format!("no sé hacer {other}") } });
                let _ = writeln!(stdout, "{error}");
                let _ = stdout.flush();
                continue;
            }
        };
        let _ = writeln!(stdout, "{}", json!({ "jsonrpc": "2.0", "id": id, "result": result }));
        let _ = stdout.flush();
    }
    std::process::ExitCode::SUCCESS
}

/// Pide una herramienta al daemon por su socket.
fn call_daemon(name: &str, input: &Value) -> Result<String, String> {
    // Medir tarda: se mide aquí, sin parar al daemon.
    if name == "sistema" {
        return Ok(crate::system::told(input, &[]));
    }
    let mut stream = std::os::unix::net::UnixStream::connect(config::socket()).map_err(|_| "veyld no está en marcha".to_string())?;
    stream.set_read_timeout(Some(Duration::from_secs(60))).ok();
    writeln!(stream, "{}", json!({ "id": 1, "tool": name, "input": input })).map_err(|e| e.to_string())?;
    let mut reader = std::io::BufReader::new(stream);
    let mut line = String::new();
    while reader.read_line(&mut line).map_err(|e| e.to_string())? > 0 {
        if let Ok(reply) = serde_json::from_str::<Value>(&line)
            && reply.get("id") == Some(&json!(1))
        {
            return reply.get("result").and_then(Value::as_str).map(str::to_string).ok_or_else(|| "veyld no contesta".to_string());
        }
        line.clear();
    }
    Err("veyld no contesta".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_schemas_are_strict() {
        for (name, _, schema) in tools().into_iter().filter(|(n, ..)| *n != "mostrar") {
            assert_eq!(schema["additionalProperties"], json!(false), "{name}");
            let props: Vec<&String> = schema["properties"].as_object().unwrap().keys().collect();
            let required: Vec<&str> = schema["required"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
            assert_eq!(props.len(), required.len(), "{name}: todo lo que hay es obligatorio");
        }
    }

    #[test]
    fn sources_are_dropped() {
        assert_eq!(plain("Mañana llueve en Madrid.\n**Máxima** 15 °C\n\nSources:\n- [AEMET](https://aemet.es)"), "Mañana llueve en Madrid.\nMáxima 15 °C");
        assert_eq!(plain("Sources:\n- x"), "");
    }

    #[test]
    fn history_goes_in_the_prompt() {
        let ask = Ask { said: "¿y mañana?".into(), history: vec![("qué tengo hoy".into(), "Nada apuntado".into())], context: String::new() };
        let text = prompt(&ask);
        assert!(text.contains("- Yo: qué tengo hoy") && text.ends_with("¿y mañana?"));
    }
}
