import QtQuick
import Quickshell.Services.Pipewire
import qs
import qs.base
import qs.state

// Sonido: por dónde suena y por dónde se graba, y a cuánto.
Column {
    id: root

    spacing: 22

    // Entre los dispositivos, el que se usa.
    component Device: Option {
        id: device

        property var nodes: []
        property PwNode node

        title: "Dispositivo"

        Select {
            width: 360
            options: device.nodes.map(node => ({ value: node.id, label: Sound.label(node) }))
            value: device.node?.id
            placeholder: "Ninguno"
            onPicked: value => Sound.use(device.nodes.find(node => node.id === value))
        }
    }

    // El volumen de un dispositivo: el botón lo silencia.
    component Level: Option {
        id: level

        property PwNode node
        property string silent
        readonly property bool muted: node?.audio?.muted ?? false

        title: "Volumen"
        visible: !!node?.audio

        Knob {
            anchors.verticalCenter: parent.verticalCenter
            icon: level.muted ? level.silent : level.glyph
            tint: level.muted ? Tokens.critical : Tokens.dim
            lit: Tokens.border
            onPressed: Sound.mute(level.node)
        }

        Slider {
            width: 320
            to: 1
            step: 0.01
            value: level.node?.audio?.volume ?? 0
            shown: level.muted ? "Silencio" : `${Math.round(current * 100)} %`
            onMoved: value => {
                if (!level.node?.audio)
                    return;
                level.node.audio.volume = value;
                if (value > 0)
                    level.node.audio.muted = false;
            }
        }
    }

    Group {
        title: "Salida"
        note: Sound.sinks.length === 0 ? "No hay dispositivos de salida." : ""

        Device {
            glyph: /auricular|headphone|headset|cascos/i.test(Sound.label(Sound.sink)) ? "headphones" : "speaker-hifi"
            nodes: Sound.sinks
            node: Sound.sink
        }

        Level {
            node: Sound.sink
            glyph: "speaker-high"
            silent: "speaker-x"
        }
    }

    Group {
        title: "Entrada"
        note: Sound.sources.length === 0 ? "No hay dispositivos de entrada." : ""

        Device {
            glyph: "microphone-stage"
            nodes: Sound.sources
            node: Sound.source
        }

        Level {
            node: Sound.source
            glyph: "microphone"
            silent: "microphone-slash"
        }
    }
}
