import QtQuick
import Quickshell
import qs
import qs.base.card
import qs.state

// La tarjeta que el cerebro compone para una pregunta: la respuesta a la
// medida de lo preguntado, con piezas de un vocabulario fijo (veyld/src/card.rs):
// texto, lista, datos, cifra, métricas, cuenta atrás, gráficas (barras, líneas,
// anillo), tabla, pros y contras, tira, pasos, cronología, valoración, cita,
// código, galería, etiquetas, imagen, barra, botones (mandan una orden a Veyl)
// y enlaces, y preguntas para seguir (van al cerebro, con la conversación);
// y la previsión del tiempo, un marcador, un versus, medidores, barras de
// progreso, casillas, una definición, un aviso y una paleta. El modelo elige
// el estilo de la cabecera (ficha, portada a sangre o titular). Todo va en el
// acento del tema, en colores lisos (sin degradados), y el modelo solo marca
// cosas pequeñas con un color con sentido (bien, aviso, mal). Sale en la isla del centro en
// cuanto el modelo la compone, con sus piezas entrando una tras otra; mientras
// acaba de escribir (`live`), su icono respira, y lo que le llega después (la
// frase, la portada) la completa sin rehacerla.
Item {
    id: root

    property var card: null
    signal done
    // Lo que se hace con una pregunta de seguimiento: de fábrica, al cerebro.
    property var follow: text => Daemon.think(text, "card")
    // Escribiendo la siguiente pregunta: la tarjeta no se va sola, y su capa
    // necesita el teclado.
    readonly property bool typing: next.activeFocus || next.text !== ""

    // Las piezas solo cambian si cambian de verdad: completar la tarjeta no
    // las vuelve a crear (ni a hacer entrar).
    property var blocks: []
    function sync(): void {
        const next = card?.bloques ?? [];
        if (JSON.stringify(next) !== JSON.stringify(blocks))
            blocks = next;
    }
    onCardChanged: sync()
    Component.onCompleted: sync()
    readonly property bool live: !!card?.live

    readonly property color tone: Tokens.accent
    readonly property string style: card?.estilo ?? ""

    // Una mezcla lisa de dos colores: f = 0 es a; f = 1, b.
    function mix(a: color, b: color, f: real): color {
        return Qt.rgba(a.r + (b.r - a.r) * f, a.g + (b.g - a.g) * f, a.b + (b.b - a.b) * f, 1);
    }

    // Mientras el modelo acaba de escribir, lo que identifica a la tarjeta respira.
    property real breath: 1
    SequentialAnimation on breath {
        running: root.live && root.visible
        loops: Animation.Infinite
        NumberAnimation { to: 0.45; duration: 700; easing.type: Easing.InOutSine }
        NumberAnimation { to: 1; duration: 700; easing.type: Easing.InOutSine }
        onRunningChanged: if (!running) root.breath = 1
    }

    // Los colores con sentido que puede pedir el modelo, del esquema.
    function tint(name: string, otherwise: color): color {
        switch (name) {
        case "bien": return Tokens.done;
        case "aviso": return Tokens.attention;
        case "mal": return Tokens.critical;
        case "neutro": return Tokens.dim;
        default: return otherwise;
        }
    }

    // Una cifra corta: «82 k €», «1,2 M», «4,5 h».
    function figure(n: real, unit: string): string {
        const big = Math.abs(n) >= 1e6 ? [n / 1e6, " M"] : Math.abs(n) >= 1e4 ? [n / 1e3, " k"] : [n, ""];
        const digits = Number.isInteger(big[0]) || Math.abs(big[0]) >= 100 ? 0 : 1;
        return Number(big[0]).toLocaleString(Qt.locale(), "f", digits) + big[1] + (unit ? " " + unit : "");
    }

    function soft(c: color, alpha: real): color {
        return Qt.rgba(c.r, c.g, c.b, alpha);
    }

    // Lo que cabe en la pantalla: de ancho y de alto.
    property real room: Tokens.answerWide
    property real limit: 900

    // Las piezas del cuerpo y, aparte y por este orden debajo, de dónde sale
    // (enlaces pequeños), lo que se puede hacer (botones) y, al pie y
    // separado, lo que se puede seguir preguntando.
    readonly property var below: ["enlaces", "acciones", "preguntas"]
    readonly property var main: blocks.filter(b => !below.includes(b.tipo))
    readonly property var sources: blocks.filter(b => b.tipo === "enlaces")
    readonly property var actions: blocks.filter(b => b.tipo === "acciones")
    readonly property var asks: blocks.filter(b => b.tipo === "preguntas")

    // Lo que ocupa a lo alto cada pieza, a ojo, en una columna.
    function weight(b: var): real {
        switch (b.tipo) {
        case "texto": return 1 + b.texto.length / 110;
        case "lista": case "pasos": case "cronologia": return b.items.length * 1.1;
        case "datos": return 0.6 + b.items.length * 0.5;
        case "tabla": return 1 + b.items.length * 0.7;
        case "metricas": return Math.ceil(b.items.length / 2) * 1.6;
        case "pros": return 1 + Math.max(b.pros.length, b.contras.length) * 1.1;
        case "grafica": case "imagen": return 4.5;
        case "cuenta": case "tira": case "galeria": return 2.4;
        case "codigo": return 1 + b.texto.split("\n").length * 0.4;
        case "prevision": return 1 + b.items.length * 0.75 + (b.valor ? 2 : 0);
        case "marcador": return 3.4;
        case "versus": return 1.8 + b.items.length * 0.9;
        case "medidor": return 3.4;
        case "progreso": return b.items.length * 0.95;
        case "checklist": return b.items.length * 0.8;
        case "definicion": return 1.6 + b.items.length * 1.2;
        case "aviso": return 1.3 + b.texto.length / 110;
        case "paleta": return 2.6;
        default: return 1.5;
        }
    }
    readonly property var weights: main.map(b => weight(b))
    readonly property real heft: weights.reduce((sum, w) => sum + w, 0)
    // Mucho que enseñar: ancha y en dos columnas, en vez de crecer sin fin.
    readonly property bool wide: main.length >= 2 && heft > 10 && room >= Tokens.answer * 1.4
    // Dónde se parte: el corte que deja las dos columnas más parejas.
    readonly property int cut: {
        let best = 1, gap = Infinity, sum = 0;
        for (let i = 1; i < weights.length; i++) {
            sum += weights[i - 1];
            const off = Math.abs(heft - 2 * sum);
            if (off < gap) {
                gap = off;
                best = i;
            }
        }
        return best;
    }

    implicitWidth: wide ? Math.min(Tokens.answerWide, room) : Tokens.answer
    implicitHeight: Math.min(content.height, limit)

    component Piece: Loader {
        id: piece

        required property var modelData
        required property int index
        // Dónde va entre todas, para entrar una tras otra.
        property int offset: 0
        readonly property color tone: root.tint(modelData.color ?? "", root.tone)

        opacity: 0
        transform: Translate {
            id: lift
            y: 10
        }
        Component.onCompleted: rise.start()

        SequentialAnimation {
            id: rise
            PauseAnimation { duration: Math.min(piece.index + piece.offset, 10) * 55 }
            ParallelAnimation {
                NumberAnimation { target: piece; property: "opacity"; to: 1; duration: 260; easing.type: Easing.OutCubic }
                NumberAnimation { target: lift; property: "y"; to: 0; duration: 340; easing.type: Easing.OutCubic }
            }
        }

        sourceComponent: ({
                texto: prose,
                lista: list,
                datos: pairs,
                cifra: figure,
                cuenta: countdown,
                barra: meter,
                etiquetas: tags,
                imagen: photo,
                acciones: actions,
                enlaces: links,
                barras: bars,
                lineas: lines,
                anillo: ring,
                tabla: table,
                pros: proscons,
                metricas: tiles,
                tira: strip,
                pasos: steps,
                cronologia: timeline,
                valoracion: rating,
                cita: quote,
                codigo: code,
                galeria: gallery,
                preguntas: asks,
                prevision: forecast,
                marcador: score,
                versus: duel,
                medidor: gauges,
                progreso: progress,
                checklist: checks,
                definicion: definition,
                aviso: callout,
                paleta: palette
            })[modelData.tipo === "grafica" ? modelData.forma : modelData.tipo] ?? null

        Component {
            id: prose

            Prose {
                block: piece
                host: root
            }
        }

        Component {
            id: list

            Bullets {
                block: piece
                host: root
            }
        }

        Component {
            id: pairs

            Pairs {
                block: piece
                host: root
            }
        }

        Component {
            id: figure

            Figure {
                block: piece
                host: root
            }
        }

        Component {
            id: countdown

            Remaining {
                block: piece
                host: root
            }
        }

        Component {
            id: meter

            Meter {
                block: piece
                host: root
            }
        }

        Component {
            id: bars

            Bars {
                block: piece
                host: root
            }
        }

        Component {
            id: lines

            Trend {
                block: piece
                host: root
            }
        }

        Component {
            id: ring

            Ring {
                block: piece
                host: root
            }
        }

        Component {
            id: table

            Sheet {
                block: piece
                host: root
            }
        }

        Component {
            id: proscons

            ProsCons {
                block: piece
                host: root
            }
        }

        Component {
            id: tiles

            Tiles {
                block: piece
                host: root
            }
        }

        Component {
            id: strip

            Rail {
                block: piece
                host: root
            }
        }

        Component {
            id: steps

            Steps {
                block: piece
                host: root
            }
        }

        Component {
            id: timeline

            Chronicle {
                block: piece
                host: root
            }
        }

        Component {
            id: rating

            Rating {
                block: piece
                host: root
            }
        }

        Component {
            id: quote

            Quote {
                block: piece
                host: root
            }
        }

        Component {
            id: code

            Snippet {
                block: piece
                host: root
            }
        }

        Component {
            id: gallery

            Shelf {
                block: piece
                host: root
            }
        }

        Component {
            id: tags

            Tags {
                block: piece
                host: root
            }
        }

        Component {
            id: photo

            Photo {
                block: piece
                host: root
            }
        }

        Component {
            id: actions

            Actions {
                block: piece
                host: root
            }
        }

        // De dónde sale: enlaces pequeños, solo subrayados, tras «Fuentes».
        Component {
            id: links

            Sources {
                block: piece
                host: root
            }
        }

        // El tiempo: ahora, en grande, y una fila por día u hora con su
        // tramo de temperatura dentro del de toda la previsión.
        Component {
            id: forecast

            Forecast {
                block: piece
                host: root
            }
        }

        // Un resultado: los dos lados con su escudo (o su inicial) y el
        // marcador en grande; el que va ganando, claro, y el otro, apagado.
        Component {
            id: score

            Scoreboard {
                block: piece
                host: root
            }
        }

        // Dos frente a frente: uno a cada lado (el primero en el acento) y,
        // por cada cosa que se compara, dos barras que salen del centro.
        Component {
            id: duel

            Duel {
                block: piece
                host: root
            }
        }

        // Anillos de progreso, uno por cifra; se llenan al salir.
        Component {
            id: gauges

            Gauges {
                block: piece
                host: root
            }
        }

        // Barras horizontales con nombre.
        Component {
            id: progress

            Progress {
                block: piece
                host: root
            }
        }

        // Casillas que se marcan al pulsarlas (los ingredientes, qué llevar).
        Component {
            id: checks

            Checklist {
                block: piece
                host: root
            }
        }

        // Qué significa algo: el término en grande, su categoría en el tono y
        // las acepciones numeradas, con su ejemplo.
        Component {
            id: definition

            Definition {
                block: piece
                host: root
            }
        }

        // Lo que no debe pasarse por alto: una caja teñida con su icono.
        Component {
            id: callout

            Callout {
                block: piece
                host: root
            }
        }

        // Colores: muestras grandes con su código; pulsar una lo copia.
        Component {
            id: palette

            Swatches {
                block: piece
                host: root
            }
        }

        // Lo que probablemente quieras preguntar después: va directo al
        // cerebro, que recuerda lo hablado, y la isla vuelve a «Pensando».
        Component {
            id: asks

            Asks {
                block: piece
                host: root
            }
        }
    }

    Flickable {
        id: scroller
        anchors.fill: parent
        contentHeight: content.height
        interactive: contentHeight > height
        boundsBehavior: Flickable.StopAtBounds
        clip: true

        Item {
            id: content
            width: root.width
            height: body.y + body.implicitHeight + Tokens.pad

            // Portada a sangre: la imagen de lado a lado, desde el borde de la
            // pantalla, y el título grande debajo. Si no carga, la cabeza normal.
            Item {
                id: hero
                width: parent.width
                height: visible ? Math.round(Math.min(300, width * 0.5)) : 0
                visible: root.style === "portada" && art.status === Image.Ready
                clip: true

                Image {
                    id: art
                    anchors.fill: parent
                    source: root.style === "portada" ? (root.card?.imagen ?? "") : ""
                    sourceSize.width: root.width * 2
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                    cache: true
                }
            }

            Column {
                id: body

                x: Tokens.pad
                y: hero.visible ? hero.height + 10 : Tokens.pad
                width: root.width - Tokens.pad * 2
                spacing: 14

                // Portada recuadrada (en una ficha). Puede llegar con la tarjeta
                // ya a la vista: entonces se abre hueco poco a poco.
                Picture {
                    id: cover
                    width: parent.width
                    tall: 210
                    source: root.style === "" || root.style === "ficha" ? (root.card?.imagen ?? "") : ""

                    Behavior on height {
                        NumberAnimation { duration: Tokens.veil; easing.type: Easing.OutCubic }
                    }
                }

                // Cabeza de ficha: icono del tono, título y subtítulo (o lo que se preguntó).
                Item {
                    width: parent.width
                    implicitHeight: Math.max(glyph.height, heading.implicitHeight)
                    visible: root.style !== "titular" && root.style !== "portada"

                    Badge {
                        id: glyph
                        anchors.verticalCenter: parent.verticalCenter
                        size: 42
                        icon: root.card?.icono || "sparkle"
                        opacity: root.breath
                    }

                    Column {
                        id: heading
                        x: glyph.width + 12
                        width: parent.width - x - 36
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 2

                        Ink {
                            width: parent.width
                            strong: true
                            font.pixelSize: Tokens.promptSize
                            wrapMode: Text.WordWrap
                            maximumLineCount: 3
                            elide: Text.ElideRight
                            text: root.card?.titulo || root.card?.say || ""
                        }

                        Ink {
                            width: parent.width
                            dim: true
                            font.pixelSize: Tokens.smallSize
                            wrapMode: Text.WordWrap
                            maximumLineCount: 2
                            elide: Text.ElideRight
                            text: root.card?.subtitulo || root.card?.said || ""
                            visible: text !== ""
                        }
                    }
                }

                // Cabeza de titular (y de portada, bajo la imagen): un
                // antetítulo en el acento y el título en grande.
                Column {
                    width: parent.width - (hero.visible ? 0 : 36)
                    spacing: 6
                    visible: root.style === "titular" || root.style === "portada"

                    Row {
                        spacing: 8

                        Icon {
                            anchors.verticalCenter: parent.verticalCenter
                            name: root.card?.icono || "sparkle"
                            color: root.tone
                            font.pixelSize: 17
                            opacity: root.breath
                        }

                        Ink {
                            anchors.verticalCenter: parent.verticalCenter
                            width: Math.min(implicitWidth, body.width - 80)
                            strong: true
                            font.pixelSize: Tokens.smallSize
                            color: root.tone
                            elide: Text.ElideRight
                            text: root.card?.subtitulo || root.card?.said || ""
                        }
                    }

                    Ink {
                        width: parent.width
                        wrapMode: Text.WordWrap
                        maximumLineCount: 3
                        elide: Text.ElideRight
                        lineHeight: 1.05
                        font.pixelSize: 31
                        font.weight: Font.Bold
                        font.variableAxes: ({ "wght": 700 })
                        text: root.card?.titulo || root.card?.say || ""
                    }
                }

                // Ancha, en dos columnas que se equilibran; estrecha, en una.
                Row {
                    id: split
                    width: parent.width
                    spacing: 18

                    Column {
                        id: first
                        width: root.wide ? (split.width - split.spacing) / 2 : split.width
                        spacing: 14

                        Repeater {
                            model: root.wide ? root.main.slice(0, root.cut) : root.main

                            Piece {
                                width: first.width
                            }
                        }
                    }

                    Column {
                        id: second
                        width: (split.width - split.spacing) / 2
                        spacing: 14
                        visible: root.wide

                        Repeater {
                            model: root.wide ? root.main.slice(root.cut) : []

                            Piece {
                                width: second.width
                                offset: root.cut
                            }
                        }
                    }
                }

                // De dónde sale, en pequeño, justo bajo lo que cuenta.
                Repeater {
                    model: root.sources

                    Piece {
                        width: body.width
                        offset: root.main.length
                    }
                }

                // Lo que se puede hacer después.
                Repeater {
                    model: root.actions

                    Piece {
                        width: body.width
                        offset: root.main.length + 1
                    }
                }

                // Seguir la conversación: al pie, tras una raya y con su
                // rótulo, para que no se confunda con los botones. Las
                // preguntas que propone el modelo y, debajo, un campo para
                // escribir la tuya; las dos van al cerebro con lo hablado.
                Column {
                    width: body.width
                    spacing: 6

                    Rectangle {
                        width: parent.width
                        height: 1
                        color: Tokens.border
                    }

                    Row {
                        topPadding: 6
                        spacing: 6

                        Icon {
                            anchors.verticalCenter: parent.verticalCenter
                            name: "chat-circle-dots"
                            color: root.tone
                            font.pixelSize: Tokens.smallSize + 1
                        }

                        Ink {
                            anchors.verticalCenter: parent.verticalCenter
                            dim: true
                            strong: true
                            font.pixelSize: Tokens.tinySize + 1
                            text: "Seguir preguntando"
                        }
                    }

                    Repeater {
                        model: root.asks

                        Piece {
                            width: body.width
                            offset: root.main.length + 2
                        }
                    }

                    Rectangle {
                        width: body.width
                        height: 38
                        radius: Tokens.radius
                        color: Tokens.surface

                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.IBeamCursor
                            onClicked: next.forceActiveFocus()
                        }

                        Icon {
                            id: nextMark
                            x: 8
                            anchors.verticalCenter: parent.verticalCenter
                            name: "arrow-bend-down-right"
                            color: next.activeFocus ? root.tone : root.soft(Tokens.dim, 0.6)
                            font.pixelSize: Tokens.smallSize + 2
                        }

                        TextInput {
                            id: next

                            function send(): void {
                                const asked = text.trim();
                                if (asked === "")
                                    return;
                                clear();
                                focus = false;
                                root.follow(asked);
                            }

                            anchors.left: nextMark.right
                            anchors.leftMargin: 10
                            anchors.right: go.left
                            anchors.rightMargin: 8
                            anchors.verticalCenter: parent.verticalCenter
                            clip: true
                            color: Tokens.text
                            selectionColor: Tokens.accent
                            selectedTextColor: Tokens.bg
                            font.family: Tokens.sans
                            font.pixelSize: Tokens.fontSize
                            Keys.onReturnPressed: send()
                            Keys.onEnterPressed: send()
                            Keys.onEscapePressed: {
                                clear();
                                focus = false;
                            }

                            Ink {
                                width: parent.width
                                dim: true
                                elide: Text.ElideRight
                                text: "Pregunta otra cosa…"
                                visible: next.text === ""
                            }
                        }

                        // Mandarla, también con Intro.
                        Rectangle {
                            id: go
                            anchors.right: parent.right
                            anchors.rightMargin: 5
                            anchors.verticalCenter: parent.verticalCenter
                            width: 28
                            height: 28
                            radius: 14
                            color: next.text.trim() !== "" ? root.tone : "transparent"

                            Behavior on color {
                                ColorAnimation { duration: Tokens.fast }
                            }

                            Icon {
                                anchors.centerIn: parent
                                name: "arrow-up"
                                font.pixelSize: 15
                                color: next.text.trim() !== "" ? Tokens.bg : root.soft(Tokens.dim, 0.5)
                            }

                            MouseArea {
                                anchors.fill: parent
                                cursorShape: next.text.trim() !== "" ? Qt.PointingHandCursor : Qt.ArrowCursor
                                onClicked: next.send()
                            }
                        }
                    }
                }

                // Una de la galería de ejemplos (state/Gallery.qml): cuál es y
                // flechas para pasar a la anterior o a la siguiente.
                Item {
                    width: body.width
                    height: 30
                    visible: (root.card?.pager ?? "") !== ""

                    Row {
                        anchors.centerIn: parent
                        spacing: 14

                        Repeater {
                            model: [{ way: -1, icon: "caret-left" }, { way: 0 }, { way: 1, icon: "caret-right" }]

                            Item {
                                id: page

                                required property var modelData

                                width: modelData.way === 0 ? counter.implicitWidth : 26
                                height: 26

                                Ink {
                                    id: counter
                                    anchors.centerIn: parent
                                    dim: true
                                    font.pixelSize: Tokens.tinySize + 1
                                    text: root.card?.pager ?? ""
                                    visible: page.modelData.way === 0
                                }

                                Rectangle {
                                    anchors.fill: parent
                                    radius: 13
                                    color: flip.containsMouse ? Tokens.surface : "transparent"
                                    visible: page.modelData.way !== 0

                                    Icon {
                                        anchors.centerIn: parent
                                        name: page.modelData.icon ?? ""
                                        color: flip.containsMouse ? root.tone : Tokens.dim
                                        font.pixelSize: 16
                                    }

                                    MouseArea {
                                        id: flip
                                        anchors.fill: parent
                                        hoverEnabled: true
                                        cursorShape: Qt.PointingHandCursor
                                        onClicked: Gallery.step(page.modelData.way)
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // El aspa, arriba a la derecha y por encima de todo (también de la portada).
            Rectangle {
                x: parent.width - width - 10
                y: 10
                width: 30
                height: 30
                radius: 15
                color: hero.visible ? root.soft(Tokens.bg, 0.55) : "transparent"

                Icon {
                    anchors.centerIn: parent
                    name: "x-circle"
                    color: closer.containsMouse ? Tokens.text : hero.visible ? root.soft(Tokens.text, 0.85) : Tokens.dim
                }

                MouseArea {
                    id: closer
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.done()
                }
            }
        }
    }
}
