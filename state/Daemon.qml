pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io
import qs

// veyld, el daemon de Veyl (veyld/, en Rust): lleva el diario de lo que
// miras, encuentra los agentes, entiende las órdenes y es el servidor de
// notificaciones. Veyl habla con él por
// su socket, en líneas JSON: le cuenta lo que sabe (si la pantalla está
// bloqueada, el temporizador) y él le manda los agentes y lo que hay que
// hacer en Veyl. Si no está en marcha, Veyl lo arranca; vive aparte, así que
// una recarga del shell no lo para.
Singleton {
    id: root

    // Uno por sesión de Hyprland; VEYLD_SOCKET lo fija a mano.
    readonly property string path: {
        const signature = Quickshell.env("HYPRLAND_INSTANCE_SIGNATURE");
        return Quickshell.env("VEYLD_SOCKET") || `${Quickshell.env("XDG_RUNTIME_DIR")}/veyl/${signature ? `veyld-${signature}.sock` : "veyld.sock"}`;
    }
    property Socket link: null
    readonly property bool connected: link?.connected ?? false

    // Lo que entiende la gramática de lo escrito en la entrada (`query`):
    // { said, intent, title, glyph, confirm, ready }, o null.
    property string query: ""
    property var parsed: null
    // Lo que el cerebro está pensando ahora: { said, since }, o null; y la
    // tarjeta que ha compuesto como respuesta (veyld/src/card.rs), o null.
    property var thinking: null
    property var card: null
    // Las vistas guardadas (veyld/src/views.rs): { name, members, apps, open, used };
    // `open` son las ventanas de su espacio, o null si no está abierta.
    property var views: []
    // Lo que le has pedido a Veyl, de lo más reciente a lo más antiguo
    // (veyld/src/journal.rs, `asked`): { id, time, source, said, ok, say,
    // more, card, intent, handler }. `journal` dice si el diario lo guarda.
    property var asked: []
    property bool journal: true
    // Lo que queda de cuota en las suscripciones de los agentes
    // (veyld/src/usage.rs): { provider, name, at, windows: [{ label, used, resets }] }.
    property var usage: []
    property int serial: 0
    // Si hay a quién preguntarle lo que la gramática no entiende (veyld, `thinks`).
    readonly property bool thinks: connected && (!!Settings.brain?.provider || Settings.llm !== "")

    // Cuándo se intentó arrancarlo por última vez, y los intentos de hablarle desde entonces.
    property double started: 0
    property int tries: 0

    signal agents(var list)

    // Lo que Veyl sabe y veyld necesita: si estás (bloqueada no lo estás), lo
    // que contesta él solo («¿cuánto queda?») y, para el diario, si el micro,
    // la cámara o la pantalla compartida están en uso.
    readonly property var state: ({
            locked: Lock.locked,
            timer: Countdown.until,
            quiet: Settings.quiet,
            scheme: Settings.scheme,
            mic: Privacy.mic,
            camera: Privacy.camera,
            screen: Privacy.screen
        })
    onStateChanged: send({ state })

    onQueryChanged: ask()

    function send(message: var): void {
        if (!connected)
            return;
        link.write(`${JSON.stringify(message)}\n`);
        link.flush();
    }

    // Qué entiende la gramática de lo escrito; contesta cuando puede.
    function ask(): void {
        const text = query.trim();
        if (text === "" || !connected) {
            parsed = null;
            return;
        }
        serial++;
        send({ id: serial, parse: text });
    }

    // El historial, entero otra vez (veyld manda luego cada consulta nueva).
    function history(): void {
        serial++;
        send({ id: serial, asked: 200 });
    }

    function quota(): void {
        serial++;
        send({ id: serial, usage: true });
    }

    function forget(id: int): void {
        send({ forget_asked: id });
    }

    // Algo que espera respuesta: devuelve con qué número vendrá.
    function request(message: var): int {
        serial++;
        send(Object.assign({ id: serial }, message));
        return serial;
    }

    // Lo dice como una orden. Desde la entrada ya se ha confirmado lo peligroso.
    function say(text: string, source: string, confirmed: bool): void {
        send({ say: text, source, confirmed });
    }

    // Se lo pregunta al cerebro tal cual, sin pasar por la gramática.
    function think(text: string, source: string): void {
        send({ say: text, source, ask: true });
    }

    // Algo con una vista, por su nombre tal cual (sin pasar por la gramática):
    // create (con `level`, solo lo de este nivel), open, again, undo, save o forget.
    function view(op: string, name: string, level: bool): void {
        send({ view: op, name, level });
    }

    function receive(line: string): void {
        let message;
        try {
            message = JSON.parse(line);
        } catch (e) {
            return;
        }
        if (message.agents !== undefined && message.id === undefined)
            agents(message.agents);
        if (message.notes !== undefined && message.id === undefined)
            Notifications.load(message.notes);
        if (message.note !== undefined)
            Notifications.update(message.note);
        if (message.note_gone !== undefined)
            Notifications.drop(message.note_gone);
        if (message.call)
            call(message.call, message.args ?? []);
        if (message.views !== undefined && message.id === undefined)
            views = message.views;
        if (message.asked !== undefined) {
            asked = message.asked;
            journal = message.journal !== false;
        }
        if (message.usage !== undefined)
            usage = message.usage;
        if (message.asked_one !== undefined)
            asked = [message.asked_one].concat(asked.filter(row => row.id !== message.asked_one.id));
        if (message.asked_gone !== undefined)
            asked = asked.filter(row => row.id !== message.asked_gone);
        if (message.calendar !== undefined)
            Agenda.told(message.calendar);
        if (message.thinking !== undefined) {
            thinking = message.thinking;
            // Una pregunta nueva recoge la tarjeta que hubiera: la isla vuelve
            // a «Pensando» y luego se convierte en la respuesta nueva.
            if (thinking)
                card = null;
        }
        if (message.card !== undefined)
            card = message.card;
        if (message.voice)
            Listen.hear(message.voice);
        // Lo que pidió la vista del diario.
        if (message.id !== undefined && (message.id === Recall.loadingId || message.id === Recall.findingId || message.id === Recall.asking))
            Recall.told(message);
        // Solo la respuesta a lo último que se preguntó.
        if (message.parse !== undefined && message.id === serial)
            parsed = message.parse && message.said === query.trim() ? Object.assign({ said: message.said }, message.parse) : null;
    }

    // Lo que veyld le pide a Veyl.
    function call(name: string, args: var): void {
        switch (name) {
        case "lock":
            Lock.lock();
            break;
        case "timer.start":
            Countdown.start(Number(args[0]) || 1);
            break;
        case "timer.stop":
            Countdown.stop();
            break;
        case "quiet.set":
            Settings.quiet = !!args[0];
            break;
        case "scheme.set":
            Settings.scheme = String(args[0]);
            break;
        case "listen.set":
            Listen.active = !!args[0];
            break;
        case "diary.close":
            Recall.hide();
            break;
        case "diary.open":
            Recall.show("", String(args[0] ?? ""), String(args[1] ?? ""));
            break;
        case "launcher.open":
            Launcher.show(String(args[0] ?? ""));
            break;
        case "agenda.add":
            Agenda.add(String(args[0]), String(args[1]), String(args[2] ?? ""));
            break;
        case "agent.focus":
            Agents.focus(String(args[0]));
            break;
        case "reload":
            Quickshell.reload(false);
            break;
        default:
            console.warn("veyld pide algo que Veyl no sabe hacer:", name);
        }
    }

    function connect(): void {
        link?.destroy();
        link = channel.createObject(root);
    }

    // Lo arranca: el de $PATH o, si no hay, el que se compila en veyld/.
    function start(): void {
        started = Date.now();
        tries = 0;
        const log = `${Quickshell.env("XDG_STATE_HOME") || Quickshell.env("HOME") + "/.local/state"}/veyl`;
        Quickshell.execDetached(["sh", "-c", 'bin=$(command -v veyld || echo "$1/veyld/target/release/veyld"); [ -x "$bin" ] || exit 0; mkdir -p "$2"; exec "$bin" >>"$2/veyld.log" 2>&1', "sh", Quickshell.shellDir, log]);
    }

    onConnectedChanged: {
        if (connected) {
            tries = 0;
            send({ hello: "shell", root: Quickshell.shellDir });
            send({ state });
            send({ hold: Notifications.held });
            history();
            if (Listen.active)
                send({ listen: true, demo: Listen.demo, hold: Listen.hold });
            ask();
        } else {
            Listen.level = 0;
            thinking = null;
            parsed = null;
            agents([]);
            asked = [];
            Notifications.load([]);
        }
    }

    // Un socket que se cae no vuelve: se hace otro en cada intento.
    Component {
        id: channel

        Socket {
            path: root.path
            connected: true
            parser: SplitParser {
                onRead: line => root.receive(line)
            }
        }
    }

    // Sin daemon: se arranca (como mucho una vez por minuto) y se le llama
    // a menudo justo después y cada medio minuto si sigue sin contestar.
    Timer {
        running: !root.connected
        repeat: true
        triggeredOnStart: true
        interval: root.tries < 8 ? 1000 : 30000
        onTriggered: {
            if (Date.now() - root.started > 60000 && root.tries > 0)
                root.start();
            root.tries++;
            root.connect();
        }
    }
}
