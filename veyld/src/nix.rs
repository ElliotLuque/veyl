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

use std::fs;
use std::path::{Path, PathBuf};

use rnix::ast::{self, HasEntry};
use rnix::{TextRange, TextSize};
use rowan::ast::AstNode;
use serde_json::{Map, Value, json};

use crate::config;

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
    for dir in [".config/home-manager", ".config/nixos", ".config/nix-config"] {
        push(&config::home().join(dir).to_string_lossy());
    }
    roots
}

/// El archivo de tu configuración donde vive `programs.veyl`: el fijado a
/// mano o, de los que da el sistema de módulos, el primero que se encuentre
/// fuera del almacén (con un flake, el mismo camino dentro de su carpeta).
pub fn locate(manifest: &Value) -> Result<PathBuf, String> {
    if let Some(file) = manifest.get("file").and_then(Value::as_str).filter(|f| !f.is_empty()) {
        let path = PathBuf::from(file);
        return if path.is_file() { Ok(path) } else { Err(format!("no existe {}", config::tilde(file))) };
    }
    let files: Vec<&str> = manifest.get("files").and_then(Value::as_array).map(|files| files.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    if files.is_empty() {
        return Err("el módulo no sabe en qué archivo está programs.veyl: fíjalo con programs.veyl.nix.file".into());
    }
    let roots = roots(manifest);
    for file in &files {
        let Some(rest) = file.strip_prefix("/nix/store/") else {
            if Path::new(file).is_file() {
                return Ok(PathBuf::from(file));
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
                return Ok(path);
            }
        }
    }
    Err("no encuentro la carpeta de tu flake: dila en programs.veyl.nix.flake (o en $NH_FLAKE)".into())
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
    match locate(&manifest) {
        Ok(file) => status["file"] = json!(file),
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

/// Escribe los cambios en el archivo de tu configuración. Devuelve cuál.
pub fn write(changes: &[Change]) -> Result<PathBuf, String> {
    let (_, manifest) = manifest().ok_or("Veyl no está en tu configuración de Nix")?;
    let file = locate(&manifest)?;
    let mut text = fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
    for change in changes {
        text = edit(&text, change).map_err(|why| format!("{}: {why}", config::tilde(&file.to_string_lossy())))?;
    }
    // Junto al original y luego encima: un corte no deja el archivo a medias.
    let temporary = file.with_extension("nix.veyl~");
    fs::write(&temporary, &text).and_then(|()| fs::rename(&temporary, &file)).map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(file)
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
pub fn edit(text: &str, change: &Change) -> Result<String, String> {
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
    let ours: Vec<(&Binding, &[String])> = bindings.iter().filter_map(|b| under_veyl(&b.full).map(|rel| (b, rel))).collect();
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

/// `veyld nix …`: lo que pide Veyl desde los ajustes.
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
            let file = write(&changes)?;
            println!("{}", json!({ "file": file, "written": changes.len() }));
            Ok(())
        }
        // `veyld nix show '{"scheme": "…"}'`: cómo se escribiría en una configuración nueva.
        Some("show") => {
            let settings: Value = serde_json::from_str(args.get(1).map_or("{}", String::as_str)).map_err(|e| e.to_string())?;
            print!("{}", snippet(settings.as_object().ok_or("se esperaba un objeto")?));
            Ok(())
        }
        Some(other) => Err(format!("no sé qué es «{other}»: status, write o show")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(text: &str, path: &[&str], value: Value) -> String {
        let change = Change { path: path.iter().map(|s| s.to_string()).collect(), value: Some(value) };
        edit(text, &change).unwrap()
    }

    fn unset(text: &str, path: &[&str]) -> String {
        let change = Change { path: path.iter().map(|s| s.to_string()).collect(), value: None };
        edit(text, &change).unwrap()
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
        assert!(edit(text, &change).is_err());
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
}
