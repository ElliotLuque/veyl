import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

// Voz: cómo te oye Veyl (el modelo de Whisper y el micro) y cómo contesta
// (si habla, con qué motor, qué voz, a qué velocidad y a qué volumen). Lo
// lee veyld de settings.json en cada latido.
Column {
    id: root

    readonly property var voice: Settings.voice ?? {}

    spacing: 22

    Component.onCompleted: Host.refresh("voice")

    Group {
        title: "Reconocimiento"

        Option {
            glyph: "waveform"
            title: "Modo escucha"

            Toggle {
                on: Listen.active
                onToggled: Listen.toggle()
            }
        }

        Option {
            glyph: "brain"
            title: "Modelo"
            text: Host.models.length === 0 ? "No hay modelos instalados. Descarga uno con «veyl voice get small-q5_1»." : ""

            Select {
                width: 260
                options: [{ value: "", label: "Automático" }, ...Host.models.map(value => ({ value, label: value }))]
                value: root.voice.model ?? ""
                onPicked: value => Tuning.voice("model", value)
            }
        }

        Option {
            glyph: "microphone"
            title: "Micrófono"

            Select {
                width: 260
                options: [{ value: "", label: "Predeterminado" }, ...Sound.sources.map(node => ({ value: node.name, label: Sound.label(node) }))]
                value: root.voice.mic ?? ""
                onPicked: value => Tuning.voice("mic", value)
            }
        }
    }

    Group {
        title: "Síntesis"

        Option {
            glyph: "chat-teardrop-dots"
            title: "Responder en voz alta"

            Toggle {
                on: root.voice.speak ?? true
                onToggled: on => Tuning.voice("speak", on ? null : false)
            }
        }

        Option {
            glyph: "engine"
            title: "Motor"

            Select {
                width: 260
                options: [{ value: "", label: "Automático" }, { value: "xtts", label: "XTTS" }, { value: "piper", label: "Piper" }]
                value: root.voice.engine ?? ""
                onPicked: value => Tuning.voice("engine", value)
            }
        }

        Option {
            glyph: "user-sound"
            title: "Voz"

            Field {
                width: 220
                value: root.voice.voz ?? ""
                placeholder: "Predeterminada"
                onCommitted: text => Tuning.voice("voz", text.trim())
            }
        }

        Option {
            glyph: "speedometer"
            title: "Velocidad"

            Slider {
                from: 0.5
                to: 2
                step: 0.05
                value: root.voice.speed ?? 1.2
                shown: `× ${current.toFixed(2)}`
                onPicked: value => Tuning.voice("speed", Math.round(value * 100) / 100)
            }
        }

        Option {
            glyph: "speaker-high"
            title: "Volumen"

            Slider {
                from: 0
                to: 1
                step: 0.05
                value: root.voice.volume ?? 0.6
                shown: `${Math.round(current * 100)} %`
                onPicked: value => Tuning.voice("volume", Math.round(value * 100) / 100)
            }
        }

        Option {
            glyph: "robot"
            title: "Efecto robótico (Piper)"

            Slider {
                from: 0
                to: 1
                step: 0.05
                value: root.voice.robot ?? 1
                shown: `${Math.round(current * 100)} %`
                onPicked: value => Tuning.voice("robot", Math.round(value * 100) / 100)
            }
        }

        Option {
            glyph: "play"
            title: "Probar voz"

            Button {
                icon: "play"
                text: "Reproducir"
                onPressed: Quickshell.execDetached(["sh", "-c", `exec "$(command -v veyld || echo "$0/veyld/target/release/veyld")" speak "Hola. Así es como te contesto."`, Quickshell.shellPath("")])
            }
        }
    }
}
