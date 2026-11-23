//! Las tarjetas que compone el cerebro: la respuesta a una pregunta, hecha a
//! su medida (una lista, unas cifras, unos pasos, unos botones) y pintada por
//! Veyl en la isla del centro, después de «Pensando». No es código: es un
//! vocabulario fijo de piezas que Veyl sabe pintar, con un estilo de cabecera
//! que elige el modelo; todo va en el acento del tema. Lo que llega se limpia
//! aquí (longitudes, cuántas piezas, iconos que existen, solo enlaces web) y
//! lo que no se entiende se tira.

use serde_json::{Value, json};
use std::io::Read;
use std::time::Duration;

/// Los colores que puede pedir el modelo, para cosas pequeñas y con sentido:
/// Veyl los toma del esquema (lo demás va siempre en el acento del tema).
pub const COLORS: &[&str] = &["bien", "aviso", "mal", "neutro"];

/// Cómo va su cabeza: con icono y título (`ficha`), con la imagen a sangre y
/// el título grande debajo (`portada`) o con un título grande (`titular`).
pub const STYLES: &[&str] = &["ficha", "portada", "titular"];

/// Los iconos de Phosphor que puede usar una tarjeta (comprobados en la fuente).
pub const ICONS: &[&str] = &[
    "info",
    "memory",
    "gauge",
    "swap",
    "hourglass-medium",
    "hard-drives",
    "database",
    "wifi-x",
    "network",
    "address-book",
    "arrow-down",
    "arrow-up",
    "graphics-card",
    "battery-charging",
    "battery-full",
    "clock",
    "calendar",
    "calendar-blank",
    "music-notes",
    "cloud",
    "sun",
    "moon",
    "cloud-rain",
    "snowflake",
    "thermometer",
    "drop",
    "wind",
    "umbrella",
    "globe",
    "link",
    "file",
    "file-text",
    "folder",
    "terminal",
    "code",
    "robot",
    "lightning",
    "chart-bar",
    "chart-line",
    "list-bullets",
    "list-checks",
    "check-circle",
    "warning",
    "x-circle",
    "star",
    "heart",
    "map-pin",
    "envelope",
    "chat-circle",
    "user",
    "users",
    "book-open",
    "newspaper",
    "magnifying-glass",
    "play",
    "pause",
    "timer",
    "hourglass",
    "cpu",
    "wifi-high",
    "bluetooth",
    "speaker-high",
    "microphone",
    "monitor",
    "browsers",
    "app-window",
    "stack",
    "keyboard",
    "lightbulb",
    "coffee",
    "airplane",
    "car",
    "train",
    "currency-eur",
    "currency-dollar",
    "shopping-cart",
    "gift",
    "game-controller",
    "film-strip",
    "image",
    "camera",
    "git-branch",
    "bug",
    "rocket",
    "graduation-cap",
    "trophy",
    "fire",
    "leaf",
    "calculator",
    "translate",
    "question",
    "sparkle",
    "brain",
    "flask",
    "hammer",
    "wrench",
    "house",
    "briefcase",
    "tag",
    "clipboard-text",
    "arrow-right",
    "trend-up",
    "trend-down",
    "cloud-sun",
    "cloud-moon",
    "cloud-lightning",
    "cloud-snow",
    "cloud-fog",
    "rainbow",
    "tornado",
    "sun-horizon",
    "sun-dim",
    "moon-stars",
    "thermometer-hot",
    "thermometer-cold",
    "umbrella-simple",
    "fork-knife",
    "cooking-pot",
    "chef-hat",
    "bowl-steam",
    "bowl-food",
    "coffee-bean",
    "pizza",
    "hamburger",
    "bread",
    "egg",
    "carrot",
    "orange",
    "avocado",
    "cheese",
    "pepper",
    "cake",
    "cookie",
    "ice-cream",
    "popcorn",
    "wine",
    "beer-stein",
    "martini",
    "soccer-ball",
    "basketball",
    "football",
    "tennis-ball",
    "volleyball",
    "medal",
    "flag-checkered",
    "barbell",
    "person-simple-run",
    "person-simple-bike",
    "person-simple-swim",
    "bicycle",
    "motorcycle",
    "scooter",
    "airplane-takeoff",
    "bus",
    "train-simple",
    "boat",
    "sailboat",
    "suitcase-rolling",
    "map-trifold",
    "compass",
    "mountains",
    "island",
    "tree",
    "tree-palm",
    "tent",
    "campfire",
    "gas-pump",
    "charging-station",
    "film-slate",
    "film-reel",
    "television",
    "ticket",
    "guitar",
    "piano-keys",
    "microphone-stage",
    "vinyl-record",
    "headphones",
    "palette",
    "paint-brush",
    "books",
    "atom",
    "dna",
    "microscope",
    "planet",
    "rocket-launch",
    "heartbeat",
    "pill",
    "first-aid",
    "tooth",
    "baby",
    "dog",
    "cat",
    "bird",
    "fish",
    "paw-print",
    "plant",
    "flower",
    "crown",
    "smiley",
    "magic-wand",
    "star-four",
    "flame",
    "check",
    "checks",
    "house-line",
    "buildings",
    "bank",
    "money",
    "coins",
    "wallet",
    "credit-card",
    "piggy-bank",
    "receipt",
    "storefront",
    "shopping-bag",
    "chart-line-up",
    "chart-line-down",
    "chart-pie",
    "chart-donut",
    "calendar-check",
    "calendar-star",
    "clock-countdown",
    "hourglass-high",
    "t-shirt",
    "sneaker",
    "gavel",
    "scales",
    "shield-check",
    "lock",
    "key",
    "currency-btc",
    "music-note",
    "device-mobile",
    "laptop",
    "person-simple-walk",
];

/// Lo que escribe el modelo en vez del nombre de un icono (en español, con
/// emoji) y el icono que quería.
const ALIASES: &[(&str, &str)] = &[
    ("sol", "sun"),
    ("soleado", "sun"),
    ("despejado", "sun"),
    ("luna", "moon"),
    ("noche", "moon-stars"),
    ("nube", "cloud"),
    ("nubes", "cloud"),
    ("nublado", "cloud"),
    ("cubierto", "cloud"),
    ("nube-sol", "cloud-sun"),
    ("sol-nube", "cloud-sun"),
    ("sol-nubes", "cloud-sun"),
    ("intervalos", "cloud-sun"),
    ("parcialmente-nublado", "cloud-sun"),
    ("lluvia", "cloud-rain"),
    ("nube-lluvia", "cloud-rain"),
    ("lluvioso", "cloud-rain"),
    ("chubascos", "cloud-rain"),
    ("llovizna", "cloud-rain"),
    ("tormenta", "cloud-lightning"),
    ("rayo", "lightning"),
    ("nieve", "cloud-snow"),
    ("nevada", "cloud-snow"),
    ("niebla", "cloud-fog"),
    ("viento", "wind"),
    ("temperatura", "thermometer"),
    ("calor", "thermometer-hot"),
    ("frio", "thermometer-cold"),
    ("paraguas", "umbrella"),
    ("gota", "drop"),
    ("humedad", "drop"),
    ("comida", "fork-knife"),
    ("cocina", "cooking-pot"),
    ("cocinar", "cooking-pot"),
    ("receta", "chef-hat"),
    ("huevo", "egg"),
    ("huevos", "egg"),
    ("cafe", "coffee"),
    ("vino", "wine"),
    ("cerveza", "beer-stein"),
    ("pan", "bread"),
    ("tarta", "cake"),
    ("pastel", "cake"),
    ("futbol", "soccer-ball"),
    ("balon", "soccer-ball"),
    ("baloncesto", "basketball"),
    ("tenis", "tennis-ball"),
    ("trofeo", "trophy"),
    ("copa", "trophy"),
    ("medalla", "medal"),
    ("deporte", "trophy"),
    ("correr", "person-simple-run"),
    ("bici", "bicycle"),
    ("bicicleta", "bicycle"),
    ("coche", "car"),
    ("tren", "train"),
    ("avion", "airplane"),
    ("viaje", "suitcase-rolling"),
    ("maleta", "suitcase-rolling"),
    ("mapa", "map-trifold"),
    ("ubicacion", "map-pin"),
    ("lugar", "map-pin"),
    ("montana", "mountains"),
    ("playa", "island"),
    ("musica", "music-notes"),
    ("cancion", "music-note"),
    ("pelicula", "film-slate"),
    ("cine", "film-slate"),
    ("serie", "television"),
    ("tele", "television"),
    ("libro", "book-open"),
    ("libros", "books"),
    ("juego", "game-controller"),
    ("videojuego", "game-controller"),
    ("arte", "palette"),
    ("pintura", "paint-brush"),
    ("dinero", "money"),
    ("euro", "currency-eur"),
    ("dolar", "currency-dollar"),
    ("bitcoin", "currency-btc"),
    ("banco", "bank"),
    ("precio", "tag"),
    ("compra", "shopping-cart"),
    ("tienda", "storefront"),
    ("salud", "heartbeat"),
    ("medicina", "pill"),
    ("corazon", "heart"),
    ("ciencia", "flask"),
    ("espacio", "planet"),
    ("cohete", "rocket"),
    ("casa", "house"),
    ("reloj", "clock"),
    ("hora", "clock"),
    ("calendario", "calendar"),
    ("fecha", "calendar"),
    ("persona", "user"),
    ("gente", "users"),
    ("estrella", "star"),
    ("fuego", "fire"),
    ("hoja", "leaf"),
    ("planta", "plant"),
    ("flor", "flower"),
    ("arbol", "tree"),
    ("perro", "dog"),
    ("gato", "cat"),
    ("idea", "lightbulb"),
    ("consejo", "lightbulb"),
    ("aviso", "warning"),
    ("alerta", "warning"),
    ("informacion", "info"),
    ("pregunta", "question"),
    ("codigo", "code"),
    ("ordenador", "monitor"),
    ("noticia", "newspaper"),
    ("noticias", "newspaper"),
    ("enlace", "link"),
    ("web", "globe"),
    ("mundo", "globe"),
    ("☀️", "sun"),
    ("☀", "sun"),
    ("🌤️", "cloud-sun"),
    ("⛅", "cloud-sun"),
    ("☁️", "cloud"),
    ("🌧️", "cloud-rain"),
    ("⛈️", "cloud-lightning"),
    ("❄️", "snowflake"),
    ("🌡️", "thermometer"),
    ("🍳", "cooking-pot"),
    ("🍽️", "fork-knife"),
    ("☕", "coffee"),
    ("⚽", "soccer-ball"),
    ("🏀", "basketball"),
    ("🏆", "trophy"),
    ("🎬", "film-slate"),
    ("🎮", "game-controller"),
    ("🎵", "music-notes"),
    ("📚", "books"),
    ("💡", "lightbulb"),
    ("⚠️", "warning"),
    ("💰", "money"),
    ("✈️", "airplane"),
    ("🚗", "car"),
];

/// Lo que se le enseña al modelo de la herramienta `mostrar`.
pub fn description() -> String {
    format!(
        "Enseña la respuesta como una tarjeta en la isla de Veyl, hecha a la medida de lo preguntado: es lo que el usuario ve, así que hazla \
         rica y visual, como una ficha de un buscador o de un asistente con interfaz, y elige las piezas que mejor cuenten esta respuesta \
         (una gráfica para una evolución, una tabla para comparar, pros y contras para decidir, una tira para una previsión…). Llámala una \
         vez, al final y con todo (también los enlaces y las preguntas), y después contesta con tu frase corta (la que se dice en voz alta).\n\
         Los iconos son nombres en inglés de la lista del final, tal cual: nada de emoji ni de nombres en español.\n\
         Arriba: `titulo` (corto), `subtitulo`, `icono` e `imagen` (URL https directa de una imagen que hayas visto: portada, cartel, foto del \
         producto o del lugar; si no tienes, déjala vacía y Veyl saca la del primer enlace).\n\
         `estilo` ({}): «portada» pone la imagen a sangre y el título grande debajo (para películas, juegos, lugares, personas, \
         productos), «titular» un título grande sin imagen (noticias, definiciones, un dato), «ficha» lo normal.\n\
         Los colores son los del tema del usuario: no los elijas tú. Solo puedes marcar cosas pequeñas con un `color` que signifique algo: {}.\n\
         Piezas (`bloques`, en orden, como mucho 8), cada una con su `tipo`:\n\
         - texto: `texto` (un párrafo corto).\n\
         - lista: `items` [{{titulo, detalle?, valor?, icono?, color?, imagen?}}] (como mucho 8).\n\
         - datos: `items` [{{titulo, valor}}] (pares nombre–valor, en dos columnas).\n\
         - cifra: `valor` (lo grande: «18 °C», «2 h 10 min»), `texto` (qué es) e `icono?`.\n\
         - metricas: `items` [{{titulo, valor, detalle?, icono?, color?, serie?}}] (2 a 6 cifras en mosaico; `detalle` p. ej. «+3 % hoy»; \
         `serie`, números en orden, dibuja su minigráfica).\n\
         - cuenta: `fecha` (ISO 8601, «2026-12-25T00:00:00+01:00») y `texto`: cuenta atrás (o «hace…») que Veyl mantiene al día.\n\
         - grafica: `forma` (barras, lineas o anillo), `texto?` (qué muestra), `unidad?` («€», «°C», «h») e `items` [{{titulo, numero}}] \
         (hasta 12 puntos, en orden; `numero` es un número).\n\
         - tabla: `columnas` [cabeceras, la primera la de las filas] y `items` [{{titulo, valores: [texto por columna], mejor?}}] para \
         comparar (como mucho 4 columnas y 8 filas; `mejor` es el índice del valor que gana en esa fila).\n\
         - pros: `pros` [textos] y `contras` [textos].\n\
         - tira: `items` [{{titulo, valor, icono?, detalle?}}]: fichas en fila (horarios, tallas, fases, turnos…; el tiempo va en `prevision`).\n\
         - pasos: `items` [{{titulo, detalle?}}]: pasos numerados.\n\
         - cronologia: `items` [{{valor (la fecha), titulo, detalle?}}]: una línea de tiempo.\n\
         - valoracion: `valor` (de 0 a 5, admite medios), `texto?` (de dónde: «IMDb 8,1», «Metacritic 88»).\n\
         - cita: `texto` y `valor` (quién lo dijo).\n\
         - codigo: `texto` (el código) y `valor?` (el lenguaje).\n\
         - galeria: `items` [{{imagen, titulo?, url?}}] (2 a 6 imágenes).\n\
         - etiquetas: `items` [{{titulo, icono?, color?}}]: fichas cortas (plataformas, géneros, estado).\n\
         - imagen: `url` (https, una imagen) y `texto?` (pie).\n\
         - barra: `valor` (de 0 a 1) y `texto` (qué mide).\n\
         - acciones: `items` [{{titulo, orden}}]: botones; `orden` es una orden de Veyl en español («abre Firefox», «pon un temporizador de 10 minutos»).\n\
         - enlaces: `items` [{{titulo, url}}] (solo http/https).\n\
         - preguntas: `items` [{{titulo}}]: 2 o 3 preguntas cortas que el usuario probablemente quiera hacerte después, como las diría él \
         («¿Y en PC?», «¿Cuánto cuesta?»), y que tú sepas contestar; al pulsar una, te llega con esta conversación.\n\
         - prevision: el tiempo. `valor` (la temperatura de ahora, «12°»), `texto?` (cómo está: «Nublado, sensación de 10°»), `icono?` e \
         `items` [{{titulo (día u hora), icono, min, max (números), detalle? (lluvia: «60 %»)}}], 2 a 10.\n\
         - marcador: un resultado. `texto` (estado: «Final», «En juego · 67'») e `items` [{{titulo (equipo o jugador), valor (su \
         marcador), imagen? (escudo), detalle?}}], exactamente 2.\n\
         - versus: dos frente a frente. `columnas` [nombre A, nombre B], `imagenes?` [foto A, foto B] e `items` [{{titulo (qué se \
         compara), valores: [A, B], mejor? (0 o 1)}}]; con una cifra en cada valor («8 GB», «959 €») se dibujan barras enfrentadas.\n\
         - medidor: anillos de progreso. `items` [{{titulo, numero, maximo? (de fábrica, 100), valor? (lo que se lee dentro: «8,1»), \
         color?}}], 1 a 4 (batería, calidad del aire, una nota sobre 10, humedad).\n\
         - progreso: barras horizontales con nombre. `items` [{{titulo, numero, maximo?, valor? (lo que se lee: «120 g»), color?}}] \
         (macros, presupuesto, reparto), hasta 8.\n\
         - checklist: casillas que se marcan. `items` [{{titulo, valor? (cantidad: «200 g»), detalle?}}] (ingredientes, qué llevar), \
         hasta 12.\n\
         - definicion: `texto` (el término, la palabra misma), `valor?` (categoría o pronunciación: «sustantivo femenino») e `items` \
         [{{titulo (un significado), detalle? (un ejemplo de uso)}}], hasta 4.\n\
         - aviso: solo si hay algo que de verdad no debe pasarse por alto (un riesgo, un plazo, una advertencia); no en todas las \
         tarjetas. `valor?` (título corto), `texto`, `icono?` y `color?` (bien, aviso, mal).\n\
         - paleta: colores. `items` [{{titulo?, valor: «#RRGGBB»}}], 2 a 8.\n\
         Iconos que existen: {}.",
        STYLES.join(", "),
        COLORS.join(", "),
        ICONS.join(", ")
    )
}

pub fn schema() -> Value {
    let item = json!({
        "type": "object",
        "properties": {
            "titulo": { "type": "string" },
            "detalle": { "type": "string" },
            "valor": { "type": "string" },
            "numero": { "type": "number" },
            "valores": { "type": "array", "items": { "type": "string" } },
            "mejor": { "type": "integer" },
            "icono": { "type": "string" },
            "orden": { "type": "string" },
            "url": { "type": "string" },
            "color": { "type": "string", "enum": COLORS },
            "imagen": { "type": "string" },
            "min": { "type": "number" },
            "max": { "type": "number" },
            "maximo": { "type": "number" },
            "serie": { "type": "array", "items": { "type": "number" } }
        }
    });
    json!({
        "type": "object",
        "properties": {
            "titulo": { "type": "string", "description": "Corto: de qué va la tarjeta" },
            "subtitulo": { "type": "string" },
            "icono": { "type": "string" },
            "imagen": { "type": "string" },
            "estilo": { "type": "string", "enum": STYLES },
            "bloques": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "tipo": { "type": "string", "enum": KINDS },
                        "texto": { "type": "string" },
                        "fecha": { "type": "string" },
                        "url": { "type": "string" },
                        "icono": { "type": "string" },
                        "forma": { "type": "string", "enum": ["barras", "lineas", "anillo"] },
                        "unidad": { "type": "string" },
                        "valor": { "type": ["string", "number"] },
                        "columnas": { "type": "array", "items": { "type": "string" } },
                        "imagenes": { "type": "array", "items": { "type": "string" } },
                        "pros": { "type": "array", "items": { "type": "string" } },
                        "contras": { "type": "array", "items": { "type": "string" } },
                        "items": { "type": "array", "items": item }
                    },
                    "required": ["tipo"]
                }
            }
        },
        "required": ["titulo", "bloques"]
    })
}

/// Las piezas que Veyl sabe pintar.
pub const KINDS: &[&str] = &[
    "texto",
    "lista",
    "datos",
    "cifra",
    "metricas",
    "cuenta",
    "grafica",
    "tabla",
    "pros",
    "tira",
    "pasos",
    "cronologia",
    "valoracion",
    "cita",
    "codigo",
    "galeria",
    "etiquetas",
    "imagen",
    "barra",
    "acciones",
    "enlaces",
    "preguntas",
    "prevision",
    "marcador",
    "versus",
    "medidor",
    "progreso",
    "checklist",
    "definicion",
    "aviso",
    "paleta",
];

fn cut(value: Option<&Value>, max: usize) -> String {
    let text = match value {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    };
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() > max { format!("{}…", text.chars().take(max - 1).collect::<String>().trim_end()) } else { text }
}

/// Un texto en una línea y, si es largo, cortado con «…».
pub fn clip(text: &str, max: usize) -> String {
    cut(Some(&json!(text)), max)
}

/// Un icono que existe: el nombre tal cual o, si el modelo lo ha escrito a su
/// manera («Nube lluvia», «cloud-rain-fill», «ph-sun», «☀️»), el que quería.
fn icon(value: Option<&Value>) -> String {
    let raw = value.and_then(Value::as_str).unwrap_or("").trim();
    if ICONS.contains(&raw) {
        return raw.to_string();
    }
    let folded: String = raw
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' => 'a',
            'é' | 'è' => 'e',
            'í' => 'i',
            'ó' | 'ò' => 'o',
            'ú' | 'ü' => 'u',
            'ñ' => 'n',
            ' ' | '_' => '-',
            c => c,
        })
        .collect();
    let name = folded.strip_prefix("ph-").unwrap_or(&folded);
    let name = ["-fill", "-bold", "-light", "-thin", "-duotone", "-regular"].iter().fold(name, |n, end| n.strip_suffix(end).unwrap_or(n));
    if let Some(found) = ICONS.iter().find(|i| **i == name) {
        return found.to_string();
    }
    ALIASES.iter().find(|(alias, _)| *alias == name || *alias == raw).map(|(_, icon)| icon.to_string()).unwrap_or_default()
}

/// Una máxima y una mínima escritas juntas: «8° / 18°», «-2 / 5 ºC», «12–20°».
fn temperatures(text: &str) -> Option<(f64, f64)> {
    if !text.contains(['°', 'º']) {
        return None;
    }
    let (a, b) = text.split_once('/').or_else(|| text.split_once('–')).or_else(|| text.split_once(" - "))?;
    let (a, b) = (first_number(a)?, first_number(b)?);
    Some((a.min(b), a.max(b)))
}

/// El primer porcentaje de un texto: «Chubascos, lluvia 90 %» → «90 %».
fn percent(text: &str) -> String {
    let Some(at) = text.find('%') else { return String::new() };
    let digits: String = text[..at].trim_end().chars().rev().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() { String::new() } else { format!("{} %", digits.chars().rev().collect::<String>()) }
}

/// Una `tira` que en realidad es el tiempo (cada ficha, una mínima y una
/// máxima) se pinta como `prevision`.
fn as_forecast(block: &Value) -> Option<Value> {
    let items = block.get("items")?.as_array()?;
    let days: Vec<Value> = items
        .iter()
        .map(|i| {
            let (low, high) = temperatures(i.get("valor")?.as_str()?)?;
            Some(json!({ "titulo": i.get("titulo"), "icono": i.get("icono"), "min": low, "max": high, "detalle": percent(i.get("detalle").and_then(Value::as_str).unwrap_or("")) }))
        })
        .collect::<Option<_>>()?;
    (days.len() >= 2).then(|| json!({ "tipo": "prevision", "items": days }))
}

/// Las piezas que van al pie de una tarjeta.
const TAIL: &[&str] = &["enlaces", "acciones", "preguntas"];

/// Si el modelo manda otra tarjeta solo con lo del pie (las preguntas que se
/// le olvidaron, los enlaces), se le añade a la que ya había en vez de taparla.
pub fn merge(old: &Value, new: &Value) -> Option<Value> {
    let blocks = new["bloques"].as_array()?;
    if blocks.is_empty() || !blocks.iter().all(|b| b["tipo"].as_str().is_some_and(|k| TAIL.contains(&k))) {
        return None;
    }
    let mut merged = old.clone();
    let kinds: Vec<&Value> = blocks.iter().map(|b| &b["tipo"]).collect();
    let mut all: Vec<Value> = old["bloques"].as_array()?.iter().filter(|b| !kinds.contains(&&b["tipo"])).cloned().collect();
    all.extend(blocks.iter().cloned());
    merged["bloques"] = json!(all);
    Some(merged)
}

fn color(value: Option<&Value>) -> String {
    value.and_then(Value::as_str).map(str::trim).filter(|c| COLORS.contains(c)).unwrap_or("").to_string()
}

/// Uno de los de una lista cerrada (un estilo), o vacío.
fn one_of(value: Option<&Value>, list: &[&str]) -> String {
    value.and_then(Value::as_str).map(|v| v.trim().to_lowercase().replace('á', "a")).filter(|v| list.contains(&v.as_str())).unwrap_or_default()
}

/// La primera cifra de un texto: «12 GB / 256 GB» → 12; «1.234,5 €» → 1234.5.
fn first_number(text: &str) -> Option<f64> {
    let start = text.find(|c: char| c.is_ascii_digit())?;
    let negative = text[..start].trim_end().ends_with('-');
    let run: String = text[start..].chars().take_while(|c| c.is_ascii_digit() || matches!(c, '.' | ',')).collect();
    let run = run.trim_end_matches(['.', ',']);
    number(Some(&json!(run))).map(|n| if negative { -n } else { n })
}

/// «#0af» o «#00AAFF» → «#00AAFF»; lo demás, vacío.
fn hex(value: Option<&Value>) -> String {
    let text = value.and_then(Value::as_str).unwrap_or("").trim().trim_start_matches('#');
    let full = match text.len() {
        3 => text.chars().flat_map(|c| [c, c]).collect(),
        6 => text.to_string(),
        _ => return String::new(),
    };
    if full.chars().all(|c| c.is_ascii_hexdigit()) { format!("#{}", full.to_uppercase()) } else { String::new() }
}

/// Los números de una lista, en orden (los que no lo son, fuera).
fn numbers(value: Option<&Value>, most: usize) -> Vec<f64> {
    value.and_then(Value::as_array).into_iter().flatten().filter_map(|v| number(Some(v))).take(most).collect()
}

fn web(value: Option<&Value>) -> String {
    let url = value.and_then(Value::as_str).unwrap_or("").trim();
    if (url.starts_with("https://") || url.starts_with("http://")) && url.len() <= 1000 && !url.contains(char::is_whitespace) {
        url.to_string()
    } else {
        String::new()
    }
}

/// Un número, venga como número o como texto («1.234,5 €», «40%»).
fn number(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => {
            let digits: String = s.chars().filter(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | '-')).collect();
            let digits = if digits.contains(',') { digits.replace('.', "").replace(',', ".") } else { digits };
            digits.parse().ok()
        }
        _ => None,
    }
    .filter(|n: &f64| n.is_finite())
}

fn texts(value: Option<&Value>, most: usize, max: usize) -> Vec<String> {
    value.and_then(Value::as_array).into_iter().flatten().take(most).map(|v| cut(Some(v), max)).filter(|t| !t.is_empty()).collect()
}

/// La tarjeta, limpia; `None` si no queda nada que enseñar.
pub fn clean(input: &Value) -> Option<Value> {
    let title = cut(input.get("titulo"), 60);
    let mut blocks = Vec::new();
    for block in input.get("bloques").and_then(Value::as_array).into_iter().flatten().take(8) {
        let forecast = (block.get("tipo").and_then(Value::as_str) == Some("tira")).then(|| as_forecast(block)).flatten();
        let block = forecast.as_ref().unwrap_or(block);
        let kind = block.get("tipo").and_then(Value::as_str).unwrap_or("");
        let items = || block.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
        let some = |list: Vec<Value>, least: usize, piece: Value| (list.len() >= least).then_some(piece);
        let cleaned = match kind {
            "texto" => {
                let text = cut(block.get("texto"), 400);
                (!text.is_empty()).then(|| json!({ "tipo": "texto", "texto": text }))
            }
            "lista" | "datos" | "metricas" | "tira" | "pasos" | "cronologia" => {
                let most = match kind {
                    "metricas" => 6,
                    _ => 8,
                };
                let rows: Vec<Value> = items()
                    .iter()
                    .take(most)
                    .map(|i| {
                        let mut row = json!({
                            "titulo": cut(i.get("titulo"), 90),
                            "detalle": cut(i.get("detalle"), 160),
                            "valor": cut(i.get("valor"), if kind == "datos" { 60 } else { 30 }),
                            "icono": icon(i.get("icono")),
                            "color": color(i.get("color")),
                            "imagen": web(i.get("imagen"))
                        });
                        let serie = numbers(i.get("serie"), 24);
                        if kind == "metricas" && serie.len() >= 2 {
                            row["serie"] = json!(serie);
                        }
                        row
                    })
                    .filter(|i| i["titulo"] != "" && (!matches!(kind, "datos" | "metricas" | "tira") || i["valor"] != ""))
                    .collect();
                let least = if kind == "metricas" { 2 } else { 1 };
                some(rows.clone(), least, json!({ "tipo": kind, "items": rows }))
            }
            "cifra" => {
                let value = cut(block.get("valor"), 24);
                (!value.is_empty()).then(|| json!({ "tipo": "cifra", "valor": value, "texto": cut(block.get("texto"), 80), "icono": icon(block.get("icono")) }))
            }
            "cuenta" => {
                let date = cut(block.get("fecha"), 40);
                let valid = chrono::DateTime::parse_from_rfc3339(&date).is_ok()
                    || chrono::NaiveDateTime::parse_from_str(&date, "%Y-%m-%dT%H:%M:%S").is_ok()
                    || chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d").is_ok();
                valid.then(|| json!({ "tipo": "cuenta", "fecha": date, "texto": cut(block.get("texto"), 80) }))
            }
            "grafica" => {
                let shape = match block.get("forma").and_then(Value::as_str) {
                    Some("lineas") => "lineas",
                    Some("anillo") => "anillo",
                    _ => "barras",
                };
                let points: Vec<Value> = items()
                    .iter()
                    .take(12)
                    .filter_map(|i| {
                        let n = number(i.get("numero")).or_else(|| number(i.get("valor")))?;
                        (shape != "anillo" || n > 0.0).then(|| json!({ "titulo": cut(i.get("titulo"), 24), "numero": n }))
                    })
                    .collect();
                let least = if shape == "barras" { 1 } else { 2 };
                some(
                    points.clone(),
                    least,
                    json!({ "tipo": "grafica", "forma": shape, "texto": cut(block.get("texto"), 80), "unidad": cut(block.get("unidad"), 8), "items": points }),
                )
            }
            "tabla" => {
                // La primera cabecera puede ir vacía (la de las filas): no se quita.
                let columns: Vec<String> = block.get("columnas").and_then(Value::as_array).into_iter().flatten().take(4).map(|v| cut(Some(v), 24)).collect();
                let wide = columns.len().max(2);
                let rows: Vec<Value> = items()
                    .iter()
                    .take(8)
                    .map(|i| {
                        let values = texts(i.get("valores"), wide - 1, 40);
                        let best = i.get("mejor").and_then(Value::as_i64).filter(|b| (*b as usize) < values.len()).unwrap_or(-1);
                        json!({ "titulo": cut(i.get("titulo"), 40), "valores": values, "mejor": best })
                    })
                    .filter(|r| r["titulo"] != "" && r["valores"].as_array().is_some_and(|v| !v.is_empty()))
                    .collect();
                some(rows.clone(), 1, json!({ "tipo": "tabla", "columnas": columns, "items": rows }))
            }
            "pros" => {
                let pros = texts(block.get("pros"), 6, 90);
                let cons = texts(block.get("contras"), 6, 90);
                (!pros.is_empty() || !cons.is_empty()).then(|| json!({ "tipo": "pros", "pros": pros, "contras": cons }))
            }
            "valoracion" => number(block.get("valor"))
                .map(|v| if v > 5.0 { v / 2.0 } else { v })
                .map(|v| json!({ "tipo": "valoracion", "valor": (v.clamp(0.0, 5.0) * 2.0).round() / 2.0, "texto": cut(block.get("texto"), 60) })),
            "cita" => {
                let text = cut(block.get("texto"), 300);
                (!text.is_empty()).then(|| json!({ "tipo": "cita", "texto": text, "valor": cut(block.get("valor"), 60) }))
            }
            "codigo" => {
                let code = block.get("texto").and_then(Value::as_str).unwrap_or("").trim_end();
                let code: String = code.lines().take(20).map(|l| l.chars().take(90).collect::<String>()).collect::<Vec<_>>().join("\n");
                (!code.trim().is_empty()).then(|| json!({ "tipo": "codigo", "texto": code, "valor": cut(block.get("valor"), 20) }))
            }
            "galeria" => {
                let shots: Vec<Value> = items()
                    .iter()
                    .take(6)
                    .map(|i| json!({ "imagen": web(i.get("imagen")), "titulo": cut(i.get("titulo"), 40), "url": web(i.get("url")) }))
                    .filter(|i| i["imagen"] != "")
                    .collect();
                some(shots.clone(), 2, json!({ "tipo": "galeria", "items": shots }))
            }
            "etiquetas" => {
                let tags: Vec<Value> = items()
                    .iter()
                    .take(8)
                    .map(|i| json!({ "titulo": cut(i.get("titulo"), 30), "icono": icon(i.get("icono")), "color": color(i.get("color")) }))
                    .filter(|i| i["titulo"] != "")
                    .collect();
                some(tags.clone(), 1, json!({ "tipo": "etiquetas", "items": tags }))
            }
            "imagen" => {
                let url = web(block.get("url"));
                (!url.is_empty()).then(|| json!({ "tipo": "imagen", "url": url, "texto": cut(block.get("texto"), 120) }))
            }
            "barra" => number(block.get("valor"))
                .map(|v| if v > 1.0 { v / 100.0 } else { v })
                .map(|v| json!({ "tipo": "barra", "valor": v.clamp(0.0, 1.0), "texto": cut(block.get("texto"), 80) })),
            "acciones" => {
                let buttons: Vec<Value> = items()
                    .iter()
                    .take(4)
                    .map(|i| json!({ "titulo": cut(i.get("titulo"), 28), "orden": cut(i.get("orden"), 120) }))
                    .filter(|i| i["titulo"] != "" && i["orden"] != "")
                    .collect();
                some(buttons.clone(), 1, json!({ "tipo": "acciones", "items": buttons }))
            }
            "preguntas" => {
                // Una pregunta puede llegar como texto suelto en vez de {titulo}.
                let asks: Vec<Value> =
                    items().iter().take(3).map(|i| json!({ "titulo": cut(i.get("titulo").or(Some(i)), 80) })).filter(|i| i["titulo"] != "").collect();
                some(asks.clone(), 1, json!({ "tipo": "preguntas", "items": asks }))
            }
            "prevision" => {
                let days: Vec<Value> = items()
                    .iter()
                    .take(10)
                    .filter_map(|i| {
                        let (low, high) = (number(i.get("min"))?, number(i.get("max"))?);
                        let (low, high) = if low <= high { (low, high) } else { (high, low) };
                        let title = cut(i.get("titulo"), 14);
                        (!title.is_empty())
                            .then(|| json!({ "titulo": title, "icono": icon(i.get("icono")), "min": low, "max": high, "detalle": cut(i.get("detalle"), 12) }))
                    })
                    .collect();
                some(
                    days.clone(),
                    2,
                    json!({ "tipo": "prevision", "valor": cut(block.get("valor"), 12), "texto": cut(block.get("texto"), 80), "icono": icon(block.get("icono")), "unidad": cut(block.get("unidad"), 4), "items": days }),
                )
            }
            "marcador" => {
                let sides: Vec<Value> = items()
                    .iter()
                    .take(2)
                    .map(|i| json!({ "titulo": cut(i.get("titulo"), 28), "valor": cut(i.get("valor"), 8), "imagen": web(i.get("imagen")), "detalle": cut(i.get("detalle"), 40) }))
                    .filter(|s| s["titulo"] != "")
                    .collect();
                (sides.len() == 2).then(|| json!({ "tipo": "marcador", "texto": cut(block.get("texto"), 40), "items": sides }))
            }
            "versus" => {
                // Como en una tabla, la primera cabecera puede venir vacía.
                let names: Vec<String> =
                    block.get("columnas").and_then(Value::as_array).into_iter().flatten().map(|v| cut(Some(v), 28)).filter(|n| !n.is_empty()).take(2).collect();
                let mut pictures: Vec<String> = block.get("imagenes").and_then(Value::as_array).into_iter().flatten().take(2).map(|v| web(Some(v))).collect();
                pictures.resize(2, String::new());
                let rows: Vec<Value> = items()
                    .iter()
                    .take(8)
                    .filter_map(|i| {
                        let values = texts(i.get("valores"), 2, 30);
                        let title = cut(i.get("titulo"), 40);
                        if values.len() < 2 || title.is_empty() {
                            return None;
                        }
                        let figures: Vec<f64> = values.iter().filter_map(|v| first_number(v)).collect();
                        let best = i.get("mejor").and_then(Value::as_i64).filter(|b| (0..2).contains(b)).unwrap_or(-1);
                        Some(json!({ "titulo": title, "valores": values, "numeros": if figures.len() == 2 { figures } else { Vec::new() }, "mejor": best }))
                    })
                    .collect();
                (names.len() == 2 && !rows.is_empty()).then(|| json!({ "tipo": "versus", "columnas": names, "imagenes": pictures, "items": rows }))
            }
            "medidor" | "progreso" => {
                let rows: Vec<Value> = items()
                    .iter()
                    .take(if kind == "medidor" { 4 } else { 8 })
                    .filter_map(|i| {
                        let n = number(i.get("numero")).or_else(|| number(i.get("valor")))?;
                        let most = number(i.get("maximo")).filter(|m| *m > 0.0).unwrap_or(if n > 1.0 { 100.0 } else { 1.0 });
                        let title = cut(i.get("titulo"), 30);
                        (!title.is_empty() || kind == "medidor").then(|| {
                            json!({ "titulo": title, "numero": n.max(0.0), "maximo": most, "valor": cut(i.get("valor"), 14), "color": color(i.get("color")) })
                        })
                    })
                    .collect();
                some(rows.clone(), 1, json!({ "tipo": kind, "items": rows }))
            }
            "checklist" => {
                let rows: Vec<Value> = items()
                    .iter()
                    .take(12)
                    .map(|i| json!({ "titulo": cut(i.get("titulo").or(Some(i)), 90), "valor": cut(i.get("valor"), 24), "detalle": cut(i.get("detalle"), 120) }))
                    .filter(|i| i["titulo"] != "")
                    .collect();
                some(rows.clone(), 1, json!({ "tipo": "checklist", "items": rows }))
            }
            "definicion" => {
                let mut senses: Vec<Value> = items()
                    .iter()
                    .take(4)
                    .map(|i| json!({ "titulo": cut(i.get("titulo").or(Some(i)), 220), "detalle": cut(i.get("detalle"), 160) }))
                    .filter(|i| i["titulo"] != "")
                    .collect();
                // Sin acepciones, `texto` es el significado y el término, el título de la tarjeta.
                let mut term = cut(block.get("texto"), 40);
                if senses.is_empty() {
                    let meaning = cut(block.get("texto"), 220);
                    if !meaning.is_empty() {
                        senses.push(json!({ "titulo": meaning, "detalle": "" }));
                    }
                    term = String::new();
                }
                if term.is_empty() {
                    term = cut(input.get("titulo"), 40);
                }
                (!term.is_empty() && !senses.is_empty())
                    .then(|| json!({ "tipo": "definicion", "texto": term, "valor": cut(block.get("valor"), 60), "items": senses }))
            }
            "aviso" => {
                let text = cut(block.get("texto"), 300);
                (!text.is_empty()).then(|| {
                    json!({ "tipo": "aviso", "texto": text, "valor": cut(block.get("valor"), 50), "icono": icon(block.get("icono")), "color": color(block.get("color")) })
                })
            }
            "paleta" => {
                let swatches: Vec<Value> = items()
                    .iter()
                    .take(8)
                    .map(|i| json!({ "titulo": cut(i.get("titulo"), 24), "valor": hex(i.get("valor").or(Some(i))) }))
                    .filter(|i| i["valor"] != "")
                    .collect();
                some(swatches.clone(), 2, json!({ "tipo": "paleta", "items": swatches }))
            }
            "enlaces" => {
                let links: Vec<Value> = items()
                    .iter()
                    .take(5)
                    .filter_map(|i| {
                        let url = web(i.get("url"));
                        (!url.is_empty()).then(|| json!({ "titulo": cut(i.get("titulo"), 70), "url": url }))
                    })
                    .collect();
                some(links.clone(), 1, json!({ "tipo": "enlaces", "items": links }))
            }
            _ => None,
        };
        blocks.extend(cleaned);
    }
    // Sin piezas no hay nada que enseñar: solo un título («x») tapaba la tarjeta buena de antes.
    if blocks.is_empty() {
        return None;
    }
    Some(json!({
        "titulo": title,
        "subtitulo": cut(input.get("subtitulo"), 100),
        "icono": icon(input.get("icono")),
        "imagen": web(input.get("imagen")),
        "estilo": one_of(input.get("estilo"), STYLES),
        "bloques": blocks
    }))
}

/// Los enlaces de una tarjeta, de donde sacar su imagen si no trae.
pub fn links(card: &Value) -> Vec<String> {
    let blocks = card["bloques"].as_array().cloned().unwrap_or_default();
    blocks
        .iter()
        .filter(|b| b["tipo"] == "enlaces")
        .flat_map(|b| b["items"].as_array().cloned().unwrap_or_default())
        .filter_map(|i| i["url"].as_str().map(String::from))
        .collect()
}

/// La imagen con la que una página se presenta (`og:image` o `twitter:image`),
/// leyendo solo su cabeza y con poco tiempo: si tarda, la tarjeta va sin ella.
pub fn picture(page: &str) -> Option<String> {
    let response = ureq::get(page)
        .timeout(Duration::from_millis(2500))
        .set("User-Agent", "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36")
        .set("Accept", "text/html")
        .call()
        .ok()?;
    let mut head = String::new();
    response.into_reader().take(400_000).read_to_string(&mut head).ok();
    let found = meta_image(&head)?;
    let found = found.replace("&amp;", "&");
    let url = if found.starts_with("//") {
        format!("https:{found}")
    } else if found.starts_with('/') {
        let origin: String = page.splitn(4, '/').take(3).collect::<Vec<_>>().join("/");
        format!("{origin}{found}")
    } else {
        found
    };
    (!web(Some(&json!(url))).is_empty()).then_some(url)
}

fn meta_image(html: &str) -> Option<String> {
    for key in ["og:image", "twitter:image"] {
        let mut from = 0;
        while let Some(at) = html[from..].find(key) {
            let at = from + at;
            from = at + key.len();
            let start = html[..at].rfind('<')?;
            let end = at + html[at..].find('>')?;
            let tag = &html[start..end];
            if !tag.starts_with("<meta") {
                continue;
            }
            let Some(content) = tag.find("content=") else { continue };
            let rest = &tag[content + 8..];
            let quote = rest.chars().next()?;
            if quote != '"' && quote != '\'' {
                continue;
            }
            let value = &rest[1..];
            if let Some(close) = value.find(quote) {
                let value = value[..close].trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

/// Una tarjeta hecha con la respuesta escrita, para cuando el modelo no ha
/// compuesto la suya: la primera frase de título y lo demás, en párrafos.
pub fn from_text(text: &str) -> Value {
    let text = text.trim();
    let first = text.lines().next().unwrap_or("");
    let (title, rest) = match first.find(". ") {
        Some(at) if at < 90 => (&first[..=at], format!("{}{}", &first[at + 2..], &text[first.len()..])),
        _ if first.chars().count() <= 90 => (first, text[first.len()..].to_string()),
        _ => ("", text.to_string()),
    };
    let blocks: Vec<Value> =
        rest.split("\n").map(str::trim).filter(|p| !p.is_empty()).take(8).map(|p| json!({ "tipo": "texto", "texto": cut(Some(&json!(p)), 400) })).collect();
    json!({ "titulo": cut(Some(&json!(title)), 90), "subtitulo": "", "icono": "sparkle", "color": "", "imagen": "", "bloques": blocks })
}

/// Una tarjeta hecha con lo que contesta la gramática en varias líneas (la
/// agenda, el diario, los agentes): `say` de título y cada línea, una fila.
/// Las que empiezan por una hora («09:22–09:30  kitty · ~/dev») son una
/// cronología; las que acaban en un rato («Firefox, 2 h 9 min»), una lista
/// con el rato a la derecha; «nombre: lo demás» y «nombre · lo demás», con
/// su detalle. `None` si es una sola línea corta: para eso basta la notificación.
pub fn from_answer(say: &str, icon: &str, body: &str) -> Option<Value> {
    let lines: Vec<&str> = body.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    if lines.len() < 2 && lines.first().is_none_or(|l| l.chars().count() <= 110) {
        return None;
    }
    let lasting = |tail: &str| {
        let tail = tail.trim();
        tail.chars().next().is_some_and(|c| c.is_ascii_digit())
            && tail.split_whitespace().all(|w| w.chars().all(|c| c.is_ascii_digit()) || matches!(w, "h" | "min" | "s" | "d"))
    };
    let at = |line: &str| -> Option<(String, String)> {
        let (time, rest) = line.split_once("  ")?;
        let clock = time.chars().take(5).collect::<String>();
        (clock.len() == 5 && clock.as_bytes()[2] == b':' && clock.chars().filter(char::is_ascii_digit).count() == 4)
            .then(|| (time.trim().to_string(), rest.trim().to_string()))
    };
    let row = |line: &str| -> Value {
        if let Some((name, time)) = line.rsplit_once(", ").filter(|(_, t)| lasting(t)) {
            let (title, detail) = name.split_once(" · ").unwrap_or((name, ""));
            return json!({ "titulo": title, "detalle": detail, "valor": time });
        }
        if let Some((title, detail)) = line.split_once(": ").filter(|(t, _)| t.chars().count() <= 60) {
            return json!({ "titulo": title, "detalle": detail });
        }
        if let Some((title, detail)) = line.split_once(" · ") {
            return json!({ "titulo": title, "detalle": detail });
        }
        json!({ "titulo": line })
    };
    let timeline = lines.iter().filter(|l| at(l).is_some()).count() * 2 > lines.len();
    let rows: Vec<Value> = if timeline {
        lines
            .iter()
            .filter_map(|l| at(l))
            .map(|(time, rest)| {
                let (title, detail) = rest.split_once(" · ").unwrap_or((&rest, ""));
                json!({ "valor": time, "titulo": title, "detalle": detail })
            })
            .collect()
    } else {
        lines.iter().map(|l| row(l)).collect()
    };
    // De ocho en ocho: cada pieza lleva como mucho ocho filas.
    let kind = if timeline { "cronologia" } else { "lista" };
    let blocks: Vec<Value> = rows.chunks(8).take(4).map(|c| json!({ "tipo": kind, "items": c })).collect();
    clean(&json!({ "titulo": say, "icono": icon, "bloques": blocks }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_alone_is_no_card() {
        assert!(clean(&json!({ "titulo": "x", "bloques": [] })).is_none());
        assert!(clean(&json!({ "titulo": "x", "bloques": [{ "tipo": "texto", "texto": "" }] })).is_none());
    }

    #[test]
    fn grammar_answers_become_cards() {
        assert!(from_answer("Mañana, nada apuntado", "calendar", "").is_none());
        let time = from_answer("Hoy: 2 h 1 min de pantalla", "hourglass", "kitty · nixos-pc: veyl, 1 h 4 min\nFirefox · Inicio / X, 8 min").unwrap();
        assert_eq!(time["bloques"][0]["tipo"], "lista");
        assert_eq!(time["bloques"][0]["items"][0]["titulo"], "kitty");
        assert_eq!(time["bloques"][0]["items"][0]["valor"], "1 h 4 min");
        let day: Vec<String> = (0..12).map(|i| format!("{:02}:00–{:02}:30  kitty · ~/dev/veyl · claude", 8 + i, 8 + i)).collect();
        let day = from_answer("Hoy, rato a rato", "list-bullets", &day.join("\n")).unwrap();
        let blocks = day["bloques"].as_array().unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0]["tipo"], "cronologia");
        assert_eq!(blocks[0]["items"][0]["valor"], "08:00–08:30");
        assert_eq!(blocks[0]["items"][0]["detalle"], "~/dev/veyl · claude");
        let agents = from_answer("2 agentes trabajando", "robot", "Claude en «Daemon»: trabaja desde hace 2 min\nClaude en «UI»: trabaja").unwrap();
        assert_eq!(agents["bloques"][0]["items"][1]["detalle"], "trabaja");
    }

    #[test]
    fn cleaning_keeps_what_veyl_can_draw() {
        let card = clean(&json!({
            "titulo": "El tiempo mañana",
            "icono": "sun",
            "bloques": [
                { "tipo": "cifra", "valor": 18, "texto": "máxima" },
                { "tipo": "barra", "valor": "40%", "texto": "lluvia" },
                { "tipo": "lista", "items": [{ "titulo": "Mañana", "valor": "12 °C", "icono": "no-existe" }, { "detalle": "sin título" }] },
                { "tipo": "enlaces", "items": [{ "titulo": "AEMET", "url": "https://aemet.es" }, { "titulo": "malo", "url": "file:///etc/passwd" }] },
                { "tipo": "acciones", "items": [{ "titulo": "Paraguas", "orden": "apunta coger el paraguas mañana a las 8" }] },
                { "tipo": "preguntas", "items": [{ "titulo": "¿Y pasado mañana?" }, "¿Llueve en Bilbao?", { "titulo": "" }, "a", "b"] },
                { "tipo": "html", "texto": "<script>" }
            ]
        }))
        .unwrap();
        let blocks = card["bloques"].as_array().unwrap();
        assert_eq!(blocks.iter().map(|b| b["tipo"].as_str().unwrap()).collect::<Vec<_>>(), ["cifra", "barra", "lista", "enlaces", "acciones", "preguntas"]);
        assert_eq!(blocks[5]["items"], json!([{ "titulo": "¿Y pasado mañana?" }, { "titulo": "¿Llueve en Bilbao?" }]));
        assert_eq!(blocks[0]["valor"], "18");
        assert_eq!(blocks[1]["valor"], 0.4);
        assert_eq!(blocks[2]["items"].as_array().unwrap().len(), 1);
        assert_eq!(blocks[2]["items"][0]["icono"], "");
        assert_eq!(blocks[3]["items"].as_array().unwrap().len(), 1);
        assert!(clean(&json!({ "bloques": [] })).is_none());
    }

    #[test]
    fn charts_tables_and_more() {
        let card = clean(&json!({
            "titulo": "Bitcoin",
            "bloques": [
                { "tipo": "grafica", "forma": "lineas", "unidad": "€", "items": [{ "titulo": "lun", "numero": 81000 }, { "titulo": "mar", "valor": "82.350,5 €" }, { "titulo": "mié", "valor": "?" }] },
                { "tipo": "grafica", "forma": "anillo", "items": [{ "titulo": "solo", "numero": 3 }] },
                { "tipo": "tabla", "columnas": ["", "iPhone", "Pixel"], "items": [{ "titulo": "Precio", "valores": ["959 €", "899 €", "sobra"], "mejor": 1 }, { "titulo": "Vacía", "valores": [] }] },
                { "tipo": "pros", "pros": ["Rápido"], "contras": [] },
                { "tipo": "valoracion", "valor": "8,1" },
                { "tipo": "metricas", "items": [{ "titulo": "Solo una", "valor": "1" }] },
                { "tipo": "codigo", "texto": "fn main() {}\n", "valor": "rust" },
                { "tipo": "galeria", "items": [{ "imagen": "https://a/1.jpg" }, { "imagen": "ftp://b" }] }
            ]
        }))
        .unwrap();
        let blocks = card["bloques"].as_array().unwrap();
        assert_eq!(blocks.iter().map(|b| b["tipo"].as_str().unwrap()).collect::<Vec<_>>(), ["grafica", "tabla", "pros", "valoracion", "codigo"]);
        assert_eq!(blocks[0]["items"].as_array().unwrap().len(), 2);
        assert_eq!(blocks[0]["items"][1]["numero"], 82350.5);
        assert_eq!(blocks[1]["items"].as_array().unwrap().len(), 1);
        assert_eq!(blocks[1]["items"][0]["valores"].as_array().unwrap().len(), 2);
        assert_eq!(blocks[1]["items"][0]["mejor"], 1);
        assert_eq!(blocks[3]["valor"], 4.0);
    }

    #[test]
    fn art_direction_and_new_pieces() {
        let card = clean(&json!({
            "titulo": "Madrid", "tono": "cielo", "estilo": "Portada",
            "bloques": [
                { "tipo": "prevision", "valor": "12°", "icono": "cloud", "items": [
                    { "titulo": "lun", "icono": "sun", "min": 15, "max": 7, "detalle": "10 %" },
                    { "titulo": "mar", "min": "6", "max": "13 °C" },
                    { "titulo": "mié", "min": 5 }
                ] },
                { "tipo": "marcador", "texto": "Final", "items": [{ "titulo": "Real Madrid", "valor": "2" }, { "titulo": "Barça", "valor": 1 }] },
                { "tipo": "versus", "columnas": ["", "iPhone 17", "Pixel 10"], "imagenes": ["https://a/i.jpg"], "items": [
                    { "titulo": "RAM", "valores": ["12 GB / 256 GB", "16 GB"], "mejor": 1 },
                    { "titulo": "Sistema", "valores": ["iOS", "Android"], "mejor": 7 },
                    { "titulo": "Solo uno", "valores": ["x"] }
                ] },
                { "tipo": "medidor", "items": [{ "titulo": "Batería", "numero": 72 }, { "titulo": "Nota", "numero": 8.1, "maximo": 10, "valor": "8,1" }, { "titulo": "x" }] },
                { "tipo": "checklist", "items": [{ "titulo": "Harina", "valor": "200 g" }, "Huevos", { "valor": "sin nombre" }] },
                { "tipo": "aviso", "valor": "Ojo", "texto": "Lleva paraguas", "color": "aviso", "icono": "umbrella" },
                { "tipo": "paleta", "items": [{ "titulo": "Mar", "valor": "#0af" }, { "valor": "00AAFF" }, { "valor": "azul" }] },
                { "tipo": "definicion", "texto": "serendipia", "valor": "sustantivo femenino", "items": [{ "titulo": "Hallazgo afortunado", "detalle": "Fue una serendipia." }] }
            ]
        }))
        .unwrap();
        assert_eq!(card["estilo"], "portada");
        assert!(card.get("tono").is_none(), "el color es siempre el acento del tema");
        let blocks = card["bloques"].as_array().unwrap();
        let kinds: Vec<&str> = blocks.iter().map(|b| b["tipo"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["prevision", "marcador", "versus", "medidor", "checklist", "aviso", "paleta", "definicion"]);
        // Una mínima mayor que la máxima se da la vuelta; el día sin máxima, fuera.
        assert_eq!(blocks[0]["items"].as_array().unwrap().len(), 2);
        assert_eq!((blocks[0]["items"][0]["min"].as_f64(), blocks[0]["items"][0]["max"].as_f64()), (Some(7.0), Some(15.0)));
        assert_eq!(blocks[0]["items"][1]["max"], 13.0);
        assert_eq!(blocks[1]["items"][1]["valor"], "1");
        assert_eq!(blocks[2]["columnas"], json!(["iPhone 17", "Pixel 10"]));
        assert_eq!(blocks[2]["imagenes"], json!(["https://a/i.jpg", ""]));
        assert_eq!(blocks[2]["items"].as_array().unwrap().len(), 2);
        assert_eq!(blocks[2]["items"][0]["numeros"], json!([12.0, 16.0]));
        assert_eq!(blocks[2]["items"][1]["numeros"], json!([]));
        assert_eq!(blocks[2]["items"][1]["mejor"], -1);
        assert_eq!(blocks[3]["items"].as_array().unwrap().len(), 2);
        assert_eq!(blocks[3]["items"][0]["maximo"], 100.0);
        assert_eq!(blocks[3]["items"][1]["maximo"], 10.0);
        assert_eq!(blocks[4]["items"].as_array().unwrap().len(), 2);
        assert_eq!(blocks[6]["items"], json!([{ "titulo": "Mar", "valor": "#00AAFF" }, { "titulo": "", "valor": "#00AAFF" }]));
        assert_eq!(blocks[7]["valor"], "sustantivo femenino");
        // Con el significado en `texto` y sin acepciones, el término es el título.
        let word = clean(&json!({ "titulo": "Serendipia", "bloques": [{ "tipo": "definicion", "texto": "Hallazgo afortunado." }] })).unwrap();
        assert_eq!(word["bloques"][0]["texto"], "Serendipia");
        assert_eq!(word["bloques"][0]["items"][0]["titulo"], "Hallazgo afortunado.");
        // Un estilo que no existe se queda en el de fábrica.
        assert_eq!(clean(&json!({ "titulo": "x", "estilo": "barroco", "bloques": [{ "tipo": "texto", "texto": "y" }] })).unwrap()["estilo"], "");
        assert_eq!(first_number("-3,5 °C"), Some(-3.5));
        assert_eq!(first_number("1.234,5 €"), Some(1234.5));
        assert_eq!(first_number("n/a"), None);
        let metric = clean(&json!({ "titulo": "BTC", "bloques": [{ "tipo": "metricas", "items": [
            { "titulo": "Precio", "valor": "82 k€", "serie": [1, 2, "3", "x"] }, { "titulo": "Hoy", "valor": "+2 %", "serie": [1] }
        ] }] }))
        .unwrap();
        assert_eq!(metric["bloques"][0]["items"][0]["serie"], json!([1.0, 2.0, 3.0]));
        assert!(metric["bloques"][0]["items"][1].get("serie").is_none());
    }

    #[test]
    fn what_the_model_writes_its_own_way() {
        assert_eq!(icon(Some(&json!("Nube lluvia"))), "cloud-rain");
        assert_eq!(icon(Some(&json!("tormenta"))), "cloud-lightning");
        assert_eq!(icon(Some(&json!("ph-sun-fill"))), "sun");
        assert_eq!(icon(Some(&json!("☀️"))), "sun");
        assert_eq!(icon(Some(&json!("montaña"))), "mountains");
        assert_eq!(icon(Some(&json!("🦄"))), "");
        assert!(ICONS.iter().all(|i| ICONS.iter().filter(|j| *j == i).count() == 1), "sin repetidos");
        assert!(ALIASES.iter().all(|(_, i)| ICONS.contains(i)), "cada alias lleva a un icono que existe");
        // Una tira con mínimas y máximas es el tiempo.
        let card = clean(&json!({ "titulo": "Bilbao", "bloques": [
            { "tipo": "tira", "items": [
                { "titulo": "Dom 22", "valor": "8° / 18°", "icono": "nube-lluvia", "detalle": "Chubascos, lluvia 90 %" },
                { "titulo": "Lun 23", "valor": "-2º / 6º", "icono": "nieve", "detalle": "Despejado" }
            ] },
            { "tipo": "tira", "items": [{ "titulo": "Mañana", "valor": "9:00" }, { "titulo": "Tarde", "valor": "17:00" }] }
        ] }))
        .unwrap();
        assert_eq!(card["bloques"][0]["tipo"], "prevision");
        assert_eq!(card["bloques"][0]["items"][0], json!({ "titulo": "Dom 22", "icono": "cloud-rain", "min": 8.0, "max": 18.0, "detalle": "90 %" }));
        assert_eq!((card["bloques"][0]["items"][1]["min"].as_f64(), card["bloques"][0]["items"][1]["detalle"].as_str()), (Some(-2.0), Some("")));
        assert_eq!(card["bloques"][1]["tipo"], "tira");
        // Una segunda tarjeta solo con preguntas se añade a la primera.
        let first = clean(&json!({ "titulo": "Tortilla", "bloques": [{ "tipo": "pasos", "items": [{ "titulo": "Pela" }] }, { "tipo": "preguntas", "items": ["¿Con cebolla?"] }] })).unwrap();
        let more = clean(&json!({ "titulo": "", "bloques": [{ "tipo": "preguntas", "items": ["¿Y sin huevo?"] }, { "tipo": "enlaces", "items": [{ "url": "https://a.es" }] }] })).unwrap();
        let merged = merge(&first, &more).unwrap();
        let kinds: Vec<&str> = merged["bloques"].as_array().unwrap().iter().map(|b| b["tipo"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["pasos", "preguntas", "enlaces"]);
        assert_eq!(merged["bloques"][1]["items"][0]["titulo"], "¿Y sin huevo?");
        assert_eq!(merged["titulo"], "Tortilla");
        assert!(merge(&first, &first).is_none());
    }

    /// La galería de ejemplos (`data/cards.json`, `veyl cards`): cada tarjeta
    /// tiene que ser una que veyld dejaría tal cual, con todo lo que Veyl lee.
    /// Tras editarla, `GALLERY_WRITE=1 cargo test the_gallery` la deja limpia.
    #[test]
    fn the_gallery_is_clean() {
        let Ok(text) = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../data/cards.json")) else {
            return;
        };
        let mut examples: Vec<Value> = serde_json::from_str(&text).unwrap();
        if std::env::var_os("GALLERY_WRITE").is_some() {
            for e in &mut examples {
                e["card"] = clean(&e["card"]).unwrap();
            }
            std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/../data/cards.json"), serde_json::to_string_pretty(&examples).unwrap() + "\n").unwrap();
            return;
        }
        for e in &examples {
            let card = &e["card"];
            assert_eq!(clean(card).as_ref(), Some(card), "{}", card["titulo"]);
        }
    }

    #[test]
    fn rich_pieces_and_pictures() {
        let card = clean(&json!({
            "titulo": "GTA VI", "color": "rosa", "imagen": "javascript:alert(1)",
            "bloques": [
                { "tipo": "cuenta", "fecha": "2026-11-19T00:00:00+01:00", "texto": "lanzamiento" },
                { "tipo": "cuenta", "fecha": "pronto" },
                { "tipo": "etiquetas", "items": [{ "titulo": "PS5", "color": "bien" }, { "titulo": "Xbox", "color": "fucsia" }] },
                { "tipo": "imagen", "url": "https://x.com/a.jpg" },
                { "tipo": "enlaces", "items": [{ "titulo": "Rockstar", "url": "https://rockstargames.com/VI" }] }
            ]
        }))
        .unwrap();
        assert!(card.get("color").is_none());
        assert_eq!(card["imagen"], "");
        let kinds: Vec<_> = card["bloques"].as_array().unwrap().iter().map(|b| b["tipo"].as_str().unwrap().to_string()).collect();
        assert_eq!(kinds, ["cuenta", "etiquetas", "imagen", "enlaces"]);
        assert_eq!(card["bloques"][1]["items"][0]["color"], "bien");
        assert_eq!(card["bloques"][1]["items"][1]["color"], "");
        assert_eq!(links(&card), ["https://rockstargames.com/VI"]);
        let pairs =
            clean(&json!({ "titulo": "Tokio", "bloques": [{ "tipo": "datos", "items": [{ "titulo": "Zona", "valor": "JST" }, { "titulo": "Vacío" }] }] }))
                .unwrap();
        assert_eq!(pairs["bloques"][0]["items"].as_array().unwrap().len(), 1);
        assert_eq!(
            meta_image(r#"<head><meta property="og:image" content="https://a.b/c.jpg?x=1&amp;y=2"></head>"#).as_deref(),
            Some("https://a.b/c.jpg?x=1&amp;y=2")
        );
        assert_eq!(meta_image(r#"<meta content='/p.png' name='twitter:image'>"#).as_deref(), Some("/p.png"));
        let text = from_text("GTA 6 sale hoy, 19 de noviembre. Para PS5 y Xbox.\nEl PC aún no tiene fecha.");
        assert_eq!(text["titulo"], "GTA 6 sale hoy, 19 de noviembre.");
        assert_eq!(text["bloques"].as_array().unwrap().len(), 2);
    }
}
