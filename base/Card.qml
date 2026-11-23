import QtQuick
import Quickshell
import qs
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

    component Pill: Rectangle {
        id: pill

        property alias text: label.text
        property string icon: ""
        // Los botones, siempre del acento del tema: el color de la tarjeta es
        // para iconos y resaltes, no para cambiar la estética.
        property color tone: Tokens.accent
        property bool solid: false
        signal pressed

        width: row.implicitWidth + 26
        height: 32
        radius: height / 2
        color: solid ? (area.containsMouse ? Qt.lighter(tone, 1.15) : tone) : (area.containsMouse ? root.soft(tone, 0.28) : root.soft(tone, 0.16))

        Behavior on color {
            ColorAnimation { duration: Tokens.fast }
        }

        Row {
            id: row
            anchors.centerIn: parent
            spacing: 6

            Icon {
                anchors.verticalCenter: parent.verticalCenter
                name: pill.icon
                color: pill.solid ? Tokens.bg : pill.tone
                visible: pill.icon !== ""
            }

            Ink {
                id: label
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(implicitWidth, 300)
                elide: Text.ElideRight
                font.pixelSize: Tokens.smallSize
                strong: true
                color: pill.solid ? Tokens.bg : Tokens.text
            }
        }

        MouseArea {
            id: area
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: pill.pressed()
        }
    }

    // Un icono en su círculo del color de la pieza.
    component Badge: Rectangle {
        property string icon: ""
        property color tone: root.tone
        property int size: 30

        width: size
        height: size
        radius: size / 2
        color: root.soft(tone, 0.18)

        Icon {
            anchors.centerIn: parent
            name: parent.icon
            color: parent.tone
            font.pixelSize: Math.round(parent.size * 0.55)
        }
    }

    // Una imagen de la web, con esquinas redondeadas; sin alto hasta que carga
    // (si no carga, no está).
    component Picture: Clip {
        id: frame

        property alias source: art.source
        property int tall: 200
        readonly property bool ready: art.status === Image.Ready

        height: ready ? tall : 0
        visible: ready
        radius: Tokens.radius
        color: Tokens.surface

        Image {
            id: art
            anchors.fill: parent
            sourceSize.width: frame.width * 2
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            cache: true
        }
    }

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

            Ink {
                width: piece.width
                wrapMode: Text.WordWrap
                lineHeight: 1.2
                text: piece.modelData.texto
            }
        }

        Component {
            id: list

            Column {
                width: piece.width
                spacing: 10

                Repeater {
                    model: piece.modelData.items

                    Item {
                        id: entry

                        required property var modelData
                        readonly property color tone: root.tint(modelData.color, piece.tone)

                        width: piece.width
                        implicitHeight: Math.max(lead.height, words.implicitHeight)

                        Item {
                            id: lead
                            width: thumb.ready ? 48 : 30
                            height: thumb.ready ? 48 : 30
                            anchors.verticalCenter: parent.verticalCenter

                            Picture {
                                id: thumb
                                width: 48
                                tall: 48
                                radius: 8
                                source: entry.modelData.imagen
                            }

                            Badge {
                                visible: !thumb.ready
                                tone: entry.tone
                                icon: entry.modelData.icono || "arrow-right"
                            }
                        }

                        Column {
                            id: words
                            x: lead.width + 12
                            width: parent.width - x - value.width - 12
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 2

                            Ink {
                                width: parent.width
                                wrapMode: Text.WordWrap
                                text: entry.modelData.titulo
                            }

                            Ink {
                                width: parent.width
                                dim: true
                                font.pixelSize: Tokens.smallSize
                                wrapMode: Text.WordWrap
                                maximumLineCount: 3
                                elide: Text.ElideRight
                                text: entry.modelData.detalle
                                visible: text !== ""
                            }
                        }

                        Ink {
                            id: value
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            strong: true
                            color: entry.tone
                            text: entry.modelData.valor
                        }
                    }
                }
            }
        }

        Component {
            id: pairs

            Rectangle {
                width: piece.width
                implicitHeight: grid.implicitHeight + 20
                radius: Tokens.radius
                color: Tokens.surface

                Grid {
                    id: grid
                    x: 14
                    y: 10
                    width: parent.width - 28
                    columns: 2
                    columnSpacing: 16
                    rowSpacing: 8

                    Repeater {
                        // Sin flatMap: el JS de QML no lo tiene.
                        model: {
                            const cells = [];
                            for (const i of piece.modelData.items)
                                cells.push({ text: i.titulo, dim: true }, { text: i.valor, dim: false });
                            return cells;
                        }

                        Ink {
                            required property var modelData
                            width: (grid.width - 16) / 2
                            wrapMode: Text.WordWrap
                            dim: modelData.dim
                            strong: !modelData.dim
                            horizontalAlignment: modelData.dim ? Text.AlignLeft : Text.AlignRight
                            text: modelData.text
                        }
                    }
                }
            }
        }

        Component {
            id: figure

            Rectangle {
                width: piece.width
                implicitHeight: Math.max(84, figureText.implicitHeight + 28)
                radius: Tokens.radius
                color: Tokens.surface

                Badge {
                    id: sign
                    x: 14
                    anchors.verticalCenter: parent.verticalCenter
                    size: 40
                    tone: piece.tone
                    icon: piece.modelData.icono ?? ""
                    visible: icon !== ""
                }

                Column {
                    id: figureText
                    x: sign.visible ? sign.x + sign.width + 14 : 16
                    width: parent.width - x - 16
                    anchors.verticalCenter: parent.verticalCenter

                    // Grande si es corta; si es larga, menor y en dos líneas.
                    Ink {
                        width: parent.width
                        strong: true
                        font.pixelSize: piece.modelData.valor.length > 14 ? 24 : 36
                        wrapMode: Text.WordWrap
                        maximumLineCount: 2
                        elide: Text.ElideRight
                        color: piece.tone
                        text: piece.modelData.valor
                    }

                    Ink {
                        width: parent.width
                        dim: true
                        elide: Text.ElideRight
                        text: piece.modelData.texto
                        visible: text !== ""
                    }
                }
            }
        }

        Component {
            id: countdown

            Rectangle {
                id: clock

                property real now: Date.now()
                readonly property real at: Date.parse(piece.modelData.fecha)
                readonly property real remaining: (at - now) / 1000
                readonly property bool past: remaining <= 0
                readonly property var parts: {
                    const s = Math.abs(remaining);
                    return [[Math.floor(s / 86400), "días"], [Math.floor(s % 86400 / 3600), "horas"], [Math.floor(s % 3600 / 60), "min"], [Math.floor(s % 60), "s"]];
                }

                width: piece.width
                implicitHeight: units.height + caption.implicitHeight + 34
                radius: Tokens.radius
                color: Tokens.surface

                // Cuatro veces por segundo, para que los segundos no salten de dos en dos.
                Timer {
                    running: clock.visible
                    repeat: true
                    interval: 250
                    onTriggered: clock.now = Date.now()
                }

                Ink {
                    id: caption
                    x: 16
                    y: 12
                    width: parent.width - 32
                    dim: true
                    font.pixelSize: Tokens.smallSize
                    elide: Text.ElideRight
                    text: (piece.modelData.texto || "") + (clock.past ? " · hace" : " · faltan")
                }

                Row {
                    id: units
                    x: 16
                    y: caption.y + caption.height + 8
                    spacing: 10

                    Repeater {
                        model: clock.parts

                        Rectangle {
                            required property var modelData
                            width: (piece.width - 32 - 30) / 4
                            height: 58
                            radius: 8
                            color: Tokens.plate

                            Column {
                                anchors.centerIn: parent

                                Ink {
                                    anchors.horizontalCenter: parent.horizontalCenter
                                    strong: true
                                    font.pixelSize: Tokens.figureSize
                                    color: piece.tone
                                    text: modelData[0]
                                }

                                Ink {
                                    anchors.horizontalCenter: parent.horizontalCenter
                                    dim: true
                                    font.pixelSize: Tokens.tinySize
                                    text: modelData[1]
                                }
                            }
                        }
                    }
                }
            }
        }

        Component {
            id: meter

            Column {
                width: piece.width
                spacing: 6

                Item {
                    width: parent.width
                    implicitHeight: percent.implicitHeight

                    Ink {
                        width: parent.width - percent.width - 8
                        dim: true
                        elide: Text.ElideRight
                        text: piece.modelData.texto
                    }

                    Ink {
                        id: percent
                        anchors.right: parent.right
                        strong: true
                        color: piece.tone
                        text: Math.round(piece.modelData.valor * 100) + " %"
                    }
                }

                Rectangle {
                    width: parent.width
                    height: 6
                    radius: 3
                    color: Tokens.surface

                    Rectangle {
                        width: parent.width * piece.modelData.valor
                        height: parent.height
                        radius: 3
                        color: piece.tone
                    }
                }
            }
        }

        Component {
            id: bars

            Column {
                id: chart

                readonly property var points: piece.modelData.items
                readonly property real most: Math.max(1e-9, Math.max.apply(null, points.map(p => p.numero)))

                width: piece.width
                spacing: 8

                Ink {
                    width: parent.width
                    dim: true
                    font.pixelSize: Tokens.smallSize
                    elide: Text.ElideRight
                    text: piece.modelData.texto
                    visible: text !== ""
                }

                Row {
                    id: columns
                    width: parent.width
                    height: 160
                    spacing: 6

                    Repeater {
                        model: chart.points

                        Item {
                            id: column

                            required property var modelData
                            readonly property bool peak: modelData.numero === chart.most

                            width: (columns.width - columns.spacing * (chart.points.length - 1)) / chart.points.length
                            height: columns.height

                            Ink {
                                anchors.horizontalCenter: parent.horizontalCenter
                                anchors.bottom: bar.top
                                anchors.bottomMargin: 4
                                font.pixelSize: Tokens.tinySize
                                strong: column.peak
                                dim: !column.peak
                                text: root.figure(column.modelData.numero, piece.modelData.unidad)
                            }

                            Rectangle {
                                id: bar
                                anchors.bottom: label.top
                                anchors.bottomMargin: 6
                                anchors.horizontalCenter: parent.horizontalCenter
                                width: Math.min(parent.width, 40)
                                height: Math.max(3, (parent.height - label.height - 30) * Math.max(0, column.modelData.numero) / chart.most)
                                radius: 4
                                color: column.peak ? piece.tone : root.soft(piece.tone, 0.4)
                            }

                            Ink {
                                id: label
                                anchors.bottom: parent.bottom
                                width: parent.width
                                horizontalAlignment: Text.AlignHCenter
                                elide: Text.ElideRight
                                dim: true
                                font.pixelSize: Tokens.tinySize
                                text: column.modelData.titulo
                            }
                        }
                    }
                }
            }
        }

        Component {
            id: lines

            Column {
                id: trend

                readonly property var points: piece.modelData.items
                readonly property real high: Math.max.apply(null, points.map(p => p.numero))
                readonly property real low: Math.min.apply(null, points.map(p => p.numero))

                width: piece.width
                spacing: 6

                Ink {
                    width: parent.width
                    dim: true
                    font.pixelSize: Tokens.smallSize
                    elide: Text.ElideRight
                    text: piece.modelData.texto
                    visible: text !== ""
                }

                Item {
                    width: parent.width
                    height: 150

                    Canvas {
                        id: plot
                        anchors.fill: parent
                        anchors.topMargin: 18
                        anchors.bottomMargin: 18
                        onWidthChanged: requestPaint()
                        Component.onCompleted: requestPaint()
                        onPaint: {
                            const ctx = getContext("2d");
                            ctx.reset();
                            const points = trend.points, n = points.length, pad = 6;
                            const span = (trend.high - trend.low) || 1;
                            const xy = points.map((p, i) => [pad + (width - pad * 2) * i / (n - 1), pad + (height - pad * 2) * (1 - (p.numero - trend.low) / span)]);
                            const a = piece.tone;
                            ctx.beginPath();
                            xy.forEach((p, i) => i ? ctx.lineTo(p[0], p[1]) : ctx.moveTo(p[0], p[1]));
                            ctx.strokeStyle = a;
                            ctx.lineWidth = 2.5;
                            ctx.lineJoin = "round";
                            ctx.stroke();
                            const last = xy[n - 1];
                            ctx.beginPath();
                            ctx.arc(last[0], last[1], 4.5, 0, Math.PI * 2);
                            ctx.fillStyle = a;
                            ctx.fill();
                        }
                    }

                    Ink {
                        anchors.top: parent.top
                        dim: true
                        font.pixelSize: Tokens.tinySize
                        text: "máx. " + root.figure(trend.high, piece.modelData.unidad)
                    }

                    Ink {
                        anchors.bottom: parent.bottom
                        dim: true
                        font.pixelSize: Tokens.tinySize
                        text: "mín. " + root.figure(trend.low, piece.modelData.unidad)
                    }

                    Ink {
                        anchors.top: parent.top
                        anchors.right: parent.right
                        strong: true
                        color: piece.tone
                        font.pixelSize: Tokens.smallSize
                        text: root.figure(trend.points[trend.points.length - 1].numero, piece.modelData.unidad)
                    }
                }

                Item {
                    width: parent.width
                    implicitHeight: first.implicitHeight

                    Ink {
                        id: first
                        dim: true
                        font.pixelSize: Tokens.tinySize
                        text: trend.points[0].titulo
                    }

                    Ink {
                        anchors.horizontalCenter: parent.horizontalCenter
                        dim: true
                        font.pixelSize: Tokens.tinySize
                        text: trend.points.length > 2 ? trend.points[Math.floor(trend.points.length / 2)].titulo : ""
                    }

                    Ink {
                        anchors.right: parent.right
                        dim: true
                        font.pixelSize: Tokens.tinySize
                        text: trend.points[trend.points.length - 1].titulo
                    }
                }
            }
        }

        Component {
            id: ring

            Item {
                id: pie

                readonly property var points: piece.modelData.items
                readonly property real total: points.reduce((sum, p) => sum + p.numero, 0)

                function shade(i: int): color {
                    return root.soft(piece.tone, [1, 0.7, 0.5, 0.35, 0.24, 0.16][i % 6]);
                }

                width: piece.width
                implicitHeight: Math.max(150, legend.implicitHeight)

                Canvas {
                    id: donut
                    width: 150
                    height: 150
                    anchors.verticalCenter: parent.verticalCenter
                    Component.onCompleted: requestPaint()
                    onPaint: {
                        const ctx = getContext("2d");
                        ctx.reset();
                        let start = -Math.PI / 2;
                        pie.points.forEach((p, i) => {
                            const sweep = Math.PI * 2 * p.numero / pie.total;
                            ctx.beginPath();
                            ctx.arc(75, 75, 60, start + 0.025, start + sweep - 0.025);
                            ctx.strokeStyle = pie.shade(i);
                            ctx.lineWidth = 20;
                            ctx.stroke();
                            start += sweep;
                        });
                    }

                    Column {
                        anchors.centerIn: parent
                        width: 90

                        Ink {
                            width: parent.width
                            horizontalAlignment: Text.AlignHCenter
                            strong: true
                            font.pixelSize: Tokens.clockSize
                            elide: Text.ElideRight
                            text: root.figure(pie.total, piece.modelData.unidad)
                        }

                        Ink {
                            width: parent.width
                            horizontalAlignment: Text.AlignHCenter
                            dim: true
                            font.pixelSize: Tokens.tinySize
                            elide: Text.ElideRight
                            text: piece.modelData.texto || "total"
                        }
                    }
                }

                Column {
                    id: legend
                    x: donut.width + 20
                    width: parent.width - x
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 7

                    Repeater {
                        model: pie.points

                        Item {
                            id: slice

                            required property var modelData
                            required property int index

                            width: legend.width
                            implicitHeight: name.implicitHeight

                            Rectangle {
                                anchors.verticalCenter: parent.verticalCenter
                                width: 10
                                height: 10
                                radius: 3
                                color: pie.shade(slice.index)
                            }

                            Ink {
                                id: name
                                x: 18
                                width: parent.width - x - share.width - 8
                                elide: Text.ElideRight
                                font.pixelSize: Tokens.smallSize
                                text: slice.modelData.titulo
                            }

                            Ink {
                                id: share
                                anchors.right: parent.right
                                strong: true
                                font.pixelSize: Tokens.smallSize
                                text: Math.round(slice.modelData.numero / pie.total * 100) + " %"
                            }
                        }
                    }
                }
            }
        }

        Component {
            id: table

            Rectangle {
                id: sheet

                readonly property var heads: piece.modelData.columnas
                readonly property int wide: Math.max(heads.length, 1 + Math.max.apply(null, piece.modelData.items.map(r => r.valores.length)))

                width: piece.width
                implicitHeight: rows.implicitHeight + 8
                radius: Tokens.radius
                color: Tokens.surface

                Column {
                    id: rows
                    y: 4
                    width: parent.width

                    Item {
                        width: parent.width
                        height: 32
                        visible: sheet.heads.some(h => h !== "")

                        Row {
                            x: 14
                            width: parent.width - 28
                            anchors.verticalCenter: parent.verticalCenter

                            Repeater {
                                model: sheet.wide

                                Ink {
                                    required property int index
                                    width: parent.width / sheet.wide
                                    horizontalAlignment: index ? Text.AlignRight : Text.AlignLeft
                                    elide: Text.ElideRight
                                    strong: index > 0
                                    dim: true
                                    font.pixelSize: Tokens.smallSize
                                    text: sheet.heads[index] ?? ""
                                }
                            }
                        }
                    }

                    Repeater {
                        model: piece.modelData.items

                        Rectangle {
                            id: line

                            required property var modelData
                            required property int index

                            width: rows.width
                            height: Math.max(34, cells.implicitHeight + 14)
                            color: index % 2 ? "transparent" : root.soft(Tokens.text, 0.035)

                            Row {
                                id: cells
                                x: 14
                                width: parent.width - 28
                                anchors.verticalCenter: parent.verticalCenter

                                Ink {
                                    width: cells.width / sheet.wide
                                    dim: true
                                    wrapMode: Text.WordWrap
                                    text: line.modelData.titulo
                                }

                                Repeater {
                                    model: sheet.wide - 1

                                    Ink {
                                        required property int index
                                        readonly property bool best: line.modelData.mejor === index

                                        width: cells.width / sheet.wide
                                        horizontalAlignment: Text.AlignRight
                                        wrapMode: Text.WordWrap
                                        strong: best
                                        color: best ? piece.tone : Tokens.text
                                        text: line.modelData.valores[index] ?? ""
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Component {
            id: proscons

            Row {
                id: sides

                readonly property bool both: piece.modelData.pros.length > 0 && piece.modelData.contras.length > 0

                width: piece.width
                spacing: 10

                Repeater {
                    model: [
                        { head: "A favor", icon: "check-circle", tone: Tokens.done, list: piece.modelData.pros },
                        { head: "En contra", icon: "x-circle", tone: Tokens.critical, list: piece.modelData.contras }
                    ]

                    Rectangle {
                        id: side

                        required property var modelData

                        width: sides.both ? (sides.width - sides.spacing) / 2 : sides.width
                        implicitHeight: points.implicitHeight + 24
                        radius: Tokens.radius
                        color: Tokens.surface
                        visible: modelData.list.length > 0

                        Column {
                            id: points
                            x: 12
                            y: 12
                            width: parent.width - 24
                            spacing: 8

                            Ink {
                                dim: true
                                strong: true
                                font.pixelSize: Tokens.smallSize
                                text: side.modelData.head
                            }

                            Repeater {
                                model: side.modelData.list

                                Item {
                                    required property string modelData

                                    width: points.width
                                    implicitHeight: Math.max(18, said.implicitHeight)

                                    Icon {
                                        name: side.modelData.icon
                                        color: side.modelData.tone
                                        font.pixelSize: 16
                                    }

                                    Ink {
                                        id: said
                                        x: 24
                                        width: parent.width - x
                                        wrapMode: Text.WordWrap
                                        font.pixelSize: Tokens.smallSize
                                        text: parent.modelData
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Component {
            id: tiles

            Grid {
                id: mosaic
                width: piece.width
                columns: piece.modelData.items.length === 4 ? 2 : Math.min(3, piece.modelData.items.length)
                spacing: 8

                Repeater {
                    model: piece.modelData.items

                    Rectangle {
                        id: tile

                        required property var modelData
                        required property int index
                        readonly property color tone: piece.tone
                        readonly property var serie: modelData.serie ?? []

                        width: (mosaic.width - mosaic.spacing * (mosaic.columns - 1)) / mosaic.columns
                        implicitHeight: stat.implicitHeight + 24 + (spark.visible ? spark.height + 6 : 0)
                        radius: Tokens.radius
                        color: Tokens.surface

                        // Un filo del tono arriba.
                        Rectangle {
                            x: 12
                            width: 22
                            height: 3
                            radius: 2
                            color: tile.tone
                        }

                        Canvas {
                            id: spark
                            x: 12
                            anchors.bottom: parent.bottom
                            anchors.bottomMargin: 10
                            width: parent.width - 24
                            height: 30
                            visible: tile.serie.length >= 2
                            onWidthChanged: requestPaint()
                            Component.onCompleted: requestPaint()
                            onPaint: {
                                const ctx = getContext("2d");
                                ctx.reset();
                                const n = tile.serie, hi = Math.max.apply(null, n), lo = Math.min.apply(null, n), span = (hi - lo) || 1;
                                const xy = n.map((v, i) => [width * i / (n.length - 1), 3 + (height - 6) * (1 - (v - lo) / span)]);
                                const c = tile.tone;
                                ctx.beginPath();
                                xy.forEach((p, i) => i ? ctx.lineTo(p[0], p[1]) : ctx.moveTo(p[0], p[1]));
                                ctx.strokeStyle = c;
                                ctx.lineWidth = 2;
                                ctx.lineJoin = "round";
                                ctx.stroke();
                            }
                        }

                        Column {
                            id: stat
                            x: 12
                            y: 12
                            width: parent.width - 24
                            spacing: 3

                            Row {
                                spacing: 6

                                Icon {
                                    anchors.verticalCenter: parent.verticalCenter
                                    name: tile.modelData.icono
                                    color: tile.tone
                                    font.pixelSize: 15
                                    visible: name !== ""
                                }

                                Ink {
                                    width: stat.width - 21
                                    dim: true
                                    font.pixelSize: Tokens.smallSize
                                    elide: Text.ElideRight
                                    text: tile.modelData.titulo
                                }
                            }

                            Ink {
                                width: parent.width
                                strong: true
                                font.pixelSize: Tokens.figureSize
                                elide: Text.ElideRight
                                text: tile.modelData.valor
                            }

                            Ink {
                                width: parent.width
                                font.pixelSize: Tokens.smallSize
                                elide: Text.ElideRight
                                color: root.tint(tile.modelData.color, Tokens.dim)
                                text: tile.modelData.detalle
                                visible: text !== ""
                            }
                        }
                    }
                }
            }
        }

        Component {
            id: strip

            Flickable {
                id: rail

                readonly property int shown: Math.min(piece.modelData.items.length, 6)

                width: piece.width
                height: 112
                contentWidth: band.width
                flickableDirection: Flickable.HorizontalFlick
                interactive: contentWidth > width
                clip: true

                Row {
                    id: band
                    spacing: 8

                    Repeater {
                        model: piece.modelData.items

                        Rectangle {
                            id: slot

                            required property var modelData
                            required property int index

                            width: Math.max(76, (rail.width - band.spacing * (rail.shown - 1)) / rail.shown)
                            height: rail.height
                            radius: Tokens.radius
                            color: index === 0 ? root.soft(piece.tone, 0.14) : Tokens.surface

                            Column {
                                anchors.centerIn: parent
                                width: parent.width - 12
                                spacing: 6

                                Ink {
                                    width: parent.width
                                    horizontalAlignment: Text.AlignHCenter
                                    elide: Text.ElideRight
                                    dim: true
                                    font.pixelSize: Tokens.smallSize
                                    text: slot.modelData.titulo
                                }

                                Icon {
                                    anchors.horizontalCenter: parent.horizontalCenter
                                    name: slot.modelData.icono
                                    color: piece.tone
                                    font.pixelSize: 24
                                    visible: name !== ""
                                }

                                Ink {
                                    width: parent.width
                                    horizontalAlignment: Text.AlignHCenter
                                    elide: Text.ElideRight
                                    strong: true
                                    text: slot.modelData.valor
                                }

                                Ink {
                                    width: parent.width
                                    horizontalAlignment: Text.AlignHCenter
                                    elide: Text.ElideRight
                                    dim: true
                                    font.pixelSize: Tokens.tinySize
                                    text: slot.modelData.detalle
                                    visible: text !== ""
                                }
                            }
                        }
                    }
                }
            }
        }

        Component {
            id: steps

            Column {
                width: piece.width

                Repeater {
                    model: piece.modelData.items

                    Item {
                        id: step

                        required property var modelData
                        required property int index
                        readonly property bool last: index === piece.modelData.items.length - 1

                        width: piece.width
                        implicitHeight: Math.max(28, words.implicitHeight) + (last ? 0 : 14)

                        Rectangle {
                            x: 13
                            y: 30
                            width: 2
                            height: parent.height - 32
                            color: Tokens.border
                            visible: !step.last
                        }

                        Rectangle {
                            width: 28
                            height: 28
                            radius: 14
                            color: root.soft(piece.tone, 0.16)

                            Ink {
                                anchors.centerIn: parent
                                strong: true
                                color: piece.tone
                                font.pixelSize: Tokens.smallSize
                                text: step.index + 1
                            }
                        }

                        Column {
                            id: words
                            x: 40
                            width: parent.width - x
                            topPadding: 4
                            spacing: 2

                            Ink {
                                width: parent.width
                                wrapMode: Text.WordWrap
                                text: step.modelData.titulo
                            }

                            Ink {
                                width: parent.width
                                dim: true
                                wrapMode: Text.WordWrap
                                font.pixelSize: Tokens.smallSize
                                text: step.modelData.detalle
                                visible: text !== ""
                            }
                        }
                    }
                }
            }
        }

        Component {
            id: timeline

            Column {
                width: piece.width

                Repeater {
                    model: piece.modelData.items

                    Item {
                        id: moment

                        required property var modelData
                        required property int index
                        readonly property bool last: index === piece.modelData.items.length - 1

                        width: piece.width
                        implicitHeight: words.implicitHeight + (last ? 0 : 14)

                        Rectangle {
                            x: 5
                            y: 6
                            width: 2
                            height: parent.height
                            color: Tokens.border
                            visible: !moment.last
                        }

                        Rectangle {
                            y: 4
                            width: 12
                            height: 12
                            radius: 6
                            color: moment.last ? piece.tone : Tokens.plate
                            border.color: piece.tone
                            border.width: 2
                        }

                        Column {
                            id: words
                            x: 26
                            width: parent.width - x
                            spacing: 2

                            Ink {
                                strong: true
                                color: piece.tone
                                font.pixelSize: Tokens.smallSize
                                text: moment.modelData.valor
                                visible: text !== ""
                            }

                            Ink {
                                width: parent.width
                                wrapMode: Text.WordWrap
                                text: moment.modelData.titulo
                            }

                            Ink {
                                width: parent.width
                                dim: true
                                wrapMode: Text.WordWrap
                                font.pixelSize: Tokens.smallSize
                                text: moment.modelData.detalle
                                visible: text !== ""
                            }
                        }
                    }
                }
            }
        }

        Component {
            id: rating

            Row {
                spacing: 14

                Ink {
                    anchors.verticalCenter: parent.verticalCenter
                    strong: true
                    font.pixelSize: 32
                    color: piece.tone
                    text: Number(piece.modelData.valor).toLocaleString(Qt.locale(), "f", 1)
                }

                Column {
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 2

                    Row {
                        spacing: 3

                        Repeater {
                            model: 5

                            Item {
                                id: star

                                required property int index

                                width: 20
                                height: 22

                                Text {
                                    text: "★"
                                    color: Tokens.border
                                    font.pixelSize: 20
                                }

                                Item {
                                    width: star.width * Math.max(0, Math.min(1, piece.modelData.valor - star.index))
                                    height: star.height
                                    clip: true

                                    Text {
                                        text: "★"
                                        color: piece.tone
                                        font.pixelSize: 20
                                    }
                                }
                            }
                        }
                    }

                    Ink {
                        dim: true
                        font.pixelSize: Tokens.smallSize
                        text: piece.modelData.texto
                        visible: text !== ""
                    }
                }
            }
        }

        Component {
            id: quote

            Item {
                width: piece.width
                implicitHeight: words.implicitHeight + 8

                Ink {
                    y: -14
                    strong: true
                    font.pixelSize: 76
                    color: root.soft(piece.tone, 0.45)
                    text: "“"
                }

                Column {
                    id: words
                    x: 42
                    y: 8
                    width: parent.width - x
                    spacing: 6

                    Ink {
                        width: parent.width
                        wrapMode: Text.WordWrap
                        lineHeight: 1.2
                        font.italic: true
                        font.pixelSize: Tokens.clockSize
                        text: "«" + piece.modelData.texto + "»"
                    }

                    Ink {
                        dim: true
                        font.pixelSize: Tokens.smallSize
                        text: "— " + piece.modelData.valor
                        visible: piece.modelData.valor !== ""
                    }
                }
            }
        }

        Component {
            id: code

            Rectangle {
                width: piece.width
                implicitHeight: listing.y + listing.implicitHeight + 14
                radius: Tokens.radius
                color: Tokens.surface

                Ink {
                    id: language
                    x: 14
                    y: 10
                    dim: true
                    strong: true
                    font.pixelSize: Tokens.tinySize
                    text: piece.modelData.valor
                    visible: text !== ""
                }

                Text {
                    id: listing
                    x: 14
                    y: language.visible ? 30 : 14
                    width: parent.width - 28
                    wrapMode: Text.WrapAnywhere
                    textFormat: Text.PlainText
                    font.family: "JetBrains Mono"
                    font.pixelSize: 12
                    color: Tokens.text
                    text: piece.modelData.texto
                }
            }
        }

        Component {
            id: gallery

            Row {
                id: shelf
                width: piece.width
                spacing: 8

                Repeater {
                    model: piece.modelData.items

                    Item {
                        id: shot

                        required property var modelData

                        width: (shelf.width - shelf.spacing * (piece.modelData.items.length - 1)) / piece.modelData.items.length
                        height: 120

                        Clip {
                            anchors.fill: parent
                            radius: Tokens.radius
                            color: Tokens.surface

                            Image {
                                anchors.fill: parent
                                source: shot.modelData.imagen
                                sourceSize.width: width * 2
                                fillMode: Image.PreserveAspectCrop
                                asynchronous: true
                            }
                        }

                        MouseArea {
                            anchors.fill: parent
                            enabled: shot.modelData.url !== ""
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                Qt.openUrlExternally(shot.modelData.url);
                                root.done();
                            }
                        }
                    }
                }
            }
        }

        Component {
            id: tags

            Flow {
                width: piece.width
                spacing: 8

                Repeater {
                    model: piece.modelData.items

                    Rectangle {
                        id: chip

                        required property var modelData
                        readonly property color tone: root.tint(modelData.color, piece.tone)

                        width: chipRow.implicitWidth + 22
                        height: 28
                        radius: height / 2
                        color: root.soft(tone, 0.16)
                        border.color: root.soft(tone, 0.35)
                        border.width: 1

                        Row {
                            id: chipRow
                            anchors.centerIn: parent
                            spacing: 5

                            Icon {
                                anchors.verticalCenter: parent.verticalCenter
                                name: chip.modelData.icono
                                color: chip.tone
                                font.pixelSize: 15
                                visible: name !== ""
                            }

                            Ink {
                                anchors.verticalCenter: parent.verticalCenter
                                font.pixelSize: Tokens.smallSize
                                color: chip.tone
                                strong: true
                                text: chip.modelData.titulo
                            }
                        }
                    }
                }
            }
        }

        Component {
            id: photo

            Column {
                width: piece.width
                spacing: 6

                Picture {
                    id: shot
                    width: parent.width
                    tall: 220
                    source: piece.modelData.url
                }

                Ink {
                    width: parent.width
                    dim: true
                    font.pixelSize: Tokens.smallSize
                    wrapMode: Text.WordWrap
                    text: piece.modelData.texto
                    visible: shot.ready && text !== ""
                }
            }
        }

        Component {
            id: actions

            Flow {
                width: piece.width
                spacing: 8

                Repeater {
                    model: piece.modelData.items

                    Pill {
                        required property var modelData
                        required property int index
                        text: modelData.titulo
                        icon: "lightning"
                        solid: index === 0
                        onPressed: {
                            Daemon.say(modelData.orden, "card", false);
                            root.done();
                        }
                    }
                }
            }
        }

        // De dónde sale: enlaces pequeños, solo subrayados, tras «Fuentes».
        Component {
            id: links

            Flow {
                width: piece.width
                spacing: 14

                Ink {
                    dim: true
                    font.pixelSize: Tokens.tinySize + 1
                    text: "Fuentes"
                }

                Repeater {
                    model: piece.modelData.items

                    Ink {
                        required property var modelData

                        width: Math.min(implicitWidth, 260)
                        elide: Text.ElideRight
                        font.pixelSize: Tokens.tinySize + 1
                        font.underline: true
                        color: source.containsMouse ? Tokens.text : Tokens.dim
                        text: modelData.titulo || modelData.url.replace(/^https?:\/\/(www\.)?/, "").split("/")[0]

                        MouseArea {
                            id: source
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                Qt.openUrlExternally(parent.modelData.url);
                                root.done();
                            }
                        }
                    }
                }
            }
        }

        // El tiempo: ahora, en grande, y una fila por día u hora con su
        // tramo de temperatura dentro del de toda la previsión.
        Component {
            id: forecast

            Column {
                id: sky

                readonly property var days: piece.modelData.items
                readonly property real low: Math.min.apply(null, days.map(d => d.min))
                readonly property real high: Math.max.apply(null, days.map(d => d.max))

                width: piece.width
                spacing: 2

                Item {
                    width: parent.width
                    implicitHeight: now.implicitHeight + 10
                    visible: piece.modelData.valor !== ""

                    Row {
                        id: now
                        spacing: 14

                        Icon {
                            anchors.verticalCenter: parent.verticalCenter
                            name: piece.modelData.icono || (sky.days[0]?.icono ?? "")
                            color: piece.tone
                            font.pixelSize: 46
                            visible: name !== ""
                        }

                        Ink {
                            anchors.verticalCenter: parent.verticalCenter
                            font.pixelSize: 56
                            font.weight: Font.Light
                            font.variableAxes: ({ "wght": 300 })
                            text: piece.modelData.valor
                        }

                        Ink {
                            anchors.verticalCenter: parent.verticalCenter
                            width: Math.max(0, Math.min(implicitWidth, sky.width - 220))
                            dim: true
                            wrapMode: Text.WordWrap
                            maximumLineCount: 2
                            elide: Text.ElideRight
                            text: piece.modelData.texto
                        }
                    }
                }

                Repeater {
                    model: sky.days

                    Item {
                        id: day

                        required property var modelData
                        required property int index

                        width: sky.width
                        height: 34

                        Rectangle {
                            width: parent.width
                            height: 1
                            color: Tokens.border
                            visible: day.index > 0
                        }

                        Ink {
                            id: dayName
                            width: 64
                            anchors.verticalCenter: parent.verticalCenter
                            elide: Text.ElideRight
                            strong: day.index === 0
                            text: day.modelData.titulo
                        }

                        Icon {
                            id: weather
                            x: dayName.width + 4
                            anchors.verticalCenter: parent.verticalCenter
                            name: day.modelData.icono
                            color: piece.tone
                        }

                        Ink {
                            id: rain
                            x: weather.x + 28
                            width: 42
                            anchors.verticalCenter: parent.verticalCenter
                            font.pixelSize: Tokens.tinySize
                            strong: true
                            color: Tokens.focus
                            elide: Text.ElideRight
                            text: day.modelData.detalle
                        }

                        Ink {
                            id: cold
                            x: rain.x + rain.width
                            width: 38
                            anchors.verticalCenter: parent.verticalCenter
                            horizontalAlignment: Text.AlignRight
                            dim: true
                            text: Math.round(day.modelData.min) + "°"
                        }

                        Rectangle {
                            id: track

                            readonly property real range: Math.max(1e-9, sky.high - sky.low)

                            x: cold.x + cold.width + 12
                            width: warm.x - x - 12
                            height: 6
                            radius: 3
                            anchors.verticalCenter: parent.verticalCenter
                            color: Tokens.surface

                            Rectangle {
                                x: track.width * (day.modelData.min - sky.low) / track.range
                                width: Math.max(track.height, track.width * (day.modelData.max - day.modelData.min) / track.range)
                                height: track.height
                                radius: track.radius
                                color: piece.tone
                            }
                        }

                        Ink {
                            id: warm
                            anchors.right: parent.right
                            width: 38
                            anchors.verticalCenter: parent.verticalCenter
                            horizontalAlignment: Text.AlignRight
                            strong: true
                            text: Math.round(day.modelData.max) + "°"
                        }
                    }
                }
            }
        }

        // Un resultado: los dos lados con su escudo (o su inicial) y el
        // marcador en grande; el que va ganando, claro, y el otro, apagado.
        Component {
            id: score

            Rectangle {
                id: board

                readonly property var sides: piece.modelData.items
                readonly property var goals: sides.map(side => parseFloat(String(side.valor).replace(",", ".")))
                readonly property int leader: goals[0] > goals[1] ? 0 : goals[1] > goals[0] ? 1 : -1

                width: piece.width
                implicitHeight: 168
                radius: Tokens.radius
                color: Tokens.surface

                Ink {
                    id: phase
                    anchors.horizontalCenter: parent.horizontalCenter
                    y: 14
                    strong: true
                    font.pixelSize: Tokens.smallSize
                    color: piece.tone
                    text: piece.modelData.texto
                    visible: text !== ""
                }

                Repeater {
                    model: board.sides

                    Column {
                        id: side

                        required property var modelData
                        required property int index

                        x: index === 0 ? 14 : board.width - width - 14
                        y: 40
                        width: (board.width - 150) / 2
                        spacing: 8

                        Item {
                            anchors.horizontalCenter: parent.horizontalCenter
                            width: 56
                            height: 56

                            Rectangle {
                                anchors.fill: parent
                                radius: width / 2
                                color: root.soft(piece.tone, 0.16)
                                border.color: root.soft(piece.tone, 0.45)
                                border.width: 1
                                visible: !crest.ready

                                Ink {
                                    anchors.centerIn: parent
                                    strong: true
                                    font.pixelSize: 22
                                    color: piece.tone
                                    text: side.modelData.titulo.charAt(0).toUpperCase()
                                }
                            }

                            Image {
                                id: crest

                                readonly property bool ready: status === Image.Ready

                                anchors.fill: parent
                                source: side.modelData.imagen
                                sourceSize.width: 112
                                fillMode: Image.PreserveAspectFit
                                asynchronous: true
                                visible: ready
                            }
                        }

                        Ink {
                            width: parent.width
                            horizontalAlignment: Text.AlignHCenter
                            wrapMode: Text.WordWrap
                            maximumLineCount: 2
                            elide: Text.ElideRight
                            strong: board.leader === side.index
                            text: side.modelData.titulo
                        }

                        Ink {
                            width: parent.width
                            horizontalAlignment: Text.AlignHCenter
                            dim: true
                            font.pixelSize: Tokens.tinySize
                            elide: Text.ElideRight
                            text: side.modelData.detalle
                            visible: text !== ""
                        }
                    }
                }

                Row {
                    anchors.horizontalCenter: parent.horizontalCenter
                    y: 52
                    spacing: 12

                    Ink {
                        strong: true
                        font.pixelSize: 46
                        color: board.leader === 1 ? Tokens.dim : Tokens.text
                        text: board.sides[0].valor
                    }

                    Ink {
                        anchors.verticalCenter: parent.verticalCenter
                        dim: true
                        font.pixelSize: 28
                        text: "–"
                    }

                    Ink {
                        strong: true
                        font.pixelSize: 46
                        color: board.leader === 0 ? Tokens.dim : Tokens.text
                        text: board.sides[1].valor
                    }
                }
            }
        }

        // Dos frente a frente: uno a cada lado (el primero en el acento) y,
        // por cada cosa que se compara, dos barras que salen del centro.
        Component {
            id: duel

            Column {
                id: arena

                readonly property var names: piece.modelData.columnas
                readonly property var pictures: piece.modelData.imagenes ?? ["", ""]
                readonly property var tones: [piece.tone, Tokens.text]
                readonly property real edge: 104

                width: piece.width
                spacing: 12

                Item {
                    width: parent.width
                    height: 52

                    Repeater {
                        model: 2

                        Row {
                            id: contender

                            required property int index

                            x: index === 0 ? 0 : arena.width - width
                            anchors.verticalCenter: parent.verticalCenter
                            layoutDirection: index === 0 ? Qt.LeftToRight : Qt.RightToLeft
                            spacing: 10

                            Item {
                                width: 44
                                height: 44
                                anchors.verticalCenter: parent.verticalCenter

                                Rectangle {
                                    anchors.fill: parent
                                    radius: width / 2
                                    color: root.soft(arena.tones[contender.index], 0.2)
                                    border.color: arena.tones[contender.index]
                                    border.width: 1.5

                                    Ink {
                                        anchors.centerIn: parent
                                        strong: true
                                        color: arena.tones[contender.index]
                                        text: arena.names[contender.index].charAt(0).toUpperCase()
                                        visible: !face.ready
                                    }
                                }

                                Picture {
                                    id: face
                                    anchors.fill: parent
                                    tall: 44
                                    radius: 22
                                    source: arena.pictures[contender.index] ?? ""
                                }
                            }

                            Ink {
                                anchors.verticalCenter: parent.verticalCenter
                                width: Math.min(implicitWidth, arena.width / 2 - 80)
                                strong: true
                                elide: Text.ElideRight
                                color: arena.tones[contender.index]
                                text: arena.names[contender.index]
                            }
                        }
                    }

                    Ink {
                        anchors.centerIn: parent
                        dim: true
                        strong: true
                        font.pixelSize: Tokens.smallSize
                        text: "vs"
                    }
                }

                Repeater {
                    model: piece.modelData.items

                    Column {
                        id: bout

                        required property var modelData
                        readonly property var numbers: modelData.numeros ?? []
                        readonly property real most: numbers.length === 2 ? Math.max(Math.abs(numbers[0]), Math.abs(numbers[1]), 1e-9) : 1
                        // Sin decir quién gana, gana el mayor.
                        readonly property int best: modelData.mejor >= 0 ? modelData.mejor : numbers.length === 2 && numbers[0] !== numbers[1] ? (numbers[0] > numbers[1] ? 0 : 1) : -1
                        property real grow: 0
                        NumberAnimation on grow { to: 1; duration: 800; easing.type: Easing.OutCubic }

                        width: arena.width
                        spacing: 4

                        Ink {
                            anchors.horizontalCenter: parent.horizontalCenter
                            dim: true
                            font.pixelSize: Tokens.smallSize
                            text: bout.modelData.titulo
                        }

                        Item {
                            width: parent.width
                            height: Math.max(22, before.implicitHeight)

                            Ink {
                                id: before
                                width: arena.edge
                                anchors.verticalCenter: parent.verticalCenter
                                wrapMode: Text.WordWrap
                                maximumLineCount: 2
                                elide: Text.ElideRight
                                strong: bout.best === 0
                                color: bout.best === 0 ? arena.tones[0] : Tokens.text
                                text: bout.modelData.valores[0]
                            }

                            Rectangle {
                                anchors.right: parent.horizontalCenter
                                anchors.rightMargin: 2
                                anchors.verticalCenter: parent.verticalCenter
                                width: (parent.width / 2 - arena.edge - 10) * Math.abs(bout.numbers[0] ?? 0) / bout.most * bout.grow
                                height: 8
                                radius: 4
                                color: bout.best === 1 ? root.soft(arena.tones[0], 0.35) : arena.tones[0]
                                visible: bout.numbers.length === 2
                            }

                            Rectangle {
                                anchors.left: parent.horizontalCenter
                                anchors.leftMargin: 2
                                anchors.verticalCenter: parent.verticalCenter
                                width: (parent.width / 2 - arena.edge - 10) * Math.abs(bout.numbers[1] ?? 0) / bout.most * bout.grow
                                height: 8
                                radius: 4
                                color: bout.best === 0 ? root.soft(arena.tones[1], 0.35) : arena.tones[1]
                                visible: bout.numbers.length === 2
                            }

                            Ink {
                                anchors.right: parent.right
                                width: arena.edge
                                anchors.verticalCenter: parent.verticalCenter
                                horizontalAlignment: Text.AlignRight
                                wrapMode: Text.WordWrap
                                maximumLineCount: 2
                                elide: Text.ElideRight
                                strong: bout.best === 1
                                color: bout.best === 1 ? arena.tones[1] : Tokens.text
                                text: bout.modelData.valores[1]
                            }
                        }
                    }
                }
            }
        }

        // Anillos de progreso, uno por cifra; se llenan al salir.
        Component {
            id: gauges

            Row {
                id: dials

                readonly property int count: piece.modelData.items.length
                readonly property real each: (width - spacing * (count - 1)) / count
                readonly property real size: Math.min(count === 1 ? 150 : 124, each)

                width: piece.width
                spacing: 12

                Repeater {
                    model: piece.modelData.items

                    Column {
                        id: dial

                        required property var modelData
                        required property int index
                        readonly property color tone: root.tint(modelData.color, piece.tone)
                        readonly property real share: Math.max(0, Math.min(1, modelData.numero / modelData.maximo))
                        property real shown: 0
                        NumberAnimation on shown { to: dial.share; duration: 1000; easing.type: Easing.OutCubic }

                        width: dials.each
                        spacing: 6

                        Item {
                            anchors.horizontalCenter: parent.horizontalCenter
                            width: dials.size
                            height: dials.size

                            Canvas {
                                id: arc
                                anchors.fill: parent
                                onPaint: {
                                    const ctx = getContext("2d");
                                    ctx.reset();
                                    const r = width / 2, line = Math.max(8, width * 0.1), from = Math.PI * 0.75, sweep = Math.PI * 1.5;
                                    const t = dial.tone;
                                    ctx.lineCap = "round";
                                    ctx.lineWidth = line;
                                    ctx.beginPath();
                                    ctx.arc(r, r, r - line / 2 - 1, from, from + sweep);
                                    ctx.strokeStyle = Qt.rgba(t.r, t.g, t.b, 0.16);
                                    ctx.stroke();
                                    if (dial.shown <= 0)
                                        return;
                                    ctx.beginPath();
                                    ctx.arc(r, r, r - line / 2 - 1, from, from + sweep * dial.shown);
                                    ctx.strokeStyle = t;
                                    ctx.stroke();
                                }

                                Connections {
                                    target: dial
                                    function onShownChanged(): void {
                                        arc.requestPaint();
                                    }
                                }
                            }

                            Column {
                                anchors.centerIn: parent
                                width: parent.width * 0.7

                                Ink {
                                    width: parent.width
                                    horizontalAlignment: Text.AlignHCenter
                                    strong: true
                                    font.pixelSize: Math.round(dials.size * (text.length > 5 ? 0.17 : 0.23))
                                    elide: Text.ElideRight
                                    text: dial.modelData.valor || (dial.modelData.maximo === 100 ? Math.round(dial.modelData.numero) + " %" : root.figure(dial.modelData.numero, ""))
                                }

                                Ink {
                                    width: parent.width
                                    horizontalAlignment: Text.AlignHCenter
                                    dim: true
                                    font.pixelSize: Tokens.tinySize
                                    text: dial.modelData.maximo !== 100 && dial.modelData.maximo !== 1 ? "de " + root.figure(dial.modelData.maximo, "") : ""
                                    visible: text !== ""
                                }
                            }
                        }

                        Ink {
                            width: parent.width
                            horizontalAlignment: Text.AlignHCenter
                            wrapMode: Text.WordWrap
                            maximumLineCount: 2
                            elide: Text.ElideRight
                            font.pixelSize: Tokens.smallSize
                            text: dial.modelData.titulo
                            visible: text !== ""
                        }
                    }
                }
            }
        }

        // Barras horizontales con nombre.
        Component {
            id: progress

            Column {
                width: piece.width
                spacing: 12

                Repeater {
                    model: piece.modelData.items

                    Column {
                        id: lane

                        required property var modelData
                        required property int index
                        readonly property color tone: root.tint(modelData.color, piece.tone)
                        readonly property real share: Math.max(0, Math.min(1, modelData.numero / modelData.maximo))
                        property real grow: 0
                        NumberAnimation on grow { to: 1; duration: 800; easing.type: Easing.OutCubic }

                        width: piece.width
                        spacing: 6

                        Item {
                            width: parent.width
                            implicitHeight: label.implicitHeight

                            Rectangle {
                                anchors.verticalCenter: parent.verticalCenter
                                width: 8
                                height: 8
                                radius: 4
                                color: lane.tone
                            }

                            Ink {
                                id: label
                                x: 16
                                width: parent.width - x - amount.width - 10
                                elide: Text.ElideRight
                                text: lane.modelData.titulo
                            }

                            Ink {
                                id: amount
                                anchors.right: parent.right
                                strong: true
                                text: lane.modelData.valor || (lane.modelData.maximo === 100 ? Math.round(lane.modelData.numero) + " %" : root.figure(lane.modelData.numero, ""))
                            }
                        }

                        Rectangle {
                            width: parent.width
                            height: 8
                            radius: 4
                            color: root.soft(lane.tone, 0.14)

                            Rectangle {
                                width: Math.max(height, parent.width * lane.share * lane.grow)
                                height: parent.height
                                radius: parent.radius
                                color: lane.tone
                            }
                        }
                    }
                }
            }
        }

        // Casillas que se marcan al pulsarlas (los ingredientes, qué llevar).
        Component {
            id: checks

            Column {
                id: sheet

                property int ticked: 0

                width: piece.width
                spacing: 2

                Ink {
                    anchors.right: parent.right
                    dim: true
                    font.pixelSize: Tokens.tinySize
                    text: `${sheet.ticked} de ${piece.modelData.items.length}`
                }

                Repeater {
                    model: piece.modelData.items

                    Rectangle {
                        id: entry

                        required property var modelData
                        property bool done: false
                        onDoneChanged: {
                            sheet.ticked += done ? 1 : -1;
                            tick.requestPaint();
                        }

                        width: sheet.width
                        height: Math.max(36, words.implicitHeight + 14)
                        radius: Tokens.radius
                        color: tap.containsMouse ? Tokens.surface : "transparent"

                        Behavior on color {
                            ColorAnimation { duration: Tokens.fast }
                        }

                        Rectangle {
                            id: box
                            x: 8
                            anchors.verticalCenter: parent.verticalCenter
                            width: 20
                            height: 20
                            radius: 10
                            color: entry.done ? piece.tone : "transparent"
                            border.color: entry.done ? piece.tone : root.soft(Tokens.text, 0.3)
                            border.width: 1.5

                            Behavior on color {
                                ColorAnimation { duration: Tokens.fast }
                            }

                            Canvas {
                                id: tick
                                anchors.fill: parent
                                onPaint: {
                                    const ctx = getContext("2d");
                                    ctx.reset();
                                    if (!entry.done)
                                        return;
                                    ctx.strokeStyle = Tokens.bg;
                                    ctx.lineWidth = 2.2;
                                    ctx.lineCap = "round";
                                    ctx.lineJoin = "round";
                                    ctx.beginPath();
                                    ctx.moveTo(5.5, 10.5);
                                    ctx.lineTo(8.8, 13.8);
                                    ctx.lineTo(14.8, 6.8);
                                    ctx.stroke();
                                }
                            }
                        }

                        Column {
                            id: words
                            x: box.x + box.width + 12
                            width: parent.width - x - quantity.width - 18
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 1

                            Ink {
                                width: parent.width
                                wrapMode: Text.WordWrap
                                font.strikeout: entry.done
                                color: entry.done ? Tokens.dim : Tokens.text
                                text: entry.modelData.titulo
                            }

                            Ink {
                                width: parent.width
                                dim: true
                                font.pixelSize: Tokens.smallSize
                                wrapMode: Text.WordWrap
                                text: entry.modelData.detalle
                                visible: text !== ""
                            }
                        }

                        Ink {
                            id: quantity
                            anchors.right: parent.right
                            anchors.rightMargin: 10
                            anchors.verticalCenter: parent.verticalCenter
                            strong: true
                            color: entry.done ? Tokens.dim : piece.tone
                            text: entry.modelData.valor
                        }

                        MouseArea {
                            id: tap
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: entry.done = !entry.done
                        }
                    }
                }
            }
        }

        // Qué significa algo: el término en grande, su categoría en el tono y
        // las acepciones numeradas, con su ejemplo.
        Component {
            id: definition

            Column {
                id: entry

                // Si el título de la tarjeta ya es la palabra, no se repite: solo su categoría.
                readonly property bool said: (root.card?.titulo ?? "").toLowerCase().indexOf(piece.modelData.texto.toLowerCase()) >= 0

                width: piece.width
                spacing: 12

                Column {
                    width: parent.width
                    spacing: 2
                    visible: !entry.said || piece.modelData.valor !== ""

                    Ink {
                        width: parent.width
                        wrapMode: Text.WordWrap
                        font.pixelSize: 32
                        font.weight: Font.DemiBold
                        font.variableAxes: ({ "wght": 650 })
                        text: piece.modelData.texto
                        visible: !entry.said
                    }

                    Ink {
                        width: parent.width
                        wrapMode: Text.WordWrap
                        font.italic: true
                        color: piece.tone
                        text: piece.modelData.valor
                        visible: text !== ""
                    }
                }

                Repeater {
                    model: piece.modelData.items

                    Item {
                        id: sense

                        required property var modelData
                        required property int index

                        width: piece.width
                        implicitHeight: meaning.implicitHeight

                        Ink {
                            strong: true
                            color: piece.tone
                            text: sense.index + 1
                        }

                        Column {
                            id: meaning
                            x: 24
                            width: parent.width - x
                            spacing: 3

                            Ink {
                                width: parent.width
                                wrapMode: Text.WordWrap
                                lineHeight: 1.15
                                text: sense.modelData.titulo
                            }

                            Ink {
                                width: parent.width
                                wrapMode: Text.WordWrap
                                dim: true
                                font.italic: true
                                font.pixelSize: Tokens.smallSize
                                text: "«" + sense.modelData.detalle + "»"
                                visible: sense.modelData.detalle !== ""
                            }
                        }
                    }
                }
            }
        }

        // Lo que no debe pasarse por alto: una caja teñida con su icono.
        Component {
            id: callout

            Rectangle {
                width: piece.width
                implicitHeight: Math.max(52, words.implicitHeight + 26)
                radius: Tokens.radius
                color: root.mix(Tokens.bg, piece.tone, 0.12)
                border.color: root.soft(piece.tone, 0.35)
                border.width: 1

                Icon {
                    id: mark
                    x: 14
                    y: 14
                    color: piece.tone
                    font.pixelSize: 22
                    name: piece.modelData.icono || ({ aviso: "warning", mal: "x-circle", bien: "check-circle" })[piece.modelData.color] || "lightbulb"
                }

                Column {
                    id: words
                    x: 48
                    y: 13
                    width: parent.width - x - 16
                    spacing: 3

                    Ink {
                        width: parent.width
                        strong: true
                        color: piece.tone
                        wrapMode: Text.WordWrap
                        text: piece.modelData.valor
                        visible: text !== ""
                    }

                    Ink {
                        width: parent.width
                        wrapMode: Text.WordWrap
                        lineHeight: 1.15
                        text: piece.modelData.texto
                    }
                }
            }
        }

        // Colores: muestras grandes con su código; pulsar una lo copia.
        Component {
            id: palette

            Row {
                id: swatches

                readonly property int count: piece.modelData.items.length

                width: piece.width
                spacing: 8

                Repeater {
                    model: piece.modelData.items

                    Column {
                        id: swatch

                        required property var modelData
                        readonly property color paint: modelData.valor
                        readonly property bool light: 0.299 * paint.r + 0.587 * paint.g + 0.114 * paint.b > 0.6
                        property bool copied: false

                        width: (swatches.width - swatches.spacing * (swatches.count - 1)) / swatches.count
                        spacing: 6

                        Rectangle {
                            width: parent.width
                            height: 76
                            radius: Tokens.radius
                            color: swatch.paint
                            border.color: root.soft(Tokens.text, 0.12)
                            border.width: 1

                            Ink {
                                anchors.centerIn: parent
                                font.pixelSize: Tokens.smallSize
                                strong: true
                                color: swatch.light ? "#111111" : "#ffffff"
                                text: "Copiado"
                                opacity: swatch.copied ? 1 : 0

                                Behavior on opacity {
                                    NumberAnimation { duration: Tokens.fast }
                                }
                            }

                            Timer {
                                running: swatch.copied
                                interval: 1200
                                onTriggered: swatch.copied = false
                            }

                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: {
                                    Quickshell.clipboardText = swatch.modelData.valor;
                                    swatch.copied = true;
                                }
                            }
                        }

                        Text {
                            width: parent.width
                            horizontalAlignment: Text.AlignHCenter
                            font.family: "JetBrains Mono"
                            font.pixelSize: 11
                            color: Tokens.dim
                            text: swatch.modelData.valor
                        }

                        Ink {
                            width: parent.width
                            horizontalAlignment: Text.AlignHCenter
                            elide: Text.ElideRight
                            font.pixelSize: Tokens.smallSize
                            text: swatch.modelData.titulo
                            visible: text !== ""
                        }
                    }
                }
            }
        }

        // Lo que probablemente quieras preguntar después: va directo al
        // cerebro, que recuerda lo hablado, y la isla vuelve a «Pensando».
        Component {
            id: asks

            Column {
                width: piece.width
                spacing: 0

                Repeater {
                    model: piece.modelData.items

                    Rectangle {
                        id: ask

                        required property var modelData

                        width: piece.width
                        height: 34
                        radius: Tokens.radius
                        color: press.containsMouse ? Tokens.surface : "transparent"

                        Behavior on color {
                            ColorAnimation { duration: Tokens.fast }
                        }

                        Icon {
                            id: mark
                            x: 8
                            anchors.verticalCenter: parent.verticalCenter
                            name: "arrow-bend-down-right"
                            color: press.containsMouse ? root.tone : root.soft(Tokens.dim, 0.6)
                            font.pixelSize: Tokens.smallSize + 2
                        }

                        Ink {
                            x: mark.x + mark.width + 10
                            width: parent.width - x - 36
                            anchors.verticalCenter: parent.verticalCenter
                            elide: Text.ElideRight
                            color: press.containsMouse ? Tokens.text : Tokens.dim
                            text: ask.modelData.titulo
                        }

                        Icon {
                            anchors.right: parent.right
                            anchors.rightMargin: 10
                            anchors.verticalCenter: parent.verticalCenter
                            name: "arrow-up"
                            color: root.tone
                            opacity: press.containsMouse ? 1 : 0

                            Behavior on opacity {
                                NumberAnimation { duration: Tokens.fast }
                            }
                        }

                        MouseArea {
                            id: press
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.follow(ask.modelData.titulo)
                        }
                    }
                }
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
