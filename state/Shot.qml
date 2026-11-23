pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Hyprland
import Quickshell.Io

// Las capturas: una zona que recortas (con slurp) o la pantalla entera. Se
// guardan en ~/Pictures/Screenshots, como las de screenshot-area, se copian
// al portapapeles y se avisan con la propia imagen; pulsar el aviso la abre. La de la pantalla entera, además,
// destella un instante en su monitor, como al hacer una foto.
Singleton {
    id: root

    readonly property bool available: Tools.has("grim") && Tools.has("wl-copy")
    readonly property bool zones: available && Tools.has("slurp")

    // La que está en marcha: mientras eliges la zona, otra no empieza (dos
    // slurp a la vez se quitan el puntero y se quedan colgados).
    property Process current: null
    // Mientras se captura: la entrada y el centro se apartan ya, sin plegarse
    // (ver `tucked` en base/Palette.qml y base/Veil.qml), para no salir.
    readonly property bool hiding: current !== null

    // El monitor que acaba de capturarse entero: su destello lo escucha.
    signal flashed(string monitor)

    // `delay`: lo que esperar antes, para que se aparte lo que la pidió (un
    // par de fotogramas basta: no se espera a que se plieguen).
    function take(monitor: string, area: bool, delay: real): void {
        if (current || (area ? !zones : !available))
            return;
        // Sin monitor, grim las juntaría todas: la del monitor con foco.
        current = shot.createObject(root, { monitor: monitor || (Hyprland.focusedMonitor?.name ?? ""), area, delay });
    }

    Component {
        id: shot

        Process {
            required property string monitor
            required property bool area
            required property real delay

            running: true
            // slurp lee zonas de stdin si no es una terminal: sin </dev/null
            // se queda esperando y nunca enseña la selección.
            // Escribe dónde la dejó; si eliges la zona y la cancelas (Esc),
            // nada y sin aviso.
            command: ["sh", "-c", `
                sleep "$3"
                if [ "$2" = area ]; then
                    zone=$(slurp -d </dev/null 2>/dev/null) || exit 1
                fi
                dir="$HOME/Pictures/Screenshots"
                mkdir -p "$dir" || exit 1
                file="$dir/snapshot_$(date '+%Y-%m-%d_%H:%M:%S').png"
                if [ "$2" = area ]; then
                    grim -g "$zone" "$file" || exit 1
                else
                    grim \${1:+-o "$1"} "$file" || exit 1
                fi
                wl-copy --type image/png < "$file"
                printf %s "$file"`, "sh", monitor, area ? "area" : "screen", String(delay)]
            stdout: StdioCollector {
                id: out
            }
            onExited: code => {
                const file = out.text.trim();
                if (code === 0 && file) {
                    if (!area)
                        root.flashed(monitor);
                    root.tell(file);
                }
                root.current = null;
                destroy();
            }
        }
    }

    // SUPER + S en Hyprland: hl.dsp.global("veyl:shot").
    GlobalShortcut {
        appid: "veyl"
        name: "shot"
        description: "Recortar una zona de la pantalla"
        onPressed: root.take("", true, 0)
    }

    GlobalShortcut {
        appid: "veyl"
        name: "shot-screen"
        description: "Capturar la pantalla entera"
        onPressed: root.take("", false, 0)
    }

    IpcHandler {
        target: "shot"

        // En el monitor con foco: `veyl shot area` o `veyl shot screen`.
        function area(): void {
            root.take("", true, 0);
        }

        function screen(): void {
            root.take("", false, 0);
        }
    }

    // El aviso, con la captura de imagen. Espera a que lo atiendas: pulsarlo
    // la abre; «Carpeta», donde se guardó.
    function tell(file: string): void {
        Quickshell.execDetached(["sh", "-c", `
            act=$(notify-send -a Veyl -i camera-photo -h "string:image-path:$1" -A default=Abrir -A folder=Carpeta "Captura copiada" "\${1##*/}") || exit 0
            case $act in
                default) xdg-open "$1" ;;
                folder) xdg-open "\${1%/*}" ;;
            esac`, "sh", file]);
    }
}
