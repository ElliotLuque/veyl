//! Si estás: el compositor avisa (ext-idle-notify) cuando lleva un rato sin
//! que toques nada y cuando vuelves. Respeta lo que impide el reposo (un
//! vídeo a pantalla completa, por ejemplo): eso cuenta como estar.

use std::sync::mpsc::Sender;
use std::time::Duration;

use wayland_client::protocol::{wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols::ext::idle_notify::v1::client::ext_idle_notification_v1::{self, ExtIdleNotificationV1};
use wayland_protocols::ext::idle_notify::v1::client::ext_idle_notifier_v1::ExtIdleNotifierV1;

use crate::daemon::Event;

/// Tiempo sin tocar nada a partir del cual no estás.
pub const AWAY: Duration = Duration::from_secs(120);

struct State {
    seat: Option<wl_seat::WlSeat>,
    notifier: Option<ExtIdleNotifierV1>,
    tx: Sender<Event>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(state: &mut Self, registry: &wl_registry::WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { name, interface, .. } = event {
            match interface.as_str() {
                "wl_seat" if state.seat.is_none() => state.seat = Some(registry.bind(name, 1, qh, ())),
                "ext_idle_notifier_v1" => state.notifier = Some(registry.bind(name, 1, qh, ())),
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(_: &mut Self, _: &wl_seat::WlSeat, _: wl_seat::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ExtIdleNotifierV1, ()> for State {
    fn event(_: &mut Self, _: &ExtIdleNotifierV1, _: <ExtIdleNotifierV1 as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ExtIdleNotificationV1, ()> for State {
    fn event(state: &mut Self, _: &ExtIdleNotificationV1, event: ext_idle_notification_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        let away = match event {
            ext_idle_notification_v1::Event::Idled => true,
            ext_idle_notification_v1::Event::Resumed => false,
            _ => return,
        };
        let _ = state.tx.send(Event::Idle(away));
    }
}

/// Escucha hasta que se cierra la conexión; vuelve con el motivo.
pub fn watch(tx: Sender<Event>) -> String {
    let conn = match Connection::connect_to_env() {
        Ok(conn) => conn,
        Err(e) => return format!("sin Wayland: {e}"),
    };
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    conn.display().get_registry(&qh, ());
    let mut state = State { seat: None, notifier: None, tx };
    if let Err(e) = queue.roundtrip(&mut state) {
        return format!("Wayland: {e}");
    }
    let (Some(seat), Some(notifier)) = (state.seat.clone(), state.notifier.clone()) else {
        return "el compositor no avisa de la inactividad (ext-idle-notify)".into();
    };
    let _notification = notifier.get_idle_notification(AWAY.as_millis() as u32, &seat, &qh, ());
    loop {
        if let Err(e) = queue.blocking_dispatch(&mut state) {
            return format!("Wayland: {e}");
        }
    }
}
