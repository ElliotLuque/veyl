import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

// Barras horizontales con nombre.
Column {
    // La pieza (su Loader en Card: el bloque, su ancho y su tono) y la tarjeta.
    required property Loader block
    required property Item host

    width: block.width
    spacing: 12

    Repeater {
        model: block.modelData.items

        Column {
            id: lane

            required property var modelData
            required property int index
            readonly property color tone: host.tint(modelData.color, block.tone)
            readonly property real share: Math.max(0, Math.min(1, modelData.numero / modelData.maximo))
            property real grow: 0
            NumberAnimation on grow { to: 1; duration: 800; easing.type: Easing.OutCubic }

            width: block.width
            spacing: 6

            Item {
                width: parent.width
                implicitHeight: label.implicitHeight

                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    width: 8
                    height: 8
                    radius: 4
                    color: lane.tone
                }

                Ink {
                    id: label
                    x: 16
                    width: parent.width - x - amount.width - 10
                    elide: Text.ElideRight
                    text: lane.modelData.titulo
                }

                Ink {
                    id: amount
                    anchors.right: parent.right
                    strong: true
                    text: lane.modelData.valor || (lane.modelData.maximo === 100 ? Math.round(lane.modelData.numero) + " %" : host.figure(lane.modelData.numero, ""))
                }
            }

            Rectangle {
                width: parent.width
                height: 8
                radius: 4
                color: host.soft(lane.tone, 0.14)

                Rectangle {
                    width: Math.max(height, parent.width * lane.share * lane.grow)
                    height: parent.height
                    radius: parent.radius
                    color: lane.tone
                }
            }
        }
    }
}
