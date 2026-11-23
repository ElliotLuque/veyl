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

    Item {
        width: parent.width
        implicitHeight: percent.implicitHeight

        Ink {
            width: parent.width - percent.width - 8
            dim: true
            elide: Text.ElideRight
            text: block.modelData.texto
        }

        Ink {
            id: percent
            anchors.right: parent.right
            strong: true
            color: block.tone
            text: Math.round(block.modelData.valor * 100) + " %"
        }
    }

    Rectangle {
        width: parent.width
        height: 6
        radius: 3
        color: Tokens.surface

        Rectangle {
            width: parent.width * block.modelData.valor
            height: parent.height
            radius: 3
            color: block.tone
        }
    }
}
