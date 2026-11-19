import QtQuick
import Quickshell.Hyprland
import qs
import qs.base
import qs.state

// Pantallas: cómo están colocadas y, de cada una, resolución, frecuencia y
// escala, y el brillo si se puede. Un cambio es una prueba: si no se
// confirma, se deshace solo.
Column {
    id: root

    readonly property var scales: [1, 1.25, 1.333333, 1.5, 1.6, 1.75, 2, 2.5, 3]

    spacing: 22

    Component.onCompleted: Host.refresh("screens")

    function size(mode: string): string {
        return mode.split("@")[0];
    }

    function rate(mode: string): real {
        return parseFloat(mode.split("@")[1]) || 0;
    }

    // Las resoluciones distintas, de la más grande a la más pequeña, y las
    // frecuencias de una.
    function resolutions(monitor: var): var {
        return [...new Set(monitor.modes.map(size))].map(value => ({ value, label: value.replace("x", " × ") }));
    }

    function rates(monitor: var): var {
        return monitor.modes.filter(mode => size(mode) === size(monitor.mode)).map(mode => ({ value: mode, label: `${rate(mode).toFixed(rate(mode) % 1 ? 2 : 0)} Hz` }));
    }

    // Las escalas que dan un tamaño entero con esta resolución (Hyprland no
    // acepta las demás), y la de ahora.
    function fitting(monitor: var): var {
        const [width, height] = size(monitor.mode).split("x").map(Number);
        const whole = scale => [width, height].every(side => Math.abs(side / scale - Math.round(side / scale)) < 0.01);
        return scales.filter(scale => whole(scale) || Math.abs(scale - monitor.scale) < 0.01);
    }

    // Cambiar de resolución lleva a su frecuencia más alta.
    function resize(monitor: var, resolution: string): void {
        const best = monitor.modes.filter(mode => size(mode) === resolution).sort((a, b) => rate(b) - rate(a))[0];
        if (best)
            Host.arrange(monitor.name, { mode: best });
    }

    // Un cambio a prueba: se queda solo si lo confirmas.
    Rectangle {
        width: parent.width
        height: 64
        radius: Tokens.round - Tokens.inset
        color: Tokens.surface
        border.width: Tokens.line
        border.color: Tokens.attention
        visible: !!Host.trial

        Label {
            x: Tokens.pad
            anchors.verticalCenter: parent.verticalCenter
            icon: "hourglass"
            tone: Tokens.attention
            text: `¿Mantener esta configuración? Se revertirá en ${Host.left} s.`
        }

        Row {
            anchors.right: parent.right
            anchors.rightMargin: Tokens.pad
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8

            Button {
                icon: "arrow-counter-clockwise"
                text: "Revertir"
                onPressed: Host.revert()
            }

            Button {
                icon: "check"
                text: "Mantener"
                strong: true
                onPressed: Host.keep()
            }
        }
    }

    // Cómo están colocadas, a escala: la que tiene el foco, del acento.
    Group {
        title: "Disposición"
        visible: Host.monitors.length > 0

        Item {
            id: map

            readonly property var boxes: Host.monitors.map(monitor => {
                const [width, height] = root.size(monitor.mode).split("x").map(Number);
                const turned = monitor.transform % 2 === 1;
                // Se colocan por su tamaño en el escritorio (con la escala
                // aplicada), pero dicen su resolución de verdad.
                return { name: monitor.name, x: monitor.x, y: monitor.y, w: (turned ? height : width) / monitor.scale, h: (turned ? width : height) / monitor.scale, mode: root.size(monitor.mode), scale: monitor.scale };
            })
            readonly property real minX: Math.min(...boxes.map(box => box.x))
            readonly property real minY: Math.min(...boxes.map(box => box.y))
            readonly property real wide: Math.max(...boxes.map(box => box.x + box.w)) - minX
            readonly property real tall: Math.max(...boxes.map(box => box.y + box.h)) - minY
            readonly property real ratio: Math.min((width - 40) / wide, (height - 40) / tall)

            width: parent.width
            height: 180

            Repeater {
                model: map.boxes

                Rectangle {
                    id: box

                    required property var modelData
                    readonly property bool focused: Hyprland.focusedMonitor?.name === modelData.name

                    x: (map.width - map.wide * map.ratio) / 2 + (modelData.x - map.minX) * map.ratio + 2
                    y: (map.height - map.tall * map.ratio) / 2 + (modelData.y - map.minY) * map.ratio + 2
                    width: modelData.w * map.ratio - 4
                    height: modelData.h * map.ratio - 4
                    radius: Tokens.soft
                    color: Tokens.bg
                    border.width: focused ? 2 : Tokens.line
                    border.color: focused ? Tokens.accent : Tokens.border

                    Column {
                        anchors.centerIn: parent
                        spacing: 2

                        Ink {
                            anchors.horizontalCenter: parent.horizontalCenter
                            text: box.modelData.name
                            strong: true
                        }

                        Ink {
                            anchors.horizontalCenter: parent.horizontalCenter
                            text: [box.modelData.mode.replace("x", " × "), box.modelData.scale !== 1 ? `${Math.round(box.modelData.scale * 100)} %` : ""].filter(part => part).join(" · ")
                            dim: true
                            font.pixelSize: Tokens.smallSize
                        }
                    }
                }
            }
        }
    }

    Repeater {
        model: Host.monitors

        Group {
            id: screen

            required property var modelData

            title: [modelData.name, modelData.description].filter(part => part).join(" · ")

            Option {
                glyph: "monitor"
                title: "Resolución"

                Select {
                    width: 220
                    options: root.resolutions(screen.modelData)
                    value: root.size(screen.modelData.mode)
                    onPicked: value => root.resize(screen.modelData, value)
                }
            }

            Option {
                glyph: "speedometer"
                title: "Frecuencia de actualización"

                Select {
                    width: 220
                    options: root.rates(screen.modelData)
                    value: screen.modelData.modes.find(mode => mode === screen.modelData.mode) ?? screen.modelData.modes.find(mode => root.size(mode) === root.size(screen.modelData.mode) && Math.abs(root.rate(mode) - screen.modelData.refresh) < 0.05) ?? ""
                    onPicked: value => Host.arrange(screen.modelData.name, { mode: value })
                }
            }

            Option {
                glyph: "magnifying-glass"
                title: "Escala"

                Select {
                    width: 220
                    options: root.fitting(screen.modelData).map(value => ({ value, label: `${Math.round(value * 100)} %` }))
                    value: root.scales.find(scale => Math.abs(scale - screen.modelData.scale) < 0.01) ?? screen.modelData.scale
                    onPicked: value => Host.arrange(screen.modelData.name, { scale: value })
                }
            }
        }
    }

    Group {
        visible: Brightness.present

        Option {
            glyph: "sun"
            title: "Brillo"

            Slider {
                from: 1
                to: 100
                step: 1
                value: Brightness.percent
                shown: `${Math.round(current)} %`
                onPicked: value => Brightness.set(Math.round(value))
            }
        }
    }

    Ink {
        x: 4
        text: "Consultando Hyprland…"
        dim: true
        font.pixelSize: Tokens.smallSize
        visible: Host.monitors.length === 0
    }
}
