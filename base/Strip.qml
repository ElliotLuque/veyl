import QtQuick
import Quickshell.Hyprland
import qs
import qs.state

// La tira de un nivel en miniatura: sus ventanas de izquierda a derecha y,
// enmarcado, el trozo que cabe en pantalla. Lo que queda fuera del marco se
// ve más tenue.
Item {
    id: root

    property HyprlandWorkspace workspace
    property HyprlandMonitor monitor
    property real unit: Tokens.tile  // alto de la pantalla en miniatura
    property real reach: 1.5  // pantallas que se enseñan, como mucho, a cada lado
    property bool lit: true   // resalta la ventana activa

    readonly property var tiles: Windows.tiles(workspace)
    readonly property real screen: Math.round(unit * (monitor ? monitor.width / monitor.height : 16 / 9))
    // Cuánto sobresale la tira por cada lado, en pantallas…
    readonly property real lead: Math.min(reach, Math.max(0, ...tiles.map(t => -t.x)))
    readonly property real trail: Math.min(reach, Math.max(0, ...tiles.map(t => t.x + t.w - 1)))
    // …y cuánto sitio se le deja: el suyo, o el que haga falta para que
    // varias tiras apiladas compartan marco.
    property real before: lead
    property real after: trail
    readonly property int frame: 3  // aire entre las ventanas y el marco

    implicitWidth: (before + 1 + after) * screen + frame * 2
    implicitHeight: unit + frame * 2
    clip: true

    Behavior on before {
        NumberAnimation { duration: Tokens.veil; easing.type: Easing.OutCubic }
    }

    Behavior on after {
        NumberAnimation { duration: Tokens.veil; easing.type: Easing.OutCubic }
    }

    Repeater {
        model: root.tiles.length

        Rectangle {
            required property int index
            readonly property var tile: root.tiles[index] ?? null
            readonly property bool inside: !!tile && tile.x + tile.w > 0.02 && tile.x < 0.98

            x: root.frame + (root.before + (tile?.x ?? 0)) * root.screen + 0.5
            y: root.frame + (tile?.y ?? 0) * root.unit + 0.5
            width: Math.max(2, (tile?.w ?? 0) * root.screen - 1)
            height: Math.max(2, (tile?.h ?? 0) * root.unit - 1)
            radius: Tokens.small
            color: Qt.alpha(Tokens.text, tile?.active && root.lit ? 0.95 : inside ? 0.36 : 0.16)

            Behavior on x {
                NumberAnimation { duration: Tokens.veil; easing.type: Easing.OutCubic }
            }

            Behavior on width {
                NumberAnimation { duration: Tokens.veil; easing.type: Easing.OutCubic }
            }

            Behavior on color {
                ColorAnimation { duration: Tokens.fast }
            }
        }
    }

    // El marco: lo que se ve en pantalla
    Rectangle {
        x: root.before * root.screen
        width: root.screen + root.frame * 2
        height: parent.height
        radius: Tokens.small + root.frame
        color: "transparent"
        border.width: Tokens.line
        border.color: root.lit ? Tokens.accent : Qt.alpha(Tokens.text, 0.2)
    }
}
