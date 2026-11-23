//! Las órdenes sobre ventanas, niveles, monitores y vistas.

use super::*;

/// «la terminal» es el terminal de Veyl.
pub(super) fn terminal_asked(query: &str) -> bool {
    matches!(fold(query.trim()).as_str(), "terminal" | "la terminal" | "una terminal" | "consola" | "la consola" | "shell")
}

pub(super) fn terminal_program(d: &Daemon) -> String {
    let first = d.settings.terminal.split_whitespace().next().unwrap_or("kitty");
    first.rsplit('/').next().unwrap_or(first).to_string()
}

/// La ventana que mejor casa con lo dicho: por su aplicación, primero, y
/// por su título; a igualdad, la que tuvo el foco más hace poco.
pub(super) fn pick(d: &Daemon, query: &str) -> Option<Window> {
    let q = fold(query.trim());
    let app = if terminal_asked(query) { None } else { find_app(d, query) };
    let terminal = terminal_asked(query).then(|| terminal_program(d));
    let mut best: Option<(f32, Window)> = None;
    for window in d.hypr.clients() {
        let named = apps::label(&d.apps, &window.class);
        let mut score = apps::likeness(&q, window.app()).max(apps::likeness(&q, &named)).max(apps::likeness(&q, &window.title) * 0.6);
        if app.is_some_and(|a| a.owns(&window.class)) {
            score = score.max(90.0);
        }
        if terminal.as_deref().is_some_and(|t| fold(window.app()) == fold(t)) {
            score = 100.0;
        }
        let better = match &best {
            None => score > 0.0,
            Some((top, old)) => score > *top || (score == *top && window.recency < old.recency),
        };
        if better {
            best = Some((score, window));
        }
    }
    best.map(|(_, w)| w)
}

pub(super) fn find_app<'a>(d: &'a Daemon, query: &str) -> Option<&'a App> {
    let open: Vec<String> = d.hypr.clients().into_iter().map(|w| w.class).collect();
    apps::find(&d.apps, query, &open)
}

pub(super) fn launch(d: &Daemon, command: &str) -> Result<(), String> {
    d.hypr.dispatch(&format!("hl.dsp.exec_cmd({})", lua(command)))
}

pub(super) fn open(d: &mut Daemon, query: &str) -> Result<String, String> {
    let before: Vec<String> = d.hypr.clients().into_iter().filter(|w| owns(d, query, w)).map(|w| w.address).collect();
    let said = if terminal_asked(query) {
        launch(d, &d.settings.terminal)?;
        "Abro el terminal".to_string()
    } else {
        let app = find_app(d, query).ok_or_else(|| format!("No encuentro «{query}»"))?;
        launch(d, &app.command(&d.settings.terminal))?;
        format!("Abro {}", app.name)
    };
    remember(d, query, None);
    d.context.launched = Some((Instant::now(), before));
    Ok(said)
}

/// Si la ventana es de la aplicación de la que se habla.
pub(super) fn owns(d: &Daemon, query: &str, window: &Window) -> bool {
    if terminal_asked(query) {
        return fold(window.app()) == fold(&terminal_program(d));
    }
    find_app(d, query).is_some_and(|a| a.owns(&window.class)) || apps::likeness(&fold(query), window.app()) >= 80.0
}

/// La ventana de la que se habla: la que señala el contexto, la de la
/// aplicación nombrada o ninguna (entonces, la que tiene el foco). Si la
/// aplicación se acaba de abrir, se espera a que salga su ventana nueva.
pub(super) fn target(d: &mut Daemon, p: &Parse) -> Result<Option<Window>, String> {
    let app = p.text("app").to_string();
    let address = p.text("window");
    if !address.is_empty()
        && let Some(w) = d.hypr.clients().into_iter().find(|w| w.address == address)
    {
        remember(d, &app, Some(&w));
        return Ok(Some(w));
    }
    if app.is_empty() {
        return Ok(None);
    }
    let window = match fresh_window(d, &app) {
        Some(w) => w,
        None => pick(d, &app).ok_or_else(|| format!("No hay ninguna ventana de «{app}»"))?,
    };
    remember(d, &app, Some(&window));
    Ok(Some(window))
}

/// Cuánto se espera a que salga la ventana de lo que se acaba de abrir.
pub(super) const ARRIVAL: Duration = Duration::from_secs(8);

pub(super) fn fresh_window(d: &Daemon, app: &str) -> Option<Window> {
    let (at, before) = d.context.launched.clone()?;
    if fold(&d.context.app) != fold(app) || at.elapsed() > Duration::from_secs(30) {
        return None;
    }
    loop {
        let new = d.hypr.clients().into_iter().filter(|w| !before.contains(&w.address) && owns(d, app, w)).min_by_key(|w| w.recency);
        if new.is_some() || at.elapsed() > ARRIVAL {
            return new;
        }
        thread::sleep(Duration::from_millis(150));
    }
}

/// De los clientes de Hyprland, lo que no lleva `Window`: el sitio en la
/// tira, si flota y si está a pantalla completa.
pub(super) fn client(d: &Daemon, address: &str) -> Option<Value> {
    d.hypr.json("clients")?.as_array()?.iter().find(|c| c.get("address").and_then(Value::as_str) == Some(address)).cloned()
}

/// Las ventanas de un espacio, de izquierda a derecha (sin las flotantes).
pub(super) fn strip(d: &Daemon, workspace: i64) -> Vec<String> {
    let Some(all) = d.hypr.json("clients") else {
        return Vec::new();
    };
    let mut windows: Vec<(i64, i64, String)> = all
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c.pointer("/workspace/id").and_then(Value::as_i64) == Some(workspace) && c.get("floating").and_then(Value::as_bool) != Some(true))
        .filter_map(|c| {
            let x = c.pointer("/at/0").and_then(Value::as_i64)?;
            let y = c.pointer("/at/1").and_then(Value::as_i64)?;
            Some((x, y, c.get("address")?.as_str()?.to_string()))
        })
        .collect();
    windows.sort();
    windows.into_iter().map(|(_, _, a)| a).collect()
}

pub(super) fn active_workspace(d: &Daemon) -> Option<i64> {
    d.hypr.json("activeworkspace")?.get("id")?.as_i64()
}

/// Las ventanas que cierra «cierra todas menos esta»: todas (o las del
/// nivel) menos la que tiene el foco o las de la aplicación nombrada.
pub(super) fn others(d: &Daemon, p: &Parse) -> Vec<Window> {
    let keep = p.text("keep").trim().to_string();
    let here = active_workspace(d);
    let active = d.hypr.active().map(|w| w.address).unwrap_or_default();
    let mine = keep.is_empty() || matches!(fold(&keep).as_str(), "esta" | "esto" | "esta ventana" | "la actual" | "la de ahora" | "la que tengo delante");
    d.hypr
        .clients()
        .into_iter()
        .filter(|w| p.text("scope") != "level" || Some(w.workspace_id) == here)
        .filter(|w| match () {
            _ if p.flag("all") == Some(true) => true,
            _ if mine => w.address != active,
            _ => !owns(d, &keep, w),
        })
        .collect()
}

pub(super) fn focus(d: &Daemon, window: &Window) -> Result<(), String> {
    d.hypr.dispatch(&format!("hl.dsp.focus({{ window = {} }})", lua(&format!("address:{}", window.address))))
}

/// El nivel `n` del monitor con el foco, en Lua: con hyprsplit, el suyo.
pub(super) fn level_lua(n: u32) -> String {
    format!(r#"local ok, hs = pcall(require, "hyprsplit") local target = ok and hs.get_workspace_string("{n}") or "{n}""#)
}

pub(super) fn levels(d: &Daemon) -> u32 {
    if d.split > 0 { d.split } else { 10 }
}

/// El nivel en el que estás.
pub(super) fn current_level(d: &Daemon) -> Option<u32> {
    let id = d.hypr.json("activeworkspace")?.get("id")?.as_i64()? as u32;
    Some(if d.split > 0 { (id.max(1) - 1) % d.split + 1 } else { id })
}

pub(super) fn go_level(d: &Daemon, n: u32) -> Result<String, String> {
    d.hypr.eval(&format!(
        r#"local ok, hs = pcall(require, "hyprsplit") if ok then hl.dispatch(hs.dsp.focus({{ workspace = {n} }})) else hl.dispatch(hl.dsp.focus({{ workspace = "{n}" }})) end"#
    ))?;
    Ok(format!("Nivel {n}"))
}

/// Un monitor: su id, su centro y el espacio que enseña.
pub(super) struct Screen {
    id: i64,
    center: (i64, i64),
    workspace: i64,
    focused: bool,
}

/// Los monitores, de izquierda a derecha (y de arriba abajo).
pub(super) fn screens(d: &Daemon) -> Vec<Screen> {
    let mut all: Vec<Screen> = d
        .hypr
        .json("monitors")
        .and_then(|m| m.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|m| {
            let num = |k: &str| m.get(k).and_then(Value::as_i64);
            let scale = m.get("scale").and_then(Value::as_f64).unwrap_or(1.0).max(0.1);
            let (w, h) = ((num("width")? as f64 / scale) as i64, (num("height")? as f64 / scale) as i64);
            Some(Screen {
                id: num("id")?,
                center: (num("x")? + w / 2, num("y")? + h / 2),
                workspace: m.pointer("/activeWorkspace/id")?.as_i64()?,
                focused: m.get("focused").and_then(Value::as_bool) == Some(true),
            })
        })
        .collect();
    all.sort_by_key(|m| (m.center.0, m.center.1));
    all
}

/// El monitor más cercano hacia un lado; «other», el siguiente (y vuelta al primero).
pub(super) fn beside<'a>(all: &'a [Screen], from: &Screen, dir: &str) -> Option<&'a Screen> {
    let (fx, fy) = from.center;
    let toward = |m: &&Screen| {
        let (dx, dy) = (m.center.0 - fx, m.center.1 - fy);
        match dir {
            "left" => dx < 0,
            "right" => dx > 0,
            "up" => dy < 0,
            "down" => dy > 0,
            _ => false,
        }
    };
    let distance = |m: &&Screen| {
        let (dx, dy) = ((m.center.0 - fx).abs(), (m.center.1 - fy).abs());
        if matches!(dir, "left" | "right") { (dx, dy) } else { (dy, dx) }
    };
    match dir {
        "left" | "right" | "up" | "down" => all.iter().filter(toward).min_by_key(distance),
        _ => {
            let at = all.iter().position(|m| m.id == from.id)?;
            all.get((at + 1) % all.len())
        }
    }
}

pub(super) fn side_name(dir: &str) -> &'static str {
    match dir {
        "left" => "de la izquierda",
        "right" => "de la derecha",
        "up" => "de arriba",
        "down" => "de abajo",
        _ => "más",
    }
}

pub(super) fn windows(d: &mut Daemon, p: &Parse) -> Result<String, String> {
    let app = p.text("app").to_string();
    match p.intent.as_str() {
        "window.focus" => {
            let window = if p.text("window").is_empty() { pick(d, &app) } else { target(d, p).ok().flatten() };
            if let Some(window) = window {
                focus(d, &window)?;
                remember(d, &app, Some(&window));
                return Ok(apps::label(&d.apps, &window.class));
            }
            if find_app(d, &app).is_none() && !terminal_asked(&app) {
                // Quizá se instaló algo después de leerlas.
                d.reload_apps();
            }
            open(d, &app)
        }
        "app.open" => {
            if find_app(d, &app).is_none() && !terminal_asked(&app) {
                d.reload_apps();
            }
            open(d, &app)
        }
        "window.close" => {
            let Some(window) = target(d, p)? else {
                d.hypr.dispatch("hl.dsp.window.close()")?;
                return Ok("Ventana cerrada".into());
            };
            d.hypr.dispatch(&format!("hl.dsp.window.close({{ window = {} }})", lua(&format!("address:{}", window.address))))?;
            Ok(format!("Cierro {}", apps::label(&d.apps, &window.class)))
        }
        "window.fullscreen" => {
            let maximized = p.text("mode") == "maximized";
            let window = target(d, p)?;
            if let Some(w) = &window {
                focus(d, w)?;
            }
            let address = window.map(|w| w.address).or_else(|| d.hypr.active().map(|w| w.address)).ok_or("No hay ninguna ventana")?;
            // «Ponlo a pantalla completa» la pone, no la alterna: si ya lo está, se queda.
            let state = client(d, &address).and_then(|c| c.get("fullscreen")?.as_i64()).unwrap_or(0);
            let is = state == if maximized { 1 } else { 2 };
            let (done, undone) = if maximized { ("Maximizada", "Ya no está maximizada") } else { ("Pantalla completa", "Sin pantalla completa") };
            if p.flag("on").is_some_and(|on| on == is) {
                return Ok(if is { format!("Ya estaba: {}", done.to_lowercase()) } else { undone.to_string() });
            }
            let mode = if maximized { "maximized" } else { "fullscreen" };
            d.hypr.dispatch(&format!(r#"hl.dsp.window.fullscreen({{ mode = "{mode}", action = "toggle" }})"#))?;
            Ok(if is { undone.into() } else { done.into() })
        }
        "window.float" => {
            if let Some(w) = target(d, p)? {
                focus(d, &w)?;
            }
            d.hypr.dispatch(r#"hl.dsp.window.float({ action = "toggle" })"#)?;
            Ok("Flotante".into())
        }
        "window.place" => {
            let left = p.text("side") == "left";
            let level = current_level(d).ok_or("No sé en qué nivel estás")?;
            let here = active_workspace(d).ok_or("No sé en qué nivel estás")?;
            let window = match target(d, p)? {
                Some(w) => w,
                None => d.hypr.active().ok_or("No hay ninguna ventana")?,
            };
            let name = apps::label(&d.apps, &window.class);
            let address = lua(&format!("address:{}", window.address));
            // Primero, a este nivel (el nivel se calcula antes de que el foco cambie de monitor).
            if window.workspace_id != here {
                d.hypr.eval(&format!(
                    "{} hl.dispatch(hl.dsp.focus({{ window = {address} }})) hl.dispatch(hl.dsp.window.move({{ workspace = target, follow = true }}))",
                    level_lua(level)
                ))?;
            } else {
                focus(d, &window)?;
            }
            // Luego, tantos pasos como haya hasta el borde: ni uno más, que
            // en el borde se iría al otro monitor.
            let row = strip(d, here);
            let at = row.iter().position(|a| *a == window.address).unwrap_or(0);
            let steps = if left { at } else { row.len().saturating_sub(at + 1) };
            let dir = if left { "left" } else { "right" };
            if steps > 0 {
                d.hypr.eval(&format!(r#"for _ = 1, {steps} do hl.dispatch(hl.dsp.window.move({{ direction = "{dir}" }})) end"#))?;
            }
            Ok(format!("{name}, a la {}", if left { "izquierda" } else { "derecha" }))
        }
        "window.others" => {
            let close = others(d, p);
            if close.is_empty() {
                return Ok("No hay nada más que cerrar".into());
            }
            for w in &close {
                d.hypr.dispatch(&format!("hl.dsp.window.close({{ window = {} }})", lua(&format!("address:{}", w.address))))?;
            }
            Ok(format!("Cierro {} ventana{}", close.len(), if close.len() == 1 { "" } else { "s" }))
        }
        "window.direction" | "window.shift" => {
            let dir = if p.text("dir") == "left" { "left" } else { "right" };
            let verb = if p.intent == "window.shift" { "hl.dsp.window.move" } else { "hl.dsp.focus" };
            if p.intent == "window.shift"
                && let Some(w) = target(d, p)?
            {
                focus(d, &w)?;
            }
            d.hypr.dispatch(&format!(r#"{verb}({{ direction = "{dir}" }})"#))?;
            Ok(if dir == "left" { "A la izquierda".into() } else { "A la derecha".into() })
        }
        "window.monitor" => {
            let window = match target(d, p)? {
                Some(w) => w,
                None => d.hypr.active().ok_or("No hay ninguna ventana")?,
            };
            let mine = d.hypr.active().is_some_and(|a| a.address == window.address);
            let monitors = screens(d);
            if monitors.len() < 2 {
                return Err("Solo hay un monitor".into());
            }
            let from = client(d, &window.address).and_then(|c| c.get("monitor")?.as_i64());
            let from = monitors.iter().find(|m| Some(m.id) == from).or_else(|| monitors.iter().find(|m| m.focused)).ok_or("No sé en qué monitor está")?;
            let to = match p.number("n") {
                Some(n) => monitors.get((n.max(1) - 1) as usize).ok_or_else(|| format!("No hay monitor {n}: hay {}", monitors.len()))?,
                None => beside(&monitors, from, p.text("dir")).ok_or_else(|| match p.text("dir") {
                    "left" => "No hay ningún monitor más a la izquierda".to_string(),
                    "right" => "No hay ningún monitor más a la derecha".to_string(),
                    "up" => "No hay ningún monitor más arriba".to_string(),
                    _ => "No hay ningún monitor más abajo".to_string(),
                })?,
            };
            let name = apps::label(&d.apps, &window.class);
            if to.id == from.id {
                return Ok(format!("{name} ya está en ese monitor"));
            }
            // Al nivel que enseña el otro monitor; la tuya se lleva contigo.
            let address = lua(&format!("address:{}", window.address));
            let mut lua =
                format!("hl.dispatch(hl.dsp.window.move({{ window = {address}, workspace = {}, follow = false }}))", self::lua(&to.workspace.to_string()));
            if mine {
                lua += &format!(" hl.dispatch(hl.dsp.focus({{ window = {address} }}))");
            }
            d.hypr.eval(&lua)?;
            let (dx, dy) = (to.center.0 - from.center.0, to.center.1 - from.center.1);
            let dir = match () {
                _ if dx.abs() >= dy.abs() && dx < 0 => "left",
                _ if dx.abs() >= dy.abs() => "right",
                _ if dy < 0 => "up",
                _ => "down",
            };
            Ok(format!("{name}, al monitor {}", side_name(dir)))
        }
        "window.move" => {
            let n = (p.number("n").unwrap_or(1).max(1) as u32).min(levels(d));
            // Otra ventana se manda y se queda allí; la tuya se lleva contigo.
            let window = target(d, p)?;
            let follow = window.is_none();
            let mut name = "La ventana".to_string();
            if let Some(window) = &window {
                focus(d, window)?;
                name = apps::label(&d.apps, &window.class);
            }
            remember_level(d, n);
            let mut lua = format!("{} hl.dispatch(hl.dsp.window.move({{ workspace = target, follow = false }}))", level_lua(n));
            if follow {
                lua += " hl.dispatch(hl.dsp.focus({ workspace = target, on_current_monitor = true }))";
            }
            d.hypr.eval(&lua)?;
            Ok(format!("{name}, al nivel {n}"))
        }
        "level.go" => {
            let n = (p.number("n").unwrap_or(1).max(1) as u32).min(levels(d));
            remember_level(d, n);
            go_level(d, n)
        }
        "level.step" => {
            let now = current_level(d).ok_or("No sé en qué nivel estás")?;
            let by = p.number("by").unwrap_or(1);
            let next = (now as i64 + by).clamp(1, levels(d) as i64) as u32;
            if next == now {
                return Ok(if by < 0 { format!("Ya estás arriba, en el nivel {now}") } else { format!("Ya estás abajo, en el nivel {now}") });
            }
            go_level(d, next)
        }
        _ => Err(format!("No sé hacer {}", p.intent)),
    }
}

// --- Vistas --------------------------------------------------------------------

/// Un nombre de espacio en Lua.
pub(super) fn workspace_lua(view: &View) -> String {
    lua(&view.workspace())
}

/// Lleva ventanas al espacio de una vista, sin seguirlas.
pub(super) fn bring(d: &Daemon, view: &View, addresses: &[String]) -> Result<(), String> {
    if addresses.is_empty() {
        return Ok(());
    }
    let target = workspace_lua(view);
    let lua: Vec<String> = addresses
        .iter()
        .map(|a| format!("hl.dispatch(hl.dsp.window.move({{ window = {}, workspace = {target}, follow = false }}))", lua(&format!("address:{a}"))))
        .collect();
    d.hypr.eval(&lua.join(" "))
}

/// Va al espacio de una vista: si no existe, nace en el monitor con el foco.
pub(super) fn enter(d: &Daemon, view: &View) -> Result<(), String> {
    d.hypr.dispatch(&format!("hl.dsp.focus({{ workspace = {} }})", workspace_lua(view)))
}

pub(super) fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// La vista de la que se habla: la nombrada o, sin nombre, la del espacio en el que estás.
pub(super) fn view_named(d: &Daemon, name: &str) -> Result<usize, String> {
    if name.trim().is_empty() {
        let here = d.hypr.json("activeworkspace").and_then(|w| Some(w.get("name")?.as_str()?.to_string())).unwrap_or_default();
        return d.views.of_workspace(&here).ok_or_else(|| "Aquí no hay ninguna vista: ¿cuál?".to_string());
    }
    d.views.find(name).ok_or_else(|| format!("No hay ninguna vista «{}»", views::tidy(name)))
}

/// Hace la vista con lo elegido: la guarda, abre su espacio y le lleva las ventanas.
pub fn make_view(d: &mut Daemon, name: &str, candidates: &[Candidate], choice: Choice) -> Outcome {
    if choice.is_empty() {
        return Outcome::fail(format!("No encuentro nada abierto que sea de «{name}»"));
    }
    let now = crate::daemon::now();
    let mut view = View { name: name.to_string(), created: now, used: now, ..Default::default() };
    let mut moved = Vec::new();
    let mut lines = Vec::new();
    for (i, keys) in choice {
        let c = &candidates[i];
        view.members.push(views::member(c, keys));
        view.origin.insert(c.window.address.clone(), c.window.workspace_id);
        moved.push(c.window.address.clone());
        lines.push(format!("- {}: {}", c.app, short(&tidy(&c.window.title, &c.app), 60)));
    }
    let done = enter(d, &view).and_then(|_| bring(d, &view, &moved));
    d.views.put(view);
    let saved = d.views.save();
    d.views_changed();
    if let Err(why) = done.and(saved) {
        return Outcome::fail(why);
    }
    remember(d, "", None);
    Outcome::tell(format!("Vista «{name}», con {}", plural(moved.len(), "ventana", "ventanas")), lines.join("\n"))
}

/// Abre una vista. Si su espacio ya existe, va allí; si no (o si se pide
/// recrearla), junta las ventanas que casan y abre las que faltan.
pub(super) fn open_view(d: &mut Daemon, name: &str, again: bool) -> Result<String, String> {
    let at = view_named(d, name)?;
    let view = d.views.list[at].clone();
    let alive = d
        .hypr
        .json("workspaces")
        .and_then(|w| w.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .any(|w| w.get("name").and_then(Value::as_str) == Some(view.name.as_str()) && w.get("windows").and_then(Value::as_i64).unwrap_or(0) > 0);
    d.views.list[at].used = crate::daemon::now();
    let _ = d.views.save();
    if alive && !again {
        enter(d, &view)?;
        return Ok(format!("Vista «{}»", view.name));
    }
    let candidates = d.candidates();
    let (pairs, missing) = views::gather(&view, &candidates);
    let brought: Vec<String> =
        pairs.iter().map(|(_, i)| &candidates[*i]).filter(|c| c.window.workspace != view.name).map(|c| c.window.address.clone()).collect();
    enter(d, &view)?;
    bring(d, &view, &brought)?;
    for (_, i) in &pairs {
        let c = &candidates[*i];
        if c.window.workspace != view.name {
            d.views.list[at].origin.insert(c.window.address.clone(), c.window.workspace_id);
        }
    }
    let mut opened = 0;
    for m in &missing {
        if let Some(command) = views::relaunch(&view.members[*m], &d.apps, &d.settings.terminal) {
            // Al espacio de la vista aunque mientras tanto te vayas a otro.
            d.hypr.eval(&format!("hl.exec_cmd({}, {{ workspace = {} }})", lua(&command), lua(&format!("{} silent", view.workspace()))))?;
            opened += 1;
        }
    }
    let _ = d.views.save();
    let mut said = format!("Vista «{}»", view.name);
    if !brought.is_empty() {
        said += &format!(": traigo {}", plural(brought.len(), "ventana", "ventanas"));
    }
    if opened > 0 {
        let what = if opened == 1 { "la que faltaba".to_string() } else { format!("las {opened} que faltaban") };
        said += &format!("{} abro {what}", if brought.is_empty() { ":" } else { " y" });
    }
    Ok(said)
}

/// Deshace una vista: cada ventana vuelve al nivel del que vino (si ya no
/// existe, al primero) y la vista queda guardada para recrearla.
pub(super) fn undo_view(d: &mut Daemon, name: &str) -> Result<String, String> {
    let at = view_named(d, name)?;
    let view = d.views.list[at].clone();
    let inside: Vec<Window> = d.hypr.clients().into_iter().filter(|w| w.workspace == view.name).collect();
    if inside.is_empty() {
        return Ok(format!("La vista «{}» no está abierta", view.name));
    }
    let lua: Vec<String> = inside
        .iter()
        .map(|w| {
            let back = view.origin.get(&w.address).copied().filter(|id| *id > 0).unwrap_or(1);
            format!("hl.dispatch(hl.dsp.window.move({{ window = {}, workspace = {back}, follow = false }}))", lua(&format!("address:{}", w.address)))
        })
        .collect();
    let first = view.origin.get(&inside[0].address).copied().filter(|id| *id > 0).unwrap_or(1);
    d.hypr.eval(&format!("{} hl.dispatch(hl.dsp.focus({{ workspace = {first} }}))", lua.join(" ")))?;
    d.views.list[at].origin.clear();
    let _ = d.views.save();
    Ok(format!("Vista «{}» recogida: cada ventana, a su nivel", view.name))
}

/// Guarda en una vista lo que tiene ahora su espacio (o, si no es de
/// ninguna, el nivel en el que estás, como una vista nueva).
pub(super) fn save_view(d: &mut Daemon, name: &str) -> Result<String, String> {
    let here = d.hypr.json("activeworkspace").ok_or("No sé dónde estás")?;
    let here_name = here.get("name").and_then(Value::as_str).unwrap_or("").to_string();
    let mut here_id = here.get("id").and_then(Value::as_i64).unwrap_or(0);
    let name = if name.trim().is_empty() {
        d.views.of_workspace(&here_name).map(|i| d.views.list[i].name.clone()).ok_or("¿Con qué nombre?")?
    } else {
        d.views.find(name).map(|i| d.views.list[i].name.clone()).unwrap_or_else(|| views::tidy(name))
    };
    // Si la vista está abierta, lo de su espacio, estés donde estés.
    let spaces = d.hypr.json("workspaces").and_then(|w| w.as_array().cloned()).unwrap_or_default();
    if let Some(id) = spaces.iter().find(|w| w.get("name").and_then(Value::as_str) == Some(name.as_str())).and_then(|w| w.get("id")?.as_i64()) {
        here_id = id;
    }
    let candidates = d.candidates();
    let choice = views::level(&candidates, here_id);
    if choice.is_empty() {
        return Err("En este nivel no hay nada que guardar".into());
    }
    let now = crate::daemon::now();
    let mut view = d.views.find(&name).filter(|i| fold(&d.views.list[*i].name) == fold(&name)).map(|i| d.views.list[i].clone()).unwrap_or_default();
    view.name = name.clone();
    view.created = if view.created == 0 { now } else { view.created };
    view.used = now;
    // Las que ya eran suyas conservan sus palabras.
    let old = std::mem::take(&mut view.members);
    view.members = choice
        .into_iter()
        .map(|(i, keys)| {
            let c = &candidates[i];
            let keys = old.iter().find(|m| !m.keys.is_empty() && views::owns(m, c)).map_or(keys, |m| m.keys.clone());
            views::member(c, keys)
        })
        .collect();
    let count = view.members.len();
    // Un nivel numerado se queda como está: la vista nace la próxima vez que se abra.
    d.views.put(view);
    d.views.save()?;
    d.views_changed();
    Ok(format!("Vista «{name}» guardada, con {}", plural(count, "ventana", "ventanas")))
}

pub(super) fn views_order(d: &mut Daemon, p: &Parse, source: &str) -> Outcome {
    let name = p.text("name").to_string();
    match p.intent.as_str() {
        "view.create" => {
            let name = views::tidy(&name);
            if name.is_empty() {
                return Outcome::fail("¿Cómo se llama la vista?");
            }
            let here = active_workspace(d).unwrap_or(0);
            let candidates = d.candidates();
            if candidates.is_empty() {
                return Outcome::fail("No hay ninguna ventana abierta");
            }
            if p.text("from") == "level" {
                let choice = views::level(&candidates, here);
                return make_view(d, &name, &candidates, choice);
            }
            if !d.thinks() {
                let choice = views::guess(&name, &candidates, here);
                return make_view(d, &name, &candidates, choice);
            }
            d.think_view(&name, candidates, here, source);
            Outcome::done(format!("Preparo la vista «{name}»"))
        }
        "view.open" => Outcome::from(open_view(d, &name, p.flag("again") == Some(true))),
        "view.undo" => Outcome::from(undo_view(d, &name)),
        "view.save" => Outcome::from(save_view(d, &name)),
        "view.forget" => Outcome::from(view_named(d, &name).and_then(|at| {
            let gone = d.views.list.remove(at);
            d.views.save()?;
            d.views_changed();
            Ok(format!("Olvido la vista «{}»", gone.name))
        })),
        _ => Outcome::from(veyl(d, "launcher.open", json!(["views"])).map(|_| String::new())),
    }
}
