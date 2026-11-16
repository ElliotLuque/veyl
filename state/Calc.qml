pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io

// La calculadora de la entrada: Qalculate (`qalc`), con su configuración y
// su idioma. Cada cosa que se escribe y parece una cuenta se le pregunta a
// un `qalc` nuevo, que tarda unas decenas de milisegundos; el anterior, si
// aún no ha contestado, se descarta.
Singleton {
    id: root

    readonly property bool available: Tools.has("qalc")
    property string expression: ""  // lo que hay escrito
    property string asked: ""       // la cuenta de la que es `result`
    property string result: ""      // vacío: no hay nada que enseñar
    property var pending: null

    // Si merece la pena preguntarlo: con «=» delante, siempre; si no, algo
    // con cifras que empiece como una cuenta o lleve un operador. Así «h264»
    // o «kitty», que para qalc son productos de unidades, no dan resultado.
    function worth(text: string): bool {
        if (text.startsWith("="))
            return text.length > 1;
        return /\d/.test(text) && (/^[\d(.,√+-]/.test(text) || /[+*\/^()!%=×÷]/.test(text));
    }

    // «15% de 200» es «15% × 200»; y nada que empiece por «-» es una opción.
    function tidy(text: string): string {
        const sum = text.replace(/^=\s*/, "").replace(/(\d\s*%)\s*(?:of|de)\s+/gi, "$1 × ").trim();
        return sum.startsWith("-") ? `0${sum}` : sum;
    }

    function same(a: string, b: string): bool {
        const bare = text => text.replace(/[\s"]/g, "").replace(/−/g, "-").toLowerCase();
        return bare(a) === bare(b);
    }

    function drop(): void {
        if (pending)
            pending.stale = true;
        pending = null;
    }

    onExpressionChanged: refresh()
    onAvailableChanged: refresh()

    function refresh(): void {
        drop();
        delay.stop();
        asked = "";
        result = "";
        const text = expression.trim();
        if (!available || !worth(text)) {
            result = "";
            return;
        }
        delay.restart();
    }

    // No lanzar un proceso por cada tecla de una ráfaga de escritura.
    Timer {
        id: delay
        interval: 100
        onTriggered: root.pending = asking.createObject(root, { sum: root.tidy(root.expression.trim()) })
    }

    // Las tasas de cambio que trae qalc envejecen: se le piden las de hoy.
    function update(): void {
        Quickshell.execDetached(["qalc", "-t", "-e", "1 + 1"]);
    }

    Component {
        id: asking

        Process {
            id: process

            required property string sum
            property bool stale: false
            property int code: -1
            property bool read: false

            // Cuando ha salido y se ha leído lo que dijo, en el orden que sea.
            function settle(): void {
                if (code < 0 || !read)
                    return;
                if (!stale) {
                    const answer = output.text.trim().split("\n").pop().replace(/^"(.*)"$/, "$1");
                    const fine = code === 0 && answer !== "" && !root.same(answer, sum);
                    root.asked = fine ? sum : "";
                    root.result = fine ? answer : "";
                    root.pending = null;
                }
                destroy();
            }

            running: true
            // -t: solo el resultado, y sin preguntar por las tasas de cambio;
            // -m: una cuenta que no acaba se corta.
            command: ["qalc", "-t", "-m", "1500", sum]
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
