import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

Column {
    // La pieza (su Loader en Card: el bloque, su ancho y su tono) y la tarjeta.
    required property Loader block
    required property Item host

    width: block.width

    Repeater {
        model: block.modelData.items

        Item {
            id: step

            required property var modelData
            required property int index
            readonly property bool last: index === block.modelData.items.length - 1

            width: block.width
            implicitHeight: Math.max(28, words.implicitHeight) + (last ? 0 : 14)

            Rectangle {
                x: 13
                y: 30
                width: 2
                height: parent.height - 32
                color: Tokens.border
                visible: !step.last
            }

            Rectangle {
                width: 28
                height: 28
                radius: 14
                color: host.soft(block.tone, 0.16)

                Ink {
                    anchors.centerIn: parent
                    strong: true
                    color: block.tone
                    font.pixelSize: Tokens.smallSize
                    text: step.index + 1
                }
            }

            Column {
                id: words
                x: 40
                width: parent.width - x
                topPadding: 4
                spacing: 2

                Ink {
                    width: parent.width
                    wrapMode: Text.WordWrap
                    text: step.modelData.titulo
                }

                Ink {
                    width: parent.width
                    dim: true
                    wrapMode: Text.WordWrap
                    font.pixelSize: Tokens.smallSize
                    text: step.modelData.detalle
                    visible: text !== ""
                }
            }
        }
    }
}
