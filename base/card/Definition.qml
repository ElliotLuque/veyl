import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

// Qué significa algo: el término en grande, su categoría en el tono y
// las acepciones numeradas, con su ejemplo.
Column {
    id: entry

    // La pieza (su Loader en Card: el bloque, su ancho y su tono) y la tarjeta.
    required property Loader block
    required property Item host

    // Si el título de la tarjeta ya es la palabra, no se repite: solo su categoría.
    readonly property bool said: (host.card?.titulo ?? "").toLowerCase().indexOf(block.modelData.texto.toLowerCase()) >= 0

    width: block.width
    spacing: 12

    Column {
        width: parent.width
        spacing: 2
        visible: !entry.said || block.modelData.valor !== ""

        Ink {
            width: parent.width
            wrapMode: Text.WordWrap
            font.pixelSize: 32
            font.weight: Font.DemiBold
            font.variableAxes: ({ "wght": 650 })
            text: block.modelData.texto
            visible: !entry.said
        }

        Ink {
            width: parent.width
            wrapMode: Text.WordWrap
            font.italic: true
            color: block.tone
            text: block.modelData.valor
            visible: text !== ""
        }
    }

    Repeater {
        model: block.modelData.items

        Item {
            id: sense

            required property var modelData
            required property int index

            width: block.width
            implicitHeight: meaning.implicitHeight

            Ink {
                strong: true
                color: block.tone
                text: sense.index + 1
            }

            Column {
                id: meaning
                x: 24
                width: parent.width - x
                spacing: 3

                Ink {
                    width: parent.width
                    wrapMode: Text.WordWrap
                    lineHeight: 1.15
                    text: sense.modelData.titulo
                }

                Ink {
                    width: parent.width
                    wrapMode: Text.WordWrap
                    dim: true
                    font.italic: true
                    font.pixelSize: Tokens.smallSize
                    text: "«" + sense.modelData.detalle + "»"
                    visible: sense.modelData.detalle !== ""
                }
            }
        }
    }
}
