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
    implicitHeight: Math.max(84, figureText.implicitHeight + 28)
    radius: Tokens.radius
    color: Tokens.surface

    Badge {
        id: sign
        x: 14
        anchors.verticalCenter: parent.verticalCenter
        size: 40
        tone: block.tone
        icon: block.modelData.icono ?? ""
        visible: icon !== ""
    }

    Column {
        id: figureText
        x: sign.visible ? sign.x + sign.width + 14 : 16
        width: parent.width - x - 16
        anchors.verticalCenter: parent.verticalCenter

        // Grande si es corta; si es larga, menor y en dos líneas.
        Ink {
            width: parent.width
            strong: true
            font.pixelSize: block.modelData.valor.length > 14 ? 24 : 36
            wrapMode: Text.WordWrap
            maximumLineCount: 2
            elide: Text.ElideRight
            color: block.tone
            text: block.modelData.valor
        }

        Ink {
            width: parent.width
            dim: true
            elide: Text.ElideRight
            text: block.modelData.texto
            visible: text !== ""
        }
    }
}
