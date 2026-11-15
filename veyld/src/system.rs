//! Lo que pasa en el ordenador, medido sin preguntar a nadie: qué tira de la
//! CPU, qué ocupa la memoria y el disco, por qué va lenta la red, la gráfica,
//! la temperatura y la batería. Cada pregunta mide lo suyo (en /proc, /sys y
//! con df, du, ss y nvidia-smi), da un diagnóstico en una frase y lo enseña
//! en una tarjeta: cifras, una gráfica de lo que más gasta y botones.
//!
//! Medir tarda (la CPU y la red se miden durante un rato; el disco, con du),
//! así que esto corre en un hilo aparte, no en el bucle de veyld.

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::apps;
use crate::card;
use crate::daemon::Daemon;
use crate::orders::Outcome;
use crate::procs;

/// Una ventana abierta: de qué proceso es, cómo se llama la aplicación y su
/// clase (para «cierra …»).
#[derive(Clone, Debug)]
pub struct Seen {
    pub pid: i32,
    pub label: String,
}

/// Las ventanas abiertas, para llamar a cada programa por su nombre.
pub fn seen(d: &Daemon) -> Vec<Seen> {
    d.hypr.clients().into_iter().map(|w| Seen { pid: w.pid, label: apps::label(&d.apps, &w.class) }).collect()
}

/// Cómo se llama cada pregunta en la entrada, antes de hacerla.
pub fn title(intent: &str) -> (&'static str, &'static str) {
    match intent {
        "system.cpu" => ("Qué está usando la CPU", "cpu"),
        "system.memory" => ("Qué ocupa la memoria", "memory"),
        "system.disk" => ("Qué ocupa el disco", "hard-drives"),
        "system.net" => ("Cómo va la red", "wifi-high"),
        "system.gpu" => ("Qué usa la gráfica", "graphics-card"),
        "system.temp" => ("La temperatura", "thermometer"),
        "system.battery" => ("La batería", "battery-full"),
        _ => ("Cómo va el ordenador", "gauge"),
    }
}

/// Para el cerebro (la herramienta `sistema`): lo medido, en texto.
pub fn told(input: &Value, windows: &[Seen]) -> String {
    let intent = match input.get("que").and_then(Value::as_str).unwrap_or("todo") {
        "cpu" => "system.cpu",
        "memoria" => "system.memory",
        "disco" => "system.disk",
        "red" => "system.net",
        "grafica" => "system.gpu",
        "temperatura" => "system.temp",
        "bateria" => "system.battery",
        _ => "system.status",
    };
    let (outcome, card) = run(intent, windows);
    match card {
        Some(card) => format!("{}\nTarjeta (puedes pasarla a `mostrar`, retocada): {card}", outcome.say),
        None => outcome.say,
    }
}

/// Lo preguntado, medido: lo que se dice y la tarjeta que lo enseña.
pub fn run(intent: &str, windows: &[Seen]) -> (Outcome, Option<Value>) {
    let (outcome, card) = match intent {
        "system.cpu" => cpu(windows),
        "system.memory" => memory(windows),
        "system.disk" => disk(),
        "system.net" => net(windows),
        "system.gpu" => gpu(windows),
        "system.temp" => temp(),
        "system.battery" => battery(),
        _ => status(windows),
    };
    (outcome, card.and_then(|c| card::clean(&c)))
}

// --- Medidas -------------------------------------------------------------------

fn read(path: impl AsRef<Path>) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

/// Un campo de /proc/meminfo, en bytes.
fn meminfo() -> HashMap<String, u64> {
    read("/proc/meminfo")
        .lines()
        .filter_map(|l| {
            let (key, rest) = l.split_once(':')?;
            let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
            Some((key.to_string(), kb * 1024))
        })
        .collect()
}

/// La presión (PSI) de cpu, memory o io: % del tiempo que algo esperó, en los últimos 10 s.
fn pressure(what: &str) -> f64 {
    read(format!("/proc/pressure/{what}"))
        .lines()
        .find(|l| l.starts_with("some"))
        .and_then(|l| l.split_whitespace().find_map(|f| f.strip_prefix("avg10=")))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0)
}

fn cores() -> usize {
    thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
}

fn ticks_per_second() -> f64 {
    // SAFETY: sysconf no tiene efectos.
    let n = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if n > 0 { n as f64 } else { 100.0 }
}

/// Los tics de toda la máquina: (ocupados, total).
fn machine_ticks() -> (u64, u64) {
    let text = read("/proc/stat");
    let fields: Vec<u64> = text.lines().next().unwrap_or("").split_whitespace().skip(1).filter_map(|f| f.parse().ok()).collect();
    let total: u64 = fields.iter().take(8).sum();
    let idle = fields.get(3).copied().unwrap_or(0) + fields.get(4).copied().unwrap_or(0);
    (total.saturating_sub(idle), total)
}

/// El nombre con que se reconoce un proceso: el del ejecutable sin los
/// envoltorios de Nix (`.firefox-wrapped` → firefox) y, si es un intérprete,
/// el del programa que corre (`python server.py` → server).
pub fn name(pid: i32) -> String {
    let comm = read(format!("/proc/{pid}/comm")).trim().to_string();
    let exe = fs::read_link(format!("/proc/{pid}/exe")).ok().and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
    let mut name = exe.unwrap_or(comm);
    name = name.trim_start_matches('.').to_string();
    if let Some(at) = name.find("-wrap") {
        name.truncate(at);
    }
    let interpreter = ["python", "node", "perl", "ruby", "java", "bun", "deno"].iter().any(|i| name.starts_with(i));
    if interpreter
        && let Some(script) = procs::argv(pid).iter().skip(1).find(|a| !a.starts_with('-') && a.contains(['.', '/']))
        && let Some(stem) = Path::new(script).file_stem()
    {
        name = stem.to_string_lossy().into_owned();
    }
    if name.is_empty() { format!("pid {pid}") } else { name }
}

/// Lo que gasta cada programa (todos sus procesos juntos).
#[derive(Clone, Debug, Default)]
struct Use {
    name: String,
    pids: Vec<i32>,
    /// % de toda la máquina.
    cpu: f64,
    /// Bytes de memoria (PSS si se puede leer, si no, residente).
    memory: u64,
    /// Bytes por segundo leídos y escritos en disco.
    io: f64,
}

fn io_bytes(pid: i32) -> Option<u64> {
    let text = read(format!("/proc/{pid}/io"));
    let field = |key: &str| text.lines().find_map(|l| l.strip_prefix(key)).and_then(|v| v.trim().parse::<u64>().ok());
    Some(field("read_bytes:")? + field("write_bytes:")?)
}

fn proc_memory(pid: i32) -> u64 {
    let rollup = read(format!("/proc/{pid}/smaps_rollup"));
    if let Some(kb) = rollup.lines().find_map(|l| l.strip_prefix("Pss:")).and_then(|v| v.split_whitespace().next()?.parse::<u64>().ok()) {
        return kb * 1024;
    }
    let statm = read(format!("/proc/{pid}/statm"));
    // SAFETY: sysconf no tiene efectos.
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) }.max(4096) as u64;
    statm.split_whitespace().nth(1).and_then(|r| r.parse::<u64>().ok()).unwrap_or(0) * page
}

/// Los programas, medidos durante `window`: la CPU de cada uno, su disco y,
/// si se pide, su memoria. También el % de CPU de toda la máquina.
fn sample(window: Duration, with_memory: bool) -> (Vec<Use>, f64) {
    let before = procs::all();
    let io_before: HashMap<i32, u64> = before.keys().filter_map(|&p| Some((p, io_bytes(p)?))).collect();
    let machine_before = machine_ticks();
    let start = Instant::now();
    thread::sleep(window);
    let after = procs::all();
    let machine_after = machine_ticks();
    let seconds = start.elapsed().as_secs_f64().max(0.1);
    let busy = machine_after.0.saturating_sub(machine_before.0) as f64;
    let total = machine_after.1.saturating_sub(machine_before.1).max(1) as f64;
    let whole = cores() as f64 * ticks_per_second() * seconds;
    let mut groups: HashMap<String, Use> = HashMap::new();
    for (&pid, stat) in &after {
        // Los hilos del núcleo no tienen ejecutable ni línea de órdenes.
        if stat.parent == 2 || pid == 2 {
            continue;
        }
        let spent = before.get(&pid).filter(|b| b.start == stat.start).map_or(0, |b| stat.ticks.saturating_sub(b.ticks));
        let io = match (io_before.get(&pid), io_bytes(pid)) {
            (Some(a), Some(b)) => b.saturating_sub(*a) as f64 / seconds,
            _ => 0.0,
        };
        let name = name(pid);
        let group = groups.entry(name.clone()).or_insert_with(|| Use { name, ..Default::default() });
        group.pids.push(pid);
        group.cpu += spent as f64 / whole * 100.0;
        group.io += io;
        if with_memory {
            group.memory += proc_memory(pid);
        }
    }
    (groups.into_values().collect(), busy / total * 100.0)
}

/// Grados de la CPU, del disco y de la gráfica que se pueden leer.
fn temperatures() -> Vec<(String, f64)> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/class/hwmon") else {
        return out;
    };
    let mut cpu: Option<f64> = None;
    for entry in entries.flatten() {
        let dir = entry.path();
        let kind = read(dir.join("name")).trim().to_string();
        let first = |label: &str| -> Option<f64> {
            (1..12).find_map(|i| {
                let l = read(dir.join(format!("temp{i}_label")));
                (label.is_empty() || l.trim() == label).then(|| read(dir.join(format!("temp{i}_input"))).trim().parse::<f64>().ok()).flatten()
            })
        };
        match kind.as_str() {
            "coretemp" => cpu = cpu.or(first("Package id 0")),
            "k10temp" | "zenpower" => cpu = cpu.or(first("Tctl").or_else(|| first(""))),
            "nvme" => {
                if let Some(t) = first("Composite") {
                    out.push(("Disco".to_string(), t / 1000.0));
                }
            }
            "amdgpu" => {
                if let Some(t) = first("edge") {
                    out.push(("Gráfica".to_string(), t / 1000.0));
                }
            }
            _ => {}
        }
    }
    if let Some(t) = cpu {
        out.insert(0, ("CPU".to_string(), t / 1000.0));
    }
    if let Some(gpu) = nvidia() {
        out.push(("Gráfica".to_string(), gpu.temp));
    }
    out
}

fn cpu_temp(temps: &[(String, f64)]) -> Option<f64> {
    temps.iter().find(|(n, _)| n == "CPU").map(|(_, t)| *t)
}

/// El sistema de archivos de una carpeta: (total, libre para ti).
fn space(path: &str) -> Option<(u64, u64)> {
    let c = std::ffi::CString::new(path).ok()?;
    // SAFETY: statvfs rellena la estructura que se le da; la ruta acaba en 0.
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
        return None;
    }
    let block = s.f_frsize as u64;
    Some((s.f_blocks as u64 * block, s.f_bavail as u64 * block))
}

// --- Cómo se dice ---------------------------------------------------------------

fn decimal(n: f64, digits: usize) -> String {
    format!("{n:.digits$}").replace('.', ",")
}

/// «820 MB», «1,4 GB».
pub fn bytes(n: f64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = n.max(0.0);
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    let digits = if unit >= 3 && value < 100.0 { 1 } else { 0 };
    format!("{} {}", decimal(value, digits), UNITS[unit])
}

fn rate(n: f64) -> String {
    format!("{}/s", bytes(n))
}

fn percent(n: f64) -> String {
    format!("{} %", n.round() as i64)
}

fn degrees(n: f64) -> String {
    format!("{} °C", n.round() as i64)
}

/// El nombre que se enseña: el de la aplicación si tiene ventana.
fn shown(u: &Use, windows: &[Seen]) -> String {
    windowed(u, windows).unwrap_or_else(|| u.name.clone())
}

fn windowed(u: &Use, windows: &[Seen]) -> Option<String> {
    windows.iter().find(|w| u.pids.contains(&w.pid)).map(|w| w.label.clone())
}

/// Los que más gastan de algo, en una gráfica de barras.
fn chart(title: &str, unit: &str, rows: &[(String, f64)]) -> Value {
    json!({
        "tipo": "grafica",
        "forma": "barras",
        "texto": title,
        "unidad": unit,
        "items": rows.iter().take(6).map(|(name, n)| json!({ "titulo": name, "numero": n })).collect::<Vec<_>>()
    })
}

/// Bytes en la unidad que mejor se lee para el mayor: (factor, unidad).
fn scale(most: f64) -> (f64, &'static str) {
    if most >= 1024.0 * 1024.0 * 1024.0 {
        (1024.0 * 1024.0 * 1024.0, "GB")
    } else if most >= 1024.0 * 1024.0 {
        (1024.0 * 1024.0, "MB")
    } else {
        (1024.0, "KB")
    }
}

fn round1(n: f64) -> f64 {
    (n * 10.0).round() / 10.0
}

/// Botones para seguir mirando lo demás, sin el de lo que ya se ve.
fn more(here: &str, close: Option<&str>) -> Value {
    let mut buttons = Vec::new();
    if let Some(app) = close {
        buttons.push(json!({ "titulo": format!("Cerrar {app}"), "orden": format!("cierra {app}") }));
    }
    for (what, title, order) in [
        ("cpu", "CPU", "qué está usando la cpu"),
        ("memory", "Memoria", "qué ocupa más memoria"),
        ("disk", "Disco", "qué ocupa más en el disco"),
        ("net", "Red", "cómo va la red"),
    ] {
        if what != here && buttons.len() < 4 {
            buttons.push(json!({ "titulo": title, "orden": order }));
        }
    }
    json!({ "tipo": "acciones", "items": buttons })
}

fn card(title: &str, subtitle: &str, icon: &str, blocks: Vec<Value>) -> Value {
    json!({ "titulo": title, "subtitulo": subtitle, "icono": icon, "bloques": blocks })
}

// --- Las preguntas ---------------------------------------------------------------

/// Qué tira de la CPU.
fn cpu(windows: &[Seen]) -> (Outcome, Option<Value>) {
    let (mut used, total) = sample(Duration::from_millis(800), false);
    used.sort_by(|a, b| b.cpu.total_cmp(&a.cpu));
    let temps = temperatures();
    let load = read("/proc/loadavg").split_whitespace().next().unwrap_or("0").replace('.', ",");
    let top = used.first().filter(|u| u.cpu >= 3.0);
    let say = match top {
        Some(u) if total >= 15.0 => format!("{} usa el {} de la CPU", shown(u, windows), percent(u.cpu)),
        _ => format!("La CPU está tranquila: {}", percent(total)),
    };
    let rows: Vec<(String, f64)> = used.iter().filter(|u| u.cpu >= 0.05).map(|u| (shown(u, windows), round1(u.cpu).max(0.1))).collect();
    let mut metrics = vec![
        json!({ "titulo": "CPU", "valor": percent(total), "icono": "cpu", "color": if total > 85.0 { "aviso" } else { "" } }),
        json!({ "titulo": "Núcleos", "valor": cores().to_string(), "icono": "stack" }),
        json!({ "titulo": "Carga", "valor": load, "detalle": "último minuto", "icono": "gauge" }),
    ];
    if let Some(t) = cpu_temp(&temps) {
        metrics.push(json!({ "titulo": "Temperatura", "valor": degrees(t), "icono": "thermometer", "color": if t >= 90.0 { "mal" } else { "" } }));
    }
    let mut blocks = vec![json!({ "tipo": "metricas", "items": metrics })];
    if !rows.is_empty() {
        blocks.push(chart("% de toda la CPU, ahora", "%", &rows));
    }
    let close = top.filter(|u| u.cpu >= 20.0).and_then(|u| windowed(u, windows));
    blocks.push(more("cpu", close.as_deref()));
    (Outcome::done(say.clone()), Some(card(&say, "Lo que más tira de la CPU ahora", "cpu", blocks)))
}

/// Qué ocupa la memoria.
fn memory(windows: &[Seen]) -> (Outcome, Option<Value>) {
    let (mut used, _) = sample(Duration::from_millis(50), true);
    used.sort_by_key(|a| std::cmp::Reverse(a.memory));
    let info = meminfo();
    let get = |k: &str| info.get(k).copied().unwrap_or(0);
    let (total, available) = (get("MemTotal"), get("MemAvailable"));
    let in_use = total.saturating_sub(available);
    let swap = get("SwapTotal").saturating_sub(get("SwapFree"));
    let share = in_use as f64 / total.max(1) as f64;
    let squeeze = pressure("memory");
    let top = used.first();
    let say = match top {
        Some(u) if share > 0.85 || squeeze > 5.0 => format!("Falta memoria: {} ocupa {}", shown(u, windows), bytes(u.memory as f64)),
        Some(u) => format!("{} es lo que más ocupa: {}", shown(u, windows), bytes(u.memory as f64)),
        None => format!("En uso, {} de {}", bytes(in_use as f64), bytes(total as f64)),
    };
    let (factor, unit) = scale(top.map_or(0, |u| u.memory) as f64);
    let rows: Vec<(String, f64)> = used.iter().map(|u| (shown(u, windows), round1(u.memory as f64 / factor))).filter(|(_, n)| *n > 0.0).collect();
    let mut metrics = vec![
        json!({ "titulo": "En uso", "valor": bytes(in_use as f64), "detalle": format!("de {}", bytes(total as f64)), "icono": "memory" }),
        json!({ "titulo": "Libre", "valor": bytes(available as f64), "icono": "check-circle", "color": if share > 0.9 { "aviso" } else { "" } }),
    ];
    if get("SwapTotal") > 0 {
        metrics.push(json!({ "titulo": "Swap", "valor": bytes(swap as f64), "detalle": format!("de {}", bytes(get("SwapTotal") as f64)), "icono": "swap" }));
    }
    if squeeze >= 1.0 {
        metrics
            .push(json!({ "titulo": "Esperas", "valor": percent(squeeze), "detalle": "por falta de memoria", "icono": "hourglass-medium", "color": "aviso" }));
    }
    let mut blocks = vec![
        json!({ "tipo": "barra", "valor": share, "texto": format!("{} de la memoria en uso", percent(share * 100.0)) }),
        json!({ "tipo": "metricas", "items": metrics }),
    ];
    if !rows.is_empty() {
        blocks.push(chart("Memoria de cada programa", unit, &rows));
    }
    let close = top.filter(|u| u.memory as f64 > total as f64 * 0.2).and_then(|u| windowed(u, windows));
    blocks.push(more("memory", close.as_deref()));
    (Outcome::done(say.clone()), Some(card(&say, "Lo que más memoria ocupa", "memory", blocks)))
}

/// El tamaño de cada cosa de una carpeta, con du en paralelo; lo que no
/// acaba en `deadline` no se cuenta.
fn sizes(dir: &Path, deadline: Duration) -> Vec<(String, u64)> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let (tx, rx) = mpsc::channel();
    let mut children = Vec::new();
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(meta) = fs::symlink_metadata(&path) else { continue };
        if meta.is_file() {
            out.push((name, meta.len()));
            continue;
        }
        if !meta.is_dir() {
            continue;
        }
        let Ok(child) = Command::new("du").args(["-sxB1", "--"]).arg(&path).stdout(Stdio::piped()).stderr(Stdio::null()).spawn() else {
            continue;
        };
        children.push((name, child));
    }
    let watch: Vec<(String, std::process::Child)> = children;
    let ids: Vec<u32> = watch.iter().map(|(_, c)| c.id()).collect();
    for (name, mut child) in watch {
        let tx = tx.clone();
        thread::spawn(move || {
            let mut text = String::new();
            if let Some(mut out) = child.stdout.take() {
                let _ = out.read_to_string(&mut text);
            }
            let _ = child.wait();
            if let Some(n) = text.split_whitespace().next().and_then(|n| n.parse::<u64>().ok()) {
                let _ = tx.send((name, n));
            }
        });
    }
    drop(tx);
    let end = Instant::now() + deadline;
    while let Ok(got) = rx.recv_timeout(end.saturating_duration_since(Instant::now())) {
        out.push(got);
    }
    // Los que no han acabado, fuera.
    for id in ids {
        // SAFETY: matar un hijo propio por su pid; si ya acabó, no pasa nada.
        unsafe { libc::kill(id as i32, libc::SIGKILL) };
    }
    out.sort_by_key(|a| std::cmp::Reverse(a.1));
    out
}

/// Qué ocupa el disco.
fn disk() -> (Outcome, Option<Value>) {
    let home = std::env::var("HOME").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("/"));
    let home_text = home.to_string_lossy().into_owned();
    let (total, free) = space(&home_text).unwrap_or((0, 0));
    let used = total.saturating_sub(free);
    let share = used as f64 / total.max(1) as f64;
    let found = sizes(&home, Duration::from_secs(5));
    let top = found.first().cloned();
    let say = match &top {
        Some((name, n)) => format!("~/{name} es lo que más ocupa: {}; quedan {} libres", bytes(*n as f64), bytes(free as f64)),
        None => format!("Quedan {} libres de {}", bytes(free as f64), bytes(total as f64)),
    };
    let title = if share > 0.9 { format!("El disco está casi lleno: quedan {}", bytes(free as f64)) } else { say.clone() };
    let mut blocks = vec![
        json!({ "tipo": "barra", "valor": share, "texto": format!("{} del disco en uso", percent(share * 100.0)) }),
        json!({ "tipo": "metricas", "items": [
            { "titulo": "Libre", "valor": bytes(free as f64), "icono": "hard-drives", "color": if share > 0.9 { "aviso" } else { "" } },
            { "titulo": "En uso", "valor": bytes(used as f64), "detalle": format!("de {}", bytes(total as f64)), "icono": "database" }
        ]}),
    ];
    // Otros discos montados, con su espacio.
    let mut others: Vec<Value> = Vec::new();
    let mut seen = vec![space(&home_text)];
    for line in read("/proc/mounts").lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let (Some(source), Some(target)) = (fields.first(), fields.get(1)) else { continue };
        if !source.starts_with("/dev/") || target.starts_with("/boot") || target.starts_with("/nix") {
            continue;
        }
        let here = space(target);
        if seen.contains(&here) {
            continue;
        }
        seen.push(here);
        if let Some((t, f)) = here {
            others.push(json!({ "titulo": target, "valor": format!("{} libres", bytes(f as f64)), "detalle": format!("de {}", bytes(t as f64)) }));
        }
    }
    if !found.is_empty() {
        let (factor, unit) = scale(top.as_ref().map_or(0, |t| t.1) as f64);
        let rows: Vec<(String, f64)> = found.iter().map(|(n, b)| (format!("~/{n}"), round1(*b as f64 / factor))).collect();
        blocks.push(chart("Lo que más ocupa en tu carpeta", unit, &rows));
    }
    // Lo que se puede vaciar sin perder nada.
    let cache = found.iter().find(|(n, _)| n == ".cache").map(|(_, b)| *b).unwrap_or(0);
    if cache > 2 * 1024 * 1024 * 1024 {
        blocks.push(json!({ "tipo": "texto", "texto": format!("~/.cache ocupa {}: son copias que los programas pueden volver a bajar, se puede vaciar.", bytes(cache as f64)) }));
    }
    if Path::new("/nix/store").exists() && share > 0.8 {
        blocks.push(
            json!({ "tipo": "texto", "texto": "En NixOS, las generaciones viejas y lo que nadie usa de /nix/store se borran con «nix-collect-garbage -d»." }),
        );
    }
    if !others.is_empty() {
        blocks.push(json!({ "tipo": "lista", "items": others }));
    }
    blocks.push(more("disk", None));
    (Outcome::done(say), Some(card(&title, &format!("Tu disco, en {home_text}"), "hard-drives", blocks)))
}

/// La interfaz por la que sale internet y la puerta de enlace.
fn route() -> Option<(String, Option<[u8; 4]>)> {
    let text = read("/proc/net/route");
    let mut best: Option<(String, Option<[u8; 4]>, u32)> = None;
    for line in text.lines().skip(1) {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 7 || f[1] != "00000000" {
            continue;
        }
        let gateway = u32::from_str_radix(f[2], 16).ok().map(|g| g.to_le_bytes());
        let metric: u32 = f[6].parse().unwrap_or(0);
        if best.as_ref().is_none_or(|b| metric < b.2) {
            best = Some((f[0].to_string(), gateway.filter(|g| *g != [0, 0, 0, 0]), metric));
        }
    }
    best.map(|(i, g, _)| (i, g))
}

fn link_bytes(iface: &str) -> (u64, u64) {
    let text = read("/proc/net/dev");
    let line = text.lines().find(|l| l.trim_start().starts_with(&format!("{iface}:"))).unwrap_or("");
    let f: Vec<u64> = line.split_once(':').map_or("", |s| s.1).split_whitespace().filter_map(|x| x.parse().ok()).collect();
    (f.first().copied().unwrap_or(0), f.get(8).copied().unwrap_or(0))
}

/// Lo que tarda en contestar una dirección por TCP, en ms (un «rechazado»
/// también es contestar). La mejor de tres.
fn reach(address: SocketAddr) -> Option<f64> {
    (0..3)
        .filter_map(|_| {
            let start = Instant::now();
            match TcpStream::connect_timeout(&address, Duration::from_millis(1500)) {
                Ok(_) => Some(start.elapsed()),
                Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused => Some(start.elapsed()),
                Err(_) => None,
            }
        })
        .min()
        .map(|d| d.as_secs_f64() * 1000.0)
}

/// Lo que tarda el DNS en contestar un nombre que no está en ninguna caché,
/// y si contesta.
fn dns() -> (Option<f64>, bool) {
    let fresh =
        format!("veyl{}.example.com:443", Instant::now().elapsed().as_nanos() ^ std::process::id() as u128 ^ chrono::Utc::now().timestamp_micros() as u128);
    let start = Instant::now();
    let fresh_ok = fresh.to_socket_addrs().is_ok();
    let took = start.elapsed().as_secs_f64() * 1000.0;
    let known = "example.com:443".to_socket_addrs().is_ok();
    // Un nombre que no existe tarda lo mismo que uno que sí: cuenta el tiempo si contestó algo.
    ((known || fresh_ok || took < 3000.0).then_some(took), known)
}

/// Bytes que ha movido cada conexión TCP abierta, con su proceso.
fn sockets() -> HashMap<String, (i32, u64)> {
    let Ok(out) = Command::new("ss").args(["-tinpH", "state", "established"]).stderr(Stdio::null()).output() else {
        return HashMap::new();
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut result = HashMap::new();
    let mut current: Option<(String, i32)> = None;
    for line in text.lines() {
        if !line.starts_with(char::is_whitespace) {
            let f: Vec<&str> = line.split_whitespace().collect();
            let pid = line.split("pid=").nth(1).and_then(|p| p.split(|c: char| !c.is_ascii_digit()).next()).and_then(|p| p.parse().ok());
            current = match (f.get(2), f.get(3), pid) {
                (Some(local), Some(remote), Some(pid)) => Some((format!("{local} {remote}"), pid)),
                _ => None,
            };
            continue;
        }
        let Some((key, pid)) = current.take() else { continue };
        let field = |k: &str| line.split_whitespace().find_map(|f| f.strip_prefix(k)).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
        result.insert(key, (pid, field("bytes_received:") + field("bytes_acked:")));
    }
    result
}

/// Por qué va lenta la red.
fn net(windows: &[Seen]) -> (Outcome, Option<Value>) {
    let Some((iface, gateway)) = route() else {
        let say = "No hay red: no estás conectado a nada".to_string();
        return (
            Outcome::done(say.clone()),
            Some(card(&say, "", "wifi-x", vec![json!({ "tipo": "acciones", "items": [{ "titulo": "Encender el wifi", "orden": "enciende el wifi" }] })])),
        );
    };
    let wifi = Path::new(&format!("/sys/class/net/{iface}/wireless")).exists();
    // Mientras se mide el tráfico, se miden también los tiempos.
    let router = gateway.map(|g| thread::spawn(move || reach(SocketAddr::from((g, 80)))));
    let internet = thread::spawn(|| reach(SocketAddr::from(([1, 1, 1, 1], 443))).or_else(|| reach(SocketAddr::from(([8, 8, 8, 8], 443)))));
    let names = thread::spawn(dns);
    let sockets_before = sockets();
    let link_before = link_bytes(&iface);
    let start = Instant::now();
    thread::sleep(Duration::from_millis(1200));
    let sockets_after = sockets();
    let link_after = link_bytes(&iface);
    let seconds = start.elapsed().as_secs_f64();
    let down = link_after.0.saturating_sub(link_before.0) as f64 / seconds;
    let up = link_after.1.saturating_sub(link_before.1) as f64 / seconds;
    let router = router.and_then(|t| t.join().ok()).flatten();
    let internet = internet.join().ok().flatten();
    let (dns_ms, dns_ok) = names.join().unwrap_or((None, false));

    // Quién mueve más, por programa.
    let mut by_pid: HashMap<i32, f64> = HashMap::new();
    for (key, (pid, after)) in &sockets_after {
        let before = sockets_before.get(key).map_or(0, |b| b.1);
        // Una conexión nueva cuenta desde cero solo si es pequeña: si no, es lo de antes de abrirla.
        let moved = if sockets_before.contains_key(key) { after.saturating_sub(before) } else { 0 };
        *by_pid.entry(*pid).or_default() += moved as f64 / seconds;
    }
    let mut groups: HashMap<String, Use> = HashMap::new();
    for (pid, moved) in by_pid {
        let name = name(pid);
        let g = groups.entry(name.clone()).or_insert_with(|| Use { name, ..Default::default() });
        g.pids.push(pid);
        g.io += moved;
    }
    let mut users: Vec<Use> = groups.into_values().filter(|u| u.io > 1024.0).collect();
    users.sort_by(|a, b| b.io.total_cmp(&a.io));

    let signal = wifi.then(|| {
        read("/proc/net/wireless")
            .lines()
            .find(|l| l.trim_start().starts_with(&format!("{iface}:")))
            .and_then(|l| l.split_whitespace().nth(3).and_then(|v| v.trim_end_matches('.').parse::<f64>().ok()))
    });
    let signal = signal.flatten();
    let speed: Option<u64> = read(format!("/sys/class/net/{iface}/speed")).trim().parse().ok().filter(|s| *s > 0);
    let heavy = users.first().filter(|u| u.io > 500.0 * 1024.0);
    let say = if gateway.is_some() && router.is_none() && internet.is_none() {
        "No llega al router: revisa el cable o el wifi".to_string()
    } else if internet.is_none() {
        "El router no da salida a internet".to_string()
    } else if !dns_ok {
        "Internet va, pero falla el DNS: las webs no cargan".to_string()
    } else if let Some(s) = signal.filter(|s| *s < -72.0) {
        format!("La señal del wifi es débil ({} dBm)", s.round() as i64)
    } else if let Some(ms) = router.filter(|ms| *ms > 30.0) {
        format!("La red de casa va con retraso: {} ms al router", ms.round() as i64)
    } else if let Some(u) = heavy {
        format!("{} está gastando la red: {}", shown(u, windows), rate(u.io))
    } else if let Some(ms) = internet.filter(|ms| *ms > 120.0) {
        format!("Internet va con retraso: {} ms", ms.round() as i64)
    } else if let Some(ms) = dns_ms.filter(|ms| *ms > 400.0) {
        format!("El DNS tarda en contestar: {} ms", ms.round() as i64)
    } else {
        format!("La red va bien: {} ms a internet", internet.unwrap_or(0.0).round() as i64)
    };
    let ms = |m: Option<f64>| m.map_or("no contesta".to_string(), |m| if m < 1.0 { "< 1 ms".to_string() } else { format!("{} ms", m.round() as i64) });
    let slow = |m: Option<f64>, limit: f64| match m {
        None => "mal",
        Some(m) if m > limit => "aviso",
        _ => "",
    };
    let link = match (wifi, signal, speed) {
        (true, Some(s), _) => {
            json!({ "titulo": "Wifi", "valor": format!("{} dBm", s.round() as i64), "detalle": if s < -72.0 { "débil" } else if s < -60.0 { "regular" } else { "buena" }, "icono": "wifi-high", "color": if s < -72.0 { "aviso" } else { "" } })
        }
        (true, None, _) => json!({ "titulo": "Wifi", "valor": iface, "icono": "wifi-high" }),
        (false, _, Some(s)) => {
            json!({ "titulo": "Cable", "valor": if s >= 1000 { format!("{} Gb/s", decimal(s as f64 / 1000.0, if s % 1000 == 0 { 0 } else { 1 })) } else { format!("{s} Mb/s") }, "icono": "network" })
        }
        (false, _, None) => json!({ "titulo": "Conexión", "valor": iface, "icono": "network" }),
    };
    let mut metrics = vec![link];
    if gateway.is_some() {
        metrics.push(json!({ "titulo": "Router", "valor": ms(router), "icono": "house", "color": slow(router, 30.0) }));
    }
    metrics.push(json!({ "titulo": "Internet", "valor": ms(internet), "icono": "globe", "color": slow(internet, 120.0) }));
    metrics.push(json!({ "titulo": "DNS", "valor": if dns_ok { ms(dns_ms) } else { "falla".to_string() }, "icono": "address-book", "color": if dns_ok { slow(dns_ms, 400.0) } else { "mal" } }));
    metrics.push(json!({ "titulo": "Bajada", "valor": rate(down), "icono": "arrow-down" }));
    metrics.push(json!({ "titulo": "Subida", "valor": rate(up), "icono": "arrow-up" }));
    let mut blocks = vec![json!({ "tipo": "metricas", "items": metrics })];
    if !users.is_empty() {
        let (factor, unit) = scale(users[0].io);
        let rows: Vec<(String, f64)> = users.iter().map(|u| (shown(u, windows), round1(u.io / factor))).collect();
        blocks.push(chart("Lo que mueve cada programa por la red", &format!("{unit}/s"), &rows));
    }
    let close = heavy.and_then(|u| windowed(u, windows));
    blocks.push(more("net", close.as_deref()));
    (Outcome::done(say.clone()), Some(card(&say, "Router, internet y DNS, medidos ahora", if wifi { "wifi-high" } else { "network" }, blocks)))
}

#[derive(Clone, Debug, Default)]
struct Gpu {
    name: String,
    busy: f64,
    used: f64,
    total: f64,
    temp: f64,
    /// (pid, nombre, MB)
    apps: Vec<(i32, String, f64)>,
}

fn nvidia() -> Option<Gpu> {
    let query = ["--query-gpu=name,utilization.gpu,memory.used,memory.total,temperature.gpu", "--format=csv,noheader,nounits"];
    let out = Command::new("nvidia-smi").args(query).stderr(Stdio::null()).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let f: Vec<&str> = text.lines().next()?.split(',').map(str::trim).collect();
    let n = |i: usize| f.get(i).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
    Some(Gpu { name: f.first()?.to_string(), busy: n(1), used: n(2), total: n(3), temp: n(4), apps: Vec::new() })
}

/// Los programas que usan la gráfica de NVIDIA y su memoria, de la tabla de nvidia-smi.
fn nvidia_apps() -> Vec<(i32, String, f64)> {
    let Ok(out) = Command::new("nvidia-smi").stderr(Stdio::null()).output() else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut apps = Vec::new();
    for line in text.lines().skip_while(|l| !l.contains("Processes")) {
        let f: Vec<&str> = line.trim_matches(|c| c == '|' || c == ' ').split_whitespace().collect();
        let (Some(pid), Some(memory)) =
            (f.get(3).and_then(|p| p.parse::<i32>().ok()), f.last().and_then(|m| m.strip_suffix("MiB")).and_then(|m| m.parse::<f64>().ok()))
        else {
            continue;
        };
        apps.push((pid, name(pid), memory));
    }
    apps
}

/// Qué usa la gráfica.
fn gpu(windows: &[Seen]) -> (Outcome, Option<Value>) {
    let Some(mut g) = nvidia() else {
        return (Outcome::fail("No sé leer esta gráfica (de momento, solo las de NVIDIA)"), None);
    };
    g.apps = nvidia_apps();
    let mut groups: HashMap<String, Use> = HashMap::new();
    for (pid, name, mb) in &g.apps {
        let u = groups.entry(name.clone()).or_insert_with(|| Use { name: name.clone(), ..Default::default() });
        u.pids.push(*pid);
        u.memory += (*mb * 1024.0 * 1024.0) as u64;
    }
    let mut users: Vec<Use> = groups.into_values().collect();
    users.sort_by_key(|a| std::cmp::Reverse(a.memory));
    let top = users.first();
    let say = match top {
        Some(u) => format!("La gráfica va al {}; {} usa {} de su memoria", percent(g.busy), shown(u, windows), bytes(u.memory as f64)),
        None => format!("La gráfica va al {}", percent(g.busy)),
    };
    let blocks = vec![
        json!({ "tipo": "metricas", "items": [
            { "titulo": "Uso", "valor": percent(g.busy), "icono": "graphics-card" },
            { "titulo": "Memoria", "valor": bytes(g.used * 1024.0 * 1024.0), "detalle": format!("de {}", bytes(g.total * 1024.0 * 1024.0)), "icono": "memory" },
            { "titulo": "Temperatura", "valor": degrees(g.temp), "icono": "thermometer", "color": if g.temp >= 85.0 { "aviso" } else { "" } }
        ]}),
        json!({ "tipo": "barra", "valor": g.used / g.total.max(1.0), "texto": "Memoria de la gráfica en uso" }),
        chart(
            "Memoria de la gráfica de cada programa",
            "MB",
            &users.iter().map(|u| (shown(u, windows), (u.memory as f64 / 1024.0 / 1024.0).round())).collect::<Vec<_>>(),
        ),
        more("gpu", None),
    ];
    (Outcome::done(say.clone()), Some(card(&say, &g.name, "graphics-card", blocks)))
}

/// Lo caliente que está.
fn temp() -> (Outcome, Option<Value>) {
    let temps = temperatures();
    let Some((name, hottest)) = temps.iter().max_by(|a, b| a.1.total_cmp(&b.1)).cloned() else {
        return (Outcome::fail("No encuentro ningún sensor de temperatura"), None);
    };
    let say = match cpu_temp(&temps) {
        Some(t) if t >= 90.0 => format!("La CPU está muy caliente: {}", degrees(t)),
        Some(t) if t >= 80.0 => format!("La CPU está caliente: {}", degrees(t)),
        Some(t) => format!("Todo fresco: la CPU a {}", degrees(t)),
        None => format!("{name}, a {}", degrees(hottest)),
    };
    let items: Vec<Value> = temps
        .iter()
        .map(|(n, t)| json!({ "titulo": n, "valor": degrees(*t), "icono": "thermometer", "color": if *t >= 90.0 { "mal" } else if *t >= 80.0 { "aviso" } else { "" } }))
        .collect();
    let mut blocks = vec![json!({ "tipo": if items.len() >= 2 { "metricas" } else { "datos" }, "items": items })];
    if cpu_temp(&temps).is_some_and(|t| t >= 85.0) {
        blocks.push(json!({ "tipo": "texto", "texto": "Tan caliente, la CPU baja la velocidad para enfriarse y todo va más lento." }));
        blocks.push(json!({ "tipo": "acciones", "items": [{ "titulo": "Qué la calienta", "orden": "qué está usando la cpu" }] }));
    }
    (Outcome::done(say.clone()), Some(card(&say, "", "thermometer", blocks)))
}

/// Cómo va la batería.
fn battery() -> (Outcome, Option<Value>) {
    let bat = fs::read_dir("/sys/class/power_supply").ok().into_iter().flatten().flatten().map(|e| e.path()).find(|p| read(p.join("type")).trim() == "Battery");
    let Some(bat) = bat else {
        return (Outcome::done("Este ordenador no tiene batería"), None);
    };
    let n = |f: &str| read(bat.join(f)).trim().parse::<f64>().ok();
    let level = n("capacity").unwrap_or(0.0);
    let status = read(bat.join("status")).trim().to_string();
    let charging = status == "Charging";
    let now = n("energy_now").or_else(|| n("charge_now"));
    let full = n("energy_full").or_else(|| n("charge_full"));
    let design = n("energy_full_design").or_else(|| n("charge_full_design"));
    let power = n("power_now").or_else(|| Some(n("current_now")? * n("voltage_now")? / 1e6)).filter(|p| *p > 0.0);
    let hours = match (now, full, power) {
        (Some(now), _, Some(p)) if !charging && n("energy_now").is_some() => Some(now / p),
        (Some(now), Some(full), Some(p)) if charging && n("energy_now").is_some() => Some((full - now) / p),
        _ => None,
    };
    let left = hours.map(|h| {
        let minutes = (h * 60.0).round() as i64;
        if minutes >= 60 { format!("{} h {} min", minutes / 60, minutes % 60) } else { format!("{minutes} min") }
    });
    let say = match (charging, &left) {
        (true, Some(l)) => format!("Cargando, al {}: llena en {l}", percent(level)),
        (true, None) => format!("Cargando, al {}", percent(level)),
        (false, Some(l)) if status == "Discharging" => format!("Al {}: te queda {l}", percent(level)),
        _ => format!("Batería al {}", percent(level)),
    };
    let mut items = vec![
        json!({ "titulo": "Estado", "valor": match status.as_str() { "Charging" => "cargando", "Discharging" => "gastándose", "Full" => "llena", _ => "enchufada" } }),
    ];
    if let Some(p) = power {
        items.push(json!({ "titulo": "Consumo", "valor": format!("{} W", decimal(p / 1e6, 1)) }));
    }
    if let (Some(full), Some(design)) = (full, design) {
        items.push(json!({ "titulo": "Salud", "valor": percent(full / design.max(1.0) * 100.0) }));
    }
    let blocks =
        vec![json!({ "tipo": "barra", "valor": level / 100.0, "texto": format!("{} de batería", percent(level)) }), json!({ "tipo": "datos", "items": items })];
    (Outcome::done(say.clone()), Some(card(&say, "", if charging { "battery-charging" } else { "battery-full" }, blocks)))
}

/// Por qué va lento el ordenador: lo que más espera (CPU, memoria, disco), lo
/// caliente que está y quién tiene la culpa.
fn status(windows: &[Seen]) -> (Outcome, Option<Value>) {
    let (mut used, total) = sample(Duration::from_millis(800), true);
    let info = meminfo();
    let get = |k: &str| info.get(k).copied().unwrap_or(0);
    let memory_share = get("MemTotal").saturating_sub(get("MemAvailable")) as f64 / get("MemTotal").max(1) as f64;
    let (cpu_wait, memory_wait, io_wait) = (pressure("cpu"), pressure("memory"), pressure("io"));
    let temps = temperatures();
    let hot = cpu_temp(&temps);
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
    let disk_share = space(&home).map_or(0.0, |(t, f)| 1.0 - f as f64 / t.max(1) as f64);

    let by_cpu = |used: &mut Vec<Use>| used.sort_by(|a, b| b.cpu.total_cmp(&a.cpu));
    // Lo que más pesa decide qué se enseña.
    let (say, focus) = if memory_wait > 10.0 || memory_share > 0.92 {
        used.sort_by_key(|a| std::cmp::Reverse(a.memory));
        let u = &used[0];
        (format!("Falta memoria: {} ocupa {}", shown(u, windows), bytes(u.memory as f64)), "memory")
    } else if total > 80.0 || cpu_wait > 30.0 {
        by_cpu(&mut used);
        let u = &used[0];
        (format!("La CPU va al límite: {} usa el {}", shown(u, windows), percent(u.cpu)), "cpu")
    } else if io_wait > 25.0 {
        used.sort_by(|a, b| b.io.total_cmp(&a.io));
        let culprit = used.first().filter(|u| u.io > 1024.0 * 1024.0);
        let say = match culprit {
            Some(u) => format!("El disco no da abasto: {} mueve {}", shown(u, windows), rate(u.io)),
            None => "El disco no da abasto".to_string(),
        };
        (say, "io")
    } else if hot.is_some_and(|t| t >= 90.0) {
        by_cpu(&mut used);
        (format!("Está muy caliente: la CPU a {} va más despacio", degrees(hot.unwrap_or(0.0))), "cpu")
    } else if disk_share > 0.95 {
        by_cpu(&mut used);
        ("El disco está casi lleno".to_string(), "disk")
    } else {
        by_cpu(&mut used);
        let say = match used.first().filter(|u| u.cpu >= 15.0) {
            Some(u) => format!("Va bien; lo que más tira es {} ({})", shown(u, windows), percent(u.cpu)),
            None => "Todo va bien: nada está tirando de más".to_string(),
        };
        (say, "cpu")
    };
    let warn = |bad: bool| if bad { "aviso" } else { "" };
    let mut metrics = vec![
        json!({ "titulo": "CPU", "valor": percent(total), "icono": "cpu", "color": warn(total > 80.0) }),
        json!({ "titulo": "Memoria", "valor": percent(memory_share * 100.0), "icono": "memory", "color": warn(memory_share > 0.9) }),
        json!({ "titulo": "Disco", "valor": percent(disk_share * 100.0), "detalle": "lleno", "icono": "hard-drives", "color": warn(disk_share > 0.9) }),
    ];
    if let Some(t) = hot {
        metrics.push(json!({ "titulo": "Temperatura", "valor": degrees(t), "icono": "thermometer", "color": if t >= 90.0 { "mal" } else { warn(t >= 80.0) } }));
    }
    let waits: Vec<(&str, f64)> = [("CPU", cpu_wait), ("memoria", memory_wait), ("disco", io_wait)].into_iter().filter(|(_, w)| *w >= 5.0).collect();
    let mut blocks = vec![json!({ "tipo": "metricas", "items": metrics })];
    if !waits.is_empty() {
        let text = waits.iter().map(|(n, w)| format!("{} del tiempo esperando a {n}", percent(*w))).collect::<Vec<_>>().join(", ");
        blocks.push(json!({ "tipo": "texto", "texto": format!("En los últimos 10 s, los programas pasaron un {text}.") }));
    }
    let rows: Vec<(String, f64)> = match focus {
        "memory" => {
            let (factor, _) = scale(used.first().map_or(0, |u| u.memory) as f64);
            used.iter().map(|u| (shown(u, windows), round1(u.memory as f64 / factor))).collect()
        }
        "io" => {
            let (factor, _) = scale(used.first().map_or(0.0, |u| u.io));
            used.iter().filter(|u| u.io > 0.0).map(|u| (shown(u, windows), round1(u.io / factor))).collect()
        }
        _ => used.iter().filter(|u| u.cpu >= 0.05).map(|u| (shown(u, windows), round1(u.cpu).max(0.1))).collect(),
    };
    let unit = match focus {
        "memory" => scale(used.first().map_or(0, |u| u.memory) as f64).1.to_string(),
        "io" => format!("{}/s", scale(used.first().map_or(0.0, |u| u.io)).1),
        _ => "%".to_string(),
    };
    let what = match focus {
        "memory" => "Memoria de cada programa",
        "io" => "Lo que lee y escribe cada programa",
        _ => "% de toda la CPU, ahora",
    };
    if !rows.is_empty() {
        blocks.push(chart(what, &unit, &rows));
    }
    let culprit = used.first().filter(|u| match focus {
        "memory" => u.memory as f64 > get("MemTotal") as f64 * 0.2,
        "io" => u.io > 10.0 * 1024.0 * 1024.0,
        _ => u.cpu >= 20.0,
    });
    let close = culprit.and_then(|u| windowed(u, windows));
    blocks.push(more(if focus == "io" { "disk" } else { focus }, close.as_deref()));
    (Outcome::done(say.clone()), Some(card(&say, "Cómo va el ordenador ahora", "gauge", blocks)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_well() {
        assert_eq!(bytes(512.0), "512 B");
        assert_eq!(bytes(1536.0 * 1024.0 * 1024.0), "1,5 GB");
        assert_eq!(bytes(820.0 * 1024.0 * 1024.0), "820 MB");
        assert_eq!(rate(2.0 * 1024.0 * 1024.0), "2 MB/s");
        assert_eq!(scale(3e9), (1024.0 * 1024.0 * 1024.0, "GB"));
    }

    #[test]
    fn every_question_draws_a_card() {
        // Lo que se puede medir en cualquier máquina: no falla y deja algo que enseñar.
        for intent in ["system.status", "system.cpu", "system.memory"] {
            let (outcome, card) = run(intent, &[]);
            assert!(outcome.ok, "{intent}: {}", outcome.say);
            let card = card.unwrap_or_else(|| panic!("{intent} sin tarjeta"));
            assert!(card["bloques"].as_array().is_some_and(|b| b.len() >= 2), "{intent}: {card}");
        }
        assert!(!name(std::process::id() as i32).is_empty());
    }
}
