pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Bluetooth
import Quickshell.Hyprland
import Quickshell.Io
import Quickshell.Services.Mpris
import Quickshell.Services.Pipewire

// Las órdenes de la entrada: lo que se le puede pedir al sistema sin abrir
// nada. Solo están las que se pueden cumplir: las que necesitan un programa
// que no hay, o algo que no existe (un reproductor, un micro), no salen.
Singleton {
    id: root

    // Lo que suena: el reproductor que está sonando o, si ninguno, el primero.
    readonly property MprisPlayer player: Mpris.players.values.find(player => player.isPlaying) ?? Mpris.players.values[0] ?? null
    readonly property BluetoothAdapter bluetooth: Bluetooth.defaultAdapter
    property bool wifi: false

    // Lo que hay que preguntar fuera se pregunta al abrir la entrada.
    function refresh(): void {
        if (Tools.has("nmcli"))
            radio.running = true;
    }

    // El cuentagotas: copia el color y lo avisa con una muestra suya de
    // imagen. Si se cancela (Esc), hyprpicker no dice nada y no hay aviso.
    // Sin -q: en hyprpicker calla también el color que imprime.
    readonly property string picker: `
        hex=$(hyprpicker -b -f hex 2>/dev/null | grep -o '#[0-9A-Fa-f]\\{6\\}' | head -n 1)
        [ -n "$hex" ] || exit 0
        printf %s "$hex" | wl-copy
        dir="\${XDG_CACHE_HOME:-$HOME/.cache}/veyl/colors"
        swatch="$dir/\${hex#\\#}.png"
        mkdir -p "$dir"
        [ -s "$swatch" ] || python3 -c '
import struct, sys, zlib
r, g, b = (int(sys.argv[1][i:i + 2], 16) for i in (1, 3, 5))
side = 64
chunk = lambda kind, data: struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
rows = (b"\\0" + bytes((r, g, b)) * side) * side
png = b"\\x89PNG\\r\\n\\x1a\\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", side, side, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b"")
open(sys.argv[2], "wb").write(png)
' "$hex" "$swatch" 2>/dev/null || swatch=""
        notify-send -a Veyl -i color-select \${swatch:+-h "string:image-path:$swatch"} "Color copiado" "$hex"`

    function shell(script: string): void {
        Quickshell.execDetached(["sh", "-c", script]);
    }

    // Una orden. `extra` puede traer: state (lo que enseña a la derecha, en
    // vivo), stay (no cierra la entrada), confirm (pide una segunda pulsación)
    // y verb (cómo se llama pulsarla).
    function order(id: string, title: string, group: string, glyph: string, words: string, run: var, extra: var): var {
        return Object.assign({
            key: `order:${id}`,
            kind: "order",
            title,
            subtitle: group,
            label: "Orden",
            glyph,
            verb: "Ejecutar",
            fields: [
                { text: Match.fold(title), weight: 1, loose: true },
                { text: Match.fold(words), weight: 0.7, loose: false },
                { text: Match.fold(group), weight: 0.5, loose: false }
            ],
            run
        }, extra);
    }

    readonly property var list: {
        const has = tool => Tools.has(tool);
        const level = () => Sound.muted ? "Silenciado" : `${Sound.volume} %`;
        const mic = () => Sound.micMuted ? "Silenciado" : `${Sound.micVolume} %`;
        const track = () => [root.player?.trackTitle, root.player?.trackArtist].filter(part => part).join(" · ") || root.player?.identity || "";
        // Lo que se hace sobre la pantalla espera a que la entrada se haya ido.
        const after = script => root.shell(`sleep 0.4; ${script}`);
        const list = [];

        list.push(order("clipboard", "Historial del portapapeles", "Veyl", "clipboard-text", "copiar pegar clipboard paste", () => Launcher.push("clipboard"), {
            verb: "Abrir",
            stay: true,
            state: () => Clipboard.entries.length > 0 ? `${Clipboard.entries.length}` : ""
        }));

        list.push(order("files", "Buscar archivos", "Veyl", "folder-open", "archivo fichero documento carpeta files buscar encontrar find", () => Launcher.push("files"), {
            verb: "Abrir",
            stay: true
        }));

        list.push(order("symbols", "Emojis y símbolos", "Veyl", "smiley", "emoji emoticono caritas simbolos flechas caracteres unicode", () => Launcher.push("symbols"), {
            verb: "Abrir",
            stay: true
        }));

        if (Sound.ready) {
            list.push(order("volume-up", "Subir el volumen", "Sonido", "speaker-high", "audio volume up mas alto", () => Sound.nudge(Sound.sink, 1), { stay: true, state: level }));
            list.push(order("volume-down", "Bajar el volumen", "Sonido", "speaker-low", "audio volume down mas bajo", () => Sound.nudge(Sound.sink, -1), { stay: true, state: level }));
            list.push(order("mute", "Silenciar el sonido", "Sonido", "speaker-x", "audio mute mudo callar altavoces", () => Sound.mute(Sound.sink), { stay: true, state: level }));
        }
        if (Sound.sinks.length > 0)
            list.push(order("outputs", "Cambiar la salida de audio", "Sonido", "headphones", "altavoces auriculares output speakers sink dispositivo", () => Launcher.push("outputs"), {
                verb: "Elegir",
                stay: true,
                state: () => Sound.label(Sound.sink)
            }));
        if (Sound.sources.length > 0)
            list.push(order("inputs", "Cambiar la entrada de audio", "Sonido", "microphone", "microfono micro input source dispositivo", () => Launcher.push("inputs"), {
                verb: "Elegir",
                stay: true,
                state: () => Sound.label(Sound.source)
            }));
        if (Sound.micReady) {
            list.push(order("mic-mute", "Silenciar el micrófono", "Sonido", "microphone-slash", "micro mute mudo entrada", () => Sound.mute(Sound.source), { stay: true, state: mic }));
            list.push(order("mic-up", "Subir el volumen del micrófono", "Sonido", "microphone", "micro entrada ganancia mas", () => Sound.nudge(Sound.source, 1), { stay: true, state: mic }));
            list.push(order("mic-down", "Bajar el volumen del micrófono", "Sonido", "microphone", "micro entrada ganancia menos", () => Sound.nudge(Sound.source, -1), { stay: true, state: mic }));
        }

        if (root.player) {
            if (root.player.canTogglePlaying)
                list.push(order("play", "Reproducir o pausar", "Multimedia", "play-pause", "musica play pause parar", () => root.player.togglePlaying(), { stay: true, state: track }));
            if (root.player.canGoNext)
                list.push(order("next", "Pista siguiente", "Multimedia", "skip-forward", "musica next cancion saltar", () => root.player.next(), { stay: true, state: track }));
            if (root.player.canGoPrevious)
                list.push(order("previous", "Pista anterior", "Multimedia", "skip-back", "musica previous cancion", () => root.player.previous(), { stay: true, state: track }));
        }

        if (Daemon.connected)
            list.push(order("views", "Vistas", "Veyl", "squares-four", "vista escritorio contexto espacio de trabajo workspace organizar ordenar estudio proyecto tarea", () => Launcher.push("views"), {
                verb: "Abrir",
                stay: true,
                state: () => Daemon.views.length > 0 ? `${Daemon.views.length}` : ""
            }));

        list.push(order("schemes", "Cambiar el esquema de color", "Veyl", "palette", "tema colores theme scheme", () => Launcher.push("schemes"), {
            verb: "Elegir",
            stay: true,
            state: () => Scheme.colors.name ?? Scheme.name
        }));
        if (Daemon.connected)
            list.push(order("diary", "Diario", "Veyl", "notebook", "recall historial que hice que estuve haciendo cronologia linea del tiempo actividad buscar recordar memoria journal", () => Recall.show(Launcher.monitor, "", ""), { verb: "Abrir" }));
        list.push(order("settings", "Ajustes", "Veyl", "gear-six", `configuracion preferencias opciones settings sistema panel de control ${Tuning.sections.map(section => section.title).join(" ")}`, () => Tuning.show(Launcher.monitor, ""), { verb: "Abrir" }));
        list.push(order("listen", "Modo escuchar", "Veyl", "waveform", "voz micro listen", () => Listen.toggle(), { state: () => Listen.active ? "Escuchando" : "" }));
        if (Clipboard.available)
            list.push(order("clipboard-clear", "Vaciar el historial del portapapeles", "Veyl", "broom", "borrar limpiar clipboard clear", () => Clipboard.run("clear", ""), { stay: true, confirm: true }));
        if (Calc.available)
            list.push(order("rates", "Actualizar los tipos de cambio", "Veyl", "currency-eur", "divisas monedas calculadora qalc exchange rates", () => Calc.update()));
        if (Gallery.count > 0)
            list.push(order("cards", "Galería de tarjetas", "Veyl", "cards", "ejemplos tarjetas cerebro respuestas piezas demo previsualizar", () => Gallery.show(0)));

        if (Shot.zones)
            list.push(order("shot-area", "Capturar una zona de la pantalla", "Pantalla", "selection", "captura screenshot recorte", () => Shot.take(Launcher.monitor, true, 0.05), { verb: "Capturar" }));
        if (Shot.available)
            list.push(order("shot-screen", "Capturar la pantalla", "Pantalla", "monitor", "captura screenshot entera", () => Shot.take(Launcher.monitor, false, 0.05), { verb: "Capturar" }));
        if (has("hyprpicker"))
            list.push(order("picker", "Elegir un color de la pantalla", "Pantalla", "eyedropper", "cuentagotas color picker hex", () => after(root.picker)));

        if (has("nmcli"))
            list.push(order("wifi", "Encender o apagar el Wi-Fi", "Red", "wifi-high", "wireless internet red", () => {
                root.shell(`nmcli radio wifi ${root.wifi ? "off" : "on"}`);
                root.wifi = !root.wifi;
            }, { stay: true, state: () => root.wifi ? "Encendido" : "Apagado" }));
        if (root.bluetooth)
            list.push(order("bluetooth", "Encender o apagar el Bluetooth", "Red", "bluetooth", "inalambrico auriculares", () => root.bluetooth.enabled = !root.bluetooth.enabled, {
                stay: true,
                state: () => root.bluetooth?.enabled ? "Encendido" : "Apagado"
            }));

        list.push(order("lock", "Bloquear la pantalla", "Sesión", "lock", "lock candado", () => Lock.lock()));
        if (has("systemctl"))
            list.push(order("suspend", "Suspender", "Sesión", "moon", "dormir sleep suspend", () => root.shell("systemctl suspend")));
        list.push(order("logout", "Cerrar la sesión", "Sesión", "sign-out", "salir logout exit", () => Compositor.run("hl.dispatch(hl.dsp.exit())"), { confirm: true }));
        if (has("systemctl")) {
            list.push(order("reboot", "Reiniciar", "Sesión", "arrow-clockwise", "reboot restart", () => root.shell("systemctl reboot"), { confirm: true }));
            list.push(order("poweroff", "Apagar", "Sesión", "power", "shutdown poweroff", () => root.shell("systemctl poweroff"), { confirm: true }));
        }
        list.push(order("reload", "Recargar Veyl", "Veyl", "arrows-clockwise", "reload reiniciar shell", () => Quickshell.reload(false)));
        return list;
    }

    // Un dispositivo de sonido entre los que elegir.
    function device(node: PwNode): var {
        const name = Sound.label(node);
        return {
            key: `device:${node.id}`,
            kind: "device",
            title: name,
            label: "",
            glyph: !node.isSink ? "microphone" : /auricular|headphone|headset|cascos/i.test(name) ? "headphones" : "speaker-high",
            verb: "Usar",
            stay: true,
            fields: [{ text: Match.fold(name), weight: 1, loose: true }],
            current: () => node === (node.isSink ? Sound.sink : Sound.source),
            // Sin canales que medir (un puerto sin conectar), nada que decir.
            state: () => !node.audio ? "" : node.audio.muted ? "Silenciado" : isNaN(node.audio.volume) ? "" : `${Math.round(node.audio.volume * 100)} %`,
            run: () => Sound.use(node)
        };
    }

    // Una vista guardada: Intro va a ella (si está cerrada, la recrea).
    function view(view: var): var {
        const count = n => `${n} ${n === 1 ? "ventana" : "ventanas"}`;
        const actions = [
            { title: "Recrear: traer lo suyo y abrir lo que falta", glyph: "arrows-clockwise", keys: "ctrl+r", run: () => Daemon.view("again", view.name, false) },
            { title: "Guardar lo que tiene ahora", glyph: "floppy-disk", keys: "ctrl+s", run: () => Daemon.view("save", view.name, false) }
        ];
        if (view.open)
            actions.push({ title: "Deshacer: cada ventana a su nivel", glyph: "arrow-u-up-left", keys: "ctrl+d", run: () => Daemon.view("undo", view.name, false) });
        actions.push({ title: "Olvidar la vista", glyph: "trash", keys: "ctrl+x", confirm: true, run: () => Daemon.view("forget", view.name, false) });
        return {
            key: `view:${view.name}`,
            kind: "view",
            title: view.name,
            subtitle: view.apps.join(" · "),
            label: view.open ? "" : "Guardada",
            glyph: "squares-four",
            verb: view.open ? "Ir a la vista" : "Recrear la vista",
            fields: [
                { text: Match.fold(view.name), weight: 1, loose: true },
                { text: Match.fold(view.apps.join(" ")), weight: 0.6, loose: false }
            ],
            current: () => Hyprland.focusedWorkspace?.name === view.name,
            state: () => view.open ? count(view.open) : "",
            run: () => Daemon.view("open", view.name, false),
            more: () => actions
        };
    }

    // Lo escrito en las vistas, si no es ninguna: una nueva con ese nombre.
    function newView(name: string): var {
        return {
            key: "view:new",
            kind: "view",
            title: `Crear la vista «${name}»`,
            subtitle: "Un nivel nuevo con lo tuyo que ya está abierto",
            label: "Nueva",
            glyph: "plus",
            verb: "Crear",
            run: () => Daemon.view("create", name, false),
            more: () => [
                    { title: "Crearla con lo de este nivel", glyph: "stack", keys: "ctrl+return", run: () => Daemon.view("create", name, true) }
                ]
        };
    }

    // Un esquema de color entre los que elegir.
    function scheme(scheme: var): var {
        return {
            key: `scheme:${scheme.id}`,
            kind: "scheme",
            title: scheme.name,
            label: "",
            glyph: "palette",
            swatches: scheme.colors,
            verb: "Usar",
            stay: true,
            fields: [{ text: Match.fold(`${scheme.name} ${scheme.id}`), weight: 1, loose: true }],
            current: () => Scheme.name === scheme.id,
            run: () => Settings.scheme = scheme.id
        };
    }

    Process {
        id: radio
        command: ["nmcli", "-t", "-f", "WIFI", "radio"]
        environment: ({ LC_ALL: "C" })
        stdout: StdioCollector {
            onStreamFinished: root.wifi = text.trim() === "enabled"
        }
    }
}
