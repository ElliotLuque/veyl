import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

Column {
    // La pieza (su Loader en Card: el bloque, su ancho y su tono) y la tarjeta.
    required property Loader block
    required property Item host

    width: block.width
    spacing: 6

    Picture {
        id: shot
        width: parent.width
        tall: 220
        source: block.modelData.url
    }

    Ink {
        width: parent.width
        dim: true
        font.pixelSize: Tokens.smallSize
        wrapMode: Text.WordWrap
        text: block.modelData.texto
        visible: shot.ready && text !== ""
    }
}
