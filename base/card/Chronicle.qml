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
            id: moment

            required property var modelData
            required property int index
            readonly property bool last: index === block.modelData.items.length - 1

            width: block.width
            implicitHeight: words.implicitHeight + (last ? 0 : 14)

            Rectangle {
                x: 5
                y: 6
                width: 2
                height: parent.height
                color: Tokens.border
                visible: !moment.last
            }

            Rectangle {
                y: 4
                width: 12
                height: 12
                radius: 6
                color: moment.last ? block.tone : Tokens.plate
                border.color: block.tone
                border.width: 2
            }

            Column {
                id: words
                x: 26
                width: parent.width - x
                spacing: 2

                Ink {
                    strong: true
                    color: block.tone
                    font.pixelSize: Tokens.smallSize
                    text: moment.modelData.valor
                    visible: text !== ""
                }

                Ink {
                    width: parent.width
                    wrapMode: Text.WordWrap
                    text: moment.modelData.titulo
                }

                Ink {
                    width: parent.width
                    dim: true
                    wrapMode: Text.WordWrap
                    font.pixelSize: Tokens.smallSize
                    text: moment.modelData.detalle
                    visible: text !== ""
                }
            }
        }
    }
}
