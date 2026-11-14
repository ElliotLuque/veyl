pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Hyprland
import Quickshell.Io
import Quickshell.Services.Pipewire

// Micro, cámara y pantalla compartida en uso.
Singleton {
    id: root

    // Algún programa graba de una entrada real (no del monitor de una salida).
    readonly property bool mic: Pipewire.linkGroups.values.some(g => {
        const source = g.source, target = g.target;
        return !!source && !!target && target.isStream && !source.isStream && !source.isSink && (source.type & PwNodeType.AudioSource) === PwNodeType.AudioSource;
    })
    property bool camera: false
    property int casts: 0
    readonly property bool screen: casts > 0

    // Cámara: algún proceso tiene abierto un /dev/video*, directo o vía PipeWire.
    Process {
        id: probe
        command: ["sh", "-c", `find /proc/[0-9]*/fd -lname "/dev/video*" -print -quit 2>/dev/null | grep -q .`]
        onExited: code => root.camera = code === 0
    }

    Timer {
        interval: 3000
        running: true
        repeat: true
        triggeredOnStart: true
        onTriggered: probe.running = true
    }

    Connections {
        target: Hyprland

        function onRawEvent(event: HyprlandEvent): void {
            if (event.name === "screencast")
                root.casts = Math.max(0, root.casts + (event.data.startsWith("1") ? 1 : -1));
        }
    }
}
