import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

Rectangle {
    id: sheet

    // La pieza (su Loader en Card: el bloque, su ancho y su tono) y la tarjeta.
    required property Loader block
    required property Item host

    readonly property var heads: block.modelData.columnas
    readonly property int wide: Math.max(heads.length, 1 + Math.max.apply(null, block.modelData.items.map(r => r.valores.length)))

    width: block.width
    implicitHeight: rows.implicitHeight + 8
    radius: Tokens.radius
    color: Tokens.surface

    Column {
        id: rows
        y: 4
        width: parent.width

        Item {
            width: parent.width
            height: 32
            visible: sheet.heads.some(h => h !== "")

            Row {
                x: 14
                width: parent.width - 28
                anchors.verticalCenter: parent.verticalCenter

                Repeater {
                    model: sheet.wide

                    Ink {
                        required property int index
                        width: parent.width / sheet.wide
                        horizontalAlignment: index ? Text.AlignRight : Text.AlignLeft
                        elide: Text.ElideRight
                        strong: index > 0
                        dim: true
                        font.pixelSize: Tokens.smallSize
                        text: sheet.heads[index] ?? ""
                    }
                }
            }
        }

        Repeater {
            model: block.modelData.items

            Rectangle {
                id: line

                required property var modelData
                required property int index

                width: rows.width
                height: Math.max(34, cells.implicitHeight + 14)
                color: index % 2 ? "transparent" : host.soft(Tokens.text, 0.035)

                Row {
                    id: cells
                    x: 14
                    width: parent.width - 28
                    anchors.verticalCenter: parent.verticalCenter

                    Ink {
                        width: cells.width / sheet.wide
                        dim: true
                        wrapMode: Text.WordWrap
                        text: line.modelData.titulo
                    }

                    Repeater {
                        model: sheet.wide - 1

                        Ink {
                            required property int index
                            readonly property bool best: line.modelData.mejor === index

                            width: cells.width / sheet.wide
                            horizontalAlignment: Text.AlignRight
                            wrapMode: Text.WordWrap
                            strong: best
                            color: best ? block.tone : Tokens.text
                            text: line.modelData.valores[index] ?? ""
                        }
                    }
                }
            }
        }
    }
}
