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
    spacing: 10

    Repeater {
        model: block.modelData.items

        Item {
            id: entry

            required property var modelData
            readonly property color tone: host.tint(modelData.color, block.tone)

            width: block.width
            implicitHeight: Math.max(lead.height, words.implicitHeight)

            Item {
                id: lead
                width: thumb.ready ? 48 : 30
                height: thumb.ready ? 48 : 30
                anchors.verticalCenter: parent.verticalCenter

                Picture {
                    id: thumb
                    width: 48
                    tall: 48
                    radius: 8
                    source: entry.modelData.imagen
                }

                Badge {
                    visible: !thumb.ready
                    tone: entry.tone
                    icon: entry.modelData.icono || "arrow-right"
                }
            }

            Column {
                id: words
                x: lead.width + 12
                width: parent.width - x - value.width - 12
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2

                Ink {
                    width: parent.width
                    wrapMode: Text.WordWrap
                    text: entry.modelData.titulo
                }

                Ink {
                    width: parent.width
                    dim: true
                    font.pixelSize: Tokens.smallSize
                    wrapMode: Text.WordWrap
                    maximumLineCount: 3
                    elide: Text.ElideRight
                    text: entry.modelData.detalle
                    visible: text !== ""
                }
            }

            Ink {
                id: value
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                strong: true
                color: entry.tone
                text: entry.modelData.valor
            }
        }
    }
}
