import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

// Sistema: de qué máquina se trata y cómo está este Veyl.
Column {
    id: root

    readonly property var info: Host.info
    readonly property string settings: `${Quickshell.env("XDG_STATE_HOME") || Quickshell.env("HOME") + "/.local/state"}/veyl/settings.json`

    spacing: 22

    Component.onCompleted: Host.refresh("about")

    function bytes(kib: real): string {
        const gib = kib / 1048576;
        return gib >= 1024 ? `${(gib / 1024).toFixed(1)} TB` : gib >= 10 ? `${Math.round(gib)} GB` : `${gib.toFixed(1)} GB`;
    }

    function since(seconds: real): string {
        const minutes = Math.floor(seconds / 60);
        const days = Math.floor(minutes / 1440);
        const hours = Math.floor(minutes % 1440 / 60);
        return days > 0 ? `${days} d ${hours} h` : hours > 0 ? `${hours} h ${minutes % 60} min` : `${minutes} min`;
    }

    // La máquina, en grande, en el sitio de la fila de una isla
    Item {
        width: parent.width
        height: 66

        Icon {
            id: glyph
            x: 4
            anchors.verticalCenter: parent.verticalCenter
            name: "desktop"
            color: Tokens.accent
            font.pixelSize: 40
        }

        Column {
            anchors.left: glyph.right
            anchors.leftMargin: 16
            anchors.verticalCenter: parent.verticalCenter
            spacing: 2

            Ink {
                text: root.info.host ?? "…"
                font.pixelSize: Tokens.figureSize
                strong: true
            }

            Ink {
                text: [root.info.os, root.info.uptime ? `Tiempo activo: ${root.since(parseFloat(root.info.uptime))}` : ""].filter(part => part).join(" · ")
                dim: true
                font.pixelSize: Tokens.smallSize
            }
        }
    }

    Group {
        title: "Máquina"

        Repeater {
            model: [
                { glyph: "cpu", title: "Procesador", text: root.info.cpu ? `${root.info.cpu} · ${root.info.cores} hilos` : "" },
                { glyph: "monitor", title: "Gráfica", text: root.info.gpu ?? "" },
                { glyph: "memory", title: "Memoria", text: root.info.memory ? `${root.bytes(root.info.memory - root.info.available)} en uso de ${root.bytes(root.info.memory)}` : "" },
                { glyph: "hard-drives", title: "Disco", text: root.info.disk ? `${root.bytes(root.info.free)} libres de ${root.bytes(root.info.disk)}` : "" },
                { glyph: "terminal", title: "Kernel", text: root.info.kernel ? `Linux ${root.info.kernel}` : "" }
            ].filter(row => row.text)

            Option {
                required property var modelData

                glyph: modelData.glyph
                title: modelData.title
                text: modelData.text
            }
        }
    }

    Group {
        title: "Veyl"

        Option {
            glyph: "gear-six"
            title: "Entorno"
            text: [root.info.hyprland ? `Hyprland ${root.info.hyprland}` : "", root.info.quickshell ?? ""].filter(part => part).join(" · ")
        }

        Option {
            glyph: "cpu"
            title: "veyld"
            tone: Daemon.connected ? Tokens.accent : Tokens.attention
            text: Daemon.connected ? "En ejecución" : "Sin respuesta; reiniciando…"
        }

        Option {
            glyph: "file-text"
            title: "Archivo de configuración"
            text: root.settings.replace(Quickshell.env("HOME"), "~")

            Button {
                icon: "arrow-square-out"
                text: "Abrir"
                onPressed: {
                    Tuning.hide();
                    Qt.openUrlExternally(`file://${root.settings}`);
                }
            }
        }

        Option {
            glyph: "arrows-clockwise"
            title: "Recargar Veyl"

            Button {
                text: "Recargar"
                onPressed: Quickshell.reload(false)
            }
        }
    }
}
