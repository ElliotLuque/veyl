import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

// Lo que probablemente quieras preguntar después: va directo al
// cerebro, que recuerda lo hablado, y la isla vuelve a «Pensando».
Column {
    // La pieza (su Loader en Card: el bloque, su ancho y su tono) y la tarjeta.
    required property Loader block
    required property Item host

    width: block.width
    spacing: 0

    Repeater {
        model: block.modelData.items

        Rectangle {
            id: ask

            required property var modelData

            width: block.width
            height: 34
            radius: Tokens.radius
            color: press.containsMouse ? Tokens.surface : "transparent"

            Behavior on color {
                ColorAnimation { duration: Tokens.fast }
            }

            Icon {
                id: mark
                x: 8
                anchors.verticalCenter: parent.verticalCenter
                name: "arrow-bend-down-right"
                color: press.containsMouse ? host.tone : host.soft(Tokens.dim, 0.6)
                font.pixelSize: Tokens.smallSize + 2
            }

            Ink {
                x: mark.x + mark.width + 10
                width: parent.width - x - 36
                anchors.verticalCenter: parent.verticalCenter
                elide: Text.ElideRight
                color: press.containsMouse ? Tokens.text : Tokens.dim
                text: ask.modelData.titulo
            }

            Icon {
                anchors.right: parent.right
                anchors.rightMargin: 10
                anchors.verticalCenter: parent.verticalCenter
                name: "arrow-up"
                color: host.tone
                opacity: press.containsMouse ? 1 : 0

                Behavior on opacity {
                    NumberAnimation { duration: Tokens.fast }
                }
            }

            MouseArea {
                id: press
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: host.follow(ask.modelData.titulo)
            }
        }
    }
}
