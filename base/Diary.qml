import QtQuick
import Quickshell
import Quickshell.Wayland
import qs
import qs.state

// El diario, en el monitor desde el que se llamó: una isla grande que nace
// del centro del borde de arriba como los ajustes (y, si viene del centro de
// control, es esa isla la que crece). Arriba, el día y los de antes, como una
// tira de barras; debajo, el campo: lo que escribes se busca tal cual en los
// 90 días y, con Intro, se le pregunta a Veyl, que lo busca por lo que
// significa. Sin nada escrito, el día: a la izquierda el resumen (cifras, las
// horas, aplicaciones, proyectos, agentes, lo que sonó, archivos, llamadas y
// lo que le pediste) y a la derecha la línea del tiempo, rato a rato, de
// arriba abajo. Mientras está abierto se queda con el teclado; pulsar fuera
// o Esc lo cierra.
PanelWindow {
    id: root

    screen: Quickshell.screens.find(screen => screen.name === Recall.monitor) ?? null
    visible: Recall.open || notch.progress > 0
    anchors {
        top: true
        bottom: true
        left: true
        right: true
    }
    exclusionMode: ExclusionMode.Ignore
    color: "transparent"
    mask: Region {
        item: Recall.open ? outside : notch
    }
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "veyl-diary"
    WlrLayershell.keyboardFocus: Recall.open ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None

    readonly property real wide: Math.min(Tokens.dials, width - (Tokens.join + Tokens.gap) * 2 - 80)
    readonly property real tall: Math.min(Tokens.diaryTall, height - Tokens.gapTop * 2)
    readonly property real inner: wide - Tokens.inset * 2
    readonly property int card: Tokens.round - Tokens.inset
    readonly property var diary: Recall.report
    readonly property var labels: diary?.labels ?? ({})
    // Los ratos, sin la noche de antes de empezar: lo de fuera antes de lo primero no cuenta.
    readonly property var blocks: {
        const all = diary?.blocks ?? [];
        const first = all.findIndex(b => b.app !== "");
        return first < 0 ? all : all.slice(first);
    }
    // Una aplicación de la que se ven solo sus ratos, o "".
    property string only: ""
    // Un instante al que llevar la línea del tiempo (de un resultado), o 0.
    property double aim: 0

    function label(app: string): string {
        return labels[app] ?? (app.split(".").pop() || app);
    }

    // «45 min», «2 h 5 min», «menos de un minuto».
    function span(ms: double): string {
        const minutes = Math.round(ms / 60000);
        if (minutes < 1)
            return "menos de 1 min";
        if (minutes < 60)
            return `${minutes} min`;
        const hours = Math.floor(minutes / 60);
        return minutes % 60 && hours < 10 ? `${hours} h ${minutes % 60} min` : `${Math.round(minutes / 60)} h`;
    }

    function clock(ms: double): string {
        return Agenda.clock(ms);
    }

    function capital(text: string): string {
        return text.charAt(0).toUpperCase() + text.slice(1);
    }

    function tilde(path: string): string {
        const home = diary?.home ?? Quickshell.env("HOME");
        return home && path.startsWith(home) ? `~${path.slice(home.length)}` : path;
    }

    function base(path: string): string {
        return path.replace(/\/+$/, "").split("/").pop() || path;
    }

    // El nombre de un día: hoy, ayer, el jueves, el 12 de noviembre.
    function dayName(day: int): string {
        const away = Calendar.today - day;
        if (away === 0)
            return "Hoy";
        if (away === 1)
            return "Ayer";
        return capital(Calendar.format(day, away < 7 ? "dddd" : "dddd d 'de' MMMM"));
    }

    // Un título sin el nombre del navegador al final.
    function tidy(title: string): string {
        return (title ?? "").replace(/\s+[—–-]\s+(Mozilla Firefox|Google Chrome|Chromium|Brave|Zen Browser|Vivaldi)$/, "");
    }

    function openLink(url: string): void {
        if (!url)
            return;
        Qt.openUrlExternally(url);
        Recall.hide();
    }

    Connections {
        target: Recall

        function onOpenChanged(): void {
            if (!Recall.open)
                return;
            if (Recall.origin.width > 0)
                notch.from(Recall.origin);
            field.forceActiveFocus();
        }

        function onDayChanged(): void {
            root.only = "";
            if (!root.aim)
                timeline.contentY = 0;
            summary.contentY = 0;
        }

        function onReportChanged(): void {
            if (root.aim)
                Qt.callLater(root.reach);
        }
    }

    // Lleva la línea del tiempo al rato en el que cae `aim`.
    function reach(): void {
        const blocks = root.blocks;
        const at = blocks.findIndex(b => b.end >= root.aim);
        if (at >= 0) {
            const row = rail.itemAt(at);
            if (row)
                timeline.contentY = Math.max(0, Math.min(row.y - 40, timeline.contentHeight - timeline.height));
        }
        lit.restart();
    }

    Timer {
        id: lit
        interval: 2600
        onTriggered: root.aim = 0
    }

    MouseArea {
        id: outside
        anchors.fill: parent
        acceptedButtons: Qt.AllButtons
        onPressed: Recall.hide()
    }

    Rectangle {
        anchors.fill: parent
        color: Tokens.scrim
        opacity: Math.min(1, notch.progress)
    }

    // --- Piezas ---

    component Sheet: Rectangle {
        radius: root.card
        color: Tokens.surface
    }

    component Heading: Item {
        property alias text: words.text
        property alias aside: extra.text

        width: parent.width
        height: 30

        Ink {
            id: words
            x: 4
            anchors.verticalCenter: parent.verticalCenter
            dim: true
            font.pixelSize: Tokens.smallSize
        }

        Ink {
            id: extra
            anchors.right: parent.right
            anchors.rightMargin: 4
            anchors.verticalCenter: parent.verticalCenter
            dim: true
            font.pixelSize: Tokens.tinySize
        }
    }

    // El icono de una aplicación por su clase o, si el tema no lo tiene, uno de Phosphor.
    component Mark: Item {
        id: mark

        property string app
        property string glyph: "app-window"
        readonly property string picture: Apps.emblem(app)

        width: 26
        height: 26

        Rectangle {
            anchors.fill: parent
            radius: Tokens.soft
            color: Tokens.bg
            visible: mark.picture === ""
        }

        Image {
            anchors.fill: parent
            anchors.margins: 1
            source: mark.picture
            sourceSize: Qt.size(52, 52)
            fillMode: Image.PreserveAspectFit
            asynchronous: true
            visible: mark.picture !== ""
        }

        Icon {
            anchors.centerIn: parent
            name: mark.glyph
            font.pixelSize: Tokens.iconSize - 3
            visible: mark.picture === ""
        }
    }

    // Una fila con algo y su rato, y una barra apagada de lo que pesa.
    component Share: Press {
        id: share

        property string app
        property string glyph: "app-window"
        property string title
        property string detail
        property double ms: 0
        property double most: 1
        property bool lit: false

        width: parent.width
        height: detail ? 44 : 36
        radius: root.card - 6
        rest: lit ? Tokens.bg : "transparent"

        Mark {
            id: icon
            x: 6
            anchors.verticalCenter: parent.verticalCenter
            app: share.app
            glyph: share.glyph
            width: 24
            height: 24
        }

        Column {
            anchors.left: icon.right
            anchors.leftMargin: 10
            anchors.right: amount.left
            anchors.rightMargin: 10
            anchors.verticalCenter: parent.verticalCenter
            spacing: 3

            Ink {
                width: parent.width
                text: share.title
                font.pixelSize: Tokens.smallSize + 1
                elide: Text.ElideRight
            }

            Ink {
                width: parent.width
                text: share.detail
                dim: true
                font.pixelSize: Tokens.tinySize
                elide: Text.ElideMiddle
                visible: text !== ""
            }

            Rectangle {
                width: parent.width
                height: 3
                radius: 1.5
                color: Tokens.bg
                visible: share.most > 0 && share.ms > 0

                Rectangle {
                    width: Math.max(3, parent.width * Math.min(1, share.ms / share.most))
                    height: parent.height
                    radius: parent.radius
                    color: share.lit ? Tokens.accent : Tokens.dim
                    opacity: share.lit ? 1 : 0.55
                }
            }
        }

        Ink {
            id: amount
            anchors.right: parent.right
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            text: share.ms > 0 ? root.span(share.ms) : ""
            dim: true
            font.pixelSize: Tokens.tinySize
        }
    }

    // Una cifra del día.
    component Figure: Column {
        property alias value: number.text
        property alias caption: words.text

        width: (parent.width - parent.spacing * 3) / 4
        spacing: 2

        Ink {
            id: number
            width: parent.width
            font.pixelSize: Tokens.promptSize
            font.weight: Font.DemiBold
            elide: Text.ElideRight
        }

        Ink {
            id: words
            width: parent.width
            dim: true
            font.pixelSize: Tokens.tinySize
            elide: Text.ElideRight
        }
    }

    component Round: Press {
        id: round

        property alias icon: glyph.name

        width: Tokens.knob
        height: Tokens.knob
        radius: width / 2

        Icon {
            id: glyph
            anchors.centerIn: parent
            font.pixelSize: Tokens.iconSize - 2
        }
    }

    component Empty: Item {
        property alias icon: glyph.name
        property alias text: title.text
        property alias hint: detail.text

        width: parent.width
        height: 150

        Column {
            anchors.centerIn: parent
            width: parent.width - 48
            spacing: 6

            Icon {
                id: glyph
                anchors.horizontalCenter: parent.horizontalCenter
                font.pixelSize: 30
                color: Tokens.border
            }

            Ink {
                id: title
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.WordWrap
            }

            Ink {
                id: detail
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.WordWrap
                dim: true
                font.pixelSize: Tokens.smallSize
                visible: text !== ""
            }
        }
    }

    // --- La isla ---

    Notch {
        id: notch

        anchors.horizontalCenter: parent.horizontalCenter
        shown: Recall.open
        settle: Tokens.summon
        stretch: Tokens.veil
        bounce: 0.2

        FocusScope {
            id: body

            width: root.wide
            height: root.tall
            focus: true

            Column {
                id: column
                x: Tokens.inset
                width: root.inner
                spacing: 8

                // El día, grande, y a la derecha los días de antes, como barras.
                Item {
                    width: parent.width
                    height: 58

                    Text {
                        id: number
                        x: Tokens.pad - 4
                        anchors.verticalCenter: parent.verticalCenter
                        text: Calendar.parts(Recall.day).date
                        color: Tokens.accent
                        font.family: Tokens.sans
                        font.pixelSize: 34
                        font.weight: Font.DemiBold
                        font.features: ({ "tnum": 1 })
                    }

                    Column {
                        anchors.left: number.right
                        anchors.leftMargin: 12
                        anchors.right: strip.left
                        anchors.rightMargin: 16
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 1

                        Row {
                            spacing: 8

                            Ink {
                                text: root.dayName(Recall.day)
                                strong: true
                            }

                            Ink {
                                anchors.baseline: parent.children[0].baseline
                                text: Recall.today || Calendar.today - Recall.day < 7 ? Calendar.format(Recall.day, "d 'de' MMMM") : ""
                                dim: true
                                font.pixelSize: Tokens.smallSize
                            }
                        }

                        Ink {
                            width: parent.width
                            dim: true
                            font.pixelSize: Tokens.smallSize
                            elide: Text.ElideRight
                            text: {
                                if (!Daemon.connected)
                                    return "veyld no está en marcha";
                                if (!root.diary)
                                    return Recall.loading ? "Abriendo el diario…" : Recall.failed;
                                if (root.diary.total < 60000)
                                    return "Nada en el diario";
                                return `${root.span(root.diary.total)} delante · de ${root.clock(root.diary.first)} a ${root.clock(root.diary.last)}`;
                            }
                        }
                    }

                    Row {
                        id: strip
                        anchors.right: parent.right
                        anchors.rightMargin: Tokens.pad - 10
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 4

                        Round {
                            anchors.verticalCenter: parent.verticalCenter
                            icon: "caret-left"
                            onTapped: Recall.step(-1)
                        }

                        // Cada día, una barra de lo que hubo delante; el que se mira, del acento.
                        Row {
                            id: bars
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 3

                            readonly property var days: root.diary?.days ?? []
                            readonly property double most: Math.max(3600000, ...days.map(d => d.ms))

                            Repeater {
                                model: bars.days

                                Press {
                                    id: bar

                                    required property var modelData
                                    readonly property int day: Recall.fromIso(modelData.day)
                                    readonly property bool here: day === Recall.day

                                    width: 14
                                    height: 44
                                    radius: 4
                                    onTapped: Recall.go(day)

                                    Rectangle {
                                        anchors.horizontalCenter: parent.horizontalCenter
                                        anchors.bottom: initial.top
                                        anchors.bottomMargin: 3
                                        width: 6
                                        height: Math.max(3, 26 * bar.modelData.ms / bars.most)
                                        radius: 3
                                        color: bar.here ? Tokens.accent : bar.hovered ? Tokens.dim : Tokens.border

                                        Behavior on height {
                                            NumberAnimation { duration: Tokens.veil; easing.type: Easing.OutCubic }
                                        }
                                    }

                                    Ink {
                                        id: initial
                                        anchors.horizontalCenter: parent.horizontalCenter
                                        anchors.bottom: parent.bottom
                                        text: Calendar.locale.dayName(Calendar.parts(bar.day).weekday, Locale.NarrowFormat)
                                        color: bar.here ? Tokens.text : Tokens.dim
                                        font.pixelSize: 9
                                        font.weight: bar.here ? Font.DemiBold : Font.Medium
                                    }
                                }
                            }
                        }

                        Round {
                            anchors.verticalCenter: parent.verticalCenter
                            icon: "caret-right"
                            active: !Recall.today
                            opacity: Recall.today ? 0.3 : 1
                            onTapped: Recall.step(1)
                        }
                    }
                }

                // El campo: buscar tal cual mientras escribes, preguntar con Intro.
                Rectangle {
                    width: parent.width
                    height: 46
                    radius: root.card
                    color: Tokens.surface
                    border.width: Tokens.line
                    border.color: field.activeFocus ? Tokens.accent : "transparent"

                    Behavior on border.color {
                        ColorAnimation { duration: Tokens.fast }
                    }

                    Icon {
                        id: lens
                        x: 16
                        anchors.verticalCenter: parent.verticalCenter
                        name: Recall.pending ? "sparkle" : "magnifying-glass"
                        color: Recall.pending || field.activeFocus ? Tokens.accent : Tokens.dim
                    }

                    TextInput {
                        id: field
                        anchors.left: lens.right
                        anchors.leftMargin: 12
                        anchors.right: hint.left
                        anchors.rightMargin: 12
                        anchors.verticalCenter: parent.verticalCenter
                        clip: true
                        focus: true
                        text: Recall.query
                        onTextChanged: if (Recall.query !== text) Recall.query = text
                        color: Tokens.text
                        selectionColor: Tokens.accent
                        selectedTextColor: Tokens.bg
                        selectByMouse: true
                        font.family: Tokens.sans
                        font.pixelSize: Tokens.fontSize

                        Connections {
                            target: Recall

                            function onQueryChanged(): void {
                                if (field.text !== Recall.query)
                                    field.text = Recall.query;
                            }
                        }

                        Keys.onPressed: event => {
                            const empty = field.text === "";
                            if (event.key === Qt.Key_Escape) {
                                if (!empty || Recall.answer) {
                                    Recall.query = "";
                                    Recall.forget();
                                } else {
                                    Recall.hide();
                                }
                            } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                                Recall.ask();
                            } else if (event.key === Qt.Key_PageUp || (event.key === Qt.Key_Left && (empty || event.modifiers & Qt.AltModifier))) {
                                Recall.step(-1);
                            } else if (event.key === Qt.Key_PageDown || (event.key === Qt.Key_Right && (empty || event.modifiers & Qt.AltModifier))) {
                                Recall.step(1);
                            } else if (event.key === Qt.Key_Down && empty) {
                                timeline.flick(0, -900);
                            } else if (event.key === Qt.Key_Up && empty) {
                                timeline.flick(0, 900);
                            } else {
                                return;
                            }
                            event.accepted = true;
                        }

                        Ink {
                            anchors.verticalCenter: parent.verticalCenter
                            width: parent.width
                            text: Daemon.thinks ? "Busca en el diario o pregúntale a Veyl: «¿cuándo leí lo de la beca?»" : "Busca en el diario"
                            dim: true
                            elide: Text.ElideRight
                            visible: field.text === "" && !field.preeditText
                        }
                    }

                    Row {
                        id: hint
                        anchors.right: parent.right
                        anchors.rightMargin: 14
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 8
                        opacity: Recall.searching && Daemon.thinks && !Recall.pending ? 1 : 0
                        visible: opacity > 0

                        Behavior on opacity {
                            NumberAnimation { duration: Tokens.fast }
                        }

                        Ink {
                            anchors.verticalCenter: parent.verticalCenter
                            text: "Preguntar a Veyl"
                            dim: true
                            font.pixelSize: Tokens.smallSize
                        }

                        Keycap {
                            anchors.verticalCenter: parent.verticalCenter
                            keys: "return"
                        }
                    }
                }

                // Lo de abajo: el día, o lo buscado y lo que contesta Veyl.
                Item {
                    id: stage
                    width: parent.width
                    height: root.tall - 58 - 46 - column.spacing * 2 - Tokens.inset

                    // --- El día ---
                    Item {
                        anchors.fill: parent
                        opacity: Recall.searching || Recall.answer ? 0 : 1
                        visible: opacity > 0.01
                        enabled: visible

                        Behavior on opacity {
                            NumberAnimation { duration: Tokens.fast }
                        }

                        Empty {
                            anchors.centerIn: parent
                            visible: !Daemon.connected || !Settings.journal || !Recall.loading && (!root.diary || root.diary.total < 60000 && (root.diary.blocks ?? []).length === 0)
                            icon: !Daemon.connected ? "plugs" : !Settings.journal ? "notebook" : "moon-stars"
                            text: !Daemon.connected ? "veyld no está en marcha" : !Settings.journal ? "El diario está apagado" : Recall.failed || "Nada en el diario este día"
                            hint: !Daemon.connected ? "Sin él no hay diario." : !Settings.journal ? "Se enciende en Ajustes → Diario y órdenes." : Recall.today ? "Lo que hagas a partir de ahora saldrá aquí." : "Prueba otro día en la tira de arriba."
                        }

                        // A la izquierda, el resumen.
                        Flickable {
                            id: summary
                            width: Math.round(parent.width * 0.42)
                            height: parent.height
                            contentHeight: left.height
                            clip: true
                            boundsBehavior: Flickable.StopAtBounds
                            visible: !!root.diary && (root.diary.total >= 60000 || (root.diary.blocks ?? []).length > 0)

                            Column {
                                id: left
                                width: summary.width
                                spacing: 8

                                Sheet {
                                    width: parent.width
                                    height: figures.height + hours.height + 34

                                    Row {
                                        id: figures
                                        x: 14
                                        y: 12
                                        width: parent.width - 28
                                        spacing: 8

                                        Figure {
                                            value: root.diary ? root.span(root.diary.total).replace("menos de 1 min", "—") : ""
                                            caption: "delante"
                                        }

                                        Figure {
                                            value: root.diary?.sessions?.length ?? 0
                                            caption: (root.diary?.sessions?.length ?? 0) === 1 ? "rato seguido" : "ratos seguidos"
                                        }

                                        Figure {
                                            value: root.diary?.switches ?? 0
                                            caption: "cambios de ventana"
                                        }

                                        Figure {
                                            value: (root.diary?.agents ?? []).length
                                            caption: "agentes"
                                        }
                                    }

                                    // Las horas del día: cuánto hubo delante en cada una.
                                    Item {
                                        id: hours
                                        x: 14
                                        anchors.top: figures.bottom
                                        anchors.topMargin: 12
                                        width: parent.width - 28
                                        height: 52

                                        readonly property var slices: root.diary?.slices ?? []
                                        readonly property real each: width / 24

                                        Repeater {
                                            model: hours.slices

                                            Press {
                                                id: hour

                                                required property var modelData
                                                readonly property int at: new Date(modelData.start).getHours()
                                                readonly property bool now: Recall.today && at === Clock.now.getHours()

                                                x: at * hours.each
                                                width: hours.each
                                                height: 38
                                                radius: 3
                                                active: modelData.ms > 0
                                                onTapped: {
                                                    root.aim = modelData.start;
                                                    root.reach();
                                                }

                                                Rectangle {
                                                    anchors.horizontalCenter: parent.horizontalCenter
                                                    anchors.bottom: parent.bottom
                                                    width: Math.max(3, parent.width - 4)
                                                    height: Math.max(2, 36 * hour.modelData.ms / 3600000)
                                                    radius: 2
                                                    color: hour.now ? Tokens.accent : hour.hovered ? Tokens.dim : Tokens.border
                                                }
                                            }
                                        }

                                        Repeater {
                                            model: [0, 6, 12, 18]

                                            Ink {
                                                required property int modelData
                                                x: modelData * hours.each
                                                y: 40
                                                text: `${modelData}`
                                                dim: true
                                                font.pixelSize: 9
                                            }
                                        }
                                    }
                                }

                                Sheet {
                                    width: parent.width
                                    height: apps.height + 8
                                    visible: (root.diary?.apps ?? []).length > 0

                                    Column {
                                        id: apps
                                        x: 6
                                        y: 2
                                        width: parent.width - 12

                                        Heading {
                                            text: "Aplicaciones"
                                            aside: root.only ? "solo esa, en la línea del tiempo" : ""
                                        }

                                        Repeater {
                                            model: (root.diary?.apps ?? []).slice(0, 7)

                                            Share {
                                                required property var modelData
                                                app: modelData.app
                                                title: root.label(modelData.app)
                                                detail: (modelData.sites?.[0]?.name ?? "") || root.tilde(modelData.folders?.[0]?.name ?? "") || root.tidy(modelData.titles?.[0]?.[0] ?? "")
                                                ms: modelData.ms
                                                most: root.diary.apps[0].ms
                                                lit: root.only === modelData.app
                                                onTapped: root.only = root.only === modelData.app ? "" : modelData.app
                                            }
                                        }
                                    }
                                }

                                Sheet {
                                    width: parent.width
                                    height: projects.height + 8
                                    visible: (root.diary?.projects ?? []).length > 0

                                    Column {
                                        id: projects
                                        x: 6
                                        y: 2
                                        width: parent.width - 12

                                        Heading {
                                            text: "Proyectos"
                                        }

                                        Repeater {
                                            model: (root.diary?.projects ?? []).slice(0, 5)

                                            Share {
                                                required property var modelData
                                                glyph: "folder-simple"
                                                title: root.base(modelData.path)
                                                detail: [root.tilde(modelData.path), modelData.agents > 60000 ? `agentes ${root.span(modelData.agents)}` : ""].filter(x => x).join(" · ")
                                                ms: modelData.ms
                                                most: Math.max(1, root.diary.projects[0].ms)
                                            }
                                        }
                                    }
                                }

                                Sheet {
                                    width: parent.width
                                    height: agents.height + 8
                                    visible: (root.diary?.agents ?? []).length > 0

                                    Column {
                                        id: agents
                                        x: 6
                                        y: 2
                                        width: parent.width - 12

                                        Heading {
                                            text: "Agentes"
                                        }

                                        Repeater {
                                            model: (root.diary?.agents ?? []).slice(0, 5)

                                            Share {
                                                required property var modelData
                                                glyph: "robot"
                                                title: [modelData.label, modelData.name].filter(x => x).join(" · ")
                                                detail: [root.tilde(modelData.folder), `${root.clock(modelData.first)}–${root.clock(modelData.last)}`, modelData.waits > 0 ? `te esperó ${modelData.waits} ${modelData.waits === 1 ? "vez" : "veces"}` : ""].filter(x => x).join(" · ")
                                                ms: modelData.working
                                                most: Math.max(1, root.diary.agents[0].working)
                                            }
                                        }
                                    }
                                }

                                Sheet {
                                    width: parent.width
                                    height: media.height + 8
                                    visible: (root.diary?.media ?? []).length > 0

                                    Column {
                                        id: media
                                        x: 6
                                        y: 2
                                        width: parent.width - 12

                                        Heading {
                                            text: "Lo que sonó"
                                        }

                                        Repeater {
                                            model: (root.diary?.media ?? []).slice(0, 5)

                                            Share {
                                                required property var modelData
                                                glyph: "music-notes"
                                                title: modelData.title || modelData.player
                                                detail: [modelData.artist, modelData.player].filter(x => x).join(" · ")
                                                ms: modelData.ms
                                                most: Math.max(1, root.diary.media[0].ms)
                                            }
                                        }
                                    }
                                }

                                Sheet {
                                    width: parent.width
                                    height: files.height + 8
                                    visible: (root.diary?.files ?? []).length > 0

                                    Column {
                                        id: files
                                        x: 6
                                        y: 2
                                        width: parent.width - 12

                                        Heading {
                                            text: "Archivos"
                                            aside: (root.diary?.files ?? []).length > 6 ? `${root.diary.files.length} en total` : ""
                                        }

                                        Repeater {
                                            model: (root.diary?.files ?? []).slice(0, 6)

                                            Share {
                                                required property var modelData
                                                glyph: "file"
                                                title: root.base(modelData.path)
                                                detail: `${root.clock(modelData.time)} · ${root.tilde(modelData.path.replace(/\/[^/]*$/, ""))}`
                                                onTapped: root.openLink(`file://${modelData.path}`)
                                            }
                                        }
                                    }
                                }

                                Sheet {
                                    width: parent.width
                                    height: calls.height + 8
                                    visible: (root.diary?.calls ?? []).length > 0

                                    Column {
                                        id: calls
                                        x: 6
                                        y: 2
                                        width: parent.width - 12

                                        Heading {
                                            text: "Micro, cámara y pantalla"
                                        }

                                        Repeater {
                                            model: (root.diary?.calls ?? []).slice(0, 6)

                                            Share {
                                                required property var modelData
                                                glyph: modelData.kind === "cámara" || modelData.kind === "camera" ? "video-camera" : modelData.kind === "pantalla" || modelData.kind === "screen" ? "screencast" : "microphone"
                                                title: `${root.clock(modelData.start)}–${root.clock(modelData.end)} · ${root.capital(modelData.kind)}`
                                                detail: [modelData.app ? root.label(modelData.app) : "", modelData.site].filter(x => x).join(" · ")
                                                ms: modelData.end - modelData.start
                                                most: 0
                                            }
                                        }
                                    }
                                }

                                Sheet {
                                    width: parent.width
                                    height: asked.height + 8
                                    visible: (root.diary?.orders ?? []).length > 0

                                    Column {
                                        id: asked
                                        x: 6
                                        y: 2
                                        width: parent.width - 12

                                        Heading {
                                            text: "Lo que le pediste a Veyl"
                                            aside: `${(root.diary?.orders ?? []).length}`
                                        }

                                        Repeater {
                                            model: (root.diary?.orders ?? []).slice(-6).reverse()

                                            Share {
                                                required property var modelData
                                                glyph: !modelData.ok ? "warning-circle" : modelData.source === "voice" ? "microphone" : modelData.source === "diary" ? "notebook" : "keyboard"
                                                title: root.capital(modelData.said)
                                                detail: `${root.clock(modelData.time)} · ${modelData.answer}`
                                            }
                                        }
                                    }
                                }

                                Item {
                                    width: 1
                                    height: 4
                                }
                            }
                        }

                        // A la derecha, la línea del tiempo: rato a rato, de arriba abajo.
                        Sheet {
                            anchors.right: parent.right
                            width: parent.width - summary.width - 8
                            height: parent.height
                            visible: summary.visible

                            Heading {
                                id: railHead
                                x: 6
                                y: 2
                                width: parent.width - 12
                                text: root.only ? `Rato a rato · ${root.label(root.only)}` : "Rato a rato"
                                aside: Recall.today ? `hasta las ${root.clock(Clock.now.getTime())}` : ""
                            }

                            Flickable {
                                id: timeline
                                x: 6
                                anchors.top: railHead.bottom
                                anchors.bottom: parent.bottom
                                anchors.bottomMargin: 6
                                width: parent.width - 12
                                contentHeight: rows.height
                                clip: true
                                boundsBehavior: Flickable.StopAtBounds

                                Behavior on contentY {
                                    enabled: root.aim > 0
                                    NumberAnimation { duration: Tokens.veil; easing.type: Easing.OutCubic }
                                }

                                Column {
                                    id: rows
                                    width: timeline.width

                                    Repeater {
                                        id: rail
                                        model: root.blocks

                                        Press {
                                            id: block

                                            required property var modelData
                                            required property int index
                                            readonly property bool away: modelData.app === ""
                                            readonly property bool shown: !root.only || modelData.app === root.only
                                            readonly property bool last: Recall.today && index === root.blocks.length - 1
                                            readonly property bool aimed: root.aim > 0 && modelData.start <= root.aim + 60000 && modelData.end >= root.aim

                                            width: rows.width
                                            height: !shown ? 0 : away ? 34 : 52
                                            visible: shown
                                            radius: root.card - 6
                                            rest: aimed ? Tokens.bg : "transparent"
                                            active: !away && modelData.url !== ""
                                            onTapped: root.openLink(modelData.url)

                                            // El raíl: una línea que une los ratos, con su punto.
                                            Rectangle {
                                                x: 66
                                                width: 1
                                                height: parent.height
                                                color: Tokens.border
                                            }

                                            Rectangle {
                                                x: 63
                                                anchors.verticalCenter: parent.verticalCenter
                                                width: 7
                                                height: 7
                                                radius: 3.5
                                                color: block.last || block.aimed ? Tokens.accent : block.away ? Tokens.border : Tokens.dim
                                                visible: !block.away
                                            }

                                            Column {
                                                x: 6
                                                width: 50
                                                anchors.verticalCenter: parent.verticalCenter

                                                Ink {
                                                    width: parent.width
                                                    horizontalAlignment: Text.AlignRight
                                                    text: root.clock(block.modelData.start)
                                                    font.pixelSize: Tokens.smallSize
                                                    color: block.away ? Tokens.dim : Tokens.text
                                                }

                                                Ink {
                                                    width: parent.width
                                                    horizontalAlignment: Text.AlignRight
                                                    text: root.span(block.modelData.end - block.modelData.start)
                                                    dim: true
                                                    font.pixelSize: 10
                                                    visible: !block.away
                                                }
                                            }

                                            Mark {
                                                id: emblem
                                                x: 80
                                                anchors.verticalCenter: parent.verticalCenter
                                                app: block.modelData.app
                                                visible: !block.away
                                            }

                                            Label {
                                                x: 80
                                                anchors.verticalCenter: parent.verticalCenter
                                                visible: block.away
                                                dim: true
                                                icon: block.modelData.away === "suspendido" ? "moon" : block.modelData.away === "bloqueado" ? "lock" : block.modelData.away ? "coffee" : "question"
                                                text: `${block.modelData.away ? root.capital(block.modelData.away) : "Sin diario"} · ${root.span(block.modelData.end - block.modelData.start)}`
                                                scale: 0.88
                                                transformOrigin: Item.Left
                                            }

                                            Column {
                                                anchors.left: emblem.right
                                                anchors.leftMargin: 10
                                                anchors.right: go.left
                                                anchors.rightMargin: 6
                                                anchors.verticalCenter: parent.verticalCenter
                                                spacing: 2
                                                visible: !block.away

                                                Row {
                                                    width: parent.width
                                                    spacing: 6

                                                    Ink {
                                                        id: who
                                                        text: root.label(block.modelData.app)
                                                        font.pixelSize: Tokens.smallSize + 1
                                                        font.weight: Font.DemiBold
                                                    }

                                                    Ink {
                                                        anchors.baseline: who.baseline
                                                        width: parent.width - who.width - 6
                                                        text: block.modelData.place.startsWith("/") ? root.tilde(block.modelData.place) : block.modelData.place
                                                        dim: true
                                                        font.pixelSize: Tokens.tinySize
                                                        elide: Text.ElideMiddle
                                                    }
                                                }

                                                Ink {
                                                    width: parent.width
                                                    text: block.modelData.command ? `$ ${block.modelData.detail}` : root.tidy(block.modelData.detail)
                                                    dim: true
                                                    font.pixelSize: Tokens.smallSize
                                                    font.family: block.modelData.command ? "monospace" : Tokens.sans
                                                    elide: Text.ElideRight
                                                    visible: text !== ""
                                                }
                                            }

                                            Icon {
                                                id: go
                                                anchors.right: parent.right
                                                anchors.rightMargin: 10
                                                anchors.verticalCenter: parent.verticalCenter
                                                name: "arrow-square-out"
                                                font.pixelSize: Tokens.iconSize - 4
                                                opacity: block.hovered && block.active ? 1 : 0
                                                width: block.active ? implicitWidth : 0

                                                Behavior on opacity {
                                                    NumberAnimation { duration: Tokens.fast }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // --- Lo buscado y lo que contesta Veyl ---
                    Flickable {
                        id: results
                        anchors.fill: parent
                        contentHeight: found.height
                        clip: true
                        boundsBehavior: Flickable.StopAtBounds
                        opacity: Recall.searching || Recall.answer ? 1 : 0
                        visible: opacity > 0.01
                        enabled: visible

                        Behavior on opacity {
                            NumberAnimation { duration: Tokens.fast }
                        }

                        Column {
                            id: found
                            width: results.width
                            spacing: 8

                            // La respuesta de Veyl: lo que dice y, si la compuso, su tarjeta.
                            Sheet {
                                width: parent.width
                                height: reply.height + 24
                                visible: !!Recall.answer

                                Column {
                                    id: reply
                                    x: 16
                                    y: 12
                                    width: parent.width - 32
                                    spacing: 10

                                    Item {
                                        width: parent.width
                                        height: Tokens.knob

                                        Icon {
                                            id: spark
                                            anchors.verticalCenter: parent.verticalCenter
                                            name: "sparkle"
                                            color: Tokens.accent
                                        }

                                        Ink {
                                            anchors.left: spark.right
                                            anchors.leftMargin: 10
                                            anchors.right: close.left
                                            anchors.rightMargin: 10
                                            anchors.verticalCenter: parent.verticalCenter
                                            text: Recall.answer ? `«${Recall.answer.said}»` : ""
                                            dim: true
                                            font.pixelSize: Tokens.smallSize
                                            elide: Text.ElideRight
                                        }

                                        Round {
                                            id: close
                                            anchors.right: parent.right
                                            anchors.verticalCenter: parent.verticalCenter
                                            icon: "x-circle"
                                            onTapped: {
                                                Recall.forget();
                                                field.forceActiveFocus();
                                            }
                                        }
                                    }

                                    // Mientras piensa: los tres puntos del acento, en ola.
                                    Row {
                                        spacing: 4
                                        visible: Recall.pending

                                        Repeater {
                                            model: 3

                                            Rectangle {
                                                id: dot

                                                required property int index

                                                width: 6
                                                height: 6
                                                radius: 3
                                                color: Tokens.accent
                                                opacity: 0.25

                                                SequentialAnimation on opacity {
                                                    running: Recall.pending && Recall.open
                                                    loops: Animation.Infinite
                                                    PauseAnimation { duration: dot.index * 160 }
                                                    NumberAnimation { to: 1; duration: 320; easing.type: Easing.OutSine }
                                                    NumberAnimation { to: 0.25; duration: 480; easing.type: Easing.InSine }
                                                    PauseAnimation { duration: (2 - dot.index) * 160 }
                                                }
                                            }
                                        }
                                    }

                                    Ink {
                                        width: parent.width
                                        visible: !Recall.pending && text !== ""
                                        text: Recall.answer ? [Recall.answer.say, Recall.answer.card ? "" : Recall.answer.more].filter(x => x).join("\n") : ""
                                        color: Recall.answer && !Recall.answer.ok ? Tokens.critical : Tokens.text
                                        wrapMode: Text.WordWrap
                                        lineHeight: 1.15
                                    }

                                    Card {
                                        id: answer
                                        width: Math.min(implicitWidth, parent.width)
                                        height: implicitHeight
                                        card: Recall.answer?.card ?? null
                                        room: parent.width
                                        limit: 2000
                                        visible: !Recall.pending && !!Recall.answer?.card
                                        // Lo que se pregunta después se contesta aquí mismo.
                                        follow: text => {
                                            Recall.query = text;
                                            Recall.ask();
                                        }
                                        onDone: Recall.forget()
                                    }
                                }
                            }

                            // Lo que sale tal cual en el diario, lo último primero.
                            Sheet {
                                width: parent.width
                                height: hits.height + 8
                                visible: Recall.searching

                                Column {
                                    id: hits
                                    x: 6
                                    y: 2
                                    width: parent.width - 12

                                    Heading {
                                        text: Recall.hits.length > 0 ? "En el diario, tal cual" : ""
                                        aside: Recall.hits.length > 0 ? "pulsa uno para ir a ese rato" : ""
                                        visible: Recall.hits.length > 0
                                    }

                                    Repeater {
                                        model: Recall.hits

                                        Press {
                                            id: hit

                                            required property var modelData
                                            readonly property int day: Calendar.at(modelData.start)

                                            width: hits.width
                                            height: 52
                                            radius: root.card - 6
                                            onTapped: {
                                                root.aim = modelData.start;
                                                Recall.query = "";
                                                Recall.forget();
                                                if (day === Recall.day)
                                                    root.reach();
                                                else
                                                    Recall.go(day);
                                            }

                                            Column {
                                                x: 6
                                                width: 84
                                                anchors.verticalCenter: parent.verticalCenter

                                                Ink {
                                                    width: parent.width
                                                    text: root.dayName(hit.day).replace(/ \d+ de .*/, "")
                                                    font.pixelSize: Tokens.smallSize
                                                    elide: Text.ElideRight
                                                }

                                                Ink {
                                                    width: parent.width
                                                    text: hit.modelData.end - hit.modelData.start >= 60000 ? `${root.clock(hit.modelData.start)}–${root.clock(hit.modelData.end)}` : root.clock(hit.modelData.start)
                                                    dim: true
                                                    font.pixelSize: Tokens.tinySize
                                                }
                                            }

                                            Mark {
                                                id: hitMark
                                                x: 96
                                                anchors.verticalCenter: parent.verticalCenter
                                                app: hit.modelData.kind === "focus" ? hit.modelData.app : ""
                                                glyph: hit.modelData.kind === "media" ? "music-notes" : hit.modelData.kind === "file" ? "file" : "app-window"
                                            }

                                            Column {
                                                anchors.left: hitMark.right
                                                anchors.leftMargin: 10
                                                anchors.right: out.left
                                                anchors.rightMargin: 8
                                                anchors.verticalCenter: parent.verticalCenter
                                                spacing: 2

                                                Ink {
                                                    width: parent.width
                                                    text: root.tidy(hit.modelData.title) || hit.modelData.label
                                                    font.pixelSize: Tokens.smallSize + 1
                                                    elide: Text.ElideRight
                                                }

                                                Ink {
                                                    width: parent.width
                                                    text: [hit.modelData.label, hit.modelData.place.startsWith("/") ? root.tilde(hit.modelData.place) : hit.modelData.place].filter(x => x).join(" · ")
                                                    dim: true
                                                    font.pixelSize: Tokens.tinySize
                                                    elide: Text.ElideMiddle
                                                }
                                            }

                                            Round {
                                                id: out
                                                anchors.right: parent.right
                                                anchors.rightMargin: 6
                                                anchors.verticalCenter: parent.verticalCenter
                                                icon: "arrow-square-out"
                                                visible: hit.modelData.url !== "" || hit.modelData.kind === "file"
                                                opacity: hit.hovered || hovered ? 1 : 0.4
                                                onTapped: root.openLink(hit.modelData.kind === "file" ? `file://${hit.modelData.place}` : hit.modelData.url)
                                            }
                                        }
                                    }

                                    Empty {
                                        visible: Recall.hits.length === 0 && Recall.searched !== "" && !Recall.answer
                                        icon: "magnifying-glass"
                                        text: `«${Recall.searched}» no sale tal cual en el diario`
                                        hint: Daemon.thinks ? "Pulsa Intro y Veyl lo buscará por lo que significa, también en otros días." : "Prueba con otras palabras."
                                    }
                                }
                            }

                            Item {
                                width: 1
                                height: 4
                            }
                        }
                    }
                }
            }
        }
    }
}
