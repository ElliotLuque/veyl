import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

Column {
    id: trend

    // La pieza (su Loader en Card: el bloque, su ancho y su tono) y la tarjeta.
    required property Loader block
    required property Item host

    readonly property var points: block.modelData.items
    readonly property real high: Math.max.apply(null, points.map(p => p.numero))
    readonly property real low: Math.min.apply(null, points.map(p => p.numero))

    width: block.width
    spacing: 6

    Ink {
        width: parent.width
        dim: true
        font.pixelSize: Tokens.smallSize
        elide: Text.ElideRight
        text: block.modelData.texto
        visible: text !== ""
    }

    Item {
        width: parent.width
        height: 150

        Canvas {
            id: plot
            anchors.fill: parent
            anchors.topMargin: 18
            anchors.bottomMargin: 18
            onWidthChanged: requestPaint()
            Component.onCompleted: requestPaint()
            onPaint: {
                const ctx = getContext("2d");
                ctx.reset();
                const points = trend.points, n = points.length, pad = 6;
                const span = (trend.high - trend.low) || 1;
                const xy = points.map((p, i) => [pad + (width - pad * 2) * i / (n - 1), pad + (height - pad * 2) * (1 - (p.numero - trend.low) / span)]);
                const a = block.tone;
                ctx.beginPath();
                xy.forEach((p, i) => i ? ctx.lineTo(p[0], p[1]) : ctx.moveTo(p[0], p[1]));
                ctx.strokeStyle = a;
                ctx.lineWidth = 2.5;
                ctx.lineJoin = "round";
                ctx.stroke();
                const last = xy[n - 1];
                ctx.beginPath();
                ctx.arc(last[0], last[1], 4.5, 0, Math.PI * 2);
                ctx.fillStyle = a;
                ctx.fill();
            }
        }

        Ink {
            anchors.top: parent.top
            dim: true
            font.pixelSize: Tokens.tinySize
            text: "máx. " + host.figure(trend.high, block.modelData.unidad)
        }

        Ink {
            anchors.bottom: parent.bottom
            dim: true
            font.pixelSize: Tokens.tinySize
            text: "mín. " + host.figure(trend.low, block.modelData.unidad)
        }

        Ink {
            anchors.top: parent.top
            anchors.right: parent.right
            strong: true
            color: block.tone
            font.pixelSize: Tokens.smallSize
            text: host.figure(trend.points[trend.points.length - 1].numero, block.modelData.unidad)
        }
    }

    Item {
        width: parent.width
        implicitHeight: first.implicitHeight

        Ink {
            id: first
            dim: true
            font.pixelSize: Tokens.tinySize
            text: trend.points[0].titulo
        }

        Ink {
            anchors.horizontalCenter: parent.horizontalCenter
            dim: true
            font.pixelSize: Tokens.tinySize
            text: trend.points.length > 2 ? trend.points[Math.floor(trend.points.length / 2)].titulo : ""
        }

        Ink {
            anchors.right: parent.right
            dim: true
            font.pixelSize: Tokens.tinySize
            text: trend.points[trend.points.length - 1].titulo
        }
    }
}
