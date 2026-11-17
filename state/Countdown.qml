pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io

// El temporizador: una cuenta atrás de unos minutos. Mientras corre va en la
// isla de la hora; al acabar avisa con una notificación que no caduca. Se
// pone desde el almanaque, con `veyl timer` o por IPC, y una recarga del
// shell no lo para.
Singleton {
    id: root

    property alias until: kept.until    // ms desde epoch en que acaba; 0 = parado
    property alias span: kept.span      // minutos que se pidieron
    property alias preset: kept.preset  // minutos que propone el almanaque
    readonly property var presets: [1, 3, 5, 10, 15, 20, 25, 30, 45, 60, 90, 120]
    property double now: Date.now()     // solo avanza mientras corre

    readonly property bool running: until > 0
    readonly property double left: running ? Math.max(0, until - now) : 0  // ms
    readonly property real progress: running && span > 0 ? Math.min(1, 1 - left / (span * 60000)) : 0
    // En la isla, sin prisa: minutos y, en el último, segundos.
    readonly property string brief: left > 59000 ? `${Math.ceil(left / 60000)} min` : `${Math.ceil(left / 1000)} s`
    // En el almanaque, al segundo
    readonly property string exact: {
        const seconds = Math.ceil(left / 1000);
        const pad = part => String(part).padStart(2, "0");
        const minutes = Math.floor(seconds / 60) % 60;
        return seconds >= 3600 ? `${Math.floor(seconds / 3600)}:${pad(minutes)}:${pad(seconds % 60)}` : `${minutes}:${pad(seconds % 60)}`;
    }

    function start(minutes: int): void {
        if (minutes < 1)
            return;
        span = minutes;
        now = Date.now();
        until = now + minutes * 60000;
    }

    function stop(): void {
        until = 0;
    }

    // Sube o baja lo que propone, de escalón en escalón.
    function nudge(way: int): void {
        const next = way > 0 ? presets.find(minutes => minutes > preset) : presets.slice().reverse().find(minutes => minutes < preset);
        if (next)
            preset = next;
    }

    function finish(): void {
        until = 0;
        herald.createObject(root, { body: `Han pasado ${span} min.` });
    }

    PersistentProperties {
        id: kept
        reloadableId: "countdown"

        property double until: 0
        property int span: 0
        property int preset: 25
    }

    Timer {
        interval: 1000
        running: root.running
        repeat: true
        triggeredOnStart: true
        onTriggered: {
            root.now = Date.now();
            if (root.now >= root.until)
                root.finish();
        }
    }

    // El aviso va por donde van todos: al servidor de notificaciones, sea
    // Veyl u otro. Pide quedarse hasta que se atienda, y su sonido.
    Component {
        id: herald

        Process {
            required property string body

            running: true
            command: ["sh", "-c", `
                notify-send -a Veyl -t 0 -h string:sound-name:alarm-clock-elapsed "$1" "$2" 2>/dev/null ||
                busctl --user call org.freedesktop.Notifications /org/freedesktop/Notifications org.freedesktop.Notifications Notify 'susssasa{sv}i' Veyl 0 '' "$1" "$2" 0 1 sound-name s alarm-clock-elapsed 0`, "sh", "Temporizador", body]
            onExited: destroy()
        }
    }

    IpcHandler {
        target: "timer"

        function start(minutes: int): string {
            if (minutes < 1)
                return "¿cuántos minutos? (veyl timer 25)";
            root.start(minutes);
            return "";
        }

        function stop(): void {
            root.stop();
        }

        // Lo que le queda (m:ss), o nada si está parado.
        function get(): string {
            return root.running ? root.exact : "";
        }
    }
}
