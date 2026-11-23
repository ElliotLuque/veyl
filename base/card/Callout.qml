import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

// Lo que no debe pasarse por alto: una caja teñida con su icono.
Rectangle {
    // La pieza (su Loader en Card: el bloque, su ancho y su tono) y la tarjeta.
    required property Loader block
    required property Item host

    width: block.width
    implicitHeight: Math.max(52, words.implicitHeight + 26)
    radius: Tokens.radius
    color: host.mix(Tokens.bg, block.tone, 0.12)
    border.color: host.soft(block.tone, 0.35)
    border.width: 1

    Icon {
        id: mark
        x: 14
        y: 14
        color: block.tone
        font.pixelSize: 22
        name: block.modelData.icono || ({ aviso: "warning", mal: "x-circle", bien: "check-circle" })[block.modelData.color] || "lightbulb"
    }

    Column {
        id: words
        x: 48
        y: 13
        width: parent.width - x - 16
        spacing: 3

        Ink {
            width: parent.width
            strong: true
            color: block.tone
            wrapMode: Text.WordWrap
            text: block.modelData.valor
            visible: text !== ""
        }

        Ink {
            width: parent.width
            wrapMode: Text.WordWrap
            lineHeight: 1.15
            text: block.modelData.texto
        }
    }
}
