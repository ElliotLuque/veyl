pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io

// El centro de control (base/Controls.qml): en qué página está y si se ha
// pedido desde fuera. Con el puntero se abre solo (base/Veil.qml); `veyl
// controls` o el IPC lo sacan en el monitor con foco hasta que el puntero
// entra y sale, o hasta que se cierra.
Singleton {
    id: root

    // home (lo de siempre), veyl (lo que le has pedido) o notes (las notificaciones)
    readonly property var pages: ["home", "veyl", "notes"]
    property string page: "home"
    property bool pinned: false

    function show(page: string): void {
        if (pages.includes(page))
            root.page = page;
        pinned = true;
    }

    function hide(): void {
        pinned = false;
    }

    IpcHandler {
        target: "controls"

        // Abre el centro en una página: home, veyl o notes (sin nada, la de antes).
        function open(page: string): string {
            if (page && !root.pages.includes(page))
                return `no existe la página ${page}`;
            root.show(page);
            return "";
        }

        function close(): void {
            root.hide();
        }

        function toggle(): void {
            if (root.pinned)
                root.hide();
            else
                root.show("");
        }
    }
}
