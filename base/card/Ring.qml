import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

Item {
    id: pie

    // La pieza (su Loader en Card: el bloque, su ancho y su tono) y la tarjeta.
    required property Loader block
    required property Item host

    readonly property var points: block.modelData.items
    readonly property real total: points.reduce((sum, p) => sum + p.numero, 0)

    function shade(i: int): color {
        return host.soft(block.tone, [1, 0.7, 0.5, 0.35, 0.24, 0.16][i % 6]);
    }

    width: block.width
    implicitHeight: Math.max(150, legend.implicitHeight)

    Canvas {
        id: donut
        width: 150
        height: 150
        anchors.verticalCenter: parent.verticalCenter
        Component.onCompleted: requestPaint()
        onPaint: {
            const ctx = getContext("2d");
            ctx.reset();
            let start = -Math.PI / 2;
            pie.points.forEach((p, i) => {
                const sweep = Math.PI * 2 * p.numero / pie.total;
                ctx.beginPath();
                ctx.arc(75, 75, 60, start + 0.025, start + sweep - 0.025);
                ctx.strokeStyle = pie.shade(i);
                ctx.lineWidth = 20;
                ctx.stroke();
                start += sweep;
            });
        }

        Column {
            anchors.centerIn: parent
            width: 90

            Ink {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                strong: true
                font.pixelSize: Tokens.clockSize
                elide: Text.ElideRight
                text: host.figure(pie.total, block.modelData.unidad)
            }

            Ink {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                dim: true
                font.pixelSize: Tokens.tinySize
                elide: Text.ElideRight
                text: block.modelData.texto || "total"
            }
        }
    }

    Column {
        id: legend
        x: donut.width + 20
        width: parent.width - x
        anchors.verticalCenter: parent.verticalCenter
        spacing: 7

        Repeater {
            model: pie.points

            Item {
                id: slice

                required property var modelData
                required property int index

                width: legend.width
                implicitHeight: name.implicitHeight

                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    width: 10
                    height: 10
                    radius: 3
                    color: pie.shade(slice.index)
                }

                Ink {
                    id: name
                    x: 18
                    width: parent.width - x - share.width - 8
                    elide: Text.ElideRight
                    font.pixelSize: Tokens.smallSize
                    text: slice.modelData.titulo
                }

                Ink {
                    id: share
                    anchors.right: parent.right
                    strong: true
                    font.pixelSize: Tokens.smallSize
                    text: Math.round(slice.modelData.numero / pie.total * 100) + " %"
                }
            }
        }
    }
}
