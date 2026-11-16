//! Cuándo: «el jueves por la tarde», «ayer a las cinco», «hace dos horas»,
//! «mañana a las 16:30», «en veinte minutos». Se lee igual para mirar atrás
//! (el diario) que para apuntar algo (la agenda); lo que cambia es hacia
//! dónde se resuelve.

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveTime, TimeZone, Timelike};

use crate::words::{duration, number, spoken};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Day {
    /// Hoy más (o menos) tantos días: ayer es -1.
    Offset(i64),
    /// Un día de la semana (0 = lunes); `shift` -1 es «pasado», +1 «que viene».
    Weekday {
        day: u32,
        shift: i64,
    },
    /// Un día del mes, de este mes si no se dice cuál.
    Date {
        day: u32,
        month: Option<u32>,
    },
    ThisWeek,
    LastWeek,
    Weekend,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Part {
    Dawn,
    Morning,
    Noon,
    Afternoon,
    Night,
    /// Anoche: de la noche de ayer a la madrugada de hoy.
    LastNight,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct When {
    pub day: Option<Day>,
    pub part: Option<Part>,
    /// Hora y minuto tal como se dicen («las cinco» son 5): la parte del
    /// día decide si son de la tarde.
    pub clock: Option<(u32, u32)>,
    /// «hace dos horas», en segundos.
    pub ago: Option<u32>,
    /// «en veinte minutos», «dentro de una hora», en segundos.
    pub within: Option<u32>,
    /// «en la última hora», en segundos.
    pub last: Option<u32>,
}

const WEEKDAYS: [&str; 7] = ["lunes", "martes", "miercoles", "jueves", "viernes", "sabado", "domingo"];
const MONTHS: [&str; 12] = ["enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre"];
const DAY_NAMES: [&str; 7] = ["lunes", "martes", "miércoles", "jueves", "viernes", "sábado", "domingo"];

fn weekday(word: &str) -> Option<u32> {
    WEEKDAYS.iter().position(|&w| w == word).map(|i| i as u32)
}

fn month(word: &str) -> Option<u32> {
    if word == "setiembre" {
        return Some(9);
    }
    MONTHS.iter().position(|&m| m == word).map(|i| i as u32 + 1)
}

fn owned(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| w.to_string()).collect()
}

/// El número más largo al principio de `words` (hasta cuatro palabras):
/// su valor y cuántas ocupa.
fn leading_number(words: &[&str]) -> Option<(u32, usize)> {
    (1..=words.len().min(4)).rev().find_map(|len| number(&owned(&words[..len])).map(|n| (n, len)))
}

/// La duración más larga detrás de unas palabras de entrada.
fn leading_duration(words: &[&str]) -> Option<(u32, usize)> {
    (1..=words.len()).rev().find_map(|len| duration(&owned(&words[..len])).map(|d| (d, len)))
}

/// Una parte del día con su preposición: «por la tarde», «esta noche»,
/// «de madrugada». Devuelve la parte, si dice «hoy» y cuántas palabras ocupa.
fn part(w: &[&str]) -> Option<(Part, bool, usize)> {
    let named = |word: &str| match word {
        "manana" => Some(Part::Morning),
        "tarde" => Some(Part::Afternoon),
        "noche" => Some(Part::Night),
        "madrugada" => Some(Part::Dawn),
        "mediodia" => Some(Part::Noon),
        _ => None,
    };
    match w {
        ["por" | "de" | "en" | "a", "la", p, ..] if named(p).is_some() => Some((named(p)?, false, 3)),
        ["esta", p, ..] if named(p).is_some() => Some((named(p)?, true, 2)),
        ["de" | "a", "madrugada", ..] => Some((Part::Dawn, false, 2)),
        ["a" | "al" | "del", "mediodia", ..] => Some((Part::Noon, false, 2)),
        ["mediodia", ..] => Some((Part::Noon, false, 1)),
        ["a", "la", "hora", "de", "comer", ..] => Some((Part::Noon, false, 5)),
        ["anoche", ..] => Some((Part::LastNight, false, 1)),
        _ => None,
    }
}

/// Una hora: «a las cinco y media», «las 16:30», «a la una menos cuarto»,
/// «sobre las 5». Devuelve (hora, minuto) y cuántas palabras ocupa.
fn clock(w: &[&str]) -> Option<((u32, u32), usize)> {
    let mut at = match w {
        ["a", "eso", "de", "las" | "la", ..] => 4,
        ["a" | "sobre" | "hacia" | "para" | "desde" | "hasta", "las" | "la", ..] => 2,
        ["las" | "la", ..] => 1,
        _ => 0,
    };
    // La hora de antes: «la una menos cuarto» son las 12:45.
    let before = |h: u32| match h {
        0 => 23,
        1 => 12,
        h => h - 1,
    };
    let (mut hour, mut minute);
    if let Some((h, m)) = w.get(at).and_then(|t| t.split_once(':')) {
        hour = h.parse().ok()?;
        minute = m.parse().ok()?;
        at += 1;
    } else {
        // Sin «las» delante, solo vale con «:» («16:30»).
        if at == 0 {
            return None;
        }
        let (h, used) = leading_number(&w[at..])?;
        if h > 23 {
            return None;
        }
        hour = h;
        minute = 0;
        at += used;
        match &w[at..] {
            ["y", "media", ..] => (minute, at) = (30, at + 2),
            ["y", "cuarto", ..] => (minute, at) = (15, at + 2),
            ["menos", "cuarto", ..] => (hour, minute, at) = (before(hour), 45, at + 2),
            ["en", "punto", ..] => at += 2,
            ["y" | "menos", rest @ ..] => {
                if let Some((m, used)) = leading_number(rest).filter(|&(m, _)| (1..60).contains(&m)) {
                    if w[at] == "y" {
                        minute = m;
                    } else {
                        (hour, minute) = (before(hour), 60 - m);
                    }
                    at += 1 + used;
                }
            }
            _ => {}
        }
    }
    if matches!(w.get(at), Some(&("h" | "horas"))) {
        at += 1;
    }
    (hour < 24 && minute < 60).then_some(((hour, minute), at))
}

/// Lo que se puede decir de un día: «hoy», «el jueves pasado», «el 12 de
/// noviembre», «la semana pasada».
fn day(w: &[&str]) -> Option<(Day, usize)> {
    let simple = match w {
        ["hoy", ..] => Some((Day::Offset(0), 1)),
        ["ayer", ..] => Some((Day::Offset(-1), 1)),
        ["anteayer" | "antier", ..] => Some((Day::Offset(-2), 1)),
        ["antes", "de", "ayer", ..] => Some((Day::Offset(-2), 3)),
        ["pasado", "manana", ..] => Some((Day::Offset(2), 2)),
        ["manana", ..] => Some((Day::Offset(1), 1)),
        ["esta", "semana", ..] => Some((Day::ThisWeek, 2)),
        ["la", "semana", "pasada", ..] | ["la", "pasada", "semana", ..] => Some((Day::LastWeek, 3)),
        ["este" | "el", "fin", "de", "semana", ..] => Some((Day::Weekend, 4)),
        _ => None,
    };
    if simple.is_some() {
        return simple;
    }
    // Un día de la semana.
    let mut at = 0;
    let mut shift = 0;
    match w {
        ["el" | "este", "proximo" | "siguiente", ..] => (at, shift) = (2, 1),
        ["el" | "este" | "esta", ..] => at = 1,
        _ => {}
    }
    if let Some(d) = w.get(at).and_then(|t| weekday(t)) {
        at += 1;
        match &w[at..] {
            ["pasado", ..] => (shift, at) = (-1, at + 1),
            ["que", "viene", ..] => (shift, at) = (1, at + 2),
            ["proximo" | "siguiente", ..] => (shift, at) = (1, at + 1),
            _ => {}
        }
        return Some((Day::Weekday { day: d, shift }, at));
    }
    // Un día del mes: «el 12», «el día 12 de noviembre», «12 de noviembre».
    let prefix = match w {
        ["el", "dia", ..] => 2,
        ["el" | "dia", ..] => 1,
        _ => 0,
    };
    let (d, used) = leading_number(&w[prefix..])?;
    let mut at = prefix + used;
    let mut month_of = None;
    if let ["de", m, ..] = &w[at..]
        && let Some(m) = month(m)
    {
        month_of = Some(m);
        at += 2;
    }
    // Un número suelto no es un día: hace falta «el», «día» o el mes.
    if (prefix == 0 && month_of.is_none()) || !(1..=31).contains(&d) {
        return None;
    }
    Some((Day::Date { day: d, month: month_of }, at))
}

/// Lee una expresión de tiempo entera; `None` si sobra o falta algo.
pub fn parse(words: &[String]) -> Option<When> {
    let w: Vec<&str> = words.iter().map(String::as_str).collect();
    let mut when = When::default();
    // El «hoy» que dice «esta tarde»: otro día dicho aparte lo sustituye.
    let mut implied = false;
    let mut at = 0;
    while at < w.len() {
        let rest = &w[at..];
        // Las partes del día van antes que «mañana» suelta: «por la mañana».
        if let Some((p, today, used)) = part(rest) {
            if when.part.is_some() {
                return None;
            }
            when.part = Some(p);
            if (p == Part::LastNight || today) && when.day.is_none() {
                when.day = Some(Day::Offset(if p == Part::LastNight { -1 } else { 0 }));
                implied = true;
            }
            at += used;
            continue;
        }
        match rest {
            ["hace", tail @ ..] => {
                let (secs, used) = leading_duration(tail)?;
                when.ago = Some(secs);
                at += 1 + used;
                continue;
            }
            ["en" | "durante", "la" | "las" | "el" | "los", "ultima" | "ultimas" | "ultimo" | "ultimos", tail @ ..] => {
                let (secs, used) = leading_duration(tail)?;
                when.last = Some(secs);
                at += 3 + used;
                continue;
            }
            ["dentro", "de", tail @ ..] | ["en", tail @ ..] if leading_duration(tail).is_some() => {
                let (secs, used) = leading_duration(tail)?;
                when.within = Some(secs);
                at += rest.len() - tail.len() + used;
                continue;
            }
            ["todo", "el", "dia", ..] => {
                at += 3;
                continue;
            }
            _ => {}
        }
        if let Some((c, used)) = clock(rest) {
            if when.clock.is_some() {
                return None;
            }
            when.clock = Some(c);
            at += used;
            continue;
        }
        if let Some((d, used)) = day(rest) {
            if when.day.is_some() && !implied {
                return None;
            }
            when.day = Some(d);
            implied = false;
            at += used;
            continue;
        }
        return None;
    }
    let relative = [when.ago, when.within, when.last].iter().filter(|value| value.is_some()).count();
    if relative > 1 || (relative > 0 && (when.day.is_some() || when.part.is_some() || when.clock.is_some())) {
        return None;
    }
    (when != When::default()).then_some(when)
}

/// Un tramo de tiempo ya pasado, y cómo se dice.
#[derive(Clone, Debug)]
pub struct Span {
    pub from: DateTime<Local>,
    pub to: DateTime<Local>,
    pub label: String,
}

fn at(date: NaiveDate, minutes: i64) -> DateTime<Local> {
    let naive = date.and_hms_opt(0, 0, 0).expect("medianoche") + Duration::minutes(minutes);
    Local
        .from_local_datetime(&naive)
        .earliest()
        // Un cambio de hora se come esa hora: la siguiente que existe.
        .unwrap_or_else(|| Local.from_local_datetime(&(naive + Duration::hours(1))).earliest().expect("hora local"))
}

/// Entre qué horas cae cada parte (en minutos desde la medianoche; la noche
/// pasa de las 24).
fn hours(part: Part) -> (i64, i64) {
    match part {
        Part::Dawn => (0, 6 * 60),
        Part::Morning => (6 * 60, 13 * 60),
        Part::Noon => (13 * 60, 16 * 60),
        Part::Afternoon => (15 * 60, 21 * 60),
        Part::Night => (21 * 60, 26 * 60),
        Part::LastNight => (21 * 60, 28 * 60),
    }
}

/// Las cinco son de la tarde salvo que se diga otra cosa: nadie pide algo
/// a las 5 de la madrugada sin decirlo.
fn hour24(hour: u32, part: Option<Part>) -> u32 {
    match (hour, part) {
        (12, Some(Part::Night | Part::LastNight | Part::Dawn)) => 0,
        (h, _) if h == 0 || h >= 13 => h % 24,
        (h, Some(Part::Afternoon | Part::Night | Part::LastNight)) if h < 12 => h + 12,
        (h, Some(Part::Noon)) if h <= 4 => h + 12,
        (h, Some(_)) => h,
        (h, None) if (1..=7).contains(&h) => h + 12,
        (h, None) => h,
    }
}

fn hhmm(minutes: i64) -> String {
    format!("{:02}:{:02}", (minutes / 60) % 24, minutes % 60)
}

/// «hoy», «ayer», «el jueves 12 de noviembre».
pub fn day_name(date: NaiveDate, today: NaiveDate) -> String {
    match (date - today).num_days() {
        0 => "hoy".into(),
        -1 => "ayer".into(),
        -2 => "anteayer".into(),
        1 => "mañana".into(),
        _ => {
            let wd = DAY_NAMES[date.weekday().num_days_from_monday() as usize];
            let mut label = format!("el {wd} {} de {}", date.day(), MONTHS[date.month0() as usize]);
            if date.year() != today.year() {
                label += &format!(" de {}", date.year());
            }
            label
        }
    }
}

fn part_name(part: Part) -> &'static str {
    match part {
        Part::Dawn => "de madrugada",
        Part::Morning => "por la mañana",
        Part::Noon => "a mediodía",
        Part::Afternoon => "por la tarde",
        Part::Night => "por la noche",
        Part::LastNight => "anoche",
    }
}

/// Lo dicho, hacia atrás: el tramo de tiempo al que se refiere, que acaba
/// como muy tarde ahora. `None` si aún no ha pasado.
pub fn past(when: &When, now: DateTime<Local>) -> Option<Span> {
    if when.within.is_some() {
        return None;
    }
    if let Some(secs) = when.last {
        let from = now - Duration::seconds(secs as i64);
        return Some(Span { from, to: now, label: format!("en los últimos {}", spoken(secs as i64 * 1000)) });
    }
    if let Some(secs) = when.ago {
        let center = now - Duration::seconds(secs as i64);
        let half = Duration::seconds((secs as i64 / 4).clamp(15 * 60, 2 * 3600));
        return Some(Span { from: center - half, to: (center + half).min(now), label: format!("hace {}", spoken(secs as i64 * 1000)) });
    }
    let today = now.date_naive();
    let now_minutes = (now.hour() * 60 + now.minute()) as i64;
    let monday = today - Duration::days(today.weekday().num_days_from_monday() as i64);

    // Los que son una semana entera, o un fin de semana.
    match when.day {
        Some(Day::ThisWeek) => {
            return Some(Span { from: at(monday, 0), to: now, label: "esta semana".into() });
        }
        Some(Day::LastWeek) => {
            let from = monday - Duration::days(7);
            return Some(Span { from: at(from, 0), to: at(monday, 0), label: "la semana pasada".into() });
        }
        Some(Day::Weekend) => {
            let wd = today.weekday().num_days_from_monday() as i64;
            let saturday = if wd >= 5 { today - Duration::days(wd - 5) } else { monday - Duration::days(2) };
            let to = at(saturday + Duration::days(2), 0).min(now);
            return Some(Span { from: at(saturday, 0), to, label: "el fin de semana".into() });
        }
        _ => {}
    }

    let date = match when.day {
        None => None,
        Some(Day::Offset(n)) => Some(today + Duration::days(n)),
        Some(Day::Weekday { day, shift }) => {
            if shift > 0 {
                return None;
            }
            let back = (today.weekday().num_days_from_monday() + 7 - day) % 7;
            let back = if back == 0 && shift < 0 { 7 } else { back };
            Some(today - Duration::days(back as i64))
        }
        Some(Day::Date { day, month }) => {
            let (mut y, mut m) = (today.year(), month.unwrap_or(today.month()));
            let mut date = NaiveDate::from_ymd_opt(y, m, day)?;
            if date > today {
                if month.is_some() {
                    y -= 1;
                } else {
                    (y, m) = if m == 1 { (y - 1, 12) } else { (y, m - 1) };
                }
                date = NaiveDate::from_ymd_opt(y, m, day)?;
            }
            Some(date)
        }
        Some(Day::ThisWeek | Day::LastWeek | Day::Weekend) => unreachable!(),
    };

    let (date, from, to, mut label) = if let Some((hour, minute)) = when.clock {
        let center = hour24(hour, when.part) as i64 * 60 + minute as i64;
        // Sin día, la última vez que fue esa hora.
        let date = date.unwrap_or(if center <= now_minutes { today } else { today - Duration::days(1) });
        let name = format!("{} hacia las {}", day_name(date, today), hhmm(center));
        (date, center - 30, center + 30, name)
    } else if let Some(p) = when.part {
        let (from, to) = hours(p);
        let date = date.unwrap_or(if from <= now_minutes { today } else { today - Duration::days(1) });
        let name = if p == Part::LastNight {
            "anoche".to_string()
        } else {
            let day = day_name(date, today);
            match (day.as_str(), p) {
                ("hoy", Part::Morning) => "esta mañana".into(),
                ("hoy", Part::Afternoon) => "esta tarde".into(),
                ("hoy", Part::Night) => "esta noche".into(),
                ("hoy", Part::Dawn) => "esta madrugada".into(),
                _ => format!("{day} {}", part_name(p)),
            }
        };
        (date, from, to, name)
    } else {
        let date = date?;
        (date, 0, 24 * 60, day_name(date, today))
    };
    let (from, to) = (at(date, from), at(date, to).min(now));
    if from >= now {
        return None;
    }
    if when.clock.is_none() && when.part.is_some() {
        label += &format!(" ({}–{})", from.format("%H:%M"), if to == now { "ahora".into() } else { to.format("%H:%M").to_string() });
    }
    Some(Span { from, to, label })
}

/// Lo dicho, hacia delante: el día y, si se dice, la hora a la que apuntar
/// algo. «el jueves» es el que viene; una hora sin día, hoy o, si ya pasó,
/// mañana.
pub fn ahead(when: &When, now: DateTime<Local>) -> Option<(NaiveDate, Option<NaiveTime>)> {
    if when.ago.is_some() || when.last.is_some() {
        return None;
    }
    if let Some(secs) = when.within {
        let then = now + Duration::seconds(secs as i64);
        return Some((then.date_naive(), NaiveTime::from_hms_opt(then.hour(), then.minute(), 0)));
    }
    let today = now.date_naive();
    let date = match when.day {
        None => None,
        Some(Day::Offset(n)) if n >= 0 => Some(today + Duration::days(n)),
        Some(Day::Weekday { day, shift }) if shift >= 0 => {
            let forward = (day + 7 - today.weekday().num_days_from_monday()) % 7;
            Some(today + Duration::days(if forward == 0 { 7 } else { forward } as i64))
        }
        Some(Day::Date { day, month }) => {
            let (mut y, mut m) = (today.year(), month.unwrap_or(today.month()));
            let mut date = NaiveDate::from_ymd_opt(y, m, day)?;
            if date < today {
                if month.is_some() {
                    y += 1;
                } else {
                    (y, m) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
                }
                date = NaiveDate::from_ymd_opt(y, m, day)?;
            }
            Some(date)
        }
        Some(Day::Weekend) => {
            let wd = today.weekday().num_days_from_monday() as i64;
            Some(today + Duration::days((5 - wd).rem_euclid(7)))
        }
        _ => return None,
    };
    let minutes = match (when.clock, when.part) {
        (Some((hour, minute)), part) => Some(hour24(hour, part) * 60 + minute),
        (None, Some(Part::Dawn)) => Some(6 * 60),
        (None, Some(Part::Morning)) => Some(9 * 60),
        (None, Some(Part::Noon)) => Some(14 * 60),
        (None, Some(Part::Afternoon)) => Some(17 * 60),
        (None, Some(Part::Night | Part::LastNight)) => Some(21 * 60),
        (None, None) => None,
    };
    let time = minutes.and_then(|m| NaiveTime::from_hms_opt(m / 60 % 24, m % 60, 0));
    let date = match (date, time) {
        (Some(date), _) => date,
        (None, Some(time)) => {
            if time > now.time() {
                today
            } else {
                today + Duration::days(1)
            }
        }
        (None, None) => return None,
    };
    Some((date, time))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::words::tokens;

    #[test]
    fn rejects_invalid_clocks_and_conflicting_times() {
        for text in ["24:30", "a las 25 menos cuarto", "hace una hora dentro de dos horas", "mañana hace una hora"] {
            assert!(parse(&tokens(text)).is_none(), "{text}");
        }
        let future = parse(&tokens("dentro de una hora")).unwrap();
        assert!(past(&future, now()).is_none());
    }

    // Martes 17 de noviembre de 2026, a las 10:30.
    fn now() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 11, 17, 10, 30, 0).unwrap()
    }

    fn back(text: &str) -> Option<(String, String, String)> {
        let span = past(&parse(&tokens(text))?, now())?;
        Some((span.from.format("%d %H:%M").to_string(), span.to.format("%d %H:%M").to_string(), span.label))
    }

    fn forth(text: &str) -> Option<String> {
        let (date, time) = ahead(&parse(&tokens(text))?, now())?;
        Some(match time {
            Some(t) => format!("{} {}", date.format("%d"), t.format("%H:%M")),
            None => date.format("%d").to_string(),
        })
    }

    #[test]
    fn reads() {
        let w = parse(&tokens("el jueves por la tarde")).unwrap();
        assert_eq!(w.day, Some(Day::Weekday { day: 3, shift: 0 }));
        assert_eq!(w.part, Some(Part::Afternoon));
        assert!(parse(&tokens("el jueves por la tarde de")).is_none());
        assert!(parse(&tokens("5")).is_none());
        assert!(parse(&tokens("hoy ayer")).is_none());
        assert_eq!(parse(&tokens("a las cinco y media")).unwrap().clock, Some((5, 30)));
        assert_eq!(parse(&tokens("a la una menos cuarto")).unwrap().clock, Some((12, 45)));
        assert_eq!(parse(&tokens("a las 16:30")).unwrap().clock, Some((16, 30)));
        assert_eq!(parse(&tokens("hace dos horas")).unwrap().ago, Some(7200));
        assert_eq!(parse(&tokens("en la última hora")).unwrap().last, Some(3600));
        assert_eq!(parse(&tokens("dentro de veinte minutos")).unwrap().within, Some(1200));
        assert_eq!(parse(&tokens("el 12 de noviembre")).unwrap().day, Some(Day::Date { day: 12, month: Some(11) }));
    }

    #[test]
    fn looking_back() {
        let s = |a: &str, b: &str, c: &str| Some((a.to_string(), b.to_string(), c.to_string()));
        assert_eq!(back("el jueves por la tarde"), s("12 15:00", "12 21:00", "el jueves 12 de noviembre por la tarde (15:00–21:00)"));
        assert_eq!(back("ayer"), s("16 00:00", "17 00:00", "ayer"));
        assert_eq!(back("esta mañana"), s("17 06:00", "17 10:30", "esta mañana (06:00–ahora)"));
        // Las tardes de hoy aún no han llegado: la de ayer.
        assert_eq!(back("por la tarde"), s("16 15:00", "16 21:00", "ayer por la tarde (15:00–21:00)"));
        assert_eq!(back("anoche"), s("16 21:00", "17 04:00", "anoche (21:00–04:00)"));
        assert_eq!(back("ayer a las cinco"), s("16 16:30", "16 17:30", "ayer hacia las 17:00"));
        assert_eq!(back("hace dos horas"), s("17 08:00", "17 09:00", "hace 2 h"));
        assert_eq!(back("el martes"), s("17 00:00", "17 10:30", "hoy"));
        assert_eq!(back("el martes pasado"), s("10 00:00", "11 00:00", "el martes 10 de noviembre"));
        assert_eq!(back("el 20"), s("20 00:00", "21 00:00", "el martes 20 de octubre"));
        assert_eq!(back("mañana"), None);
    }

    #[test]
    fn looking_ahead() {
        assert_eq!(forth("mañana a las cinco y media"), Some("18 17:30".into()));
        assert_eq!(forth("el jueves"), Some("19".into()));
        assert_eq!(forth("el martes"), Some("24".into()));
        assert_eq!(forth("a las 9"), Some("18 09:00".into()));
        assert_eq!(forth("a las 11"), Some("17 11:00".into()));
        assert_eq!(forth("en veinte minutos"), Some("17 10:50".into()));
        assert_eq!(forth("el viernes por la mañana"), Some("20 09:00".into()));
        assert_eq!(forth("a las 8 de la tarde"), Some("17 20:00".into()));
        assert_eq!(forth("ayer"), None);
    }
}
