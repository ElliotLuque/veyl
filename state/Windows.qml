pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Hyprland

// Dónde está cada ventana. Hyprland solo cuenta la geometría si se le
// pregunta: se le pregunta cada vez que avisa de que algo ha cambiado.
Singleton {
    id: root

    // Avisos que no mueven ninguna ventana.
    readonly property list<string> still: ["windowtitle", "windowtitlev2", "activelayout", "submap", "openlayer", "closelayer", "urgent", "bell", "screencast", "custom"]

    // Las ventanas en mosaico de un nivel, de izquierda a derecha. Se miden
    // en pantallas desde la esquina del monitor: x = 1 es su borde derecho,
    // y lo que pase de ahí está fuera de la vista.
    function tiles(workspace: HyprlandWorkspace): var {
        const monitor = workspace?.monitor;
        if (!monitor)
            return [];
        const width = monitor.width / monitor.scale;
        const height = monitor.height / monitor.scale;
        const tiles = [];
        for (const toplevel of workspace.toplevels.values) {
            const ipc = toplevel.lastIpcObject;
            if (!ipc.at || ipc.floating || ipc.hidden)
                continue;
            tiles.push({
                address: toplevel.address,
                active: toplevel.activated,
                x: (ipc.at[0] - monitor.x) / width,
                y: (ipc.at[1] - monitor.y) / height,
                w: ipc.size[0] / width,
                h: ipc.size[1] / height
            });
        }
        return tiles.sort((a, b) => a.x - b.x || a.y - b.y);
    }

    // Qué aplicaciones hay en un nivel, sin repetir y en el orden de la tira.
    function apps(workspace: HyprlandWorkspace): list<string> {
        if (!workspace)
            return [];
        const windows = workspace.toplevels.values.map(t => t.lastIpcObject).filter(ipc => ipc.class);
        windows.sort((a, b) => a.at[0] - b.at[0]);
        // «org.mozilla.firefox» es «firefox».
        const names = windows.map(ipc => ipc.class.split(".").pop().toLowerCase());
        return names.filter((name, i) => names.indexOf(name) === i);
    }

    Component.onCompleted: Hyprland.refreshToplevels()

    Connections {
        target: Hyprland

        function onRawEvent(event: HyprlandEvent): void {
            if (!root.still.includes(event.name))
                soon.restart();
        }
    }

    Timer {
        id: soon
        interval: 30
        onTriggered: {
            Hyprland.refreshToplevels();
            settled.restart();
        }
    }

    // Mover o redimensionar una ventana no avisa: con la torre a la vista
    // se pregunta de seguido.
    Timer {
        interval: 250
        repeat: true
        running: Glance.open
        onTriggered: Hyprland.refreshToplevels()
    }

    // Y otra vez cuando el compositor ha terminado de animar el cambio.
    Timer {
        id: settled
        interval: 500
        onTriggered: Hyprland.refreshToplevels()
    }
}
