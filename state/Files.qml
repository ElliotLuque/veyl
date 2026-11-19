pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io

// Los archivos de la entrada: lo que hay en la carpeta personal, por su
// nombre. Cada búsqueda es un `fd` nuevo (o, sin él, un `find`), que deja
// fuera lo oculto y, con `fd`, lo que ignora git; el anterior, si aún no ha
// contestado, se descarta. Sin nada escrito, lo último que se abrió
// (recently-used.xbel, el de GTK y los portales).
Singleton {
    id: root

    readonly property string home: Quickshell.env("HOME")
    readonly property bool fast: Tools.has("fd")
    property string query: ""     // lo que hay escrito
    property string asked: ""     // la búsqueda de la que son `found`
    property var found: []        // [{ path, folder }]
    property var pending: null
    readonly property bool busy: pending !== null || delay.running
    // Lo último que se abrió, de lo más reciente a lo más antiguo.
    readonly property var recent: parse(recents.text())

    function drop(): void {
        if (pending)
            pending.stale = true;
        pending = null;
    }

    onQueryChanged: refresh()

    function refresh(): void {
        drop();
        delay.stop();
        if (query.trim() === "") {
            asked = "";
            found = [];
            return;
        }
        delay.restart();
    }

    // Las palabras van por separado: con `fd`, el nombre ha de llevarlas
    // todas (con «=», para que una que empiece por «-» no sea una opción);
    // con `find`, la primera, y las demás se miran luego.
    function command(text: string): var {
        const words = text.split(/\s+/).filter(word => word);
        if (fast)
            return ["fd", "--fixed-strings", "--ignore-case", "--absolute-path", "--type", "f", "--type", "d", "--max-results", "400"].concat(words.map(word => `--and=${word}`), ["--", "", home]);
        const pattern = `*${words[0].replace(/[\[\]*?\\]/g, "\\$&")}*`;
        return ["sh", "-c", 'find "$1" -mindepth 1 -name ".*" -prune -o -iname "$2" -print 2>/dev/null | head -n 400; :', "sh", home, pattern];
    }

    function entry(line: string): var {
        const folder = line.endsWith("/");
        return { path: folder ? line.slice(0, -1) : line, folder };
    }

    // La ruta para leerla: la carpeta personal, «~».
    function pretty(path: string): string {
        return path === home ? "~" : path.startsWith(`${home}/`) ? `~${path.slice(home.length)}` : path;
    }

    function url(path: string): string {
        return `file://${encodeURI(path).replace(/#/g, "%23").replace(/\?/g, "%3F")}`;
    }

    function parse(xml: string): var {
        const seen = {};
        const list = [];
        // Sin matchAll: el JS de QML no lo tiene.
        const bookmark = /<bookmark\s[^>]*href="file:\/\/([^"]+)"[^>]*modified="([^"]+)"/g;
        let match;
        while ((match = bookmark.exec(xml)) !== null) {
            let path;
            try {
                path = decodeURIComponent(match[1].replace(/&amp;/g, "&").replace(/&apos;/g, "'").replace(/&quot;/g, "\""));
            } catch (error) {
                continue;
            }
            if (seen[path])
                continue;
            seen[path] = true;
            list.push({ path, folder: false, time: Date.parse(match[2]) || 0 });
        }
        return list.sort((a, b) => b.time - a.time).slice(0, 30);
    }

    // No lanzar un proceso por cada tecla de una ráfaga de escritura.
    Timer {
        id: delay
        interval: 120
        onTriggered: root.pending = searching.createObject(root, { text: root.query.trim() })
    }

    FileView {
        id: recents
        path: `${Quickshell.env("XDG_DATA_HOME") || root.home + "/.local/share"}/recently-used.xbel`
        watchChanges: true
        printErrors: false
        onFileChanged: reload()
    }

    Component {
        id: searching

        Process {
            id: process

            required property string text
            property bool stale: false
            property int code: -1
            property bool read: false

            // Cuando ha salido y se ha leído lo que dijo, en el orden que sea.
            function settle(): void {
                if (code < 0 || !read)
                    return;
                if (!stale) {
                    root.asked = text;
                    root.found = output.text.split("\n").filter(line => line).map(root.entry);
                    root.pending = null;
                }
                destroy();
            }

            running: true
            command: root.command(text)
            stdout: StdioCollector {
                id: output
                onStreamFinished: {
                    process.read = true;
                    process.settle();
                }
            }
            onStaleChanged: if (stale) running = false
            onExited: exitCode => {
                code = Math.abs(exitCode);
                settle();
            }
        }
    }
}
