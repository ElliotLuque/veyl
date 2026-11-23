import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

// Dos frente a frente: uno a cada lado (el primero en el acento) y,
// por cada cosa que se compara, dos barras que salen del centro.
Column {
    id: arena

    // La pieza (su Loader en Card: el bloque, su ancho y su tono) y la tarjeta.
    required property Loader block
    required property Item host

    readonly property var names: block.modelData.columnas
    readonly property var pictures: block.modelData.imagenes ?? ["", ""]
    readonly property var tones: [block.tone, Tokens.text]
    readonly property real edge: 104

    width: block.width
    spacing: 12

    Item {
        width: parent.width
        height: 52

        Repeater {
            model: 2

            Row {
                id: contender

                required property int index

                x: index === 0 ? 0 : arena.width - width
                anchors.verticalCenter: parent.verticalCenter
                layoutDirection: index === 0 ? Qt.LeftToRight : Qt.RightToLeft
                spacing: 10

                Item {
                    width: 44
                    height: 44
                    anchors.verticalCenter: parent.verticalCenter

                    Rectangle {
                        anchors.fill: parent
                        radius: width / 2
                        color: host.soft(arena.tones[contender.index], 0.2)
                        border.color: arena.tones[contender.index]
                        border.width: 1.5

                        Ink {
                            anchors.centerIn: parent
                            strong: true
                            color: arena.tones[contender.index]
                            text: arena.names[contender.index].charAt(0).toUpperCase()
                            visible: !face.ready
                        }
                    }

                    Picture {
                        id: face
                        anchors.fill: parent
                        tall: 44
                        radius: 22
                        source: arena.pictures[contender.index] ?? ""
                    }
                }

                Ink {
                    anchors.verticalCenter: parent.verticalCenter
                    width: Math.min(implicitWidth, arena.width / 2 - 80)
                    strong: true
                    elide: Text.ElideRight
                    color: arena.tones[contender.index]
                    text: arena.names[contender.index]
                }
            }
        }

        Ink {
            anchors.centerIn: parent
            dim: true
            strong: true
            font.pixelSize: Tokens.smallSize
            text: "vs"
        }
    }

    Repeater {
        model: block.modelData.items

        Column {
            id: bout

            required property var modelData
            readonly property var numbers: modelData.numeros ?? []
            readonly property real most: numbers.length === 2 ? Math.max(Math.abs(numbers[0]), Math.abs(numbers[1]), 1e-9) : 1
            // Sin decir quién gana, gana el mayor.
            readonly property int best: modelData.mejor >= 0 ? modelData.mejor : numbers.length === 2 && numbers[0] !== numbers[1] ? (numbers[0] > numbers[1] ? 0 : 1) : -1
            property real grow: 0
            NumberAnimation on grow { to: 1; duration: 800; easing.type: Easing.OutCubic }

            width: arena.width
            spacing: 4

            Ink {
                anchors.horizontalCenter: parent.horizontalCenter
                dim: true
                font.pixelSize: Tokens.smallSize
                text: bout.modelData.titulo
            }

            Item {
                width: parent.width
                height: Math.max(22, before.implicitHeight)

                Ink {
                    id: before
                    width: arena.edge
                    anchors.verticalCenter: parent.verticalCenter
                    wrapMode: Text.WordWrap
                    maximumLineCount: 2
                    elide: Text.ElideRight
                    strong: bout.best === 0
                    color: bout.best === 0 ? arena.tones[0] : Tokens.text
                    text: bout.modelData.valores[0]
                }

                Rectangle {
                    anchors.right: parent.horizontalCenter
                    anchors.rightMargin: 2
                    anchors.verticalCenter: parent.verticalCenter
                    width: (parent.width / 2 - arena.edge - 10) * Math.abs(bout.numbers[0] ?? 0) / bout.most * bout.grow
                    height: 8
                    radius: 4
                    color: bout.best === 1 ? host.soft(arena.tones[0], 0.35) : arena.tones[0]
                    visible: bout.numbers.length === 2
                }

                Rectangle {
                    anchors.left: parent.horizontalCenter
                    anchors.leftMargin: 2
                    anchors.verticalCenter: parent.verticalCenter
                    width: (parent.width / 2 - arena.edge - 10) * Math.abs(bout.numbers[1] ?? 0) / bout.most * bout.grow
                    height: 8
                    radius: 4
                    color: bout.best === 0 ? host.soft(arena.tones[1], 0.35) : arena.tones[1]
                    visible: bout.numbers.length === 2
                }

                Ink {
                    anchors.right: parent.right
                    width: arena.edge
                    anchors.verticalCenter: parent.verticalCenter
                    horizontalAlignment: Text.AlignRight
                    wrapMode: Text.WordWrap
                    maximumLineCount: 2
                    elide: Text.ElideRight
                    strong: bout.best === 1
                    color: bout.best === 1 ? arena.tones[1] : Tokens.text
                    text: bout.modelData.valores[1]
                }
            }
        }
    }
}
