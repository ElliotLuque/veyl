import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

Flickable {
    id: rail

    // La pieza (su Loader en Card: el bloque, su ancho y su tono) y la tarjeta.
    required property Loader block
    required property Item host

    readonly property int shown: Math.min(block.modelData.items.length, 6)

    width: block.width
    height: 112
    contentWidth: band.width
    flickableDirection: Flickable.HorizontalFlick
    interactive: contentWidth > width
    clip: true

    Row {
        id: band
        spacing: 8

        Repeater {
            model: block.modelData.items

            Rectangle {
                id: slot

                required property var modelData
                required property int index

                width: Math.max(76, (rail.width - band.spacing * (rail.shown - 1)) / rail.shown)
                height: rail.height
                radius: Tokens.radius
                color: index === 0 ? host.soft(block.tone, 0.14) : Tokens.surface

                Column {
                    anchors.centerIn: parent
                    width: parent.width - 12
                    spacing: 6

                    Ink {
                        width: parent.width
                        horizontalAlignment: Text.AlignHCenter
                        elide: Text.ElideRight
                        dim: true
                        font.pixelSize: Tokens.smallSize
                        text: slot.modelData.titulo
                    }

                    Icon {
                        anchors.horizontalCenter: parent.horizontalCenter
                        name: slot.modelData.icono
                        color: block.tone
                        font.pixelSize: 24
                        visible: name !== ""
                    }

                    Ink {
                        width: parent.width
                        horizontalAlignment: Text.AlignHCenter
                        elide: Text.ElideRight
                        strong: true
                        text: slot.modelData.valor
                    }

                    Ink {
                        width: parent.width
                        horizontalAlignment: Text.AlignHCenter
                        elide: Text.ElideRight
                        dim: true
                        font.pixelSize: Tokens.tinySize
                        text: slot.modelData.detalle
                        visible: text !== ""
                    }
                }
            }
        }
    }
}
