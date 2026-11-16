pragma Singleton

import QtQuick
import Quickshell
import qs

// Notificaciones. El servidor (org.freedesktop.Notifications) es veyld: las
// recibe, las guarda, decide cuáles salen como aviso y cuándo caducan, y le
// cuenta a cada programa lo que haces con las suyas. Aquí está lo que él
// cuenta (`Daemon` lo trae) y lo que hace falta para pintarlo: el velo apila
// los avisos, la más reciente arriba, y el cajón enseña todas las guardadas.
Singleton {
    id: root

    // Todas las guardadas, de la más reciente a la más antigua, tal como
    // las manda veyld (ver `Note` en veyld/src/notes.rs).
    property var notes: []
    // Las tarjetas del velo, de la más reciente a la más antigua (`key` es el
    // id de la notificación). Una que se va se queda un momento, `gone`, para
    // plegarse antes de dejar su sitio.
    readonly property ListModel deck: ListModel {}
    // Las del cajón, en el mismo orden que `notes` (`key` es el id): cambia
    // solo lo que cambia, para que sus tarjetas no se rehagan.
    readonly property ListModel shelf: ListModel {}
    // Con el puntero encima, o escribiendo una respuesta, los avisos no caducan.
    property bool held: false
    // No molestar: veyld no saca como aviso ni hace sonar lo que llega,
    // salvo lo crítico; todo se guarda igual.
    readonly property bool quiet: Settings.quiet  // se cambia en Settings: se recuerda
    // Los avisos, de la más reciente a la más antigua; a la vista, apilados,
    // solo los primeros: los demás esperan detrás.
    readonly property var shown: notes.filter(note => note.popup)
    property int stack: 3
    readonly property var front: shown.slice(0, stack).map(note => note.id)
    readonly property int waiting: Math.max(0, shown.length - stack)
    readonly property bool loud: shown.length > 0
    // Las últimas, para el centro de control.
    readonly property var recent: notes.slice(0, 3).map(note => read(note))

    onHeldChanged: Daemon.send({ hold: held })
    onLoudChanged: if (!loud) held = false

    function find(id: int): var {
        return notes.find(note => note.id === id) ?? null;
    }

    // Lo que manda veyld: todas al saludar; luego, una que llega o cambia, o
    // una que se va.
    function load(list: var): void {
        notes = list;
        shelf.clear();
        for (const note of list)
            shelf.append({ key: note.id });
        for (let i = deck.count - 1; i >= 0; i--) {
            const card = deck.get(i);
            if (!card.gone && !find(card.key)?.popup)
                retire(card.key);
        }
        for (const note of list.slice().reverse())
            if (note.popup)
                deal(note.id);
    }

    function update(note: var): void {
        notes = [note].concat(notes.filter(other => other.id !== note.id));
        const at = place(note.id);
        if (at < 0)
            shelf.insert(0, { key: note.id });
        else if (at > 0)
            shelf.move(at, 0, 1);
        if (note.popup)
            deal(note.id);
        else
            retire(note.id);
    }

    function drop(id: int): void {
        notes = notes.filter(note => note.id !== id);
        const at = place(id);
        if (at >= 0)
            shelf.remove(at);
        retire(id);
    }

    function place(key: int): int {
        for (let i = 0; i < shelf.count; i++)
            if (shelf.get(i).key === key)
                return i;
        return -1;
    }

    // Su tarjeta entra en el velo, arriba, si no está ya.
    function deal(key: int): void {
        for (let i = 0; i < deck.count; i++)
            if (deck.get(i).key === key && !deck.get(i).gone)
                return;
        deck.insert(0, { key, gone: false });
    }

    function retire(key: int): void {
        for (let i = 0; i < deck.count; i++) {
            if (deck.get(i).key === key && !deck.get(i).gone) {
                deck.setProperty(i, "gone", true);
                sweep.createObject(root, { key });
                return;
            }
        }
    }

    // Lo que haces con ellas: veyld se lo cuenta a su programa.
    function act(id: int, key: string): void {
        Daemon.send({ note: id, act: key });
    }

    function reply(id: int, text: string): void {
        Daemon.send({ note: id, reply: text });
    }

    function dismiss(id: int): void {
        Daemon.send({ note: id, dismiss: true });
    }

    function clear(): void {
        Daemon.send({ notes: "clear" });
    }

    // Pulsar una: su acción por defecto si su programa la escucha; si no, se descarta.
    function revisit(entry: var): void {
        if (entry.live && entry.primary)
            act(entry.id, "default");
        else
            dismiss(entry.id);
    }

    function forget(id: int): void {
        dismiss(id);
    }

    // Dónde está una imagen o un icono: una ruta, una URL o un nombre del
    // tema de iconos. "" si no hay o no existe.
    function locate(source: string): string {
        if (!source)
            return "";
        if (source.includes("://"))
            return source;
        if (source.startsWith("/"))
            return `file://${source}`;
        return Quickshell.iconPath(source, true);
    }

    // Icono propio según la categoría («im.received», «device.added»…).
    function glyph(category: string): string {
        const kinds = { call: "phone", device: "plugs", email: "envelope", im: "chat-circle", network: "wifi-high", presence: "user", transfer: "download-simple" };
        return kinds[category.split(".")[0]] ?? "bell";
    }

    // El cuerpo, listo para pintar: sus imágenes van aparte, para poder
    // ajustarlas a la tarjeta, y sus saltos de línea se conservan.
    function body(raw: string): var {
        const pictures = [];
        const text = raw.replace(/<img\b[^>]*>/gi, tag => {
            const source = tag.match(/\bsrc\s*=\s*["']([^"']*)["']/i);
            const alt = tag.match(/\balt\s*=\s*["']([^"']*)["']/i);
            if (source)
                pictures.push({ source: locate(source[1]), alt: alt?.[1] ?? "" });
            return "";
        }).trim().replace(/\r?\n/g, "<br/>");
        return { text, pictures };
    }

    // Una nota de veyld, lista para pintar: con su nombre y su icono
    // buscados entre las aplicaciones, y su cuerpo preparado.
    function read(note: var): var {
        if (!note)
            return null;
        const entry = note.entry ? DesktopEntries.heuristicLookup(note.entry) : null;
        const content = body(note.body ?? "");
        return {
            id: note.id,
            at: note.at,
            live: note.live,
            app: note.app || (entry?.name ?? ""),
            icon: locate(note.icon) || locate(entry?.icon ?? ""),
            image: locate(note.image),
            glyph: glyph(note.category ?? ""),
            summary: note.summary,
            body: content.text,
            pictures: content.pictures,
            urgency: note.urgency,
            // Avance de 0 a 1, o -1 si no trae
            progress: note.progress >= 0 ? note.progress / 100 : -1,
            // La acción por defecto no es un botón: es pulsar la tarjeta. Sin
            // nadie que la escuche, ninguna acción hace nada.
            primary: note.live && note.primary,
            actions: note.live ? note.actions.map(action => ({
                id: action.id,
                text: action.text,
                // Con iconos de acción, el identificador es el nombre del icono.
                icon: note.action_icons ? Quickshell.iconPath(action.id, true) : ""
            })) : [],
            reply: note.live && note.reply,
            placeholder: note.placeholder
        };
    }

    // Quita de la baraja una que se fue, cuando ya se ha plegado.
    Component {
        id: sweep

        Timer {
            required property int key

            interval: Tokens.arrive + Tokens.fast
            running: true
            onTriggered: {
                for (let i = 0; i < root.deck.count; i++) {
                    if (root.deck.get(i).key === key && root.deck.get(i).gone) {
                        root.deck.remove(i);
                        break;
                    }
                }
                destroy();
            }
        }
    }
}
