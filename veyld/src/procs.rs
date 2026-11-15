//! Los procesos, por /proc.

use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::MetadataExt;

/// Lo que dice /proc/<pid>/stat de un proceso.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stat {
    pub parent: i32,
    pub group: i32,
    /// La terminal que lo controla (0 si ninguna) y el grupo que la tiene en
    /// primer plano.
    pub tty: u32,
    pub foreground: i32,
    /// Tics de CPU gastados (usuario + sistema).
    pub ticks: u64,
    /// Cuándo arrancó, en tics desde que arrancó la máquina.
    pub start: u64,
}

pub fn stat(pid: i32) -> Option<Stat> {
    let text = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // El nombre va entre paréntesis y puede llevar espacios: se cuenta desde el último.
    let fields: Vec<&str> = text.rsplit_once(')')?.1.split_whitespace().collect();
    let field = |i: usize| fields.get(i).and_then(|f| f.parse::<i64>().ok());
    Some(Stat {
        parent: field(1)? as i32,
        group: field(2)? as i32,
        tty: field(4)? as u32,
        foreground: field(5)? as i32,
        ticks: (field(11)? + field(12)?) as u64,
        start: field(19)? as u64,
    })
}

/// Todos los procesos que se dejan leer.
pub fn all() -> HashMap<i32, Stat> {
    let mut out = HashMap::new();
    let Ok(entries) = fs::read_dir("/proc") else {
        return out;
    };
    for entry in entries.flatten() {
        if let Some(pid) = entry.file_name().to_str().and_then(|n| n.parse::<i32>().ok())
            && let Some(stat) = stat(pid)
        {
            out.insert(pid, stat);
        }
    }
    out
}

/// De cada proceso, sus hijos.
pub fn children(procs: &HashMap<i32, Stat>) -> HashMap<i32, Vec<i32>> {
    let mut out: HashMap<i32, Vec<i32>> = HashMap::new();
    for (&pid, stat) in procs {
        out.entry(stat.parent).or_default().push(pid);
    }
    out
}

/// Un proceso y todos los que cuelgan de él.
pub fn tree(pid: i32, children: &HashMap<i32, Vec<i32>>) -> Vec<i32> {
    let mut out = Vec::new();
    let mut stack = vec![pid];
    while let Some(p) = stack.pop() {
        out.push(p);
        if let Some(kids) = children.get(&p) {
            stack.extend(kids);
        }
    }
    out
}

pub fn argv(pid: i32) -> Vec<String> {
    let Ok(bytes) = fs::read(format!("/proc/{pid}/cmdline")) else {
        return Vec::new();
    };
    bytes.split(|&b| b == 0).filter(|a| !a.is_empty()).map(|a| String::from_utf8_lossy(a).into_owned()).collect()
}

pub fn comm(pid: i32) -> String {
    fs::read_to_string(format!("/proc/{pid}/comm")).map(|s| s.trim().to_string()).unwrap_or_default()
}

pub fn cwd(pid: i32) -> String {
    fs::read_link(format!("/proc/{pid}/cwd")).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()
}

pub fn environ(pid: i32) -> HashMap<String, String> {
    let Ok(bytes) = fs::read(format!("/proc/{pid}/environ")) else {
        return HashMap::new();
    };
    bytes
        .split(|&b| b == 0)
        .filter_map(|item| {
            let item = String::from_utf8_lossy(item);
            let (k, v) = item.split_once('=')?;
            Some((k.to_string(), v.to_string()))
        })
        .collect()
}

/// Cuándo se leyó por última vez de una terminal (s), para saber en cuál se
/// escribe: la de una pestaña en la que nadie teclea no cambia.
fn touched(tty: u32) -> i64 {
    // El mayor va en los bits 15-8; el menor, en 31-20 y 7-0.
    let (major, minor) = ((tty >> 8) & 0xff, (tty & 0xff) | ((tty >> 12) & 0xfff00));
    let path = if (136..=143).contains(&major) {
        format!("/dev/pts/{}", (major - 136) * 256 + minor)
    } else {
        return 0;
    };
    fs::metadata(path).map(|m| m.atime()).unwrap_or(0)
}

/// Dónde y con qué se trabaja en una ventana.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Place {
    pub folder: String,
    pub command: String,
}

/// En una terminal, la carpeta y la orden de lo que tiene en primer plano la
/// pestaña en la que se teclea. Fuera de una terminal, nada: la carpeta de un
/// programa gráfico es la de quien lo lanzó.
pub fn place(pid: i32, procs: &HashMap<i32, Stat>, children: &HashMap<i32, Vec<i32>>) -> Place {
    let mut best: Option<(i64, u64, i32)> = None;
    for p in tree(pid, children) {
        let Some(stat) = procs.get(&p) else {
            continue;
        };
        if p == pid || stat.tty == 0 || stat.group != stat.foreground {
            continue;
        }
        let key = (touched(stat.tty), stat.start, p);
        if best.is_none_or(|b| key > b) {
            best = Some(key);
        }
    }
    match best {
        Some((_, _, p)) => Place { folder: cwd(p), command: command(&argv(p)) },
        None => Place::default(),
    }
}

const SHELLS: &[&str] = &["fish", "bash", "zsh", "sh", "dash", "nu", "elvish", "xonsh", "tcsh", "ksh", "ion", "oil", "osh"];
const RUNNERS: &[&str] = &["node", "python", "bun", "deno", "ruby", "perl", "java"];

/// El nombre de un programa: «/nix/store/…/bin/.claude-wrapped» → «claude».
fn program(arg: &str) -> String {
    let base = arg.rsplit('/').next().unwrap_or(arg).trim_start_matches('-');
    let base = base.strip_prefix('.').unwrap_or(base);
    base.strip_suffix("-wrapped").unwrap_or(base).to_string()
}

/// Lo que se está haciendo, sin lo que pueda ser secreto: el programa y, como
/// mucho, dos palabras más («cargo test», «nvim journal.rs»), hasta la primera
/// opción o lo que no parezca un nombre (una clave, una dirección, «a=b»).
/// La shell esperando a que escribas es nada.
pub fn command(argv: &[String]) -> String {
    let Some(first) = argv.first() else {
        return String::new();
    };
    let mut name = program(first);
    let mut rest = &argv[1..];
    let bare = name.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
    if RUNNERS.contains(&bare)
        && let Some(script) = rest.first().filter(|a| !a.starts_with('-'))
    {
        name = program(script);
        for ext in [".js", ".mjs", ".py", ".rb", ".pl", ".jar"] {
            name = name.strip_suffix(ext).map(str::to_string).unwrap_or(name);
        }
        rest = &rest[1..];
    }
    let words: Vec<&str> = rest
        .iter()
        .take_while(|a| {
            let n = a.chars().count();
            !a.starts_with('-') && n <= 40 && a.chars().all(|c| c.is_alphanumeric() || "._/~+-".contains(c)) && (n < 16 || a.contains('.') || a.contains('/'))
        })
        .take(2)
        .map(String::as_str)
        .collect();
    if SHELLS.contains(&name.as_str()) && words.is_empty() {
        return String::new();
    }
    std::iter::once(name.as_str()).chain(words).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn says(line: &str) -> String {
        command(&line.split_whitespace().map(String::from).collect::<Vec<_>>())
    }

    #[test]
    fn commands() {
        assert_eq!(says("/run/current-system/sw/bin/fish"), "");
        assert_eq!(says("-fish"), "");
        assert_eq!(says("bash build.sh"), "bash build.sh");
        assert_eq!(says("nvim src/journal.rs"), "nvim src/journal.rs");
        assert_eq!(says("cargo test --release"), "cargo test");
        assert_eq!(says("git commit -m secreto"), "git commit");
        assert_eq!(says("ssh root@servidor"), "ssh");
        assert_eq!(says("mysql -pclave"), "mysql");
        assert_eq!(says("curl https://x.com/?token=1"), "curl");
        assert_eq!(says("env API_KEY=abc programa"), "env");
        assert_eq!(says("tool ghp_0123456789abcdefghij"), "tool");
        assert_eq!(says("/nix/store/abc-claude/bin/.claude-wrapped"), "claude");
        assert_eq!(says("node /usr/lib/node_modules/@x/cli.js chat"), "cli chat");
        assert_eq!(says("python3.12 -I script.py"), "python3.12");
        assert_eq!(says("python3 manage.py runserver"), "manage runserver");
    }
}
