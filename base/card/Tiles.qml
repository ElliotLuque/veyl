import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

Grid {
    id: mosaic

    // La pieza (su Loader en Card: el bloque, su ancho y su tono) y la tarjeta.
    required property Loader block
    required property Item host

    width: block.width
    columns: block.modelData.items.length === 4 ? 2 : Math.min(3, block.modelData.items.length)
    spacing: 8

    Repeater {
        model: block.modelData.items

        Rectangle {
            id: tile

            required property var modelData
            required property int index
            readonly property color tone: block.tone
            readonly property var serie: modelData.serie ?? []

            width: (mosaic.width - mosaic.spacing * (mosaic.columns - 1)) / mosaic.columns
            implicitHeight: stat.implicitHeight + 24 + (spark.visible ? spark.height + 6 : 0)
            radius: Tokens.radius
            color: Tokens.surface

            // Un filo del tono arriba.
            Rectangle {
                x: 12
                width: 22
                height: 3
                radius: 2
                color: tile.tone
            }

            Canvas {
                id: spark
                x: 12
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 10
                width: parent.width - 24
                height: 30
                visible: tile.serie.length >= 2
                onWidthChanged: requestPaint()
                Component.onCompleted: requestPaint()
                onPaint: {
                    const ctx = getContext("2d");
                    ctx.reset();
                    const n = tile.serie, hi = Math.max.apply(null, n), lo = Math.min.apply(null, n), span = (hi - lo) || 1;
                    const xy = n.map((v, i) => [width * i / (n.length - 1), 3 + (height - 6) * (1 - (v - lo) / span)]);
                    const c = tile.tone;
                    ctx.beginPath();
                    xy.forEach((p, i) => i ? ctx.lineTo(p[0], p[1]) : ctx.moveTo(p[0], p[1]));
                    ctx.strokeStyle = c;
                    ctx.lineWidth = 2;
                    ctx.lineJoin = "round";
                    ctx.stroke();
                }
            }

            Column {
                id: stat
                x: 12
                y: 12
                width: parent.width - 24
                spacing: 3

                Row {
                    spacing: 6

                    Icon {
                        anchors.verticalCenter: parent.verticalCenter
                        name: tile.modelData.icono
                        color: tile.tone
                        font.pixelSize: 15
                        visible: name !== ""
                    }

                    Ink {
                        width: stat.width - 21
                        dim: true
                        font.pixelSize: Tokens.smallSize
                        elide: Text.ElideRight
                        text: tile.modelData.titulo
                    }
                }

                Ink {
                    width: parent.width
                    strong: true
                    font.pixelSize: Tokens.figureSize
                    elide: Text.ElideRight
                    text: tile.modelData.valor
                }

                Ink {
                    width: parent.width
                    font.pixelSize: Tokens.smallSize
                    elide: Text.ElideRight
                    color: host.tint(tile.modelData.color, Tokens.dim)
                    text: tile.modelData.detalle
                    visible: text !== ""
                }
            }
        }
    }
}
