pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Hyprland
import Quickshell.Io
import qs

// Agentes en marcha. Los encuentra veyld (mira los procesos y reconoce
// Claude Code, Codex, Gemini, opencode…, se lancen desde donde se lancen) y,
// además, cualquiera puede avisar de su estado por IPC (ver bin/veyl). Un
// agente cuya ventana pide atención (campana, `urgent`) te necesita hasta que
// vas a ella.
Singleton {
    id: root

    readonly property var phases: ["working", "needs", "idle"]
    // Filas: { agent, phase, window, provider, label, name, detail, model,
    // tool, since, focus, ask, source }. `window` es la dirección de la ventana de Hyprland
    // donde vive el agente («0x…»), o "". `ask`: lo que te pregunta si te
    // necesita ({ title, detail, question, options: [{ key, label }] }, en
    // JSON), o "". `source`: detected o told.
    readonly property ListModel list: ListModel {}
    property int needing: 0
    // Lo mismo que `list`, en un array: lo que pueden seguir los enlaces.
    property var rows: []
    // La última foto de veyld.
    property var found: []
    // Ventanas que han pedido atención y aún no has visitado.
    property var calling: []
    readonly property string active: Hyprland.activeToplevel ? `0x${Hyprland.activeToplevel.address}` : ""
    onActiveChanged: if (calling.includes(active)) {
        calling = calling.filter(w => w !== active);
        sync();
    }

    function find(agent: string): int {
        for (let i = 0; i < list.count; i++)
            if (list.get(i).agent === agent)
                return i;
        return -1;
    }

    function row(fields: var): var {
        return Object.assign({ agent: "", phase: "idle", window: "", provider: "", label: "", name: "", cwd: "", detail: "", model: "", tool: "", since: Date.now(), focus: "[]", ask: "", source: "told" }, fields);
    }

    function put(fields: var): void {
        const i = find(fields.agent);
        if (i < 0) {
            list.append(row(fields));
            return;
        }
        const old = list.get(i);
        // Solo lo que cambia: cada cambio despierta a quien lo mira.
        const changed = {};
        for (const key in fields)
            if (old[key] !== fields[key])
                changed[key] = fields[key];
        if (Object.keys(changed).length > 0)
            list.set(i, changed);
    }

    function set(agent: string, phase: string, window: string): string {
        if (!phases.includes(phase))
            return `fase desconocida: ${phase} (${phases.join(", ")})`;
        const i = find(agent);
        const fields = { agent, phase, source: "told" };
        if (window)
            fields.window = window;
        if (i < 0 || list.get(i).phase !== phase)
            fields.since = Date.now();
        if (i < 0)
            Object.assign(fields, { label: agent, name: agent });
        put(fields);
        refresh();
        return "";
    }

    function remove(agent: string): void {
        const i = find(agent);
        if (i < 0)
            return;
        // Se marca y se barre después, para que la cinta pueda irse animada.
        list.setProperty(i, "phase", "gone");
        sweep.restart();
        refresh();
    }

    // La ventana de un proceso detectado: la del primero de sus ancestros que
    // Hyprland conoce.
    function windowOf(chain: var): string {
        const byPid = {};
        for (const toplevel of Hyprland.toplevels.values) {
            const pid = toplevel.lastIpcObject?.pid;
            if (pid && !(pid in byPid))
                byPid[pid] = toplevel.address;
        }
        for (const pid of chain)
            if (pid in byPid)
                return `0x${byPid[pid]}`;
        return "";
    }

    // Vuelca la última foto en la lista: entra lo nuevo, se va lo que ya no
    // está y lo demás se actualiza en su sitio.
    function sync(): void {
        const ids = [];
        for (const agent of found) {
            ids.push(agent.id);
            const window = windowOf(agent.chain);
            const called = window !== "" && calling.includes(window) && agent.phase !== "needs";
            put({
                agent: agent.id,
                phase: called ? "needs" : agent.phase,
                window,
                provider: agent.provider,
                label: agent.label,
                name: agent.name,
                cwd: agent.cwd ?? "",
                detail: called ? "te llama" : agent.detail,
                model: agent.model ?? "",
                tool: agent.tool ?? "",
                since: agent.since,
                focus: JSON.stringify(agent.focus ?? []),
                ask: agent.ask ? JSON.stringify(agent.ask) : "",
                source: "detected"
            });
        }
        for (let i = 0; i < list.count; i++) {
            const row = list.get(i);
            if (row.source === "detected" && row.phase !== "gone" && !ids.includes(row.agent))
                remove(row.agent);
        }
        refresh();
    }

    // Lleva a un agente: su ventana (y su nivel) y, dentro de ella, su
    // pestaña si vive en un multiplexor.
    function focus(agent: string): void {
        const i = find(agent);
        if (i < 0)
            return;
        const row = list.get(i);
        if (row.window)
            Hyprland.dispatch(`hl.dsp.focus({ window = "address:${row.window}" })`);
        for (const command of JSON.parse(row.focus || "[]"))
            Quickshell.execDetached(command);
    }

    // El logotipo de quien hace el agente (logos/<proveedor>.svg), o "" si
    // Veyl no lo trae: entonces va un icono de Phosphor.
    readonly property list<string> logos: ["anthropic", "openai", "google", "opencode", "cursor", "github", "goose", "amp", "qwen", "factory"]

    function logo(provider: string): string {
        return logos.includes(provider) ? Qt.resolvedUrl(`../logos/${provider}.svg`) : "";
    }

    // El color de quien hace el agente, parecido al de su marca (del
    // esquema); el de los que no tienen uno, el acento.
    function vendor(provider: string): color {
        return ({ anthropic: Tokens.anthropic, openai: Tokens.openai, google: Tokens.google, github: Tokens.github })[provider] ?? Tokens.accent;
    }

    // Contesta a lo que pregunta un agente con una de sus opciones: veyld
    // manda su tecla a la terminal del agente.
    function answer(agent: string, key: string): void {
        Daemon.send({ agent_answer: { id: agent, key } });
        answered(agent, key);
    }

    // Se ha contestado a un agente (desde su isla, con sus teclas o con un atajo).
    signal answered(string agent, string key)

    // El agente que pregunta ahora (el último que empezó, si hay varios), con
    // lo que pregunta: { agent, ask }, o null.
    function asking(): var {
        let best = null;
        for (let i = 0; i < list.count; i++) {
            const row = list.get(i);
            if (row.phase !== "needs" || !row.ask)
                continue;
            let ask = null;
            try {
                ask = JSON.parse(row.ask);
            } catch (e) {
                continue;
            }
            if ((ask?.options?.length ?? 0) > 0 && (!best || row.since > best.since))
                best = { agent: row.agent, since: row.since, ask };
        }
        return best;
    }

    // Le contesta con una de sus opciones: por su tecla («1», «y») o por lo
    // que es: accept (la primera), second (la segunda) o deny (la que dice
    // que no, o la última). Devuelve por qué no, si no puede.
    function reply(what: string): string {
        const now = asking();
        if (!now)
            return "ningún agente pregunta nada";
        const options = now.ask.options;
        const no = options.find(o => /^(no|deny|reject|denegar|cancel)\b/i.test(o.label));
        const option = what === "accept" ? options[0] : what === "second" ? options[1] : what === "deny" ? (no ?? options[options.length - 1]) : options.find(o => o.key.toLowerCase() === what.toLowerCase());
        if (!option)
            return `no tiene la opción «${what}» (${options.map(o => o.key).join(", ")})`;
        answer(now.agent, option.key);
        return "";
    }

    // Lleva al agente que pregunta.
    function openAsking(): string {
        const now = asking();
        if (!now)
            return "ningún agente pregunta nada";
        focus(now.agent);
        return "";
    }

    GlobalShortcut {
        appid: "veyl"
        name: "agent-accept"
        description: "Al agente que pregunta: la primera opción (sí)"
        onPressed: root.reply("accept")
    }

    GlobalShortcut {
        appid: "veyl"
        name: "agent-second"
        description: "Al agente que pregunta: la segunda opción"
        onPressed: root.reply("second")
    }

    GlobalShortcut {
        appid: "veyl"
        name: "agent-deny"
        description: "Al agente que pregunta: la que dice que no (o la última)"
        onPressed: root.reply("deny")
    }

    GlobalShortcut {
        appid: "veyl"
        name: "agent-open"
        description: "Ir al agente que pregunta"
        onPressed: root.openAsking()
    }

    // Dónde está: «nixdots · nivel 2» (su carpeta y el nivel de su ventana).
    function place(cwd: string, window: string): string {
        const toplevel = Hyprland.toplevels.values.find(t => `0x${t.address}` === window);
        const level = toplevel?.workspace ? Contexts.label(toplevel.workspace) : "";
        const folder = cwd.replace(/\/+$/, "").split("/").pop();
        return [folder, level === "" ? "" : /^\d+$/.test(level) ? `nivel ${level}` : level].filter(t => t).join(" · ");
    }

    // Cuánto lleva en lo que está: «42 s», «4:05», «1 h 05».
    property real now: Date.now()
    function elapsed(since: real): string {
        const s = Math.max(0, Math.floor((now - since) / 1000));
        const m = Math.floor(s / 60);
        const two = n => (n < 10 ? "0" : "") + n;
        return s < 60 ? `${s} s` : m < 60 ? `${m}:${two(s % 60)}` : `${Math.floor(m / 60)} h ${two(m % 60)}`;
    }

    // Cuatro veces por segundo (a una, el temporizador cae en la frontera del
    // segundo y salta de dos en dos), y solo si alguno trabaja o te necesita.
    Timer {
        interval: 250
        repeat: true
        running: root.rows.some(r => r.phase === "working" || r.phase === "needs")
        onTriggered: root.now = Date.now()
    }

    // Las fases de los agentes que viven en un espacio de trabajo.
    function inside(workspace: HyprlandWorkspace): list<string> {
        if (!workspace)
            return [];
        const here = workspace.toplevels.values.map(t => t.address);
        return rows.filter(r => r.phase !== "gone" && here.includes(r.window.replace(/^0x/, ""))).map(r => r.phase);
    }

    // La fase del agente que vive en una ventana, o "". Si hay varios, la
    // más urgente.
    function at(address: string): string {
        const here = rows.filter(r => r.phase !== "gone" && r.window.replace(/^0x/, "") === address).map(r => r.phase);
        return here.includes("needs") ? "needs" : here.includes("working") ? "working" : here[0] ?? "";
    }

    function refresh(): void {
        let count = 0;
        const windows = [];
        const all = [];
        for (let i = 0; i < list.count; i++) {
            const row = list.get(i);
            all.push({ agent: row.agent, phase: row.phase, window: row.window });
            if (row.phase !== "needs")
                continue;
            count++;
            if (row.window && !windows.includes(row.window))
                windows.push(row.window);
        }
        rows = all;
        needing = count;
        Compositor.needing = windows;
    }

    Timer {
        id: sweep
        interval: Tokens.fast * 2
        onTriggered: {
            for (let i = root.list.count - 1; i >= 0; i--)
                if (root.list.get(i).phase === "gone")
                    root.list.remove(i);
        }
    }

    // Cada vez que cambia algo, veyld manda la foto entera.
    Connections {
        target: Daemon

        function onAgents(list: var): void {
            root.found = list ?? [];
            root.sync();
        }
    }

    // Las ventanas cambian de dueño al abrirse y cerrarse: se vuelve a casar
    // cada agente con la suya.
    Connections {
        target: Hyprland.toplevels

        function onValuesChanged(): void {
            Qt.callLater(root.sync);
        }
    }

    // Una ventana que pide atención (la campana de una terminal) con un agente
    // dentro: ese agente te necesita.
    Connections {
        target: Hyprland

        function onRawEvent(event: HyprlandEvent): void {
            if (event.name !== "urgent")
                return;
            const window = `0x${event.data}`;
            if (window === root.active || root.calling.includes(window))
                return;
            root.calling = root.calling.concat([window]);
            root.sync();
        }
    }

    IpcHandler {
        target: "agents"

        function set(agent: string, phase: string, window: string): string {
            return root.set(agent, phase, window);
        }

        function remove(agent: string): void {
            root.remove(agent);
        }

        function focus(agent: string): void {
            root.focus(agent);
        }

        // Contesta al agente que pregunta: una tecla de sus opciones, o accept, second o deny.
        function answer(what: string): string {
            return root.reply(what);
        }

        // Lleva al agente que pregunta.
        function open(): string {
            return root.openAsking();
        }

        function list(): string {
            const rows = [];
            for (let i = 0; i < root.list.count; i++) {
                const row = root.list.get(i);
                rows.push(`${row.agent}\t${row.phase}\t${row.window}\t${row.name}\t${row.detail}`);
            }
            return rows.join("\n");
        }
    }
}
