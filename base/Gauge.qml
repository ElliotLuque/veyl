import QtQuick
import qs
import qs.state

// Los niveles del monitor, apilados: una marca por nivel, el primero arriba.
// La del nivel actual es la larga y clara; las de niveles vacíos casi no se
// ven.
Item {
    id: root

    property var levels: []  // ver Contexts.levels
    // Con muchos niveles las marcas adelgazan para caber en la isla.
    readonly property int step: Math.max(2, Math.min(8, Math.floor((Tokens.island - 8) / Math.max(1, levels.length))))
    readonly property int thick: Math.max(1, Math.floor(step / 2))

    implicitWidth: Tokens.mark
    implicitHeight: Math.max(0, levels.length * step - (step - thick))

    Repeater {
        model: root.levels.length

        Rectangle {
            required property int index
            readonly property var level: root.levels[index] ?? null
            readonly property bool current: level?.current ?? false
            readonly property bool busy: (level?.workspace?.toplevels.values.length ?? 0) > 0

            y: index * root.step
            width: current ? Tokens.mark : Tokens.mark * 0.6
            height: root.thick
            radius: height / 2
            color: current ? Tokens.accent : busy ? Tokens.dim : Qt.alpha(Tokens.dim, 0.35)

            Behavior on width {
                NumberAnimation { duration: Tokens.veil; easing.type: Easing.OutCubic }
            }

            Behavior on color {
                ColorAnimation { duration: Tokens.fast }
            }
        }
    }
}
