import QtQuick
import Quickshell
import Quickshell.Services.Pipewire
import qs
import qs.state

// El centro de control: lo que enseña la isla del centro cuando el puntero
// sube al borde de arriba. Arriba, la fecha y las páginas; la isla cambia de
// forma con cada una:
// - Inicio: los interruptores, el sonido, lo que suena, lo próximo y el
//   temporizador, los agentes con lo que les queda de cuota y, al pie, los
//   atajos y la sesión.
// - Veyl: lo que le has pedido, por días; cada consulta se abre con su
//   respuesta, se vuelve a preguntar o se olvida.
// - Avisos: todas las notificaciones guardadas.
// El engranaje de arriba la convierte en los ajustes.
Item {
    id: root

    readonly property int card: Tokens.round - Tokens.inset  // radio de lo que va dentro
    readonly property int space: 8                            // entre tarjetas
    readonly property real inner: width - Tokens.inset * 2
    readonly property real half: (inner - space) / 2
    readonly property var player: Commands.player
    readonly property int list: 420  // alto de una lista, como mucho: lo demás se desplaza
    property bool open: false
    // Lo peligroso pide dos pulsaciones: la orden que espera la segunda.
    property string armed: ""
    property bool slid: false  // arrastrando un volumen: no se cierra a medias
    readonly property bool busy: armed !== "" || slid
    property double clock: Date.now()
    // La consulta abierta del historial (su id), o -1; y qué fuentes se ven.
    property int opened: -1
    property string only: ""
    signal done     // algo se ha cumplido fuera: el centro sobra
    signal touched  // se ha pulsado algo dentro: la isla puede cambiar de tamaño bajo el puntero

    function order(id: string): var {
        return Commands.list.find(order => order.key === `order:${id}`) ?? null;
    }

    function press(order: var): void {
        if (order.confirm && armed !== order.key) {
            armed = order.key;
            disarm.restart();
            return;
        }
        armed = "";
        order.run();
        done();
    }

    // Lo que hace algo sobre la pantalla espera a que el centro se haya ido.
    function shoot(order: var): void {
        done();
        order.run();
    }

    function go(page: string): void {
        if (Center.page === page)
            return;
        touched();
        Center.page = page;
    }

    // Al abrirse: lo que hay que preguntar fuera, y nada a medio confirmar.
    function reset(): void {
        armed = "";
        opened = -1;
        clock = Date.now();
        Commands.refresh();
        Daemon.history();
        Daemon.quota();
        history.contentY = 0;
        shelf.contentY = 0;
    }

    // Vuelve a la entrada con lo que se dijo, para cambiarlo o repetirlo con Intro.
    function again(said: string): void {
        done();
        Launcher.show("");
        Launcher.query = said;
    }

    function capital(text: string): string {
        return text.charAt(0).toUpperCase() + text.slice(1);
    }

    // Cuándo es, contado desde ahora
    function when(event: var): string {
        const day = Calendar.at(event.at);
        const minutes = Math.ceil((event.at - Agenda.now) / 60000);
        if (!event.allDay && minutes <= 0)
            return "ahora";
        if (!event.allDay && minutes < 60)
            return `en ${minutes} min`;
        const away = day - Calendar.today;
        const name = away === 0 ? "hoy" : away === 1 ? "mañana" : Calendar.format(day, away < 7 ? "ddd d" : "d MMM").replace(".", "");
        return event.allDay ? name : `${name} · ${Agenda.clock(event.at)}`;
    }

    // Cuánto hace, en corto
    function ago(at: double): string {
        const minutes = Math.floor((clock - at) / 60000);
        return minutes < 1 ? "ahora" : minutes < 60 ? `${minutes} min` : minutes < 1440 ? `${Math.floor(minutes / 60)} h` : `${Math.floor(minutes / 1440)} d`;
    }

    // Cuánto falta, en corto: «2 h 10 min», «3 d».
    function until(at: double): string {
        const minutes = Math.max(1, Math.round((at - clock) / 60000));
        if (minutes < 60)
            return `${minutes} min`;
        if (minutes < 1440)
            return minutes % 60 && minutes < 600 ? `${Math.floor(minutes / 60)} h ${minutes % 60} min` : `${Math.round(minutes / 60)} h`;
        return `${Math.round(minutes / 1440)} d`;
    }

    // El nombre de un día del historial: hoy, ayer o su fecha.
    function dayName(day: int): string {
        const away = Calendar.today - day;
        if (away === 0)
            return "Hoy";
        if (away === 1)
            return "Ayer";
        return capital(Calendar.format(day, away < 7 ? "dddd" : "dddd d 'de' MMMM").replace(".", ""));
    }

    readonly property var pages: [
        { id: "home", title: "Inicio", count: 0 },
        { id: "veyl", title: "Veyl", count: 0 },
        { id: "notes", title: "Avisos", count: Notifications.notes.length }
    ]

    readonly property var switches: {
        const list = [];
        const wifi = order("wifi");
        if (wifi)
            list.push({ icon: Commands.wifi ? "wifi-high" : "wifi-slash", title: "Wi-Fi", state: Network.offline ? "sin red" : Commands.wifi ? "encendido" : "apagado", on: Commands.wifi, run: () => wifi.run() });
        if (Commands.bluetooth)
            list.push({ icon: Commands.bluetooth.enabled ? "bluetooth" : "bluetooth-slash", title: "Bluetooth", state: Commands.bluetooth.enabled ? "encendido" : "apagado", on: Commands.bluetooth.enabled, run: () => Commands.bluetooth.enabled = !Commands.bluetooth.enabled });
        list.push({ icon: Notifications.quiet ? "bell-slash" : "bell", title: "No molestar", state: Notifications.quiet ? "solo lo crítico" : "apagado", on: Notifications.quiet, run: () => Settings.quiet = !Settings.quiet });
        list.push({ icon: "waveform", title: "Escuchar", state: Listen.active ? "escuchando" : "apagado", on: Listen.active, run: () => Listen.toggle() });
        return list;
    }

    // Lo que viene: los próximos eventos, de hoy en adelante.
    readonly property var coming: Agenda.ahead.slice(0, 3)
    readonly property int working: Agents.rows.filter(row => row.phase === "working" || row.phase === "needs").length
    readonly property var shortcuts: [["shot-area", "Recortar zona"], ["shot-screen", "Captura"], ["picker", "Color"]].map(([id, name]) => ({ order: order(id), name })).filter(item => item.order)
    readonly property var session: ["lock", "suspend", "logout", "reboot", "poweroff"].map(id => order(id)).filter(order => order)

    // El historial, por días: un título por día y sus consultas debajo.
    readonly property var asked: Daemon.asked.filter(row => !only || (only === "voice") === (row.source === "voice"))
    readonly property var entries: {
        const out = [];
        let last = null;
        for (const row of asked) {
            const day = Calendar.at(row.time);
            if (day !== last) {
                out.push({ kind: "day", title: dayName(day), row: null });
                last = day;
            }
            out.push({ kind: "row", title: "", row });
        }
        return out;
    }
    readonly property var notes: Center.page === "notes" || open ? Notifications.notes.map(note => Notifications.read(note)) : []

    implicitWidth: Tokens.controls
    implicitHeight: column.height + Tokens.inset

    Timer {
        interval: 30000
        running: root.open
        repeat: true
        onTriggered: root.clock = Date.now()
    }

    // La cuota cambia mientras los agentes trabajan: se vuelve a mirar.
    Timer {
        interval: 60000
        running: root.open
        repeat: true
        onTriggered: Daemon.quota()
    }

    Timer {
        id: disarm
        interval: 3000
        onTriggered: root.armed = ""
    }

    // Una tarjeta: la superficie sobre la placa.
    component Card: Rectangle {
        radius: root.card
        color: Tokens.surface
    }

    // El título de una tarjeta, pequeño y apagado; a la derecha, lo que se
    // le pase (una acción).
    component Heading: Item {
        property alias text: label.text
        default property alias extra: aside.data

        width: parent.width
        height: 32

        Ink {
            id: label
            x: 6
            anchors.verticalCenter: parent.verticalCenter
            dim: true
            font.pixelSize: Tokens.smallSize
        }

        Row {
            id: aside
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: 4
        }
    }

    // Una píldora que se pulsa: un icono y, si lo hay, su texto. Encendida
    // (`on`), del acento.
    component Chip: Press {
        id: chip

        property string icon
        property string text
        property bool lit: false
        property color base: Tokens.surface

        width: text ? row.width + 22 : height
        height: Tokens.knob + 2
        radius: height / 2
        rest: lit ? Tokens.accent : base

        Row {
            id: row
            anchors.centerIn: parent
            spacing: 6

            Icon {
                anchors.verticalCenter: parent.verticalCenter
                name: chip.icon
                color: chip.lit ? Tokens.bg : Tokens.dim
                font.pixelSize: Tokens.iconSize - 2
                visible: chip.icon !== ""
            }

            Ink {
                anchors.verticalCenter: parent.verticalCenter
                text: chip.text
                color: chip.lit ? Tokens.bg : Tokens.text
                font.pixelSize: Tokens.smallSize
                visible: chip.text !== ""
            }
        }
    }

    // Un botón redondo con el mismo tacto que lo demás.
    component Round: Press {
        id: round

        property alias icon: glyph.name
        property color tint: Tokens.dim

        width: Tokens.knob
        height: Tokens.knob
        radius: width / 2

        Icon {
            id: glyph
            anchors.centerIn: parent
            color: round.tint
            font.pixelSize: Tokens.iconSize - 2
        }
    }

    // Lo que se va y lo que viene en el mismo sitio, sin saltos: lo de reposo
    // se apaga y lo otro se enciende.
    component Swap: Item {
        id: swap

        property bool flipped: false
        property Item rest
        property Item lit

        Binding {
            target: swap.rest
            property: "opacity"
            value: swap.flipped ? 0 : 1
        }

        Binding {
            target: swap.lit
            property: "opacity"
            value: swap.flipped ? 1 : 0
        }
    }

    // Un interruptor: encendido, su icono va sobre el acento, que es lo tuyo.
    component Switch: Press {
        id: tile

        required property var modelData

        width: (root.inner - 6 * (root.switches.length - 1)) / root.switches.length
        height: Tokens.toggle
        radius: root.card
        rest: Tokens.surface
        onTapped: tile.modelData.run()

        Rectangle {
            id: badge
            x: 10
            anchors.verticalCenter: parent.verticalCenter
            width: Tokens.knob + 4
            height: width
            radius: width / 2
            color: tile.modelData.on ? Tokens.accent : Tokens.bg

            Behavior on color {
                ColorAnimation { duration: Tokens.fast }
            }

            Icon {
                anchors.centerIn: parent
                name: tile.modelData.icon
                color: tile.modelData.on ? Tokens.bg : Tokens.dim
                font.pixelSize: Tokens.iconSize - 2
            }
        }

        Column {
            anchors.left: badge.right
            anchors.leftMargin: 10
            anchors.right: parent.right
            anchors.rightMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            spacing: 1

            Ink {
                width: parent.width
                text: tile.modelData.title
                elide: Text.ElideRight
            }

            Ink {
                width: parent.width
                text: tile.modelData.state
                color: tile.modelData.on ? Tokens.accent : Tokens.dim
                font.pixelSize: Tokens.smallSize
                elide: Text.ElideRight
            }
        }
    }

    // Un volumen: el botón lo silencia; la barra se arrastra, se pulsa o se
    // mueve con la rueda, a saltos de Sound.step. Su punto siempre se ve, y
    // crece con el puntero encima.
    component Level: Item {
        id: level

        property PwNode node
        property string icon
        property string silent
        readonly property bool muted: node?.audio?.muted ?? false
        readonly property real value: Math.min(1, node?.audio?.volume ?? 0)
        readonly property bool near: drag.containsMouse || drag.pressed

        function set(x: real): void {
            if (!node?.audio)
                return;
            node.audio.volume = Math.max(0, Math.min(1, x / track.width));
            if (node.audio.volume > 0)
                node.audio.muted = false;
        }

        height: 46

        Round {
            id: mute
            anchors.verticalCenter: parent.verticalCenter
            icon: level.muted ? level.silent : level.icon
            tint: level.muted ? Tokens.critical : Tokens.text
            onTapped: Sound.mute(level.node)
        }

        Ink {
            anchors.left: track.left
            anchors.right: amount.right
            y: 5
            text: Sound.label(level.node)
            dim: true
            font.pixelSize: Tokens.smallSize
            elide: Text.ElideRight
        }

        Item {
            id: track
            anchors.left: mute.right
            anchors.leftMargin: 8
            anchors.right: amount.left
            anchors.rightMargin: 8
            y: 29
            height: level.near ? 8 : 6

            Behavior on height {
                NumberAnimation { duration: Tokens.fast; easing.type: Easing.OutCubic }
            }

            Rectangle {
                anchors.fill: parent
                radius: height / 2
                color: Tokens.border
            }

            Rectangle {
                width: Math.max(height, parent.width * level.value)
                height: parent.height
                radius: height / 2
                color: level.muted ? Tokens.dim : Tokens.accent
                visible: level.value > 0

                Behavior on width {
                    enabled: !drag.pressed
                    NumberAnimation { duration: Tokens.fast; easing.type: Easing.OutCubic }
                }
            }

            Rectangle {
                x: parent.width * level.value - width / 2
                anchors.verticalCenter: parent.verticalCenter
                width: level.near ? 16 : 10
                height: width
                radius: width / 2
                color: Tokens.text

                Behavior on width {
                    NumberAnimation { duration: Tokens.fast; easing.type: Easing.OutCubic }
                }
            }

            MouseArea {
                id: drag
                anchors.fill: parent
                anchors.margins: -10
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onPressed: event => level.set(event.x - 10)
                onPositionChanged: event => {
                    if (pressed)
                        level.set(event.x - 10);
                }
                onPressedChanged: root.slid = pressed
            }

            WheelHandler {
                target: null
                property real spin: 0

                onWheel: event => {
                    spin += event.angleDelta.y;
                    while (Math.abs(spin) >= 120) {
                        const way = Math.sign(spin);
                        spin -= way * 120;
                        Sound.nudge(level.node, way);
                    }
                }
            }
        }

        Ink {
            id: amount
            anchors.right: parent.right
            anchors.verticalCenter: track.verticalCenter
            width: 28
            horizontalAlignment: Text.AlignRight
            text: level.muted ? "—" : Math.round((level.node?.audio?.volume ?? 0) * 100)
            dim: level.muted
            font.pixelSize: Tokens.smallSize
        }
    }

    // Un evento que viene: cuándo y su título. Lo inminente, con la barra
    // del aviso; lo de hoy, del acento; lo demás, apagado.
    component Event: Item {
        id: event

        required property var modelData
        readonly property bool soon: !modelData.allDay && modelData.at - Agenda.now <= Agenda.lead * 60000
        readonly property bool today: Calendar.at(modelData.at) === Calendar.today

        width: parent.width
        height: 38

        Rectangle {
            id: bar
            x: 6
            anchors.verticalCenter: parent.verticalCenter
            width: 3
            height: 24
            radius: 1.5
            color: event.soon ? Tokens.attention : event.today ? Tokens.accent : Tokens.border
        }

        Column {
            anchors.left: bar.right
            anchors.leftMargin: 10
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter

            Ink {
                width: parent.width
                text: event.modelData.title
                elide: Text.ElideRight
            }

            Ink {
                width: parent.width
                text: root.when(event.modelData)
                color: event.soon ? Tokens.attention : Tokens.dim
                font.pixelSize: Tokens.tinySize
                elide: Text.ElideRight
            }
        }
    }

    // Una página: entra deslizándose desde su lado (Inicio a la izquierda,
    // Avisos a la derecha) mientras la de antes se va hacia el otro.
    component Page: Column {
        required property string name
        readonly property int place: Center.pages.indexOf(name)
        readonly property bool current: Center.page === name

        width: pages.width
        spacing: root.space
        opacity: current ? 1 : 0
        x: current ? 0 : place < Center.pages.indexOf(Center.page) ? -32 : 32
        visible: opacity > 0.01
        enabled: current

        Behavior on opacity {
            NumberAnimation { duration: Tokens.veil * 0.8; easing.type: Easing.OutCubic }
        }

        Behavior on x {
            NumberAnimation { duration: Tokens.veil; easing.type: Easing.OutCubic }
        }
    }

    // Lo que se ve cuando una lista está vacía.
    component Empty: Item {
        property alias icon: glyph.name
        property alias text: title.text
        property alias hint: detail.text

        width: parent.width
        height: 132

        Column {
            anchors.centerIn: parent
            width: parent.width - 48
            spacing: 6

            Icon {
                id: glyph
                anchors.horizontalCenter: parent.horizontalCenter
                font.pixelSize: 28
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

    Column {
        id: column
        width: parent.width
        spacing: root.space

        // La fecha, grande, en el sitio de la fila de una isla; a la derecha,
        // las páginas y el engranaje.
        Item {
            width: parent.width
            height: 54

            Text {
                id: number
                x: Tokens.pad + 2
                anchors.verticalCenter: parent.verticalCenter
                text: Clock.now.getDate()
                color: Tokens.accent
                font.family: Tokens.sans
                font.pixelSize: 34
                font.weight: Font.DemiBold
                font.features: ({ "tnum": 1 })
            }

            Column {
                anchors.left: number.right
                anchors.leftMargin: 12
                anchors.verticalCenter: parent.verticalCenter

                Ink {
                    text: root.capital(Clock.now.toLocaleDateString(Qt.locale(), "dddd"))
                    strong: true
                }

                Row {
                    spacing: 8

                    Ink {
                        text: Clock.now.toLocaleDateString(Qt.locale(), "MMMM 'de' yyyy")
                        dim: true
                        font.pixelSize: Tokens.smallSize
                    }

                    Label {
                        anchors.verticalCenter: parent.verticalCenter
                        visible: Battery.present
                        icon: !Battery.discharging ? "battery-charging" : Battery.critical ? "battery-warning" : Battery.low ? "battery-low" : Battery.percent > 80 ? "battery-full" : Battery.percent > 50 ? "battery-high" : "battery-medium"
                        tone: Battery.critical ? Tokens.critical : Battery.low ? Tokens.attention : Tokens.dim
                        text: `${Battery.percent} %`
                        dim: true
                        spacing: 4
                        scale: 0.86
                        transformOrigin: Item.Left
                    }
                }
            }

            Row {
                anchors.right: parent.right
                anchors.rightMargin: Tokens.pad - 6
                anchors.verticalCenter: parent.verticalCenter
                spacing: 8

                // Las páginas: la actual, con su placa que se desliza de una a otra.
                Rectangle {
                    id: tabs
                    anchors.verticalCenter: parent.verticalCenter
                    width: strip.width + 6
                    height: 34
                    radius: height / 2
                    color: Tokens.surface

                    readonly property Item current: tabbed.count, tabbed.itemAt(Center.pages.indexOf(Center.page))

                    Rectangle {
                        x: 3 + (tabs.current?.x ?? 0)
                        y: 3
                        width: tabs.current?.width ?? 0
                        height: parent.height - 6
                        radius: height / 2
                        color: Tokens.border

                        Behavior on x {
                            NumberAnimation { duration: Tokens.veil; easing.type: Easing.OutCubic }
                        }

                        Behavior on width {
                            NumberAnimation { duration: Tokens.veil; easing.type: Easing.OutCubic }
                        }
                    }

                    Row {
                        id: strip
                        x: 3
                        y: 3

                        Repeater {
                            id: tabbed
                            model: root.pages

                            Press {
                                id: tab

                                required property var modelData
                                readonly property string page: modelData.id
                                readonly property bool here: Center.page === page

                                width: words.width + 26
                                height: tabs.height - 6
                                radius: height / 2
                                active: !here
                                onTapped: root.go(page)

                                Row {
                                    id: words
                                    anchors.centerIn: parent
                                    spacing: 6

                                    Ink {
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: tab.modelData.title
                                        color: tab.here ? Tokens.text : Tokens.dim
                                        font.pixelSize: Tokens.smallSize
                                        font.weight: tab.here ? Font.DemiBold : Font.Medium

                                        Behavior on color {
                                            ColorAnimation { duration: Tokens.fast }
                                        }
                                    }

                                    Rectangle {
                                        anchors.verticalCenter: parent.verticalCenter
                                        visible: tab.modelData.count > 0
                                        width: Math.max(height, count.implicitWidth + 10)
                                        height: 17
                                        radius: height / 2
                                        color: tab.here ? Tokens.accent : Tokens.bg

                                        Ink {
                                            id: count
                                            anchors.centerIn: parent
                                            text: tab.modelData.count > 99 ? "99+" : tab.modelData.count
                                            color: tab.here ? Tokens.bg : Tokens.dim
                                            font.pixelSize: Tokens.tinySize
                                            font.weight: Font.DemiBold
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // El diario: esta misma isla crece hasta ser la suya.
                Round {
                    anchors.verticalCenter: parent.verticalCenter
                    width: 34
                    height: 34
                    icon: "notebook"
                    visible: Daemon.connected
                    onTapped: {
                        Recall.origin = Qt.size(root.width, root.height);
                        Recall.show("", "", "");
                        root.done();
                    }
                }

                // Los ajustes: esta misma isla crece hasta ser la suya.
                Round {
                    anchors.verticalCenter: parent.verticalCenter
                    width: 34
                    height: 34
                    icon: "gear-six"
                    onTapped: {
                        Tuning.origin = Qt.size(root.width, root.height);
                        Tuning.show("", "");
                        root.done();
                    }
                }
            }
        }

        Item {
            id: pages
            x: Tokens.inset
            width: root.inner
            height: (Center.page === "veyl" ? veyl : Center.page === "notes" ? notices : home).height

            // --- Inicio ---
            Page {
                id: home
                name: "home"

                Row {
                    spacing: 6

                    Repeater {
                        model: root.switches

                        Switch {}
                    }
                }

                // El sonido: la salida y el micro, uno junto al otro.
                Card {
                    width: parent.width
                    height: 58
                    visible: Sound.ready || Sound.micReady

                    Row {
                        x: 8
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 20

                        readonly property real each: (parent.width - 16 - (Sound.ready && Sound.micReady ? spacing : 0)) / (Sound.ready && Sound.micReady ? 2 : 1)

                        Level {
                            width: parent.each
                            visible: Sound.ready
                            node: Sound.sink
                            icon: Sound.volume > 50 ? "speaker-high" : Sound.volume > 0 ? "speaker-low" : "speaker-none"
                            silent: "speaker-x"
                        }

                        Level {
                            width: parent.each
                            visible: Sound.micReady
                            node: Sound.source
                            icon: "microphone"
                            silent: "microphone-slash"
                        }
                    }
                }

                // Lo que suena
                Card {
                    id: media
                    width: parent.width
                    height: 76
                    clip: true
                    visible: !!root.player

                    readonly property bool timed: !!root.player && root.player.lengthSupported && root.player.length > 0

                    // La posición no avisa sola: se le pregunta mientras suena.
                    Timer {
                        interval: 1000
                        running: root.open && media.visible && media.timed && (root.player?.isPlaying ?? false)
                        repeat: true
                        onTriggered: root.player.positionChanged()
                    }

                    Clip {
                        id: art
                        x: 12
                        anchors.verticalCenter: parent.verticalCenter
                        width: 52
                        height: 52
                        radius: Tokens.radius
                        color: Tokens.border

                        Icon {
                            anchors.centerIn: parent
                            name: "music-notes"
                            visible: cover.status !== Image.Ready
                        }

                        Image {
                            id: cover
                            anchors.fill: parent
                            source: root.player?.trackArtUrl ?? ""
                            sourceSize: Qt.size(width * 2, height * 2)
                            fillMode: Image.PreserveAspectCrop
                            asynchronous: true
                        }
                    }

                    Column {
                        anchors.left: art.right
                        anchors.leftMargin: 14
                        anchors.right: buttons.left
                        anchors.rightMargin: 8
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 1

                        Ink {
                            width: parent.width
                            text: root.player?.trackTitle || root.player?.identity || ""
                            strong: true
                            elide: Text.ElideRight
                        }

                        Ink {
                            width: parent.width
                            text: root.player?.trackArtist || root.player?.identity || ""
                            dim: true
                            font.pixelSize: Tokens.smallSize
                            elide: Text.ElideRight
                        }
                    }

                    Row {
                        id: buttons
                        anchors.right: parent.right
                        anchors.rightMargin: 12
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 4

                        Round {
                            anchors.verticalCenter: parent.verticalCenter
                            icon: "skip-back"
                            visible: root.player?.canGoPrevious ?? false
                            onTapped: root.player.previous()
                        }

                        Round {
                            width: Tokens.knob + 8
                            height: width
                            icon: root.player?.isPlaying ? "pause" : "play"
                            tint: Tokens.bg
                            rest: Tokens.text
                            visible: root.player?.canTogglePlaying ?? false
                            onTapped: root.player.togglePlaying()
                        }

                        Round {
                            anchors.verticalCenter: parent.verticalCenter
                            icon: "skip-forward"
                            visible: root.player?.canGoNext ?? false
                            onTapped: root.player.next()
                        }
                    }

                    // Lo que lleva sonado
                    Rectangle {
                        anchors.bottom: parent.bottom
                        width: media.timed ? parent.width * Math.min(1, root.player.position / root.player.length) : 0
                        height: 2
                        color: Tokens.accent
                        visible: media.timed

                        Behavior on width {
                            NumberAnimation { duration: 1000 }
                        }
                    }
                }

                // A dos columnas: lo próximo y el temporizador, igual de altos.
                Row {
                    id: pair
                    spacing: root.space

                    readonly property real tall: Math.max(agenda.height + 8, 150)

                    Card {
                        width: root.half
                        height: pair.tall

                        Column {
                            id: agenda
                            x: 6
                            y: 2
                            width: parent.width - 12

                            Heading {
                                text: "Próximo"
                            }

                            Repeater {
                                model: root.coming

                                Event {}
                            }

                            Ink {
                                x: 6
                                height: 38
                                verticalAlignment: Text.AlignVCenter
                                text: "Nada por delante"
                                dim: true
                                visible: root.coming.length === 0
                            }
                        }
                    }

                    // El temporizador: en marcha, lo que queda y pararlo; si
                    // no, la duración (con la rueda) y tres para empezar ya.
                    Card {
                        width: root.half
                        height: pair.tall
                        clip: true

                        Heading {
                            x: 6
                            y: 2
                            width: parent.width - 12
                            text: Countdown.running ? `Temporizador · de ${Countdown.span} min` : "Temporizador"
                        }

                        Text {
                            x: 14
                            anchors.verticalCenter: parent.verticalCenter
                            anchors.verticalCenterOffset: -2
                            text: Countdown.running ? Countdown.exact : `${Countdown.preset}:00`
                            color: Countdown.running ? Tokens.text : Tokens.dim
                            font.family: Tokens.sans
                            font.pixelSize: 34
                            font.weight: Countdown.running ? Font.DemiBold : Font.Normal
                            font.features: ({ "tnum": 1 })

                            WheelHandler {
                                enabled: !Countdown.running
                                property real spin: 0

                                onWheel: event => {
                                    spin += event.angleDelta.y;
                                    while (Math.abs(spin) >= 120) {
                                        const way = Math.sign(spin);
                                        spin -= way * 120;
                                        Countdown.nudge(way);
                                    }
                                }
                            }
                        }

                        Row {
                            x: 10
                            anchors.bottom: parent.bottom
                            anchors.bottomMargin: 12
                            spacing: 6

                            Repeater {
                                model: Countdown.running ? [] : [5, 15, 25]

                                Chip {
                                    required property int modelData
                                    anchors.verticalCenter: parent.verticalCenter
                                    base: Tokens.bg
                                    text: `${modelData} min`
                                    onTapped: Countdown.start(modelData)
                                }
                            }
                        }

                        Round {
                            anchors.right: parent.right
                            anchors.rightMargin: 10
                            anchors.bottom: parent.bottom
                            anchors.bottomMargin: 10
                            width: 36
                            height: 36
                            icon: Countdown.running ? "stop" : "play"
                            tint: Countdown.running ? Tokens.text : Tokens.bg
                            rest: Countdown.running ? Tokens.border : Tokens.accent
                            onTapped: Countdown.running ? Countdown.stop() : Countdown.start(Countdown.preset)
                        }

                        // Lo que lleva hecho
                        Rectangle {
                            anchors.bottom: parent.bottom
                            width: parent.width * Countdown.progress
                            height: 2
                            color: Tokens.accent
                            visible: Countdown.running

                            Behavior on width {
                                NumberAnimation { duration: 1000 }
                            }
                        }
                    }
                }

                // Los agentes: lo que les queda de cuota a sus suscripciones y
                // los que trabajan o te necesitan (pulsar uno lleva a él).
                Card {
                    width: parent.width
                    height: agents.height + 8
                    visible: root.working > 0 || Daemon.usage.length > 0

                    Column {
                        id: agents
                        x: 6
                        y: 2
                        width: parent.width - 12

                        Heading {
                            text: root.working > 0 ? `Agentes · ${root.working} en marcha` : "Agentes"
                        }

                        Repeater {
                            model: Daemon.usage

                            Item {
                                id: plan

                                required property var modelData
                                // La ventana que más aprieta: la que menos deja.
                                readonly property var tight: modelData.windows.reduce((a, b) => b.used > a.used ? b : a)

                                width: agents.width
                                height: 46

                                Logo {
                                    id: mark
                                    x: 14
                                    anchors.verticalCenter: parent.verticalCenter
                                    source: Agents.logo(plan.modelData.provider)
                                    color: Agents.vendor(plan.modelData.provider)
                                }

                                Column {
                                    anchors.left: mark.right
                                    anchors.leftMargin: 18
                                    anchors.right: windows.left
                                    anchors.rightMargin: 12
                                    anchors.verticalCenter: parent.verticalCenter

                                    Ink {
                                        width: parent.width
                                        text: plan.modelData.name
                                        elide: Text.ElideRight
                                    }

                                    Ink {
                                        width: parent.width
                                        text: plan.tight.resets ? `${plan.tight.label === "Semana" ? "la semana" : "el tramo de " + plan.tight.label} se renueva en ${root.until(plan.tight.resets)}` : plan.modelData.at ? `visto hace ${root.ago(plan.modelData.at)}` : ""
                                        dim: true
                                        font.pixelSize: Tokens.tinySize
                                        elide: Text.ElideRight
                                        visible: text !== ""
                                    }
                                }

                                // Lo que queda de cada ventana: su barra se vacía al gastarse.
                                Row {
                                    id: windows
                                    anchors.right: parent.right
                                    anchors.rightMargin: 10
                                    anchors.verticalCenter: parent.verticalCenter
                                    spacing: 14

                                    Repeater {
                                        model: plan.modelData.windows

                                        Column {
                                            id: span

                                            required property var modelData
                                            readonly property real spare: Math.max(0, 100 - modelData.used)
                                            readonly property color tone: spare < 15 ? Tokens.critical : spare < 35 ? Tokens.attention : Agents.vendor(plan.modelData.provider)

                                            width: 96
                                            spacing: 5

                                            Item {
                                                width: parent.width
                                                height: 14

                                                Ink {
                                                    text: span.modelData.label
                                                    dim: true
                                                    font.pixelSize: Tokens.tinySize
                                                }

                                                Ink {
                                                    anchors.right: parent.right
                                                    text: `${Math.round(span.spare)} %`
                                                    color: span.spare < 35 ? span.tone : Tokens.text
                                                    font.pixelSize: Tokens.tinySize
                                                    font.weight: Font.DemiBold
                                                }
                                            }

                                            Rectangle {
                                                width: parent.width
                                                height: 4
                                                radius: 2
                                                color: Tokens.border

                                                Rectangle {
                                                    width: Math.max(span.spare > 0 ? height : 0, parent.width * span.spare / 100)
                                                    height: parent.height
                                                    radius: 2
                                                    color: span.tone

                                                    Behavior on width {
                                                        NumberAnimation { duration: Tokens.veil; easing.type: Easing.OutCubic }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        Rectangle {
                            x: 8
                            width: parent.width - 16
                            height: 1
                            color: Tokens.border
                            visible: Daemon.usage.length > 0 && root.working > 0
                        }

                        Repeater {
                            model: Agents.list

                            Press {
                                id: entry

                                required property int index
                                required property string agent
                                required property string phase
                                required property string provider
                                required property string name
                                required property string detail
                                required property double since
                                // Su rol `model` no puede ser una propiedad: choca con
                                // el del delegado. Se lee a mano, al cambiar de fase.
                                readonly property string engine: phase, Agents.list.get(index)?.model ?? ""
                                readonly property bool needs: phase === "needs"

                                visible: phase === "working" || needs
                                width: agents.width
                                height: visible ? 48 : 0
                                radius: root.card - 6
                                onTapped: {
                                    Agents.focus(entry.agent);
                                    root.done();
                                }

                                Logo {
                                    id: logo
                                    x: 14
                                    anchors.verticalCenter: parent.verticalCenter
                                    source: Agents.logo(entry.provider)
                                    color: Agents.vendor(entry.provider)
                                }

                                Column {
                                    anchors.left: logo.right
                                    anchors.leftMargin: 18
                                    anchors.right: elapsed.left
                                    anchors.rightMargin: 10
                                    anchors.verticalCenter: parent.verticalCenter

                                    Ink {
                                        width: parent.width
                                        text: entry.name
                                        color: entry.needs ? Tokens.attention : Tokens.text
                                        elide: Text.ElideRight
                                    }

                                    Ink {
                                        width: parent.width
                                        text: [entry.engine, entry.detail].filter(part => part).join(" · ")
                                        color: Tokens.dim
                                        font.pixelSize: Tokens.tinySize
                                        elide: Text.ElideRight
                                        visible: text !== ""
                                    }
                                }

                                Ink {
                                    id: elapsed
                                    anchors.right: parent.right
                                    anchors.rightMargin: 12
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: entry.needs ? "te necesita" : root.ago(entry.since)
                                    color: entry.needs ? Tokens.attention : Tokens.dim
                                    font.pixelSize: Tokens.smallSize
                                }
                            }
                        }
                    }
                }

                // NixOS / Flake: generación, actualizaciones y botones de flake update / rebuild.
                Card {
                    width: parent.width
                    height: nixCard.height + 10
                    visible: Nix.systemActive

                    Column {
                        id: nixCard
                        x: 6
                        y: 2
                        width: parent.width - 12
                        spacing: 6

                        Heading {
                            text: Nix.generation > 0 ? `NixOS · #${Nix.generation}` : (Nix.flakeName ? `Nix · ${Nix.flakeName}` : "Nix")

                            Round {
                                anchors.verticalCenter: parent.verticalCenter
                                icon: "arrows-clockwise"
                                tint: Nix.checking ? Tokens.accent : Tokens.dim
                                visible: Nix.flake !== ""
                                onTapped: Nix.checkUpdates()
                            }

                            Round {
                                anchors.verticalCenter: parent.verticalCenter
                                icon: "gear-six"
                                onTapped: {
                                    Tuning.origin = Qt.size(root.width, root.height);
                                    Tuning.show("", "nix");
                                    root.done();
                                }
                            }
                        }

                        Item {
                            width: parent.width
                            height: 36

                            Icon {
                                id: nixGlyph
                                x: 10
                                anchors.verticalCenter: parent.verticalCenter
                                name: Nix.authNeeded ? "lock" : Nix.jobState === "error" ? "warning" : Nix.jobState === "done" ? "check-circle" : "snowflake"
                                color: Nix.authNeeded || Nix.rebootNeeded ? Tokens.attention : Nix.jobState === "error" ? Tokens.critical : Nix.jobState === "done" ? Tokens.done : (Nix.running || Nix.updates.length > 0 || Nix.pendingRebuild) ? Tokens.accent : Tokens.dim
                            }

                            Column {
                                anchors.left: nixGlyph.right
                                anchors.leftMargin: 12
                                anchors.right: nixActions.left
                                anchors.rightMargin: 10
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: 1

                                Ink {
                                    width: parent.width
                                    text: Nix.summaryLine()
                                    color: Nix.authNeeded ? Tokens.attention : Nix.jobState === "error" ? Tokens.critical : Tokens.text
                                    elide: Text.ElideRight
                                }

                                Ink {
                                    width: parent.width
                                    text: [Nix.flakeName, Nix.version, Nix.checkedAt > 0 ? `comprobado ${Nix.ageText(Nix.checkedAt, root.clock)}` : ""].filter(p => p).join(" · ")
                                    dim: true
                                    font.pixelSize: Tokens.tinySize
                                    elide: Text.ElideRight
                                    visible: text !== ""
                                }
                            }

                            Row {
                                id: nixActions
                                anchors.right: parent.right
                                anchors.rightMargin: 6
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: 6

                                Chip {
                                    icon: "x-circle"
                                    text: "Detener"
                                    base: Tokens.bg
                                    visible: Nix.running && !(Nix.job?.external ?? false)
                                    onTapped: Nix.stopJob()
                                }

                                Chip {
                                    icon: "arrows-clockwise"
                                    text: "Actualizar"
                                    base: Tokens.bg
                                    visible: !Nix.running && Nix.flake !== ""
                                    onTapped: {
                                        Nix.updateFlake("");
                                        root.done();
                                    }
                                }

                                Chip {
                                    icon: "hammer"
                                    text: "Reconstruir"
                                    base: Tokens.bg
                                    lit: Nix.pendingRebuild || Nix.dirty > 0
                                    visible: !Nix.running && Nix.rebuild !== ""
                                    onTapped: {
                                        Nix.rebuildSystem();
                                        root.done();
                                    }
                                }
                            }
                        }
                    }
                }

                // Al pie: los atajos sobre la pantalla y la sesión. Lo que dice
                // el botón bajo el puntero, o lo que espera una segunda
                // pulsación, sale junto a la sesión: nada se mueve bajo el puntero.
                Item {
                    id: foot

                    readonly property var hovered: root.session.find((order, index) => knobs.itemAt(index)?.hovered) ?? null
                    readonly property var waiting: root.session.find(order => order.key === root.armed) ?? null

                    width: parent.width
                    height: Tokens.knob + 10

                    Row {
                        id: tools
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 6

                        Repeater {
                            model: root.shortcuts

                            Chip {
                                required property var modelData
                                icon: modelData.order.glyph
                                text: modelData.name
                                onTapped: root.shoot(modelData.order)
                            }
                        }
                    }

                    Ink {
                        anchors.right: sessions.left
                        anchors.rightMargin: 10
                        anchors.left: tools.right
                        anchors.leftMargin: 10
                        anchors.verticalCenter: parent.verticalCenter
                        horizontalAlignment: Text.AlignRight
                        elide: Text.ElideLeft
                        text: foot.waiting ? `${foot.waiting.title}: pulsa otra vez` : foot.hovered?.title ?? ""
                        color: foot.waiting ? Tokens.attention : Tokens.dim
                        font.pixelSize: Tokens.smallSize
                        opacity: foot.waiting || foot.hovered ? 1 : 0

                        Behavior on opacity {
                            NumberAnimation { duration: Tokens.fast }
                        }
                    }

                    Row {
                        id: sessions
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 2

                        Repeater {
                            id: knobs
                            model: root.session

                            Round {
                                required property var modelData
                                readonly property bool waiting: root.armed === modelData.key

                                width: Tokens.knob + 4
                                height: width
                                icon: modelData.glyph
                                tint: waiting ? Tokens.bg : Tokens.dim
                                rest: waiting ? Tokens.attention : "transparent"
                                onTapped: root.press(modelData)
                            }
                        }
                    }
                }
            }

            // --- Veyl: lo que le has pedido ---
            Page {
                id: veyl
                name: "veyl"

                // Preguntar algo nuevo es abrir la entrada.
                Press {
                    width: parent.width
                    height: 46
                    radius: root.card
                    rest: Tokens.surface
                    onTapped: {
                        root.done();
                        Launcher.show("");
                    }

                    Icon {
                        id: spark
                        x: 16
                        anchors.verticalCenter: parent.verticalCenter
                        name: "sparkle"
                        color: Tokens.accent
                    }

                    Ink {
                        anchors.left: spark.right
                        anchors.leftMargin: 12
                        anchors.verticalCenter: parent.verticalCenter
                        text: "Pregúntale algo a Veyl…"
                        dim: true
                    }

                    Row {
                        anchors.right: parent.right
                        anchors.rightMargin: 12
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 4

                        Repeater {
                            model: ["Super", "Espacio"]

                            Rectangle {
                                required property string modelData
                                width: key.implicitWidth + 12
                                height: 22
                                radius: Tokens.soft
                                color: Tokens.bg

                                Ink {
                                    id: key
                                    anchors.centerIn: parent
                                    text: parent.modelData
                                    dim: true
                                    font.pixelSize: Tokens.tinySize
                                }
                            }
                        }
                    }
                }

                Card {
                    width: parent.width
                    height: past.height + 8

                    Column {
                        id: past
                        x: 6
                        y: 2
                        width: parent.width - 12

                        Heading {
                            text: Daemon.asked.length > 0 ? "Lo que le has pedido" : "Historial"

                            Repeater {
                                model: Daemon.asked.length > 0 ? [["", "Todo"], ["voice", "Dicho"], ["typed", "Escrito"]] : []

                                Chip {
                                    required property var modelData
                                    height: 24
                                    base: Tokens.bg
                                    text: modelData[1]
                                    lit: root.only === modelData[0]
                                    onTapped: {
                                        root.touched();
                                        root.only = modelData[0];
                                        history.contentY = 0;
                                    }
                                }
                            }
                        }

                        Flickable {
                            id: history
                            width: parent.width
                            height: Math.min(contentHeight, root.list)
                            contentHeight: rows.height + 4
                            clip: true
                            boundsBehavior: Flickable.StopAtBounds
                            visible: root.entries.length > 0

                            Behavior on height {
                                NumberAnimation { duration: Tokens.veil; easing.type: Easing.OutCubic }
                            }

                            Column {
                                id: rows
                                width: history.width

                                Repeater {
                                    model: root.entries

                                    Item {
                                        id: item

                                        required property var modelData
                                        readonly property var row: modelData.row
                                        readonly property bool open: !!row && root.opened === row.id

                                        width: rows.width
                                        height: modelData.kind === "day" ? 30 : query.height

                                        Ink {
                                            x: 10
                                            anchors.bottom: parent.bottom
                                            anchors.bottomMargin: 6
                                            visible: item.modelData.kind === "day"
                                            text: item.modelData.title
                                            color: Tokens.dim
                                            font.pixelSize: Tokens.tinySize
                                            font.weight: Font.DemiBold
                                            font.capitalization: Font.AllUppercase
                                            font.letterSpacing: 0.6
                                        }

                                        Press {
                                            id: query
                                            visible: !!item.row
                                            width: parent.width
                                            height: visible ? 52 + (item.open ? detail.height + 6 : 0) : 0
                                            radius: root.card - 6
                                            rest: item.open ? Tokens.bg : "transparent"
                                            clip: true
                                            onTapped: {
                                                root.touched();
                                                root.opened = item.open ? -1 : item.row.id;
                                            }

                                            Behavior on height {
                                                NumberAnimation { duration: Tokens.veil; easing.type: Easing.OutCubic }
                                            }

                                            // De dónde vino: dicho, escrito o desde una terminal.
                                            Rectangle {
                                                id: source
                                                x: 8
                                                y: 10
                                                width: 32
                                                height: 32
                                                radius: width / 2
                                                color: Tokens.bg

                                                Icon {
                                                    anchors.centerIn: parent
                                                    name: !item.row ? "" : !item.row.ok ? "warning-circle" : item.row.source === "voice" ? "microphone" : item.row.source === "launcher" ? "keyboard" : item.row.source === "diary" ? "notebook" : "terminal"
                                                    color: item.row && !item.row.ok ? Tokens.critical : item.row?.handler?.startsWith("brain") || item.row?.handler === "llm" ? Tokens.accent : Tokens.dim
                                                    font.pixelSize: Tokens.iconSize - 2
                                                }
                                            }

                                            Column {
                                                anchors.left: source.right
                                                anchors.leftMargin: 12
                                                anchors.right: aside.left
                                                anchors.rightMargin: 8
                                                y: 8
                                                spacing: 1

                                                Ink {
                                                    width: parent.width
                                                    text: item.row ? root.capital(item.row.said) : ""
                                                    elide: Text.ElideRight
                                                }

                                                Ink {
                                                    width: parent.width
                                                    text: item.row?.say || (item.row?.card ? item.row.card.titulo ?? "" : "")
                                                    color: item.row && !item.row.ok ? Tokens.critical : Tokens.dim
                                                    font.pixelSize: Tokens.smallSize
                                                    elide: Text.ElideRight
                                                    opacity: item.open ? 0 : 1

                                                    Behavior on opacity {
                                                        NumberAnimation { duration: Tokens.fast }
                                                    }
                                                }
                                            }

                                            // A la derecha, la hora; con el puntero encima, lo que se puede hacer.
                                            Item {
                                                id: aside
                                                anchors.right: parent.right
                                                anchors.rightMargin: 8
                                                y: 12
                                                width: 64
                                                height: Tokens.knob

                                                Ink {
                                                    id: hour
                                                    anchors.right: parent.right
                                                    anchors.rightMargin: 4
                                                    anchors.verticalCenter: parent.verticalCenter
                                                    text: item.row ? Agenda.clock(item.row.time) : ""
                                                    dim: true
                                                    font.pixelSize: Tokens.tinySize

                                                    Behavior on opacity {
                                                        NumberAnimation { duration: Tokens.fast }
                                                    }
                                                }

                                                Row {
                                                    id: acts
                                                    anchors.right: parent.right
                                                    spacing: 2
                                                    visible: opacity > 0

                                                    Behavior on opacity {
                                                        NumberAnimation { duration: Tokens.fast }
                                                    }

                                                    Round {
                                                        icon: "arrow-u-up-left"
                                                        onTapped: root.again(item.row.said)
                                                    }

                                                    Round {
                                                        icon: "trash"
                                                        onTapped: {
                                                            root.touched();
                                                            Daemon.forget(item.row.id);
                                                        }
                                                    }
                                                }

                                                Swap {
                                                    flipped: query.hovered || item.open
                                                    rest: hour
                                                    lit: acts
                                                }
                                            }

                                            // Abierta: la respuesta entera y, si compuso una, su tarjeta.
                                            Column {
                                                id: detail
                                                anchors.left: parent.left
                                                anchors.leftMargin: 52
                                                anchors.right: parent.right
                                                anchors.rightMargin: 14
                                                y: 48
                                                spacing: 10
                                                opacity: item.open ? 1 : 0
                                                visible: opacity > 0

                                                Behavior on opacity {
                                                    NumberAnimation { duration: Tokens.fast }
                                                }

                                                Ink {
                                                    width: parent.width
                                                    text: item.row ? [item.row.say, item.row.more].filter(part => part).join("\n") || "Sin respuesta" : ""
                                                    color: item.row && !item.row.ok ? Tokens.critical : Tokens.text
                                                    font.pixelSize: Tokens.smallSize
                                                    wrapMode: Text.WordWrap
                                                    maximumLineCount: 14
                                                    elide: Text.ElideRight
                                                    lineHeight: 1.15
                                                }

                                                Row {
                                                    spacing: 6

                                                    Chip {
                                                        visible: !!item.row?.card
                                                        icon: "cards"
                                                        text: "Ver la tarjeta"
                                                        base: Tokens.surface
                                                        onTapped: {
                                                            root.done();
                                                            Daemon.card = item.row.card;
                                                        }
                                                    }

                                                    Chip {
                                                        icon: "arrow-u-up-left"
                                                        text: "Volver a preguntar"
                                                        base: Tokens.surface
                                                        onTapped: root.again(item.row.said)
                                                    }
                                                }

                                                Item {
                                                    width: 1
                                                    height: 2
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        Empty {
                            visible: root.entries.length === 0
                            icon: !Daemon.connected ? "plugs" : !Daemon.journal ? "notebook" : "chat-circle-dots"
                            text: !Daemon.connected ? "veyld no está en marcha" : !Daemon.journal ? "El diario está apagado" : Daemon.asked.length > 0 ? "Nada por aquí" : "Aún no le has pedido nada a Veyl"
                            hint: !Daemon.connected ? "Sin él no se guarda lo que le pides." : !Daemon.journal ? "Veyl no guarda lo que le pides. Se enciende en Ajustes → Diario y órdenes." : Daemon.asked.length > 0 ? "" : "Lo que le digas o le escribas en la entrada saldrá aquí, con su respuesta."
                        }
                    }
                }
            }

            // --- Avisos: todas las notificaciones guardadas ---
            Page {
                id: notices
                name: "notes"

                Card {
                    width: parent.width
                    height: inbox.height + 8

                    Column {
                        id: inbox
                        x: 6
                        y: 2
                        width: parent.width - 12

                        Heading {
                            text: root.notes.length === 1 ? "1 notificación" : `${root.notes.length} notificaciones`

                            Chip {
                                height: 24
                                base: Tokens.bg
                                icon: Notifications.quiet ? "bell-slash" : "bell"
                                text: "No molestar"
                                lit: Notifications.quiet
                                onTapped: Settings.quiet = !Settings.quiet
                            }

                            Chip {
                                height: 24
                                base: Tokens.bg
                                icon: "broom"
                                text: "Borrar todo"
                                visible: root.notes.length > 0
                                onTapped: {
                                    root.touched();
                                    Notifications.clear();
                                }
                            }
                        }

                        Flickable {
                            id: shelf
                            width: parent.width
                            height: Math.min(contentHeight, root.list)
                            contentHeight: stack.height + 4
                            clip: true
                            boundsBehavior: Flickable.StopAtBounds
                            visible: root.notes.length > 0

                            Behavior on height {
                                NumberAnimation { duration: Tokens.veil; easing.type: Easing.OutCubic }
                            }

                            Column {
                                id: stack
                                width: shelf.width

                                Repeater {
                                    model: root.notes

                                    Press {
                                        id: note

                                        required property var modelData
                                        readonly property bool critical: modelData.urgency === "critical"
                                        readonly property string kind: modelData.image && photo.status !== Image.Error ? "image" : modelData.icon && emblem.status !== Image.Error ? "icon" : "glyph"

                                        width: stack.width
                                        height: 58
                                        radius: root.card - 6
                                        onTapped: {
                                            root.touched();
                                            Notifications.revisit(note.modelData);
                                            if (note.modelData.primary)
                                                root.done();
                                        }
                                        onContext: {
                                            root.touched();
                                            Notifications.forget(note.modelData.id);
                                        }

                                        Item {
                                            id: avatar
                                            x: 8
                                            anchors.verticalCenter: parent.verticalCenter
                                            width: 38
                                            height: 38

                                            Clip {
                                                anchors.fill: parent
                                                radius: Tokens.radius
                                                color: Tokens.bg
                                                visible: note.kind === "image"

                                                Image {
                                                    id: photo
                                                    anchors.fill: parent
                                                    source: note.modelData.image
                                                    sourceSize: Qt.size(width * 2, height * 2)
                                                    fillMode: Image.PreserveAspectCrop
                                                    asynchronous: true
                                                }
                                            }

                                            Image {
                                                id: emblem
                                                anchors.fill: parent
                                                anchors.margins: 3
                                                source: note.modelData.icon
                                                sourceSize: Qt.size(width * 2, height * 2)
                                                fillMode: Image.PreserveAspectFit
                                                asynchronous: true
                                                visible: note.kind === "icon"
                                            }

                                            Rectangle {
                                                anchors.fill: parent
                                                radius: width / 2
                                                color: Tokens.bg
                                                visible: note.kind === "glyph"

                                                Icon {
                                                    anchors.centerIn: parent
                                                    name: note.critical ? "warning" : note.modelData.glyph
                                                    color: note.critical ? Tokens.critical : Tokens.text
                                                }
                                            }
                                        }

                                        Column {
                                            anchors.left: avatar.right
                                            anchors.leftMargin: 12
                                            anchors.right: side.left
                                            anchors.rightMargin: 10
                                            anchors.verticalCenter: parent.verticalCenter
                                            spacing: 1

                                            Ink {
                                                width: parent.width
                                                text: note.modelData.summary || note.modelData.app
                                                color: note.critical ? Tokens.critical : Tokens.text
                                                elide: Text.ElideRight
                                            }

                                            Ink {
                                                width: parent.width
                                                text: note.modelData.body.replace(/<br\/>/g, " ")
                                                textFormat: Text.StyledText
                                                maximumLineCount: 1
                                                elide: Text.ElideRight
                                                dim: true
                                                font.pixelSize: Tokens.smallSize
                                                visible: text !== ""
                                            }
                                        }

                                        // De quién y cuándo; con el puntero encima, el aspa en su sitio.
                                        Item {
                                            id: side
                                            anchors.right: parent.right
                                            anchors.rightMargin: 6
                                            anchors.verticalCenter: parent.verticalCenter
                                            width: 120
                                            height: Tokens.knob

                                            Ink {
                                                id: stamp
                                                anchors.right: parent.right
                                                anchors.rightMargin: 6
                                                anchors.verticalCenter: parent.verticalCenter
                                                width: Math.min(implicitWidth, parent.width - 8)
                                                horizontalAlignment: Text.AlignRight
                                                text: [note.modelData.app, root.ago(note.modelData.at)].filter(part => part).join(" · ")
                                                elide: Text.ElideLeft
                                                dim: true
                                                font.pixelSize: Tokens.tinySize

                                                Behavior on opacity {
                                                    NumberAnimation { duration: Tokens.fast }
                                                }
                                            }

                                            Round {
                                                id: wipe
                                                anchors.right: parent.right
                                                icon: "x-circle"
                                                visible: opacity > 0
                                                onTapped: {
                                                    root.touched();
                                                    Notifications.forget(note.modelData.id);
                                                }

                                                Behavior on opacity {
                                                    NumberAnimation { duration: Tokens.fast }
                                                }
                                            }

                                            Swap {
                                                flipped: note.hovered
                                                rest: stamp
                                                lit: wipe
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        Empty {
                            visible: root.notes.length === 0
                            icon: "bell"
                            text: "Nada nuevo"
                            hint: "Lo que llegue y no atiendas se queda aquí."
                        }
                    }
                }
            }
        }
    }
}
