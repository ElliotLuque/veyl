import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

// Nix: estado del sistema NixOS / flake, actualizaciones pendientes de
// `flake.lock`, reconstrucción en vivo y (si Veyl está declarado con su
// módulo) los ajustes que no coinciden con tu configuración.
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
    property double now: Date.now()

    spacing: 22

    Timer {
        running: root.visible
        interval: Nix.running ? 1000 : 30000
        repeat: true
        triggeredOnStart: true
        onTriggered: root.now = Date.now()
    }

    // --- Sistema y flake ---
    Group {
        title: "Sistema y flake"
        visible: Nix.systemActive

        Option {
            glyph: "snowflake"
            tone: Nix.rebootNeeded ? Tokens.attention : Tokens.accent
            title: Nix.generation > 0 ? `Generación #${Nix.generation}` : (Nix.nixos ? "NixOS" : "Flake")
            text: [
                Nix.version,
                Nix.builtAt > 0 ? `construida ${Nix.ageText(Nix.builtAt, root.now)}` : "",
                Nix.rebootNeeded ? "kernel nuevo · reinicia para aplicarlo" : ""
            ].filter(p => p).join(" · ")
            visible: Nix.generation > 0 || Nix.nixos
        }

        Option {
            glyph: "folder-open"
            tone: Nix.lockNewer ? Tokens.accent : Tokens.dim
            title: Nix.flakeName ? `Flake · ${Nix.flakeName}` : "Flake"
            text: [
                Nix.flake ? Nix.flake.replace(Quickshell.env("HOME"), "~") : "No se encontró flake.nix",
                Nix.lockNewer ? "flake.lock actualizado · falta reconstruir" : "",
                Nix.dirty > 0 ? `${Nix.dirty} ${Nix.dirty === 1 ? "cambio local" : "cambios locales"}` : "",
                Nix.nixpkgsAge > 0 ? `nixpkgs ${Nix.ageText(Nix.nixpkgsAge, root.now)}` : ""
            ].filter(p => p).join(" · ")
            visible: Nix.flake !== ""

            Button {
                icon: "terminal"
                text: "Terminal"
                onPressed: {
                    Tuning.hide();
                    Nix.openFlakeTerminal();
                }
            }
        }

        Option {
            glyph: Nix.jobState === "error" ? "warning" : Nix.jobState === "done" ? "check-circle" : "hammer"
            tone: Nix.jobState === "error" ? Tokens.critical : Nix.jobState === "done" ? Tokens.done : Nix.running || Nix.pendingRebuild ? Tokens.accent : Tokens.dim
            title: Nix.running ? (Nix.job?.phase || "En marcha…") : "Reconstruir el sistema"
            text: Nix.running
                ? [Nix.elapsedText(Nix.job?.since ?? 0, root.now), (Nix.job?.log ?? []).slice(-1)[0] ?? ""].filter(p => p).join(" · ")
                : Nix.jobState === "error" || Nix.jobState === "done"
                    ? `${Nix.job?.summary || ""} · ${Nix.rebuild}`
                    : Nix.rebuild
            visible: Nix.rebuild !== ""

            Button {
                icon: "file-text"
                text: "Registro"
                visible: !!(Nix.job?.logFile) && (Nix.job?.log?.length ?? 0) > 0
                onPressed: Nix.openLog()
            }

            Button {
                icon: "x-circle"
                text: "Detener"
                tone: Tokens.critical
                visible: Nix.running && !(Nix.job?.external ?? false)
                onPressed: Nix.stopJob()
            }

            Button {
                icon: "terminal"
                text: "En terminal"
                visible: !Nix.running
                onPressed: {
                    Tuning.hide();
                    Nix.build();
                }
            }

            Button {
                icon: "hammer"
                text: "Reconstruir"
                strong: !Nix.running && (Nix.pendingRebuild || Nix.unbuilt.length > 0 || Nix.dirty > 0)
                visible: !Nix.running
                onPressed: {
                    Tuning.hide();
                    Nix.rebuildSystem();
                }
            }
        }
    }

    // --- Actualizaciones del flake ---
    Group {
        title: "Actualizaciones del flake"
        note: Nix.checkError
        visible: Nix.flake !== ""

        Option {
            glyph: Nix.updates.length > 0 ? "arrow-circle-up" : "check-circle"
            tone: Nix.updates.length > 0 ? Tokens.accent : Tokens.dim
            title: Nix.checking
                ? "Comprobando entradas…"
                : Nix.updates.length > 0
                    ? `${Nix.updates.length} ${Nix.updates.length === 1 ? "entrada con actualizaciones" : "entradas con actualizaciones"}`
                    : Nix.checkedAt > 0
                        ? "Todas las entradas al día"
                        : "Sin comprobar todavía"
            text: Nix.checkedAt > 0 && !Nix.checking ? `Última comprobación ${Nix.ageText(Nix.checkedAt, root.now)}` : ""

            Button {
                icon: "arrows-clockwise"
                text: Nix.checking ? "Comprobando…" : "Comprobar"
                visible: !Nix.running
                onPressed: Nix.checkUpdates()
            }

            Button {
                icon: "arrow-down"
                text: "Actualizar flake"
                visible: !Nix.running
                onPressed: {
                    Tuning.hide();
                    Nix.updateFlake("");
                }
            }

            Button {
                icon: "sparkle"
                text: "Act. + Reconstruir"
                strong: true
                visible: !Nix.running && Nix.updates.length > 0 && Nix.rebuild !== ""
                onPressed: {
                    Tuning.hide();
                    Nix.upgradeSystem();
                }
            }
        }

        Repeater {
            model: Nix.updates

            Option {
                required property var modelData

                glyph: "package"
                tone: modelData.name === "nixpkgs" ? Tokens.accent : Tokens.dim
                title: modelData.name
                text: [
                    modelData.branch,
                    modelData.lastModified > 0 ? `bloqueada ${Nix.ageText(modelData.lastModified, root.now)}` : "",
                    modelData.currentRev ? `${modelData.currentRev.slice(0, 7)} → ${(modelData.remoteRev || "").slice(0, 7)}` : ""
                ].filter(p => p).join(" · ")

                Button {
                    icon: "arrow-down"
                    text: "Actualizar"
                    visible: !Nix.running
                    onPressed: {
                        Tuning.hide();
                        Nix.updateFlake(modelData.name);
                    }
                }
            }
        }
    }

    // --- Configuración declarada de Veyl (programs.veyl) ---
    Group {
        title: "Tu configuración de Veyl"
        visible: Nix.declared

        Option {
            glyph: "file-code"
            title: Nix.files.length > 1 ? "Archivos" : "Archivo"
            tone: Nix.problem ? Tokens.attention : Tokens.dim
            text: Nix.file ? (Nix.file.replace(Quickshell.env("HOME"), "~") + (Nix.files.length > 1 ? ` (+${Nix.files.length - 1})` : "")) : Nix.problem || "Buscando…"

            Button {
                icon: "arrow-square-out"
                text: "Abrir"
                visible: Nix.file !== ""
                onPressed: {
                    Tuning.hide();
                    Qt.openUrlExternally(`file://${Nix.file}`);
                }
            }
        }
    }

    Group {
        title: "No coincide con tu configuración"
        note: Nix.failure
        visible: Nix.declared && Nix.unsaved.length > 0

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
                icon: "arrow-counter-clockwise"
                text: "Deshacer todo"
                onPressed: Nix.revertAll()
            }

            Button {
                text: Nix.busy ? "Guardando…" : "Guardar todo"
                strong: true
                onPressed: Nix.save(Nix.unsaved)
            }
        }
    }

    Group {
        title: "Guardado, falta reconstruir"
        visible: Nix.declared && Nix.unbuilt.length > 0

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

        Option {
            glyph: "hammer"
            title: "Aplicar cambios"
            text: Nix.rebuild
            visible: Nix.rebuild !== ""

            Button {
                icon: "hammer"
                text: "Reconstruir"
                strong: true
                onPressed: {
                    Tuning.hide();
                    Nix.rebuildSystem();
                }
            }
        }
    }

    Group {
        visible: Nix.declared && Nix.drift.length === 0

        Option {
            glyph: "check-circle"
            tone: Tokens.accent
            title: "Todos los ajustes de Veyl coinciden con tu configuración"
        }
    }
}
