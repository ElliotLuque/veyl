pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io

// Nix y NixOS en Veyl:
// 1. Si Veyl está declarado con su módulo (`programs.veyl`, en nix/), aplica lo
//    declarado al reconstruir y compara con los ajustes actuales para guardarlos
//    o deshacerlos en tu configuración.
// 2. En sistemas NixOS o con flake, mantiene el estado en vivo que manda veyld:
//    generación actual, cambios locales en el flake, actualizaciones pendientes
//    de las entradas de `flake.lock`, reconstrucción/actualización en marcha y
//    contraseña de sudo integrada en la isla.
Singleton {
    id: root

    readonly property string mine: `${Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME") + "/.config"}/veyl/nix.json`
    // El de Home Manager gana al de NixOS.
    property var manifest: null
    property var system: null
    readonly property var used: manifest ?? system
    readonly property bool declared: used !== null
    readonly property var settings: used?.settings ?? {}

    // Lo que cuenta `veyld nix status`: { nixos, declared, file, files, error, rebuild }.
    property var status: ({})
    // Lo que manda el hilo de Nix de veyld por el socket en vivo.
    property var live: ({})

    readonly property bool nixos: live.nixos === true || status.nixos === true
    // Interfaz de ajustes declarados (pie de cambios por aplicar): solo con módulo de Nix.
    readonly property bool active: declared
    // Islas, centro de control, órdenes y sección de sistema/flake: en NixOS o con flake/módulo.
    readonly property bool systemActive: declared || nixos || flake !== ""
    readonly property string file: status.file ?? ""
    readonly property var files: status.files ?? (file ? [file] : [])
    readonly property string problem: status.error ?? ""
    readonly property string flake: live.flake || used?.flake || ""
    readonly property string flakeName: live.flakeName || (flake ? flake.split("/").filter(part => part).pop() ?? "" : "")
    readonly property string rebuild: live.rebuild || status.rebuild || used?.rebuild || ""
    readonly property int generation: live.generation ?? 0
    readonly property string version: live.version ?? ""
    readonly property double builtAt: live.builtAt ?? 0
    readonly property bool rebootNeeded: live.rebootNeeded === true
    readonly property int dirty: live.dirty ?? 0
    readonly property double lockModified: live.lockModified ?? 0
    readonly property bool lockNewer: live.lockNewer === true
    readonly property double nixpkgsAge: live.nixpkgsAge ?? 0
    readonly property int inputsCount: live.inputsCount ?? 0
    readonly property bool checking: live.checking === true
    readonly property double checkedAt: live.checkedAt ?? 0
    readonly property var updates: live.updates ?? []
    readonly property string checkError: live.checkError ?? ""
    readonly property var job: live.job ?? null
    readonly property string jobState: job?.state ?? "idle"
    readonly property bool running: jobState === "running"
    readonly property var auth: live.auth ?? null
    readonly property bool authNeeded: auth !== null
    readonly property bool pendingRebuild: lockNewer || unbuilt.length > 0
    readonly property bool needsAttention: authNeeded || jobState === "error"
    property bool busy: false
    property string failure: ""

    // Lo que hay ahora, clave a clave (así se entera de cada cambio).
    readonly property var current: ({
            scheme: Settings.scheme,
            wallpaper: Settings.wallpaper,
            webSearch: Settings.webSearch,
            terminal: Settings.terminal,
            weekNumbers: Settings.weekNumbers,
            quiet: Settings.quiet,
            journal: Settings.journal,
            llm: Settings.llm,
            notificationDays: Settings.notificationDays,
            voice: Settings.voice,
            brain: Settings.brain,
            input: Settings.input,
            monitors: Settings.monitors,
            lockAfter: Settings.lockAfter,
            calendarTarget: Settings.calendarTarget,
            eventWarning: Settings.eventWarning
        })

    readonly property var titles: ({
            scheme: "Esquema de color",
            wallpaper: "Fondo de pantalla",
            webSearch: "Búsqueda web",
            terminal: "Terminal",
            weekNumbers: "Números de semana",
            quiet: "No molestar",
            journal: "Diario",
            llm: "Comando de respaldo",
            notificationDays: "Días que se guardan las notificaciones",
            voice: "Voz",
            brain: "Cerebro",
            input: "Teclado y ratón",
            monitors: "Pantallas",
            lockAfter: "Bloqueo automático",
            calendarTarget: "Calendario de lo nuevo",
            eventWarning: "Aviso antes de un evento"
        })

    // Lo que no coincide con tu configuración, por ajuste: { key, title,
    // changes: [{ path, value, nix }], written } (written: ya guardado en el
    // archivo, falta reconstruir). value null = se quita.
    readonly property var drift: compare(current, settings, memory.written)
    readonly property var unsaved: drift.filter(group => !group.written)
    readonly property var unbuilt: drift.filter(group => group.written)
    onDriftChanged: if (failure && drift.length === 0) failure = ""

    // Hoja a hoja: { '["voice","speed"]': { path, value } }. Lo vacío de dentro
    // de un objeto (null, "") es como si no estuviera: así lo guarda Tuning.nest.
    function leaves(value: var, path: var): var {
        const out = {};
        const walk = (value, path) => {
            if (value !== null && typeof value === "object" && !Array.isArray(value)) {
                for (const key of Object.keys(value))
                    walk(value[key], path.concat(key));
            } else if (value !== undefined && !(path.length > 1 && (value === null || value === ""))) {
                out[JSON.stringify(path)] = { path, value };
            }
        };
        walk(value, path);
        return out;
    }

    // Lo que hay contra lo declarado (o, sin declarar, lo de fábrica).
    function compare(now: var, declared: var, written: var): var {
        const groups = [];
        for (const key of Object.keys(Settings.defaults)) {
            const mine = leaves(now[key], [key]);
            const nix = leaves(declared[key], [key]);
            const factory = leaves(Settings.defaults[key], [key]);
            const changes = [];
            for (const at of new Set(Object.keys(mine).concat(Object.keys(nix)))) {
                const value = mine[at]?.value;
                const wanted = nix[at]?.value;
                const base = wanted !== undefined ? wanted : factory[at]?.value;
                if (!same(value, base))
                    changes.push({ path: (mine[at] ?? nix[at]).path, value: value === undefined ? null : value, nix: wanted });
            }
            if (changes.length > 0)
                groups.push({
                    key,
                    title: titles[key] ?? key,
                    changes,
                    written: changes.every(change => written[JSON.stringify(change.path)] === JSON.stringify(change.value))
                });
        }
        return groups;
    }

    function same(a: var, b: var): bool {
        return JSON.stringify(a) === JSON.stringify(b);
    }

    function plain(value: var): bool {
        return value !== null && typeof value === "object" && !Array.isArray(value);
    }

    // Un valor como se lee en una fila.
    function show(value: var): string {
        if (value === undefined)
            return "sin declarar";
        if (value === null)
            return "lo de fábrica";
        if (typeof value === "boolean")
            return value ? "sí" : "no";
        if (typeof value === "string")
            return value === "" ? "vacío" : value.replace(Quickshell.env("HOME"), "~");
        return JSON.stringify(value);
    }

    // Lo que se ve de un grupo: el cambio, o cuáles si son varios.
    function describe(group: var): string {
        if (group.changes.length === 1) {
            const change = group.changes[0];
            const where = change.path.length > 1 ? `${change.path.slice(1).join(".")}: ` : "";
            return `${where}${show(change.value)} · en Nix: ${show(change.nix)}`;
        }
        return group.changes.map(change => change.path.slice(1).join(".") || change.path[0]).join(", ");
    }

    // Cuánto hace, en corto: «ahora», «hace 12 min», «hace 3 h», «hace 4 d».
    function ageText(at: double, now: double): string {
        if (!at || at <= 0)
            return "";
        const minutes = Math.max(0, Math.floor((now - at) / 60000));
        if (minutes < 1)
            return "ahora";
        if (minutes < 60)
            return `hace ${minutes} min`;
        if (minutes < 1440)
            return `hace ${Math.floor(minutes / 60)} h`;
        return `hace ${Math.floor(minutes / 1440)} d`;
    }

    // Cuánto lleva una operación en marcha: «18 s», «1:04».
    function elapsedText(since: double, now: double): string {
        if (!since || since <= 0)
            return "";
        const seconds = Math.max(0, Math.floor((now - since) / 1000));
        if (seconds < 60)
            return `${seconds} s`;
        const mins = Math.floor(seconds / 60);
        const rem = seconds % 60;
        return `${mins}:${rem < 10 ? "0" : ""}${rem}`;
    }

    // Resumen en una línea para el centro de control y las órdenes.
    function summaryLine(): string {
        if (authNeeded)
            return "Esperando contraseña de sudo";
        if (running)
            return job?.phase || "En marcha…";
        if (jobState === "error")
            return job?.summary || "Falló la última operación";
        if (updates.length > 0) {
            const names = updates.slice(0, 3).map(u => u.name).join(", ");
            const rest = updates.length > 3 ? ` y ${updates.length - 3} más` : "";
            return `${updates.length} ${updates.length === 1 ? "actualización" : "actualizaciones"}: ${names}${rest}`;
        }
        if (pendingRebuild)
            return lockNewer ? "flake.lock actualizado · falta reconstruir" : `${unbuilt.length} ajustes guardados · falta reconstruir`;
        if (rebootNeeded)
            return "Kernel nuevo · reinicia para aplicarlo";
        if (dirty > 0)
            return `${dirty} ${dirty === 1 ? "archivo modificado" : "archivos modificados"} en ${flakeName || "flake"}`;
        if (checking)
            return "Comprobando actualizaciones…";
        return checkedAt > 0 ? "Al día" : (flakeName || "NixOS");
    }

    // Pone una hoja (o la quita, con null: vuelve a lo de fábrica).
    function put(path: var, value: var): void {
        const key = path[0];
        if (!(key in Settings.defaults))
            return;
        if (path.length === 1) {
            Settings[key] = value === null || value === undefined ? Settings.defaults[key] : value;
            return;
        }
        const next = JSON.parse(JSON.stringify(Settings[key] ?? {}));
        const trail = [next];
        let at = next;
        for (const step of path.slice(1, -1)) {
            if (!plain(at[step]))
                at[step] = {};
            at = at[step];
            trail.push(at);
        }
        const last = path[path.length - 1];
        if (value === null || value === undefined)
            delete at[last];
        else
            at[last] = value;
        // Lo que se queda vacío, fuera.
        for (let i = trail.length - 1; i > 0; i--)
            if (Object.keys(trail[i]).length === 0)
                delete trail[i - 1][path[i]];
        Settings[key] = next;
    }

    // Lo declarado manda cada vez que cambia: lo de antes que ya no se
    // declara (y no se ha tocado desde entonces) vuelve a lo de fábrica.
    function apply(): void {
        const text = JSON.stringify(settings);
        if (!declared || text === memory.applied)
            return;
        let before = {};
        try {
            before = memory.applied ? JSON.parse(memory.applied) : {};
        } catch (e) {}
        const now = leaves(settings, []);
        for (const [at, leaf] of Object.entries(leaves(before, [])))
            if (!(at in now) && same(leaves(current, [])[at]?.value, leaf.value))
                put(leaf.path, null);
        for (const leaf of Object.values(now)) {
            if (leaf.path[0] in Settings.defaults)
                put(leaf.path, leaf.value);
            else
                console.warn("Veyl no conoce el ajuste declarado", leaf.path.join("."));
        }
        memory.applied = text;
        memory.written = {};
    }

    function told(data: var): void {
        live = data ?? ({});
    }

    function disconnected(): void {
        if (!live || typeof live !== "object")
            return;
        live = Object.assign({}, live, { auth: null, checking: false, job: live.job?.state === "running" ? null : live.job });
    }

    function veyld(args: var, done: var): void {
        Host.ask(["sh", "-c", 'bin=$(command -v veyld || echo "$1/veyld/target/release/veyld"); shift; exec "$bin" nix "$@"', "sh", Quickshell.shellDir].concat(args), done);
    }

    onDeclaredChanged: {
        if (declared)
            queryStatus();
        else
            status = {};
    }

    function queryStatus(): void {
        veyld(["status"], text => {
            try {
                root.status = JSON.parse(text);
            } catch (e) {
                root.status = {};
            }
        });
    }

    function refresh(): void {
        home.reload();
        etc.reload();
        if (declared)
            queryStatus();
        else
            status = {};
        refreshSystem();
    }

    function refreshSystem(): void {
        if (Daemon.connected)
            Daemon.send({ nix: { op: "refresh" } });
    }

    function checkUpdates(): void {
        if (Daemon.connected)
            Daemon.send({ nix: { op: "check" } });
    }

    function updateFlake(input: string): void {
        if (Daemon.connected)
            Daemon.send({ nix: { op: "update", input: input ?? "" } });
    }

    function rebuildSystem(): void {
        if (Daemon.connected)
            Daemon.send({ nix: { op: "rebuild" } });
        else
            build();
    }

    function upgradeSystem(): void {
        if (Daemon.connected)
            Daemon.send({ nix: { op: "upgrade" } });
    }

    function stopJob(): void {
        if (Daemon.connected)
            Daemon.send({ nix: { op: "stop" } });
    }

    function dismissJob(): void {
        if (Daemon.connected)
            Daemon.send({ nix: { op: "dismiss" } });
    }

    function submitAuth(password: string): void {
        if (Daemon.connected)
            Daemon.send({ nix: { op: "auth", password } });
    }

    function cancelAuth(): void {
        if (Daemon.connected)
            Daemon.send({ nix: { op: "auth_cancel" } });
    }

    // Escribe en tu configuración lo de estos grupos.
    function save(groups: var): void {
        const changes = [].concat(...groups.map(group => group.changes.map(change => ({ path: change.path, value: change.value }))));
        if (changes.length === 0 || busy)
            return;
        busy = true;
        failure = "";
        veyld(["write", JSON.stringify(changes)], (text, code, error) => {
            root.busy = false;
            if (code !== 0) {
                root.failure = error.trim().replace(/^veyld: /, "") || "No se pudo escribir";
                return;
            }
            const written = Object.assign({}, memory.written);
            for (const change of changes)
                written[JSON.stringify(change.path)] = JSON.stringify(change.value);
            memory.written = written;
        });
    }

    // Vuelve a lo que dice tu configuración.
    function revert(group: var): void {
        failure = "";
        for (const change of group.changes)
            put(change.path, change.nix === undefined ? null : change.nix);
    }

    // Deshace todos los cambios aún no guardados en Nix.
    function revertAll(): void {
        failure = "";
        for (const group of unsaved.slice())
            for (const change of group.changes)
                put(change.path, change.nix === undefined ? null : change.nix);
    }

    // Reconstruye en una terminal (útil si quieres seguir toda la salida interactiva).
    function build(): void {
        if (!rebuild)
            return;
        Quickshell.execDetached([Apps.terminal, "-e", "sh", "-c", `${rebuild}; status=$?; echo; printf 'Intro para cerrar'; read -r _; exit $status`]);
    }

    function openFlakeTerminal(): void {
        if (!flake)
            return;
        Quickshell.execDetached(["sh", "-c", 'cd "$1" && exec "$2"', "sh", flake, Apps.terminal]);
    }

    function openLog(): void {
        if (job?.logFile)
            Qt.openUrlExternally(`file://${job.logFile}`);
    }

    // Sin Veyl en tu configuración: cómo se escribiría lo que tienes, al portapapeles.
    function copy(): void {
        const settings = {};
        for (const group of drift)
            for (const change of group.changes) {
                let at = settings;
                for (const step of change.path.slice(0, -1))
                    at = at[step] = plain(at[step]) ? at[step] : {};
                at[change.path[change.path.length - 1]] = change.value;
            }
        veyld(["show", JSON.stringify(settings)], text => {
            if (text)
                Quickshell.execDetached(["sh", "-c", 'printf %s "$1" | wl-copy', "sh", text]);
        });
    }

    Component.onCompleted: refresh()

    // Lo que se lee a la vez que se abren los ajustes, y de vez en cuando:
    // Home Manager cambia el enlace, no el archivo, y eso no se ve.
    Connections {
        target: Tuning

        function onOpenChanged(): void {
            if (Tuning.open)
                root.refresh();
        }
    }

    Timer {
        running: root.active
        interval: 20000
        repeat: true
        onTriggered: {
            home.reload();
            etc.reload();
        }
    }

    FileView {
        id: home
        path: root.mine
        printErrors: false
        watchChanges: true
        onFileChanged: reload()
        onLoaded: {
            try {
                root.manifest = JSON.parse(text());
            } catch (e) {
                root.manifest = null;
            }
            root.apply();
        }
        onLoadFailed: root.manifest = null
    }

    FileView {
        id: etc
        path: "/etc/veyl/nix.json"
        printErrors: false
        watchChanges: true
        onFileChanged: reload()
        onLoaded: {
            try {
                root.system = JSON.parse(text());
            } catch (e) {
                root.system = null;
            }
            root.apply();
        }
        onLoadFailed: root.system = null
    }

    // Lo último declarado que se aplicó, y lo ya guardado en tu
    // configuración que aún no ha llegado (falta reconstruir).
    FileView {
        path: `${Quickshell.env("XDG_STATE_HOME") || Quickshell.env("HOME") + "/.local/state"}/veyl/nix.json`
        blockLoading: true
        printErrors: false
        onAdapterUpdated: writeAdapter()

        JsonAdapter {
            id: memory

            property string applied: ""
            property var written: ({})
        }
    }

    IpcHandler {
        target: "nix"

        function status(): string {
            return JSON.stringify(root.live);
        }

        function check(): string {
            if (!root.flake)
                return "no se encontró ningún flake";
            root.checkUpdates();
            return "";
        }

        function update(input: string): string {
            if (!root.flake)
                return "no se encontró ningún flake";
            root.updateFlake(input);
            return "";
        }

        function rebuild(): string {
            if (!root.rebuild)
                return "no hay orden de reconstrucción configurada";
            root.rebuildSystem();
            return "";
        }

        function upgrade(): string {
            if (!root.flake || !root.rebuild)
                return "falta el flake o la orden de reconstrucción";
            root.upgradeSystem();
            return "";
        }

        function stop(): void {
            root.stopJob();
        }
    }
}
