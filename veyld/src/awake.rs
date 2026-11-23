//! Lo que pide que no se bloquee: org.freedesktop.ScreenSaver en el bus de
//! sesión, el «no te duermas» al que llaman los navegadores mientras ponen un
//! vídeo (y el portal, por los de Flatpak). Hyprland solo atiende el del
//! propio Wayland (idle-inhibit); este lo atiende veyld y se lo cuenta a
//! Veyl, que no bloquea mientras haya alguno.
//!
//! Cada petición vale hasta que su programa la suelta (UnInhibit) o se va
//! del bus: un navegador que se cierra a medio vídeo no deja nada colgado.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use zbus::blocking::Connection;
use zbus::blocking::fdo::DBusProxy;
use zbus::fdo::RequestNameFlags;
use zbus::message::Header;

use crate::daemon::Event;

const NAME: &str = "org.freedesktop.ScreenSaver";
// Unos llaman a una y otros a la otra.
const PATHS: [&str; 2] = ["/org/freedesktop/ScreenSaver", "/ScreenSaver"];

/// Uno que pide que no se bloquee.
#[derive(Clone, Debug, PartialEq)]
pub struct Hold {
    /// Quién: «Firefox», «Chromium».
    pub app: String,
    /// Por qué, si lo dice: «video-playing».
    pub reason: String,
}

#[derive(Default)]
struct Holds {
    next: AtomicU32,
    /// Por su número: quién lo pidió en el bus y lo pedido.
    all: Mutex<HashMap<u32, (String, Hold)>>,
}

impl Holds {
    fn listing(&self) -> Vec<Hold> {
        let all = self.all.lock().unwrap();
        let mut cookies: Vec<&u32> = all.keys().collect();
        cookies.sort();
        cookies.into_iter().map(|cookie| all[cookie].1.clone()).collect()
    }
}

struct Server {
    holds: Arc<Holds>,
    tx: Sender<Event>,
}

impl Server {
    fn changed(&self) {
        let _ = self.tx.send(Event::Held(self.holds.listing()));
    }
}

#[zbus::interface(name = "org.freedesktop.ScreenSaver")]
impl Server {
    fn inhibit(&self, #[zbus(header)] header: Header<'_>, application_name: String, reason_for_inhibit: String) -> u32 {
        let cookie = self.holds.next.fetch_add(1, Ordering::Relaxed) + 1;
        let sender = header.sender().map(|name| name.to_string()).unwrap_or_default();
        let hold = Hold { app: application_name, reason: reason_for_inhibit };
        self.holds.all.lock().unwrap().insert(cookie, (sender, hold));
        self.changed();
        cookie
    }

    fn un_inhibit(&self, cookie: u32) {
        if self.holds.all.lock().unwrap().remove(&cookie).is_some() {
            self.changed();
        }
    }

    // Lo demás de la interfaz, para quien lo pregunte: el bloqueo es de Veyl.
    fn get_active(&self) -> bool {
        false
    }

    fn simulate_user_activity(&self) {}
}

/// Atiende hasta que se cae el bus; vuelve con el motivo. Si otro tiene el
/// nombre, espera en la cola.
pub fn watch(tx: Sender<Event>) -> String {
    match serve(tx) {
        Ok(()) => "el bus se ha cerrado".into(),
        Err(e) => e.to_string(),
    }
}

fn serve(tx: Sender<Event>) -> zbus::Result<()> {
    let holds = Arc::new(Holds::default());
    let mut builder = zbus::blocking::connection::Builder::session()?;
    for path in PATHS {
        builder = builder.serve_at(path, Server { holds: holds.clone(), tx: tx.clone() })?;
    }
    let connection: Connection = builder.build()?;
    connection.request_name_with_flags(NAME, RequestNameFlags::AllowReplacement.into())?;
    // Los que se van del bus sueltan lo suyo.
    let bus = DBusProxy::new(&connection)?;
    for change in bus.receive_name_owner_changed()? {
        let Ok(args) = change.args() else {
            continue;
        };
        if args.new_owner().is_some() || !args.name().starts_with(':') {
            continue;
        }
        let gone = args.name().to_string();
        let dropped = {
            let mut all = holds.all.lock().unwrap();
            let before = all.len();
            all.retain(|_, (sender, _)| *sender != gone);
            all.len() != before
        };
        if dropped {
            let _ = tx.send(Event::Held(holds.listing()));
        }
    }
    Ok(())
}
