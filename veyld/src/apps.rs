//! Las aplicaciones instaladas (sus archivos .desktop), para abrirlas por su
//! nombre y para nombrar bien las ventanas en el diario.

use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config;
use crate::words::fold;

#[derive(Clone, Debug, Default)]
pub struct App {
    pub id: String,
    pub name: String,
    generic: String,
    keywords: String,
    exec: String,
    terminal: bool,
    wmclass: String,
}

/// Las lenguas en las que buscar los nombres: «es_ES.UTF-8» da es_ES y es.
fn locales() -> Vec<String> {
    let lang = ["LC_ALL", "LC_MESSAGES", "LANG"].iter().find_map(|v| env::var(v).ok().filter(|s| !s.is_empty())).unwrap_or_default();
    let lang = lang.split(['.', '@']).next().unwrap_or("").to_string();
    let mut out = Vec::new();
    if !lang.is_empty() && lang != "C" && lang != "POSIX" {
        out.push(lang.clone());
        if let Some((short, _)) = lang.split_once('_') {
            out.push(short.to_string());
        }
    }
    out
}

fn dirs() -> Vec<PathBuf> {
    let home = env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| config::home().join(".local/share"));
    let system = env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    std::iter::once(home).chain(system.split(':').filter(|d| !d.is_empty()).map(PathBuf::from)).map(|d| d.join("applications")).collect()
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<(String, PathBuf)>, visited: &mut HashSet<PathBuf>) {
    let Ok(real) = fs::canonicalize(dir) else {
        return;
    };
    if !visited.insert(real) {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            walk(&path, &format!("{prefix}{name}-"), out, visited);
        } else if let Some(stem) = name.strip_suffix(".desktop") {
            out.push((format!("{prefix}{stem}"), path));
        }
    }
}

fn read(id: String, path: &Path, locales: &[String]) -> Option<App> {
    let text = fs::read_to_string(path).ok()?;
    let mut app = App { id, ..App::default() };
    let mut main = false;
    // De cada clave, la versión en la lengua más precisa que haya.
    let mut rank = [usize::MAX; 3];
    let mut kind = String::new();
    let mut hidden = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            main = line == "[Desktop Entry]";
            continue;
        }
        if !main {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim().to_string());
        let (base, locale) = match key.split_once('[') {
            Some((base, rest)) => (base, Some(rest.trim_end_matches(']'))),
            None => (key, None),
        };
        let level = match locale {
            None => locales.len(),
            Some(l) => match locales.iter().position(|x| x == l) {
                Some(i) => i,
                None => continue,
            },
        };
        let slot = match base {
            "Name" => 0,
            "GenericName" => 1,
            "Keywords" => 2,
            _ => {
                match base {
                    "Type" => kind = value,
                    "Exec" if locale.is_none() => app.exec = value,
                    "Terminal" => app.terminal = value == "true",
                    "NoDisplay" | "Hidden" => hidden |= value == "true",
                    "StartupWMClass" => app.wmclass = value,
                    _ => {}
                }
                continue;
            }
        };
        if level <= rank[slot] {
            rank[slot] = level;
            *[&mut app.name, &mut app.generic, &mut app.keywords][slot] = value;
        }
    }
    (kind == "Application" && !hidden && !app.name.is_empty() && !app.exec.is_empty()).then_some(app)
}

/// Las aplicaciones a la vista; la primera de cada id manda (la del usuario
/// tapa a la del sistema).
pub fn load() -> Vec<App> {
    let locales = locales();
    let mut seen = HashSet::new();
    let mut apps = Vec::new();
    for dir in dirs() {
        let mut found = Vec::new();
        walk(&dir, "", &mut found, &mut HashSet::new());
        found.sort();
        for (id, path) in found {
            if seen.insert(id.clone())
                && let Some(app) = read(id, &path, &locales)
            {
                apps.push(app);
            }
        }
    }
    apps
}

/// Cuánto se parece `query` a un texto: igual, su principio, el principio de
/// una de sus palabras o dentro.
pub fn likeness(query: &str, text: &str) -> f32 {
    if query.is_empty() || text.is_empty() {
        return 0.0;
    }
    let text = fold(text);
    if text == query {
        100.0
    } else if query.chars().count() < 3 {
        // «no», «a»: por dos letras no se abre nada (la voz se come palabras).
        0.0
    } else if text.starts_with(query) {
        80.0
    } else if text.split(|c: char| !c.is_alphanumeric()).any(|word| word.starts_with(query)) {
        60.0
    } else if text.contains(query) {
        35.0
    } else {
        0.0
    }
}

impl App {
    fn program(&self) -> &str {
        let first = self.exec.split_whitespace().next().unwrap_or("");
        first.rsplit('/').next().unwrap_or(first)
    }

    pub fn score(&self, query: &str) -> f32 {
        let keywords = self.keywords.replace(';', " ");
        [
            likeness(query, &self.name),
            likeness(query, &self.generic) * 0.8,
            likeness(query, self.program()) * 0.8,
            likeness(query, self.id.rsplit('.').next().unwrap_or(&self.id)) * 0.7,
            likeness(query, &keywords) * 0.6,
        ]
        .into_iter()
        .fold(0.0, f32::max)
    }

    /// Si una ventana de esta clase es de esta aplicación.
    pub fn owns(&self, class: &str) -> bool {
        let class = fold(class);
        let short = class.rsplit('.').next().unwrap_or(&class).to_string();
        [fold(&self.wmclass), fold(&self.id), fold(self.program())]
            .iter()
            .any(|key| !key.is_empty() && (*key == class || *key == short || key.rsplit('.').next() == Some(short.as_str())))
    }

    /// La orden para abrirla, sin los códigos de campo (%u, %F…).
    pub fn command(&self, terminal: &str) -> String {
        let mut out = String::new();
        let mut chars = self.exec.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '%' {
                // «%%» es un «%»; lo demás, un código que se quita.
                if let Some('%') = chars.next() {
                    out.push('%');
                }
            } else {
                out.push(c);
            }
        }
        let command = out.trim().to_string();
        if self.terminal { format!("{terminal} -e {command}") } else { command }
    }
}

/// Lo que se pide por lo que es, no por su nombre: la aplicación por
/// defecto para ese tipo de cosa.
const KINDS: [(&[&str], &str); 7] = [
    (&["navegador", "navegador web", "browser", "internet"], "x-scheme-handler/https"),
    (&["correo", "email", "mail", "cliente de correo"], "x-scheme-handler/mailto"),
    (&["archivos", "ficheros", "explorador", "explorador de archivos", "gestor de archivos"], "inode/directory"),
    (&["editor", "editor de texto"], "text/plain"),
    (&["reproductor", "reproductor de musica"], "audio/mpeg"),
    (&["visor de imagenes", "imagenes", "fotos"], "image/png"),
    (&["pdf", "visor de pdf", "lector de pdf"], "application/pdf"),
];

/// La aplicación por defecto de un tipo, según xdg-mime.
fn default_for<'a>(apps: &'a [App], query: &str) -> Option<&'a App> {
    let mime = KINDS.iter().find(|(names, _)| names.contains(&query))?.1;
    let out = crate::process::output(std::process::Command::new("xdg-mime").args(["query", "default", mime]), b"", std::time::Duration::from_secs(2)).ok()?;
    let id = String::from_utf8_lossy(&out.stdout).trim().trim_end_matches(".desktop").to_string();
    apps.iter().find(|app| app.id == id)
}

/// Por debajo de esto, un nombre no casa con nada (solo dentro de unas palabras clave).
const ENOUGH: f32 = 35.0;

/// La aplicación que mejor casa con un nombre; a igualdad, una de las que
/// tienen ventanas abiertas (`open`, sus clases) y, si no, la de nombre más corto.
pub fn find<'a>(apps: &'a [App], query: &str, open: &[String]) -> Option<&'a App> {
    let query = fold(query.trim());
    if let Some(app) = default_for(apps, &query) {
        return Some(app);
    }
    let running = |app: &App| open.iter().any(|class| app.owns(class));
    let mut best: Option<(f32, bool, &App)> = None;
    for app in apps {
        let score = app.score(&query);
        let better = match best {
            None => score >= ENOUGH,
            Some((top, was_running, old)) => {
                score > top || (score == top && (running(app), std::cmp::Reverse(app.name.len())) > (was_running, std::cmp::Reverse(old.name.len())))
            }
        };
        if better {
            best = Some((score, running(app), app));
        }
    }
    best.map(|(_, _, app)| app)
}

/// El nombre de la aplicación de una ventana: «Firefox» para «firefox».
pub fn label(apps: &[App], class: &str) -> String {
    apps.iter().find(|app| app.owns(class)).map(|app| app.name.clone()).unwrap_or_else(|| class.rsplit('.').next().unwrap_or(class).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_letters_open_nothing() {
        assert_eq!(likeness("no", "nomacs"), 0.0);
        assert_eq!(likeness("ne", "ne"), 100.0);
        assert_eq!(likeness("nom", "nomacs"), 80.0);
    }

    #[test]
    fn exec_preserves_spaces_inside_quotes() {
        let app = App { exec: "tool --name \"two  spaces\" %u".into(), ..App::default() };
        assert_eq!(app.command("kitty"), "tool --name \"two  spaces\"");
    }

    #[test]
    fn application_scan_handles_symlink_cycles() {
        let dir = std::env::temp_dir().join(format!("veyl-apps-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("app.desktop"), "").unwrap();
        std::os::unix::fs::symlink(&dir, dir.join("cycle")).unwrap();
        let mut found = Vec::new();
        walk(&dir, "", &mut found, &mut HashSet::new());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0, "app");
        fs::remove_dir_all(dir).unwrap();
    }
}
