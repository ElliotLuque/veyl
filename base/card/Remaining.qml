import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

Rectangle {
    id: clock

    // La pieza (su Loader en Card: el bloque, su ancho y su tono) y la tarjeta.
    required property Loader block
    required property Item host

    property real now: Date.now()
    readonly property real at: Date.parse(block.modelData.fecha)
    readonly property real remaining: (at - now) / 1000
    readonly property bool past: remaining <= 0
    readonly property var parts: {
        const s = Math.abs(remaining);
        return [[Math.floor(s / 86400), "días"], [Math.floor(s % 86400 / 3600), "horas"], [Math.floor(s % 3600 / 60), "min"], [Math.floor(s % 60), "s"]];
    }

    width: block.width
    implicitHeight: units.height + caption.implicitHeight + 34
    radius: Tokens.radius
    color: Tokens.surface

    // Cuatro veces por segundo, para que los segundos no salten de dos en dos.
    Timer {
        running: clock.visible
        repeat: true
        interval: 250
        onTriggered: clock.now = Date.now()
    }

    Ink {
        id: caption
        x: 16
        y: 12
        width: parent.width - 32
        dim: true
        font.pixelSize: Tokens.smallSize
        elide: Text.ElideRight
        text: (block.modelData.texto || "") + (clock.past ? " · hace" : " · faltan")
    }

    Row {
        id: units
        x: 16
        y: caption.y + caption.height + 8
        spacing: 10

        Repeater {
            model: clock.parts

            Rectangle {
                required property var modelData
                width: (block.width - 32 - 30) / 4
                height: 58
                radius: 8
                color: Tokens.plate

                Column {
                    anchors.centerIn: parent

                    Ink {
                        anchors.horizontalCenter: parent.horizontalCenter
                        strong: true
                        font.pixelSize: Tokens.figureSize
                        color: block.tone
                        text: modelData[0]
                    }

                    Ink {
                        anchors.horizontalCenter: parent.horizontalCenter
                        dim: true
                        font.pixelSize: Tokens.tinySize
                        text: modelData[1]
                    }
                }
            }
        }
    }
}
