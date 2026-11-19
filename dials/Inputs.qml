import QtQuick
import qs
import qs.base
import qs.state

// Teclado y ratón: lo que se le cambia a Hyprland en caliente. Se guarda en
// settings.json y se vuelve a poner en cada arranque; lo que no se toca
// sigue siendo lo de su configuración.
Column {
    id: root

    spacing: 22

    Component.onCompleted: Host.refresh("input")

    // Lo que hay ahora: lo cambiado aquí o, si no, lo que dice Hyprland.
    function now(key: string, fallback: var): var {
        return Settings.input?.[key] ?? Host.options[key] ?? fallback;
    }

    function change(key: string, value: var): void {
        Tuning.input(key, value);
        Host.options = Object.assign({}, Host.options, { [key]: value });
    }

    Group {
        title: "Teclado"

        Option {
            glyph: "keyboard"
            title: "Distribución"

            Field {
                width: 220
                value: root.now("kb_layout", "")
                placeholder: "es,us"
                onCommitted: text => root.change("kb_layout", text.replace(/\s+/g, ""))
            }
        }

        Option {
            glyph: "sliders-horizontal"
            title: "Opciones XKB"

            Field {
                width: 220
                value: root.now("kb_options", "")
                placeholder: "caps:escape"
                onCommitted: text => root.change("kb_options", text.replace(/\s+/g, ""))
            }
        }

        Option {
            glyph: "timer"
            title: "Velocidad de repetición"

            Slider {
                from: 10
                to: 80
                step: 1
                value: root.now("repeat_rate", 25)
                shown: `${Math.round(current)} /s`
                onPicked: value => root.change("repeat_rate", Math.round(value))
            }
        }

        Option {
            glyph: "hourglass"
            title: "Retardo de repetición"

            Slider {
                from: 150
                to: 1000
                step: 25
                value: root.now("repeat_delay", 600)
                shown: `${Math.round(current)} ms`
                onPicked: value => root.change("repeat_delay", Math.round(value))
            }
        }

        Item {
            width: parent.width
            height: 64

            Field {
                x: Tokens.pad
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - Tokens.pad * 2
                placeholder: "Escribe aquí para probar"
                onCommitted: text => {}
            }
        }
    }

    Group {
        title: "Ratón"

        Option {
            glyph: "cursor"
            title: "Sensibilidad"

            Slider {
                from: -1
                to: 1
                step: 0.05
                value: root.now("sensitivity", 0)
                shown: (current > 0 ? "+" : "") + current.toFixed(2)
                onPicked: value => root.change("sensitivity", Math.round(value * 100) / 100)
            }
        }

        Option {
            glyph: "mouse"
            title: "Desplazamiento natural"

            Toggle {
                on: root.now("natural_scroll", false)
                onToggled: on => root.change("natural_scroll", on)
            }
        }
    }

    Group {
        title: "Panel táctil"

        Option {
            glyph: "hand-pointing"
            title: "Desplazamiento natural"

            Toggle {
                on: root.now("touchpad:natural_scroll", false)
                onToggled: on => root.change("touchpad:natural_scroll", on)
            }
        }

        Option {
            glyph: "cursor-click"
            title: "Tocar para hacer clic"

            Toggle {
                on: root.now("touchpad:tap-to-click", true)
                onToggled: on => root.change("touchpad:tap-to-click", on)
            }
        }
    }
}
