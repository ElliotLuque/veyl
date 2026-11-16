//! La gramática: lo que Veyl entiende sin preguntar a un modelo.
//!
//! Una gramática es un archivo de reglas, una por línea:
//!
//! ```text
//! # comentario
//! alias sube = subir | subas | súbelo        las formas de la derecha se leen como la de la izquierda
//! filler por favor | oye | puedes            se quitan antes de leer
//! [pon|sube] [el] volumen (a|al) <n:pct>  =>  volume.set
//! (enciende|activa) [el] wifi             =>  radio device=wifi on=true
//! ```
//!
//! En un patrón, `(a|b c)` es una de las dos, `[a|b]` lo mismo pero
//! opcional, y `<nombre:tipo>` un hueco que se rellena con lo dicho: `num`
//! (un número), `pct` (un porcentaje), `ord` (un ordinal), `dur` (una
//! duración, en segundos), `when` (cuándo), `word` (una palabra) o `text`
//! (lo que sea). Detrás de `=>`, la intención y sus valores fijos.
//!
//! Lo dicho se pasa a minúsculas y sin tildes, igual que los patrones. De
//! todas las reglas que casan con lo dicho entero gana la que más palabras
//! nombra (cada palabra vale 2, cada hueco con tipo 1 y uno de texto 0); a
//! igualdad, la primera.

use std::collections::HashMap;

use serde::Serialize;
use serde_json::{Map, Value};

use crate::when;
use crate::words::{self, Token};

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Num,
    Pct,
    Ord,
    Dur,
    When,
    Word,
    Text,
}

impl Kind {
    fn named(name: &str) -> Option<Kind> {
        Some(match name {
            "num" => Kind::Num,
            "pct" => Kind::Pct,
            "ord" => Kind::Ord,
            "dur" => Kind::Dur,
            "when" => Kind::When,
            "word" => Kind::Word,
            "text" => Kind::Text,
            _ => return None,
        })
    }

    fn read(self, tokens: &[Token]) -> Option<Value> {
        let words: Vec<String> = tokens.iter().map(|t| t.key.clone()).collect();
        let words = &words[..];
        // Lo libre, como se dijo; lo demás, como se lee.
        let joined = || Value::String(words.join(" "));
        let said = || Value::String(tokens.iter().map(|t| t.said.as_str()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" "));
        match self {
            Kind::Num => words::number(words).map(Value::from),
            Kind::Pct => words::percent(words).filter(|&n| n <= 150).map(Value::from),
            Kind::Ord => words::ordinal(words).filter(|&n| (1..=50).contains(&n)).map(Value::from),
            Kind::Dur => words::duration(words).map(Value::from),
            Kind::When => when::parse(words).map(|_| joined()),
            Kind::Word => (words.len() == 1).then(said),
            Kind::Text => Some(said()),
        }
    }

    fn weight(self) -> i32 {
        if self == Kind::Text { 0 } else { 1 }
    }
}

#[derive(Debug)]
enum Node {
    Word(String),
    Choice(Vec<Vec<Node>>),
    Maybe(Vec<Vec<Node>>),
    Slot(String, Kind),
}

#[derive(Debug)]
struct Rule {
    pattern: Vec<Node>,
    intent: String,
    fixed: Vec<(String, Value)>,
}

/// Lo que se ha rellenado de cada hueco.
type Captures<'r> = Vec<(&'r str, Value)>;

/// Lo que se entendió: la intención y sus valores.
#[derive(Clone, Debug, Serialize)]
pub struct Parse {
    pub intent: String,
    pub args: Map<String, Value>,
    /// Lo dicho, tal como se leyó (sin muletillas y con los alias puestos).
    pub heard: String,
}

impl Parse {
    pub fn text(&self, key: &str) -> &str {
        self.args.get(key).and_then(Value::as_str).unwrap_or("")
    }

    pub fn number(&self, key: &str) -> Option<i64> {
        self.args.get(key).and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()))
    }

    pub fn flag(&self, key: &str) -> Option<bool> {
        self.args.get(key).and_then(Value::as_bool)
    }
}

#[derive(Debug, Default)]
pub struct Grammar {
    rules: Vec<Rule>,
    /// Frase → la palabra por la que se lee.
    aliases: HashMap<Vec<String>, Vec<String>>,
    fillers: Vec<Vec<String>>,
    /// Lo que se quita solo al principio («dime», «¿sabes…?», «perdona») y
    /// solo al final («si puedes»): en medio de la frase puede ser lo dicho.
    leads: Vec<Vec<String>>,
    tails: Vec<Vec<String>>,
    longest: usize,
}

/// La gramática de fábrica.
pub const SPANISH: &str = include_str!("../grammar/es.txt");

impl Grammar {
    /// Lee una gramática; los errores dicen en qué línea.
    pub fn load(&mut self, source: &str, origin: &str) -> Result<(), String> {
        for (number, line) in source.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let fail = |why: String| format!("{origin}:{}: {why}", number + 1);
            if let Some(rest) = line.strip_prefix("alias ") {
                let (canon, forms) = rest.split_once('=').ok_or_else(|| fail("falta «=» en el alias".into()))?;
                let canon = words::tokens(canon);
                for form in forms.split('|') {
                    let form = words::tokens(form);
                    if !form.is_empty() {
                        self.longest = self.longest.max(form.len());
                        self.aliases.insert(form, canon.clone());
                    }
                }
            } else if let Some((kind, rest)) = ["filler ", "lead ", "tail "].iter().find_map(|kind| line.strip_prefix(kind).map(|rest| (kind.trim(), rest))) {
                let list = match kind {
                    "filler" => &mut self.fillers,
                    "lead" => &mut self.leads,
                    _ => &mut self.tails,
                };
                for phrase in rest.split('|') {
                    let phrase = words::tokens(phrase);
                    if !phrase.is_empty() {
                        list.push(phrase);
                    }
                }
            } else {
                let (pattern, intent) = line.split_once("=>").ok_or_else(|| fail("falta «=>»".into()))?;
                let mut target = intent.split_whitespace();
                let intent = target.next().ok_or_else(|| fail("falta la intención".into()))?.to_string();
                let mut fixed = Vec::new();
                for pair in target {
                    let (key, value) = pair.split_once('=').ok_or_else(|| fail(format!("«{pair}» no es clave=valor")))?;
                    let value = match value {
                        "true" => Value::Bool(true),
                        "false" => Value::Bool(false),
                        v => v.parse::<i64>().map(Value::from).unwrap_or_else(|_| Value::String(v.to_string())),
                    };
                    fixed.push((key.to_string(), value));
                }
                let pattern = compile(pattern).map_err(fail)?;
                self.rules.push(Rule { pattern, intent, fixed });
            }
        }
        // Las muletillas más largas primero: «me puedes» antes que «puedes».
        for list in [&mut self.fillers, &mut self.leads, &mut self.tails] {
            list.sort_by_key(|f| std::cmp::Reverse(f.len()));
        }
        Ok(())
    }

    pub fn rules(&self) -> usize {
        self.rules.len()
    }

    /// Lo dicho, listo para leer: sin muletillas y con cada forma en su alias.
    /// Quita lo que envuelve la pregunta u orden: «dime qué hora es», «¿sabes
    /// qué día es?», «… si puedes». Nunca todo.
    fn unwrap(&self, said: &mut Vec<Token>) {
        let starts = |said: &[Token], phrase: &[String]| said.len() > phrase.len() && said.iter().zip(phrase).all(|(t, w)| &t.key == w);
        let ends =
            |said: &[Token], phrase: &[String]| said.len() > phrase.len() && said[said.len() - phrase.len()..].iter().zip(phrase).all(|(t, w)| &t.key == w);
        while let Some(lead) = self.leads.iter().find(|l| starts(said, l)) {
            said.drain(..lead.len());
        }
        while let Some(tail) = self.tails.iter().find(|t| ends(said, t)) {
            said.truncate(said.len() - tail.len());
        }
    }

    fn prepare(&self, text: &str) -> Vec<Token> {
        let mut said = words::split(text);
        // Primero lo que envuelve («… si puedes»), antes de que las muletillas lo partan.
        self.unwrap(&mut said);
        let mut kept = Vec::with_capacity(said.len());
        let mut at = 0;
        'outer: while at < said.len() {
            for filler in &self.fillers {
                if said.len() - at >= filler.len() && said[at..at + filler.len()].iter().zip(filler).all(|(t, f)| &t.key == f) {
                    at += filler.len();
                    continue 'outer;
                }
            }
            kept.push(said[at].clone());
            at += 1;
        }
        // Solo muletillas («venga»): entonces cuentan.
        if kept.is_empty() {
            kept = said;
        }
        // Y otra vez lo que envuelve, por si lo tapaban muletillas («oye veyl, dime…»).
        self.unwrap(&mut kept);
        let mut out = Vec::with_capacity(kept.len());
        let mut at = 0;
        while at < kept.len() {
            let found = (1..=self.longest.min(kept.len() - at)).rev().find_map(|len| {
                let keys: Vec<String> = kept[at..at + len].iter().map(|t| t.key.clone()).collect();
                self.aliases.get(&keys).map(|canon| (canon, len))
            });
            match found {
                Some((canon, len)) => {
                    // La primera pieza del alias lleva cómo se dijo la frase entera.
                    let phrase = kept[at..at + len].iter().map(|t| t.said.as_str()).collect::<Vec<_>>().join(" ");
                    for (i, key) in canon.iter().enumerate() {
                        out.push(Token { key: key.clone(), said: if i == 0 { phrase.clone() } else { String::new() } });
                    }
                    at += len;
                }
                None => {
                    out.push(kept[at].clone());
                    at += 1;
                }
            }
        }
        out
    }

    /// La regla que mejor casa con lo dicho, si alguna casa.
    pub fn parse(&self, text: &str) -> Option<Parse> {
        // Las órdenes son frases cortas. Limita el trabajo combinatorio de
        // los huecos libres antes de buscar en todas las reglas.
        if text.len() > 4096 {
            return None;
        }
        let said = self.prepare(text);
        if said.is_empty() || said.len() > 64 {
            return None;
        }
        let mut best: Option<(i32, &Rule, Captures)> = None;
        for rule in &self.rules {
            let mut found = None;
            walk(&mut vec![&rule.pattern[..]], &said, 0, &mut Vec::new(), 0, &mut found);
            if let Some((score, caps)) = found
                && best.as_ref().is_none_or(|(top, ..)| score > *top)
            {
                best = Some((score, rule, caps));
            }
        }
        let (_, rule, caps) = best?;
        let mut args = Map::new();
        for (key, value) in &rule.fixed {
            args.insert(key.clone(), value.clone());
        }
        for (key, value) in caps {
            args.insert(key.to_string(), value);
        }
        let heard = said.iter().map(|t| t.key.as_str()).collect::<Vec<_>>().join(" ");
        Some(Parse { intent: rule.intent.clone(), args, heard })
    }
}

/// Lo que une órdenes seguidas: «y», «luego», «después», una coma.
const LINKS: &[&str] = &["y", "e", "luego", "despues", "entonces", "tambien", "ademas", ","];

/// Una orden de una frase compuesta y lo que se dijo para ella.
#[derive(Clone, Debug)]
pub struct Clause {
    pub parse: Parse,
    pub said: String,
}

/// Lo dicho en palabras, con las comas y los puntos aparte (no los de «20,5»).
fn pieces(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut spaced = String::with_capacity(text.len() + 8);
    for (i, &c) in chars.iter().enumerate() {
        let number = i > 0 && chars[i - 1].is_ascii_digit() && chars.get(i + 1).is_some_and(char::is_ascii_digit);
        if matches!(c, ',' | '.' | ';') && !number {
            spaced.push_str(" , ");
        } else {
            spaced.push(c);
        }
    }
    spaced.split_whitespace().map(str::to_string).collect()
}

fn is_link(word: &str) -> bool {
    LINKS.contains(&words::fold(word).as_str())
}

/// Sin los enlaces de los bordes: «y después ve allí» → «ve allí».
fn trim_links(words: &[String]) -> &[String] {
    let start = words.iter().position(|w| !is_link(w)).unwrap_or(words.len());
    let end = words.iter().rposition(|w| !is_link(w)).map_or(start, |e| e + 1);
    &words[start..end.max(start)]
}

impl Grammar {
    /// Lo dicho como órdenes seguidas: «abre Firefox y muévelo al nivel 2»
    /// son dos. Se parte por los enlaces («y», «luego», una coma) solo si
    /// cada trozo es una orden; un trozo sin verbo toma el de la orden de
    /// antes («pon Spotify a la izquierda y Discord a la derecha»). Si no se
    /// puede partir, lo dicho entero es una orden (o ninguna).
    pub fn clauses(&self, text: &str) -> Option<Vec<Clause>> {
        if text.len() > 4096 {
            return None;
        }
        self.chain(&pieces(text), None)
    }

    fn chain(&self, words: &[String], verb: Option<&str>) -> Option<Vec<Clause>> {
        let words = trim_links(words);
        if words.is_empty() {
            return None;
        }
        for i in 1..words.len() {
            if !is_link(&words[i]) {
                continue;
            }
            let left = trim_links(&words[..i]);
            let Some(first) = (!left.is_empty()).then(|| self.one(left, verb)).flatten() else {
                continue;
            };
            // Lo que acaba en un texto libre («busca pan y leche», «pon Simon y
            // Garfunkel») no presta su verbo: lo de detrás tiene que ser otra orden.
            let whole = first.parse.args.get("whole") == Some(&Value::Bool(true));
            let carry = if whole { String::new() } else { first.parse.heard.split(' ').next().unwrap_or("").to_string() };
            if let Some(rest) = self.chain(&words[i..], Some(&carry)) {
                return Some(std::iter::once(first).chain(rest).collect());
            }
        }
        self.one(words, verb).map(|clause| vec![clause])
    }

    /// Una orden; si no casa sola y va detrás de otra, con el verbo de aquella.
    fn one(&self, words: &[String], verb: Option<&str>) -> Option<Clause> {
        let said = words.join(" ");
        if let Some(parse) = self.parse(&said) {
            return Some(Clause { parse, said });
        }
        let said = format!("{} {said}", verb.filter(|v| !v.is_empty())?);
        self.parse(&said).map(|parse| Clause { parse, said })
    }

    /// Si en lo dicho hay un pronombre que señala algo de antes: «muévelo»,
    /// «ponla», «ciérralo», o «lo» y «él» sueltos. «Esto» y «eso» no: son lo
    /// que tienes delante.
    pub fn pointing(&self, said: &str) -> bool {
        let words: Vec<String> = words::split(said).into_iter().map(|t| t.key).collect();
        words.iter().enumerate().any(|(i, w)| {
            let enclitic = ["los", "las", "lo", "la"].iter().any(|end| {
                w.len() > end.len() + 1 && w.ends_with(end) && self.aliases.get(&vec![w.clone()]).is_some_and(|canon| canon.len() == 1 && canon[0] != *w)
            });
            // «el» y «la» son también artículos: solo cuentan si no les sigue un nombre.
            let bare = words.get(i + 1).is_none_or(|n| matches!(n.as_str(), "a" | "al" | "en"));
            let alone = match w.as_str() {
                "lo" | "ella" => true,
                "el" | "la" => bare,
                _ => false,
            };
            enclitic || alone
        })
    }
}

/// Si `said` es `word` con una letra cambiada, quitada o puesta (dos, si
/// es larga). Solo en palabras de seis letras o más: en las cortas una
/// letra cambia el sentido («sube», «sabe»).
fn near(said: &str, word: &str) -> bool {
    let (a, b): (Vec<char>, Vec<char>) = (said.chars().collect(), word.chars().collect());
    if b.len() < 6 || a.len().abs_diff(b.len()) > 2 {
        return false;
    }
    let most = if b.len() >= 10 { 2 } else { 1 };
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let next = (row[j + 1] + 1).min(row[j] + 1).min(prev + usize::from(ca != cb));
            prev = row[j + 1];
            row[j + 1] = next;
        }
    }
    row[b.len()] <= most
}

/// Recorre un patrón contra lo dicho desde `at`. `stack` es lo que queda por
/// casar, de lo último a lo primero; cada llamada lo deja como estaba.
fn walk<'r>(stack: &mut Vec<&'r [Node]>, said: &[Token], at: usize, caps: &mut Captures<'r>, score: i32, best: &mut Option<(i32, Captures<'r>)>) {
    let Some(top) = stack.pop() else {
        if at == said.len() && best.as_ref().is_none_or(|(top, _)| score > *top) {
            *best = Some((score, caps.clone()));
        }
        return;
    };
    match top.split_first() {
        None => walk(stack, said, at, caps, score, best),
        Some((node, rest)) => {
            stack.push(rest);
            match node {
                Node::Word(word) => match said.get(at) {
                    Some(t) if &t.key == word => walk(stack, said, at + 1, caps, score + 2, best),
                    // Lo que la voz transcribe mal por una letra («temporizanor»,
                    // «volume»): vale, pero menos que lo dicho bien.
                    Some(t) if near(&t.key, word) => walk(stack, said, at + 1, caps, score + 1, best),
                    _ => {}
                },
                Node::Choice(options) | Node::Maybe(options) => {
                    if matches!(node, Node::Maybe(_)) {
                        walk(stack, said, at, caps, score, best);
                    }
                    for option in options {
                        stack.push(option);
                        walk(stack, said, at, caps, score, best);
                        stack.pop();
                    }
                }
                Node::Slot(name, kind) => {
                    // Lo libre, lo más corto que valga; lo que tiene tipo, lo más largo:
                    // en «apunta dentista mañana a las cinco», el cuándo se lleva todo
                    // lo que puede.
                    let ends: Box<dyn Iterator<Item = usize>> =
                        if *kind == Kind::Text { Box::new(at + 1..=said.len()) } else { Box::new((at + 1..=said.len()).rev()) };
                    for end in ends {
                        if let Some(value) = kind.read(&said[at..end]) {
                            caps.push((name, value));
                            walk(stack, said, end, caps, score + kind.weight(), best);
                            caps.pop();
                        }
                    }
                }
            }
            stack.pop();
        }
    }
    stack.push(top);
}

/// Un patrón, de texto a nodos.
fn compile(pattern: &str) -> Result<Vec<Node>, String> {
    let mut pieces = Vec::new();
    let mut word = String::new();
    let mut chars = pattern.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '(' | ')' | '[' | ']' | '|' => {
                if !word.is_empty() {
                    pieces.push(std::mem::take(&mut word));
                }
                pieces.push(c.to_string());
            }
            '<' => {
                if !word.is_empty() {
                    pieces.push(std::mem::take(&mut word));
                }
                let mut slot = String::from("<");
                for c in chars.by_ref() {
                    slot.push(c);
                    if c == '>' {
                        break;
                    }
                }
                pieces.push(slot);
            }
            c if c.is_whitespace() => {
                if !word.is_empty() {
                    pieces.push(std::mem::take(&mut word));
                }
            }
            c => word.push(c),
        }
    }
    if !word.is_empty() {
        pieces.push(word);
    }
    let mut at = 0;
    let nodes = sequence(&pieces, &mut at)?;
    if at != pieces.len() {
        return Err(format!("sobra «{}»", pieces[at]));
    }
    if nodes.is_empty() {
        return Err("patrón vacío".into());
    }
    Ok(nodes)
}

fn sequence(pieces: &[String], at: &mut usize) -> Result<Vec<Node>, String> {
    let mut nodes = Vec::new();
    while let Some(piece) = pieces.get(*at) {
        match piece.as_str() {
            ")" | "]" | "|" => break,
            "(" | "[" => {
                let close = if piece == "(" { ")" } else { "]" };
                *at += 1;
                let mut options = vec![sequence(pieces, at)?];
                while pieces.get(*at).map(String::as_str) == Some("|") {
                    *at += 1;
                    options.push(sequence(pieces, at)?);
                }
                if pieces.get(*at).map(String::as_str) != Some(close) {
                    return Err(format!("falta «{close}»"));
                }
                *at += 1;
                nodes.push(if close == ")" { Node::Choice(options) } else { Node::Maybe(options) });
            }
            slot if slot.starts_with('<') => {
                let inner = slot.trim_start_matches('<').trim_end_matches('>');
                let (name, kind) = inner.split_once(':').ok_or_else(|| format!("el hueco {slot} no dice su tipo"))?;
                let kind = Kind::named(kind).ok_or_else(|| format!("tipo desconocido: {kind}"))?;
                nodes.push(Node::Slot(name.to_string(), kind));
                *at += 1;
            }
            word => {
                // Las palabras del patrón se leen como lo dicho.
                nodes.extend(words::tokens(word).into_iter().map(Node::Word));
                *at += 1;
            }
        }
    }
    Ok(nodes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spanish() -> Grammar {
        let mut g = Grammar::default();
        g.load(SPANISH, "es.txt").unwrap();
        g
    }

    fn heard(g: &Grammar, text: &str) -> String {
        match g.parse(text) {
            None => "-".into(),
            Some(p) => {
                let mut keys: Vec<_> = p.args.iter().map(|(k, v)| format!("{k}={}", v.as_str().map(String::from).unwrap_or(v.to_string()))).collect();
                keys.sort();
                std::iter::once(p.intent).chain(keys).collect::<Vec<_>>().join(" ")
            }
        }
    }

    #[test]
    fn patterns() {
        let mut g = Grammar::default();
        g.load("alias sube = subir | súbelo\nfiller por favor\n[pon|sube] [el] volumen (a|al) <n:pct> => volume.set\n(sube) [el] volumen [<n:pct>] => volume.up\nabre <app:text> => app.open\nabre [el] portapapeles => launcher view=clipboard", "t")
            .unwrap();
        assert_eq!(heard(&g, "Sube el volumen al 85%, por favor"), "volume.set n=85");
        assert_eq!(heard(&g, "subir el volumen"), "volume.up");
        assert_eq!(heard(&g, "súbelo volumen diez"), "volume.up n=10");
        assert_eq!(heard(&g, "abre el portapapeles"), "launcher view=clipboard");
        assert_eq!(heard(&g, "abre firefox"), "app.open app=firefox");
        assert_eq!(heard(&g, "volumen"), "-");
        assert!(g.load("(sin cerrar => x", "t").is_err());
        assert!(g.load("<n:raro> => x", "t").is_err());
    }

    fn intents(g: &Grammar, said: &str) -> Vec<String> {
        g.clauses(said).map(|cs| cs.iter().map(|c| c.parse.intent.clone()).collect()).unwrap_or_default()
    }

    #[test]
    fn compound_orders() {
        let g = spanish();
        assert_eq!(intents(&g, "Abre Firefox y muévelo al nivel dos"), ["app.open", "window.move"]);
        assert_eq!(intents(&g, "Baja el volumen al 20 % y activa no molestar"), ["volume.set", "quiet"]);
        assert_eq!(intents(&g, "Mueve Firefox al nivel tres y después ve allí"), ["window.move", "context.go"]);
        assert_eq!(intents(&g, "sube el volumen, luego pausa la música y después bloquea la pantalla"), ["volume.up", "media.pause", "session.lock"]);
        // El trozo sin verbo toma el de antes.
        let cs = g.clauses("Pon Spotify a la izquierda y Discord a la derecha").unwrap();
        assert_eq!(
            cs.iter().map(|c| (c.parse.intent.as_str(), c.parse.text("app"))).collect::<Vec<_>>(),
            [("window.place", "Spotify"), ("window.place", "Discord")]
        );
        // Lo que no se puede partir es una sola orden.
        assert_eq!(intents(&g, "apunta comprar pan y leche mañana"), ["event.add"]);
        assert_eq!(intents(&g, "Y ponlo a pantalla completa"), ["window.fullscreen"]);
        assert!(g.clauses("y").is_none());
    }

    #[test]
    fn wrapped_orders() {
        let g = spanish();
        let intent = |said: &str| g.parse(said).map(|p| p.intent);
        for (said, want) in [
            ("oye, qué hora es", "clock.time"),
            ("Oye Bail, ¿qué día es hoy", "clock.date"),
            ("oye veyl, me dices qué hora es", "clock.time"),
            ("perdona, ¿sabes qué hora es?", "clock.time"),
            ("dime qué hora es por favor", "clock.time"),
            ("veyl, ¿me puedes decir qué día es hoy?", "clock.date"),
            ("hola veyl, sube el volumen a 50 si puedes", "volume.set"),
            ("vale, abre firefox", "app.open"),
        ] {
            assert_eq!(intent(said).as_deref(), Some(want), "{said}");
        }
        // Lo que no es una orden, o lo es con algo más, no se fuerza: irá al modelo.
        for said in ["no subas el volumen", "oye, ¿cuál es la capital de Francia?", "qué hora es en Tokio"] {
            assert_eq!(intent(said), None, "{said}");
        }
        // En medio de la frase, lo que envuelve se queda.
        assert_eq!(g.parse("apunta decir hola a Ana mañana").unwrap().text("title"), "decir hola a Ana");
    }

    #[test]
    fn pronouns() {
        let g = spanish();
        for said in ["muévelo al nivel 3", "ponla a pantalla completa", "ciérralo", "mueve lo al nivel 2", "maximízala"] {
            assert!(g.pointing(said), "{said}");
        }
        for said in ["mueve la ventana al nivel 3", "cierra esto", "pon el volumen al 20", "maximiza firefox", "súbele"] {
            assert!(!g.pointing(said), "{said}");
        }
    }

    #[test]
    fn spanish_grammar() {
        let g = spanish();
        let cases = [
            ("sube el volumen a 85", "volume.set n=85"),
            ("Sube el volumen a ochenta y cinco", "volume.set n=85"),
            ("pon el volumen al 40 por ciento", "volume.set n=40"),
            ("volumen al cincuenta", "volume.set n=50"),
            ("¿puedes subir el volumen?", "volume.up"),
            ("sube el volumen un 10%", "volume.up n=10"),
            ("baja un poco el volumen", "volume.down"),
            ("silencia el sonido", "volume.mute on=true"),
            ("quita el silencio", "volume.mute on=false"),
            ("silencia el micro", "mic.mute on=true"),
            ("pon el micrófono al 70", "mic.set n=70"),
            ("pausa la música", "media.pause"),
            ("siguiente canción", "media.next"),
            ("qué está sonando", "media.what"),
            ("cambia a firefox", "window.focus app=firefox"),
            ("llévame a la terminal", "window.focus app=terminal"),
            ("abre el navegador", "app.open app=navegador"),
            ("cierra la ventana", "window.close"),
            ("cierra spotify", "window.close app=spotify"),
            ("pon la ventana en pantalla completa", "window.fullscreen mode=fullscreen on=true"),
            ("maximiza la ventana", "window.fullscreen mode=maximized on=true"),
            ("mueve la ventana al nivel 3", "window.move n=3"),
            ("manda firefox al nivel dos", "window.move app=firefox n=2"),
            ("ve al nivel 2", "level.go n=2"),
            ("ve al tercer nivel", "level.go n=3"),
            ("nivel cuatro", "level.go n=4"),
            ("sube un nivel", "level.step by=-1"),
            ("baja de nivel", "level.step by=1"),
            ("ventana de la izquierda", "window.direction dir=left"),
            ("siguiente ventana", "window.direction dir=right"),
            ("bloquea la pantalla", "session.lock"),
            ("apaga el ordenador", "session.poweroff"),
            ("enciende el wifi", "radio device=wifi on=true"),
            ("apaga el bluetooth", "radio device=bluetooth on=false"),
            ("activa el no molestar", "quiet on=true"),
            ("pon un temporizador de 25 minutos", "timer.start d=1500"),
            ("temporizador de media hora", "timer.start d=1800"),
            ("avísame en diez minutos", "timer.start d=600"),
            ("para el temporizador", "timer.stop"),
            ("cuánto queda", "timer.left"),
            ("¿Qué estuve mirando el jueves por la tarde?", "recall w=el jueves por la tarde"),
            ("en qué estuve trabajando ayer", "recall w=ayer"),
            ("abre el diario", "recall.open"),
            ("abre el diario de ayer", "recall.open w=ayer"),
            ("abre mi diario", "recall.open"),
            ("muéstrame el diario del jueves", "recall.open w=jueves"),
            ("llévame al diario", "recall.open"),
            ("abre la ventana del diario", "recall.open"),
            ("diario de ayer", "recall.open w=ayer"),
            ("abre el recall", "recall.open"),
            ("cierra el diario", "recall.close"),
            ("qué hice esta mañana", "recall w=esta manana"),
            ("qué estuve haciendo", "recall"),
            ("apunta dentista mañana a las cinco", "event.add title=dentista w=manana a las cinco"),
            ("recuérdame llamar a mamá en veinte minutos", "event.add title=llamar a mamá w=en veinte minutos"),
            ("qué tengo mañana", "agenda.ask w=manana"),
            ("qué hora es", "clock.time"),
            ("qué día es hoy", "clock.date"),
            ("cómo van los agentes", "agents.ask"),
            ("qué hace claude", "agents.ask who=claude"),
            ("cambia el esquema a catppuccin", "scheme.set name=catppuccin"),
            ("abre el portapapeles", "launcher view=clipboard"),
            ("haz una captura de pantalla", "shot what=screen"),
            ("haz una captura de una zona", "shot what=area"),
            ("crea una vista de estudio de uni", "view.create name=estudio de uni"),
            ("nueva vista llamada Rust", "view.create name=Rust"),
            ("guarda este nivel como la vista trabajo", "view.create from=level name=trabajo"),
            ("abre la vista estudio de uni", "view.open name=estudio de uni"),
            ("ve a la vista trabajo", "view.open name=trabajo"),
            ("recrea la vista trabajo", "view.open again=true name=trabajo"),
            ("recrea esta vista", "view.open again=true"),
            ("deshaz la vista trabajo", "view.undo name=trabajo"),
            ("cierra esta vista", "view.undo"),
            ("actualiza esta vista", "view.save"),
            ("olvida la vista trabajo", "view.forget name=trabajo"),
            ("muéstrame mis vistas", "view.list"),
            ("mueve esto al monitor de la izquierda", "window.monitor dir=left"),
            ("pon esto en el otro monitor", "window.monitor dir=other"),
            ("pon esto al otro monitor", "window.monitor dir=other"),
            ("muévelo a la otra pantalla", "window.monitor dir=other"),
            ("pásalo a la pantalla de la derecha", "window.monitor dir=right"),
            ("manda firefox al monitor izquierdo", "window.monitor app=firefox dir=left"),
            ("mueve la ventana al monitor de arriba", "window.monitor dir=up"),
            ("lleva esto al segundo monitor", "window.monitor n=2"),
            ("mueve spotify al monitor 1", "window.monitor app=spotify n=1"),
            ("pon esto a la izquierda", "window.place side=left"),
            ("sí", "confirm"),
            ("venga", "confirm"),
            ("cancela", "cancel"),
            ("escribe un poema sobre gatos", "-"),
            ("qué tiempo hace mañana", "-"),
        ];
        let mut wrong = Vec::new();
        for (said, want) in cases {
            let got = heard(&g, said);
            if got != want {
                wrong.push(format!("«{said}»: {got} (se esperaba {want})"));
            }
        }
        assert!(wrong.is_empty(), "\n{}", wrong.join("\n"));
    }
}
