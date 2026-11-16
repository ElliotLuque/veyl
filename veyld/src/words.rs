//! Las palabras: lo dicho, sin tildes ni mayúsculas y en piezas, y lo que
//! significan las que cuentan (números, ordinales, duraciones).

/// Sin mayúsculas ni tildes: «Música» y «musica» son lo mismo.
pub fn fold(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            c => c,
        })
        .collect()
}

/// Una pieza de lo dicho: cómo se lee (sin tildes ni mayúsculas) y cómo se
/// dijo, para devolverlo tal cual («llamar a Mamá»).
#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub key: String,
    pub said: String,
}

/// Lo dicho, en piezas: palabras, cifras, horas («16:30») y «%». Lo demás
/// (signos, comillas) separa. Las cifras se separan de las letras: «5min»
/// son «5» y «min».
pub fn split(text: &str) -> Vec<Token> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut token = Token { key: String::new(), said: String::new() };
    let flush = |token: &mut Token, out: &mut Vec<Token>| {
        if !token.key.is_empty() {
            out.push(std::mem::replace(token, Token { key: String::new(), said: String::new() }));
        }
    };
    let digit = |c: char| c.is_ascii_digit() || c == ':';
    for (i, &c) in chars.iter().enumerate() {
        let digit_before = i > 0 && chars[i - 1].is_ascii_digit();
        let digit_after = chars.get(i + 1).is_some_and(char::is_ascii_digit);
        if c.is_alphanumeric() {
            // Entre cifra y letra, otra pieza.
            if let Some(last) = token.key.chars().last()
                && digit(last) != digit(c)
            {
                flush(&mut token, &mut out);
            }
            token.key.push_str(&fold(&c.to_string()));
            token.said.push(c);
        } else if (c == ':' || c == '.') && digit_before && digit_after {
            token.key.push(':');
            token.said.push(':');
        } else if c == '%' {
            flush(&mut token, &mut out);
            out.push(Token { key: "%".into(), said: "%".into() });
        } else {
            flush(&mut token, &mut out);
        }
    }
    flush(&mut token, &mut out);
    out
}

/// Solo cómo se lee cada pieza.
pub fn tokens(text: &str) -> Vec<String> {
    split(text).into_iter().map(|t| t.key).collect()
}

fn unit(word: &str) -> Option<u32> {
    Some(match word {
        "cero" => 0,
        "un" | "uno" | "una" => 1,
        "dos" => 2,
        "tres" => 3,
        "cuatro" => 4,
        "cinco" => 5,
        "seis" => 6,
        "siete" => 7,
        "ocho" => 8,
        "nueve" => 9,
        _ => return None,
    })
}

/// De cero a veintinueve, en una palabra.
fn small(word: &str) -> Option<u32> {
    if let Some(n) = unit(word) {
        return Some(n);
    }
    Some(match word {
        "diez" => 10,
        "once" => 11,
        "doce" => 12,
        "trece" => 13,
        "catorce" => 14,
        "quince" => 15,
        "dieciseis" => 16,
        "diecisiete" => 17,
        "dieciocho" => 18,
        "diecinueve" => 19,
        "veinte" => 20,
        "veintiun" | "veintiuno" | "veintiuna" => 21,
        _ => {
            let rest = word.strip_prefix("veinti")?;
            return unit(rest).filter(|&n| n > 1).map(|n| 20 + n);
        }
    })
}

fn tens(word: &str) -> Option<u32> {
    Some(match word {
        "treinta" => 30,
        "cuarenta" => 40,
        "cincuenta" => 50,
        "sesenta" => 60,
        "setenta" => 70,
        "ochenta" => 80,
        "noventa" => 90,
        _ => return None,
    })
}

fn hundreds(word: &str) -> Option<u32> {
    Some(match word {
        "cien" | "ciento" => 100,
        "doscientos" | "doscientas" => 200,
        "trescientos" | "trescientas" => 300,
        "cuatrocientos" | "cuatrocientas" => 400,
        "quinientos" | "quinientas" => 500,
        "seiscientos" | "seiscientas" => 600,
        "setecientos" | "setecientas" => 700,
        "ochocientos" | "ochocientas" => 800,
        "novecientos" | "novecientas" => 900,
        _ => return None,
    })
}

/// Un número entero, en cifras («85») o en palabras («ochenta y cinco»,
/// «ciento veinte»). Tiene que ocupar todas las piezas.
pub fn number(words: &[String]) -> Option<u32> {
    if let [only] = words
        && !only.is_empty()
        && only.bytes().all(|b| b.is_ascii_digit())
    {
        return only.parse().ok();
    }
    let mut at = 0;
    let mut total = 0;
    let word = |at: usize| words.get(at).map(String::as_str);
    if let Some(h) = word(at).and_then(hundreds) {
        // «cien» va solo; «ciento», con algo detrás.
        if word(at) == Some("cien") && at + 1 != words.len() {
            return None;
        }
        total += h;
        at += 1;
    }
    if let Some(t) = word(at).and_then(tens) {
        total += t;
        at += 1;
        if word(at) == Some("y")
            && let Some(u) = word(at + 1).and_then(unit).filter(|&u| u > 0)
        {
            total += u;
            at += 2;
        }
    } else if let Some(s) = word(at).and_then(small) {
        total += s;
        at += 1;
    }
    (at > 0 && at == words.len()).then_some(total)
}

/// Un ordinal o un número: «tercero», «el 3», «tres».
pub fn ordinal(words: &[String]) -> Option<u32> {
    if let [only] = words {
        let n = match only.as_str() {
            "primer" | "primero" | "primera" => 1,
            "segundo" | "segunda" => 2,
            "tercer" | "tercero" | "tercera" => 3,
            "cuarto" | "cuarta" => 4,
            "quinto" | "quinta" => 5,
            "sexto" | "sexta" => 6,
            "septimo" | "septima" | "setimo" => 7,
            "octavo" | "octava" => 8,
            "noveno" | "novena" => 9,
            "decimo" | "decima" => 10,
            _ => 0,
        };
        if n > 0 {
            return Some(n);
        }
    }
    number(words)
}

/// Un porcentaje: un número, con «%» o «por ciento» detrás o sin nada.
pub fn percent(words: &[String]) -> Option<u32> {
    let bare = match words {
        [rest @ .., p] if p == "%" || p == "porciento" => rest,
        [rest @ .., por, ciento] if por == "por" && ciento == "ciento" => rest,
        _ => words,
    };
    number(bare)
}

/// Segundos de una unidad de tiempo.
fn seconds(word: &str) -> Option<u32> {
    Some(match word {
        "segundo" | "segundos" | "seg" | "s" => 1,
        "minuto" | "minutos" | "min" | "m" => 60,
        "hora" | "horas" | "h" => 3600,
        _ => return None,
    })
}

/// Una duración, en segundos: «cinco minutos», «hora y media», «media hora»,
/// «1 hora y 20 minutos», «un cuarto de hora». Un número solo son minutos.
pub fn duration(words: &[String]) -> Option<u32> {
    if let Some(n) = number(words) {
        return n.checked_mul(60).filter(|&secs| secs > 0);
    }
    let w: Vec<&str> = words.iter().map(String::as_str).collect();
    let mut at = 0;
    let mut total: u32 = 0;
    while at < w.len() {
        if at > 0 {
            if w[at] != "y" {
                return None;
            }
            at += 1;
        }
        let rest = &w[at..];
        let (secs, used) = match rest {
            ["media", "hora", ..] => (1800, 2),
            ["un", "cuarto", "de", "hora", ..] => (900, 4),
            ["cuarto", "de", "hora", ..] => (900, 3),
            ["tres", "cuartos", "de", "hora", ..] => (2700, 4),
            _ => {
                // Un número (de una a tres palabras) y su unidad; «hora» sola es una.
                let mut found = matches!(rest.first(), Some(&("hora" | "minuto" | "segundo"))).then(|| seconds(rest[0]).map(|unit| (unit, 1, unit))).flatten();
                for len in (1..=rest.len().min(4)).rev() {
                    if found.is_some() {
                        break;
                    }
                    let Some(unit) = rest.get(len).and_then(|u| seconds(u)) else {
                        continue;
                    };
                    let digits: Vec<String> = rest[..len].iter().map(|s| s.to_string()).collect();
                    if let Some(n) = number(&digits) {
                        found = Some((n.checked_mul(unit)?, len + 1, unit));
                    }
                }
                let (secs, used, unit) = found?;
                // «hora y media», «minuto y medio», «hora y cuarto».
                match &rest[used..] {
                    ["y", "media" | "medio", ..] => (secs.checked_add(unit / 2)?, used + 2),
                    ["y", "cuarto", ..] if unit == 3600 => (secs.checked_add(900)?, used + 2),
                    _ => (secs, used),
                }
            }
        };
        total = total.checked_add(secs)?;
        at += used;
    }
    (total > 0).then_some(total)
}

/// Una duración en palabras: «1 h 20 min», «45 min», «30 s».
pub fn spoken(ms: i64) -> String {
    let minutes = (ms + 30_000) / 60_000;
    if ms < 60_000 {
        return format!("{} s", (ms / 1000).max(1));
    }
    if minutes < 60 {
        return format!("{minutes} min");
    }
    match minutes % 60 {
        0 => format!("{} h", minutes / 60),
        m => format!("{} h {m} min", minutes / 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(text: &str) -> Vec<String> {
        tokens(text)
    }

    #[test]
    fn pieces() {
        assert_eq!(t("¡Sube el volumen al 85%!"), ["sube", "el", "volumen", "al", "85", "%"]);
        assert_eq!(t("A las 16:30h"), ["a", "las", "16:30", "h"]);
        assert_eq!(t("las 16.30"), ["las", "16:30"]);
        assert_eq!(t("5min"), ["5", "min"]);
        assert_eq!(t("Mañana, por favor."), ["manana", "por", "favor"]);
    }

    #[test]
    fn numbers() {
        assert_eq!(number(&t("85")), Some(85));
        assert_eq!(number(&t("ochenta y cinco")), Some(85));
        assert_eq!(number(&t("ochenta")), Some(80));
        assert_eq!(number(&t("veinticinco")), Some(25));
        assert_eq!(number(&t("veintiún")), Some(21));
        assert_eq!(number(&t("cien")), Some(100));
        assert_eq!(number(&t("ciento veinte")), Some(120));
        assert_eq!(number(&t("cien veinte")), None);
        assert_eq!(number(&t("y cinco")), None);
        assert_eq!(number(&t("treinta y")), None);
        assert_eq!(ordinal(&t("tercer")), Some(3));
        assert_eq!(ordinal(&t("dos")), Some(2));
        assert_eq!(percent(&t("85 %")), Some(85));
        assert_eq!(percent(&t("cuarenta por ciento")), Some(40));
    }

    #[test]
    fn durations() {
        assert_eq!(duration(&t("cinco minutos")), Some(300));
        assert_eq!(duration(&t("25")), Some(1500));
        assert_eq!(duration(&t("media hora")), Some(1800));
        assert_eq!(duration(&t("hora y media")), Some(5400));
        assert_eq!(duration(&t("una hora y media")), Some(5400));
        assert_eq!(duration(&t("1 hora y 20 minutos")), Some(4800));
        assert_eq!(duration(&t("un cuarto de hora")), Some(900));
        assert_eq!(duration(&t("dos horas y cuarto")), Some(8100));
        assert_eq!(duration(&t("90 segundos")), Some(90));
        assert_eq!(duration(&t("minuto y medio")), Some(90));
        assert_eq!(duration(&t("un minuto y medio")), Some(90));
        assert_eq!(duration(&t("minutos")), None);
    }

    #[test]
    fn duration_overflow_and_zero() {
        for text in ["0", "cero segundos", "4294967295", "4294967295 horas", "4294967295 segundos y un segundo", "71582788 minutos y medio"] {
            assert_eq!(duration(&t(text)), None, "{text}");
        }
        assert_eq!(duration(&t("4294967295 segundos")), Some(u32::MAX));
    }

    #[test]
    fn spans() {
        assert_eq!(spoken(4_800_000), "1 h 20 min");
        assert_eq!(spoken(2_700_000), "45 min");
        assert_eq!(spoken(7_200_000), "2 h");
        assert_eq!(spoken(12_000), "12 s");
    }
}
