//! El servidor de notificaciones (org.freedesktop.Notifications) en el bus
//! de sesión. Lo que llega se traduce a una nota y se manda al hilo del
//! daemon, que es quien la guarda y la enseña (`notes.rs`); de vuelta, se le
//! cuenta a cada programa qué ha pasado con las suyas.
//!
//! Si otro servidor tiene el nombre, veyld espera en la cola y lo toma
//! cuando ese se va.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::Sender;

use zbus::blocking::Connection;
use zbus::fdo::RequestNameFlags;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::{OwnedValue, Value};

use crate::daemon::{Event, now};
use crate::notes::{Action, Note};

const NAME: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";

/// El sonido que pide al llegar.
#[derive(Clone, Debug, PartialEq)]
pub enum Sound {
    File(String),
    Name(String),
}

/// Una que llega, ya traducida.
#[derive(Clone, Debug)]
pub struct Incoming {
    pub note: Note,
    pub sound: Option<Sound>,
}

struct Server {
    tx: Sender<Event>,
    next: AtomicU32,
    pictures: PathBuf,
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Server {
    fn get_capabilities(&self) -> Vec<&'static str> {
        vec!["actions", "action-icons", "body", "body-hyperlinks", "body-images", "body-markup", "icon-static", "inline-reply", "persistence", "sound"]
    }

    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
    ) -> u32 {
        let id = if replaces_id != 0 { replaces_id } else { self.next.fetch_add(1, Ordering::Relaxed).max(1) };
        let mut incoming = read(id, app_name, app_icon, summary, body, &actions, &hints, expire_timeout);
        if let Some(picture) = picture(&hints) {
            incoming.note.image = save(&self.pictures, id, &picture).unwrap_or_default();
        }
        let _ = self.tx.send(Event::Notified(Box::new(incoming)));
        id
    }

    fn close_notification(&self, id: u32) {
        let _ = self.tx.send(Event::Withdrawn(id));
    }

    fn get_server_information(&self) -> (&'static str, &'static str, &'static str, &'static str) {
        ("veyl", "veyl", env!("CARGO_PKG_VERSION"), "1.2")
    }

    #[zbus(signal)]
    async fn notification_closed(emitter: &SignalEmitter<'_>, id: u32, reason: u32) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn action_invoked(emitter: &SignalEmitter<'_>, id: u32, action_key: &str) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn notification_replied(emitter: &SignalEmitter<'_>, id: u32, text: &str) -> zbus::Result<()>;
}

pub struct Bus {
    connection: Connection,
}

impl Bus {
    /// Se pone en el bus, repartiendo ids desde `first`; las imágenes que
    /// llegan en datos se guardan en `pictures`.
    pub fn start(tx: Sender<Event>, first: u32, pictures: PathBuf) -> zbus::Result<Bus> {
        let server = Server { tx, next: AtomicU32::new(first.max(1)), pictures };
        let connection = zbus::blocking::connection::Builder::session()?.serve_at(PATH, server)?.build()?;
        // Sin «no hacer cola»: si hay otro servidor, se espera turno.
        let reply = connection.request_name_with_flags(NAME, RequestNameFlags::AllowReplacement.into())?;
        eprintln!("veyld: notificaciones: {reply:?}");
        Ok(Bus { connection })
    }

    fn emit(&self, signal: &str, body: &(impl serde::Serialize + zbus::zvariant::DynamicType)) {
        if let Err(e) = self.connection.emit_signal(None::<&str>, PATH, NAME, signal, body) {
            eprintln!("veyld: {signal}: {e}");
        }
    }

    pub fn closed(&self, id: u32, reason: u32) {
        self.emit("NotificationClosed", &(id, reason));
    }

    pub fn invoked(&self, id: u32, key: &str) {
        self.emit("ActionInvoked", &(id, key));
    }

    pub fn replied(&self, id: u32, text: &str) {
        self.emit("NotificationReplied", &(id, text));
    }
}

fn text(hints: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    match hints.get(key).map(|v| &**v) {
        Some(Value::Str(s)) => Some(s.to_string()).filter(|s| !s.is_empty()),
        Some(Value::ObjectPath(p)) => Some(p.to_string()),
        _ => None,
    }
}

fn number(hints: &HashMap<String, OwnedValue>, key: &str) -> Option<i64> {
    match hints.get(key).map(|v| &**v)? {
        Value::U8(n) => Some(*n as i64),
        Value::I16(n) => Some(*n as i64),
        Value::U16(n) => Some(*n as i64),
        Value::I32(n) => Some(*n as i64),
        Value::U32(n) => Some(*n as i64),
        Value::I64(n) => Some(*n),
        Value::U64(n) => Some(*n as i64),
        _ => None,
    }
}

fn flag(hints: &HashMap<String, OwnedValue>, key: &str) -> bool {
    match hints.get(key).map(|v| &**v) {
        Some(Value::Bool(b)) => *b,
        _ => number(hints, key).is_some_and(|n| n != 0),
    }
}

/// Traduce lo que llega por D-Bus a una nota (sin la imagen en datos).
#[allow(clippy::too_many_arguments)]
pub fn read(
    id: u32,
    app: String,
    icon: String,
    summary: String,
    body: String,
    actions: &[String],
    hints: &HashMap<String, OwnedValue>,
    timeout: i32,
) -> Incoming {
    // Las acciones llegan en pares: identificador y texto.
    let pairs: Vec<Action> = actions.chunks(2).map(|pair| Action { id: pair[0].clone(), text: pair.get(1).cloned().unwrap_or_default() }).collect();
    let urgency = match number(hints, "urgency") {
        Some(0) => "low",
        Some(2) => "critical",
        _ => "normal",
    };
    let sound = if flag(hints, "suppress-sound") {
        None
    } else if let Some(file) = text(hints, "sound-file") {
        Some(Sound::File(file))
    } else {
        text(hints, "sound-name").map(Sound::Name)
    };
    let note = Note {
        id,
        app,
        icon,
        entry: text(hints, "desktop-entry").unwrap_or_default(),
        image: text(hints, "image-path").or_else(|| text(hints, "image_path")).unwrap_or_default(),
        summary,
        body,
        primary: pairs.iter().any(|a| a.id == "default"),
        reply: pairs.iter().any(|a| a.id == "inline-reply"),
        actions: pairs.into_iter().filter(|a| a.id != "default" && a.id != "inline-reply").collect(),
        action_icons: flag(hints, "action-icons"),
        urgency: urgency.into(),
        category: text(hints, "category").unwrap_or_default(),
        progress: number(hints, "value").map(|v| v.clamp(0, 100) as i32).unwrap_or(-1),
        placeholder: text(hints, "x-kde-reply-placeholder-text").unwrap_or_default(),
        at: now(),
        live: true,
        popup: false,
        resident: flag(hints, "resident"),
        transient: flag(hints, "transient"),
        timeout,
    };
    Incoming { note, sound }
}

/// Una imagen en datos: ancho, alto, bytes por fila, si tiene alfa y los
/// píxeles (8 bits por canal).
#[derive(Debug, PartialEq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub stride: usize,
    pub alpha: bool,
    pub channels: usize,
    pub data: Vec<u8>,
}

fn picture(hints: &HashMap<String, OwnedValue>) -> Option<Picture> {
    let value = ["image-data", "image_data", "icon_data"].iter().find_map(|key| hints.get(*key))?;
    let Value::Structure(structure) = &**value else { return None };
    let fields = structure.fields();
    let int = |i: usize| match fields.get(i) {
        Some(Value::I32(n)) => Some(*n),
        _ => None,
    };
    let alpha = matches!(fields.get(3), Some(Value::Bool(true)));
    let data: Vec<u8> = match fields.get(6)? {
        Value::Array(array) => array.iter().filter_map(|v| if let Value::U8(b) = v { Some(*b) } else { None }).collect(),
        _ => return None,
    };
    let (width, height, stride, bits, channels) = (int(0)?, int(1)?, int(2)?, int(4)?, int(5)?);
    if width <= 0 || height <= 0 || stride <= 0 || bits != 8 || !(3..=4).contains(&channels) {
        return None;
    }
    Some(Picture { width: width as u32, height: height as u32, stride: stride as usize, alpha, channels: channels as usize, data })
}

/// La guarda en un PNG con un nombre nuevo cada vez (Veyl guarda en caché
/// lo que ya ha leído) y borra las anteriores de la misma nota.
fn save(dir: &Path, id: u32, picture: &Picture) -> Option<String> {
    let Picture { width, height, stride, channels, data, .. } = picture;
    let row = *width as usize * channels;
    if *stride < row || data.len() < stride * (*height as usize - 1) + row {
        return None;
    }
    let mut pixels = Vec::with_capacity(row * *height as usize);
    for y in 0..*height as usize {
        pixels.extend_from_slice(&data[y * stride..y * stride + row]);
    }
    fs::create_dir_all(dir).ok()?;
    forget_pictures(dir, id);
    let path = dir.join(format!("{id}-{}.png", now()));
    let file = fs::File::create(&path).ok()?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), *width, *height);
    encoder.set_color(if *channels == 4 { png::ColorType::Rgba } else { png::ColorType::Rgb });
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().ok()?;
    writer.write_image_data(&pixels).ok()?;
    Some(format!("file://{}", path.display()))
}

/// Borra las imágenes guardadas de una nota.
pub fn forget_pictures(dir: &Path, id: u32) {
    let prefix = format!("{id}-");
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with(&prefix) {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}

/// Borra las imágenes de las notas que ya no están.
pub fn sweep_pictures(dir: &Path, keep: &[u32]) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let id = name.split('-').next().and_then(|n| n.parse::<u32>().ok());
            if id.is_none_or(|id| !keep.contains(&id)) {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}

/// El sonido que pide: un archivo o un nombre del tema de sonidos.
pub fn play(sound: &Sound) {
    let file = match sound {
        Sound::File(file) => Some(PathBuf::from(file.strip_prefix("file://").unwrap_or(file))),
        Sound::Name(name) if !name.contains('/') => {
            let dirs = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
            let home = crate::config::home().join(".local/share");
            std::iter::once(home)
                .chain(dirs.split(':').filter(|d| !d.is_empty()).map(PathBuf::from))
                .flat_map(|dir| ["oga", "ogg", "wav"].map(|ext| dir.join(format!("sounds/freedesktop/stereo/{name}.{ext}"))))
                .find(|path| path.is_file())
        }
        Sound::Name(_) => None,
    };
    if let Some(file) = file {
        crate::orders::detach(&["pw-play", &file.to_string_lossy()]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::{Array, StructureBuilder};

    fn hints(pairs: Vec<(&str, Value<'static>)>) -> HashMap<String, OwnedValue> {
        pairs.into_iter().map(|(k, v)| (k.to_string(), v.try_into().unwrap())).collect()
    }

    #[test]
    fn reads_actions_urgency_progress_and_reply() {
        let actions: Vec<String> = ["default", "Abrir", "inline-reply", "Responder", "later", "Luego"].map(String::from).to_vec();
        let h = hints(vec![
            ("urgency", Value::U8(2)),
            ("value", Value::I32(140)),
            ("desktop-entry", Value::from("firefox")),
            ("category", Value::from("im.received")),
            ("x-kde-reply-placeholder-text", Value::from("Escribe")),
            ("resident", Value::Bool(true)),
            ("sound-name", Value::from("message-new-instant")),
        ]);
        let incoming = read(5, "App".into(), "firefox".into(), "Hola".into(), "Cuerpo".into(), &actions, &h, -1);
        let note = incoming.note;
        assert_eq!(note.urgency, "critical");
        assert_eq!(note.progress, 100);
        assert!(note.primary && note.reply && note.resident);
        assert_eq!(note.actions, vec![Action { id: "later".into(), text: "Luego".into() }]);
        assert_eq!(note.entry, "firefox");
        assert_eq!(note.placeholder, "Escribe");
        assert_eq!(incoming.sound, Some(Sound::Name("message-new-instant".into())));
        let quiet = read(
            6,
            "".into(),
            "".into(),
            "".into(),
            "".into(),
            &[],
            &hints(vec![("suppress-sound", Value::Bool(true)), ("sound-file", Value::from("/x.oga"))]),
            0,
        );
        assert_eq!(quiet.sound, None);
        assert_eq!(quiet.note.urgency, "normal");
        assert_eq!(quiet.note.progress, -1);
    }

    #[test]
    fn image_data_becomes_a_png_and_is_swept() {
        let dir = std::env::temp_dir().join(format!("veyld-pictures-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        // 2×2 RGB con un byte de relleno al final de cada fila.
        let bytes: Vec<u8> = vec![255, 0, 0, 0, 255, 0, 9, 0, 0, 255, 255, 255, 255, 9];
        let structure = StructureBuilder::new()
            .add_field(2i32)
            .add_field(2i32)
            .add_field(7i32)
            .add_field(false)
            .add_field(8i32)
            .add_field(3i32)
            .add_field(Array::from(bytes))
            .build()
            .unwrap();
        let h = hints(vec![("image-data", Value::Structure(structure))]);
        let picture = picture(&h).unwrap();
        assert_eq!((picture.width, picture.height, picture.stride, picture.channels), (2, 2, 7, 3));
        let url = save(&dir, 4, &picture).unwrap();
        let path = PathBuf::from(url.strip_prefix("file://").unwrap());
        let decoder = png::Decoder::new(std::io::BufReader::new(fs::File::open(&path).unwrap()));
        let mut reader = decoder.read_info().unwrap();
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut pixels).unwrap();
        assert_eq!(&pixels[..12], &[255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255]);
        sweep_pictures(&dir, &[]);
        assert!(!path.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn short_image_data_is_refused() {
        let picture = Picture { width: 4, height: 4, stride: 12, alpha: false, channels: 3, data: vec![0; 10] };
        assert_eq!(save(&std::env::temp_dir(), 1, &picture), None);
    }
}
