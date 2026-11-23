import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

Item {
    // La pieza (su Loader en Card: el bloque, su ancho y su tono) y la tarjeta.
    required property Loader block
    required property Item host

    width: block.width
    implicitHeight: words.implicitHeight + 8

    Ink {
        y: -14
        strong: true
        font.pixelSize: 76
        color: host.soft(block.tone, 0.45)
        text: "“"
    }

    Column {
        id: words
        x: 42
        y: 8
        width: parent.width - x
        spacing: 6

        Ink {
            width: parent.width
            wrapMode: Text.WordWrap
            lineHeight: 1.2
            font.italic: true
            font.pixelSize: Tokens.clockSize
            text: "«" + block.modelData.texto + "»"
        }

        Ink {
            dim: true
            font.pixelSize: Tokens.smallSize
            text: "— " + block.modelData.valor
            visible: block.modelData.valor !== ""
        }
    }
}
