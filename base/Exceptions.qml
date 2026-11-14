import QtQuick
import qs
import qs.state

// Lo que solo aparece cuando ocurre, y se va solo cuando deja de ocurrir.
// Cada excepción entra en la isla que la aloja: un icono del color de su
// trabajo y, si hay algo que decir, su texto.
Item {
    id: root

    readonly property bool any: Privacy.mic || Sound.micMuted || Sound.muted || Privacy.camera || Privacy.screen || Battery.low || Network.offline || Agenda.imminent || Countdown.running || Notifications.quiet
    // Cuánto de la separación tras la última se conserva: entera si detrás
    // viene algo, nada si la última cierra la isla.
    property real tail: 1
    readonly property real gap: Tokens.pad - 2
    readonly property real peak: Math.max(mic.progress, speaker.progress, camera.progress, screen.progress, battery.progress, network.progress, agenda.progress, countdown.progress, quiet.progress)

    implicitWidth: Math.max(0, row.width - gap * peak * (1 - tail))
    implicitHeight: Tokens.island

    component Exception: Reveal {
        property alias icon: label.icon
        property alias text: label.text
        property alias tone: label.tone

        anchors.verticalCenter: parent.verticalCenter
        gap: root.gap

        Label {
            id: label
        }
    }

    Row {
        id: row
        height: parent.height

        // El micro, tanto si alguien lo está oyendo como si está silenciado:
        // las dos cosas conviene tenerlas a la vista.
        Exception {
            id: mic
            shown: Privacy.mic || Sound.micMuted
            icon: Sound.micMuted ? "microphone-slash" : "microphone"
            tone: Tokens.critical
        }

        Exception {
            id: speaker
            shown: Sound.ready && Sound.muted
            icon: "speaker-x"
            tone: Tokens.critical
        }

        Exception {
            id: camera
            shown: Privacy.camera
            icon: "video-camera"
            tone: Tokens.critical
        }

        Exception {
            id: screen
            shown: Privacy.screen
            icon: "screencast"
            tone: Tokens.critical
        }

        Exception {
            id: battery
            shown: Battery.low
            icon: Battery.critical ? "battery-warning" : "battery-low"
            text: `${Battery.percent} %`
            tone: Battery.critical ? Tokens.critical : Tokens.attention
        }

        Exception {
            id: network
            shown: Network.offline
            icon: "wifi-slash"
            text: "sin red"
            tone: Tokens.attention
        }

        Exception {
            id: agenda
            shown: Agenda.imminent
            icon: "calendar-blank"
            text: Agenda.label
            tone: Tokens.attention
        }

        // No molestar: lo has pedido tú, va apagado.
        Exception {
            id: quiet
            shown: Notifications.quiet
            icon: "bell-slash"
            tone: Tokens.dim
        }

        // El temporizador no es un aviso: es algo tuyo que avanza.
        Exception {
            id: countdown
            shown: Countdown.running
            icon: "timer"
            text: Countdown.brief
            tone: Tokens.accent
        }
    }
}
