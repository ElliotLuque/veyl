import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

// Colores: muestras grandes con su código; pulsar una lo copia.
Row {
    id: swatches

    // La pieza (su Loader en Card: el bloque, su ancho y su tono) y la tarjeta.
    required property Loader block
    required property Item host

    readonly property int count: block.modelData.items.length

    width: block.width
    spacing: 8

    Repeater {
        model: block.modelData.items

        Column {
            id: swatch

            required property var modelData
            readonly property color paint: modelData.valor
            readonly property bool light: 0.299 * paint.r + 0.587 * paint.g + 0.114 * paint.b > 0.6
            property bool copied: false

            width: (swatches.width - swatches.spacing * (swatches.count - 1)) / swatches.count
            spacing: 6

            Rectangle {
                width: parent.width
                height: 76
                radius: Tokens.radius
                color: swatch.paint
                border.color: host.soft(Tokens.text, 0.12)
                border.width: 1

                Ink {
                    anchors.centerIn: parent
                    font.pixelSize: Tokens.smallSize
                    strong: true
                    color: swatch.light ? "#111111" : "#ffffff"
                    text: "Copiado"
                    opacity: swatch.copied ? 1 : 0

                    Behavior on opacity {
                        NumberAnimation { duration: Tokens.fast }
                    }
                }

                Timer {
                    running: swatch.copied
                    interval: 1200
                    onTriggered: swatch.copied = false
                }

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        Quickshell.clipboardText = swatch.modelData.valor;
                        swatch.copied = true;
                    }
                }
            }

            Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                font.family: "JetBrains Mono"
                font.pixelSize: 11
                color: Tokens.dim
                text: swatch.modelData.valor
            }

            Ink {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                elide: Text.ElideRight
                font.pixelSize: Tokens.smallSize
                text: swatch.modelData.titulo
                visible: text !== ""
            }
        }
    }
}
