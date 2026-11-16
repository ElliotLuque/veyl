pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Hyprland
import Quickshell.Io
import qs

// La entrada: lo que se abre para pedir algo escribiendo. En la raíz busca
// a la vez entre aplicaciones, órdenes y ventanas abiertas, y hace cuentas;
// de ella cuelgan vistas (el portapapeles, los archivos, los emojis y símbolos, los
// dispositivos de sonido, los esquemas) y, de cada cosa, sus acciones. Aquí está lo que sabe y lo que
// hace; lo que se ve, en base/Prompt.qml.
//
// Todo lo que enseña es una fila, un objeto con: key, kind, title, subtitle,
// label, glyph, icon o character (un emoji o un símbolo), verb (cómo se llama pulsarla), run, y si hace falta
// state y current (en vivo), stay, confirm y more (sus otras acciones).
Singleton {
    id: root

    property bool open: false
    property string monitor: ""  // en qué monitor se abrió
    // La ventana que tenía el foco al abrir ({ address, kind }): en ella se pega.
    property var origin: null
    property real opened: 0      // cuándo, en segundos desde 1970
    property string query: ""
    // Las vistas abiertas sobre la raíz, y lo que había en cada paso.
    property list<string> stack: []
    property var trail: []
    property bool direct: false  // se abrió ya en una vista: Esc no pasa por la raíz
    readonly property string view: stack.length > 0 ? stack[stack.length - 1] : "root"
    property var subject: null   // la fila cuyas acciones se enseñan

    // De cada vista: su nombre, lo que pide el campo vacío y, si no tiene
    // nada que enseñar, con qué icono y qué palabras lo dice.
    readonly property var views: ({
            root: { title: "", hint: "Busca una aplicación o una orden, o escribe una cuenta" },
            clipboard: { title: "Portapapeles", hint: "Busca en lo copiado", glyph: "clipboard-text", none: Clipboard.available ? "Aún no has copiado nada" : "Faltan wl-clipboard o python3" },
            files: { title: "Archivos", hint: "Busca un archivo o una carpeta por su nombre", glyph: "folder-open", none: "Escribe parte del nombre de un archivo", busy: Files.busy ? "Buscando…" : "" },
            symbols: { title: "Emojis y símbolos", hint: "Busca un emoji o un símbolo", glyph: "smiley", none: "Cargando…" },
            outputs: { title: "Salida de audio", hint: "Elige por dónde suena", glyph: "speaker-x", none: "No hay por dónde sonar" },
            inputs: { title: "Entrada de audio", hint: "Elige por dónde entra el sonido", glyph: "microphone-slash", none: "No hay ningún micrófono" },
            schemes: { title: "Esquema de color", hint: "Elige un esquema", glyph: "palette", none: "No hay esquemas" },
            views: { title: "Vistas", hint: "Busca una vista o escribe el nombre de una nueva", glyph: "squares-four", none: "Aún no hay vistas: escribe un nombre («estudio de uni») para crear una" },
            actions: { title: "Acciones", hint: "Busca una acción" }
        })
    // Qué pasa con lo copiado al pulsarlo.
    readonly property string destination: origin ? `Se pega en ${DesktopEntries.heuristicLookup(origin.kind)?.name || origin.kind || "la ventana en la que estabas"}` : "Sin ventana en la que pegar: se copia"

    readonly property var items: compose()
    // Dónde empieza cada fila, de arriba abajo, y dónde acaba la última. Se
    // miden al cambiar las filas y antes de elegir una: un binding podría
    // llegar tarde a quien la pinta.
    property var tops: []
    property real extent: 0
    // Los emojis y símbolos van en cuadrícula: de cada fila, en qué línea y
    // en qué columna cae (los títulos, solos en la suya, en la -1). Cuántas
    // columnas caben lo dice quien la pinta.
    readonly property bool grid: view === "symbols"
    // Su ancho: el portapapeles, más ancho, para la vista previa al lado.
    readonly property int span: view === "clipboard" ? Tokens.promptWide : Tokens.prompt
    property int columns: 1
    property var lines: []
    property var cols: []
    property int index: 0
    readonly property var current: items[index] ?? null
    // Si la fila elegida tiene más acciones que la suya.
    readonly property bool extras: view !== "actions" && current?.more ? current.more().length > 0 : false
    // Cómo se eligió: con las teclas (keys), con el puntero (pointer) o
    // porque las filas cambiaron (reset). Quien la pinta decide con ello si
    // la acompaña o salta.
    property string manner: "reset"
    signal chosen
    property string held: ""      // la fila elegida, por si las filas cambian
    property string seek: ""      // la fila que elegir en cuanto exista
    property bool touched: false  // la elección se ha movido a mano
    property string stamp: ""
    property string armed: ""     // lo que espera una segunda pulsación
    property string note: ""      // lo último que pasó, un momento

    // El uso, tal como estaba al abrir: el orden no cambia con la entrada abierta.
    property var ranks: ({ uses: {}, picks: {} })
    // Lo copiado, ya como filas.
    readonly property var clips: Clipboard.entries.map(clip)
    readonly property var symbols: Symbols.all.map(symbol)

    function tall(item: var): real {
        return item.kind === "header" ? Tokens.heading : item.kind === "sum" ? Tokens.figure : item.kind === "symbol" && grid ? Tokens.cell : Tokens.row;
    }

    // Dónde va cada fila: una debajo de otra o, en la cuadrícula, de
    // `columns` en `columns`.
    function measure(): void {
        const tops = [];
        const lines = [];
        const cols = [];
        let top = 0;
        let line = -1;
        let col = -1;  // la columna de la casilla anterior; -1 si no era una
        for (const item of items) {
            if (grid && item.kind === "symbol") {
                if (col < 0 || col >= columns - 1) {
                    if (col >= 0)
                        top += Tokens.cell;
                    line++;
                    col = 0;
                } else {
                    col++;
                }
                tops.push(top);
                lines.push(line);
                cols.push(col);
            } else {
                if (col >= 0)
                    top += Tokens.cell;
                col = -1;
                tops.push(top);
                lines.push(++line);
                cols.push(-1);
                top += tall(item);
            }
        }
        if (col >= 0)
            top += Tokens.cell;
        root.tops = tops;
        root.lines = lines;
        root.cols = cols;
        extent = top;
    }

    // La primera fila de la línea de `at`.
    function lead(at: int): int {
        while (at > 0 && lines[at - 1] === lines[at])
            at--;
        return at;
    }

    // --- Qué enseña ---

    function header(title: string): var {
        return { key: `header:${title}`, kind: "header", title };
    }

    function compose(): var {
        const text = Match.fold(query.trim()).replace(/\s+/g, " ");
        switch (view) {
        case "clipboard":
            return copied(text);
        case "files":
            return files(text);
        case "symbols":
            return glyphs(text);
        case "outputs":
            return narrow(Sound.sinks.map(Commands.device), text);
        case "inputs":
            return narrow(Sound.sources.map(Commands.device), text);
        case "schemes":
            return narrow(Scheme.all.map(Commands.scheme), text);
        case "views": {
            const rows = narrow(Daemon.views.map(Commands.view), text);
            if (text !== "" && !Daemon.views.some(view => Match.fold(view.name) === text))
                rows.push(Commands.newView(query.trim()));
            return rows;
        }
        case "actions":
            return narrow(actions(subject), text);
        }
        return text === "" ? home() : search(text);
    }

    // Sin nada escrito: lo que más usas, las órdenes y las aplicaciones.
    function home(): var {
        const uses = ranks.uses;
        const liked = Commands.list.concat(Apps.list).filter(item => uses[item.key]).map((item, at) => ({ item, at, weight: weight(uses[item.key]) }));
        liked.sort((a, b) => b.weight - a.weight || a.at - b.at);
        let items = [];
        if (liked.length > 0)
            items = [header("Sugerencias")].concat(liked.slice(0, 5).map(found => found.item));
        return items.concat([header("Órdenes")], Commands.list, [header("Aplicaciones")], Apps.list);
    }

    // Cuánto pesa algo usado `n` veces, la última en `t`: lo reciente, más.
    function weight(use: var): real {
        return Math.log2(1 + use.n) / (1 + Math.max(0, opened - use.t) / 864000);
    }

    function search(text: string): var {
        const uses = ranks.uses;
        const pick = ranks.picks[text];
        const found = [];
        const consider = (item, scale) => {
            let score = Match.rank(text, item.fields) * scale;
            if (score <= 0)
                return;
            // Lo que usas sube un poco; lo que elegiste con estas mismas letras, mucho.
            if (uses[item.key])
                score += Math.min(12, 4 * weight(uses[item.key]));
            if (pick === item.key)
                score += 30;
            found.push({ item, score, at: found.length });
        };
        for (const item of Apps.list)
            consider(item, 1);
        for (const item of Commands.list)
            consider(item, 1);
        for (const toplevel of Hyprland.toplevels.values)
            consider(window(toplevel), 0.85);
        // A igual nota, en el orden en que se encontraron.
        found.sort((a, b) => b.score - a.score || a.at - b.at);

        const items = [];
        // La cuenta, salvo que lo escrito sea el principio de una aplicación
        // («1password» también es, para qalc, un producto de unidades).
        if (Calc.result !== "" && !Apps.list.some(app => app.fields[0].text.startsWith(text)))
            items.push(sum());
        const typed = query.trim();
        // Lo que veyld entiende como una orden («sube el volumen a 85»), si se
        // puede hacer y no es ya una de las que salen («bloquea la pantalla»).
        const said = Daemon.parsed;
        if (said?.ready && said.said === typed && !["confirm", "cancel"].includes(said.intent) && !found.some(result => Match.fold(result.item.title) === Match.fold(said.title)))
            items.push(order(said));
        if (/^(https?:\/\/\S+|[\w-]+(\.[\w-]+)*\.[a-z]{2,}(\/\S*)?)$/i.test(typed))
            items.push(link(typed));
        if (found.length > 0)
            items.push(header("Resultados"));
        for (const result of found.slice(0, 40))
            items.push(result.item);
        if (typed.length >= 2)
            items.push(lookup(typed));
        if (Daemon.thinks)
            items.push(brain(typed));
        if (Settings.webSearch)
            items.push(web(typed));
        return items;
    }

    // Las filas de una vista, las que casan con lo escrito y por parecido.
    function narrow(items: var, text: string): var {
        if (text === "")
            return items;
        const found = [];
        for (const item of items) {
            const score = Match.rank(text, item.fields);
            if (score > 0)
                found.push({ item, score, at: found.length });
        }
        return found.sort((a, b) => b.score - a.score || a.at - b.at).map(result => result.item);
    }

    // Lo copiado: en su orden, lo fijado delante; al buscar, lo que contiene
    // todas las palabras.
    function copied(text: string): var {
        if (text !== "") {
            const words = text.split(" ");
            return clips.filter(item => words.every(word => item.fields[0].text.includes(word)));
        }
        const pinned = clips.filter(item => item.entry.pinned);
        if (pinned.length === 0 || pinned.length === clips.length)
            return clips;
        return [header("Fijado")].concat(pinned, [header("Historial")], clips.filter(item => !item.entry.pinned));
    }

    function clip(entry: var): var {
        const image = entry.kind === "image";
        const id = entry.id;
        return {
            key: `clip:${id}`,
            kind: "clip",
            entry,
            title: image ? entry.width > 0 ? `Imagen de ${entry.width} × ${entry.height}` : "Imagen" : entry.text.trim().replace(/\s+/g, " ").slice(0, 160),
            label: "",
            glyph: ({ image: "image", link: "link", color: "palette", email: "envelope-simple", path: "folder", json: "brackets-curly", math: "equals" })[Clipboard.type(entry)] ?? "text-aa",
            swatches: entry.kind === "color" ? [Clipboard.tint(entry.text.trim())] : null,
            thumb: image ? Clipboard.file(id) : "",
            verb: origin ? "Pegar" : "Copiar",
            fields: [{ text: Match.fold(image ? "imagen image" : entry.text), weight: 1, loose: false }],
            state: () => Clipboard.ago(entry.time, root.opened),
            // Se pega donde se estaba; sin ventana, se queda en el portapapeles.
            run: () => root.origin ? Clipboard.paste(id, root.origin) : Clipboard.copy(id),
            // Primero, lo que se puede hacer con lo que es; luego, lo de siempre.
            more: () => treat(entry).concat([
                    { title: "Copiar sin pegar", glyph: "copy", keys: "ctrl+return", run: () => Clipboard.copy(id) },
                    { title: entry.pinned ? "Soltar" : "Fijar", glyph: entry.pinned ? "push-pin-slash" : "push-pin", keys: "ctrl+shift+p", stay: true, run: () => {
                            Clipboard.run("pin", id);
                            root.say(entry.pinned ? "Suelto" : "Fijado");
                        } },
                    { title: "Borrar", glyph: "trash", keys: "ctrl+x", stay: true, run: () => {
                            Clipboard.run("delete", id);
                            root.say("Borrado");
                        } },
                    { title: "Vaciar el historial", glyph: "broom", keys: "ctrl+shift+x", stay: true, confirm: true, run: () => {
                            Clipboard.run("clear", "");
                            root.say("Historial vacío");
                        } }
                ].slice(root.origin ? 0 : 1))
        };
    }

    // Lo que se puede hacer con lo copiado según lo que es (ver
    // Clipboard.type). Lo que sale de ello se pega donde se estaba, como lo
    // copiado; sin ventana, se queda en el portapapeles.
    function treat(entry: var): var {
        const text = entry.text?.trim() ?? "";
        const whole = Clipboard.whole(entry);
        const verb = origin ? "Pegar" : "Copiar";
        const give = value => Clipboard.put(value, root.origin);
        const opener = Tools.has("xdg-open");
        const found = [];
        switch (Clipboard.type(entry)) {
        case "image":
            if (opener)
                found.push({ title: "Abrir con el visor", glyph: "arrow-square-out", keys: "ctrl+o", run: () => Clipboard.open(entry) });
            found.push({ title: "Guardar en Imágenes", glyph: "download-simple", keys: "ctrl+s", stay: true, run: () => {
                    Clipboard.save(entry);
                    root.say("Imagen guardada");
                } });
            break;
        case "link": {
            found.push({ title: "Abrir el enlace", glyph: "arrow-square-out", keys: "ctrl+o", run: () => Qt.openUrlExternally(text) });
            const clean = Clipboard.clean(text);
            if (clean !== text)
                found.push({ title: `${verb} sin rastreo`, glyph: "eraser", run: () => give(clean) });
            break;
        }
        case "email":
            found.push({ title: "Escribir un correo", glyph: "envelope-simple", keys: "ctrl+o", run: () => Qt.openUrlExternally(`mailto:${text}`) });
            break;
        case "path": {
            const place = Clipboard.place(text);
            const folder = place.replace(/\/+$/, "").replace(/\/[^\/]*$/, "") || "/";
            found.push({ title: "Abrir", glyph: "arrow-square-out", keys: "ctrl+o", run: () => Qt.openUrlExternally(`file://${encodeURI(place)}`) });
            found.push({ title: "Abrir la carpeta", glyph: "folder-open", run: () => Qt.openUrlExternally(`file://${encodeURI(folder)}`) });
            found.push({ title: `${verb} como URI`, glyph: "link", run: () => give(`file://${encodeURI(place)}`) });
            break;
        }
        case "color":
            found.push({ title: `${verb} como RGB`, glyph: "palette", run: () => give(Clipboard.rgb(text)) });
            found.push({ title: `${verb} como HSL`, glyph: "palette", run: () => give(Clipboard.hsl(text)) });
            break;
        case "json": {
            const value = Clipboard.json(entry);
            found.push({ title: `${verb} con sangría`, glyph: "text-indent", run: () => give(JSON.stringify(value, null, 2)) });
            found.push({ title: `${verb} en una línea`, glyph: "brackets-curly", run: () => give(JSON.stringify(value)) });
            break;
        }
        case "math":
            found.push({ title: "Hacer la cuenta", glyph: "equals", keys: "ctrl+o", stay: true, run: () => root.ask(text) });
            break;
        case "text":
            if (Daemon.thinks && text.length <= 2000)
                found.push({ title: "Preguntar a Veyl", glyph: "sparkle", run: () => Daemon.think(text, "launcher") });
            if (Settings.webSearch && !text.includes("\n") && text.length <= 200)
                found.push({ title: "Buscar en la web", glyph: "globe", keys: "ctrl+o", run: () => Qt.openUrlExternally(Settings.webSearch.replace("%s", encodeURIComponent(text))) });
            if (whole !== null) {
                if (whole.toUpperCase() !== whole)
                    found.push({ title: `${verb} en mayúsculas`, glyph: "text-aa", run: () => give(whole.toUpperCase()) });
                if (whole.toLowerCase() !== whole)
                    found.push({ title: `${verb} en minúsculas`, glyph: "text-aa", run: () => give(whole.toLowerCase()) });
                if (text.includes("\n"))
                    found.push({ title: `${verb} en una línea`, glyph: "text-align-left", run: () => give(whole.trim().replace(/\s*\n\s*/g, " ")) });
            }
            break;
        }
        return found;
    }

    // Los archivos: sin nada escrito, lo último que se abrió; al buscar, lo
    // que encuentra Files, por parecido y, a igual parecido, lo menos hondo.
    function files(text: string): var {
        if (text === "") {
            const recent = Files.recent.map(file);
            return recent.length > 0 ? [header("Recientes")].concat(recent) : [];
        }
        // Mientras llega lo nuevo, lo de antes que aún case.
        const stale = Files.asked !== query.trim();
        const words = text.split(" ");
        const found = [];
        for (const entry of Files.found) {
            const item = file(entry);
            // Sin fd solo se buscó la primera palabra: las demás, aquí.
            if ((stale || !Files.fast) && !words.every(word => item.fields[0].text.includes(word)))
                continue;
            const score = Math.max(1, Match.rank(text, item.fields)) - entry.path.split("/").length * 0.5;
            found.push({ item, score, at: found.length });
        }
        return found.sort((a, b) => b.score - a.score || a.at - b.at).slice(0, 60).map(result => result.item);
    }

    function file(entry: var): var {
        const path = entry.path;
        const name = path.slice(path.lastIndexOf("/") + 1) || path;
        const folder = path.slice(0, path.lastIndexOf("/")) || "/";
        const extension = entry.folder || !name.includes(".") ? "" : name.slice(name.lastIndexOf(".") + 1).toLowerCase();
        const kinds = {
            image: ["png", "jpg", "jpeg", "webp", "gif", "bmp", "svg", "avif", "heic"],
            video: ["mp4", "mkv", "webm", "mov", "avi"],
            audio: ["mp3", "flac", "ogg", "opus", "wav", "m4a"],
            pdf: ["pdf"],
            zip: ["zip", "tar", "gz", "xz", "zst", "7z", "rar", "bz2"],
            doc: ["doc", "docx", "odt", "rtf"],
            xls: ["xls", "xlsx", "ods", "csv"],
            ppt: ["ppt", "pptx", "odp"],
            code: ["qml", "js", "ts", "rs", "py", "c", "h", "cpp", "go", "java", "lua", "nix", "sh", "fish", "json", "toml", "yaml", "yml", "html", "css"],
            text: ["txt", "md", "org", "tex", "log"]
        };
        const sort = Object.keys(kinds).find(kind => kinds[kind].includes(extension)) ?? "";
        const where = Files.pretty(folder);
        const url = Files.url(path);
        return {
            key: `file:${path}`,
            kind: "file",
            title: name,
            subtitle: where,
            label: entry.folder ? "Carpeta" : extension.toUpperCase() || "Archivo",
            glyph: entry.folder ? "folder" : sort ? `file-${sort}` : "file",
            // Las imágenes pequeñas de ver, en miniatura.
            thumb: sort === "image" && extension !== "heic" ? url : "",
            detail: Files.pretty(path),
            verb: "Abrir",
            fields: [
                { text: Match.fold(name), weight: 1, loose: false },
                { text: Match.fold(where), weight: 0.5, loose: false }
            ],
            run: () => Qt.openUrlExternally(url),
            more: () => [
                    { title: "Abrir la carpeta", glyph: "folder-open", keys: "ctrl+o", run: () => Qt.openUrlExternally(Files.url(folder)) },
                    { title: root.origin ? "Pegar la ruta" : "Copiar la ruta", glyph: "path", keys: "ctrl+return", run: () => Clipboard.put(path, root.origin) },
                    { title: "Copiar como URI", glyph: "link", run: () => Clipboard.put(url, null) }
                ].concat(root.origin ? [
                    { title: "Copiar la ruta sin pegar", glyph: "copy", run: () => Clipboard.put(path, null) }
                ] : [])
        };
    }

    // Lo escrito, a la vista de los archivos.
    function lookup(typed: string): var {
        return {
            key: "files",
            kind: "seek",
            title: `Buscar «${typed}» en los archivos`,
            label: "Archivos",
            glyph: "folder-open",
            verb: "Buscar",
            stay: true,
            run: () => root.look(typed)
        };
    }

    // Los emojis y símbolos: sin nada escrito, lo último que pegaste y luego
    // todos, por grupos; al buscar, los que más se parecen.
    function glyphs(text: string): var {
        if (text !== "")
            return narrow(symbols, text).slice(0, 150);
        const items = [];
        const recent = Symbols.recent.map(used => symbols.find(item => item.character === used)).filter(item => item);
        if (recent.length > 0)
            items.push(header("Recientes"), ...recent.map(item => Object.assign({}, item, { key: `${item.key}:recent` })));
        let group = "";
        for (const item of symbols) {
            if (item.group !== group) {
                group = item.group;
                items.push(header(group));
            }
            items.push(item);
        }
        return items;
    }

    function symbol(symbol: var): var {
        const text = symbol.text;
        const name = symbol.name.charAt(0).toUpperCase() + symbol.name.slice(1);
        const give = value => {
            Symbols.use(text);
            Clipboard.put(value, root.origin);
        };
        return {
            key: `symbol:${text}`,
            kind: "symbol",
            title: name,
            subtitle: symbol.words,
            label: symbol.group,
            group: symbol.group,
            character: text,
            detail: `${name} · ${Symbols.code(text)}`,
            verb: origin ? "Pegar" : "Copiar",
            fields: [
                { text: Match.fold(symbol.name), weight: 1, loose: false },
                { text: Match.fold(symbol.words), weight: 0.8, loose: false }
            ],
            run: () => give(text),
            // Los tonos de piel, si los tiene, y cómo se escribe en Unicode.
            more: () => symbol.tones.map((tone, i) => ({ title: `${origin ? "Pegar" : "Copiar"} con tono de piel ${Symbols.tones[i] ?? ""}`, character: tone, run: () => give(tone) })).concat(origin ? [
                    { title: "Copiar sin pegar", glyph: "copy", keys: "ctrl+return", run: () => {
                            Symbols.use(text);
                            Clipboard.put(text, null);
                        } }
                ] : [], [
                    { title: `Copiar el código (${Symbols.code(text)})`, glyph: "hash", run: () => Clipboard.put(Symbols.code(text), null) }
                ])
        };
    }

    function window(toplevel: HyprlandToplevel): var {
        const app = (toplevel.lastIpcObject.class ?? "").split(".").pop();
        const address = toplevel.address;
        return {
            key: `window:${address}`,
            kind: "window",
            title: toplevel.title,
            subtitle: app,
            label: "Ventana",
            detail: toplevel.workspace ? `En el nivel ${toplevel.workspace.name}` : "",
            emblem: toplevel.lastIpcObject.class ?? "",
            glyph: "browsers",
            verb: "Ir a la ventana",
            fields: [
                { text: Match.fold(toplevel.title), weight: 1, loose: false },
                { text: Match.fold(app), weight: 0.9, loose: false }
            ],
            run: () => Compositor.run(`hl.dispatch(hl.dsp.focus({ window = "address:0x${address}" }))`),
            more: () => [
                    { title: "Cerrar la ventana", glyph: "x-circle", keys: "ctrl+x", run: () => Compositor.run(`hl.dispatch(hl.dsp.window.close({ window = "address:0x${address}" }))`) }
                ]
        };
    }

    // Una orden dicha con palabras: la hace veyld.
    function order(said: var): var {
        return {
            key: `said:${said.intent}`,
            kind: "said",
            title: said.title,
            subtitle: "",
            label: "Orden",
            glyph: said.glyph,
            verb: "Ejecutar",
            confirm: said.confirm,
            run: () => Daemon.say(said.said, "launcher", true)
        };
    }

    function sum(): var {
        const result = Calc.result;
        const whole = `${Calc.asked} = ${result}`;
        return {
            key: "sum",
            kind: "sum",
            title: Calc.asked,
            result,
            verb: "Copiar el resultado",
            run: () => root.copy(result),
            more: () => [
                    { title: "Copiar la cuenta entera", glyph: "equals", keys: "ctrl+return", run: () => root.copy(whole) }
                ]
        };
    }

    function link(typed: string): var {
        const url = /^https?:\/\//i.test(typed) ? typed : `https://${typed}`;
        return {
            key: "link",
            kind: "link",
            title: `Abrir ${typed}`,
            label: "Enlace",
            glyph: "link",
            verb: "Abrir",
            run: () => Qt.openUrlExternally(url),
            more: () => [
                    { title: "Copiar el enlace", glyph: "copy", keys: "ctrl+return", run: () => root.copy(url) }
                ]
        };
    }

    // Lo escrito, al cerebro tal cual: contesta en la isla, con su tarjeta.
    function brain(typed: string): var {
        return {
            key: "brain",
            kind: "brain",
            title: `Preguntar «${typed}» a Veyl`,
            label: Settings.brain?.provider || "Cerebro",
            glyph: "sparkle",
            verb: "Preguntar",
            run: () => Daemon.think(typed, "launcher")
        };
    }

    function web(typed: string): var {
        const engine = Settings.webSearch;
        return {
            key: "web",
            kind: "web",
            title: `Buscar «${typed}» en la web`,
            label: engine.replace(/^\w+:\/\/(www\.)?/, "").split("/")[0],
            glyph: "globe",
            verb: "Buscar",
            run: () => Qt.openUrlExternally(engine.replace("%s", encodeURIComponent(typed)))
        };
    }

    // Lo que se puede hacer con una fila: lo suyo, primero, y lo demás.
    function actions(item: var): var {
        if (!item)
            return [];
        const main = { title: item.verb, glyph: "arrow-bend-down-left", keys: "return", run: item.run, stay: item.stay, confirm: item.confirm };
        return [main].concat(item.more ? item.more() : []).map((action, i) => ({
                    key: `${item.key}#${i}`,
                    kind: "action",
                    title: action.title,
                    label: "",
                    glyph: action.glyph,
                    character: action.character ?? (i === 0 ? item.character : undefined),
                    keys: action.keys ?? "",
                    verb: action.title,
                    fields: [{ text: Match.fold(action.title), weight: 1, loose: true }],
                    run: action.run,
                    stay: action.stay,
                    confirm: action.confirm,
                    owner: i === 0 ? item : null
                }));
    }

    function copy(text: string): void {
        if (Tools.has("wl-copy"))
            Quickshell.execDetached(["wl-copy", "--", text]);
        else
            Quickshell.clipboardText = text;
    }

    // --- Cuál está elegida ---

    function select(at: int, manner: string): void {
        root.manner = manner;
        index = at;
        held = items[at]?.key ?? "";
        armed = "";
        chosen();
    }

    function first(): int {
        return Math.max(0, items.findIndex(item => item.kind !== "header"));
    }

    // Se mueve `by` filas, saltándose los títulos y sin salirse; con `wrap`,
    // de la última pasa a la primera y al revés.
    function move(by: int, wrap: bool): void {
        const way = by > 0 ? 1 : -1;
        let at = index;
        let left = Math.abs(by);
        for (let next = index + way; next >= 0 && next < items.length && left > 0; next += way) {
            if (items[next].kind === "header")
                continue;
            at = next;
            left--;
        }
        if (at === index && wrap)
            at = way > 0 ? first() : Math.max(0, items.length - 1);
        touched = true;
        select(at, "keys");
    }

    // En la cuadrícula, `by` líneas arriba o abajo, en la misma columna o en
    // la última que haya; fuera de ella, como move.
    function step(by: int, wrap: bool): void {
        if (!grid)
            return move(by, wrap);
        const way = by > 0 ? 1 : -1;
        const col = Math.max(0, cols[index] ?? 0);
        let at = index;
        for (let left = Math.abs(by); left > 0; left--) {
            let next = at + way;
            while (next >= 0 && next < items.length && (lines[next] === lines[at] || items[next].kind === "header"))
                next += way;
            if (next < 0 || next >= items.length)
                break;
            const start = lead(next);
            let end = next;
            while (end < items.length - 1 && lines[end + 1] === lines[next])
                end++;
            at = Math.min(start + col, end);
        }
        if (at === index && wrap)
            at = way > 0 ? first() : Math.max(0, items.length - 1);
        touched = true;
        select(at, "keys");
    }

    function point(at: int): void {
        if (at === index || !items[at] || items[at].kind === "header")
            return;
        touched = true;
        select(at, "pointer");
    }

    // Las primeras filas no avisan de que han cambiado.
    Component.onCompleted: itemsChanged()

    onColumnsChanged: {
        if (grid) {
            measure();
            chosen();
        }
    }

    onItemsChanged: {
        measure();
        const now = `${view}\n${query}`;
        const fresh = now !== stamp;
        stamp = now;
        const wanted = seek !== "" ? seek : fresh || !touched ? "" : held;
        seek = "";
        const at = wanted !== "" ? items.findIndex(item => item.key === wanted) : -1;
        if (at >= 0) {
            touched = true;
            select(at, "reset");
        } else if (wanted !== "" && !fresh) {
            // La fila elegida ya no está: la que ocupa su sitio.
            let near = Math.min(index, items.length - 1);
            if (items[near]?.kind === "header")
                near = Math.min(near + 1, items.length - 1);
            select(Math.max(0, near), "reset");
        } else {
            touched = false;
            // Entre dispositivos o esquemas, sin nada escrito, el que está en uso.
            const using = query === "" ? items.findIndex(item => item.current?.()) : -1;
            select(using >= 0 ? using : first(), "reset");
        }
    }

    // --- Qué hace ---

    // Pulsa una fila o, si se da, una de sus otras acciones.
    function activate(item: var, action: var): void {
        const act = action ?? item;
        if (!act?.run)
            return;
        const id = `${item.key}/${act.title}`;
        if (act.confirm && armed !== id) {
            armed = id;
            return;
        }
        armed = "";
        remember(action ? null : item.owner ?? item);
        const within = view === "actions";
        act.run();
        if (!act.stay)
            hide();
        else if (within && view === "actions")
            back();
    }

    // La tecla (ver chord) de alguna de las otras acciones de la fila elegida.
    function shortcut(keys: string): bool {
        const item = view === "actions" ? subject : current;
        const action = item?.more ? item.more().find(action => action.keys === keys) : null;
        if (!action)
            return false;
        if (view === "actions")
            back();
        activate(item, action);
        return true;
    }

    // Una pulsación como texto: «ctrl+shift+p», «ctrl+return».
    function chord(event: var): string {
        const named = ({ [Qt.Key_Return]: "return", [Qt.Key_Enter]: "return", [Qt.Key_Backspace]: "backspace", [Qt.Key_Delete]: "delete" })[event.key];
        const key = named ?? (event.key >= Qt.Key_A && event.key <= Qt.Key_Z ? String.fromCharCode(event.key).toLowerCase() : "");
        if (key === "")
            return "";
        const held = [[Qt.ControlModifier, "ctrl"], [Qt.AltModifier, "alt"], [Qt.ShiftModifier, "shift"]].filter(mod => event.modifiers & mod[0]).map(mod => mod[1]);
        return held.concat([key]).join("+");
    }

    function remember(item: var): void {
        if (item?.kind !== "app" && item?.kind !== "order")
            return;
        const uses = Object.assign({}, memory.uses);
        uses[item.key] = { n: (uses[item.key]?.n ?? 0) + 1, t: Math.round(Date.now() / 1000) };
        memory.uses = uses;
        const text = Match.fold(query.trim()).replace(/\s+/g, " ");
        if (view !== "root" || text === "")
            return;
        // Con qué letras se eligió; de las más antiguas se olvida.
        const picks = Object.assign({}, memory.picks);
        delete picks[text];
        picks[text] = item.key;
        const all = Object.keys(picks);
        for (const old of all.slice(0, Math.max(0, all.length - 300)))
            delete picks[old];
        memory.picks = picks;
    }

    // Entra en los archivos con `text` ya escrito.
    function look(text: string): void {
        push("files");
        query = text;
    }

    // Vuelve a la raíz con `text` escrito: así se hace una cuenta copiada.
    function ask(text: string): void {
        trail = [];
        subject = null;
        stack = [];
        direct = false;
        query = text;
    }

    function say(text: string): void {
        note = text;
        fade.restart();
    }

    // --- Abrir, cerrar y moverse entre vistas ---

    function show(view: string): void {
        if (Recall.open)
            Recall.hide();
        if (open && root.view === (view || "root"))
            return;
        if (!open) {
            monitor = Hyprland.focusedMonitor?.name ?? "";
            const window = Hyprland.activeToplevel;
            origin = window?.activated ? { address: `0x${window.address}`, kind: window.lastIpcObject.class ?? "" } : null;
            opened = Date.now() / 1000;
            ranks = { uses: memory.uses ?? {}, picks: memory.picks ?? {} };
            Tools.probe();
            Commands.refresh();
            Glance.release();
            Tuning.hide();
        }
        direct = view !== "";
        trail = [];
        subject = null;
        stack = [];
        query = "";
        note = "";
        if (view !== "")
            enter(view);
        open = true;
    }

    function hide(): void {
        open = false;
        armed = "";
    }

    function toggle(view: string): void {
        if (open && root.view === (view || "root"))
            hide();
        else
            show(view);
    }

    function enter(view: string): void {
        if (view === "schemes")
            Scheme.scan();
        if (view === "symbols")
            Symbols.load();
        stack = stack.concat([view]);
    }

    // Entra en una vista; a la vuelta todo sigue como estaba.
    function push(view: string): void {
        trail = trail.concat([{ query, key: current?.key ?? "" }]);
        // Primero lo escrito y luego la vista: las filas de en medio, sin filtro.
        query = "";
        enter(view);
    }

    function back(): void {
        if (stack.length === 0)
            return;
        const last = trail[trail.length - 1];
        trail = trail.slice(0, -1);
        // Primero lo escrito, para que la fila que buscar ya esté al cambiar de vista.
        query = last?.query ?? "";
        seek = last?.key ?? "";
        stack = stack.slice(0, -1);
        subject = null;
        if (stack.length === 0)
            direct = false;
    }

    // Las acciones de la fila elegida, o de vuelta si ya se enseñan.
    function more(): void {
        if (view === "actions") {
            back();
        } else if (extras) {
            subject = current;
            push("actions");
        }
    }

    // Esc deshace lo último: la confirmación, lo escrito, la vista; y cierra.
    function cancel(): void {
        if (armed !== "")
            armed = "";
        else if (view === "actions")
            back();
        else if (query !== "")
            query = "";
        else if (stack.length > (direct ? 1 : 0))
            back();
        else
            hide();
    }

    // La cuenta sigue a lo escrito en la raíz, y veyld también lo lee.
    Binding {
        target: Calc
        property: "expression"
        value: root.open && root.view === "root" ? root.query : ""
    }

    Binding {
        target: Files
        property: "query"
        value: root.open && root.view === "files" ? root.query : ""
    }

    Binding {
        target: Daemon
        property: "query"
        value: root.open && root.view === "root" ? root.query : ""
    }

    Timer {
        id: fade
        interval: 1600
        onTriggered: root.note = ""
    }

    // Qué se usa y con qué letras se eligió: de ahí salen las sugerencias.
    FileView {
        path: `${Quickshell.env("XDG_STATE_HOME") || Quickshell.env("HOME") + "/.local/state"}/veyl/launcher.json`
        blockLoading: true
        printErrors: false
        onAdapterUpdated: writeAdapter()

        JsonAdapter {
            id: memory

            property var uses: ({})   // fila → { n: veces, t: la última }
            property var picks: ({})  // lo escrito → la fila que se eligió
        }
    }

    GlobalShortcut {
        appid: "veyl"
        name: "launcher"
        description: "La entrada: la abre o la cierra"
        onPressed: root.toggle("")
    }

    GlobalShortcut {
        appid: "veyl"
        name: "clipboard"
        description: "La entrada, en el historial del portapapeles"
        onPressed: root.toggle("clipboard")
    }

    GlobalShortcut {
        appid: "veyl"
        name: "files"
        description: "La entrada, en los archivos"
        onPressed: root.toggle("files")
    }

    GlobalShortcut {
        appid: "veyl"
        name: "symbols"
        description: "La entrada, en los emojis y símbolos"
        onPressed: root.toggle("symbols")
    }

    IpcHandler {
        target: "launcher"

        function toggle(): void {
            root.toggle("");
        }

        function open(): void {
            root.show("");
        }

        function close(): void {
            root.hide();
        }

        // Abre o cierra una vista: clipboard, files, symbols, outputs, inputs o schemes.
        function view(name: string): string {
            if (name === "root" || name === "actions" || !root.views[name])
                return `no existe la vista ${name}`;
            root.toggle(name);
            return "";
        }

        function type(text: string): void {
            root.show("");
            root.query = text;
        }

        function get(): bool {
            return root.open;
        }
    }
}
