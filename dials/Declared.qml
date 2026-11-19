import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

// Nix: dónde está declarado Veyl y lo que no coincide con lo declarado. Lo
// cambiado aquí se guarda en tu configuración (en el mismo archivo, sin
// tocar lo demás) o se deshace; lo guardado llega al reconstruir.
Column {
    id: root

    readonly property var glyphs: ({
            scheme: "palette",
            wallpaper: "image",
            webSearch: "globe",
            terminal: "terminal",
            weekNumbers: "calendar",
            quiet: "bell-slash",
            journal: "notebook",
            llm: "sparkle",
            notificationDays: "bell",
            voice: "waveform",
            brain: "brain",
            input: "keyboard",
            monitors: "monitor",
            lockAfter: "lock",
            calendarTarget: "calendar-plus",
            eventWarning: "clock-countdown"
        })
    readonly property bool writable: Nix.declared && Nix.file !== ""
    readonly property string name: Nix.file.split("/").pop()

    spacing: 22

    Group {
        title: "Tu configuración"
        note: Nix.declared ? "" : "Veyl no se instaló con su módulo de Nix. Añade su flake, importa homeManagerModules.veyl (o nixosModules.veyl) y pega ahí lo que tienes ahora."

        Option {
            glyph: "file-code"
            title: Nix.declared ? "Archivo" : "Sin declarar"
            tone: Nix.problem ? Tokens.attention : Tokens.dim
            text: Nix.file ? Nix.file.replace(Quickshell.env("HOME"), "~") : Nix.problem || (Nix.declared ? "Buscando…" : "programs.veyl no está en tu configuración")

            Button {
                icon: "arrow-square-out"
                text: "Abrir"
                visible: Nix.file !== ""
                onPressed: {
                    Tuning.hide();
                    Qt.openUrlExternally(`file://${Nix.file}`);
                }
            }

            Button {
                icon: "copy"
                text: "Copiar como Nix"
                visible: !Nix.declared
                onPressed: Nix.copy()
            }
        }

        Option {
            glyph: "hammer"
            title: "Reconstruir"
            text: Nix.rebuild
            visible: Nix.declared && Nix.rebuild !== ""

            Button {
                text: "Reconstruir"
                strong: Nix.unbuilt.length > 0
                onPressed: {
                    Tuning.hide();
                    Nix.build();
                }
            }
        }
    }

    Group {
        title: Nix.declared ? "No coincide con tu configuración" : "Cambiado en Veyl"
        note: Nix.failure
        visible: Nix.unsaved.length > 0

        Repeater {
            model: Nix.unsaved

            Option {
                required property var modelData

                glyph: root.glyphs[modelData.key] ?? "sliders"
                tone: Tokens.attention
                title: modelData.title
                text: Nix.describe(modelData)

                Button {
                    icon: "arrow-counter-clockwise"
                    text: "Deshacer"
                    visible: Nix.declared
                    onPressed: Nix.revert(modelData)
                }

                Button {
                    icon: "floppy-disk"
                    text: "Guardar"
                    strong: true
                    visible: root.writable
                    onPressed: Nix.save([modelData])
                }
            }
        }

        Option {
            glyph: "floppy-disk"
            title: `Guardar todo en ${root.name}`
            visible: root.writable && Nix.unsaved.length > 1

            Button {
                text: Nix.busy ? "Guardando…" : "Guardar todo"
                strong: true
                onPressed: Nix.save(Nix.unsaved)
            }
        }
    }

    Group {
        title: "Guardado, falta reconstruir"
        visible: Nix.unbuilt.length > 0

        Repeater {
            model: Nix.unbuilt

            Option {
                required property var modelData

                glyph: root.glyphs[modelData.key] ?? "sliders"
                tone: Tokens.accent
                title: modelData.title
                text: Nix.describe(modelData).replace(/ · en Nix: .*$/, "")
            }
        }
    }

    Group {
        visible: Nix.declared && Nix.drift.length === 0

        Option {
            glyph: "check-circle"
            tone: Tokens.accent
            title: "Todo coincide con tu configuración"
        }
    }
}
