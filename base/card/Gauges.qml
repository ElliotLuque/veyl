import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

// Anillos de progreso, uno por cifra; se llenan al salir.
Row {
    id: dials

    // La pieza (su Loader en Card: el bloque, su ancho y su tono) y la tarjeta.
    required property Loader block
    required property Item host

    readonly property int count: block.modelData.items.length
    readonly property real each: (width - spacing * (count - 1)) / count
    readonly property real size: Math.min(count === 1 ? 150 : 124, each)

    width: block.width
    spacing: 12

    Repeater {
        model: block.modelData.items

        Column {
            id: dial

            required property var modelData
            required property int index
            readonly property color tone: host.tint(modelData.color, block.tone)
            readonly property real share: Math.max(0, Math.min(1, modelData.numero / modelData.maximo))
            property real shown: 0
            NumberAnimation on shown { to: dial.share; duration: 1000; easing.type: Easing.OutCubic }

            width: dials.each
            spacing: 6

            Item {
                anchors.horizontalCenter: parent.horizontalCenter
                width: dials.size
                height: dials.size

                Canvas {
                    id: arc
                    anchors.fill: parent
                    onPaint: {
                        const ctx = getContext("2d");
                        ctx.reset();
                        const r = width / 2, line = Math.max(8, width * 0.1), from = Math.PI * 0.75, sweep = Math.PI * 1.5;
                        const t = dial.tone;
                        ctx.lineCap = "round";
                        ctx.lineWidth = line;
                        ctx.beginPath();
                        ctx.arc(r, r, r - line / 2 - 1, from, from + sweep);
                        ctx.strokeStyle = Qt.rgba(t.r, t.g, t.b, 0.16);
                        ctx.stroke();
                        if (dial.shown <= 0)
                            return;
                        ctx.beginPath();
                        ctx.arc(r, r, r - line / 2 - 1, from, from + sweep * dial.shown);
                        ctx.strokeStyle = t;
                        ctx.stroke();
                    }

                    Connections {
                        target: dial
                        function onShownChanged(): void {
                            arc.requestPaint();
                        }
                    }
                }

                Column {
                    anchors.centerIn: parent
                    width: parent.width * 0.7

                    Ink {
                        width: parent.width
                        horizontalAlignment: Text.AlignHCenter
                        strong: true
                        font.pixelSize: Math.round(dials.size * (text.length > 5 ? 0.17 : 0.23))
                        elide: Text.ElideRight
                        text: dial.modelData.valor || (dial.modelData.maximo === 100 ? Math.round(dial.modelData.numero) + " %" : host.figure(dial.modelData.numero, ""))
                    }

                    Ink {
                        width: parent.width
                        horizontalAlignment: Text.AlignHCenter
                        dim: true
                        font.pixelSize: Tokens.tinySize
                        text: dial.modelData.maximo !== 100 && dial.modelData.maximo !== 1 ? "de " + host.figure(dial.modelData.maximo, "") : ""
                        visible: text !== ""
                    }
                }
            }

            Ink {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.WordWrap
                maximumLineCount: 2
                elide: Text.ElideRight
                font.pixelSize: Tokens.smallSize
                text: dial.modelData.titulo
                visible: text !== ""
            }
        }
    }
}
