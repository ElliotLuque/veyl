//! Las tarjetas que compone el cerebro: la respuesta a una pregunta, hecha a
//! su medida (una lista, unas cifras, unos pasos, unos botones) y pintada por
//! Veyl en la isla del centro, después de «Pensando». No es código: es un
//! vocabulario fijo de piezas que Veyl sabe pintar. Lo que llega se limpia
//! aquí (longitudes, cuántas piezas, iconos que existen, solo enlaces web) y
//! lo que no se entiende se tira.

use serde_json::{Value, json};
use std::io::Read;
use std::time::Duration;

/// Los colores que puede pedir el modelo, para cosas pequeñas y con sentido:
/// Veyl los toma del esquema (lo demás va siempre en el acento del tema).
pub const COLORS: &[&str] = &["bien", "aviso", "mal", "neutro"];

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
];

/// Lo que se le enseña al modelo de la herramienta `mostrar`.
pub fn description() -> String {
    format!(
        "Enseña la respuesta como una tarjeta en la isla de Veyl, hecha a la medida de lo preguntado: es lo que el usuario ve, así que hazla \
         rica y visual, como una ficha de un buscador o de un asistente con interfaz, y elige las piezas que mejor cuenten esta respuesta \
         (una gráfica para una evolución, una tabla para comparar, pros y contras para decidir, una tira para una previsión…). Llámala una \
         vez, al final, y después contesta con tu frase corta (la que se dice en voz alta).\n\
         Arriba: `titulo` (corto), `subtitulo`, `icono` e `imagen` (URL https directa de una imagen que hayas visto: portada, cartel, foto del \
         producto o del lugar; si no tienes, déjala vacía y Veyl saca la del primer enlace).\n\
         Los colores son los del tema del usuario: no los elijas tú. Solo puedes marcar cosas pequeñas con un `color` que signifique algo: {}.\n\
         Piezas (`bloques`, en orden, como mucho 8), cada una con su `tipo`:\n\
         - texto: `texto` (un párrafo corto).\n\
         - lista: `items` [{{titulo, detalle?, valor?, icono?, color?, imagen?}}] (como mucho 8).\n\
         - datos: `items` [{{titulo, valor}}] (pares nombre–valor, en dos columnas).\n\
         - cifra: `valor` (lo grande: «18 °C», «2 h 10 min»), `texto` (qué es) e `icono?`.\n\
         - metricas: `items` [{{titulo, valor, detalle?, icono?, color?}}] (2 a 6 cifras en mosaico; `detalle` p. ej. «+3 % hoy»).\n\
         - cuenta: `fecha` (ISO 8601, «2026-12-25T00:00:00+01:00») y `texto`: cuenta atrás (o «hace…») que Veyl mantiene al día.\n\
         - grafica: `forma` (barras, lineas o anillo), `texto?` (qué muestra), `unidad?` («€», «°C», «h») e `items` [{{titulo, numero}}] \
         (hasta 12 puntos, en orden; `numero` es un número).\n\
         - tabla: `columnas` [cabeceras, la primera la de las filas] y `items` [{{titulo, valores: [texto por columna], mejor?}}] para \
         comparar (como mucho 4 columnas y 8 filas; `mejor` es el índice del valor que gana en esa fila).\n\
         - pros: `pros` [textos] y `contras` [textos].\n\
         - tira: `items` [{{titulo, valor, icono?, detalle?}}]: fichas en fila (previsión por horas o días, horarios, tallas…).\n\
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
         Iconos que existen: {}.",
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
            "imagen": { "type": "string" }
        }
    });
    json!({
        "type": "object",
        "properties": {
            "titulo": { "type": "string", "description": "Corto: de qué va la tarjeta" },
            "subtitulo": { "type": "string" },
            "icono": { "type": "string" },
            "imagen": { "type": "string" },
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

fn icon(value: Option<&Value>) -> String {
    value.and_then(Value::as_str).map(str::trim).filter(|i| ICONS.contains(i)).unwrap_or("").to_string()
}

fn color(value: Option<&Value>) -> String {
    value.and_then(Value::as_str).map(str::trim).filter(|c| COLORS.contains(c)).unwrap_or("").to_string()
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
                        json!({
                            "titulo": cut(i.get("titulo"), 90),
                            "detalle": cut(i.get("detalle"), 160),
                            "valor": cut(i.get("valor"), if kind == "datos" { 60 } else { 30 }),
                            "icono": icon(i.get("icono")),
                            "color": color(i.get("color")),
                            "imagen": web(i.get("imagen"))
                        })
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
                { "tipo": "html", "texto": "<script>" }
            ]
        }))
        .unwrap();
        let blocks = card["bloques"].as_array().unwrap();
        assert_eq!(blocks.iter().map(|b| b["tipo"].as_str().unwrap()).collect::<Vec<_>>(), ["cifra", "barra", "lista", "enlaces", "acciones"]);
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
