pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io
import qs

// Lo que los ajustes le preguntan al sistema y lo que le piden: pantallas,
// teclado y ratón (a Hyprland), la red (a nmcli), el perfil de energía, los
// modelos de voz, los fondos que hay y de qué máquina se trata. Se pregunta
// al abrir cada sección, no antes: nada de esto avisa cuando cambia.
Singleton {
    id: root

    // Pantallas, como las cuenta Hyprland: { name, description, width,
    // height, refresh, scale, x, y, transform, modes: ["3840x2160@60.00", …] }
    property var monitors: []
    // Teclado y ratón tal como están ahora: { kb_layout: "es", … }
    property var options: ({})
    // La red: los dispositivos ({ device, type, state, connection, address }),
    // las conexiones guardadas ({ name, uuid, type, device, active }) y las
    // redes Wi-Fi a la vista ({ ssid, signal, secure, used }).
    property var devices: []
    property var connections: []
    property var networks: []
    property bool scanning: false
    readonly property bool wireless: devices.some(device => device.type === "wifi")
    // Energía: los perfiles, si hay power-profiles-daemon.
    property var profiles: []
    property string profile: ""
    // Voz: los modelos de Whisper descargados ("small-q5_1", …).
    property var models: []
    // Fondos: las imágenes de las carpetas donde suelen estar.
    property var wallpapers: []
    // La máquina: { host, os, kernel, uptime, cpu, memory, gpu, hyprland, quickshell }
    property var info: ({})
    // Un cambio de pantallas a prueba: si no se confirma, se deshace solo.
    property var trial: null
    property int left: 0
    readonly property int patience: 15  // segundos para confirmar
    property string failure: ""  // lo último que salió mal, un momento

    readonly property string modelDir: `${Quickshell.env("XDG_DATA_HOME") || Quickshell.env("HOME") + "/.local/share"}/veyl/models`
    readonly property list<string> inputs: ["kb_layout", "kb_options", "repeat_rate", "repeat_delay", "sensitivity", "natural_scroll", "touchpad:natural_scroll", "touchpad:tap-to-click"]

    // Lo que se pregunta para cada sección.
    function refresh(section: string): void {
        if (section === "screens")
            ask(["hyprctl", "monitors", "-j"], text => monitors = parseMonitors(text));
        else if (section === "input")
            ask(["sh", "-c", `for o; do hyprctl getoption "input:$o" -j; done`, "sh", ...inputs], text => options = parseOptions(text));
        else if (section === "network")
            network();
        else if (section === "session")
            ask(["sh", "-c", "powerprofilesctl get && powerprofilesctl list"], (text, code) => {
                profile = code === 0 ? text.split("\n")[0].trim() : "";
                profiles = code === 0 ? text.split("\n").slice(1).map(line => line.match(/^\s*\*?\s*([\w-]+):\s*$/)?.[1]).filter(name => name) : [];
            });
        else if (section === "voice")
            ask(["sh", "-c", `for f in "$0"/ggml-*.bin; do [ -f "$f" ] && basename "$f" .bin; done`, modelDir], text => {
                models = text.split("\n").map(line => line.trim().replace(/^ggml-/, "")).filter(name => name && !name.startsWith("silero"));
            });
        else if (section === "look")
            pictures();
        else if (section === "about")
            machine();
    }

    function network(): void {
        const env = { LC_ALL: "C" };
        ask(["sh", "-c", `
            nmcli -t -f DEVICE,TYPE,STATE,CONNECTION device || exit 1
            echo --
            nmcli -t -f NAME,UUID,TYPE,DEVICE,ACTIVE connection show
            echo --
            for d in $(nmcli -t -f DEVICE,STATE device | awk -F: '$2 == "connected" { print $1 }'); do
                printf '%s:' "$d"; nmcli -g IP4.ADDRESS device show "$d" | head -n1
            done`], (text, code) => {
            if (code !== 0)
                return;
            const [devices, connections, addresses] = text.split("\n--\n").map(block => block.split("\n").filter(line => line).map(fields));
            const address = {};
            for (const [device, ip] of addresses ?? [])
                address[device] = (ip ?? "").split("/")[0];
            root.devices = devices.filter(([, type]) => !["loopback", "bridge", "tun", "wifi-p2p"].includes(type)).map(([device, type, state, connection]) => ({ device, type, state, connection, address: address[device] ?? "" }));
            root.connections = (connections ?? []).filter(([, , type]) => !["loopback", "bridge", "tun"].includes(type)).map(([name, uuid, type, device, active]) => ({ name, uuid, type, device, active: active === "yes" }));
            if (wireless)
                scan(false);
        }, env);
    }

    function scan(again: bool): void {
        scanning = true;
        ask(["nmcli", "-t", "-f", "IN-USE,SSID,SIGNAL,SECURITY", "device", "wifi", "list", "--rescan", again ? "yes" : "auto"], text => {
            scanning = false;
            const seen = {};
            for (const [used, ssid, signal, security] of text.split("\n").filter(line => line).map(fields)) {
                if (!ssid)
                    continue;
                const strength = parseInt(signal) || 0;
                if (!seen[ssid] || seen[ssid].signal < strength || used === "*")
                    seen[ssid] = { ssid, signal: strength, secure: !!security && security !== "--", used: used === "*" || !!seen[ssid]?.used };
            }
            networks = Object.values(seen).sort((a, b) => b.used - a.used || b.signal - a.signal);
        }, { LC_ALL: "C" });
    }

    // Conectarse a una red Wi-Fi; una nueva protegida, con su contraseña.
    function join(ssid: string, password: string, done: var): void {
        const command = ["nmcli", "device", "wifi", "connect", ssid];
        if (password)
            command.push("password", password);
        ask(command, (text, code, error) => {
            if (code !== 0)
                fail(error.trim().split("\n").pop() || "No se pudo conectar");
            done?.(code === 0);
            network();
        }, { LC_ALL: "C" });
    }

    function connection(uuid: string, up: bool): void {
        ask(["nmcli", "connection", up ? "up" : "down", "uuid", uuid], (text, code, error) => {
            if (code !== 0)
                fail(error.trim().split("\n").pop() || "No se pudo");
            network();
        }, { LC_ALL: "C" });
    }

    function wifi(on: bool): void {
        ask(["nmcli", "radio", "wifi", on ? "on" : "off"], () => {
            Commands.refresh();
            network();
        });
    }

    function setProfile(name: string): void {
        profile = name;
        ask(["powerprofilesctl", "set", name], (text, code) => {
            if (code !== 0)
                fail("No se pudo cambiar el perfil");
            refresh("session");
        });
    }

    function pictures(): void {
        const current = Settings.wallpaper;
        // Junto al fondo de ahora (dos carpetas más arriba) y donde suelen estar.
        const near = current ? current.split("/").slice(0, -3).join("/") : "";
        const home = Quickshell.env("HOME");
        ask(["sh", "-c", `
            for dir; do shift; [ -d "$dir" ] && set -- "$@" "$dir"; done
            [ $# -gt 0 ] || exit 0
            find -L "$@" -maxdepth 4 -type f \\( -iname '*.png' -o -iname '*.jpg' -o -iname '*.jpeg' -o -iname '*.webp' \\) 2>/dev/null |
                awk '!seen[$0]++' | head -n 120`, "sh", near, `${home}/Pictures/Wallpapers`, `${home}/Pictures/wallpapers`, `${home}/Imágenes/Fondos`, `${home}/Imágenes/fondos`, `${home}/wallpapers`, `${Quickshell.env("XDG_CONFIG_HOME") || home + "/.config"}/veyl/wallpapers`], text => {
            const list = text.split("\n").filter(line => line);
            if (current && !list.includes(current))
                list.unshift(current);
            wallpapers = list;
        });
    }

    function machine(): void {
        ask(["sh", "-c", `
            . /etc/os-release 2>/dev/null
            echo "host=$(cat /proc/sys/kernel/hostname)"
            echo "os=\${PRETTY_NAME:-Linux}"
            echo "kernel=$(uname -r)"
            echo "uptime=$(cut -d' ' -f1 /proc/uptime)"
            echo "cpu=$(awk -F': ' '/^model name/ { print $2; exit }' /proc/cpuinfo)"
            echo "cores=$(nproc)"
            echo "memory=$(awk '/^MemTotal/ { print $2 }' /proc/meminfo)"
            echo "available=$(awk '/^MemAvailable/ { print $2 }' /proc/meminfo)"
            command -v nvidia-smi >/dev/null && echo "gpu=$(nvidia-smi --query-gpu=name --format=csv,noheader 2>/dev/null | head -n1)"
            echo "hyprland=$(hyprctl version -j 2>/dev/null | jq -r .tag 2>/dev/null)"
            echo "quickshell=$(qs --version 2>/dev/null | head -n1 | cut -d, -f1)"
            df -Pk / | awk 'NR == 2 { print "disk=" $2; print "free=" $4 }'`], text => {
            const info = {};
            for (const line of text.split("\n")) {
                const at = line.indexOf("=");
                if (at > 0)
                    info[line.slice(0, at)] = line.slice(at + 1).trim();
            }
            root.info = info;
        });
    }

    // Una línea de `nmcli -t`: campos separados por «:», con «\:» dentro.
    function fields(line: string): var {
        const out = [""];
        for (let i = 0; i < line.length; i++) {
            if (line[i] === "\\" && i + 1 < line.length)
                out[out.length - 1] += line[++i];
            else if (line[i] === ":")
                out.push("");
            else
                out[out.length - 1] += line[i];
        }
        return out;
    }

    function parseMonitors(text: string): var {
        let list;
        try {
            list = JSON.parse(text);
        } catch (error) {
            return [];
        }
        return list.filter(monitor => !monitor.disabled || monitor.width > 0).map(monitor => {
            const mode = `${monitor.width}x${monitor.height}@${monitor.refreshRate.toFixed(2)}`;
            const modes = [...new Set((monitor.availableModes ?? []).map(mode => mode.replace(/Hz$/, "")))];
            return {
                name: monitor.name,
                description: [monitor.make, monitor.model].filter(part => part).join(" ") || monitor.description,
                width: monitor.width,
                height: monitor.height,
                refresh: monitor.refreshRate,
                scale: monitor.scale,
                x: monitor.x,
                y: monitor.y,
                transform: monitor.transform,
                mode,
                // Sin lista (una pantalla virtual), al menos la de ahora.
                modes: modes.length > 0 ? modes : [mode]
            };
        }).sort((a, b) => a.x - b.x || a.y - b.y);
    }

    function parseOptions(text: string): var {
        const options = {};
        for (const line of text.split("\n")) {
            let option;
            try {
                option = JSON.parse(line);
            } catch (error) {
                continue;
            }
            const key = option.option.replace(/^input:/, "");
            const value = option.str ?? option.int ?? option.float ?? option.bool;
            options[key] = value === "[[EMPTY]]" ? "" : value;
        }
        return options;
    }

    // Pantallas · cambia una (modo y escala) y recoloca las demás a su lado
    // si están en fila, para que no se pisen ni queden huecos. Es una
    // prueba: si en `patience` segundos no se confirma, vuelve a como estaba.
    function arrange(name: string, change: var): void {
        const before = monitors.map(monitor => Object.assign({}, monitor));
        const after = monitors.map(monitor => monitor.name === name ? Object.assign({}, monitor, change) : Object.assign({}, monitor));
        place(after);
        apply(after);
        trial = { before, kept: Settings.monitors };
        left = patience;
        countdown.restart();
        const kept = {};
        for (const monitor of after)
            kept[monitor.name] = { mode: monitor.mode, scale: monitor.scale, position: `${monitor.x}x${monitor.y}` };
        Settings.monitors = kept;
        reread.restart();
    }

    function keep(): void {
        countdown.stop();
        trial = null;
    }

    function revert(): void {
        countdown.stop();
        if (!trial)
            return;
        apply(trial.before);
        Settings.monitors = trial.kept;
        trial = null;
        reread.restart();
    }

    // En fila (todas a la misma altura), una tras otra de izquierda a derecha.
    function place(list: var): void {
        if (!list.every(monitor => monitor.y === list[0].y))
            return;
        let x = list[0]?.x ?? 0;
        for (const monitor of list) {
            monitor.x = x;
            const [width, height] = monitor.mode.split("@")[0].split("x").map(Number);
            x += Math.round((monitor.transform % 2 ? height : width) / monitor.scale);
        }
    }

    function apply(list: var): void {
        Compositor.run(list.map(monitor => `hl.monitor({ output = "${monitor.name}", mode = "${monitor.mode}", position = "${monitor.x}x${monitor.y}", scale = ${monitor.scale} })`).join("\n"));
    }

    function fail(text: string): void {
        failure = text;
        forget.restart();
    }

    function ask(command: var, done: var, environment: var): void {
        probe.createObject(root, { command, done, environment: environment ?? {} });
    }

    Timer {
        id: countdown
        interval: 1000
        repeat: true
        onTriggered: {
            root.left--;
            if (root.left <= 0)
                root.revert();
        }
    }

    // Hyprland tarda un momento en cambiar de modo: luego se vuelve a mirar.
    Timer {
        id: reread
        interval: 800
        onTriggered: root.refresh("screens")
    }

    Timer {
        id: forget
        interval: 5000
        onTriggered: root.failure = ""
    }

    Component {
        id: probe

        // Lo que dice y cómo acaba llegan por separado: se contesta con las tres cosas.
        Process {
            id: process

            property var done
            property int code: -1
            property int pending: 3

            function settle(): void {
                if (--pending > 0)
                    return;
                done?.(out.text, code, err.text);
                destroy();
            }

            running: true
            stdout: StdioCollector {
                id: out
                onStreamFinished: process.settle()
            }
            stderr: StdioCollector {
                id: err
                onStreamFinished: process.settle()
            }
            onExited: status => {
                process.code = status;
                process.settle();
            }
        }
    }
}
