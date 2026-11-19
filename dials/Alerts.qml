import QtQuick
import qs
import qs.base
import qs.state

// Notificaciones: no molestar, cuánto se guardan y borrarlas.
Column {
    id: root

    property bool armed: false  // borrarlas todas pide una segunda pulsación

    spacing: 22

    Timer {
        id: disarm
        interval: 3000
        onTriggered: root.armed = false
    }

    Group {

        Option {
            glyph: Settings.quiet ? "bell-slash" : "bell"
            title: "No molestar"
            text: "Solo se muestran las notificaciones críticas"

            Toggle {
                on: Settings.quiet
                onToggled: on => Settings.quiet = on
            }
        }
    }

    Group {
        title: "Historial"

        Option {
            glyph: "clock"
            title: "Conservar durante"

            Select {
                width: 200
                options: [1, 3, 7, 14, 30, 90, 365, 0].map(value => ({ value, label: value === 0 ? "Siempre" : value === 1 ? "1 día" : value === 365 ? "1 año" : `${value} días` }))
                value: Settings.notificationDays
                onPicked: value => Settings.notificationDays = value
            }
        }

        Option {
            glyph: "trash"
            title: "Borrar historial"
            text: Notifications.notes.length === 1 ? "1 notificación" : `${Notifications.notes.length} notificaciones`

            Button {
                text: root.armed ? "Confirmar" : "Borrar"
                tone: root.armed ? Tokens.attention : "transparent"
                onPressed: {
                    if (!root.armed) {
                        root.armed = true;
                        disarm.restart();
                        return;
                    }
                    root.armed = false;
                    Notifications.clear();
                }
            }
        }
    }
}
