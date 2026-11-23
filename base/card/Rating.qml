import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

Row {
    // La pieza (su Loader en Card: el bloque, su ancho y su tono) y la tarjeta.
    required property Loader block
    required property Item host

    spacing: 14

    Ink {
        anchors.verticalCenter: parent.verticalCenter
        strong: true
        font.pixelSize: 32
        color: block.tone
        text: Number(block.modelData.valor).toLocaleString(Qt.locale(), "f", 1)
    }

    Column {
        anchors.verticalCenter: parent.verticalCenter
        spacing: 2

        Row {
            spacing: 3

            Repeater {
                model: 5

                Item {
                    id: star

                    required property int index

                    width: 20
                    height: 22

                    Text {
                        text: "★"
                        color: Tokens.border
                        font.pixelSize: 20
                    }

                    Item {
                        width: star.width * Math.max(0, Math.min(1, block.modelData.valor - star.index))
                        height: star.height
                        clip: true

                        Text {
                            text: "★"
                            color: block.tone
                            font.pixelSize: 20
                        }
                    }
                }
            }
        }

        Ink {
            dim: true
            font.pixelSize: Tokens.smallSize
            text: block.modelData.texto
            visible: text !== ""
        }
    }
}
