import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

// De dónde sale: enlaces pequeños, solo subrayados, tras «Fuentes».
Flow {
    // La pieza (su Loader en Card: el bloque, su ancho y su tono) y la tarjeta.
    required property Loader block
    required property Item host

    width: block.width
    spacing: 14

    Ink {
        dim: true
        font.pixelSize: Tokens.tinySize + 1
        text: "Fuentes"
    }

    Repeater {
        model: block.modelData.items

        Ink {
            required property var modelData

            width: Math.min(implicitWidth, 260)
            elide: Text.ElideRight
            font.pixelSize: Tokens.tinySize + 1
            font.underline: true
            color: source.containsMouse ? Tokens.text : Tokens.dim
            text: modelData.titulo || modelData.url.replace(/^https?:\/\/(www\.)?/, "").split("/")[0]

            MouseArea {
                id: source
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: {
                    Qt.openUrlExternally(parent.modelData.url);
                    host.done();
                }
            }
        }
    }
}
