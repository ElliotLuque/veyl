import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

Flow {
    // La pieza (su Loader en Card: el bloque, su ancho y su tono) y la tarjeta.
    required property Loader block
    required property Item host

    width: block.width
    spacing: 8

    Repeater {
        model: block.modelData.items

        Pill {
            required property var modelData
            required property int index
            text: modelData.titulo
            icon: "lightning"
            solid: index === 0
            onPressed: {
                Daemon.say(modelData.orden, "card", false);
                host.done();
            }
        }
    }
}
