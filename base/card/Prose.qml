import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

Ink {
    // La pieza (su Loader en Card: el bloque, su ancho y su tono) y la tarjeta.
    required property Loader block
    required property Item host

    width: block.width
    wrapMode: Text.WordWrap
    lineHeight: 1.2
    text: block.modelData.texto
}
