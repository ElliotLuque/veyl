//! El vocabulario de la voz: palabras que Whisper no conoce («Veyl»,
//! «Hyprland») y cómo las oye mal («bale», «vail»). Sirve dos veces: las
//! palabras van en la pista que se le da a Whisper antes de oír, y lo oído
//! mal se corrige antes de leerlo (la gramática y el cerebro ya reciben
//! «qué puede hacer Veyl», no «qué puede hacer bale»).
//!
//! Lo de fábrica está aquí; lo tuyo, en `~/.config/veyl/vocabulary.txt`,
//! una palabra por línea y, si la oye mal, cómo:
//!
//! ```text
//! # comentario
//! Hyprland = hyperland | hiperland
//! Obsidian
//! Spotify = espotifai
//! ```

use std::fs;

use crate::config;
use crate::words::fold;

/// Lo de fábrica: Veyl y cómo lo oye Whisper en español.
const BUILTIN: &str = "\
Veyl = bale | bail | vail | veil | beil | beyl | vayl | bayl | weil | veile | veyle | beile | vel | beel | baylo
oye Veyl = jeveyl | oyeveyl | oyebeil | ojeveyl | olleveyl | heyveyl | eveyl
Hyprland = hyperland | hiperland | hyper land | hiper land | hyprlan
Quickshell = quick shell | quickshel | kuikshel
kitty = kiti | quiti | kity
Claude Code = cloud code | clod code | clode code
Claude = clod | clode
";

/// Una palabra y cómo se oye mal (ya sin tildes ni mayúsculas, en palabras).
#[derive(Clone, Debug, Default)]
pub struct Vocabulary {
    words: Vec<(String, Vec<Vec<String>>)>,
}

fn keys(text: &str) -> Vec<String> {
    fold(text).split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_string).collect()
}

impl Vocabulary {
    /// El de fábrica y el tuyo, si lo hay.
    pub fn load() -> Vocabulary {
        let mut v = Vocabulary::default();
        v.read(BUILTIN);
        if let Ok(text) = fs::read_to_string(config::config().join("vocabulary.txt")) {
            v.read(&text);
        }
        v
    }

    fn read(&mut self, text: &str) {
        for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
            let (word, heard) = line.split_once('=').unwrap_or((line, ""));
            let word = word.trim().to_string();
            if word.is_empty() {
                continue;
            }
            let heard: Vec<Vec<String>> = heard.split('|').map(keys).filter(|k| !k.is_empty()).collect();
            match self.words.iter_mut().find(|(w, _)| fold(w) == fold(&word)) {
                Some((_, old)) => old.extend(heard),
                None => self.words.push((word, heard)),
            }
        }
    }

    /// Las palabras, para la pista de Whisper.
    pub fn hints(&self) -> Vec<String> {
        self.words.iter().map(|(w, _)| w.clone()).collect()
    }

    /// Lo oído, con lo que se oye mal cambiado por su palabra. Solo palabras
    /// enteras; de lo que casa, lo más largo.
    pub fn fix(&self, text: &str) -> String {
        // Las palabras de lo dicho, con lo que va entre ellas (espacios, comas).
        let mut pieces: Vec<(String, String)> = Vec::new();
        let mut word = String::new();
        for c in text.chars() {
            if c.is_alphanumeric() {
                word.push(c);
            } else {
                pieces.push((std::mem::take(&mut word), c.to_string()));
            }
        }
        pieces.push((word, String::new()));
        // Juntar lo que va entre palabras vacías con la anterior.
        let mut tokens: Vec<(String, String)> = Vec::new();
        for (w, gap) in pieces {
            match tokens.last_mut() {
                Some(last) if w.is_empty() => last.1.push_str(&gap),
                _ if w.is_empty() => tokens.push((String::new(), gap)),
                _ => tokens.push((w, gap)),
            }
        }
        let folded: Vec<String> = tokens.iter().map(|(w, _)| fold(w)).collect();
        let mut out = String::new();
        let mut at = 0;
        while at < tokens.len() {
            let best = self
                .words
                .iter()
                .flat_map(|(word, heard)| heard.iter().map(move |h| (word, h)))
                .filter(|(_, h)| at + h.len() <= tokens.len() && folded[at..at + h.len()] == h[..])
                .max_by_key(|(_, h)| h.len());
            match best {
                Some((word, h)) => {
                    out.push_str(word);
                    out.push_str(&tokens[at + h.len() - 1].1);
                    at += h.len();
                }
                None => {
                    out.push_str(&tokens[at].0);
                    out.push_str(&tokens[at].1);
                    at += 1;
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn builtin() -> Vocabulary {
        let mut v = Vocabulary::default();
        v.read(BUILTIN);
        v
    }

    #[test]
    fn misheard_words_are_fixed() {
        let v = builtin();
        assert_eq!(v.fix("Oye, ¿qué puede hacer bale?"), "Oye, ¿qué puede hacer Veyl?");
        assert_eq!(v.fix("Vail, abre kiti"), "Veyl, abre kitty");
        assert_eq!(v.fix("reinicia hyper land"), "reinicia Hyprland");
        assert_eq!(v.fix("Jeveyl, abre Prime Video en Firefox"), "oye Veyl, abre Prime Video en Firefox");
        // Solo palabras enteras, y lo demás tal cual.
        assert_eq!(v.fix("vale, baja el volumen"), "vale, baja el volumen");
        assert_eq!(v.fix("Ballena"), "Ballena");
        assert_eq!(v.fix(""), "");
        assert!(v.hints().contains(&"Veyl".to_string()));
    }

    #[test]
    fn yours_adds_up() {
        let mut v = builtin();
        v.read("# lo mío\nSpotify = espotifai\nObsidian\nveyl = beii");
        assert_eq!(v.fix("abre espotifai"), "abre Spotify");
        assert_eq!(v.fix("beii"), "Veyl");
        assert!(v.hints().contains(&"Obsidian".to_string()));
        assert_eq!(v.hints().iter().filter(|w| fold(w) == "veyl").count(), 1);
    }
}
