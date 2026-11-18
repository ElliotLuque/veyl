//! Las vistas: escritorios con nombre hechos con lo que ya tienes abierto
//! («estudio de uni»). Como una vista de una base de datos, se guarda la
//! consulta y no el resultado: de cada ventana, su aplicación y las palabras
//! que la hacen suya (de su título, su carpeta o su página). Crearla junta
//! esas ventanas en un nivel propio, que nace al vuelo con su nombre;
//! recrearla vuelve a juntar las que casan y abre otra vez las que faltan.
//!
//! Qué ventanas van lo decide el cerebro, si lo hay, mirando títulos,
//! carpetas y páginas; si no, las que nombran alguna palabra de la vista o,
//! si ninguna, las del nivel en el que estás. Se guardan en
//! `$XDG_STATE_HOME/veyl/views.json`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::apps::{self, App};
use crate::config;
use crate::hypr::Window;
use crate::web;
use crate::words::{fold, tokens};

/// Lo que hace suya a una ventana: su aplicación y, si hay, alguna de sus palabras.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Member {
    pub class: String,
    /// El título de cuando se guardó: para enseñarlo.
    #[serde(default)]
    pub title: String,
    /// Sin tildes ni mayúsculas. Vacío: cualquier ventana de su aplicación.
    #[serde(default)]
    pub keys: Vec<String>,
    /// En una terminal, la carpeta; en un navegador, la página. Para reabrirla.
    #[serde(default)]
    pub folder: String,
    #[serde(default)]
    pub url: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct View {
    pub name: String,
    pub members: Vec<Member>,
    #[serde(default)]
    pub created: i64,
    #[serde(default)]
    pub used: i64,
    /// De qué espacio vino cada ventana (dirección → espacio), para devolverlas al deshacerla.
    #[serde(default)]
    pub origin: HashMap<String, i64>,
}

impl View {
    /// El espacio de Hyprland que la lleva: uno con su nombre.
    pub fn workspace(&self) -> String {
        format!("name:{}", self.name)
    }
}

pub struct Views {
    path: PathBuf,
    pub list: Vec<View>,
}

impl Views {
    pub fn load() -> Views {
        Views::at(config::state().join("views.json"))
    }

    fn at(path: PathBuf) -> Views {
        let list = fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        Views { path, list }
    }

    pub fn save(&self) -> Result<(), String> {
        if self.path == Path::new("") {
            return Ok(());
        }
        let text = serde_json::to_string_pretty(&self.list).map_err(|e| e.to_string())?;
        let tmp = self.path.with_extension("json.tmp");
        if let Some(dir) = self.path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        fs::write(&tmp, text).and_then(|_| fs::rename(&tmp, &self.path)).map_err(|e| format!("No puedo guardar las vistas: {e}"))
    }

    /// La vista que se llama así o, si ninguna, la que más se le parece.
    pub fn find(&self, name: &str) -> Option<usize> {
        let q = fold(name.trim());
        if q.is_empty() {
            return None;
        }
        if let Some(i) = self.list.iter().position(|v| fold(&v.name) == q) {
            return Some(i);
        }
        let mut best: Option<(f32, usize)> = None;
        for (i, view) in self.list.iter().enumerate() {
            let score = apps::likeness(&q, &view.name);
            if score >= 60.0 && best.is_none_or(|(top, _)| score > top) {
                best = Some((score, i));
            }
        }
        best.map(|(_, i)| i)
    }

    /// La vista cuyo espacio es este.
    pub fn of_workspace(&self, workspace: &str) -> Option<usize> {
        self.list.iter().position(|v| v.name == workspace)
    }

    /// Guarda una vista; si ya había una con ese nombre, la sustituye.
    pub fn put(&mut self, view: View) {
        match self.list.iter().position(|v| fold(&v.name) == fold(&view.name)) {
            Some(i) => self.list[i] = view,
            None => self.list.push(view),
        }
    }
}

/// Un nombre de vista como se enseña: sin comillas ni espacios de más, y con mayúscula.
pub fn tidy(name: &str) -> String {
    let name: String = name.trim().trim_matches(|c: char| "«»\"'“”.".contains(c)).split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Una ventana abierta, con lo que se sabe de ella para decidir.
#[derive(Clone, Debug, Default)]
pub struct Candidate {
    pub window: Window,
    /// Cómo se llama su aplicación: «Firefox».
    pub app: String,
    pub folder: String,
    pub command: String,
    pub url: String,
    pub level: i64,
}

impl Candidate {
    /// Todo lo que se sabe de ella, en un texto donde buscar palabras.
    fn hay(&self) -> String {
        fold(&[self.window.title.as_str(), &self.folder, &self.url, &self.command, &self.app, &self.window.class].join(" "))
    }
}

/// La aplicación de una clase, en corto: «org.mozilla.firefox» es «firefox».
fn short(class: &str) -> String {
    fold(class.rsplit('.').next().unwrap_or(class))
}

const STOP: &[&str] = &["del", "las", "los", "una", "uno", "para", "con", "mis", "por", "que", "the", "and", "vista"];

/// Las palabras que cuentan de un nombre: de tres letras o más y sin las de relleno.
fn words(name: &str) -> Vec<String> {
    tokens(name).into_iter().filter(|w| w.chars().count() >= 3 && !STOP.contains(&w.as_str())).collect()
}

/// Lo que la hace suya cuando no se sabe más: el último trozo de su carpeta
/// o el sitio de su página. Si no, nada: cualquiera de su aplicación.
fn keys_of(c: &Candidate) -> Vec<String> {
    if !c.folder.is_empty() {
        let last = c.folder.trim_end_matches('/').rsplit('/').next().unwrap_or("");
        if !last.is_empty() && fold(&c.folder) != fold(&config::home().display().to_string()) {
            return vec![fold(last)];
        }
    }
    let site = web::site(&c.url);
    if !site.is_empty() {
        return vec![fold(&site)];
    }
    Vec::new()
}

/// Lo elegido: qué candidatas van y, de cada una, sus palabras.
pub type Choice = Vec<(usize, Vec<String>)>;

/// Sin modelo: las ventanas que nombran alguna palabra del nombre; si
/// ninguna, las del espacio en el que estás (`here`).
pub fn guess(name: &str, candidates: &[Candidate], here: i64) -> Choice {
    let words = words(name);
    let hits: Choice = candidates
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let hay = c.hay();
            let keys: Vec<String> = words.iter().filter(|w| hay.contains(w.as_str())).cloned().collect();
            (!keys.is_empty()).then_some((i, keys))
        })
        .collect();
    if !hits.is_empty() {
        return hits;
    }
    level(candidates, here)
}

/// Todas las del espacio `here`, con sus palabras propias.
pub fn level(candidates: &[Candidate], here: i64) -> Choice {
    candidates.iter().enumerate().filter(|(_, c)| c.window.workspace_id == here).map(|(i, c)| (i, keys_of(c))).collect()
}

/// Cómo se le pide al cerebro que elija.
pub const SYSTEM: &str = "Organizas el escritorio de una persona en «vistas»: escritorios con nombre que juntan las ventanas de una misma \
    tarea. Te dan el nombre de una vista nueva y las ventanas abiertas (aplicación, título, carpeta de la terminal, orden en marcha, página \
    del navegador, nivel). Elige las que pertenecen a esa tarea por lo que muestran, no solo por la aplicación: un navegador con el campus \
    virtual o apuntes sí va en «estudio de uni»; uno con YouTube o redes, no, salvo que el nombre lo pida. Si no hay ninguna clara, no elijas \
    ninguna. De cada elegida da de una a tres «claves»: palabras o trozos cortos, en minúsculas, que aparecen en su título, carpeta o página y \
    que la reconocerán más tarde aunque el título cambie un poco (el sitio, el proyecto, la asignatura). Contesta solo con JSON, sin nada más: \
    {\"ventanas\": [{\"i\": 0, \"claves\": [\"moodle\"]}]}";

/// La pregunta: el nombre y las ventanas.
pub fn prompt(name: &str, candidates: &[Candidate]) -> String {
    let windows: Vec<Value> = candidates
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let mut w = json!({ "i": i, "app": c.app, "titulo": c.window.title, "nivel": c.level });
            for (key, value) in [("carpeta", &c.folder), ("orden", &c.command), ("pagina", &c.url)] {
                if !value.is_empty() {
                    w[key] = json!(value);
                }
            }
            w
        })
        .collect();
    format!("Vista nueva: «{name}»\n\nVentanas abiertas:\n{}", Value::Array(windows))
}

/// Lo que contestó el cerebro, como elección. Las claves que no aparecen en
/// la ventana se quitan (no la reconocerían); si no queda ninguna, las suyas.
pub fn read(answer: &str, candidates: &[Candidate]) -> Option<Choice> {
    let reply = crate::brain::json_in(answer)?;
    let mut choice: Choice = Vec::new();
    for w in reply.get("ventanas")?.as_array()? {
        let Some(i) = w.get("i").and_then(Value::as_u64).map(|i| i as usize).filter(|i| *i < candidates.len()) else {
            continue;
        };
        if choice.iter().any(|(j, _)| *j == i) {
            continue;
        }
        let hay = candidates[i].hay();
        let mut keys: Vec<String> = w
            .get("claves")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(|k| fold(k.trim()))
            .filter(|k| k.chars().count() >= 3 && hay.contains(k.as_str()))
            .take(3)
            .collect();
        if keys.is_empty() {
            keys = keys_of(&candidates[i]);
        }
        choice.push((i, keys));
    }
    Some(choice)
}

/// Lo que se guarda de una ventana elegida.
pub fn member(c: &Candidate, keys: Vec<String>) -> Member {
    Member { class: c.window.class.clone(), title: c.window.title.clone(), keys, folder: c.folder.clone(), url: c.url.clone() }
}

/// Si la ventana es la de ese miembro: su aplicación y, si tiene, alguna de sus palabras.
pub fn owns(m: &Member, c: &Candidate) -> bool {
    short(&m.class) == short(&c.window.class)
        && (m.keys.is_empty() || {
            let hay = c.hay();
            m.keys.iter().any(|k| hay.contains(k.as_str()))
        })
}

/// Las ventanas de ahora que son de la vista: para cada miembro, la que
/// mejor casa de las que no se ha quedado otro (antes las que ya están en
/// su espacio, luego las de foco más reciente). Devuelve las parejas
/// (miembro, candidata) y los miembros que no tienen ventana.
pub fn gather(view: &View, candidates: &[Candidate]) -> (Vec<(usize, usize)>, Vec<usize>) {
    let mut order: Vec<usize> = (0..view.members.len()).collect();
    // Los que tienen palabras, primero: son los más exigentes.
    order.sort_by_key(|&m| view.members[m].keys.is_empty());
    let mut taken: Vec<usize> = Vec::new();
    let mut pairs = Vec::new();
    let mut missing = Vec::new();
    for m in order {
        let member = &view.members[m];
        let best = candidates
            .iter()
            .enumerate()
            .filter(|(i, c)| !taken.contains(i) && owns(member, c))
            .min_by_key(|(_, c)| (c.window.workspace != view.name, c.window.recency))
            .map(|(i, _)| i);
        match best {
            Some(i) => {
                taken.push(i);
                pairs.push((m, i));
            }
            None => missing.push(m),
        }
    }
    pairs.sort();
    missing.sort();
    (pairs, missing)
}

/// Entre comillas simples, para la shell.
fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// La orden que vuelve a abrir un miembro que falta: la terminal en su
/// carpeta, el navegador en su página o, si no, su aplicación.
pub fn relaunch(m: &Member, apps: &[App], terminal: &str) -> Option<String> {
    let program = terminal.split_whitespace().next().unwrap_or("kitty");
    let program = program.rsplit('/').next().unwrap_or(program);
    if short(&m.class) == fold(program) {
        return Some(if !m.folder.is_empty() && Path::new(&m.folder).is_dir() {
            format!("cd {} && exec {terminal}", quoted(&m.folder))
        } else {
            terminal.to_string()
        });
    }
    let app = apps.iter().find(|a| a.owns(&m.class))?;
    let command = app.command(terminal);
    if !m.url.is_empty() && web::browser(&m.class).is_some() {
        return Some(format!("{command} --new-window {}", quoted(&m.url)));
    }
    Some(command)
}

/// Las vistas, para Veyl: nombre, si está abierta (su espacio existe y
/// cuántas ventanas tiene) y lo que lleva.
pub fn listing(views: &Views, open: &HashMap<String, i64>, apps: &[App]) -> Value {
    let mut list: Vec<&View> = views.list.iter().collect();
    list.sort_by_key(|v| std::cmp::Reverse(v.used.max(v.created)));
    Value::Array(
        list.into_iter()
            .map(|v| {
                let names: Vec<String> = v.members.iter().map(|m| apps::label(apps, &m.class)).collect();
                let mut seen: Vec<String> = Vec::new();
                for name in names {
                    if !seen.contains(&name) {
                        seen.push(name);
                    }
                }
                json!({ "name": v.name, "members": v.members.len(), "apps": seen, "open": open.get(&v.name).copied(), "used": v.used.max(v.created) })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(n: usize, class: &str, title: &str, workspace: i64) -> Candidate {
        Candidate {
            window: Window {
                address: format!("0x{n}"),
                class: class.into(),
                title: title.into(),
                workspace: workspace.to_string(),
                workspace_id: workspace,
                recency: n as i64,
                ..Default::default()
            },
            app: class.into(),
            level: workspace,
            ..Default::default()
        }
    }

    #[test]
    fn names_are_tidied() {
        assert_eq!(tidy(" «estudio   de uni». "), "Estudio de uni");
        assert_eq!(words("Estudio de uni"), vec!["estudio", "uni"]);
    }

    #[test]
    fn guess_by_words_or_level() {
        let mut moodle = window(0, "firefox", "Cálculo II — Campus Uni — Mozilla Firefox", 1);
        moodle.url = "https://campus.uni.es/course/42".into();
        let all = vec![moodle, window(1, "kitty", "nvim apuntes.md", 2), window(2, "spotify", "Spotify", 2)];
        let picked = guess("estudio de uni", &all, 2);
        assert_eq!(picked, vec![(0, vec!["uni".to_string()])]);
        let fallback = guess("cosas", &all, 2);
        assert_eq!(fallback.iter().map(|(i, _)| *i).collect::<Vec<_>>(), vec![1, 2]);
    }

    #[test]
    fn model_keys_must_appear() {
        let all = vec![window(0, "firefox", "Moodle: Álgebra — Mozilla Firefox", 1), window(1, "kitty", "~", 1)];
        let answer = "Claro:\n```json\n{\"ventanas\": [{\"i\": 0, \"claves\": [\"moodle\", \"inventada\", \"Álgebra\"]}, {\"i\": 7, \"claves\": []}]}\n```";
        assert_eq!(read(answer, &all), Some(vec![(0, vec!["moodle".to_string(), "algebra".to_string()])]));
    }

    #[test]
    fn gather_matches_and_finds_what_is_missing() {
        let view = View {
            name: "Uni".into(),
            members: vec![
                Member { class: "firefox".into(), keys: vec!["moodle".into()], ..Default::default() },
                Member { class: "kitty".into(), ..Default::default() },
                Member { class: "obsidian".into(), ..Default::default() },
            ],
            ..Default::default()
        };
        let all = vec![window(0, "firefox", "YouTube", 1), window(1, "org.mozilla.firefox", "Moodle — Firefox", 3), window(2, "kitty", "fish", 1)];
        let (pairs, missing) = gather(&view, &all);
        assert_eq!(pairs, vec![(0, 1), (1, 2)]);
        assert_eq!(missing, vec![2]);
    }

    #[test]
    fn terminals_reopen_in_their_folder() {
        let m = Member { class: "kitty".into(), folder: "/tmp".into(), ..Default::default() };
        assert_eq!(relaunch(&m, &[], "kitty").as_deref(), Some("cd '/tmp' && exec kitty"));
        let gone = Member { class: "kitty".into(), folder: "/no/existe".into(), ..Default::default() };
        assert_eq!(relaunch(&gone, &[], "kitty").as_deref(), Some("kitty"));
        assert_eq!(quoted("it's"), r"'it'\''s'");
    }
}
