import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

// Casillas que se marcan al pulsarlas (los ingredientes, qué llevar).
Column {
    id: sheet

    // La pieza (su Loader en Card: el bloque, su ancho y su tono) y la tarjeta.
    required property Loader block
    required property Item host

    property int ticked: 0

    width: block.width
    spacing: 2

    Ink {
        anchors.right: parent.right
        dim: true
        font.pixelSize: Tokens.tinySize
        text: `${sheet.ticked} de ${block.modelData.items.length}`
    }

    Repeater {
        model: block.modelData.items

        Rectangle {
            id: entry

            required property var modelData
            property bool done: false
            onDoneChanged: {
                sheet.ticked += done ? 1 : -1;
                tick.requestPaint();
            }

            width: sheet.width
            height: Math.max(36, words.implicitHeight + 14)
            radius: Tokens.radius
            color: tap.containsMouse ? Tokens.surface : "transparent"

            Behavior on color {
                ColorAnimation { duration: Tokens.fast }
            }

            Rectangle {
                id: box
                x: 8
                anchors.verticalCenter: parent.verticalCenter
                width: 20
                height: 20
                radius: 10
                color: entry.done ? block.tone : "transparent"
                border.color: entry.done ? block.tone : host.soft(Tokens.text, 0.3)
                border.width: 1.5

                Behavior on color {
                    ColorAnimation { duration: Tokens.fast }
                }

                Canvas {
                    id: tick
                    anchors.fill: parent
                    onPaint: {
                        const ctx = getContext("2d");
                        ctx.reset();
                        if (!entry.done)
                            return;
                        ctx.strokeStyle = Tokens.bg;
                        ctx.lineWidth = 2.2;
                        ctx.lineCap = "round";
                        ctx.lineJoin = "round";
                        ctx.beginPath();
                        ctx.moveTo(5.5, 10.5);
                        ctx.lineTo(8.8, 13.8);
                        ctx.lineTo(14.8, 6.8);
                        ctx.stroke();
                    }
                }
            }

            Column {
                id: words
                x: box.x + box.width + 12
                width: parent.width - x - quantity.width - 18
                anchors.verticalCenter: parent.verticalCenter
                spacing: 1

                Ink {
                    width: parent.width
                    wrapMode: Text.WordWrap
                    font.strikeout: entry.done
                    color: entry.done ? Tokens.dim : Tokens.text
                    text: entry.modelData.titulo
                }

                Ink {
                    width: parent.width
                    dim: true
                    font.pixelSize: Tokens.smallSize
                    wrapMode: Text.WordWrap
                    text: entry.modelData.detalle
                    visible: text !== ""
                }
            }

            Ink {
                id: quantity
                anchors.right: parent.right
                anchors.rightMargin: 10
                anchors.verticalCenter: parent.verticalCenter
                strong: true
                color: entry.done ? Tokens.dim : block.tone
                text: entry.modelData.valor
            }

            MouseArea {
                id: tap
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: entry.done = !entry.done
            }
        }
    }
}
