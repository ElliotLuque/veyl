pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io

// El historial del portapapeles. Lo lleva bin/veyl-clip, que Veyl deja
// vigilando mientras está en marcha: guarda texto e imágenes (nunca lo que
// un gestor de contraseñas marca como secreto) y avisa cuando algo cambia.
Singleton {
    id: root

    readonly property bool available: Tools.has("wl-paste") && Tools.has("wl-copy") && Tools.has("python3")
    readonly property string helper: `${Quickshell.shellDir}/bin/veyl-clip`
    readonly property string home: `${Quickshell.env("XDG_STATE_HOME") || Quickshell.env("HOME") + "/.local/state"}/veyl/clipboard`
    // Lo copiado, lo fijado primero y luego de lo más reciente a lo más antiguo.
    property var entries: []
    // Los terminales pegan con Ctrl+Mayús+V.
    readonly property var terminals: /^(kitty|foot(client)?|alacritty|(org\.wezfurlong\.)?wezterm|(com\.mitchellh\.)?ghostty|(org\.kde\.)?konsole|gnome-terminal.*|org\.gnome\.(console|ptyxis)|ptyxis|xterm|urxvt|st|tilix|terminator|contour|rio)$/i

    function file(id: string): string {
        return `file://${home}/data/${id}`;
    }

    // Hace cuánto, visto desde `now` (segundos desde 1970).
    function ago(time: real, now: real): string {
        const seconds = Math.max(0, now - time);
        if (seconds < 60)
            return "ahora";
        if (seconds < 3600)
            return `hace ${Math.floor(seconds / 60)} min`;
        if (seconds < 86400)
            return `hace ${Math.floor(seconds / 3600)} h`;
        const days = Math.floor(seconds / 86400);
        return days === 1 ? "ayer" : `hace ${days} días`;
    }

    // Un color copiado («#rgb», «#rgba», «#rrggbb», «#rrggbbaa»), como lo
    // entiende Qt, que pone la transparencia delante.
    function tint(text: string): string {
        let hex = text.slice(1);
        if (hex.length < 6)
            hex = hex.replace(/./g, "$&$&");
        return `#${hex.slice(6)}${hex.slice(0, 6)}`;
    }

    // Qué es lo copiado, que decide qué se puede hacer con ello: image,
    // color, link, email (una dirección), path (una ruta), json, math (una
    // cuenta) o text. veyl-clip solo distingue los cuatro primeros.
    function type(entry: var): string {
        if (entry.kind !== "text")
            return entry.kind;
        const bare = entry.text.trim();
        if (bare.includes("\n"))
            return json(entry) !== undefined ? "json" : "text";
        if (/^[^\s@<>()]+@[^\s@<>()]+\.[a-z]{2,}$/i.test(bare))
            return "email";
        if (/^(file:\/\/|~\/|\/)[^\0]{0,4095}$/.test(bare) && !/^\/\//.test(bare))
            return "path";
        // Una cuenta, pero no una fecha («17/11/2026») ni un teléfono.
        if (/^[\d\s.,+\-*\/×÷^()%]+$/.test(bare) && /\d\s*[-+*\/×÷^%]\s*[\d(]/.test(bare) && !/^\d{1,4}([\/.-])\d{1,2}\1\d{1,4}$/.test(bare) && !(/^\+?[\d\s-]{9,}$/.test(bare) && !/\s-\s/.test(bare)))
            return "math";
        return json(entry) !== undefined ? "json" : "text";
    }

    // Si el texto está entero en el índice (veyl-clip guarda las 1200
    // primeras letras), el texto; si no, null.
    function whole(entry: var): var {
        return entry.kind !== "image" && (entry.chars ?? Infinity) <= 1200 ? entry.text : null;
    }

    // Lo copiado, si es un objeto o una lista JSON entera.
    function json(entry: var): var {
        const text = whole(entry)?.trim() ?? "";
        if (!/^[\[{]/.test(text))
            return undefined;
        try {
            return JSON.parse(text);
        } catch (error) {
            return undefined;
        }
    }

    // Un color copiado como «rgb(…)» o «hsl(…)», como lo escribe CSS.
    function rgb(text: string): string {
        const [r, g, b, a] = channels(text);
        return a < 1 ? `rgb(${r} ${g} ${b} / ${round(a)})` : `rgb(${r}, ${g}, ${b})`;
    }

    function hsl(text: string): string {
        const [r, g, b, a] = channels(text).map((value, i) => i < 3 ? value / 255 : value);
        const high = Math.max(r, g, b);
        const low = Math.min(r, g, b);
        const light = (high + low) / 2;
        const chroma = high - low;
        let hue = 0;
        if (chroma > 0)
            hue = high === r ? (g - b) / chroma % 6 : high === g ? (b - r) / chroma + 2 : (r - g) / chroma + 4;
        const saturation = chroma === 0 ? 0 : chroma / (1 - Math.abs(2 * light - 1));
        const parts = `${Math.round((hue * 60 + 360) % 360)} ${Math.round(saturation * 100)}% ${Math.round(light * 100)}%`;
        return a < 1 ? `hsl(${parts} / ${round(a)})` : `hsl(${parts.replace(/ /g, ", ")})`;
    }

    function channels(text: string): var {
        let hex = text.trim().slice(1);
        if (hex.length < 6)
            hex = hex.replace(/./g, "$&$&");
        const values = [0, 2, 4].map(at => parseInt(hex.slice(at, at + 2), 16));
        return values.concat([hex.length > 6 ? parseInt(hex.slice(6, 8), 16) / 255 : 1]);
    }

    function round(value: real): string {
        return String(Math.round(value * 100) / 100);
    }

    // Un enlace sin lo que solo sirve para seguirte (utm_…, fbclid…); el
    // mismo si no lleva nada de eso.
    function clean(url: string): string {
        const at = url.indexOf("?");
        if (at < 0)
            return url;
        const hash = url.indexOf("#", at);
        const query = url.slice(at + 1, hash < 0 ? undefined : hash);
        const kept = query.split("&").filter(part => part !== "" && !/^(utm_[a-z_]+|fbclid|gclid|dclid|msclkid|mc_eid|mc_cid|igshid|igsh|si|ref_src|_hsenc|_hsmi|yclid|twclid)(=|$)/i.test(part));
        return url.slice(0, at) + (kept.length > 0 ? `?${kept.join("&")}` : "") + (hash < 0 ? "" : url.slice(hash));
    }

    // Una ruta copiada, entera: sin «file://» y con «~» ya en casa.
    function place(text: string): string {
        const bare = text.trim();
        if (bare.startsWith("file://"))
            return decodeURI(bare.slice(7));
        return bare.startsWith("~/") ? Quickshell.env("HOME") + bare.slice(1) : bare;
    }

    function size(bytes: real): string {
        if (bytes < 1024)
            return `${bytes} B`;
        if (bytes < 1024 * 1024)
            return `${(bytes / 1024).toLocaleString(Qt.locale(), "f", 1)} KiB`;
        return `${(bytes / 1024 / 1024).toLocaleString(Qt.locale(), "f", 1)} MiB`;
    }

    function run(order: string, id: string): void {
        job.createObject(root, { command: [helper].concat(id ? [order, id] : [order]) });
    }

    function copy(id: string): void {
        run("copy", id);
    }

    // Lo copia y, en cuanto está en el portapapeles, lo pega en `window`
    // ({ address, kind }): la ventana que tenía el foco al abrir la entrada.
    function paste(id: string, window: var): void {
        job.createObject(root, { command: [helper, "copy", id], window });
    }

    // Pone `text` en el portapapeles y, si se da `window`, lo pega en ella.
    function put(text: string, window: var): void {
        job.createObject(root, { command: ["sh", "-c", 'wl-copy -- "$1" >/dev/null 2>&1', "sh", text], window });
    }

    function shortcut(window: var): void {
        const mods = terminals.test(window.kind) ? "CTRL SHIFT" : "CTRL";
        Compositor.run(`hl.dispatch(hl.dsp.send_shortcut({ mods = "${mods}", key = "V", window = "address:${window.address}" }))`);
    }

    // La extensión que corresponde a una imagen copiada.
    function extension(entry: var): string {
        return ({ "image/jpeg": "jpg" })[entry.mime] ?? entry.mime.split("/").pop();
    }

    // Una imagen copiada, en la carpeta de imágenes, con la fecha en el nombre.
    function save(entry: var): void {
        const name = `Portapapeles ${Qt.formatDateTime(new Date(entry.time * 1000), "yyyy-MM-dd HH.mm.ss")}.${extension(entry)}`;
        Quickshell.execDetached(["sh", "-c", 'dir=$(xdg-user-dir PICTURES 2>/dev/null); [ -n "$dir" ] && [ "$dir" != "$HOME" ] || dir="$HOME/Pictures"; mkdir -p "$dir" && cp -- "$1" "$dir/$2"', "sh", `${home}/data/${entry.id}`, name]);
    }

    // Y con el visor: una copia con su extensión, para que se sepa qué es.
    function open(entry: var): void {
        const name = `portapapeles-${entry.id}.${extension(entry)}`;
        Quickshell.execDetached(["sh", "-c", 'dir="${XDG_RUNTIME_DIR:-/tmp}/veyl"; mkdir -p "$dir" && cp -- "$1" "$dir/$2" && exec xdg-open "$dir/$2"', "sh", `${home}/data/${entry.id}`, name]);
    }

    Component {
        id: job

        Process {
            id: process

            property var window: null

            running: true
            stderr: StdioCollector {
                onStreamFinished: if (text.trim() !== "") console.warn(text.trim())
            }
            onExited: code => {
                index.reload();
                if (code === 0 && window)
                    typing.createObject(root, { window });
                destroy();
            }
        }
    }

    // Un respiro antes de pegar: la entrada tiene que haber soltado el teclado.
    Component {
        id: typing

        Timer {
            required property var window

            interval: 90
            running: true
            onTriggered: {
                root.shortcut(window);
                destroy();
            }
        }
    }

    Process {
        running: root.available
        command: [root.helper, "watch"]
        stdout: SplitParser {
            onRead: index.reload()
        }
    }

    FileView {
        id: index
        path: `${root.home}/index.json`
        printErrors: false
        onLoaded: {
            let entries = [];
            try {
                entries = JSON.parse(text());
            } catch (error) {}
            // Lo fijado delante; dentro de cada grupo sigue mandando lo
            // reciente (sort, aquí, no respeta el orden de los empates).
            if (!Array.isArray(entries))
                entries = [];
            entries = entries.filter(entry => entry && /^[0-9a-f]{20}$/.test(entry.id) && (entry.kind === "image" || typeof entry.text === "string"));
            root.entries = entries.filter(entry => entry.pinned).concat(entries.filter(entry => !entry.pinned));
        }
        onLoadFailed: root.entries = []
    }
}
