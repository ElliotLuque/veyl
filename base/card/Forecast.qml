import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

// El tiempo: ahora, en grande, y una fila por día u hora con su
// tramo de temperatura dentro del de toda la previsión.
Column {
    id: sky

    // La pieza (su Loader en Card: el bloque, su ancho y su tono) y la tarjeta.
    required property Loader block
    required property Item host

    readonly property var days: block.modelData.items
    readonly property real low: Math.min.apply(null, days.map(d => d.min))
    readonly property real high: Math.max.apply(null, days.map(d => d.max))

    width: block.width
    spacing: 2

    Item {
        width: parent.width
        implicitHeight: now.implicitHeight + 10
        visible: block.modelData.valor !== ""

        Row {
            id: now
            spacing: 14

            Icon {
                anchors.verticalCenter: parent.verticalCenter
                name: block.modelData.icono || (sky.days[0]?.icono ?? "")
                color: block.tone
                font.pixelSize: 46
                visible: name !== ""
            }

            Ink {
                anchors.verticalCenter: parent.verticalCenter
                font.pixelSize: 56
                font.weight: Font.Light
                font.variableAxes: ({ "wght": 300 })
                text: block.modelData.valor
            }

            Ink {
                anchors.verticalCenter: parent.verticalCenter
                width: Math.max(0, Math.min(implicitWidth, sky.width - 220))
                dim: true
                wrapMode: Text.WordWrap
                maximumLineCount: 2
                elide: Text.ElideRight
                text: block.modelData.texto
            }
        }
    }

    Repeater {
        model: sky.days

        Item {
            id: day

            required property var modelData
            required property int index

            width: sky.width
            height: 34

            Rectangle {
                width: parent.width
                height: 1
                color: Tokens.border
                visible: day.index > 0
            }

            Ink {
                id: dayName
                width: 64
                anchors.verticalCenter: parent.verticalCenter
                elide: Text.ElideRight
                strong: day.index === 0
                text: day.modelData.titulo
            }

            Icon {
                id: weather
                x: dayName.width + 4
                anchors.verticalCenter: parent.verticalCenter
                name: day.modelData.icono
                color: block.tone
            }

            Ink {
                id: rain
                x: weather.x + 28
                width: 42
                anchors.verticalCenter: parent.verticalCenter
                font.pixelSize: Tokens.tinySize
                strong: true
                color: Tokens.focus
                elide: Text.ElideRight
                text: day.modelData.detalle
            }

            Ink {
                id: cold
                x: rain.x + rain.width
                width: 38
                anchors.verticalCenter: parent.verticalCenter
                horizontalAlignment: Text.AlignRight
                dim: true
                text: Math.round(day.modelData.min) + "°"
            }

            Rectangle {
                id: track

                readonly property real range: Math.max(1e-9, sky.high - sky.low)

                x: cold.x + cold.width + 12
                width: warm.x - x - 12
                height: 6
                radius: 3
                anchors.verticalCenter: parent.verticalCenter
                color: Tokens.surface

                Rectangle {
                    x: track.width * (day.modelData.min - sky.low) / track.range
                    width: Math.max(track.height, track.width * (day.modelData.max - day.modelData.min) / track.range)
                    height: track.height
                    radius: track.radius
                    color: block.tone
                }
            }

            Ink {
                id: warm
                anchors.right: parent.right
                width: 38
                anchors.verticalCenter: parent.verticalCenter
                horizontalAlignment: Text.AlignRight
                strong: true
                text: Math.round(day.modelData.max) + "°"
            }
        }
    }
}
