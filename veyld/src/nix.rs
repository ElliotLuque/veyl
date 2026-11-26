//! Veyl en Nix: dónde está declarado y cómo se le escribe lo que se cambia
//! en los ajustes, para que tu configuración y lo que ves no se separen.
//!
//! Los módulos (`programs.veyl`, en nix/) dejan un manifiesto: en
//! `~/.config/veyl/nix.json` el de Home Manager y en `/etc/veyl/nix.json` el
//! de NixOS. Lleva lo declarado en `settings`, los archivos en los que se
//! declaró (`files`, como los da el sistema de módulos: con un flake, la
//! copia del almacén), dónde está el flake (`flake`), el archivo que se
//! edita si se fija a mano (`file`) y con qué se reconstruye (`rebuild`).
//!
//! Se edita el texto, no se reescribe: con rnix se busca dónde cae cada
//! ajuste (`programs.veyl.settings.voice.speed`, venga como venga escrito:
//! con puntos, en bloques anidados, dentro de `home-manager.users.<tú>`, de
//! una función de módulo o de un `lib.mkIf`) y se cambia solo su valor; lo
//! que no está se añade en el bloque más hondo que ya exista, con su sangría.
//! Lo que calcula una expresión no se pisa.

use std::collections::{HashMap, VecDeque};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, UNIX_EPOCH};

use ring::rand::{SecureRandom, SystemRandom};
use rnix::ast::{self, HasEntry};
use rnix::{TextRange, TextSize};
use rowan::ast::AstNode;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::config;
use crate::daemon::{Event, now};
use crate::process;
use crate::procs;

/// El manifiesto que dejan los módulos de Nix, y dónde estaba.
pub fn manifest() -> Option<(PathBuf, Value)> {
    [config::config().join("nix.json"), PathBuf::from("/etc/veyl/nix.json")].into_iter().find_map(|path| {
        let text = fs::read_to_string(&path).ok()?;
        let value: Value = serde_json::from_str(&text).ok()?;
        value.is_object().then_some((path, value))
    })
}

/// Si la máquina es NixOS.
pub fn nixos() -> bool {
    Path::new("/etc/NIXOS").exists() || fs::read_to_string("/etc/os-release").is_ok_and(|text| text.lines().any(|line| line.trim() == "ID=nixos"))
}

/// Dónde puede estar el flake: el del manifiesto y los de siempre.
fn roots(manifest: &Value) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut push = |raw: &str| {
        let raw = raw.trim().trim_start_matches("path:").trim_start_matches("git+file://");
        let raw = raw.split(['?', '#']).next().unwrap_or("");
        let path = match raw.strip_prefix("~/") {
            Some(rest) => config::home().join(rest),
            None => PathBuf::from(raw),
        };
        if path.is_absolute() && !roots.contains(&path) {
            roots.push(path);
        }
    };
    if let Some(flake) = manifest.get("flake").and_then(Value::as_str) {
        push(flake);
    }
    for var in ["NH_FLAKE", "NH_OS_FLAKE", "NH_HOME_FLAKE", "FLAKE"] {
        if let Ok(value) = std::env::var(var) {
            push(&value);
        }
    }
    push("/etc/nixos");
    for dir in ["dotfiles/nixdots", "nixdots", ".config/nixos", ".config/home-manager", ".config/nix-config", "dotfiles"] {
        push(&config::home().join(dir).to_string_lossy());
    }
    roots
}

/// La primera carpeta de flake encontrada (que tenga `flake.nix`).
pub fn flake_dir(manifest: Option<&Value>) -> Option<PathBuf> {
    let empty = Value::Null;
    roots(manifest.unwrap_or(&empty)).into_iter().find(|dir| dir.join("flake.nix").is_file())
}

fn find_in_path(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let full = dir.join(program);
        if full.is_file() && fs::metadata(&full).is_ok_and(|m| m.permissions().mode() & 0o111 != 0) {
            return Some(full);
        }
    }
    None
}

/// La orden por defecto para reconstruir el sistema (la del manifiesto o la que toque).
pub fn default_rebuild(manifest: Option<&Value>, flake: Option<&Path>) -> String {
    if let Some(cmd) = manifest.and_then(|m| m.get("rebuild")).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()) {
        return cmd.to_string();
    }
    let has_nh = find_in_path("nh").is_some();
    let flake_arg = flake.map(|p| p.to_string_lossy().to_string());
    if nixos() {
        match (has_nh, flake_arg) {
            (true, Some(f)) => format!("nh os switch {f}"),
            (true, None) => "nh os switch".into(),
            (false, Some(f)) => format!("sudo nixos-rebuild switch --flake {f}"),
            (false, None) => "sudo nixos-rebuild switch".into(),
        }
    } else if let Some(f) = flake_arg {
        if has_nh { format!("nh home switch {f}") } else { format!("home-manager switch --flake {f}") }
    } else {
        String::new()
    }
}

/// Una ruta que viene del sistema de módulos puede llevar anotaciones al final
/// (por ejemplo, flake-parts añade «, via option flake.modules.homeManager.veyl»).
/// Devuelve la ruta limpia y, si la trae, el camino de la opción de origen.
fn split_module_file(raw: &str) -> (&str, Option<Vec<String>>) {
    let Some((path, rest)) = raw.split_once(", via ") else {
        return (raw.trim(), None);
    };
    let option = rest.trim().strip_prefix("option ").map(|opt| opt.trim().split('.').filter(|part| !part.is_empty()).map(str::to_string).collect::<Vec<_>>());
    (path.trim(), option.filter(|parts| !parts.is_empty()))
}

fn expand_home(raw: &str) -> PathBuf {
    match raw.trim().strip_prefix("~/") {
        Some(rest) => config::home().join(rest),
        None => PathBuf::from(raw.trim()),
    }
}

/// Un archivo resuelto fuera del almacén, con la opción de origen si venía de
/// un registro de módulos (flake-parts) para desambiguar si hay varios bloques.
#[derive(Clone, Debug, PartialEq, Eq)]
struct TargetFile {
    path: PathBuf,
    scope: Option<Vec<String>>,
}

/// Todos los archivos de tu configuración donde vive `programs.veyl`: el fijado
/// a mano o, de los que da el sistema de módulos, los que se encuentren fuera
/// del almacén (con un flake, el mismo camino dentro de su carpeta).
fn locate_all(manifest: &Value) -> Result<Vec<TargetFile>, String> {
    if let Some(file) = manifest.get("file").and_then(Value::as_str).filter(|f| !f.trim().is_empty()) {
        let path = expand_home(file);
        return if path.is_file() { Ok(vec![TargetFile { path, scope: None }]) } else { Err(format!("no existe {}", config::tilde(&path.to_string_lossy()))) };
    }
    let files: Vec<&str> = manifest.get("files").and_then(Value::as_array).map(|files| files.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    if files.is_empty() {
        return Err("el módulo no sabe en qué archivo está programs.veyl: fíjalo con programs.veyl.nix.file".into());
    }
    let roots = roots(manifest);
    let mut found = Vec::new();
    let mut push_target = |path: PathBuf, scope: Option<Vec<String>>| {
        if !found.iter().any(|existing: &TargetFile| existing.path == path && existing.scope == scope) {
            found.push(TargetFile { path, scope });
        }
    };
    for raw in &files {
        let (file, scope) = split_module_file(raw);
        let Some(rest) = file.strip_prefix("/nix/store/") else {
            let path = expand_home(file);
            if path.is_file() {
                push_target(path, scope);
            }
            continue;
        };
        // «/nix/store/<hash>-source/modules/veyl.nix» → «modules/veyl.nix».
        let Some((_, relative)) = rest.split_once('/') else {
            continue;
        };
        for root in &roots {
            let path = root.join(relative);
            if fs::read_to_string(&path).is_ok_and(|text| text.contains("veyl")) {
                push_target(path, scope.clone());
                break;
            }
        }
    }
    if found.is_empty() { Err("no encuentro la carpeta de tu flake: dila en programs.veyl.nix.flake (o en $NH_FLAKE)".into()) } else { Ok(found) }
}

/// Lo que se cuenta de Nix a Veyl: si es NixOS, si Veyl está declarado,
/// dónde se escribe y cómo se reconstruye.
pub fn status() -> Value {
    let Some((path, manifest)) = manifest() else {
        return json!({ "nixos": nixos(), "declared": false });
    };
    let mut status = json!({
        "nixos": nixos(),
        "declared": true,
        "manifest": path,
        "rebuild": manifest.get("rebuild").cloned().unwrap_or(Value::Null),
    });
    match locate_all(&manifest) {
        Ok(targets) => {
            status["file"] = json!(targets[0].path);
            let mut paths: Vec<&PathBuf> = Vec::new();
            for target in &targets {
                if !paths.contains(&&target.path) {
                    paths.push(&target.path);
                }
            }
            status["files"] = json!(paths);
        }
        Err(why) => status["error"] = json!(why),
    }
    status
}

/// Un cambio: el camino dentro de `settings` y su valor nuevo (`None`, se quita).
pub struct Change {
    pub path: Vec<String>,
    pub value: Option<Value>,
}

impl Change {
    pub fn parse(value: &Value) -> Result<Self, String> {
        let path: Vec<String> =
            value.get("path").and_then(Value::as_array).map(|path| path.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
        if path.is_empty() {
            return Err("un cambio sin camino".into());
        }
        let value = value.get("value").cloned().filter(|v| !v.is_null());
        Ok(Self { path, value })
    }
}

/// Qué tan bien encaja un archivo para un cambio concreto: si ya declara la
/// clave exacta gana; si no, el que tenga el bloque padre más hondo bajo
/// `settings`; si ninguno, 0 (se queda el primero).
fn affinity(text: &str, scope: Option<&[String]>, change: &Change) -> usize {
    let parse = rnix::Root::parse(text);
    if !parse.errors().is_empty() {
        return 0;
    }
    let mut bindings = Vec::new();
    walk(parse.tree().expr(), &[], &mut bindings);
    let mut target = vec!["settings".to_string()];
    target.extend(change.path.iter().cloned());
    let ours = scoped_bindings(&bindings, scope);
    if ours.iter().any(|(_, rel)| *rel == target.as_slice()) {
        return 1000 + target.len();
    }
    if change.value.is_none() && ours.iter().any(|(_, rel)| rel.starts_with(&target)) {
        return 1000 + target.len();
    }
    // Solo cuentan bloques bajo `settings.<algo>` (no el `settings = { }` genérico).
    ours.iter()
        .filter(|(b, rel)| rel.len() > 1 && target.starts_with(rel) && matches!(b.node.value(), Some(ast::Expr::AttrSet(_))))
        .map(|(_, rel)| rel.len())
        .max()
        .unwrap_or(0)
}

/// Escribe los cambios en tu configuración (en el archivo que ya declara cada
/// ajuste cuando hay varios, como uno común y otro por máquina). Devuelve los
/// archivos modificados.
pub fn write(changes: &[Change]) -> Result<Vec<PathBuf>, String> {
    let (_, manifest) = manifest().ok_or("Veyl no está en tu configuración de Nix")?;
    let targets = locate_all(&manifest)?;
    let mut texts: Vec<String> = Vec::with_capacity(targets.len());
    for target in &targets {
        texts.push(fs::read_to_string(&target.path).map_err(|e| format!("{}: {e}", target.path.display()))?);
    }
    let mut touched = vec![false; targets.len()];
    for change in changes {
        // Si dos entradas apuntan al mismo archivo, sus textos van sincronizados al final.
        let chosen = if targets.len() == 1 {
            0
        } else {
            let mut best = 0;
            let mut best_score = affinity(&texts[0], targets[0].scope.as_deref(), change);
            for (index, target) in targets.iter().enumerate().skip(1) {
                let score = affinity(&texts[index], target.scope.as_deref(), change);
                if score > best_score {
                    best = index;
                    best_score = score;
                }
            }
            best
        };
        let path = &targets[chosen].path;
        let updated =
            edit(&texts[chosen], change, targets[chosen].scope.as_deref()).map_err(|why| format!("{}: {why}", config::tilde(&path.to_string_lossy())))?;
        // Mantener sincronizadas posibles entradas duplicadas sobre el mismo archivo.
        for (index, target) in targets.iter().enumerate() {
            if target.path == *path {
                texts[index] = updated.clone();
            }
        }
        touched[chosen] = true;
    }
    let mut written = Vec::new();
    for (index, target) in targets.iter().enumerate() {
        if !touched[index] || written.contains(&target.path) {
            continue;
        }
        // Junto al original y luego encima: un corte no deja el archivo a medias.
        let temporary = target.path.with_extension("nix.veyl~");
        fs::write(&temporary, &texts[index]).and_then(|()| fs::rename(&temporary, &target.path)).map_err(|e| format!("{}: {e}", target.path.display()))?;
        written.push(target.path.clone());
    }
    Ok(written)
}

/// Un nombre de atributo como se escribe: tal cual si se puede, si no, entre comillas.
fn name(key: &str) -> String {
    let mut chars = key.chars();
    let plain = chars.next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '\''))
        && !matches!(key, "if" | "then" | "else" | "assert" | "with" | "let" | "in" | "rec" | "inherit" | "or");
    if plain { key.to_string() } else { string(key) }
}

fn string(text: &str) -> String {
    let mut out = String::from("\"");
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '$' if chars.peek() == Some(&'{') => out.push_str("\\$"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Un valor de JSON escrito en Nix; lo anidado, con la sangría de `indent`.
pub fn render(value: &Value, indent: &str) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => string(s),
        Value::Array(items) if items.iter().all(|item| !item.is_object() && !item.is_array()) => {
            if items.is_empty() {
                "[ ]".into()
            } else {
                format!("[ {} ]", items.iter().map(|item| render(item, indent)).collect::<Vec<_>>().join(" "))
            }
        }
        Value::Array(items) => {
            let inner = format!("{indent}  ");
            let body: String = items.iter().map(|item| format!("\n{inner}{}", render(item, &inner))).collect();
            format!("[{body}\n{indent}]")
        }
        Value::Object(map) if map.is_empty() => "{ }".into(),
        Value::Object(map) => {
            let inner = format!("{indent}  ");
            let body: String = map.iter().map(|(key, value)| format!("\n{inner}{} = {};", name(key), render(value, &inner))).collect();
            format!("{{{body}\n{indent}}}")
        }
    }
}

/// Lo que se pega en una configuración que aún no tiene a Veyl.
pub fn snippet(settings: &Map<String, Value>) -> String {
    let mut text = String::from("programs.veyl = {\n  enable = true;\n");
    if !settings.is_empty() {
        text.push_str(&format!("  settings = {};\n", render(&Value::Object(settings.clone()), "  ")));
    }
    text.push_str("};\n");
    text
}

/// Una asignación encontrada: su camino entero (con lo de los bloques de
/// fuera), cuántos tramos escribe ella misma, su nodo y el bloque que la tiene.
struct Binding {
    full: Vec<String>,
    own: usize,
    node: ast::AttrpathValue,
}

/// El texto de un nombre de atributo, si es fijo (`a`, `"a-b"`; no `${x}`).
fn attr(attr: &ast::Attr) -> Option<String> {
    match attr {
        ast::Attr::Ident(ident) => Some(ident.syntax().text().to_string()),
        ast::Attr::Str(text) => {
            let mut out = String::new();
            for part in text.normalized_parts() {
                match part {
                    ast::InterpolPart::Literal(literal) => out.push_str(&literal),
                    ast::InterpolPart::Interpolation(_) => return None,
                }
            }
            Some(out)
        }
        ast::Attr::Dynamic(_) => None,
    }
}

/// Todas las asignaciones de un archivo de módulo, con su camino entero.
fn walk(expr: Option<ast::Expr>, prefix: &[String], out: &mut Vec<Binding>) {
    let Some(expr) = expr else {
        return;
    };
    match expr {
        ast::Expr::Paren(e) => walk(e.expr(), prefix, out),
        ast::Expr::Lambda(e) => walk(e.body(), prefix, out),
        ast::Expr::LetIn(e) => walk(e.body(), prefix, out),
        ast::Expr::With(e) => walk(e.body(), prefix, out),
        ast::Expr::Assert(e) => walk(e.body(), prefix, out),
        ast::Expr::IfElse(e) => {
            walk(e.body(), prefix, out);
            walk(e.else_body(), prefix, out);
        }
        // `lib.mkIf cond { … }`, `lib.mkMerge [ … ]`.
        ast::Expr::Apply(e) => {
            match e.argument() {
                Some(ast::Expr::List(list)) => list.items().for_each(|item| walk(Some(item), prefix, out)),
                argument => walk(argument, prefix, out),
            }
            if let Some(inner @ ast::Expr::Apply(_)) = e.lambda() {
                walk(Some(inner), prefix, out);
            }
        }
        ast::Expr::BinOp(e) => {
            walk(e.lhs(), prefix, out);
            walk(e.rhs(), prefix, out);
        }
        ast::Expr::AttrSet(set) => {
            for node in set.attrpath_values() {
                let Some(path) = node.attrpath().map(|path| path.attrs().map(|a| attr(&a)).collect::<Option<Vec<_>>>()) else {
                    continue;
                };
                let Some(path) = path else {
                    continue;
                };
                let mut full = prefix.to_vec();
                full.extend(path.iter().cloned());
                let value = node.value();
                out.push(Binding { full: full.clone(), own: path.len(), node });
                walk(value, &full, out);
            }
        }
        _ => {}
    }
}

/// Lo que queda del camino tras `programs.veyl`, si lo lleva.
fn under_veyl(full: &[String]) -> Option<&[String]> {
    full.windows(2).position(|pair| pair[0] == "programs" && pair[1] == "veyl").map(|at| &full[at + 2..])
}

/// Las asignaciones bajo `programs.veyl`. Si el módulo venía con un camino de
/// opción (flake-parts, «flake.modules.homeManager.veyl») y alguna cae dentro
/// de él, se queda solo con esas para no tocar otro bloque del mismo archivo.
fn scoped_bindings<'a>(bindings: &'a [Binding], scope: Option<&[String]>) -> Vec<(&'a Binding, &'a [String])> {
    let all: Vec<(&Binding, &[String])> = bindings.iter().filter_map(|b| under_veyl(&b.full).map(|rel| (b, rel))).collect();
    if let Some(prefix) = scope {
        let filtered: Vec<(&Binding, &[String])> = all.iter().copied().filter(|(b, _)| b.full.starts_with(prefix)).collect();
        if !filtered.is_empty() {
            return filtered;
        }
    }
    all
}

/// Si una expresión es un valor escrito tal cual (y se puede cambiar por otro).
fn plain(expr: &ast::Expr) -> bool {
    match expr {
        ast::Expr::Literal(_) => true,
        ast::Expr::Str(text) => text.normalized_parts().iter().all(|part| matches!(part, ast::InterpolPart::Literal(_))),
        ast::Expr::Ident(ident) => matches!(ident.syntax().text().to_string().as_str(), "true" | "false" | "null"),
        ast::Expr::UnaryOp(op) => matches!(op.expr(), Some(ast::Expr::Literal(_))),
        ast::Expr::Paren(e) => e.expr().is_some_and(|e| plain(&e)),
        ast::Expr::List(list) => list.items().all(|item| plain(&item)),
        ast::Expr::AttrSet(set) => {
            set.rec_token().is_none() && set.inherits().next().is_none() && set.attrpath_values().all(|node| node.value().is_some_and(|v| plain(&v)))
        }
        _ => false,
    }
}

/// La sangría de la línea en la que empieza `offset`.
fn indent_at(text: &str, offset: usize) -> String {
    let start = text[..offset].rfind('\n').map_or(0, |at| at + 1);
    text[start..].chars().take_while(|c| *c == ' ' || *c == '\t').collect()
}

/// El texto con un cambio hecho.
pub fn edit(text: &str, change: &Change, scope: Option<&[String]>) -> Result<String, String> {
    let parse = rnix::Root::parse(text);
    if let Some(error) = parse.errors().first() {
        return Err(format!("no se puede leer: {error}"));
    }
    let mut bindings = Vec::new();
    walk(parse.tree().expr(), &[], &mut bindings);
    let mut target = vec!["settings".to_string()];
    target.extend(change.path.iter().cloned());
    let shown = format!("programs.veyl.{}", target.join("."));

    // Las que ya hay bajo programs.veyl, con su camino desde ahí.
    let ours = scoped_bindings(&bindings, scope);
    if ours.is_empty() {
        return Err("no aparece programs.veyl".into());
    }

    // Si se quita: fuera la asignación y todo lo que cuelga de ella.
    let Some(value) = &change.value else {
        let mut ranges: Vec<TextRange> = ours.iter().filter(|(_, rel)| rel.starts_with(&target)).map(|(b, _)| b.node.syntax().text_range()).collect();
        // Las de dentro caen con las de fuera.
        ranges.sort_by_key(|r| (r.start(), std::cmp::Reverse(r.end())));
        ranges.dedup_by(|inner, outer| outer.contains_range(*inner));
        let mut out = text.to_string();
        for range in ranges.iter().rev() {
            let (start, end) = (usize::from(range.start()), usize::from(range.end()));
            // Con la línea entera, si la asignación la ocupa sola.
            let line = text[..start].rfind('\n').map_or(0, |at| at + 1);
            let after = text[end..].find('\n').map_or(text.len(), |at| end + at);
            if text[line..start].trim().is_empty() && text[end..after].trim().is_empty() {
                out.replace_range(line..(after + 1).min(text.len()), "");
            } else {
                out.replace_range(start..end, "");
            }
        }
        return Ok(out);
    };

    // Ya está: se cambia su valor, si es uno escrito tal cual.
    if let Some((binding, _)) = ours.iter().find(|(_, rel)| *rel == target.as_slice()) {
        let old = binding.node.value().ok_or("una asignación sin valor")?;
        if !plain(&old) {
            return Err(format!("{shown} lo calcula una expresión: cámbialo a mano"));
        }
        let range = old.syntax().text_range();
        let indent = indent_at(text, usize::from(binding.node.syntax().text_range().start()));
        let mut out = text.to_string();
        out.replace_range(usize::from(range.start())..usize::from(range.end()), &render(value, &indent));
        return Ok(out);
    }

    // Algo de más arriba que no es un bloque (`voice = cfg.voz;`) no deja escribir dentro.
    if let Some((_, rel)) = ours.iter().find(|(b, rel)| target.starts_with(rel) && !matches!(b.node.value(), Some(ast::Expr::AttrSet(_)))) {
        return Err(format!("programs.veyl.{} no es un bloque {{ … }}: cámbialo a mano", rel.join(".")));
    }

    // No está: dentro del bloque más hondo que ya lleve a él…
    let deepest =
        ours.iter().filter(|(b, rel)| target.starts_with(rel) && matches!(b.node.value(), Some(ast::Expr::AttrSet(_)))).max_by_key(|(_, rel)| rel.len());
    if let Some((binding, rel)) = deepest {
        let Some(ast::Expr::AttrSet(set)) = binding.node.value() else { unreachable!() };
        let rest: Vec<String> = target[rel.len()..].iter().map(|key| name(key)).collect();
        return Ok(insert(text, &set, &rest.join("."), value));
    }

    // …o al lado de otra asignación de programs.veyl (`programs.veyl.enable = true;`).
    let (binding, rel) = ours.iter().min_by_key(|(_, rel)| rel.len()).ok_or("no aparece programs.veyl")?;
    let own = &binding.full[binding.full.len() - binding.own..];
    let outer = &own[..own.len() - rel.len().min(own.len())];
    let path: Vec<String> = outer.iter().chain(target.iter()).map(|key| name(key)).collect();
    let range = binding.node.syntax().text_range();
    let indent = indent_at(text, usize::from(range.start()));
    let line = format!("\n{indent}{} = {};", path.join("."), render(value, &indent));
    let mut out = text.to_string();
    out.insert_str(usize::from(range.end()), &line);
    Ok(out)
}

/// Añade `path = value;` al final de un bloque, con la sangría de los suyos.
fn insert(text: &str, set: &ast::AttrSet, path: &str, value: &Value) -> String {
    let entries: Vec<ast::Entry> = set.entries().collect();
    let close = set.r_curly_token().map_or(set.syntax().text_range().end() - TextSize::from(1), |t| t.text_range().start());
    let outer = indent_at(text, usize::from(set.syntax().text_range().start()));
    let mut out = text.to_string();
    match entries.last() {
        Some(last) => {
            let indent = indent_at(text, usize::from(last.syntax().text_range().start()));
            // En una línea (`{ a = 1; }`) se sigue en la misma.
            let inline = !text[usize::from(set.syntax().text_range().start())..usize::from(close)].contains('\n');
            let at = usize::from(last.syntax().text_range().end());
            let piece = if inline { format!(" {path} = {};", render(value, &outer)) } else { format!("\n{indent}{path} = {};", render(value, &indent)) };
            out.insert_str(at, &piece);
        }
        None => {
            let open = set.l_curly_token().map_or(set.syntax().text_range().start(), |t| t.text_range().end());
            let indent = format!("{outer}  ");
            let piece = format!("\n{indent}{path} = {};\n{outer}", render(value, &indent));
            out.replace_range(usize::from(open)..usize::from(close), &piece);
        }
    }
    out
}

/// `veyld nix …`: lo que pide Veyl desde los ajustes o para el sistema.
pub fn cli(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        None | Some("status") => {
            println!("{}", status());
            Ok(())
        }
        // `veyld nix write '[{"path": ["voice", "speed"], "value": 1.3}]'`
        Some("write") => {
            let changes: Value = serde_json::from_str(args.get(1).ok_or("falta lo que se escribe")?).map_err(|e| e.to_string())?;
            let changes = changes.as_array().ok_or("se esperaba una lista de cambios")?.iter().map(Change::parse).collect::<Result<Vec<_>, _>>()?;
            let files = write(&changes)?;
            let file = files.first().cloned().unwrap_or_default();
            println!("{}", json!({ "file": file, "files": files, "written": changes.len() }));
            Ok(())
        }
        // `veyld nix show '{"scheme": "…"}'`: cómo se escribiría en una configuración nueva.
        Some("show") => {
            let settings: Value = serde_json::from_str(args.get(1).map_or("{}", String::as_str)).map_err(|e| e.to_string())?;
            print!("{}", snippet(settings.as_object().ok_or("se esperaba un objeto")?));
            Ok(())
        }
        // `veyld nix check`: comprueba actualizaciones de las entradas del flake.
        Some("check") => {
            let manifest = manifest().map(|(_, v)| v);
            let flake = flake_dir(manifest.as_ref()).ok_or("no encuentro ningún flake")?;
            let cache = check_updates(&flake)?;
            println!("{}", serde_json::to_string_pretty(&cache).map_err(|e| e.to_string())?);
            Ok(())
        }
        // `veyld nix askpass [<prompt>]`: usado como SUDO_ASKPASS durante un rebuild desde Veyl.
        Some("askpass") => askpass_cli(args.get(1).map(String::as_str).unwrap_or("Contraseña de sudo")),
        Some(other) => Err(format!("no sé qué es «{other}»: status, write, show, check o askpass")),
    }
}

// ---------------------------------------------------------------------------
// Estado del sistema NixOS, actualizaciones del flake y reconstrucción en vivo
// ---------------------------------------------------------------------------

/// Cada cuánto se comprueban automáticamente las actualizaciones del flake.
const AUTO_CHECK_EVERY: Duration = Duration::from_secs(3 * 60 * 60);
/// Cada cuánto se revisa el estado local (generación, flake.lock, procesos).
const LOCAL_POLL_EVERY: Duration = Duration::from_secs(10);
/// Cuánto puede tardar una reconstrucción o actualización como mucho.
const JOB_TIMEOUT: Duration = Duration::from_secs(60 * 60);
/// Cuántas líneas de salida se mandan al shell (el resto va al archivo de log).
const LOG_TAIL: usize = 12;

fn mtime_ms(path: &Path) -> i64 {
    fs::symlink_metadata(path)
        .or_else(|_| fs::metadata(path))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Extrae el número de generación de `system-827-link` → `827`.
pub fn parse_generation_link(target: &str) -> Option<u32> {
    let name = Path::new(target).file_name()?.to_str()?;
    let rest = name.strip_prefix("system-")?.strip_suffix("-link")?;
    rest.parse().ok()
}

/// Extrae la versión de NixOS de `/nix/store/…-nixos-system-host-26.11.20261121.2786a2a`.
pub fn parse_system_version(target: &str) -> Option<String> {
    let name = Path::new(target).file_name()?.to_str()?;
    let after = name.split_once("nixos-system-")?.1;
    // Busca el tramo `YY.MM...` al final del nombre.
    for (idx, part) in after.match_indices('.') {
        if idx >= 2 {
            let prefix = &after[idx - 2..idx];
            if prefix.chars().all(|c| c.is_ascii_digit()) && (idx == 2 || after.as_bytes()[idx - 3] == b'-') {
                let tail = &after[idx - 2..];
                let parts: Vec<&str> = tail.split('.').collect();
                if parts.len() >= 3 && parts[1].chars().all(|c| c.is_ascii_digit()) {
                    return Some(format!("{}.{}.{}", parts[0], parts[1], parts[2]));
                }
                if parts.len() >= 2 && parts[1].chars().all(|c| c.is_ascii_digit()) {
                    return Some(format!("{}.{}", parts[0], parts[1]));
                }
            }
        }
        let _ = part;
    }
    None
}

/// Si el kernel del sistema activo difiere del que arrancó la máquina.
fn reboot_needed() -> bool {
    let Ok(booted) = fs::canonicalize("/run/booted-system/kernel") else {
        return false;
    };
    let Ok(current) = fs::canonicalize("/run/current-system/kernel") else {
        return false;
    };
    booted != current
}

/// Una entrada directa y flotante de `flake.lock` (sin `rev` fijado a mano).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlakeInput {
    pub name: String,
    pub url: String,
    pub refspec: String,
    pub branch: String,
    pub locked_rev: String,
    pub last_modified: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub name: String,
    pub branch: String,
    pub current_rev: String,
    pub remote_rev: String,
    pub last_modified: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatesCache {
    pub flake: String,
    pub checked_at: i64,
    pub lock_modified: i64,
    pub checked_count: usize,
    pub updates: Vec<UpdateInfo>,
    #[serde(default)]
    pub error: String,
}

fn cache_path() -> PathBuf {
    config::state().join("nix-updates.json")
}

pub fn log_path() -> PathBuf {
    config::state().join("nix-last.log")
}

/// Lee las entradas flotantes directas de `flake.lock` y la fecha de `nixpkgs`.
pub fn parse_lock_inputs(text: &str) -> (Vec<FlakeInput>, usize, i64) {
    let Ok(json) = serde_json::from_str::<Value>(text) else {
        return (Vec::new(), 0, 0);
    };
    let Some(nodes) = json.get("nodes").and_then(Value::as_object) else {
        return (Vec::new(), 0, 0);
    };
    let root_name = json.get("root").and_then(Value::as_str).unwrap_or("root");
    let Some(root_inputs) = nodes.get(root_name).and_then(|n| n.get("inputs")).and_then(Value::as_object) else {
        return (Vec::new(), 0, 0);
    };
    let total = root_inputs.len();
    let mut nixpkgs_modified = 0i64;
    let mut out = Vec::new();
    for (name, target) in root_inputs {
        let Some(node_key) = target.as_str() else {
            continue;
        };
        let Some(node) = nodes.get(node_key) else {
            continue;
        };
        let locked = node.get("locked");
        let last_modified = locked.and_then(|l| l.get("lastModified")).and_then(Value::as_i64).unwrap_or(0) * 1000;
        if name == "nixpkgs" {
            nixpkgs_modified = last_modified;
        }
        let Some(orig) = node.get("original").and_then(Value::as_object) else {
            continue;
        };
        // Una revisión fijada a mano (`rev`) no flota.
        if orig.get("rev").and_then(Value::as_str).is_some_and(|s| !s.is_empty()) {
            continue;
        }
        let Some(locked_rev) = locked.and_then(|l| l.get("rev")).and_then(Value::as_str).filter(|s| !s.is_empty()) else {
            continue;
        };
        let kind = orig.get("type").and_then(Value::as_str).unwrap_or("");
        let url = match kind {
            "github" => {
                let (Some(owner), Some(repo)) = (orig.get("owner").and_then(Value::as_str), orig.get("repo").and_then(Value::as_str)) else {
                    continue;
                };
                let host = orig.get("host").and_then(Value::as_str).unwrap_or("github.com");
                format!("https://{host}/{owner}/{repo}.git")
            }
            "gitlab" => {
                let (Some(owner), Some(repo)) = (orig.get("owner").and_then(Value::as_str), orig.get("repo").and_then(Value::as_str)) else {
                    continue;
                };
                let host = orig.get("host").and_then(Value::as_str).unwrap_or("gitlab.com");
                format!("https://{host}/{owner}/{repo}.git")
            }
            "sourcehut" => {
                let (Some(owner), Some(repo)) = (orig.get("owner").and_then(Value::as_str), orig.get("repo").and_then(Value::as_str)) else {
                    continue;
                };
                let host = orig.get("host").and_then(Value::as_str).unwrap_or("git.sr.ht");
                format!("https://{host}/{owner}/{repo}")
            }
            "git" => {
                let Some(u) = orig.get("url").and_then(Value::as_str) else {
                    continue;
                };
                if !(u.starts_with("https://") || u.starts_with("http://") || u.starts_with("ssh://") || u.starts_with("git@")) {
                    continue;
                }
                u.to_string()
            }
            _ => continue,
        };
        let branch = orig.get("ref").and_then(Value::as_str).unwrap_or("").to_string();
        // Si trae `ref`, solo miramos `refs/heads/<ref>` para ignorar etiquetas fijas (`v0.9.3`).
        let refspec = if branch.is_empty() { "HEAD".to_string() } else { format!("refs/heads/{branch}") };
        out.push(FlakeInput { name: name.clone(), url, refspec, branch, locked_rev: locked_rev.to_string(), last_modified });
    }
    (out, total, nixpkgs_modified)
}

fn input_rank(name: &str) -> (u8, &str) {
    match name {
        "nixpkgs" => (0, name),
        "home-manager" => (1, name),
        "veyl" => (2, name),
        _ => (3, name),
    }
}

fn ls_remote(url: &str, refspec: &str) -> Option<String> {
    let mut cmd = Command::new("git");
    cmd.args(["-c", "credential.helper=", "ls-remote", url, refspec]).env("GIT_TERMINAL_PROMPT", "0");
    let out = process::output(&mut cmd, b"", Duration::from_secs(12)).ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let sha = text.lines().next()?.split_whitespace().next()?;
    (sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit())).then(|| sha.to_string())
}

/// Comprueba en paralelo qué entradas directas del flake tienen commits nuevos.
pub fn check_updates(flake: &Path) -> Result<UpdatesCache, String> {
    let lock_path = flake.join("flake.lock");
    let text = fs::read_to_string(&lock_path).map_err(|e| format!("{}: {e}", lock_path.display()))?;
    let lock_modified = mtime_ms(&lock_path);
    let (inputs, _, _) = parse_lock_inputs(&text);
    let checked_count = inputs.len();

    let queue = Arc::new(Mutex::new(VecDeque::from(inputs)));
    let results = Arc::new(Mutex::new(Vec::<UpdateInfo>::new()));
    let workers = checked_count.clamp(1, 6);
    let mut handles = Vec::with_capacity(workers);
    for _ in 0..workers {
        let queue = Arc::clone(&queue);
        let results = Arc::clone(&results);
        handles.push(thread::spawn(move || {
            loop {
                let item = {
                    let mut q = queue.lock().unwrap_or_else(|e| e.into_inner());
                    q.pop_front()
                };
                let Some(input) = item else {
                    break;
                };
                if let Some(remote_rev) = ls_remote(&input.url, &input.refspec)
                    && remote_rev != input.locked_rev
                {
                    let mut res = results.lock().unwrap_or_else(|e| e.into_inner());
                    res.push(UpdateInfo {
                        name: input.name,
                        branch: input.branch,
                        current_rev: input.locked_rev,
                        remote_rev,
                        last_modified: input.last_modified,
                    });
                }
            }
        }));
    }
    for handle in handles {
        let _ = handle.join();
    }
    let mut updates = results.lock().unwrap_or_else(|e| e.into_inner()).clone();
    updates.sort_by(|a, b| input_rank(&a.name).cmp(&input_rank(&b.name)));
    let cache = UpdatesCache { flake: flake.to_string_lossy().to_string(), checked_at: now(), lock_modified, checked_count, updates, error: String::new() };
    save_updates_cache(&cache);
    Ok(cache)
}

fn load_updates_cache(flake: &Path) -> Option<UpdatesCache> {
    let text = fs::read_to_string(cache_path()).ok()?;
    let mut cache: UpdatesCache = serde_json::from_str(&text).ok()?;
    if cache.flake != flake.to_string_lossy() {
        return None;
    }
    reconcile_updates(flake, &mut cache);
    Some(cache)
}

fn save_updates_cache(cache: &UpdatesCache) {
    let path = cache_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(cache) {
        let _ = fs::write(&path, text);
    }
}

/// Si `flake.lock` cambió (p. ej. tras un `nix flake update`), saca de la lista
/// de pendientes las entradas cuyo `rev` ya cambió.
pub fn reconcile_updates(flake: &Path, cache: &mut UpdatesCache) {
    let lock_path = flake.join("flake.lock");
    let current_mtime = mtime_ms(&lock_path);
    if current_mtime == 0 || current_mtime == cache.lock_modified {
        return;
    }
    let Ok(text) = fs::read_to_string(&lock_path) else {
        return;
    };
    let (inputs, _, _) = parse_lock_inputs(&text);
    let revs: HashMap<String, String> = inputs.into_iter().map(|i| (i.name, i.locked_rev)).collect();
    cache.updates.retain(|u| revs.get(&u.name).is_some_and(|rev| rev == &u.current_rev && rev != &u.remote_rev));
    cache.lock_modified = current_mtime;
}

/// Quita secuencias ANSI y se queda con el último tramo tras `\r`.
pub fn strip_ansi(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            match chars.peek().copied() {
                Some('[') => {
                    chars.next();
                    for ch in chars.by_ref() {
                        if ('@'..='~').contains(&ch) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    chars.next();
                    while let Some(ch) = chars.next() {
                        if ch == '\x07' || (ch == '\x1b' && chars.peek() == Some(&'\\')) {
                            if ch == '\x1b' {
                                chars.next();
                            }
                            break;
                        }
                    }
                }
                _ => {}
            }
        } else if c == '\r' {
            out.clear();
        } else if !c.is_control() || c == '\t' {
            out.push(c);
        }
    }
    out.trim().to_string()
}

/// Traduce una línea de salida de Nix / nh a una fase legible.
pub fn parse_phase(kind: &str, line: &str) -> Option<&'static str> {
    let lower = line.to_lowercase();
    if kind == "update" {
        if lower.contains("updated input") {
            return Some("Actualizando entradas…");
        }
        if lower.contains("fetching") || lower.contains("unpacking") || lower.contains("downloading") {
            return Some("Descargando entradas…");
        }
        return None;
    }
    if lower.contains("activating")
        || lower.contains("setting up /etc")
        || lower.contains("reloading user units")
        || lower.contains("starting the following units")
        || lower.contains("switching")
    {
        return Some("Activando…");
    }
    if lower.contains("comparing changes") || lower.contains("upgraded packages") || lower.contains("added packages") || line.starts_with("<<< ") {
        return Some("Comparando cambios…");
    }
    if lower.contains("building ") || lower.contains("will be built") || lower.contains("compiling") {
        return Some("Compilando…");
    }
    if lower.contains("copying path") || lower.contains("fetching ") || lower.contains("will be fetched") || lower.contains("downloading") {
        return Some("Descargando…");
    }
    if lower.contains("evaluating") || lower.contains("building nixos configuration") || lower.contains("warning: git tree") {
        return Some("Evaluando…");
    }
    None
}

/// Extrae el nombre de entrada de `• Updated input 'nixpkgs':`.
pub fn parse_updated_input(line: &str) -> Option<String> {
    let rest = line.split_once("Updated input '")?.1;
    let name = rest.split_once('\'')?.0.trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// Cuenta cuántos archivos modificados o sin seguir tiene el repo del flake.
fn git_dirty(flake: &Path) -> usize {
    if !flake.join(".git").exists() {
        return 0;
    }
    let mut cmd = Command::new("git");
    cmd.args(["-C", &flake.to_string_lossy(), "status", "--porcelain"]);
    let Ok(out) = process::output(&mut cmd, b"", Duration::from_secs(2)) else {
        return 0;
    };
    if !out.status.success() {
        return 0;
    }
    String::from_utf8_lossy(&out.stdout).lines().filter(|l| !l.trim().is_empty()).count()
}

/// Detecta si hay un `nh os switch`, `nixos-rebuild` o `nix flake update` en una terminal externa.
fn detect_external() -> Option<(String, String)> {
    let me = std::process::id() as i32;
    let table = procs::all();
    for (&pid, stat) in &table {
        if pid == me || stat.parent == me {
            continue;
        }
        let Ok(bytes) = fs::read(format!("/proc/{pid}/cmdline")) else {
            continue;
        };
        let parts: Vec<String> = bytes.split(|&b| b == 0).filter(|p| !p.is_empty()).map(|p| String::from_utf8_lossy(p).to_string()).collect();
        if parts.is_empty() {
            continue;
        }
        let cmd = Path::new(&parts[0]).file_name().and_then(|s| s.to_str()).unwrap_or("");
        let rest = parts[1..].join(" ");
        if cmd == "nh" && (rest.contains("os switch") || rest.contains("home switch") || rest.contains("os boot")) {
            return Some(("rebuild".into(), "Reconstruyendo en terminal…".into()));
        }
        if cmd == "nixos-rebuild" && (rest.contains("switch") || rest.contains("boot")) {
            return Some(("rebuild".into(), "Reconstruyendo en terminal…".into()));
        }
        if cmd == "nix" && rest.contains("flake") && rest.contains("update") {
            return Some(("update".into(), "Actualizando flake en terminal…".into()));
        }
    }
    None
}

/// Estado compartido para el askpass de `sudo` (solo vive en memoria).
#[derive(Default)]
pub struct AuthShared {
    pub token: Option<String>,
    pub prompt: Option<String>,
    pub attempt: u32,
}

impl AuthShared {
    pub fn snapshot(&self) -> Value {
        match &self.prompt {
            Some(prompt) => json!({ "prompt": prompt, "attempt": self.attempt }),
            None => Value::Null,
        }
    }
}

fn random_token() -> String {
    let mut bytes = [0u8; 24];
    let _ = SystemRandom::new().fill(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sh_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// Prepara los scripts de elevación (`askpass`, `elevate`, `sudo`) en
/// `$XDG_RUNTIME_DIR/veyl/nix-bin` con permisos 0700.
fn prepare_elevation_dir() -> Result<PathBuf, String> {
    let dir = config::runtime().join("veyl").join("nix-bin");
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));

    let veyld = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let real_sudo = ["/run/wrappers/bin/sudo", "/usr/bin/sudo", "/bin/sudo"]
        .into_iter()
        .map(PathBuf::from)
        .find(|p| p.is_file())
        .or_else(|| find_in_path("sudo"))
        .unwrap_or_else(|| PathBuf::from("sudo"));

    let veyld_q = sh_quote(&veyld.to_string_lossy());
    let sudo_q = sh_quote(&real_sudo.to_string_lossy());

    let write_script = |name: &str, body: String| -> Result<(), String> {
        let path = dir.join(name);
        fs::write(&path, body).map_err(|e| format!("{}: {e}", path.display()))?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(())
    };

    write_script("askpass", format!("#!/bin/sh\nexec {veyld_q} nix askpass \"$@\"\n"))?;
    write_script("elevate", format!("#!/bin/sh\nif {sudo_q} -n true 2>/dev/null; then\n  exec {sudo_q} \"$@\"\nfi\nexec {sudo_q} -A \"$@\"\n"))?;
    write_script(
        "sudo",
        format!(
            "#!/bin/sh\nif [ -n \"${{SUDO_ASKPASS:-}}\" ]; then\n  if {sudo_q} -n true 2>/dev/null; then\n    exec {sudo_q} \"$@\"\n  fi\n  exec {sudo_q} -A \"$@\"\nfi\nexec {sudo_q} \"$@\"\n"
        ),
    )?;
    Ok(dir)
}

/// `veyld nix askpass`: llamado por `sudo -A`. Pide la contraseña al daemon
/// por el socket local y la imprime por stdout (nunca en disco ni en logs).
pub fn askpass_cli(prompt: &str) -> Result<(), String> {
    let token = std::env::var("VEYL_NIX_AUTH_TOKEN").map_err(|_| "falta VEYL_NIX_AUTH_TOKEN".to_string())?;
    let path = config::socket();
    let mut stream = UnixStream::connect(&path).map_err(|_| "veyld no está en marcha".to_string())?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(120)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let msg = json!({ "nix_askpass": { "token": token, "prompt": prompt } });
    writeln!(stream, "{msg}").map_err(|e| e.to_string())?;
    let _ = stream.flush();
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).map_err(|e| e.to_string())?;
    let reply: Value = serde_json::from_str(&line).map_err(|_| "cancelado".to_string())?;
    if let Some(password) = reply.get("password").and_then(Value::as_str) {
        println!("{password}");
        Ok(())
    } else {
        Err("cancelado".into())
    }
}

/// Órdenes que el daemon manda al hilo de Nix.
pub enum Job {
    Refresh,
    Check,
    Update { input: Option<String>, then_rebuild: bool },
    Rebuild,
    Stop,
    Dismiss,
}

#[derive(Clone, Debug, Default)]
struct JobState {
    kind: String,  // "" | "update" | "rebuild" | "upgrade"
    state: String, // "idle" | "running" | "done" | "error"
    phase: String,
    since: i64,
    ended: i64,
    summary: String,
    updated: Vec<String>,
    log: VecDeque<String>,
    external: bool,
}

impl JobState {
    fn to_json(&self) -> Value {
        if self.state.is_empty() || self.state == "idle" {
            return Value::Null;
        }
        json!({
            "kind": self.kind,
            "state": self.state,
            "phase": self.phase,
            "since": self.since,
            "ended": (self.ended > 0).then_some(self.ended),
            "summary": self.summary,
            "updated": self.updated,
            "log": self.log.iter().cloned().collect::<Vec<_>>(),
            "logFile": log_path().to_string_lossy(),
            "external": self.external,
        })
    }
}

#[derive(Clone, Debug, Default)]
struct LocalInfo {
    nixos: bool,
    declared: bool,
    flake: String,
    flake_name: String,
    rebuild: String,
    generation: Option<u32>,
    version: String,
    built_at: i64,
    reboot_needed: bool,
    dirty: usize,
    lock_modified: i64,
    lock_newer: bool,
    nixpkgs_age: i64,
    inputs_count: usize,
}

fn read_local() -> LocalInfo {
    let manifest = manifest().map(|(_, v)| v);
    let declared = manifest.is_some();
    let is_nixos = nixos();
    let flake = flake_dir(manifest.as_ref());
    let rebuild = default_rebuild(manifest.as_ref(), flake.as_deref());

    let profile = Path::new("/nix/var/nix/profiles/system");
    let generation = fs::read_link(profile).ok().and_then(|t| parse_generation_link(&t.to_string_lossy()));
    let built_at = mtime_ms(profile);
    let version = fs::read_link("/run/current-system").ok().and_then(|t| parse_system_version(&t.to_string_lossy())).unwrap_or_default();
    let reboot = if is_nixos { reboot_needed() } else { false };

    let (flake_str, flake_name, dirty, lock_modified, nixpkgs_age, inputs_count) = match &flake {
        Some(dir) => {
            let lock = dir.join("flake.lock");
            let lock_mod = mtime_ms(&lock);
            let (_, count, np_age) = fs::read_to_string(&lock).map(|t| parse_lock_inputs(&t)).unwrap_or((Vec::new(), 0, 0));
            (
                dir.to_string_lossy().to_string(),
                dir.file_name().and_then(|n| n.to_str()).unwrap_or("flake").to_string(),
                git_dirty(dir),
                lock_mod,
                np_age,
                count,
            )
        }
        None => (String::new(), String::new(), 0, 0, 0, 0),
    };
    let lock_newer = lock_modified > 0 && built_at > 0 && lock_modified > built_at;

    LocalInfo {
        nixos: is_nixos,
        declared,
        flake: flake_str,
        flake_name,
        rebuild,
        generation,
        version,
        built_at,
        reboot_needed: reboot,
        dirty,
        lock_modified,
        lock_newer,
        nixpkgs_age,
        inputs_count,
    }
}

struct Shared {
    local: LocalInfo,
    updates: UpdatesCache,
    checking: bool,
    job: JobState,
}

impl Shared {
    fn snapshot(&self, auth: &AuthShared) -> Value {
        json!({
            "nixos": self.local.nixos,
            "declared": self.local.declared,
            "flake": self.local.flake,
            "flakeName": self.local.flake_name,
            "rebuild": self.local.rebuild,
            "generation": self.local.generation,
            "version": self.local.version,
            "builtAt": (self.local.built_at > 0).then_some(self.local.built_at),
            "rebootNeeded": self.local.reboot_needed,
            "dirty": self.local.dirty,
            "lockModified": (self.local.lock_modified > 0).then_some(self.local.lock_modified),
            "lockNewer": self.local.lock_newer,
            "nixpkgsAge": (self.local.nixpkgs_age > 0).then_some(self.local.nixpkgs_age),
            "inputsCount": self.local.inputs_count,
            "checking": self.checking,
            "checkedAt": (self.updates.checked_at > 0).then_some(self.updates.checked_at),
            "updates": self.updates.updates,
            "checkError": self.updates.error,
            "job": self.job.to_json(),
            "auth": auth.snapshot(),
        })
    }
}

fn push_snapshot(shared: &Arc<Mutex<Shared>>, auth: &Arc<Mutex<AuthShared>>, tx: &Sender<Event>) {
    let snap = {
        let s = shared.lock().unwrap_or_else(|e| e.into_inner());
        let a = auth.lock().unwrap_or_else(|e| e.into_inner());
        s.snapshot(&a)
    };
    let _ = tx.send(Event::Nix(snap));
}

fn notify_desktop(summary: &str, body: &str, urgent: bool) {
    let mut cmd = Command::new("notify-send");
    cmd.args(["-a", "Veyl", "-i", "nix-snowflake"]);
    if urgent {
        cmd.args(["-u", "critical"]);
    }
    cmd.args([summary, body]);
    let _ = cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
}

/// Ejecuta un proceso leyendo stdout y stderr juntos (por líneas y `\r`),
/// con soporte para pararlo desde fuera y escribir el log completo a disco.
fn stream_command(
    command: &mut Command,
    timeout: Duration,
    stop: &AtomicBool,
    log_file: Option<&mut File>,
    mut on_line: impl FnMut(&str),
) -> Result<bool, String> {
    let mut child = command.process_group(0).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    let pid = child.id();
    let stdout = child.stdout.take().expect("stdout canalizado");
    let stderr = child.stderr.take().expect("stderr canalizado");

    let (tx, rx) = mpsc::channel::<String>();
    let spawn_reader = |mut pipe: Box<dyn Read + Send>, tx: Sender<String>| {
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            let mut acc = Vec::<u8>::new();
            while let Ok(n) = pipe.read(&mut buf) {
                if n == 0 {
                    break;
                }
                for &b in &buf[..n] {
                    if b == b'\n' || b == b'\r' {
                        if !acc.is_empty() {
                            let line = String::from_utf8_lossy(&acc).to_string();
                            acc.clear();
                            if tx.send(line).is_err() {
                                return;
                            }
                        }
                    } else if acc.len() < 4096 {
                        acc.push(b);
                    }
                }
            }
            if !acc.is_empty() {
                let _ = tx.send(String::from_utf8_lossy(&acc).to_string());
            }
        })
    };
    let t_out = spawn_reader(Box::new(stdout), tx.clone());
    let t_err = spawn_reader(Box::new(stderr), tx);

    let mut log_file = log_file;
    let start = Instant::now();
    let mut open = true;
    let outcome = loop {
        if stop.load(Ordering::Relaxed) {
            break Err("Detenido".to_string());
        }
        if start.elapsed() >= timeout {
            break Err("Tardó demasiado".to_string());
        }
        if open {
            match rx.recv_timeout(Duration::from_millis(40)) {
                Ok(raw) => {
                    let clean = strip_ansi(&raw);
                    if !clean.is_empty() {
                        if let Some(f) = log_file.as_deref_mut() {
                            let _ = writeln!(f, "{clean}");
                        }
                        on_line(&clean);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => open = false,
            }
            continue;
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status.success()),
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(e) => break Err(e.to_string()),
        }
    };
    if outcome.is_err() {
        // SAFETY: el hijo se lanzó en su propio grupo de procesos.
        unsafe {
            libc::kill(-(pid as i32), libc::SIGTERM);
        }
        thread::sleep(Duration::from_millis(150));
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
    let _ = child.wait();
    let _ = t_out.join();
    let _ = t_err.join();
    outcome
}

fn run_flake_update(
    flake: &str,
    input: Option<&str>,
    stop: &Arc<AtomicBool>,
    shared: &Arc<Mutex<Shared>>,
    auth: &Arc<Mutex<AuthShared>>,
    tx: &Sender<Event>,
    log_file: Option<&mut File>,
) -> Result<Vec<String>, String> {
    let mut cmd = Command::new("nix");
    cmd.arg("flake").arg("update");
    if let Some(name) = input.filter(|s| !s.trim().is_empty()) {
        cmd.arg(name.trim());
    }
    cmd.args(["--flake", flake]).env("NO_COLOR", "1");

    let mut updated = Vec::new();
    let mut last_push = Instant::now();
    let ok = stream_command(&mut cmd, JOB_TIMEOUT, stop, log_file, |line| {
        let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
        let mut changed = false;
        if let Some(name) = parse_updated_input(line)
            && !updated.contains(&name)
        {
            updated.push(name.clone());
            s.job.updated = updated.clone();
            s.job.phase = format!("Actualizado {name}");
            changed = true;
        } else if let Some(phase) = parse_phase("update", line)
            && s.job.phase != phase
        {
            s.job.phase = phase.to_string();
            changed = true;
        }
        if s.job.log.len() >= LOG_TAIL {
            s.job.log.pop_front();
        }
        s.job.log.push_back(line.chars().take(200).collect());
        drop(s);
        if changed || last_push.elapsed() >= Duration::from_millis(250) {
            last_push = Instant::now();
            push_snapshot(shared, auth, tx);
        }
    })?;
    if ok { Ok(updated) } else { Err("Falló nix flake update".into()) }
}

fn run_rebuild(
    rebuild_cmd: &str,
    flake: &str,
    stop: &Arc<AtomicBool>,
    shared: &Arc<Mutex<Shared>>,
    auth: &Arc<Mutex<AuthShared>>,
    tx: &Sender<Event>,
    log_file: Option<&mut File>,
) -> Result<(), String> {
    let bin_dir = prepare_elevation_dir()?;
    let token = random_token();
    {
        let mut a = auth.lock().unwrap_or_else(|e| e.into_inner());
        a.token = Some(token.clone());
        a.prompt = None;
        a.attempt = 0;
    }

    let current_path = std::env::var("PATH").unwrap_or_default();
    let path_env = format!("{}:{current_path}", bin_dir.display());
    let mut cmd = Command::new("sh");
    cmd.args(["-c", rebuild_cmd])
        .env("PATH", path_env)
        .env("SUDO_ASKPASS", bin_dir.join("askpass"))
        .env("NH_ELEVATION_STRATEGY", bin_dir.join("elevate"))
        .env("VEYL_NIX_AUTH_TOKEN", &token)
        .env("NO_COLOR", "1")
        .env("CLICOLOR", "0");
    if !flake.is_empty() {
        cmd.env("NH_FLAKE", flake);
    }

    let mut last_push = Instant::now();
    let result = stream_command(&mut cmd, JOB_TIMEOUT, stop, log_file, |line| {
        let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
        let mut changed = false;
        if let Some(phase) = parse_phase("rebuild", line)
            && s.job.phase != phase
        {
            s.job.phase = phase.to_string();
            changed = true;
        }
        if s.job.log.len() >= LOG_TAIL {
            s.job.log.pop_front();
        }
        s.job.log.push_back(line.chars().take(200).collect());
        drop(s);
        if changed || last_push.elapsed() >= Duration::from_millis(250) {
            last_push = Instant::now();
            push_snapshot(shared, auth, tx);
        }
    });

    {
        let mut a = auth.lock().unwrap_or_else(|e| e.into_inner());
        a.token = None;
        a.prompt = None;
        a.attempt = 0;
    }
    match result {
        Ok(true) => Ok(()),
        Ok(false) => Err("Falló la reconstrucción".into()),
        Err(why) => Err(why),
    }
}

/// Hilo de Nix en `veyld`: vigila el sistema y el flake, comprueba actualizaciones
/// periódicamente y ejecuta `flake update` / `rebuild` en segundo plano.
pub fn watch(jobs: Receiver<Job>, tx: Sender<Event>, auth: Arc<Mutex<AuthShared>>) {
    let local = read_local();
    let cached = (!local.flake.is_empty()).then(|| load_updates_cache(Path::new(&local.flake))).flatten().unwrap_or_default();
    let relevant = local.nixos || local.declared || !local.flake.is_empty();
    let shared = Arc::new(Mutex::new(Shared { local, updates: cached, checking: false, job: JobState::default() }));
    if relevant {
        push_snapshot(&shared, &auth, &tx);
    }

    let mut active_stop: Option<Arc<AtomicBool>> = None;
    let mut last_auto_check = Instant::now().checked_sub(AUTO_CHECK_EVERY - Duration::from_secs(12)).unwrap_or_else(Instant::now);

    let spawn_check = |shared: &Arc<Mutex<Shared>>, auth: &Arc<Mutex<AuthShared>>, tx: &Sender<Event>| {
        let flake = {
            let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
            if s.checking || s.local.flake.is_empty() {
                return;
            }
            s.checking = true;
            s.local.flake.clone()
        };
        push_snapshot(shared, auth, tx);
        let shared = Arc::clone(shared);
        let auth = Arc::clone(auth);
        let tx = tx.clone();
        thread::spawn(move || {
            let res = check_updates(Path::new(&flake));
            {
                let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
                s.checking = false;
                match res {
                    Ok(cache) => s.updates = cache,
                    Err(why) => s.updates.error = why,
                }
            }
            push_snapshot(&shared, &auth, &tx);
        });
    };

    let start_job = |kind: String,
                     input: Option<String>,
                     then_rebuild: bool,
                     active_stop: &mut Option<Arc<AtomicBool>>,
                     shared: &Arc<Mutex<Shared>>,
                     auth: &Arc<Mutex<AuthShared>>,
                     tx: &Sender<Event>| {
        let (flake, rebuild_cmd) = {
            let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
            if s.job.state == "running" && !s.job.external {
                return;
            }
            s.local = read_local();
            let flake = s.local.flake.clone();
            let rebuild = s.local.rebuild.clone();
            if (kind == "update" || kind == "upgrade") && flake.is_empty() {
                return;
            }
            if (kind == "rebuild" || kind == "upgrade") && rebuild.is_empty() {
                return;
            }
            s.job = JobState {
                kind: kind.clone(),
                state: "running".into(),
                phase: if kind == "rebuild" { "Preparando…".into() } else { "Actualizando flake…".into() },
                since: now(),
                ended: 0,
                summary: String::new(),
                updated: Vec::new(),
                log: VecDeque::new(),
                external: false,
            };
            (flake, rebuild)
        };
        let stop = Arc::new(AtomicBool::new(false));
        *active_stop = Some(Arc::clone(&stop));
        push_snapshot(shared, auth, tx);

        let shared = Arc::clone(shared);
        let auth = Arc::clone(auth);
        let tx = tx.clone();
        thread::spawn(move || {
            let path = log_path();
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let mut log_file = File::create(&path).ok();
            let outcome: Result<String, String> = (|| {
                let mut updated = Vec::new();
                if kind == "update" || kind == "upgrade" {
                    updated = run_flake_update(&flake, input.as_deref(), &stop, &shared, &auth, &tx, log_file.as_mut())?;
                    // Reconciliamos las actualizaciones enseguida.
                    {
                        let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
                        s.local = read_local();
                        reconcile_updates(Path::new(&flake), &mut s.updates);
                        save_updates_cache(&s.updates);
                    }
                    push_snapshot(&shared, &auth, &tx);
                }
                if kind == "rebuild" || then_rebuild {
                    {
                        let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
                        s.job.phase = "Evaluando…".into();
                    }
                    push_snapshot(&shared, &auth, &tx);
                    run_rebuild(&rebuild_cmd, &flake, &stop, &shared, &auth, &tx, log_file.as_mut())?;
                    let fresh = read_local();
                    let gen_txt = fresh.generation.map(|g| format!("Generación #{g}")).unwrap_or_else(|| "Sistema reconstruido".into());
                    Ok(if updated.is_empty() { gen_txt } else { format!("{} actualizadas · {gen_txt}", updated.len()) })
                } else if updated.is_empty() {
                    Ok("Flake al día".into())
                } else if updated.len() == 1 {
                    Ok(format!("Actualizada {}", updated[0]))
                } else {
                    Ok(format!("{} entradas actualizadas", updated.len()))
                }
            })();

            let stopped = stop.load(Ordering::Relaxed);
            let (summary, is_err) = {
                let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
                s.local = read_local();
                let flake_path = s.local.flake.clone();
                if !flake_path.is_empty() {
                    reconcile_updates(Path::new(&flake_path), &mut s.updates);
                    save_updates_cache(&s.updates);
                }
                s.job.ended = now();
                match outcome {
                    Ok(summary) => {
                        s.job.state = "done".into();
                        s.job.phase = "Completado".into();
                        s.job.summary = summary.clone();
                        (summary, false)
                    }
                    Err(_) if stopped => {
                        s.job.state = "idle".into();
                        s.job.phase.clear();
                        s.job.summary.clear();
                        (String::new(), false)
                    }
                    Err(why) => {
                        s.job.state = "error".into();
                        s.job.phase = "Error".into();
                        s.job.summary = why.clone();
                        (why, true)
                    }
                }
            };
            push_snapshot(&shared, &auth, &tx);
            if !summary.is_empty() {
                let title = if is_err {
                    if kind == "update" { "Error al actualizar flake" } else { "Error al reconstruir NixOS" }
                } else if kind == "update" {
                    "Flake actualizado"
                } else {
                    "NixOS reconstruido"
                };
                notify_desktop(title, &summary, is_err);
            }
        });
    };

    loop {
        match jobs.recv_timeout(LOCAL_POLL_EVERY) {
            Ok(Job::Refresh) => {
                let fresh = read_local();
                {
                    let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
                    s.local = fresh;
                    let flake_path = s.local.flake.clone();
                    if !flake_path.is_empty() {
                        reconcile_updates(Path::new(&flake_path), &mut s.updates);
                    }
                }
                push_snapshot(&shared, &auth, &tx);
            }
            Ok(Job::Check) => {
                last_auto_check = Instant::now();
                spawn_check(&shared, &auth, &tx);
            }
            Ok(Job::Update { input, then_rebuild }) => {
                let kind = if then_rebuild { "upgrade" } else { "update" }.to_string();
                start_job(kind, input, then_rebuild, &mut active_stop, &shared, &auth, &tx);
            }
            Ok(Job::Rebuild) => {
                start_job("rebuild".into(), None, false, &mut active_stop, &shared, &auth, &tx);
            }
            Ok(Job::Stop) => {
                if let Some(stop) = active_stop.take() {
                    stop.store(true, Ordering::Relaxed);
                }
            }
            Ok(Job::Dismiss) => {
                let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
                if s.job.state == "done" || s.job.state == "error" {
                    s.job = JobState::default();
                }
                drop(s);
                push_snapshot(&shared, &auth, &tx);
            }
            Err(RecvTimeoutError::Timeout) => {
                let fresh = read_local();
                let is_relevant = fresh.nixos || fresh.declared || !fresh.flake.is_empty();
                if !is_relevant {
                    continue;
                }
                let mut changed = false;
                let mut should_auto_check = false;
                {
                    let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
                    let prev_gen = s.local.generation;
                    let prev_dirty = s.local.dirty;
                    let prev_lock = s.local.lock_modified;
                    let prev_reboot = s.local.reboot_needed;
                    let prev_flake = s.local.flake.clone();
                    s.local = fresh;
                    let flake_path = s.local.flake.clone();
                    if !flake_path.is_empty() {
                        let before_u = s.updates.updates.len();
                        reconcile_updates(Path::new(&flake_path), &mut s.updates);
                        if s.updates.updates.len() != before_u {
                            save_updates_cache(&s.updates);
                            changed = true;
                        }
                    }
                    if s.local.generation != prev_gen
                        || s.local.dirty != prev_dirty
                        || s.local.lock_modified != prev_lock
                        || s.local.reboot_needed != prev_reboot
                        || s.local.flake != prev_flake
                    {
                        changed = true;
                    }
                    // Detecta rebuilds o flake update lanzados en una terminal aparte.
                    if s.job.state != "running" || s.job.external {
                        match detect_external() {
                            Some((kind, phase)) => {
                                if !s.job.external || s.job.kind != kind {
                                    s.job = JobState {
                                        kind,
                                        state: "running".into(),
                                        phase,
                                        since: now(),
                                        ended: 0,
                                        summary: String::new(),
                                        updated: Vec::new(),
                                        log: VecDeque::new(),
                                        external: true,
                                    };
                                    changed = true;
                                }
                            }
                            None if s.job.external => {
                                s.job = JobState::default();
                                changed = true;
                            }
                            None => {}
                        }
                    }
                    let cache_stale = s.updates.checked_at == 0 || (now() - s.updates.checked_at) >= AUTO_CHECK_EVERY.as_millis() as i64;
                    if !s.local.flake.is_empty() && !s.checking && cache_stale && last_auto_check.elapsed() >= AUTO_CHECK_EVERY {
                        should_auto_check = true;
                    }
                }
                if changed {
                    push_snapshot(&shared, &auth, &tx);
                }
                if should_auto_check {
                    last_auto_check = Instant::now();
                    spawn_check(&shared, &auth, &tx);
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(text: &str, path: &[&str], value: Value) -> String {
        let change = Change { path: path.iter().map(|s| s.to_string()).collect(), value: Some(value) };
        edit(text, &change, None).unwrap()
    }

    fn unset(text: &str, path: &[&str]) -> String {
        let change = Change { path: path.iter().map(|s| s.to_string()).collect(), value: None };
        edit(text, &change, None).unwrap()
    }

    #[test]
    fn cambia_un_valor_que_ya_esta() {
        let text = "{ ... }:\n{\n  programs.veyl = {\n    enable = true;\n    settings.scheme = \"living-code-night\";\n  };\n}\n";
        let out = set(text, &["scheme"], json!("living-code-frost"));
        assert!(out.contains("settings.scheme = \"living-code-frost\";"), "{out}");
        assert_eq!(out.replace("frost", "night"), text);
    }

    #[test]
    fn anade_dentro_del_bloque_mas_hondo() {
        let text = "{\n  programs.veyl = {\n    enable = true;\n    settings = {\n      scheme = \"a\";\n    };\n  };\n}\n";
        let out = set(text, &["voice", "speed"], json!(1.3));
        assert!(out.contains("      scheme = \"a\";\n      voice.speed = 1.3;\n"), "{out}");
    }

    #[test]
    fn anade_junto_a_lo_que_se_escribe_con_puntos() {
        let text = "{ config, ... }: {\n  programs.veyl.enable = true;\n}\n";
        let out = set(text, &["monitors", "HDMI-A-1", "scale"], json!(1.5));
        assert!(out.contains("  programs.veyl.enable = true;\n  programs.veyl.settings.monitors.HDMI-A-1.scale = 1.5;\n"), "{out}");
        rnix::Root::parse(&out).ok().unwrap();
    }

    #[test]
    fn encuentra_veyl_dentro_de_home_manager_y_mkif() {
        let text = "{ lib, ... }: {\n  home-manager.users.elliot = { pkgs, ... }: lib.mkIf true {\n    programs.veyl = {\n      enable = true;\n      settings = { };\n    };\n  };\n}\n";
        let out = set(text, &["quiet"], json!(true));
        assert!(out.contains("settings = {\n        quiet = true;\n      };"), "{out}");
    }

    #[test]
    fn quita_la_linea_entera() {
        let text = "{\n  programs.veyl.settings = {\n    scheme = \"a\";\n    voice.speed = 1.3;\n  };\n}\n";
        let out = unset(text, &["voice", "speed"]);
        assert_eq!(out, "{\n  programs.veyl.settings = {\n    scheme = \"a\";\n  };\n}\n");
    }

    #[test]
    fn no_pisa_una_expresion() {
        let text = "{ inputs, ... }: {\n  programs.veyl.settings.wallpaper = \"${inputs.wall}/a.png\";\n}\n";
        let change = Change { path: vec!["wallpaper".into()], value: Some(json!("/b.png")) };
        assert!(edit(text, &change, None).is_err());
    }

    #[test]
    fn escribe_objetos_y_nombres_raros() {
        let text = "{\n  programs.veyl.settings = { };\n}\n";
        let out = set(text, &["monitors"], json!({ "HDMI-A-1": { "scale": 1.5 }, "eDP 1": { "mode": "a${b}" } }));
        assert!(out.contains("\"eDP 1\" = {"), "{out}");
        assert!(out.contains("mode = \"a\\${b}\";"), "{out}");
        rnix::Root::parse(&out).ok().unwrap();
    }

    #[test]
    fn en_una_linea_sigue_en_la_linea() {
        let text = "{ programs.veyl = { enable = true; }; }";
        let out = set(text, &["quiet"], json!(false));
        assert_eq!(out, "{ programs.veyl = { enable = true; settings.quiet = false; }; }");
    }

    #[test]
    fn limpia_sufijo_de_flake_parts_y_encuentra_varios_archivos() {
        let dir = std::env::temp_dir().join(format!("veyl-nix-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("modules/desktop")).unwrap();
        fs::create_dir_all(dir.join("hosts/pc")).unwrap();
        let shared = dir.join("modules/desktop/veyl.nix");
        let host = dir.join("hosts/pc/default.nix");
        fs::write(&shared, "{ ... }: { flake.modules.homeManager.veyl = { programs.veyl.enable = true; }; }\n").unwrap();
        fs::write(&host, "{ ... }: { flake.modules.nixos.host-pc = { home-manager.users.t.programs.veyl.settings.monitors.\"HDMI-A-1\".scale = 1.5; }; }\n")
            .unwrap();
        let manifest = json!({
            "flake": dir.to_string_lossy(),
            "files": [
                "/nix/store/aaaa-source/modules/desktop/veyl.nix, via option flake.modules.homeManager.veyl",
                "/nix/store/aaaa-source/hosts/pc/default.nix, via option flake.modules.nixos.host-pc"
            ]
        });
        let targets = locate_all(&manifest).unwrap();
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].path, shared);
        assert_eq!(targets[0].scope.as_deref(), Some(&["flake".into(), "modules".into(), "homeManager".into(), "veyl".into()][..]));
        assert_eq!(targets[1].path, host);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn desambigua_bloques_de_flake_parts_y_elige_archivo_por_afinidad() {
        let shared = "{ ... }: {\n  flake.modules.nixos.veyl = {\n    programs.veyl.enable = true;\n  };\n  flake.modules.homeManager.veyl = {\n    programs.veyl = {\n      enable = true;\n      settings.scheme = \"a\";\n    };\n  };\n}\n";
        let scope = vec!["flake".into(), "modules".into(), "homeManager".into(), "veyl".into()];
        let out = edit(shared, &Change { path: vec!["quiet".into()], value: Some(json!(true)) }, Some(&scope)).unwrap();
        assert!(out.contains("settings.scheme = \"a\";\n      settings.quiet = true;"), "{out}");

        let host = "{ ... }: {\n  home-manager.users.elliot.programs.veyl.settings.monitors = {\n    \"HDMI-A-1\".scale = 1.5;\n  };\n}\n";
        let mon_change = Change { path: vec!["monitors".into(), "HDMI-A-2".into(), "scale".into()], value: Some(json!(1.0)) };
        let scheme_change = Change { path: vec!["scheme".into()], value: Some(json!("b")) };
        assert!(affinity(host, None, &mon_change) > affinity(shared, Some(&scope), &mon_change));
        assert!(affinity(shared, Some(&scope), &scheme_change) > affinity(host, None, &scheme_change));
    }

    #[test]
    fn lee_generacion_y_version_del_sistema() {
        assert_eq!(parse_generation_link("system-827-link"), Some(827));
        assert_eq!(parse_generation_link("/nix/var/nix/profiles/system-42-link"), Some(42));
        assert_eq!(parse_generation_link("system-abc-link"), None);
        assert_eq!(
            parse_system_version("/nix/store/igh3z6ipdzjzshhnsxkrajgaigi4v8df-nixos-system-nixos-pc-26.11.20261121.2786a2a"),
            Some("26.11.20261121".into())
        );
        assert_eq!(parse_system_version("/nix/store/abc-nixos-system-my-host-25.05.12345"), Some("25.05.12345".into()));
    }

    #[test]
    fn lee_entradas_flotantes_del_flake_lock() {
        let lock = r#"{
          "root": "root",
          "nodes": {
            "root": {
              "inputs": {
                "nixpkgs": "nixpkgs",
                "hyprland": "hyprland",
                "herdr": "herdr",
                "veyl": "veyl"
              }
            },
            "nixpkgs": {
              "original": { "type": "github", "owner": "nixos", "repo": "nixpkgs", "ref": "nixos-unstable" },
              "locked": { "rev": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", "lastModified": 1700000000 }
            },
            "hyprland": {
              "original": { "type": "github", "owner": "hyprwm", "repo": "Hyprland", "rev": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb" },
              "locked": { "rev": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", "lastModified": 1700000001 }
            },
            "herdr": {
              "original": { "type": "github", "owner": "herdrdev", "repo": "herdr", "ref": "v0.9.3" },
              "locked": { "rev": "cccccccccccccccccccccccccccccccccccccccc", "lastModified": 1700000002 }
            },
            "veyl": {
              "original": { "type": "github", "owner": "ElliotLuque", "repo": "veyl" },
              "locked": { "rev": "dddddddddddddddddddddddddddddddddddddddd", "lastModified": 1700000003 }
            }
          }
        }"#;
        let (inputs, total, nixpkgs_age) = parse_lock_inputs(lock);
        assert_eq!(total, 4);
        assert_eq!(nixpkgs_age, 1_700_000_000_000);
        // hyprland está fijado por `rev`, no flota.
        assert!(!inputs.iter().any(|i| i.name == "hyprland"));
        let np = inputs.iter().find(|i| i.name == "nixpkgs").unwrap();
        assert_eq!(np.url, "https://github.com/nixos/nixpkgs.git");
        assert_eq!(np.refspec, "refs/heads/nixos-unstable");
        let veyl = inputs.iter().find(|i| i.name == "veyl").unwrap();
        assert_eq!(veyl.refspec, "HEAD");
        // herdr consulta refs/heads/v0.9.3, que no coincide con etiquetas fijas.
        let herdr = inputs.iter().find(|i| i.name == "herdr").unwrap();
        assert_eq!(herdr.refspec, "refs/heads/v0.9.3");
    }

    #[test]
    fn limpia_ansi_y_reconoce_fases_de_nix() {
        assert_eq!(strip_ansi("\x1b[32m• Updated input 'nixpkgs':\x1b[0m"), "• Updated input 'nixpkgs':");
        assert_eq!(strip_ansi("viejo\rnuevo paso"), "nuevo paso");
        assert_eq!(parse_updated_input("• Updated input 'home-manager':"), Some("home-manager".into()));
        assert_eq!(parse_phase("rebuild", "evaluating derivation"), Some("Evaluando…"));
        assert_eq!(parse_phase("rebuild", "copying path '/nix/store/foo'"), Some("Descargando…"));
        assert_eq!(parse_phase("rebuild", "building '/nix/store/bar.drv'"), Some("Compilando…"));
        assert_eq!(parse_phase("rebuild", "Comparing changes"), Some("Comparando cambios…"));
        assert_eq!(parse_phase("rebuild", "activating the configuration..."), Some("Activando…"));
    }
}
