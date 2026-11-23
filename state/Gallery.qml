pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io

// La galería de tarjetas: ejemplos de todo lo que el cerebro sabe componer
// (data/cards.json, comprobados contra veyld/src/card.rs), para verlos en la
// isla de verdad sin preguntar nada. `veyl cards` (o la orden «Galería de
// tarjetas» de la entrada) saca la primera; sus flechas pasan a la siguiente.
// `veyl cards demo` la reproduce entera: «Pensando» con sus pasos y sus
// fuentes, la tarjeta en cuanto «se compone» y, al rato, acabada. No pasa por
// veyld: es solo lo que se ve.
Singleton {
    id: root

    property var examples: []
    readonly property int count: examples.length
    property int index: 0
    // Cada tarjeta que se saca lleva un número propio, negativo para no
    // chocar con los de veyld: la que se cierra no vuelve.
    property int serial: 0

    function wrap(i: int): int {
        return count > 0 ? ((i % count) + count) % count : 0;
    }

    // Lo que Veyl enseña de un ejemplo: su tarjeta, de ejemplo (no se va sola y
    // lleva las flechas), con lo que se preguntó.
    function shown(i: int, live: bool): var {
        const example = examples[i];
        return Object.assign({}, example.card, {
            n: serial,
            said: example.said,
            live,
            demo: true,
            pager: `Ejemplo ${i + 1} de ${count}`
        });
    }

    function show(i: int): void {
        if (count === 0)
            return;
        player.stop();
        index = wrap(i);
        serial--;
        Daemon.thinking = null;
        Daemon.card = shown(index, false);
    }

    function step(way: int): void {
        show(index + way);
    }

    // Lo de antes de la tarjeta: «Pensando», sus pasos uno a uno y sus fuentes.
    property var script: []
    property int stage: 0

    function play(i: int): void {
        if (count === 0)
            return;
        index = wrap(i);
        serial--;
        const example = examples[index];
        Daemon.card = null;
        Daemon.thinking = { said: example.said, since: Date.now(), n: serial, stop: false, steps: [], sources: [] };
        const steps = example.pasos ?? [];
        const script = steps.map((step, k) => ({ step, sources: k === 0 ? (example.fuentes ?? []) : [] }));
        root.script = script;
        stage = 0;
        player.restart();
    }

    function close(): void {
        player.stop();
        if (Daemon.card?.demo)
            Daemon.close();
        if (Daemon.thinking && Daemon.thinking.n < 0)
            Daemon.thinking = null;
    }

    Timer {
        id: player
        interval: 1500
        repeat: true
        onTriggered: {
            const thought = Daemon.thinking;
            if (root.stage < root.script.length && thought && thought.n === root.serial) {
                const next = root.script[root.stage];
                const steps = thought.steps.concat([Object.assign({ at: Date.now() }, next.step)]);
                const sources = thought.sources.concat(next.sources.map(host => ({ titulo: "", host, url: `https://${host}` })));
                Daemon.thinking = Object.assign({}, thought, { steps, sources });
            } else if (root.stage === root.script.length) {
                Daemon.card = root.shown(root.index, true);
            } else {
                Daemon.thinking = null;
                if (Daemon.card?.n === root.serial)
                    Daemon.card = root.shown(root.index, false);
                stop();
            }
            root.stage++;
        }
    }

    FileView {
        path: `${Quickshell.shellDir}/data/cards.json`
        watchChanges: true
        onFileChanged: reload()
        onLoaded: {
            try {
                root.examples = JSON.parse(text()).filter(e => e?.card?.bloques);
            } catch (e) {
                console.warn("data/cards.json no se entiende:", e);
            }
        }
    }

    IpcHandler {
        target: "cards"

        // Saca un ejemplo (desde 1); sin número, el que tocaba. (No se llama
        // `show`: `qs ipc call cards show 5` lo toma por `qs ipc show`.)
        function open(number: int): string {
            if (root.count === 0)
                return "no hay ejemplos (data/cards.json)";
            root.show(number > 0 ? number - 1 : root.index);
            return "";
        }

        function next(): void {
            root.step(1);
        }

        function prev(): void {
            root.step(-1);
        }

        // Lo reproduce entero: «Pensando», sus pasos y luego la tarjeta.
        function demo(number: int): string {
            if (root.count === 0)
                return "no hay ejemplos (data/cards.json)";
            root.play(number > 0 ? number - 1 : root.index);
            return "";
        }

        function close(): void {
            root.close();
        }

        // Los ejemplos, uno por línea, con su número.
        function list(): string {
            return root.examples.map((e, i) => `${i + 1}  ${e.card.titulo}  ·  ${e.said}`).join("\n");
        }
    }
}
