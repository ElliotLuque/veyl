import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

Column {
    id: chart

    // La pieza (su Loader en Card: el bloque, su ancho y su tono) y la tarjeta.
    required property Loader block
    required property Item host

    readonly property var points: block.modelData.items
    readonly property real most: Math.max(1e-9, Math.max.apply(null, points.map(p => p.numero)))

    width: block.width
    spacing: 8

    Ink {
        width: parent.width
        dim: true
        font.pixelSize: Tokens.smallSize
        elide: Text.ElideRight
        text: block.modelData.texto
        visible: text !== ""
    }

    Row {
        id: columns
        width: parent.width
        height: 160
        spacing: 6

        Repeater {
            model: chart.points

            Item {
                id: column

                required property var modelData
                readonly property bool peak: modelData.numero === chart.most

                width: (columns.width - columns.spacing * (chart.points.length - 1)) / chart.points.length
                height: columns.height

                Ink {
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.bottom: bar.top
                    anchors.bottomMargin: 4
                    font.pixelSize: Tokens.tinySize
                    strong: column.peak
                    dim: !column.peak
                    text: host.figure(column.modelData.numero, block.modelData.unidad)
                }

                Rectangle {
                    id: bar
                    anchors.bottom: label.top
                    anchors.bottomMargin: 6
                    anchors.horizontalCenter: parent.horizontalCenter
                    width: Math.min(parent.width, 40)
                    height: Math.max(3, (parent.height - label.height - 30) * Math.max(0, column.modelData.numero) / chart.most)
                    radius: 4
                    color: column.peak ? block.tone : host.soft(block.tone, 0.4)
                }

                Ink {
                    id: label
                    anchors.bottom: parent.bottom
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    elide: Text.ElideRight
                    dim: true
                    font.pixelSize: Tokens.tinySize
                    text: column.modelData.titulo
                }
            }
        }
    }
}
