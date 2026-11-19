pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Hyprland
import Quickshell.Io
import qs

// El diario, en su vista (base/Diary.qml): la isla que nace del centro del
// borde de arriba para ver un día entero de lo que hiciste, buscar en los 90
// días y preguntarle al cerebro por lo que no recuerdas con sus palabras.
// Aquí, el día que se mira, lo que se busca, lo que contesta veyld y cómo se
// abre; los datos salen de veyld (`diary`, `find` y `say` con `ask`).
Singleton {
    id: root

    property bool open: false
    property string monitor: ""
    // Con qué tamaño nace: el del centro de control si viene de él.
    property size origin: Qt.size(0, 0)

    // El día que se mira, como número de Calendar, y lo que veyld cuenta de él
    // (orders::diary_json): el informe, `blocks`, `labels`, `days`.
    property int day: Calendar.today
    property var report: null
    property bool loading: false
    property string failed: ""

    // Lo que se escribe en el campo: se busca tal cual en el diario mientras
    // se escribe; con Intro se le pregunta al cerebro.
    property string query: ""
    property var hits: []
    property string searched: ""
    // La pregunta al cerebro: { said, ok, say, more, card }; `pending`
    // mientras piensa.
    property var answer: null
    property bool pending: false

    property int asking: -1
    property int loadingId: -1
    property int findingId: -1

    readonly property bool today: day === Calendar.today
    readonly property bool searching: query.trim() !== ""

    // «2026-11-19» de un día de Calendar.
    function iso(day: int): string {
        const p = Calendar.parts(day);
        return `${p.year}-${String(p.month + 1).padStart(2, "0")}-${String(p.date).padStart(2, "0")}`;
    }

    function fromIso(text: string): int {
        const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(text ?? "");
        return m ? Math.round(Date.UTC(Number(m[1]), Number(m[2]) - 1, Number(m[3])) / 86400000) : Calendar.today;
    }

    // Cómo se le dice el día al cerebro: «hoy», «ayer», «el 12 de noviembre».
    function spoken(day: int): string {
        const away = Calendar.today - day;
        return away === 0 ? "hoy" : away === 1 ? "ayer" : `el ${Calendar.format(day, "d 'de' MMMM")}`;
    }

    function show(monitor: string, day: string, query: string): void {
        if (Launcher.open)
            Launcher.hide();
        if (Tuning.open)
            Tuning.hide();
        Glance.release();
        root.monitor = monitor || Hyprland.focusedMonitor?.name || "";
        const next = day ? fromIso(day) : open ? root.day : Calendar.today;
        root.day = Math.min(next, Calendar.today);
        if (query !== undefined && query !== "")
            root.query = query;
        open = true;
        load();
    }

    function hide(): void {
        open = false;
        origin = Qt.size(0, 0);
    }

    function toggle(): void {
        if (open)
            hide();
        else
            show("", "", "");
    }

    // Un día antes (-1) o después (1); no se pasa de hoy.
    function step(way: int): void {
        go(day + way);
    }

    function go(next: int): void {
        next = Math.min(Calendar.today, Math.max(Calendar.today - 89, next));
        if (next === day)
            return;
        day = next;
        load();
    }

    function load(): void {
        if (!Daemon.connected) {
            report = null;
            return;
        }
        loading = true;
        loadingId = Daemon.request({ diary: iso(day), days: 21 });
    }

    function find(): void {
        const text = query.trim();
        if (text === "" || !Daemon.connected) {
            hits = [];
            searched = "";
            return;
        }
        findingId = Daemon.request({ find: text, json: true });
    }

    // Al cerebro, con el día que se mira: él decide si busca por el índice,
    // por palabras o en otros días.
    function ask(): void {
        const text = query.trim();
        if (text === "" || !Daemon.connected || pending)
            return;
        answer = { said: text, ok: true, say: "", more: "", card: null };
        pending = true;
        asking = Daemon.request({ say: text, source: "diary", ask: Daemon.thinks, day: spoken(day) });
    }

    function forget(): void {
        answer = null;
        pending = false;
        asking = -1;
    }

    // Lo que contesta veyld a lo que pidió el diario.
    function told(message: var): void {
        if (message.id === loadingId) {
            loading = false;
            failed = message.ok ? "" : (message.say ?? "");
            report = message.ok ? message.diary : null;
        } else if (message.id === findingId) {
            searched = message.find ?? "";
            hits = message.ok ? message.hits : [];
        } else if (message.id === asking) {
            pending = false;
            answer = {
                said: answer?.said ?? "",
                ok: message.ok !== false,
                say: message.say ?? "",
                more: message.answer ?? "",
                card: message.answer_card ?? null
            };
        }
    }

    onQueryChanged: {
        if (!searching) {
            hits = [];
            searched = "";
        }
        debounce.restart();
    }

    Timer {
        id: debounce
        interval: 220
        onTriggered: root.find()
    }

    // Lo de hoy cambia mientras se mira: se vuelve a pedir cada minuto.
    Timer {
        interval: 60000
        repeat: true
        running: root.open && root.today
        onTriggered: root.load()
    }

    Connections {
        target: Daemon

        function onConnectedChanged(): void {
            if (Daemon.connected && root.open)
                root.load();
        }
    }

    GlobalShortcut {
        appid: "veyl"
        name: "diary"
        description: "Abrir o cerrar el diario"
        onPressed: root.toggle()
    }

    IpcHandler {
        target: "diary"

        function toggle(): void {
            root.toggle();
        }

        function close(): void {
            root.hide();
        }

        // Abre el diario en un día («2026-11-19»; vacío, hoy) y, si se pasa,
        // con algo ya escrito en el campo.
        function open(day: string, query: string): void {
            root.show("", day, query);
        }

        // Le pregunta al cerebro desde el diario, como Intro en el campo.
        function ask(question: string): void {
            root.show("", "", question);
            root.ask();
        }

        function get(): bool {
            return root.open;
        }
    }
}
