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
//!
//! Mientras piensa, cuenta lo que hace (`Step`): las herramientas que pide
//! (buscar en la web, leer una página, mirar el diario, medir el ordenador)
//! y las páginas que encuentra, para que Veyl lo enseñe en la isla de
//! «Pensando». Con Claude Code y Codex se lee de su salida según llega; con
//! las API, en cada vuelta. Y se puede parar (`Ask::stop`).

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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
         respuesta que el usuario ve: hazla rica, visual y distinta cada vez, como una ficha bien diseñada, no un texto con adornos. Elige su \
         `estilo`: «portada» si tienes una buena imagen del tema (una película, un juego, un lugar, una persona, un producto), «titular» \
         para noticias, definiciones o un dato sin imagen, «ficha» para lo demás; un `icono` que encaje, y la imagen si la hay (los colores \
         son los del tema del usuario).\n\
         Luego las piezas: abre con la que más impacte (una cifra, un medidor, un marcador, la previsión, un versus, la portada), sigue con el \
         detalle, mezcla tipos distintos y no repitas el mismo; la lista y el texto son el último recurso:\n\
         - el tiempo → `prevision` (la temperatura de ahora en `valor` y una fila por día u hora, con su mínima y su máxima);\n\
         - un partido o un resultado → `marcador`;\n\
         - dos cosas frente a frente → `versus`, con una cifra en cada valor para que salgan las barras; tres o más, o filas solo de texto → \
         `tabla`; para decidir, además `pros`;\n\
         - un porcentaje, una nota o algo sobre un máximo (batería, calidad del aire, una valoración sobre 10, humedad) → `medidor`;\n\
         - cantidades con nombre (macros, presupuesto, horas por tarea) → `progreso`, o `grafica` de `anillo` si son partes de un todo; un \
         ranking → `grafica` de `barras`;\n\
         - un precio, una cotización o algo que cambia en el tiempo → `grafica` de `lineas` y `metricas` con su `serie`;\n\
         - varias cifras clave → `metricas`; una sola, enorme → `cifra`;\n\
         - una receta → `checklist` con los ingredientes y `pasos`; qué llevar o qué hacer → `checklist`; cómo hacer algo → `pasos`;\n\
         - qué significa una palabra o un concepto → `definicion`;\n\
         - un riesgo, un plazo o una advertencia que no debe pasarse por alto → `aviso` (solo entonces: no en todas);\n\
         - colores → `paleta`; una historia o lo que pasó → `cronologia`; una nota o crítica → `valoracion`; una frase de alguien → `cita`; \
         código o una orden de terminal → `codigo`; varias fotos → `galeria`; una fecha que llega → `cuenta`; plataformas, géneros o \
         estados → `etiquetas`.\n\
         Siempre, al final, los enlaces de donde lo sacaste, los botones de lo que probablemente quiera hacer después y, en `preguntas`, dos o \
         tres que probablemente quiera hacerte a continuación (cortas y en su voz: «¿Y en PC?», «¿Cuánto cuesta?»): solo las que tú puedas \
         contestar con lo que sabes, buscando en la web o con las herramientas de Veyl; nada que Veyl no sepa hacer (montar un tablero, \
         guardar fotos, editar documentos). Con la tarjeta, la notificación no hace falta: tu respuesta escrita queda en la \
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

// --- Lo que va haciendo -------------------------------------------------------------

/// Una página que el modelo ha encontrado o va a leer.
#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    pub title: String,
    pub url: String,
}

/// Lo que va haciendo el modelo mientras piensa.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// Lo que hace ahora: un icono de Phosphor y una frase («Busca «…»»).
    Doing { icon: &'static str, text: String },
    /// Las páginas que ha encontrado (una búsqueda) o que va a leer.
    Found(Vec<Source>),
}

/// A quién se le cuenta lo que va haciendo.
pub type Progress<'a> = &'a dyn Fn(Step);

/// «abre Firefox» → «Abre Firefox».
fn upper_first(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// Lo que se enseña de una herramienta que pide el modelo, con el nombre
/// que tenga para él (las de Veyl por MCP llevan delante `mcp__veyl__`).
/// `None` para lo que no hace falta contar (componer la tarjeta: sale sola).
pub fn doing(name: &str, input: &Value) -> Option<Step> {
    let text = |key: &str| crate::card::clip(input.get(key).and_then(Value::as_str).unwrap_or(""), 80);
    let name = name.strip_prefix("mcp__veyl__").unwrap_or(name);
    let (icon, text) = match name {
        "veyl" if !text("orden").is_empty() => ("lightning", upper_first(&text("orden"))),
        "contexto" => ("app-window", "Mira lo que tienes abierto".to_string()),
        "diario" if input.get("que").and_then(Value::as_str) == Some("buscar") => ("book-open", format!("Busca «{}» en el diario", text("texto"))),
        "diario" => ("book-open", if text("cuando").is_empty() { "Mira el diario de hoy".to_string() } else { format!("Mira el diario · {}", text("cuando")) }),
        "sistema" => {
            let what = match input.get("que").and_then(Value::as_str).unwrap_or("") {
                "cpu" => "la CPU",
                "memoria" => "la memoria",
                "disco" => "el disco",
                "red" => "la red",
                "grafica" => "la gráfica",
                "temperatura" => "la temperatura",
                "bateria" => "la batería",
                _ => "el ordenador",
            };
            ("gauge", format!("Mide {what}"))
        }
        "WebSearch" | "web_search" if !text("query").is_empty() => ("magnifying-glass", format!("Busca «{}»", text("query"))),
        "WebFetch" | "web_fetch" => ("globe", format!("Lee {}", host(input.get("url").and_then(Value::as_str).unwrap_or("")))),
        _ => return None,
    };
    Some(Step::Doing { icon, text })
}

/// Lo que hace mientras razona.
fn thinks() -> Step {
    Step::Doing { icon: "brain", text: "Piensa".into() }
}

/// Le pasa a `progress` lo que va haciendo, salvo que piensa antes de haber
/// hecho nada: eso ya lo dice «Pensando».
fn relay(progress: Progress, acted: &mut bool, step: Step) {
    if step == thinks() && !*acted {
        return;
    }
    *acted = true;
    progress(step);
}

/// Cuenta lo que hace una herramienta que pide el modelo, si hay algo que contar.
fn tell(progress: Progress, name: &str, input: &Value) {
    if let Some(step) = doing(name, input) {
        progress(step);
    }
}

/// «https://www.insee.fr/fr/…» → «insee.fr».
pub fn host(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit_once('@').map_or(host, |(_, h)| h);
    host.strip_prefix("www.").unwrap_or(host).to_lowercase()
}

/// Las páginas de una lista `[{title, url}]`, solo las web.
fn sources(list: &Value) -> Vec<Source> {
    list.as_array()
        .into_iter()
        .flatten()
        .filter_map(|link| {
            let url = link.get("url").and_then(Value::as_str)?.trim();
            (url.starts_with("https://") || url.starts_with("http://"))
                .then(|| Source { title: crate::card::clip(link.get("title").and_then(Value::as_str).unwrap_or(""), 90), url: url.to_string() })
        })
        .collect()
}

/// Lo que cuenta una línea de `claude -p --output-format stream-json`: las
/// herramientas que pide (en los mensajes del asistente) y las páginas que
/// encuentra una búsqueda (en su resultado, «Links: [{title, url}]»). Con
/// `--include-partial-messages` también dice cuándo empieza a escribir cada
/// cosa: componer la tarjeta lleva un rato, y se cuenta desde que empieza.
fn claude_steps(event: &Value) -> Vec<Step> {
    if event.get("type").and_then(Value::as_str) == Some("stream_event") {
        let block = event.pointer("/event/content_block").unwrap_or(&Value::Null);
        let starts = event.pointer("/event/type").and_then(Value::as_str) == Some("content_block_start");
        return match (block.get("type").and_then(Value::as_str), block.get("name").and_then(Value::as_str)) {
            (Some("tool_use"), Some("mcp__veyl__mostrar")) if starts => vec![Step::Doing { icon: "sparkle", text: "Compone la respuesta".into() }],
            (Some("thinking"), _) if starts => vec![thinks()],
            _ => Vec::new(),
        };
    }
    let blocks = event.pointer("/message/content").and_then(Value::as_array).cloned().unwrap_or_default();
    let mut steps = Vec::new();
    for block in &blocks {
        match (event.get("type").and_then(Value::as_str), block.get("type").and_then(Value::as_str)) {
            (Some("assistant"), Some("tool_use")) => {
                let (name, input) = (block.get("name").and_then(Value::as_str).unwrap_or(""), block.get("input").unwrap_or(&Value::Null));
                steps.extend(doing(name, input));
                if name == "WebFetch" {
                    steps.push(Step::Found(sources(&json!([{ "url": input.get("url") }]))));
                }
            }
            (Some("user"), Some("tool_result")) => {
                let text = match block.get("content") {
                    Some(Value::String(text)) => text.clone(),
                    Some(Value::Array(parts)) => parts.iter().filter_map(|p| p.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n"),
                    _ => String::new(),
                };
                if text.starts_with("Web search results")
                    && let Some(at) = text.find("Links: [")
                    && let Some(Ok(links)) = serde_json::Deserializer::from_str(&text[at + 7..]).into_iter::<Value>().next()
                {
                    let found = sources(&links);
                    if !found.is_empty() {
                        steps.push(Step::Found(found));
                    }
                }
            }
            _ => {}
        }
    }
    steps
}

/// Lo que cuenta una línea de `codex exec --json`: las herramientas de Veyl,
/// las órdenes que ejecuta y las búsquedas, con lo que encuentran.
fn codex_steps(event: &Value) -> Vec<Step> {
    let item = event.get("item").unwrap_or(&Value::Null);
    let text = |v: Option<&Value>| crate::card::clip(v.and_then(Value::as_str).unwrap_or(""), 80);
    match (event.get("type").and_then(Value::as_str), item.get("type").and_then(Value::as_str)) {
        (Some("item.started"), Some("mcp_tool_call")) => {
            let input = match item.get("arguments") {
                Some(Value::String(raw)) => serde_json::from_str(raw).unwrap_or(Value::Null),
                Some(other) => other.clone(),
                None => Value::Null,
            };
            doing(item.get("tool").and_then(Value::as_str).unwrap_or(""), &input).into_iter().collect()
        }
        (Some("item.started"), Some("reasoning")) => vec![thinks()],
        (Some("item.started"), Some("command_execution")) if !text(item.get("command")).is_empty() => {
            vec![Step::Doing { icon: "terminal", text: format!("Ejecuta «{}»", text(item.get("command"))) }]
        }
        (Some("item.completed"), Some("web_search")) => {
            let action = item.get("action").unwrap_or(&Value::Null);
            let mut steps = Vec::new();
            match action.get("type").and_then(Value::as_str) {
                Some("open_page" | "find_in_page") => {
                    let url = action.get("url").and_then(Value::as_str).unwrap_or("");
                    if !url.is_empty() {
                        steps.extend(doing("web_fetch", &json!({ "url": url })));
                        steps.push(Step::Found(sources(&json!([{ "url": url }]))));
                    }
                }
                _ => {
                    let query = text(item.get("query"));
                    let query = if query.is_empty() { text(action.get("query")) } else { query };
                    steps.extend(doing("web_search", &json!({ "query": query })));
                }
            }
            let found = sources(item.get("results").unwrap_or(&Value::Null));
            if !found.is_empty() {
                steps.push(Step::Found(found));
            }
            steps
        }
        _ => Vec::new(),
    }
}

// --- Las preguntas ----------------------------------------------------------------

/// Lo que se pregunta: lo dicho y, antes, lo hablado hace poco.
pub struct Ask {
    pub said: String,
    pub history: Vec<(String, String)>,
    /// Lo que hay delante (la herramienta `contexto`), ya mirado: una vuelta menos.
    pub context: String,
    /// A `true`, se deja de pensar: se mata a Claude Code o a Codex, y las API
    /// no dan otra vuelta.
    pub stop: Arc<AtomicBool>,
}

/// Lo que se contesta cuando se para a propósito.
pub const STOPPED: &str = "Parado";

impl Ask {
    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }
}

/// Lo que se le dice al modelo: el contexto de ahora y lo dicho.
fn said(question: &Ask) -> String {
    if question.context.is_empty() { question.said.clone() } else { format!("[Contexto ahora: {}]\n\n{}", question.context, question.said) }
}

/// La herramienta que pide el modelo, hecha por el daemon.
pub type Tool<'a> = &'a dyn Fn(&str, &Value) -> String;

/// Pregunta al cerebro configurado y le cuenta a `progress` lo que va
/// haciendo. Devuelve su respuesta en texto; si se para, `Err(STOPPED)`.
pub fn ask(settings: &config::Settings, question: &Ask, tool: Tool, progress: Progress, socket: &std::path::Path) -> Result<String, String> {
    let model = settings.brain_model.as_str();
    let answer = match settings.brain.as_str() {
        "anthropic" => anthropic(if model.is_empty() { CLAUDE } else { model }, question, tool, progress),
        "openai" => openai(model, &settings.brain_url, question, tool, progress),
        "claude-code" => claude_code(if model.is_empty() { CLAUDE_CODE } else { model }, question, progress, socket),
        "codex" => codex(model, question, progress, socket),
        other => Err(format!("No conozco el proveedor «{other}»")),
    };
    if question.stopped() { Err(STOPPED.into()) } else { answer }
}

/// La API de Claude: el bucle de herramientas a mano (HTTP; no hay SDK de Rust).
fn anthropic(model: &str, question: &Ask, tool: Tool, progress: Progress) -> Result<String, String> {
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
        if question.stopped() {
            return Err(STOPPED.into());
        }
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
        // Parado mientras contestaba: lo que pida ya no se hace.
        if question.stopped() {
            return Err(STOPPED.into());
        }
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
                        tell(progress, name, b.get("input").unwrap_or(&Value::Null));
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
fn openai(model: &str, url: &str, question: &Ask, tool: Tool, progress: Progress) -> Result<String, String> {
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
        if question.stopped() {
            return Err(STOPPED.into());
        }
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
        if question.stopped() {
            return Err(STOPPED.into());
        }
        let message = response.pointer("/choices/0/message").cloned().unwrap_or(Value::Null);
        let calls = message.get("tool_calls").and_then(Value::as_array).cloned().unwrap_or_default();
        if calls.is_empty() {
            return Ok(message.get("content").and_then(Value::as_str).unwrap_or("").trim().to_string());
        }
        messages.push(message);
        for call in calls {
            let name = call.pointer("/function/name").and_then(Value::as_str).unwrap_or("");
            let input: Value = call.pointer("/function/arguments").and_then(Value::as_str).and_then(|a| serde_json::from_str(a).ok()).unwrap_or(json!({}));
            tell(progress, name, &input);
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

/// Este programa, para que Claude Code o Codex arranquen su servidor MCP. Si
/// se ha recompilado mientras corre, Linux da su ruta con « (deleted)» detrás:
/// sin ella es la del binario nuevo, que también lo sabe hacer.
fn this_exe() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    Ok(exe.to_str().and_then(|p| p.strip_suffix(" (deleted)")).map(PathBuf::from).unwrap_or(exe))
}

/// El servidor MCP de veyld, para Claude Code y Codex.
fn mcp_config(socket: &std::path::Path) -> Result<PathBuf, String> {
    let exe = this_exe()?;
    let path = config::runtime().join("veyl/mcp.json");
    let config = json!({ "mcpServers": { "veyl": { "command": exe, "args": ["mcp"], "env": { "VEYLD_SOCKET": socket } } } });
    std::fs::write(&path, config.to_string()).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// Tu suscripción de Claude, por Claude Code: sin sus herramientas, solo las de Veyl.
/// Cuenta lo que hace según lo va haciendo (`stream-json`, un evento por línea).
fn claude_code(model: &str, question: &Ask, progress: Progress, socket: &std::path::Path) -> Result<String, String> {
    let mcp = mcp_config(socket)?;
    let mut cmd = Command::new("claude");
    cmd.arg("-p")
        .arg(prompt(question))
        // De lo suyo, solo buscar en la web: nada de terminal ni de archivos.
        .args(["--output-format", "stream-json", "--verbose", "--include-partial-messages"])
        .args(["--tools", "WebSearch,WebFetch", "--strict-mcp-config", "--no-session-persistence"])
        .arg("--mcp-config")
        .arg(&mcp)
        .args(["--allowedTools", "mcp__veyl__veyl,mcp__veyl__contexto,mcp__veyl__diario,mcp__veyl__sistema,mcp__veyl__mostrar,WebSearch,WebFetch"])
        .arg("--system-prompt")
        .arg(system())
        .current_dir(config::state());
    if !model.is_empty() {
        cmd.args(["--model", model]);
    }
    let (mut reply, mut acted) = (None, false);
    let (_, stderr) = crate::process::lines(&mut cmd, Duration::from_secs(180), &question.stop, &mut |line| {
        let Ok(event) = serde_json::from_str::<Value>(line) else { return };
        if event.get("type").and_then(Value::as_str) == Some("result") {
            reply = Some(event);
            return;
        }
        for step in claude_steps(&event) {
            relay(progress, &mut acted, step);
        }
    })
    .map_err(|e| if question.stopped() { STOPPED.to_string() } else { format!("Claude Code: {e}") })?;
    let reply = reply.ok_or_else(|| {
        let why = String::from_utf8_lossy(&stderr);
        format!("Claude Code no contesta: {}", why.lines().last().unwrap_or("¿has iniciado sesión? (claude, /login)"))
    })?;
    if reply.get("is_error").and_then(Value::as_bool) == Some(true) {
        return Err(format!("Claude Code: {}", reply.get("result").and_then(Value::as_str).unwrap_or("ha fallado")));
    }
    Ok(reply.get("result").and_then(Value::as_str).unwrap_or("").trim().to_string())
}

/// Tu suscripción de ChatGPT, por Codex, en solo lectura y con las herramientas de Veyl.
/// Cuenta lo que hace según lo va haciendo (`--json`, un evento por línea).
fn codex(model: &str, question: &Ask, progress: Progress, socket: &std::path::Path) -> Result<String, String> {
    let exe = this_exe()?;
    let last = config::runtime().join("veyl/codex.txt");
    let _ = std::fs::remove_file(&last);
    let mut cmd = Command::new("codex");
    cmd.args(["exec", "--skip-git-repo-check", "--ephemeral", "-s", "read-only", "--json"])
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
    let mut acted = false;
    let (_, stderr) = crate::process::lines(&mut cmd, Duration::from_secs(180), &question.stop, &mut |line| {
        if let Ok(event) = serde_json::from_str::<Value>(line) {
            for step in codex_steps(&event) {
                relay(progress, &mut acted, step);
            }
        }
    })
    .map_err(|e| if question.stopped() { STOPPED.to_string() } else { format!("Codex: {e}") })?;
    match std::fs::read_to_string(&last) {
        Ok(text) if !text.trim().is_empty() => Ok(text.trim().to_string()),
        _ => Err(format!("Codex no contesta: {}", String::from_utf8_lossy(&stderr).lines().last().unwrap_or("¿has iniciado sesión? (codex login)"))),
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
    fn claude_code_tells_what_it_does() {
        let search = json!({ "type": "assistant", "message": { "content": [
            { "type": "tool_use", "name": "WebSearch", "input": { "query": "Lyon population habitants 2026", "mode": "standard" } }
        ] } });
        assert_eq!(claude_steps(&search), [Step::Doing { icon: "magnifying-glass", text: "Busca «Lyon population habitants 2026»".into() }]);
        let links = "Web search results for query: \"Lyon\"\n\nLinks: [{\"title\":\"Insee\",\"url\":\"https://www.insee.fr/a\"},{\"title\":\"x\",\"url\":\"ftp://x\"}]\n\nLyon [1]…";
        let found = json!({ "type": "user", "message": { "content": [{ "type": "tool_result", "tool_use_id": "t", "content": links }] } });
        assert_eq!(claude_steps(&found), [Step::Found(vec![Source { title: "Insee".into(), url: "https://www.insee.fr/a".into() }])]);
        // Las de Veyl llegan por MCP; componer la tarjeta no se cuenta.
        let veyl = json!({ "type": "assistant", "message": { "content": [
            { "type": "text", "text": "Voy" },
            { "type": "tool_use", "name": "mcp__veyl__diario", "input": { "que": "resumen", "cuando": "ayer", "texto": "" } },
            { "type": "tool_use", "name": "mcp__veyl__mostrar", "input": {} }
        ] } });
        assert_eq!(claude_steps(&veyl), [Step::Doing { icon: "book-open", text: "Mira el diario · ayer".into() }]);
        let fetch = json!({ "type": "assistant", "message": { "content": [
            { "type": "tool_use", "name": "WebFetch", "input": { "url": "https://www.rockstargames.com/VI", "prompt": "fecha" } }
        ] } });
        assert_eq!(
            claude_steps(&fetch),
            [
                Step::Doing { icon: "globe", text: "Lee rockstargames.com".into() },
                Step::Found(vec![Source { title: String::new(), url: "https://www.rockstargames.com/VI".into() }])
            ]
        );
        assert!(claude_steps(&json!({ "type": "result", "result": "Hecho" })).is_empty());
        // Componer la tarjeta se cuenta desde que empieza a escribirla; lo demás, cuando se sabe qué es.
        let starts = |name: &str| {
            json!({ "type": "stream_event", "event": { "type": "content_block_start", "index": 1,
            "content_block": { "type": "tool_use", "name": name, "input": {} } } })
        };
        assert_eq!(claude_steps(&starts("mcp__veyl__mostrar")), [Step::Doing { icon: "sparkle", text: "Compone la respuesta".into() }]);
        assert!(claude_steps(&starts("WebSearch")).is_empty());
        let thinking = json!({ "type": "stream_event", "event": { "type": "content_block_start", "content_block": { "type": "thinking", "thinking": "" } } });
        assert_eq!(claude_steps(&thinking), [thinks()]);
        // Pensar antes de hacer nada no se cuenta; después, sí.
        let told = std::cell::RefCell::new(Vec::new());
        let progress = |step: Step| told.borrow_mut().push(step);
        let mut acted = false;
        for step in [thinks(), Step::Doing { icon: "globe", text: "Lee a.com".into() }, thinks()] {
            relay(&progress, &mut acted, step);
        }
        assert_eq!(told.into_inner(), [Step::Doing { icon: "globe", text: "Lee a.com".into() }, thinks()]);
        let delta = json!({ "type": "stream_event", "event": { "type": "content_block_delta", "delta": { "type": "text_delta", "text": "Hola" } } });
        assert!(claude_steps(&delta).is_empty());
    }

    #[test]
    fn codex_tells_what_it_does() {
        let search = json!({ "type": "item.completed", "item": { "id": "x", "type": "web_search", "query": "population Lyon",
            "action": { "type": "search", "query": "population Lyon" },
            "results": [{ "type": "text_result", "domain": "www.insee.fr", "title": "Insee", "url": "https://www.insee.fr/fr/statistiques/1" }] } });
        assert_eq!(
            codex_steps(&search),
            [
                Step::Doing { icon: "magnifying-glass", text: "Busca «population Lyon»".into() },
                Step::Found(vec![Source { title: "Insee".into(), url: "https://www.insee.fr/fr/statistiques/1".into() }])
            ]
        );
        // Al empezar una búsqueda aún no sabe qué busca.
        assert!(codex_steps(&json!({ "type": "item.started", "item": { "type": "web_search", "query": "", "action": { "type": "other" } } })).is_empty());
        let measure =
            json!({ "type": "item.started", "item": { "type": "mcp_tool_call", "server": "veyl", "tool": "sistema", "arguments": { "que": "memoria" } } });
        assert_eq!(codex_steps(&measure), [Step::Doing { icon: "gauge", text: "Mide la memoria".into() }]);
        let order = json!({ "type": "item.started", "item": { "type": "mcp_tool_call", "tool": "veyl", "arguments": "{\"orden\":\"sube el volumen\"}" } });
        assert_eq!(codex_steps(&order), [Step::Doing { icon: "lightning", text: "Sube el volumen".into() }]);
    }

    #[test]
    fn hosts() {
        assert_eq!(host("https://www.insee.fr/fr?x=1"), "insee.fr");
        assert_eq!(host("http://yo@Example.com/a"), "example.com");
    }

    #[test]
    fn history_goes_in_the_prompt() {
        let ask =
            Ask { said: "¿y mañana?".into(), history: vec![("qué tengo hoy".into(), "Nada apuntado".into())], context: String::new(), stop: Arc::default() };
        let text = prompt(&ask);
        assert!(text.contains("- Yo: qué tengo hoy") && text.ends_with("¿y mañana?"));
    }
}
