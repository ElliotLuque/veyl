import QtQuick
import qs
import qs.state

// Lo que hay en el velo mientras el cerebro piensa la respuesta a algo que
// la gramática no entendía: tres puntos que respiran en ola, «Pensando» y lo
// preguntado, en gris. La respuesta llega luego como una notificación.
Item {
    id: root

    property bool running: false
    readonly property string asked: Daemon.thinking?.said ?? ""

    implicitWidth: Math.min(Tokens.card, row.implicitWidth + Tokens.pad * 2)
    implicitHeight: Tokens.island

    Row {
        id: row

        anchors.centerIn: parent
        spacing: 10

        Row {
            anchors.verticalCenter: parent.verticalCenter
            spacing: 4

            Repeater {
                model: 3

                Rectangle {
                    id: dot

                    required property int index

                    width: 6
                    height: 6
                    radius: 3
                    color: Tokens.accent
                    opacity: 0.25

                    // Una ola de izquierda a derecha; solo mientras se ve.
                    SequentialAnimation on opacity {
                        running: root.running
                        loops: Animation.Infinite
                        PauseAnimation { duration: dot.index * 160 }
                        NumberAnimation { to: 1; duration: 320; easing.type: Easing.OutSine }
                        NumberAnimation { to: 0.25; duration: 480; easing.type: Easing.InSine }
                        PauseAnimation { duration: (2 - dot.index) * 160 }
                    }
                }
            }
        }

        Ink {
            anchors.verticalCenter: parent.verticalCenter
            text: "Pensando"
        }

        Ink {
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(implicitWidth, Tokens.card - Tokens.pad * 2 - 120)
            dim: true
            elide: Text.ElideRight
            text: root.asked
            visible: text !== ""
        }
    }
}
