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
    implicitHeight: grid.implicitHeight + 20
    radius: Tokens.radius
    color: Tokens.surface

    Grid {
        id: grid
        x: 14
        y: 10
        width: parent.width - 28
        columns: 2
        columnSpacing: 16
        rowSpacing: 8

        Repeater {
            // Sin flatMap: el JS de QML no lo tiene.
            model: {
                const cells = [];
                for (const i of block.modelData.items)
                    cells.push({ text: i.titulo, dim: true }, { text: i.valor, dim: false });
                return cells;
            }

            Ink {
                required property var modelData
                width: (grid.width - 16) / 2
                wrapMode: Text.WordWrap
                dim: modelData.dim
                strong: !modelData.dim
                horizontalAlignment: modelData.dim ? Text.AlignLeft : Text.AlignRight
                text: modelData.text
            }
        }
    }
}
