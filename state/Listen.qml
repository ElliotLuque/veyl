pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Hyprland
import Quickshell.Io

// Modo escuchar: un borde del color del acento alrededor de toda la pantalla
// cuyo halo late con la voz. El micro lo abre veyld (state/Daemon.qml): oye
// una frase, la transcribe y la hace como una orden dicha. Con el atajo
// global `veyl:listen` (Veyl no le pone tecla: se la da el compositor) se
// escucha mientras la tecla está pulsada y, al soltarla, lo dicho es la
// frase. `veyl listen`, el IPC y el centro de control lo encienden y apagan;
// así deja de escuchar solo al acabar de hablar, o si no oye nada en un rato.
Singleton {
    id: root

    property bool active: false
    // Cuánto se oye por el micro, de 0 (silencio) a 1 (voz plena); solo
    // mientras escucha.
    property real level: 0
    // Sin micro: una voz inventada, para ver cómo responde (`veyl listen demo`).
    property bool demo: false
    // Con la tecla mantenida: la frase acaba al soltarla, no al callar.
    property bool hold: false
    // Ya se ha empezado a hablar, y lo último que se entendió.
    property bool speaking: false
    property string heard: ""

    function toggle(): void {
        active = !active;
    }

    onActiveChanged: {
        if (active) {
            speaking = false;
        } else {
            level = 0;
            demo = false;
            hold = false;
        }
        Daemon.send({ listen: active, demo, hold });
    }

    // Se pulsa la tecla: escucha mientras siga pulsada.
    function press(): void {
        hold = true;
        if (active)
            Daemon.send({ listen: true, demo, hold });
        else
            active = true;
    }

    // Se suelta: lo oído es la frase. veyld deja de escuchar al acabar de
    // transcribirla; sin veyld, no hay nada que esperar.
    function release(): void {
        if (!active || !hold)
            return;
        if (Daemon.connected)
            Daemon.send({ finish: true });
        else
            active = false;
    }

    // Lo que cuenta veyld mientras escucha: { level }, { speaking } o { heard }.
    function hear(voice: var): void {
        if (!active)
            return;
        if (voice.level !== undefined)
            level = Number(voice.level) || 0;
        if (voice.speaking)
            speaking = true;
        if (voice.heard !== undefined)
            heard = String(voice.heard);
    }

    GlobalShortcut {
        appid: "veyl"
        name: "listen"
        description: "Modo escuchar, mientras se mantiene pulsado"
        onPressed: root.press()
        onReleased: root.release()
    }

    IpcHandler {
        target: "listen"

        function toggle(): void {
            root.toggle();
        }

        function start(): void {
            root.active = true;
        }

        function stop(): void {
            root.active = false;
        }

        function demo(): void {
            root.demo = true;
            root.active = true;
        }

        function get(): bool {
            return root.active;
        }
    }
}
