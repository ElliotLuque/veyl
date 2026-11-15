//! Los archivos abiertos, de lo que apuntan las aplicaciones en
//! `recently-used.xbel` (GTK y las que siguen la misma norma): cada una deja
//! la última vez que abrió cada archivo.

use std::path::PathBuf;

use chrono::DateTime;

use crate::config;

/// Un archivo que abrió una aplicación.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Used {
    pub path: String,
    pub app: String,
    /// ms desde 1970.
    pub time: i64,
}

pub fn path() -> PathBuf {
    let data = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| config::home().join(".local/share"));
    data.join("recently-used.xbel")
}

/// El valor de un atributo dentro de una etiqueta: `name="…"`.
fn attr(tag: &str, name: &str) -> Option<String> {
    let mut at = 0;
    while let Some(found) = tag[at..].find(name) {
        let start = at + found;
        at = start + name.len();
        // Que sea el nombre entero: « name=» y no «filename=».
        if start > 0 && !tag[..start].ends_with([' ', '\n', '\t', '\r']) {
            continue;
        }
        let rest = tag[at..].trim_start().strip_prefix('=')?.trim_start();
        let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        let value = &rest[1..];
        let end = value.find(quote)?;
        return Some(unescape(&value[..end]));
    }
    None
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// «file:///home/u/Mis%20cosas» → «/home/u/Mis cosas».
fn file(uri: &str) -> Option<String> {
    let rest = uri.strip_prefix("file://")?;
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(byte) = bytes.get(i + 1..i + 3).and_then(|hex| u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok())
        {
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    Some(String::from_utf8_lossy(&out).into_owned())
}

fn time(stamp: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(stamp).ok().map(|t| t.timestamp_millis())
}

/// Cada archivo con cada aplicación que lo abrió, y cuándo fue la última vez.
pub fn read(text: &str) -> Vec<Used> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("<bookmark ") {
        rest = &rest[start..];
        let end = rest.find("</bookmark>").unwrap_or(rest.len());
        let (block, after) = rest.split_at(end);
        rest = after.get(1..).unwrap_or("");
        let head = &block[..block.find('>').unwrap_or(block.len())];
        let Some(path) = attr(head, "href").and_then(|href| file(&href)) else {
            continue;
        };
        let mut apps = block;
        while let Some(at) = apps.find("<bookmark:application ") {
            apps = &apps[at..];
            let tag = &apps[..apps.find('>').unwrap_or(apps.len())];
            apps = &apps[tag.len()..];
            if let (Some(app), Some(when)) = (attr(tag, "name"), attr(tag, "modified").and_then(|m| time(&m))) {
                out.push(Used { path: path.clone(), app, time: when });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bookmarks() {
        let text = r#"<?xml version="1.0" encoding="UTF-8"?>
<xbel version="1.0" xmlns:bookmark="http://www.freedesktop.org/standards/desktop-bookmarks">
  <bookmark href="file:///home/u/Mis%20cosas/informe%C3%B1.pdf" added="2026-11-07T10:34:12.123Z" modified="2026-11-07T10:34:12Z" visited="2026-11-07T10:34:12Z">
    <info><metadata owner="http://freedesktop.org">
      <bookmark:applications>
        <bookmark:application name="Papers" exec="&apos;papers %u&apos;" modified="2026-11-07T10:34:12.123456Z" count="2"/>
        <bookmark:application name="Firefox" exec="&apos;firefox %u&apos;" modified="2026-11-06T08:00:00Z" count="1"/>
      </bookmark:applications>
    </metadata></info>
  </bookmark>
  <bookmark href="recent:///x" added="2026-11-07T10:34:12Z"></bookmark>
</xbel>"#;
        let used = read(text);
        assert_eq!(used.len(), 2);
        assert_eq!(used[0].path, "/home/u/Mis cosas/informeñ.pdf");
        assert_eq!(used[0].app, "Papers");
        assert_eq!(used[0].time, time("2026-11-07T10:34:12.123Z").unwrap());
        assert_eq!(used[1].app, "Firefox");
    }
}
