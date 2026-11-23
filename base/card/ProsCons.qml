import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

Row {
    id: sides

    // La pieza (su Loader en Card: el bloque, su ancho y su tono) y la tarjeta.
    required property Loader block
    required property Item host

    readonly property bool both: block.modelData.pros.length > 0 && block.modelData.contras.length > 0

    width: block.width
    spacing: 10

    Repeater {
        model: [
            { head: "A favor", icon: "check-circle", tone: Tokens.done, list: block.modelData.pros },
            { head: "En contra", icon: "x-circle", tone: Tokens.critical, list: block.modelData.contras }
        ]

        Rectangle {
            id: side

            required property var modelData

            width: sides.both ? (sides.width - sides.spacing) / 2 : sides.width
            implicitHeight: points.implicitHeight + 24
            radius: Tokens.radius
            color: Tokens.surface
            visible: modelData.list.length > 0

            Column {
                id: points
                x: 12
                y: 12
                width: parent.width - 24
                spacing: 8

                Ink {
                    dim: true
                    strong: true
                    font.pixelSize: Tokens.smallSize
                    text: side.modelData.head
                }

                Repeater {
                    model: side.modelData.list

                    Item {
                        required property string modelData

                        width: points.width
                        implicitHeight: Math.max(18, said.implicitHeight)

                        Icon {
                            name: side.modelData.icon
                            color: side.modelData.tone
                            font.pixelSize: 16
                        }

                        Ink {
                            id: said
                            x: 24
                            width: parent.width - x
                            wrapMode: Text.WordWrap
                            font.pixelSize: Tokens.smallSize
                            text: parent.modelData
                        }
                    }
                }
            }
        }
    }
}
