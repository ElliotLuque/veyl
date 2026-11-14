pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io

// El esquema de color activo. Un esquema es un JSON que le da un color a cada
// rol de Tokens.qml (ver schemes/). Los del usuario, en
// ~/.config/veyl/schemes, tienen prioridad sobre los que trae Veyl.
Singleton {
    id: root

    readonly property string fallback: "living-code-night"
    readonly property string name: Settings.scheme || fallback
    readonly property string own: `${Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME") + "/.config"}/veyl/schemes`
    readonly property string bundled: Quickshell.shellPath("schemes")
    // Rol → color. Lo que al esquema elegido le falte lo pone el de fábrica.
    property var colors: Object.assign({}, parse(base.text()))

    // Los esquemas que hay para elegir ({ id, name, colors }); se miran con scan().
    property var all: []

    function scan(): void {
        finder.running = true;
    }

    function refresh(): void {
        const chosen = mine.found ? mine.text() : stock.found ? stock.text() : "";
        colors = Object.assign({}, parse(base.text()), parse(chosen));
    }

    function check(): void {
        if (!mine.found && !stock.found)
            console.warn(`No existe el esquema «${name}»; se queda ${fallback}`);
    }

    function parse(text: string): var {
        if (!text)
            return {};
        try {
            const scheme = JSON.parse(text);
            if (scheme && typeof scheme === "object" && !Array.isArray(scheme))
                return scheme;
        } catch (error) {}
        console.warn(`El esquema «${name}» no es un JSON válido`);
        return {};
    }

    // Un sitio donde puede estar el esquema elegido; editarlo se ve al momento.
    component Source: FileView {
        property bool found: false

        blockLoading: true
        printErrors: false
        watchChanges: true
        onFileChanged: reload()
        onLoaded: {
            found = true;
            root.refresh();
        }
        onLoadFailed: {
            found = false;
            root.refresh();
            // El otro sitio puede no haberse mirado aún.
            Qt.callLater(root.check);
        }
    }

    FileView {
        id: base
        path: `${root.bundled}/${root.fallback}.json`
        blockLoading: true
        onLoaded: root.refresh()
    }

    Source {
        id: mine
        path: `${root.own}/${root.name}.json`
    }

    Source {
        id: stock
        path: `${root.bundled}/${root.name}.json`
    }

    // Cada archivo de esquema, en una línea; sin jq, solo su ruta. Los del
    // usuario van delante: tapan a los de Veyl con su mismo nombre.
    Process {
        id: finder
        command: ["sh", "-c", `
            own=$0 bundled=$1
            set --
            for file in "$own"/*.json "$bundled"/*.json; do [ -f "$file" ] && set -- "$@" "$file"; done
            [ $# -gt 0 ] || exit 0
            jq -c '{ file: input_filename, name, bg, surface, border, accent, text }' "$@" 2>/dev/null ||
                for file; do printf '{"file":"%s"}\n' "$file"; done`, root.own, root.bundled]
        stdout: StdioCollector {
            onStreamFinished: {
                const all = [];
                for (const line of text.split("\n")) {
                    let scheme;
                    try {
                        scheme = JSON.parse(line);
                    } catch (error) {
                        continue;
                    }
                    const id = scheme.file.split("/").pop().replace(/\.json$/, "");
                    if (all.some(other => other.id === id))
                        continue;
                    all.push({
                        id,
                        name: scheme.name || id,
                        colors: ["bg", "surface", "border", "accent", "text"].map(role => scheme[role] || root.parse(base.text())[role])
                    });
                }
                root.all = all.sort((a, b) => a.name.localeCompare(b.name));
            }
        }
    }

    IpcHandler {
        target: "scheme"

        function set(name: string): void {
            Settings.scheme = name;
        }

        function get(): string {
            return root.name;
        }
    }
}
