pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io

// Qué programas de fuera hay en la máquina. La entrada solo ofrece lo que
// se puede hacer: se mira al arrancar y cada vez que se abre.
Singleton {
    id: root

    readonly property list<string> wanted: ["qalc", "wl-copy", "wl-paste", "python3", "jq", "systemctl", "nmcli", "grim", "slurp", "hyprpicker", "xdg-open", "fd"]
    property var found: ({})

    function has(tool: string): bool {
        return found[tool] === true;
    }

    function probe(): void {
        search.running = true;
    }

    Process {
        id: search
        running: true
        command: ["sh", "-c", 'for tool; do command -v "$tool" >/dev/null 2>&1 && echo "$tool"; done; :', "sh"].concat(root.wanted)
        stdout: StdioCollector {
            onStreamFinished: {
                const found = {};
                for (const tool of text.split("\n"))
                    if (tool)
                        found[tool] = true;
                // Sin cambios no se toca: de esto depende qué órdenes hay.
                if (JSON.stringify(found) !== JSON.stringify(root.found))
                    root.found = found;
            }
        }
    }
}
