import QtQuick
import qs
import qs.state

// La tarjeta que el cerebro compone para una pregunta: la respuesta a la
// medida de lo preguntado, con piezas de un vocabulario fijo (veyld/src/card.rs):
// texto, lista, datos, cifra, métricas, cuenta atrás, gráficas (barras, líneas,
// anillo), tabla, pros y contras, tira, pasos, cronología, valoración, cita,
// código, galería, etiquetas, imagen, barra, botones (mandan una orden a Veyl)
// y enlaces. Va en el acento del tema; el modelo solo marca cosas pequeñas con
// un color con sentido (bien, aviso, mal), del esquema. Sale en la isla del
// centro después de «Pensando».
Item {
    id: root

    property var card: null
    signal done

    readonly property var blocks: card?.bloques ?? []
    readonly property color tone: Tokens.accent

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

    // Las piezas del cuerpo y, aparte, enlaces y botones, que van abajo.
    readonly property var main: blocks.filter(b => b.tipo !== "enlaces" && b.tipo !== "acciones")
    readonly property var tail: blocks.filter(b => b.tipo === "enlaces" || b.tipo === "acciones")

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
    implicitHeight: Math.min(body.implicitHeight + Tokens.pad * 2, limit)

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
        readonly property color tone: root.tint(modelData.color ?? "", root.tone)

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
                galeria: gallery
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
                implicitHeight: Math.max(76, figureText.implicitHeight + 24)
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
                        font.pixelSize: piece.modelData.valor.length > 14 ? 22 : 30
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

                Timer {
                    running: clock.visible
                    repeat: true
                    interval: 1000
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
                                color: column.peak ? Tokens.accent : root.soft(Tokens.accent, 0.4)
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
                            const a = Tokens.accent;
                            const fill = ctx.createLinearGradient(0, 0, 0, height);
                            fill.addColorStop(0, Qt.rgba(a.r, a.g, a.b, 0.32));
                            fill.addColorStop(1, Qt.rgba(a.r, a.g, a.b, 0));
                            ctx.beginPath();
                            ctx.moveTo(xy[0][0], height);
                            for (const p of xy)
                                ctx.lineTo(p[0], p[1]);
                            ctx.lineTo(xy[n - 1][0], height);
                            ctx.closePath();
                            ctx.fillStyle = fill;
                            ctx.fill();
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
                        color: Tokens.accent
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
                    return root.soft(Tokens.accent, [1, 0.7, 0.5, 0.35, 0.24, 0.16][i % 6]);
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
                                        color: best ? Tokens.accent : Tokens.text
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

                        width: (mosaic.width - mosaic.spacing * (mosaic.columns - 1)) / mosaic.columns
                        implicitHeight: stat.implicitHeight + 24
                        radius: Tokens.radius
                        color: Tokens.surface

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
                                    color: Tokens.accent
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
                            color: index === 0 ? root.soft(Tokens.accent, 0.14) : Tokens.surface

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
                                    color: Tokens.accent
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
                            color: root.soft(Tokens.accent, 0.16)

                            Ink {
                                anchors.centerIn: parent
                                strong: true
                                color: Tokens.accent
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
                            color: moment.last ? Tokens.accent : Tokens.plate
                            border.color: Tokens.accent
                            border.width: 2
                        }

                        Column {
                            id: words
                            x: 26
                            width: parent.width - x
                            spacing: 2

                            Ink {
                                strong: true
                                color: Tokens.accent
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
                    color: Tokens.accent
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
                                        color: Tokens.accent
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
                implicitHeight: words.implicitHeight

                Rectangle {
                    width: 3
                    height: parent.height
                    radius: 2
                    color: Tokens.accent
                }

                Column {
                    id: words
                    x: 16
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

        Component {
            id: links

            Flow {
                width: piece.width
                spacing: 8

                Repeater {
                    model: piece.modelData.items

                    Pill {
                        required property var modelData
                        text: modelData.titulo || modelData.url.replace(/^https?:\/\/(www\.)?/, "").split("/")[0]
                        icon: "link"
                        onPressed: {
                            Qt.openUrlExternally(modelData.url);
                            root.done();
                        }
                    }
                }
            }
        }
    }

    Flickable {
        id: scroller
        anchors.fill: parent
        contentHeight: body.implicitHeight + Tokens.pad * 2
        interactive: contentHeight > height
        boundsBehavior: Flickable.StopAtBounds
        clip: true

    Column {
        id: body

        x: Tokens.pad
        y: Tokens.pad
        width: root.width - Tokens.pad * 2
        spacing: 14

        // Portada, con el color del tema fundiéndose desde abajo.
        Picture {
            id: cover
            width: parent.width
            tall: 210
            source: root.card?.imagen ?? ""

            Rectangle {
                anchors.fill: parent
                gradient: Gradient {
                    GradientStop { position: 0.55; color: "transparent" }
                    GradientStop { position: 1; color: root.soft(Tokens.bg, 0.85) }
                }
            }
        }

        // Cabeza: icono del tema, título y subtítulo (o lo que se preguntó).
        Item {
            width: parent.width
            implicitHeight: Math.max(glyph.height, heading.implicitHeight)

            Badge {
                id: glyph
                anchors.verticalCenter: parent.verticalCenter
                size: 42
                icon: root.card?.icono || "sparkle"
            }

            Column {
                id: heading
                x: glyph.width + 12
                width: parent.width - x - close.width - 12
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

            Icon {
                id: close
                anchors.right: parent.right
                anchors.top: parent.top
                name: "x-circle"
                color: shut.containsMouse ? Tokens.text : Tokens.dim

                MouseArea {
                    id: shut
                    anchors.fill: parent
                    anchors.margins: -6
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.done()
                }
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
                    }
                }
            }
        }

        // Enlaces y botones, abajo y a todo lo ancho.
        Repeater {
            model: root.tail

            Piece {
                width: body.width
            }
        }
    }
    }
}
