import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

Rectangle {
    // La pieza (su Loader en Card: el bloque, su ancho y su tono) y la tarjeta.
    required property Loader block
    required property Item host

    width: block.width
    implicitHeight: listing.y + listing.implicitHeight + 14
    radius: Tokens.radius
    color: Tokens.surface

    Ink {
        id: language
        x: 14
        y: 10
        dim: true
        strong: true
        font.pixelSize: Tokens.tinySize
        text: block.modelData.valor
        visible: text !== ""
    }

    Text {
        id: listing
        x: 14
        y: language.visible ? 30 : 14
        width: parent.width - 28
        wrapMode: Text.WrapAnywhere
        textFormat: Text.PlainText
        font.family: "JetBrains Mono"
        font.pixelSize: 12
        color: Tokens.text
        text: block.modelData.texto
    }
}
