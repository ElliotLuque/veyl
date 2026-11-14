pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io
import qs

// Todo lo que Veyl le pide a Hyprland: forma y color de las ventanas según
// los tokens, las teclas del vistazo, del modo escuchar, de la entrada y
// del bloqueo, y lo que se ha cambiado en los ajustes (teclado, ratón y
// pantallas), que se vuelve a poner en cada arranque.
Singleton {
    id: root

    // Direcciones de las ventanas cuyo agente te necesita: borde de aviso.
    property list<string> needing: []
    property list<string> marked: []

    property bool started: false

    function rgb(color: color): string {
        return `rgb(${color.toString().slice(1)})`;
    }

    function rgba(color: color): string {
        const hex = channel => Math.round(channel * 255).toString(16).padStart(2, "0");
        return `rgba(${hex(color.r)}${hex(color.g)}${hex(color.b)}${hex(color.a)})`;
    }

    readonly property string setup: `
        hl.config({
            general = {
                gaps_in = ${Tokens.gap / 2},
                gaps_out = ${Tokens.gap},
                border_size = ${Tokens.line},
                ["col.active_border"] = "${rgb(Tokens.accent)}",
                ["col.inactive_border"] = "${rgb(Tokens.border)}",
            },
            decoration = {
                rounding = ${Tokens.radius},
                shadow = { enabled = true, range = ${Tokens.shadowRange}, render_power = 3, color = "${rgba(Tokens.shadow)}" },
                blur = { enabled = true, size = ${Tokens.blur}, passes = 3 },
            },
        })
        -- Super mantenido: el vistazo. No consume la tecla, así que los
        -- atajos con Super siguen funcionando igual.
        for _, bind in ipairs(veyl_binds or {}) do
            bind:remove()
        end
        veyl_binds = {}
        for _, key in ipairs({ "Super_L", "Super_R" }) do
            table.insert(veyl_binds, hl.bind(key, hl.dsp.global("veyl:glance"), { non_consuming = true, ignore_mods = true }))
        end
        -- La entrada y los ajustes aparecen y se van con su isla: sin
        -- animación de Hyprland encima. Las reglas no se quitan, se apagan.
        for _, rule in ipairs(veyl_rules or {}) do
            rule:set_enabled(false)
        end
        veyl_rules = { hl.layer_rule({ match = { namespace = "^veyl-(launcher|dials)$" }, no_anim = true }) }
    `

    // Al cambiar de esquema cambian varios tokens seguidos: se aplica una vez.
    onSetupChanged: if (started) Qt.callLater(apply)

    // Un valor de JS escrito en Lua.
    function literal(value: var): string {
        if (typeof value === "string")
            return JSON.stringify(value);
        if (typeof value === "object" && value)
            return `{ ${Object.entries(value).map(([key, inner]) => `["${key}"] = ${literal(inner)}`).join(", ")} }`;
        return String(value);
    }

    // Teclado y ratón: solo lo que se ha tocado; lo demás, lo que diga la
    // configuración de Hyprland. «touchpad:natural_scroll» va anidado.
    readonly property string input: {
        const input = {};
        for (const [key, value] of Object.entries(Settings.input ?? {})) {
            const [outer, inner] = key.split(":");
            if (inner)
                input[outer] = Object.assign(input[outer] ?? {}, { [inner]: value });
            else
                input[outer] = value;
        }
        return Object.keys(input).length > 0 ? `hl.config({ input = ${literal(input)} })` : "";
    }
    onInputChanged: if (started && input) Qt.callLater(() => run(input))

    function start(): void {
        started = true;
        apply();
        if (input)
            run(input);
        const monitors = Object.entries(Settings.monitors ?? {});
        if (monitors.length > 0)
            run(monitors.map(([output, monitor]) => `hl.monitor(${literal(Object.assign({ output }, monitor))})`).join("\n"));
    }

    function apply(): void {
        run(setup);
    }

    function run(lua: string): void {
        request.createObject(root, { lua });
    }

    function border(window: string, color: string): string {
        return ["active_border_color", "inactive_border_color"].map(prop => `hl.dispatch(hl.dsp.window.set_prop({ window = "address:${window}", prop = "${prop}", value = "${color}" }))`).join("\n");
    }

    onNeedingChanged: {
        const lua = [];
        for (const window of needing.filter(w => !marked.includes(w)))
            lua.push(border(window, rgb(Tokens.attention)));
        for (const window of marked.filter(w => !needing.includes(w)))
            lua.push(border(window, "unset"));
        marked = needing;
        if (lua.length > 0)
            run(lua.join("\n"));
    }

    Component {
        id: request

        Process {
            required property string lua

            running: true
            command: ["hyprctl", "eval", lua]
            stdout: StdioCollector {
                onStreamFinished: if (text.trim() !== "ok") console.warn("Hyprland rechazó:", text.trim())
            }
            onExited: destroy()
        }
    }
}
