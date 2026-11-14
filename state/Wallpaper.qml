pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io

// El fondo de pantalla: una imagen que Veyl pinta bajo todo lo demás (ver
// base/Backdrop.qml). Sin imagen, queda el color de fondo del esquema.
Singleton {
    id: root

    readonly property string path: Settings.wallpaper
    readonly property url source: path ? "file://" + path.split("/").map(encodeURIComponent).join("/") : ""

    IpcHandler {
        target: "wallpaper"

        function set(path: string): void {
            Settings.wallpaper = path;
        }

        function clear(): void {
            Settings.wallpaper = "";
        }

        function get(): string {
            return root.path;
        }
    }
}
