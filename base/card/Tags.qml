import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

Flow {
    // La pieza (su Loader en Card: el bloque, su ancho y su tono) y la tarjeta.
    required property Loader block
    required property Item host

    width: block.width
    spacing: 8

    Repeater {
        model: block.modelData.items

        Rectangle {
            id: chip

            required property var modelData
            readonly property color tone: host.tint(modelData.color, block.tone)

            width: chipRow.implicitWidth + 22
            height: 28
            radius: height / 2
            color: host.soft(tone, 0.16)
            border.color: host.soft(tone, 0.35)
            border.width: 1

            Row {
                id: chipRow
                anchors.centerIn: parent
                spacing: 5

                Icon {
                    anchors.verticalCenter: parent.verticalCenter
                    name: chip.modelData.icono
                    color: chip.tone
                    font.pixelSize: 15
                    visible: name !== ""
                }

                Ink {
                    anchors.verticalCenter: parent.verticalCenter
                    font.pixelSize: Tokens.smallSize
                    color: chip.tone
                    strong: true
                    text: chip.modelData.titulo
                }
            }
        }
    }
}
