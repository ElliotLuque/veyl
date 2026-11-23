import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

// Un resultado: los dos lados con su escudo (o su inicial) y el
// marcador en grande; el que va ganando, claro, y el otro, apagado.
Rectangle {
    id: board

    // La pieza (su Loader en Card: el bloque, su ancho y su tono) y la tarjeta.
    required property Loader block
    required property Item host

    readonly property var sides: block.modelData.items
    readonly property var goals: sides.map(side => parseFloat(String(side.valor).replace(",", ".")))
    readonly property int leader: goals[0] > goals[1] ? 0 : goals[1] > goals[0] ? 1 : -1

    width: block.width
    implicitHeight: 168
    radius: Tokens.radius
    color: Tokens.surface

    Ink {
        id: phase
        anchors.horizontalCenter: parent.horizontalCenter
        y: 14
        strong: true
        font.pixelSize: Tokens.smallSize
        color: block.tone
        text: block.modelData.texto
        visible: text !== ""
    }

    Repeater {
        model: board.sides

        Column {
            id: side

            required property var modelData
            required property int index

            x: index === 0 ? 14 : board.width - width - 14
            y: 40
            width: (board.width - 150) / 2
            spacing: 8

            Item {
                anchors.horizontalCenter: parent.horizontalCenter
                width: 56
                height: 56

                Rectangle {
                    anchors.fill: parent
                    radius: width / 2
                    color: host.soft(block.tone, 0.16)
                    border.color: host.soft(block.tone, 0.45)
                    border.width: 1
                    visible: !crest.ready

                    Ink {
                        anchors.centerIn: parent
                        strong: true
                        font.pixelSize: 22
                        color: block.tone
                        text: side.modelData.titulo.charAt(0).toUpperCase()
                    }
                }

                Image {
                    id: crest

                    readonly property bool ready: status === Image.Ready

                    anchors.fill: parent
                    source: side.modelData.imagen
                    sourceSize.width: 112
                    fillMode: Image.PreserveAspectFit
                    asynchronous: true
                    visible: ready
                }
            }

            Ink {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.WordWrap
                maximumLineCount: 2
                elide: Text.ElideRight
                strong: board.leader === side.index
                text: side.modelData.titulo
            }

            Ink {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                dim: true
                font.pixelSize: Tokens.tinySize
                elide: Text.ElideRight
                text: side.modelData.detalle
                visible: text !== ""
            }
        }
    }

    Row {
        anchors.horizontalCenter: parent.horizontalCenter
        y: 52
        spacing: 12

        Ink {
            strong: true
            font.pixelSize: 46
            color: board.leader === 1 ? Tokens.dim : Tokens.text
            text: board.sides[0].valor
        }

        Ink {
            anchors.verticalCenter: parent.verticalCenter
            dim: true
            font.pixelSize: 28
            text: "–"
        }

        Ink {
            strong: true
            font.pixelSize: 46
            color: board.leader === 0 ? Tokens.dim : Tokens.text
            text: board.sides[1].valor
        }
    }
}
