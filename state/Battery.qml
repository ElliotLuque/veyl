pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io

// Batería leída de sysfs, sin depender de ningún servicio.
Singleton {
    id: root

    // VEYL_POWER_SUPPLY permite apuntar a un árbol de prueba.
    readonly property string path: Quickshell.env("VEYL_POWER_SUPPLY") || "/sys/class/power_supply"
    property int lowAt: 20
    property int criticalAt: 8

    property bool present: false
    property int percent: 100
    property bool discharging: false
    readonly property bool low: present && discharging && percent <= lowAt
    readonly property bool critical: low && percent <= criticalAt

    Process {
        id: probe
        // Primera batería del sistema; las de periféricos (scope=Device) no cuentan.
        command: ["sh", "-c", `
            for d in "$0"/*/; do
                [ "$(cat "$d/type" 2>/dev/null)" = Battery ] || continue
                [ "$(cat "$d/scope" 2>/dev/null)" = Device ] && continue
                echo "$(cat "$d/capacity") $(cat "$d/status")"
                break
            done`, root.path]
        stdout: StdioCollector {
            onStreamFinished: {
                const [percent, status] = text.trim().split(" ");
                root.present = !!status;
                if (!root.present)
                    return;
                root.percent = parseInt(percent);
                root.discharging = status === "Discharging";
            }
        }
    }

    Timer {
        interval: 10000
        running: true
        repeat: true
        triggeredOnStart: true
        onTriggered: probe.running = true
    }
}
