pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Hyprland
import Quickshell.Io

// El vistazo: lo que se revela mientras Super está mantenido.
Singleton {
    id: root

    property bool open: false
    // Espera antes de abrir, para que un atajo rápido con Super no lo dispare.
    property int hold: 200

    function press(): void {
        timer.restart();
    }

    function release(): void {
        timer.stop();
        open = false;
    }

    Timer {
        id: timer
        interval: root.hold
        // Con la entrada abierta, Super es parte de lo que se teclea.
        onTriggered: root.open = !Launcher.open
    }

    GlobalShortcut {
        appid: "veyl"
        name: "glance"
        description: "Vistazo mientras se mantiene"
        onPressed: root.press()
        onReleased: root.release()
    }

    IpcHandler {
        target: "glance"

        function press(): void {
            root.press();
        }

        function release(): void {
            root.release();
        }
    }
}
