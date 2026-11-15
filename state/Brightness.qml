pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io

// El brillo de la pantalla, leído de sysfs: la primera retroiluminación que
// haya. Sin ella (un sobremesa con monitores externos) no está, y nada lo
// enseña. sysfs no avisa de los cambios: se relee a menudo, que es barato.
Singleton {
    id: root

    // VEYL_BACKLIGHT permite apuntar a un árbol de prueba.
    readonly property string path: Quickshell.env("VEYL_BACKLIGHT") || "/sys/class/backlight"
    readonly property int step: 5  // puntos que sube o baja de una vez

    property string device: ""
    property int max: 0
    property int raw: 0
    readonly property bool present: device !== "" && max > 0
    readonly property int percent: present ? Math.round(raw * 100 / max) : 0

    // Sube o baja a saltos de `step`, sin llegar a apagarla del todo.
    function nudge(way: int): void {
        if (!present)
            return;
        const next = way > 0 ? Math.floor(percent / step) * step + step : Math.ceil(percent / step) * step - step;
        set(Math.max(1, Math.min(100, next)));
    }

    function set(percent: int): void {
        if (present)
            Quickshell.execDetached(["brightnessctl", "-q", "-d", device, "set", `${percent}%`]);
    }

    Process {
        id: probe
        command: ["sh", "-c", `for d in "$0"/*/; do [ -r "$d/max_brightness" ] && { basename "$d"; cat "$d/max_brightness"; break; }; done`, root.path]
        stdout: StdioCollector {
            onStreamFinished: {
                const [device, max] = text.trim().split("\n");
                root.device = device ?? "";
                root.max = parseInt(max) || 0;
            }
        }
        Component.onCompleted: running = true
    }

    FileView {
        id: level
        path: root.present ? `${root.path}/${root.device}/brightness` : ""
        onLoaded: root.raw = parseInt(text()) || 0
    }

    Timer {
        interval: 200
        running: root.present
        repeat: true
        onTriggered: level.reload()
    }
}
