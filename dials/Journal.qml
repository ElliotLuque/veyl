import QtQuick
import qs
import qs.base
import qs.state

// Diario y órdenes: lo que veyld apunta, quién contesta lo que la gramática
// no entiende, y lo que usa la entrada (la búsqueda web y la terminal).
Column {
    id: root

    spacing: 22

    Group {
        title: "Diario"
        note: "Se guarda solo en este equipo y se conserva 90 días."

        Option {
            glyph: "notebook"
            title: "Registrar actividad"
            text: "Ventanas, páginas, comandos y música"

            Toggle {
                on: Settings.journal
                onToggled: on => Settings.journal = on
            }
        }
    }

    Group {
        title: "Órdenes"

        Option {
            glyph: "sparkle"
            title: "Comando de respaldo"
            text: "Responde a las órdenes no reconocidas"

            Field {
                width: 360
                value: Settings.llm
                placeholder: "Ninguno"
                onCommitted: text => Settings.llm = text.trim()
            }
        }
    }

    Group {
        title: "Lanzador"

        Option {
            glyph: "globe"
            title: "Búsqueda web"

            Field {
                width: 360
                value: Settings.webSearch
                placeholder: "https://duckduckgo.com/?q=%s"
                onCommitted: text => Settings.webSearch = text.trim()
            }
        }

        Option {
            glyph: "terminal"
            title: "Terminal"

            Field {
                width: 240
                value: Settings.terminal
                placeholder: Apps.terminal
                onCommitted: text => Settings.terminal = text.trim()
            }
        }
    }
}
