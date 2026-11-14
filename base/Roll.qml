import QtQuick
import qs

// Texto que rueda en vertical al cambiar, hacia donde se ha movido el nivel:
// al bajar, el nuevo entra desde abajo, igual que sus ventanas.
Item {
    id: root

    property string text
    property int order: 0  // posición en la pila: decide el sentido
    property bool strong: false
    property int size: Tokens.fontSize
    property int weight: strong ? Font.DemiBold : Font.Medium

    property string shown
    property string last
    property int seen
    property int way: 1

    implicitWidth: before.implicitWidth + (now.implicitWidth - before.implicitWidth) * turn.value
    implicitHeight: Tokens.island
    clip: true

    Component.onCompleted: {
        shown = text;
        seen = order;
    }

    onTextChanged: Qt.callLater(roll)
    onOrderChanged: Qt.callLater(roll)

    function roll(): void {
        if (text === shown && order === seen)
            return;
        way = order < seen ? -1 : 1;
        last = shown;
        shown = text;
        seen = order;
        turn.velocity = 0;
        turn.value = 0;
    }

    Spring {
        id: turn
        to: 1
        settle: Tokens.veil
    }

    Ink {
        id: before
        y: (root.height - height) / 2 - root.way * root.height * 0.6 * turn.value
        text: root.last
        font.pixelSize: root.size
        font.weight: root.weight
        // Geist es variable: sin el eje, Qt deja todos los pesos en uno.
        font.variableAxes: ({ "wght": root.weight })
        opacity: 1 - turn.value
        visible: opacity > 0
    }

    Ink {
        id: now
        y: (root.height - height) / 2 + root.way * root.height * 0.6 * (1 - turn.value)
        text: root.shown
        font.pixelSize: root.size
        font.weight: root.weight
        // Geist es variable: sin el eje, Qt deja todos los pesos en uno.
        font.variableAxes: ({ "wght": root.weight })
        opacity: turn.value
    }
}
