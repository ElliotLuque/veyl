//! Las preguntas al diario: qué estuviste haciendo, cuándo salió algo,
//! cuánto tiempo y qué sonó.

use super::*;

/// El tramo del que se habla: lo dicho («el jueves por la tarde») o, sin nada, hoy.
pub(super) fn span_of(said: &str) -> Result<when::Span, Outcome> {
    let now = Local::now();
    let span = if said.trim().is_empty() {
        when::past(&when::When { day: Some(when::Day::Offset(0)), ..when::When::default() }, now)
    } else {
        match when::parse(&tokens(said)) {
            Some(w) => when::past(&w, now),
            None => return Err(Outcome::fail(format!("No entiendo cuándo es «{said}»"))),
        }
    };
    span.ok_or_else(|| Outcome::fail("Eso aún no ha pasado"))
}

/// El diario, con lo abierto apuntado hasta ahora.
pub(super) fn diary(d: &mut Daemon) -> Result<&rusqlite::Connection, Outcome> {
    let Some(journal) = &mut d.journal else {
        return Err(Outcome::fail("No hay diario"));
    };
    journal.beat(crate::daemon::now());
    Ok(journal.db())
}

pub(super) fn failed(e: rusqlite::Error) -> Outcome {
    Outcome::fail(format!("El diario no contesta: {e}"))
}

/// «09:05».
pub(super) fn hm(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms).map(|t| t.with_timezone(&Local).format("%H:%M").to_string()).unwrap_or_default()
}

pub(super) fn date(ms: i64) -> NaiveDate {
    chrono::DateTime::from_timestamp_millis(ms).map(|t| t.with_timezone(&Local).date_naive()).unwrap_or_default()
}

/// Un texto largo, cortado: «We're Decompiling Fallout 3…».
pub(super) fn short(text: &str, max: usize) -> String {
    let text = text.trim();
    if text.chars().count() > max { format!("{}…", text.chars().take(max - 1).collect::<String>().trim_end()) } else { text.to_string() }
}

/// «a, b y c».
pub(super) fn and(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} y {last}", rest.join(", ")),
    }
}

/// Quita del título el nombre de la aplicación: «Rust by Example — Mozilla Firefox».
pub(super) fn tidy(title: &str, app: &str) -> String {
    let app = fold(app);
    for sep in [" — ", " – ", " - ", " · ", " | "] {
        if let Some((head, tail)) = title.rsplit_once(sep)
            && !head.is_empty()
            && !app.is_empty()
            && (fold(tail).contains(&app) || app.contains(&fold(tail)))
        {
            return tidy(head, &app);
        }
    }
    short(title, 60)
}

/// El título de una página sin el navegador ni el sitio: «… - YouTube».
pub(super) fn page(title: &str, browser: &str, site: &str) -> String {
    let name = site.rsplit_once('.').map(|(head, _)| head.rsplit('.').next().unwrap_or(head)).unwrap_or(site);
    tidy(&tidy(title, browser), name)
}

/// Por qué no estabas, dicho después de un tiempo: «40 min con la pantalla bloqueada».
pub(super) fn away_name(reason: &str) -> &str {
    match reason {
        "inactivo" => "sin tocar nada",
        "bloqueado" => "con la pantalla bloqueada",
        "suspendido" => "con el ordenador suspendido",
        other => other,
    }
}

/// Lo mismo, solo: «pantalla bloqueada».
pub(super) fn away_alone(reason: &str) -> &str {
    match reason {
        "" => "sin diario",
        "inactivo" => "sin tocar nada",
        "bloqueado" => "pantalla bloqueada",
        "suspendido" => "ordenador suspendido",
        other => other,
    }
}

/// Un día en una lista: «hoy», «ayer», «sábado 14», «9 de noviembre».
pub(super) fn day_short(day: NaiveDate, today: NaiveDate) -> String {
    match (today - day).num_days() {
        0..=2 => day_name(day, today),
        3..=6 => format!("{} {}", weekday(day), day.day()),
        _ if day.year() == today.year() => format!("{} de {}", day.day(), month(day)),
        _ => format!("{} de {} de {}", day.day(), month(day), day.year()),
    }
}

/// Lo que dice el título de una terminal, si dice algo más que su carpeta o su shell.
pub(super) fn telling(title: &str, folder: &str) -> bool {
    let (title, base) = (fold(title), fold(Path::new(folder).file_name().map(|f| f.to_string_lossy()).unwrap_or_default().as_ref()));
    !(base.is_empty() || title.contains(&base) || ["fish", "bash", "zsh", "sh", "nu", "~"].contains(&title.trim()))
}

/// Lo que se busca, en palabras que cuentan: sin artículos ni «el vídeo de».
pub(super) fn significant(text: &str) -> Vec<String> {
    const EMPTY: &[&str] = &[
        "el", "la", "los", "las", "lo", "un", "una", "unos", "unas", "de", "del", "al", "a", "en", "con", "por", "para", "y", "o", "que", "mi", "mis", "su",
        "sus", "tu", "tus", "sobre", "este", "esta", "esto", "ese", "esa", "eso", "aquel", "aquella", "algo", "cosa", "cosas", "video", "videos", "pagina",
        "paginas", "web", "rollo", "tema", "vez",
    ];
    let folded = fold(text);
    let words: Vec<String> = folded.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty() && !EMPTY.contains(w)).map(str::to_string).collect();
    words
}

/// «¿Qué estuve mirando el jueves por la tarde?»
pub fn recall(d: &mut Daemon, said: &str) -> Outcome {
    let (span, report) = match gather(d, said) {
        Ok(found) => found,
        Err(why) => return why,
    };
    let mut out = describe_recall(&d.apps, &span.label, &report.recall);
    // Además del resumen: los ratos seguidos y en qué proyectos.
    let mut extra = Vec::new();
    let long = report.sessions.iter().filter(|(a, b)| b - a >= 3 * 60_000).count();
    if long > 1 {
        let longest = report.sessions.iter().map(|(a, b)| b - a).max().unwrap_or(0);
        extra.push(format!("{long} ratos seguidos; el más largo, {}; cambiaste de ventana {} veces", spoken(longest), report.switches));
    }
    let projects: Vec<String> = report
        .projects
        .iter()
        .filter(|p| p.ms + p.agents >= 60_000)
        .take(3)
        .map(|p| match (p.ms >= 60_000, p.agents >= 60_000) {
            (true, true) => format!("{} ({}; los agentes, {})", project_name(&p.path), spoken(p.ms), spoken(p.agents)),
            (false, true) => format!("{} (los agentes, {})", project_name(&p.path), spoken(p.agents)),
            _ => format!("{} ({})", project_name(&p.path), spoken(p.ms.max(60_000))),
        })
        .collect();
    if !projects.is_empty() {
        extra.push(format!("Proyectos: {}", and(&projects)));
    }
    if !extra.is_empty() && out.answer.is_some() {
        let body = out.answer.take().unwrap_or_default();
        let mut lines: Vec<String> = body.lines().map(str::to_string).collect();
        let at = usize::from(lines.first().is_some_and(|l| l.starts_with("De ")));
        lines.splice(at..at, extra);
        out.answer = Some(lines.join("\n"));
    }
    out
}

/// El tramo de lo dicho y todo lo que hubo en él (por horas si es un día o
/// menos; por días si es más).
pub(super) fn gather(d: &mut Daemon, said: &str) -> Result<(when::Span, journal::Report), Outcome> {
    let span = span_of(said)?;
    let (from, to) = (span.from.timestamp_millis(), span.to.timestamp_millis());
    let slice = if to - from <= 26 * 3600 * 1000 { 3600 * 1000 } else { 24 * 3600 * 1000 };
    let midnight =
        span.from.date_naive().and_hms_opt(0, 0, 0).and_then(|t| t.and_local_timezone(Local).earliest()).map(|t| t.timestamp_millis()).unwrap_or(from);
    let home = config::home().to_string_lossy().into_owned();
    let report = diary(d).and_then(|db| journal::report(db, from, to, slice, midnight, &home).map_err(failed))?;
    Ok((span, report))
}

/// Lo mismo que `recall`, para otro programa: todo, en JSON.
pub fn recall_json(d: &mut Daemon, said: &str) -> Result<Value, Outcome> {
    let (span, report) = gather(d, said)?;
    let mut json = serde_json::to_value(&report).map_err(|e| Outcome::fail(e.to_string()))?;
    json["label"] = json!(span.label);
    Ok(json)
}

/// Un día entero del diario, para su vista (base/Diary.qml): todo lo de
/// `report`, la línea del tiempo, el nombre de cada aplicación y lo que hubo
/// delante cada uno de los `days` días hasta hoy. `day` es «2026-11-19»; hoy
/// llega hasta ahora.
pub fn diary_json(d: &mut Daemon, day: &str, days: i64) -> Result<Value, Outcome> {
    let today = Local::now().date_naive();
    let day = if day.is_empty() { today } else { NaiveDate::parse_from_str(day, "%Y-%m-%d").map_err(|_| Outcome::fail(format!("No es un día: «{day}»")))? };
    let midnight = |day: NaiveDate| day.and_hms_opt(0, 0, 0).and_then(|t| t.and_local_timezone(Local).earliest()).map(|t| t.timestamp_millis()).unwrap_or(0);
    let from = midnight(day);
    let to = if day == today { crate::daemon::now() } else { midnight(day + chrono::Duration::days(1)) };
    let home = config::home().to_string_lossy().into_owned();
    let apps_known = d.apps.clone();
    let db = diary(d)?;
    let report = journal::report(db, from, to, 3600 * 1000, from, &home).map_err(failed)?;
    let blocks = journal::timeline(db, from, to, 5 * 60 * 1000, from).map_err(failed)?;
    let mut strip = Vec::new();
    for back in (0..days.clamp(1, 60)).rev() {
        let one = today - chrono::Duration::days(back);
        let (start, end) = (midnight(one), if one == today { crate::daemon::now() } else { midnight(one + chrono::Duration::days(1)) });
        strip.push(json!({ "day": one.format("%Y-%m-%d").to_string(), "ms": journal::screen(db, start, end).map_err(failed)? }));
    }
    let mut labels = serde_json::Map::new();
    for class in report.recall.apps.iter().map(|a| &a.app).chain(blocks.iter().map(|b| &b.app)).chain(report.opened.iter().map(|(a, _)| a)) {
        if !class.is_empty() && !labels.contains_key(class) {
            labels.insert(class.clone(), json!(apps::label(&apps_known, class)));
        }
    }
    let mut json = serde_json::to_value(&report).map_err(|e| Outcome::fail(e.to_string()))?;
    json["day"] = json!(day.format("%Y-%m-%d").to_string());
    json["label"] = json!(capital(&day_name(day, today)));
    json["blocks"] = serde_json::to_value(&blocks).map_err(|e| Outcome::fail(e.to_string()))?;
    json["labels"] = Value::Object(labels);
    json["days"] = json!(strip);
    json["home"] = json!(home);
    Ok(json)
}

/// Lo que encuentra `find`, en datos, para la vista del diario.
pub fn find_json(d: &mut Daemon, what: &str) -> Result<Value, Outcome> {
    let words = significant(what);
    if words.is_empty() {
        return Ok(json!([]));
    }
    let apps_known = d.apps.clone();
    let hits = diary(d).and_then(|db| journal::find(db, &words, |app| apps::label(&apps_known, app), 40).map_err(failed))?;
    let hits: Vec<Value> = hits
        .iter()
        .map(|hit| {
            let mut one = serde_json::to_value(hit).unwrap_or_default();
            one["label"] = json!(if hit.kind == "focus" { apps::label(&apps_known, &hit.app) } else { hit.app.clone() });
            one
        })
        .collect();
    Ok(json!(hits))
}

/// Todo lo distinto de un tramo, para que el cerebro busque por lo que
/// significa («¿cuándo miré lo de la beca?») y no solo por las palabras.
pub fn index(d: &mut Daemon, said: &str) -> Outcome {
    let span = match span_of(said) {
        Ok(span) => span,
        Err(why) => return why,
    };
    let (from, to) = (span.from.timestamp_millis(), span.to.timestamp_millis());
    let apps_known = d.apps.clone();
    let found = match diary(d).and_then(|db| journal::index(db, from, to, 300).map_err(failed)) {
        Ok(found) => found,
        Err(why) => return why,
    };
    let label = capital(&span.label);
    if found.is_empty() {
        return Outcome::done(format!("{label}: nada en el diario"));
    }
    let today = Local::now().date_naive();
    let several = date(from) != date(to - 1);
    let lines: Vec<String> = found
        .iter()
        .map(|e| {
            let app = if e.kind == "focus" { apps::label(&apps_known, &e.app) } else { e.app.clone() };
            let when = if e.last - e.first >= 60_000 { format!("{}–{}", hm(e.first), hm(e.last)) } else { hm(e.first) };
            let when = if several { format!("{}, {when}", capital(&day_short(date(e.first), today))) } else { when };
            let mut parts = vec![
                when,
                match e.kind {
                    "media" => format!("sonó en {app}"),
                    "file" => format!("archivo, con {app}"),
                    _ => app.clone(),
                },
            ];
            if !e.place.is_empty() {
                parts.push(if e.place.starts_with('/') { config::tilde(&e.place) } else { e.place.clone() });
            }
            let title = page(&e.title, &app, &e.place);
            // Un título que solo repite la carpeta o la aplicación no dice nada.
            let says =
                !title.is_empty() && title != "~" && title != app && config::tilde(&e.place) != title && !e.place.ends_with(title.trim_start_matches('~'));
            if says && e.kind != "file" {
                parts.push(format!("«{}»", short(&title, 90)));
            }
            if !e.url.is_empty() {
                parts.push(short(&e.url, 100));
            }
            if e.kind != "file" {
                parts.push(if e.times > 1 { format!("{} en {} veces", spoken(e.ms.max(60_000)), e.times) } else { spoken(e.ms.max(60_000)) });
            }
            parts.join(" · ")
        })
        .collect();
    Outcome::tell(format!("{label}: {} cosas distintas, por orden", found.len()), lines.join("\n"))
}

/// «veyl» de «/home/u/dev/veyl».
pub(super) fn project_name(path: &str) -> String {
    Path::new(path).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_else(|| config::tilde(path))
}

/// Una barra de 10 de lo lleno que está algo: «███████░░░».
pub(super) fn bar(part: i64, whole: i64) -> String {
    let filled = if whole > 0 { ((part * 10 + whole / 2) / whole).clamp(0, 10) as usize } else { 0 };
    format!("{}{}", "█".repeat(filled), "░".repeat(10 - filled))
}

/// «(a 25 min, b 10 min)», lo que pasa del minuto.
pub(super) fn timed(list: &[(String, i64)], max: usize, name: impl Fn(&str) -> String) -> String {
    let parts: Vec<String> =
        list.iter().filter(|(_, ms)| *ms >= 30_000).take(max).map(|(k, ms)| format!("{} {}", name(k), spoken((*ms).max(60_000)))).collect();
    parts.join(", ")
}

/// «¿Qué hice ayer?», con todo: ritmo, horas, proyectos, aplicaciones con
/// sus páginas, carpetas y órdenes, niveles, agentes, lo que sonó, llamadas,
/// archivos, ventanas y lo que se le pidió a Veyl.
pub fn report(d: &mut Daemon, said: &str) -> Outcome {
    let (span, r) = match gather(d, said) {
        Ok(found) => found,
        Err(why) => return why,
    };
    let label = capital(&span.label);
    let rc = &r.recall;
    let heard: i64 = rc.media.iter().map(|m| m.ms).sum();
    if rc.total < 60_000 && r.agents.is_empty() && heard < 60_000 && rc.files.is_empty() {
        return Outcome::done(format!("{label}: nada en el diario"));
    }
    let today = Local::now().date_naive();
    let several = date(r.from) != date(r.to - 1);
    let at = |ms: i64| if several { format!("{}, {}", capital(&day_short(date(ms), today)), hm(ms)) } else { hm(ms) };
    let name = |app: &str| apps::label(&d.apps, app);
    let mut out: Vec<String> = Vec::new();
    let mut section = |title: &str, lines: Vec<String>| {
        if !lines.is_empty() {
            out.push(String::new());
            out.push(title.to_string());
            out.extend(lines.into_iter().map(|l| format!("  {l}")));
        }
    };

    // Ritmo.
    let mut rhythm = Vec::new();
    if !r.sessions.is_empty() {
        // Los ratos de verdad; los vistazos sueltos (menos de 3 min), solo contados.
        let long: Vec<&(i64, i64)> = r.sessions.iter().filter(|(a, b)| b - a >= 3 * 60_000).collect();
        let short_ones = r.sessions.len() - long.len();
        // Con varios días, el día solo cuando cambia.
        let mut day = None;
        let shown: Vec<String> = long
            .iter()
            .take(10)
            .map(|(a, b)| {
                let first = several && day != Some(date(*a));
                day = Some(date(*a));
                let from = if first { format!("{} {}", day_short(date(*a), today), hm(*a)) } else { hm(*a) };
                format!("{from}–{} ({})", hm(*b), spoken(b - a))
            })
            .collect();
        let mut line = match long.len() {
            0 => String::new(),
            1 => format!("Un rato seguido: {}", shown[0]),
            n => format!("{n} ratos seguidos: {}", shown.join(", ")),
        };
        if long.len() > 10 {
            line += &format!(" y {} más", long.len() - 10);
        }
        if short_ones > 0 {
            let glance = format!("{} vistazo{} suelto{}", short_ones, if short_ones == 1 { "" } else { "s" }, if short_ones == 1 { "" } else { "s" });
            line = if line.is_empty() { capital(&glance) } else { format!("{line}; y {glance}") };
        }
        let longest = r.sessions.iter().map(|(a, b)| b - a).max().unwrap_or(0);
        if long.len() > 1 {
            line += &format!(". El más largo, {}", spoken(longest));
        }
        rhythm.push(line);
    }
    let pauses: Vec<String> = rc.away.iter().filter(|(_, ms)| *ms >= 60_000).map(|(why, ms)| format!("{} {}", spoken(*ms), away_name(why))).collect();
    if !pauses.is_empty() {
        rhythm.push(format!("Pausas: {}", and(&pauses)));
    }
    if r.switches > 0 && rc.total > 0 {
        rhythm.push(format!("Cambiaste de ventana {} veces (una cada {} de media)", r.switches, spoken(rc.total / (r.switches as i64 + 1))));
    }
    section("Ritmo", rhythm);

    // Por horas o por días.
    let busiest = r.slices.iter().map(|s| s.ms).max().unwrap_or(0);
    let hourly = r.slices.first().is_some_and(|s| s.end - s.start <= 3600 * 1000);
    let mut slices = Vec::new();
    let first = r.slices.iter().position(|s| s.ms > 0).unwrap_or(r.slices.len());
    let last = r.slices.iter().rposition(|s| s.ms > 0).map_or(0, |i| i + 1);
    let mut empty: Option<(i64, i64)> = None;
    for s in r.slices.iter().take(last).skip(first) {
        // Las horas sin nada, juntas en una línea.
        if s.ms < 60_000 {
            empty = Some(empty.map_or((s.start, s.end), |(a, _)| (a, s.end)));
            continue;
        }
        if let Some((a, b)) = empty.take() {
            let span = if hourly { format!("{}–{}", hm(a), hm(b)) } else { format!("{}–{}", day_short(date(a), today), day_short(date(b - 1), today)) };
            slices.push(format!("{span:<11}  ·"));
        }
        let when = if hourly { hm(s.start) } else { capital(&day_short(date(s.start), today)) };
        let whole = if hourly { 3600 * 1000 } else { busiest };
        let mut what = format!("  {:>9}  {}", spoken(s.ms), name(&s.app));
        if !s.place.is_empty() {
            what += &format!(" · {}", if s.place.starts_with('/') { config::tilde(&s.place) } else { s.place.clone() });
        }
        slices.push(format!("{when:<11} {}{what}", bar(s.ms, whole)));
    }
    section(if hourly { "Por horas" } else { "Por días" }, slices);

    // Proyectos.
    let projects: Vec<String> = r
        .projects
        .iter()
        .filter(|p| p.ms + p.agents >= 60_000)
        .take(8)
        .map(|p| {
            let mut line = format!("{}  {}", config::tilde(&p.path), spoken(p.ms.max(60_000)));
            let apps = timed(&p.apps, 3, |a| name(a));
            if !apps.is_empty() {
                line += &format!(" ({apps})");
            }
            let commands = timed(&p.commands, 4, str::to_string);
            if !commands.is_empty() {
                line += &format!(" · {commands}");
            }
            if p.agents >= 60_000 {
                line += &format!(" · los agentes trabajaron {}", spoken(p.agents));
            }
            line
        })
        .collect();
    section("Proyectos", projects);

    // Aplicaciones, con todo lo que hubo dentro.
    let mut apps_lines = Vec::new();
    for app in rc.apps.iter().filter(|a| a.ms >= 60_000).take(10) {
        let label = name(&app.app);
        apps_lines.push(format!("{label}  {}", spoken(app.ms)));
        let mut covered = 0;
        for site in app.sites.iter().filter(|p| p.ms >= 30_000).take(6) {
            covered += site.ms;
            let pages = timed(&site.within, 3, |t| format!("«{}»", short(&page(t, &label, &site.name), 45)));
            apps_lines.push(format!("  {}  {}{}", site.name, spoken(site.ms.max(60_000)), if pages.is_empty() { String::new() } else { format!(": {pages}") }));
        }
        for folder in app.folders.iter().filter(|p| p.ms >= 30_000).take(4) {
            covered += folder.ms;
            let commands = timed(&folder.within, 4, str::to_string);
            apps_lines.push(format!(
                "  {}  {}{}",
                config::tilde(&folder.name),
                spoken(folder.ms.max(60_000)),
                if commands.is_empty() { String::new() } else { format!(": {commands}") }
            ));
        }
        // Lo que no cae en ningún sitio ni carpeta, por sus títulos.
        let placed: Vec<&str> = app.sites.iter().flat_map(|p| p.within.iter().map(|(t, _)| t.as_str())).collect();
        if app.ms - covered >= 60_000 {
            let mut seen: Vec<(String, i64)> = Vec::new();
            for (title, ms) in app.titles.iter().filter(|(t, _)| covered == 0 || !placed.contains(&t.as_str())) {
                let clean = tidy(title, &label);
                match seen.iter_mut().find(|(t, _)| *t == clean) {
                    Some((_, total)) => *total += ms,
                    None if !clean.is_empty() => seen.push((clean, *ms)),
                    None => {}
                }
            }
            let titles = timed(&seen, 6, |t| format!("«{}»", short(t, 50)));
            if !titles.is_empty() {
                apps_lines.push(if covered > 0 { format!("  otras páginas  {}: {titles}", spoken(app.ms - covered)) } else { format!("  {titles}") });
            }
        }
    }
    section("Aplicaciones", apps_lines);

    // Niveles (con hyprsplit, el mismo nivel de cada monitor se junta).
    let mut levels: Vec<(String, i64)> = Vec::new();
    for (workspace, ms) in &r.levels {
        let n = workspace.parse::<u32>().map(|id| if d.split > 0 { (id.max(1) - 1) % d.split + 1 } else { id });
        let key = n.map(|n| format!("nivel {n}")).unwrap_or_else(|_| workspace.clone());
        match levels.iter_mut().find(|(k, _)| *k == key) {
            Some((_, total)) => *total += ms,
            None => levels.push((key, *ms)),
        }
    }
    levels.sort_by_key(|(_, ms)| std::cmp::Reverse(*ms));
    let levels = timed(&levels, 10, str::to_string);
    section("Niveles", if levels.is_empty() { vec![] } else { vec![levels] });

    // Agentes.
    let agents: Vec<String> = r
        .agents
        .iter()
        .take(10)
        .map(|a| {
            let mut line = a.label.clone();
            if !a.name.is_empty() {
                line += &format!(" «{}»", short(&a.name, 50));
            }
            if !a.folder.is_empty() {
                line += &format!(" en {}", config::tilde(&a.folder));
            }
            if !a.model.is_empty() {
                line += &format!(" ({})", a.model);
            }
            line += &format!(": trabajó {}, de {} a {}", spoken(a.working.max(60_000)), at(a.first), hm(a.last));
            if a.waits > 0 {
                let total = if a.waiting < 60_000 { "menos de un minuto".to_string() } else { spoken(a.waiting) };
                line += &format!("; te esperó {} {}, {total} en total", a.waits, if a.waits == 1 { "vez" } else { "veces" });
            }
            line
        })
        .collect();
    section("Agentes", agents);

    // Lo que sonó.
    let media: Vec<String> = rc.media.iter().take(15).map(|m| format!("{} ({})  {}", track(m), m.player, spoken(m.ms.max(60_000)))).collect();
    section(&format!("Sonó{}", if heard >= 60_000 { format!(", {}", spoken(heard)) } else { String::new() }), media);

    // Micro, cámara y pantalla compartida.
    let calls: Vec<String> = r
        .calls
        .iter()
        .filter(|c| c.end - c.start >= 30_000)
        .take(15)
        .map(|c| {
            let mut line = format!("{}–{}  {} ({})", at(c.start), hm(c.end), device_name(&c.kind), spoken((c.end - c.start).max(60_000)));
            if !c.app.is_empty() {
                line += &format!(", en {}", name(&c.app));
                if !c.site.is_empty() {
                    line += &format!(" · {}", c.site);
                }
            }
            line
        })
        .collect();
    section("Micro, cámara y pantalla", calls);

    // Archivos.
    let files: Vec<String> = rc
        .files
        .iter()
        .take(20)
        .map(|f| {
            let folder = Path::new(&f.path).parent().map(|p| config::tilde(&p.to_string_lossy())).unwrap_or_default();
            format!("{}  {} · {} · {folder}", at(f.time), file_name(&f.path), f.app)
        })
        .collect();
    section("Archivos", files);

    // Ventanas.
    let opened: i64 = r.opened.iter().map(|(_, n)| n).sum();
    if opened > 0 || r.closed > 0 {
        let by: Vec<String> = r.opened.iter().take(6).map(|(app, n)| format!("{} {n}", name(app))).collect();
        section(
            "Ventanas",
            vec![format!("Abriste {opened}{}; cerraste {}", if by.is_empty() { String::new() } else { format!(" ({})", by.join(", ")) }, r.closed)],
        );
    }

    // Lo que se le pidió a Veyl.
    let asked: Vec<String> = r
        .orders
        .iter()
        .rev()
        .take(15)
        .rev()
        .map(|o| {
            let how = match o.source.as_str() {
                "voice" => " (voz)",
                "launcher" => " (entrada)",
                _ => "",
            };
            format!("{}  «{}»{how} → {}{}", at(o.time), short(&o.said, 60), if o.ok { "" } else { "✗ " }, short(&o.answer, 60))
        })
        .collect();
    section("Le pediste a Veyl", asked);

    let mut say = format!("{label}: {} de pantalla", spoken(rc.total));
    if rc.total >= 60_000 && date(rc.first) == date(rc.last) {
        say += &format!(", de {} a {}", hm(rc.first), hm(rc.last));
    }
    let body = out.into_iter().skip_while(String::is_empty).collect::<Vec<_>>().join("\n");
    Outcome::tell(say, body)
}

pub fn describe_recall(apps_known: &[App], label: &str, found: &journal::Recall) -> Outcome {
    let label = capital(label);
    let heard: i64 = found.media.iter().map(|m| m.ms).sum();
    if found.total < 60_000 && found.agents.is_empty() && heard < 60_000 {
        return Outcome::done(format!("{label}: nada en el diario"));
    }
    let mut lines = Vec::new();
    if found.total >= 60_000 {
        let pauses: Vec<String> = found.away.iter().filter(|(_, ms)| *ms >= 60_000).map(|(why, ms)| format!("{} {}", spoken(*ms), away_name(why))).collect();
        let mut line = if date(found.first) == date(found.last) { format!("De {} a {}", hm(found.first), hm(found.last)) } else { String::new() };
        if !pauses.is_empty() {
            line = if line.is_empty() { capital(&and(&pauses)) } else { format!("{line}, con {}", and(&pauses)) };
        }
        if !line.is_empty() {
            lines.push(line);
        }
    }
    for app in found.apps.iter().filter(|a| a.ms >= 60_000).take(6) {
        let name = apps::label(apps_known, &app.app);
        let sites: i64 = app.sites.iter().map(|p| p.ms).sum();
        let folders: i64 = app.folders.iter().map(|p| p.ms).sum();
        let detail = if sites * 2 >= app.ms {
            // En el navegador: los sitios, cada uno con su página más vista.
            let top: Vec<String> = app
                .sites
                .iter()
                .filter(|p| p.ms >= 30_000)
                .take(3)
                .map(|p| {
                    let title = p.within.first().map(|(t, _)| page(t, &name, &p.name)).unwrap_or_default();
                    let bare = p.name.split('.').next().unwrap_or(&p.name);
                    if title.is_empty() || fold(&title) == fold(bare) { p.name.clone() } else { format!("{} («{}»)", p.name, short(&title, 50)) }
                })
                .collect();
            if top.is_empty() { String::new() } else { format!(": {}", top.join(", ")) }
        } else if folders * 2 >= app.ms {
            // En una terminal: las carpetas, con lo que corrió en ellas.
            let top: Vec<String> = app
                .folders
                .iter()
                .take(2)
                .map(|p| {
                    let commands: Vec<&str> = p.within.iter().filter(|(_, ms)| *ms >= 30_000).take(3).map(|(c, _)| c.as_str()).collect();
                    if commands.is_empty() { config::tilde(&p.name) } else { format!("{} ({})", config::tilde(&p.name), commands.join(", ")) }
                })
                .collect();
            format!(", en {}", top.join(" y "))
        } else {
            let mut seen: Vec<String> = Vec::new();
            for (title, _) in &app.titles {
                let clean = tidy(title, &name);
                if !clean.is_empty() && !seen.contains(&clean) {
                    seen.push(clean);
                }
                if seen.len() == 2 {
                    break;
                }
            }
            if seen.is_empty() { String::new() } else { format!(": {}", seen.iter().map(|t| format!("«{t}»")).collect::<Vec<_>>().join(", ")) }
        };
        lines.push(format!("{name}, {}{detail}", spoken(app.ms)));
    }
    for agent in found.agents.iter().filter(|a| a.ms >= 60_000).take(3) {
        let mut place = if agent.folder.is_empty() { String::new() } else { format!(" en {}", config::tilde(&agent.folder)) };
        // El nombre de la sesión, si dice algo más que la carpeta.
        if !agent.name.is_empty() && Path::new(&agent.folder).file_name().is_none_or(|f| f.to_string_lossy() != agent.name) {
            place += &format!(" («{}»)", agent.name);
        }
        lines.push(format!("{} trabajó {}{place}", agent.label, spoken(agent.ms)));
    }
    if heard >= 60_000 {
        let top: Vec<String> = found.media.iter().take(3).map(track).collect();
        lines.push(format!("Sonó {}: {}", spoken(heard), top.join(", ")));
    }
    for device in found.devices.iter().filter(|d| d.ms >= 60_000) {
        let mut line = format!("{} {}", device_name(&device.kind), spoken(device.ms));
        if device.times > 1 {
            line += &format!(" en {} veces", device.times);
        }
        if !device.app.is_empty() {
            line += &format!(", sobre todo en {}", apps::label(apps_known, &device.app));
            if !device.site.is_empty() {
                line += &format!(" ({})", device.site);
            }
        }
        lines.push(line);
    }
    if !found.files.is_empty() {
        let mut names: Vec<String> = found.files.iter().take(4).map(|f| file_name(&f.path)).collect();
        if found.files.len() > 4 {
            names.push(format!("{} más", found.files.len() - 4));
        }
        lines.push(format!("Archivos: {}", and(&names)));
    }
    let say = if found.total >= 60_000 { format!("{label}: {} de pantalla", spoken(found.total)) } else { format!("{label}: casi nada de pantalla") };
    if lines.is_empty() { Outcome::done(say) } else { Outcome::tell(say, lines.join("\n")) }
}

/// «Micro abierto», «Cámara encendida».
pub(super) fn device_name(kind: &str) -> &str {
    match kind {
        "micro" => "Micro abierto",
        "cámara" => "Cámara encendida",
        "pantalla" => "Pantalla compartida",
        other => other,
    }
}

/// «Song» de Artist.
pub(super) fn track(m: &journal::MediaTime) -> String {
    if m.artist.is_empty() { format!("«{}»", short(&m.title, 50)) } else { format!("«{}» de {}", short(&m.title, 50), short(&m.artist, 30)) }
}

pub(super) fn file_name(path: &str) -> String {
    Path::new(path).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string())
}

/// El tamaño de los ratos de la línea del tiempo: más largos cuanto más tiempo.
pub(super) fn slot_for(length: i64) -> i64 {
    const MIN: i64 = 60_000;
    if length <= 4 * 60 * MIN {
        5 * MIN
    } else if length <= 26 * 60 * MIN {
        15 * MIN
    } else {
        60 * MIN
    }
}

/// «¿Qué hice ayer, hora a hora?»: lo que más tuvo el foco en cada rato.
pub fn timeline(d: &mut Daemon, said: &str) -> Outcome {
    let span = match span_of(said) {
        Ok(span) => span,
        Err(why) => return why,
    };
    let (from, to) = (span.from.timestamp_millis(), span.to.timestamp_millis());
    let midnight =
        span.from.date_naive().and_hms_opt(0, 0, 0).and_then(|t| t.and_local_timezone(Local).earliest()).map(|t| t.timestamp_millis()).unwrap_or(from);
    let blocks = match diary(d).and_then(|db| journal::timeline(db, from, to, slot_for(to - from), midnight).map_err(failed)) {
        Ok(blocks) => blocks,
        Err(why) => return why,
    };
    let label = capital(&span.label);
    if blocks.iter().all(|b| b.app.is_empty()) {
        return Outcome::done(format!("{label}: nada en el diario"));
    }
    let today = Local::now().date_naive();
    let several = date(from) != date(to - 1);
    let mut lines = Vec::new();
    let mut day = None;
    for block in &blocks {
        if several && day != Some(date(block.start)) {
            day = Some(date(block.start));
            lines.push(capital(&day_short(date(block.start), today)));
        }
        let what = if block.app.is_empty() {
            away_alone(&block.away).to_string()
        } else {
            let name = apps::label(&d.apps, &block.app);
            let mut parts = vec![name.clone()];
            if !block.place.is_empty() {
                parts.push(if block.place.starts_with('/') { config::tilde(&block.place) } else { block.place.clone() });
            }
            let folder = block.place.starts_with('/');
            if block.command {
                parts.push(block.detail.clone());
            } else if !block.detail.is_empty() && (!folder || telling(&block.detail, &block.place)) {
                parts.push(format!("«{}»", short(&page(&block.detail, &name, &block.place), 50)));
            }
            parts.join(" · ")
        };
        lines.push(format!("{}–{}  {what}", hm(block.start), hm(block.end)));
    }
    Outcome::tell(format!("{label}, rato a rato"), lines.join("\n"))
}

/// «¿Cuándo vi lo de Fallout?»: dónde sale en el diario, lo último primero.
pub fn find(d: &mut Daemon, what: &str) -> Outcome {
    let words = significant(what);
    if words.is_empty() {
        return Outcome::fail("¿Qué busco?");
    }
    let apps_known = d.apps.clone();
    let hits = match diary(d).and_then(|db| journal::find(db, &words, |app| apps::label(&apps_known, app), 8).map_err(failed)) {
        Ok(hits) => hits,
        Err(why) => return why,
    };
    let what = what.trim();
    let Some(last) = hits.first() else {
        return Outcome::done(format!("«{what}» no sale en el diario"));
    };
    let today = Local::now().date_naive();
    let name = |hit: &journal::Hit| if hit.kind == "focus" { apps::label(&apps_known, &hit.app) } else { hit.app.clone() };
    let doing = match last.kind {
        "media" => format!("sonando en {}", name(last)),
        "file" => format!("lo abrió {}", name(last)),
        _ => format!("en {}", name(last)),
    };
    let say = format!("«{what}»: la última vez, {} a las {}, {doing}", day_name(date(last.start), today), hm(last.start));
    let lines: Vec<String> = hits
        .iter()
        .map(|hit| {
            let when = if hit.end - hit.start >= 60_000 { format!("{}–{}", hm(hit.start), hm(hit.end)) } else { hm(hit.start) };
            let app = name(hit);
            let mut parts = vec![app.clone()];
            match hit.kind {
                "file" => parts.push(config::tilde(&hit.place)),
                _ if !hit.place.is_empty() => parts.push(if hit.place.starts_with('/') { config::tilde(&hit.place) } else { hit.place.clone() }),
                _ => {}
            }
            if hit.kind != "file" && !hit.title.is_empty() {
                parts.push(format!("«{}»", short(&page(&hit.title, &app, &hit.place), 50)));
            }
            format!("{}, {when}: {}", capital(&day_short(date(hit.start), today)), parts.join(" · "))
        })
        .collect();
    Outcome::tell(say, lines.join("\n"))
}

/// «¿Cuánto rato estuve en YouTube esta semana?»; sin nada que buscar, todo lo de pantalla.
pub(super) fn spent(d: &mut Daemon, what: &str, said: &str) -> Outcome {
    let span = match span_of(said) {
        Ok(span) => span,
        Err(why) => return why,
    };
    let (from, to) = (span.from.timestamp_millis(), span.to.timestamp_millis());
    let words = significant(what);
    let apps_known = d.apps.clone();
    let rows = match diary(d).and_then(|db| journal::rows(db, from, to).map_err(failed)) {
        Ok(rows) => rows,
        Err(why) => return why,
    };
    let need = if words.len() >= 3 { words.len() - 1 } else { words.len() };
    let mut labels: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut total = 0;
    let mut days: Vec<(NaiveDate, i64)> = Vec::new();
    let mut titles: Vec<(String, i64)> = Vec::new();
    for row in &rows {
        let name = labels.entry(row.app.clone()).or_insert_with(|| apps::label(&apps_known, &row.app)).clone();
        if !words.is_empty() && journal::matches(&row.hay(&name), &words) < need {
            continue;
        }
        let ms = row.end - row.start;
        total += ms;
        match days.iter_mut().find(|(day, _)| *day == date(row.start)) {
            Some((_, t)) => *t += ms,
            None => days.push((date(row.start), ms)),
        }
        let title = if row.title.is_empty() { name.clone() } else { format!("{name} · {}", page(&row.title, &name, &row.site)) };
        match titles.iter_mut().find(|(t, _)| *t == title) {
            Some((_, t)) => *t += ms,
            None => titles.push((title, ms)),
        }
    }
    let label = capital(&span.label);
    let what = what.trim();
    let on = if words.is_empty() { "de pantalla".to_string() } else { format!("en «{what}»") };
    if total < 60_000 {
        return Outcome::done(if words.is_empty() { format!("{label}: nada en el diario") } else { format!("{label}: nada de «{what}»") });
    }
    let mut lines = Vec::new();
    let today = Local::now().date_naive();
    if days.len() > 1 {
        for (day, ms) in &days {
            lines.push(format!("{}: {}", capital(&day_short(*day, today)), spoken(*ms)));
        }
    }
    titles.sort_by_key(|(_, ms)| std::cmp::Reverse(*ms));
    for (title, ms) in titles.iter().filter(|(_, ms)| *ms >= 60_000).take(4) {
        lines.push(format!("{}, {}", short(title, 70), spoken(*ms)));
    }
    let say = format!("{label}: {} {on}", spoken(total));
    if lines.is_empty() { Outcome::done(say) } else { Outcome::tell(say, lines.join("\n")) }
}

/// «¿Qué sonó ayer?»
pub(super) fn heard(d: &mut Daemon, said: &str) -> Outcome {
    let span = match span_of(said) {
        Ok(span) => span,
        Err(why) => return why,
    };
    let found = match diary(d).and_then(|db| journal::recall(db, span.from.timestamp_millis(), span.to.timestamp_millis()).map_err(failed)) {
        Ok(found) => found,
        Err(why) => return why,
    };
    let label = capital(&span.label);
    let total: i64 = found.media.iter().map(|m| m.ms).sum();
    if found.media.is_empty() {
        return Outcome::done(format!("{label} no sonó nada"));
    }
    let lines: Vec<String> = found.media.iter().take(10).map(|m| format!("{} ({}), {}", track(m), m.player, spoken(m.ms.max(60_000)))).collect();
    Outcome::tell(format!("{label} sonó {}", spoken(total.max(60_000))), lines.join("\n"))
}

/// «¿Qué archivos abrí ayer?»
pub(super) fn opened(d: &mut Daemon, said: &str) -> Outcome {
    let span = match span_of(said) {
        Ok(span) => span,
        Err(why) => return why,
    };
    let found = match diary(d).and_then(|db| journal::recall(db, span.from.timestamp_millis(), span.to.timestamp_millis()).map_err(failed)) {
        Ok(found) => found,
        Err(why) => return why,
    };
    let label = capital(&span.label);
    if found.files.is_empty() {
        return Outcome::done(format!("{label}: ningún archivo en el diario"));
    }
    let several = found.files.iter().any(|f| date(f.time) != date(found.files[0].time));
    let today = Local::now().date_naive();
    let lines: Vec<String> = found
        .files
        .iter()
        .take(12)
        .map(|f| {
            let when = if several { format!("{}, {}", capital(&day_short(date(f.time), today)), hm(f.time)) } else { hm(f.time) };
            let folder = Path::new(&f.path).parent().map(|p| config::tilde(&p.to_string_lossy())).unwrap_or_default();
            format!("{when} · {} · {} · {folder}", file_name(&f.path), f.app)
        })
        .collect();
    let count = found.files.len();
    Outcome::tell(format!("{label}: {count} archivo{}", if count == 1 { "" } else { "s" }), lines.join("\n"))
}

/// Las preguntas al diario.
pub(super) fn diary_order(d: &mut Daemon, p: &Parse) -> Outcome {
    let w = p.text("w").to_string();
    let q = p.text("q").to_string();
    match p.intent.as_str() {
        "recall.find" => find(d, &q),
        "recall.time" => spent(d, &q, &w),
        "recall.timeline" => timeline(d, &w),
        "recall.media" => heard(d, &w),
        "recall.files" => opened(d, &w),
        "recall.full" => report(d, &w),
        "recall.close" => Outcome::from(veyl(d, "diary.close", json!([])).map(|_| String::new())),
        "recall.open" => {
            let day = match span_of(&w) {
                Ok(span) => span.from.date_naive().format("%Y-%m-%d").to_string(),
                Err(why) => return why,
            };
            Outcome::from(veyl(d, "diary.open", json!([day, q])).map(|_| String::new()))
        }
        _ => recall(d, &w),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn titles() {
        assert_eq!(tidy("Rust by Example — Mozilla Firefox", "Firefox"), "Rust by Example");
        assert_eq!(tidy("main.rs - veyld - Visual Studio Code", "Visual Studio Code"), "main.rs - veyld");
        assert_eq!(tidy("nixos-pc: veyl", "kitty"), "nixos-pc: veyl");
    }

    #[test]
    fn recall_text() {
        let part = |name: &str, ms: i64, within: &[(&str, i64)]| journal::Part {
            name: name.into(),
            ms,
            within: within.iter().map(|(w, ms)| (w.to_string(), *ms)).collect(),
        };
        let at = |h: u32, m: u32| Local.with_ymd_and_hms(2026, 11, 12, h, m, 0).unwrap().timestamp_millis();
        let found = journal::Recall {
            total: 13_200_000,
            first: at(15, 4),
            last: at(20, 41),
            apps: vec![
                journal::AppTime {
                    app: "firefox".into(),
                    ms: 5_100_000,
                    titles: vec![("Rust by Example — Mozilla Firefox".into(), 3_000_000), ("YouTube — Mozilla Firefox".into(), 2_100_000)],
                    sites: vec![
                        part("doc.rust-lang.org", 3_000_000, &[("Rust by Example — Mozilla Firefox", 3_000_000)]),
                        part("youtube.com", 2_100_000, &[("Fallout 3 - YouTube — Mozilla Firefox", 1_500_000)]),
                    ],
                    ..Default::default()
                },
                journal::AppTime {
                    app: "kitty".into(),
                    ms: 3_300_000,
                    titles: vec![("fish".into(), 3_300_000)],
                    folders: vec![part("/x/dev/veyl", 3_300_000, &[("claude", 2_000_000), ("cargo", 900_000), ("ls", 10_000)])],
                    ..Default::default()
                },
                journal::AppTime { app: "zed".into(), ms: 600_000, titles: vec![("main.rs — veyld".into(), 600_000)], ..Default::default() },
            ],
            agents: vec![journal::AgentTime { label: "Claude".into(), name: "veyld".into(), folder: "/x/dev/veyl".into(), ms: 4_200_000 }],
            media: vec![journal::MediaTime { player: "Spotify".into(), title: "Song".into(), artist: "Band".into(), ms: 2_400_000 }],
            devices: vec![journal::DeviceTime { kind: "micro".into(), ms: 2_700_000, times: 2, app: "firefox".into(), site: "meet.google.com".into() }],
            files: vec![crate::recent::Used { path: "/x/informe.pdf".into(), app: "Papers".into(), time: at(16, 0) }],
            away: vec![("bloqueado".into(), 2_400_000), ("inactivo".into(), 30_000)],
        };
        let out = describe_recall(&[], "el jueves 12 de noviembre por la tarde (15:00–21:00)", &found);
        assert_eq!(out.say, "El jueves 12 de noviembre por la tarde (15:00–21:00): 3 h 40 min de pantalla");
        assert_eq!(
            out.answer.unwrap(),
            "De 15:04 a 20:41, con 40 min con la pantalla bloqueada
firefox, 1 h 25 min: doc.rust-lang.org («Rust by Example»), youtube.com («Fallout 3»)
kitty, 55 min, en /x/dev/veyl (claude, cargo)
zed, 10 min: «main.rs — veyld»
Claude trabajó 1 h 10 min en /x/dev/veyl («veyld»)
Sonó 40 min: «Song» de Band
Micro abierto 45 min en 2 veces, sobre todo en firefox (meet.google.com)
Archivos: informe.pdf"
        );
    }

    #[test]
    fn search_words() {
        assert_eq!(significant("el vídeo de Fallout 3"), vec!["fallout", "3"]);
        assert_eq!(significant("lo del dentista"), vec!["dentista"]);
        assert_eq!(page("Fallout 3 - YouTube — Mozilla Firefox", "Firefox", "youtube.com"), "Fallout 3");
    }
}
