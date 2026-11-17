pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io
import qs

// La agenda: los eventos que Veyl conoce. Los suyos se apuntan desde el
// almanaque, por IPC (ver bin/veyl) o en agenda.json, que es donde se
// recuerdan entre arranques. Los de los calendarios de fuera (los de
// ~/.config/veyl/calendars.json) los trae veyld a events.json; los de los
// calendarios de la cuenta de Google en los que se puede escribir también se
// borran, y lo nuevo se apunta en uno de ellos si así se elige (calendarTarget).
Singleton {
    id: root

    // Los de Veyl: { id, title, at (ms desde epoch), allDay }
    property var mine: []
    // Los de fuera: lo mismo, con id negativo, `calendar`, `place`, `until`
    // (ms del final, o 0) y, si es de Google, `ref` (su id allí); `pending`,
    // mientras Google no contesta a lo recién apuntado.
    property var outside: []
    // Los calendarios de fuera: { name, color, on, count, synced, error, kind }
    property var calendars: []
    property double synced: 0
    // Los que hay apuntados en ~/.config/veyl/calendars.json: { name, url, color?, on? }
    property var sources: []
    property string pending  // lo que falta por escribir en él
    // La cuenta de Google: { account, client, calendars: [{ id, summary, color, writable, primary }], error, login }
    property var google: ({})
    // Los calendarios en los que se puede apuntar (sus nombres) y en cuál se apunta.
    readonly property var writable: calendars.filter(calendar => calendar.writable && calendar.on !== false).map(calendar => calendar.name)
    readonly property string target: writable.includes(Settings.calendarTarget) ? Settings.calendarTarget : ""
    // Los calendarios ocultos en los ajustes: sus eventos no se ven, aunque
    // veyld aún no haya vuelto a sincronizar.
    readonly property var hidden: sources.map((source, at) => source.on === false ? source.name || calendars[at]?.name : null).filter(name => name)
    // Todos los que se ven, del más antiguo al más lejano
    readonly property var events: mine.concat(outside.filter(event => !hidden.includes(event.calendar))).sort((a, b) => a.at - b.at || a.id - b.id)
    readonly property int lead: Math.max(0, Settings.eventWarning)  // minutos antes desde los que uno es inminente; 0 = nunca
    property int linger: 2 // minutos que aguanta una vez empezado

    readonly property double now: Clock.now.getTime()
    // El próximo con hora: el que puede ser inminente
    readonly property var next: events.find(event => !event.allDay && event.at > now - linger * 60000) ?? null
    readonly property int minutes: next ? Math.ceil((next.at - now) / 60000) : 0
    readonly property bool imminent: lead > 0 && !!next && minutes <= lead
    // En la isla, a qué hora y qué: «16:30 Dentista · 10 min»
    readonly property string label: next ? `${clock(next.at)} ${brief(next.title, 28)} · ${minutes > 0 ? minutes + " min" : "ahora"}` : ""
    // El aviso: el evento que acaba de volverse inminente. Mientras dura, la
    // isla de la hora se abre y lo anuncia; luego queda en la fila.
    property var herald: null
    property alias heralded: kept.heralded  // el último anunciado: ni una recarga ni una sincronización lo repiten
    onImminentChanged: announce()
    onNextChanged: announce()
    // Qué hay cada día (día → eventos; ver Calendar)
    readonly property var days: {
        const days = {};
        for (const event of events) {
            const day = Calendar.at(event.at);
            if (!days[day])
                days[day] = [];
            days[day].push(event);
        }
        return days;
    }
    // Lo que queda por delante. Solo cambia cuando cambia lo que contiene,
    // no cada minuto.
    property var ahead: []
    // Lo último que se escribió en el archivo: releerlo no es una novedad.
    property string written

    onEventsChanged: sift(true)
    onNowChanged: sift(false)

    function announce(): void {
        if (!settle.done)
            return;
        if (!imminent) {
            herald = null;
            return;
        }
        const key = `${next.title}@${next.at}`;
        if (key === heralded) {
            // El mismo, quizá traído de nuevo: que el anuncio lleve el de ahora.
            if (herald)
                herald = next;
            return;
        }
        heralded = key;
        herald = next;
        dismiss.restart();
    }

    // Un título que no estire la isla sin fin, sin partir emojis por la mitad.
    function brief(text: string, most: int): string {
        const marks = [];
        for (let at = 0; at < text.length && marks.length <= most;) {
            marks.push(at);
            at += text.codePointAt(at) > 0xffff ? 2 : 1;
        }
        return marks.length > most ? text.slice(0, marks[most - 1]).replace(/\s+$/, "") + "…" : text;
    }

    function sift(anyway: bool): void {
        const left = events.filter(event => event.allDay ? Calendar.at(event.at) >= Calendar.today : event.at > now - linger * 60000);
        if (anyway || left.length !== ahead.length || left.some((event, i) => event.id !== ahead[i].id))
            ahead = left;
    }

    // El color de un calendario de fuera: el suyo o, sin él, uno del esquema
    // según su sitio en la lista.
    function tone(name: string): color {
        const at = calendars.findIndex(calendar => calendar.name === name);
        const own = calendars[at]?.color ?? "";
        if (/^#([0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i.test(own))
            return own;
        const hues = [Tokens.google, Tokens.github, Tokens.done, Tokens.anthropic, Tokens.openai, Tokens.focus];
        return hues[Math.max(0, at) % hues.length];
    }

    // Cambia la lista de calendarios de fuera; veyld lo nota y los trae.
    function keep(list: var): void {
        sources = list;
        // La carpeta puede no existir aún: se crea antes (solo para ti) y
        // se escribe al acabar.
        pending = JSON.stringify(list, null, 2) + "\n";
        folder.running = false;
        folder.running = true;
    }

    function follow(name: string, url: string, tint: string): string {
        url = url.trim();
        name = name.trim();
        if (!/^(https?|webcals?|file):\/\/\S+$|^[~\/]\S*$/.test(url))
            return "Una dirección http(s):// o webcal://, o la ruta de un .ics o una carpeta";
        if (sources.some(source => source.url === url))
            return "Ese calendario ya está";
        if (name && sources.some(source => source.name === name))
            return `Ya hay uno que se llama ${name}`;
        const source = name ? { name, url } : { url };
        if (tint)
            source.color = tint;
        keep(sources.concat([source]));
        return "";
    }
    // El color de un evento: el de su calendario o, si es de Veyl, el acento.
    function hue(event: var): color {
        return event?.calendar ? tone(event.calendar) : Tokens.accent;
    }

    // Apunta un evento donde toque (en Veyl o en el calendario elegido). En
    // Veyl devuelve su número; en Google, 0 (llega con la próxima sincronización).
    function put(title: string, at: double, allDay: bool): int {
        if (!target || !Daemon.connected)
            return here(title, at, allDay);
        title = title.trim();
        if (!title || !Number.isFinite(at) || !Number.isFinite(new Date(at).getTime()))
            return -1;
        const day = stamp(Calendar.at(at));
        const time = allDay ? "" : clock(at);
        Daemon.send({ calendar: { add: { calendar: target, title, day, time } } });
        // Se ve ya, mientras Google contesta.
        outside = outside.concat([{ id: -(outside.length + 1), title, at, allDay, calendar: target, place: "", until: 0, pending: true }]);
        return 0;
    }

    // Apunta un evento en Veyl y devuelve su número; si ya estaba, el que tenía.
    function here(title: string, at: double, allDay: bool): int {
        title = title.trim();
        if (!title || !Number.isFinite(at) || !Number.isFinite(new Date(at).getTime()))
            return -1;
        const same = mine.find(event => event.title === title && event.at === at && event.allDay === allDay);
        if (same)
            return same.id;
        let id = mine.reduce((top, event) => Math.max(top, event.id), 0) + 1;
        if (id > 2147483647) {
            const used = new Set(mine.map(event => event.id));
            id = 1;
            while (used.has(id))
                id++;
        }
        mine = mine.concat([{ id, title, at, allDay }]).sort((a, b) => a.at - b.at || a.id - b.id);
        save();
        return id;
    }

    // Un día concreto (AAAA-MM-DD, hoy o mañana); sin hora, todo el día. Lo
    // usan el IPC y veyld («apunta dentista mañana a las cinco»). Devuelve el
    // error, o nada.
    function add(title: string, day: string, time: string): string {
        if (!title.trim())
            return "el evento necesita un título";
        const date = parseDay(day);
        const minutes = parseTime(time);
        if (date < 0)
            return `día no reconocido: ${day} (AAAA-MM-DD, hoy o mañana)`;
        if (time !== "" && minutes < 0)
            return `hora no reconocida: ${time} (HH:MM)`;
        put(title, Calendar.moment(date, Math.max(0, minutes)), minutes < 0);
        return "";
    }

    // Si un evento se puede borrar desde aquí: los de Veyl y los de los
    // calendarios de Google en los que se puede escribir.
    function removable(event: var): bool {
        return event.id > 0 || (!!event.ref && writable.includes(event.calendar));
    }

    function drop(id: int): bool {
        if (id < 0) {
            const event = outside.find(other => other.id === id);
            if (!event || !removable(event))
                return false;
            Daemon.send({ calendar: { remove: { calendar: event.calendar, id: event.ref } } });
            outside = outside.filter(other => other.ref !== event.ref || other.calendar !== event.calendar);
            return true;
        }
        const rest = mine.filter(event => event.id !== id);
        if (rest.length === mine.length)
            return false;
        mine = rest;
        save();
        return true;
    }

    // «AAAA-MM-DD», «hoy» o «mañana» → día; -1 si no se entiende
    function parseDay(text: string): int {
        if (text === "hoy" || text === "mañana")
            return Calendar.today + (text === "hoy" ? 0 : 1);
        const date = text.match(/^(\d{4})-(\d{1,2})-(\d{1,2})$/);
        if (!date || date[2] < 1 || date[2] > 12 || date[3] < 1 || date[3] > 31)
            return -1;
        const parsed = new Date(0);
        parsed.setUTCFullYear(Number(date[1]), Number(date[2]) - 1, Number(date[3]));
        if (parsed.getUTCFullYear() !== Number(date[1]) || parsed.getUTCMonth() !== Number(date[2]) - 1 || parsed.getUTCDate() !== Number(date[3]))
            return -1;
        return Math.round(parsed.getTime() / 86400000);
    }

    // «HH:MM» → minutos desde medianoche; -1 si no se entiende
    function parseTime(text: string): int {
        const clock = text.match(/^(\d{1,2})[:.](\d{2})$/);
        if (!clock || clock[1] > 23 || clock[2] > 59)
            return -1;
        return clock[1] * 60 + Number(clock[2]);
    }

    function stamp(day: int): string {
        const date = Calendar.parts(day);
        return `${date.year}-${String(date.month + 1).padStart(2, "0")}-${String(date.date).padStart(2, "0")}`;
    }

    function clock(at: double): string {
        return Qt.formatTime(new Date(at), "HH:mm");
    }

    // En el archivo, cada evento lleva su día y, si la tiene, su hora, como
    // los escribiría una persona: { "title": …, "day": "2026-11-20", "time": "16:30" }
    function read(text: string): void {
        if (text === written)
            return;
        let entries = [];
        try {
            entries = JSON.parse(text.trim() || "[]");
        } catch (error) {
            console.warn("agenda.json no es JSON válido; se conserva la agenda");
            return;
        }
        if (!Array.isArray(entries)) {
            console.warn("agenda.json no es una lista de eventos");
            return;
        }
        const validId = id => Number.isInteger(id) && id > 0 && id <= 2147483647;
        const used = new Set(entries.filter(entry => validId(entry?.id)).map(entry => entry.id));
        const seen = new Set();
        let nextId = 1;
        const events = [];
        for (const entry of entries) {
            const day = parseDay(String(entry?.day ?? ""));
            const timed = entry?.time !== undefined && entry?.time !== "";
            const time = timed ? parseTime(String(entry.time)) : -1;
            if (day < 0 || typeof entry?.title !== "string" || !entry.title.trim() || (timed && time < 0))
                continue;
            let id = entry.id;
            if (!validId(id) || seen.has(id)) {
                while (used.has(nextId))
                    nextId++;
                id = nextId++;
                used.add(id);
            }
            seen.add(id);
            events.push({ id, title: entry.title.trim(), at: Calendar.moment(day, Math.max(0, time)), allDay: !timed });
        }
        written = text;
        root.mine = events.sort((a, b) => a.at - b.at || a.id - b.id);
    }

    // Lo que trajo veyld: { synced, calendars: […], events: [{ calendar, title, day, time?, end?, where? }] }
    function take(text: string): void {
        let found;
        try {
            found = JSON.parse(text.trim() || "{}");
        } catch (error) {
            return;  // a medio escribir: ya llegará entero
        }
        calendars = Array.isArray(found?.calendars) ? found.calendars : [];
        synced = Number(found?.synced) || 0;
        const events = [];
        for (const entry of Array.isArray(found?.events) ? found.events : []) {
            const day = parseDay(String(entry?.day ?? ""));
            const timed = typeof entry?.time === "string" && entry.time !== "";
            const time = timed ? parseTime(entry.time) : -1;
            if (day < 0 || typeof entry?.title !== "string" || (timed && time < 0))
                continue;
            const at = Calendar.moment(day, Math.max(0, time));
            // El final: «HH:MM» del mismo día o «AAAA-MM-DD HH:MM».
            const end = String(entry.end ?? "").match(/^(?:(\d{4}-\d{2}-\d{2}) )?(\d{1,2}:\d{2})$/);
            const endDay = end ? (end[1] ? parseDay(end[1]) : day) : -1;
            const until = end && endDay >= 0 && parseTime(end[2]) >= 0 ? Calendar.moment(endDay, parseTime(end[2])) : 0;
            const calendar = String(entry.calendar ?? "");
            events.push({ id: -(events.length + 1), title: entry.title, at, allDay: !timed, calendar, place: String(entry.where ?? ""), until, ref: String(entry.id ?? "") });
        }
        outside = events;
    }

    // Lo que cuenta veyld de los calendarios.
    function told(news: var): void {
        if (news.google)
            google = news.google;
        // No se pudo apuntar en Google: se queda en Veyl (veyld ya avisó).
        if (news.failed) {
            const failed = news.failed;
            outside = outside.filter(event => !(event.pending && event.title === failed.title));
            const day = parseDay(String(failed.day));
            const time = parseTime(String(failed.time ?? ""));
            if (day >= 0)
                here(failed.title, Calendar.moment(day, Math.max(0, time)), time < 0);
        }
    }

    function save(): void {
        written = JSON.stringify(mine.map(event => {
            const entry = { id: event.id, title: event.title, day: stamp(Calendar.at(event.at)) };
            if (!event.allDay)
                entry.time = clock(event.at);
            return entry;
        }), null, 2) + "\n";
        file.setText(written);
    }

    Timer {
        id: dismiss
        interval: 9000
        onTriggered: root.herald = null
    }

    // Recién cargado, lo anunciado antes de la recarga aún no ha vuelto.
    Timer {
        id: settle

        property bool done: false

        running: true
        interval: 1500
        onTriggered: {
            done = true;
            root.announce();
        }
    }

    PersistentProperties {
        id: kept
        reloadableId: "agenda-herald"

        property string heralded
    }

    FileView {
        id: file
        path: `${Quickshell.env("XDG_STATE_HOME") || Quickshell.env("HOME") + "/.local/state"}/veyl/agenda.json`
        blockLoading: true
        printErrors: false
        watchChanges: true
        onFileChanged: reload()
        onLoaded: root.read(text())
    }

    Process {
        id: folder
        // Sin tocar el archivo si ya está: tocarlo lo haría releer y
        // pisaría lo que se va a escribir.
        command: ["sh", "-c", 'umask 077 && mkdir -p "$1" && { [ -e "$1/calendars.json" ] || : >"$1/calendars.json"; }', "sh", sourceFile.path.replace(/\/[^\/]*$/, "")]
        onExited: (code, status) => {
            if (code !== 0)
                console.warn(`no se pudo crear ${sourceFile.path}`);
            sourceFile.setText(root.pending);
        }
    }

    FileView {
        id: sourceFile
        path: `${Quickshell.env("XDG_CONFIG_HOME") || Quickshell.env("HOME") + "/.config"}/veyl/calendars.json`
        blockLoading: true
        printErrors: false
        watchChanges: true
        onFileChanged: if (!root.pending) reload()
        onSaved: {
            root.pending = "";
            // Que veyld lo note ya: al mostrar uno, sus eventos vuelven enseguida.
            Daemon.send({ calendars: "sync" });
        }
        onSaveFailed: error => {
            console.warn(`no se pudo guardar ${path} (${error})`);
            root.pending = "";
        }
        onLoaded: {
            // Mientras se escribe la lista nueva, lo que hay en el archivo es la vieja.
            if (root.pending)
                return;
            try {
                const list = JSON.parse(text().trim() || "[]");
                root.sources = Array.isArray(list) ? list.filter(source => typeof source?.url === "string") : [];
            } catch (error) {
                console.warn("calendars.json no es JSON válido");
            }
        }
        onLoadFailed: root.sources = []
    }

    FileView {
        path: `${Quickshell.env("XDG_STATE_HOME") || Quickshell.env("HOME") + "/.local/state"}/veyl/events.json`
        blockLoading: true
        printErrors: false
        watchChanges: true
        onFileChanged: reload()
        onLoaded: root.take(text())
        onLoadFailed: root.take("")
    }

    IpcHandler {
        target: "agenda"

        // Hoy: `when` es HH:MM (mañana si ya pasó) o +N minutos.
        function set(title: string, when: string): string {
            const now = Date.now();
            const time = root.parseTime(when);
            const offset = when.match(/^\+(\d+)$/);
            let at;
            if (time >= 0) {
                at = Calendar.moment(Calendar.today, time);
                if (at < now)
                    at = Calendar.moment(Calendar.today + 1, time);
            } else if (offset) {
                at = (Math.floor(now / 60000) + Number(offset[1])) * 60000;
            } else {
                return `hora no reconocida: ${when} (HH:MM o +minutos)`;
            }
            return root.put(title, at, false) < 0 ? "el evento necesita un título y una fecha válida" : "";
        }

        // Un día concreto (AAAA-MM-DD, hoy o mañana); sin hora, todo el día.
        function add(title: string, day: string, time: string): string {
            return root.add(title, day, time);
        }

        function remove(id: int): string {
            return root.drop(id) ? "" : `no hay ningún evento ${id} (veyl event list los enseña)`;
        }

        // Lo que queda por delante, uno por línea: número, día, hora y título.
        function list(): string {
            return root.ahead.map(event => [event.id, root.stamp(Calendar.at(event.at)), event.allDay ? "--:--" : root.clock(event.at), event.title].join("\t")).join("\n");
        }

        // Solo los de Veyl.
        function clear(): void {
            if (root.mine.length === 0)
                return;
            root.mine = [];
            root.save();
        }

        // Apunta un calendario de fuera por su enlace (o ruta); el nombre, si se quiere.
        function follow(url: string, name: string): string {
            return root.follow(name, url, "");
        }

        // Pide a veyld que vuelva a traer los calendarios de fuera.
        function sync(): void {
            Daemon.send({ calendars: "sync" });
        }

        // Los calendarios de fuera, uno por línea: nombre, eventos y si falló, por qué.
        function calendars(): string {
            return root.calendars.map(calendar => [calendar.name, calendar.on === false ? "apagado" : calendar.count ?? 0, calendar.error ?? ""].join("\t")).join("\n");
        }
    }
}
