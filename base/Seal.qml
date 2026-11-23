import QtQuick
import QtQuick.Effects
import QtQuick.Shapes
import Quickshell
import Quickshell.Wayland
import qs
import qs.state

// El bloqueo, en cada monitor. El fondo, desenfocado y oscurecido, y sobre él,
// sin isla, la hora enorme alineada a la derecha, con la fecha encima y lo
// próximo debajo. Del centro del borde izquierdo sale lo tuyo: los agentes en
// marcha, el día como una línea con lo que ya pasó, el ahora y lo que queda,
// el temporizador y cuántas notificaciones esperan. Bajo la hora, la puerta:
// tu inicial y tu nombre que, al escribir, se convierten en el campo, que
// crece letra a letra; cada punto entra con un rebote, el cursor los sigue
// y una luz del acento se enciende tras él a cada tecla. Comprobando, los puntos
// ondean; si falla, se tiñen de crítico, el campo se sacude y se van; al
// acertar, la hora se acerca y se desvanece, las islas vuelven a sus bordes
// y se suelta el bloqueo. Arriba a la derecha, las excepciones.
WlSessionLock {
    locked: Lock.locked

    WlSessionLockSurface {
        id: surface

        // Transparente con el color en un rectángulo, como el fondo (Backdrop):
        // una superficie opaca entre las transparentes da eglError 0x3009.
        color: "transparent"

        Rectangle {
            anchors.fill: parent
            color: Tokens.bg
        }

        Item {
            id: root

            anchors.fill: parent

            // Nada sale hasta que la superficie está puesta: así se ve llegar.
            property bool born: false
            readonly property bool shown: born && Lock.locked && !Lock.opening
            Component.onCompleted: Qt.callLater(() => born = true)

            // Solo el monitor principal (el que tenía el foco al bloquear; si
            // ya no está, el primero) lleva la hora, el campo y lo tuyo. Los
            // demás se desenfocan y oscurecen igual, sin nada encima.
            readonly property bool main: surface.screen?.name === Lock.screen || (!Quickshell.screens.some(screen => screen.name === Lock.screen) && surface.screen === Quickshell.screens[0])

            readonly property double now: Clock.now.getTime()
            // El minuto: lo que se rehace con la hora no se rehace cada segundo.
            readonly property double minute: Math.floor(now / 60000) * 60000
            readonly property var today: Agenda.days[Calendar.today] ?? []
            readonly property var timed: today.filter(event => !event.allDay)
            // Lo de días siguientes, si hoy no hay nada.
            readonly property var later: Agenda.ahead.filter(event => Calendar.at(event.at) > Calendar.today).slice(0, 4)
            // Agentes a la vista y cuántos trabajan; notificaciones guardadas.
            readonly property int working: Agents.rows.filter(row => row.phase !== "gone").length
            readonly property int busy: Agents.rows.filter(row => row.phase === "working").length
            readonly property int kept: Notifications.shelf.count
            readonly property string user: Quickshell.env("USER") ?? ""

            // Las filas del día: los de todo el día arriba y, de los demás,
            // unos pocos alrededor del ahora, que va entre ellos.
            readonly property var rows: {
                if (today.length === 0)
                    return later.map(event => ({ event }));
                const whole = today.filter(event => event.allDay).slice(0, 2).map(event => ({ event }));
                const passed = timed.filter(event => event.at <= minute).length;
                const start = Math.max(0, Math.min(passed - 2, timed.length - 5));
                const near = timed.slice(start, start + 5).map(event => ({ event }));
                near.splice(passed - start, 0, { now: true });
                return whole.concat(near);
            }
            readonly property int nowRow: rows.findIndex(row => row.now)

            // La contraseña, en puntos: tantos como letras; al fallar, los
            // que había, un momento, en crítico.
            readonly property int step: 18
            property bool wrong: false
            property int held: 0  // las letras que había la última vez que hubo alguna
            readonly property int count: Math.min(Tokens.lockDots, wrong ? held : input.text.length)
            readonly property bool typed: count > 0 || Lock.checking
            property color ink: wrong ? Tokens.critical : Tokens.accent
            property color rim: wrong ? Tokens.critical : typed ? Tokens.accent : Tokens.border
            property real phase: 0  // la onda de los puntos mientras se comprueba
            property real blink: 1  // el cursor

            Behavior on ink {
                ColorAnimation { duration: Tokens.fast }
            }

            Behavior on rim {
                ColorAnimation { duration: Tokens.veil }
            }

            // Cuánto falta, en corto: «en 25 min», «en 2 h 10», «en 2 días 19 h»,
            // «en 1 semana 3 días», «en 2 meses», «ahora».
            function until(at: double): string {
                const minutes = Math.ceil((at - now) / 60000);
                return minutes <= 0 ? "ahora" : `en ${lapse(minutes)}`;
            }

            // Un rato en la unidad más grande que quepa y, si suma, la siguiente.
            function lapse(minutes: int): string {
                const some = (count, one, many) => `${count} ${count === 1 ? one : many}`;
                if (minutes < 60)
                    return `${minutes} min`;
                const hours = Math.floor(minutes / 60);
                if (hours < 24)
                    return minutes % 60 ? `${hours} h ${String(minutes % 60).padStart(2, "0")}` : `${hours} h`;
                const days = Math.floor(hours / 24);
                if (days < 7)
                    return hours % 24 ? `${some(days, "día", "días")} ${hours % 24} h` : some(days, "día", "días");
                if (days < 30)
                    return days % 7 ? `${some(Math.floor(days / 7), "semana", "semanas")} ${some(days % 7, "día", "días")}` : some(days / 7, "semana", "semanas");
                if (days < 365)
                    return some(Math.floor(days / 30), "mes", "meses");
                return some(Math.floor(days / 365), "año", "años");
            }

            // El fondo se desenfoca y oscurece al llegar.
            // Al llegar, despacio; al irse, deprisa: ya has esperado bastante.
            Spring {
                id: veil
                to: root.shown ? 1 : 0
                settle: Lock.opening ? Tokens.veil : Tokens.arrive * 2
            }

            // La hora llega algo después, y al irse se acerca.
            Spring {
                id: rise
                to: root.shown ? 1 : 0
                settle: Tokens.arrive * 1.4
            }

            // Escribiendo, la hora se aparta un poco y la puerta se abre.
            Spring {
                id: lean
                to: root.typed ? 1 : 0
                settle: Tokens.arrive
            }

            Spring {
                id: mode
                to: root.typed ? 1 : 0
                settle: Tokens.veil
            }

            // Cada tecla, un latido: la puerta se hincha y la luz sube.
            Spring {
                id: beat
                settle: Tokens.arrive
            }

            // Comprobando, los puntos ondean.
            Spring {
                id: wave
                to: Lock.checking ? 1 : 0
                settle: Tokens.veil
            }

            // Un fallo sacude la puerta de lado a lado.
            Spring {
                id: shake
                settle: Tokens.veil * 1.5
                bounce: 0.75
            }

            NumberAnimation on phase {
                from: 0
                to: Math.PI * 2
                duration: Tokens.veil * 3
                loops: Animation.Infinite
                running: wave.value > 0.001
            }

            SequentialAnimation on blink {
                id: blinking
                running: root.typed && !Lock.checking
                loops: Animation.Infinite
                PauseAnimation { duration: Tokens.arrive }
                NumberAnimation { to: 0.1; duration: Tokens.arrive; easing.type: Easing.InOutSine }
                NumberAnimation { to: 1; duration: Tokens.arrive; easing.type: Easing.InOutSine }
            }

            // Lo que salió mal, un momento en crítico antes de irse.
            Timer {
                id: sting
                interval: Tokens.arrive
                onTriggered: root.wrong = false
            }

            Connections {
                target: Lock

                function onMissed(): void {
                    root.wrong = true;
                    sting.restart();
                    shake.velocity += 900;
                }

                function onSecretChanged(): void {
                    if (input.text !== Lock.secret)
                        input.text = Lock.secret;
                }
            }

            // Lo que se escribe no se ve: solo sus puntos. Fuera de la puerta,
            // para que no pierda el teclado cuando ella cambia.
            TextInput {
                id: input

                property int last: 0

                width: 1
                height: 1
                opacity: 0
                focus: true
                echoMode: TextInput.Password
                readOnly: Lock.checking || Lock.opening
                onTextChanged: {
                    Lock.secret = text;
                    if (text !== "") {
                        Lock.note = "";
                        root.wrong = false;
                        root.held = text.length;
                    }
                    // Al escribir, un latido; al borrar, uno menor hacia dentro.
                    beat.velocity += text.length > last ? 14 : text.length < last && text !== "" ? -6 : 0;
                    last = text.length;
                    root.blink = 1;
                    blinking.restart();
                }
                Keys.onPressed: event => {
                    if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                        Lock.submit();
                        event.accepted = true;
                    } else if (event.key === Qt.Key_Escape) {
                        text = "";
                        event.accepted = true;
                    }
                }
            }

            // --- El fondo ---

            // El fondo de pantalla, el mismo que hay en el escritorio y del
            // mismo tamaño: ya está cargado, así que sale en el primer
            // fotograma, nítido, y desde ahí se desenfoca.
            Image {
                id: picture
                anchors.fill: parent
                source: Wallpaper.source
                // Como en Backdrop: con la misma clave, la caché la da al momento.
                sourceSize: surface.screen ? Qt.size(surface.screen.width * surface.screen.devicePixelRatio, surface.screen.height * surface.screen.devicePixelRatio) : Qt.size(surface.width, surface.height)
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
                visible: false
            }

            MultiEffect {
                anchors.fill: parent
                source: picture
                visible: picture.status === Image.Ready
                autoPaddingEnabled: false
                blurEnabled: true
                blurMax: Tokens.haze
                blur: veil.value
                saturation: -0.3 * veil.value
                // Se acerca un poco hacia ti al llegar y se aleja al irse.
                scale: 1 + 0.04 * veil.value
            }

            Rectangle {
                anchors.fill: parent
                color: Tokens.hush
                opacity: veil.value
            }

            // Escribiendo, algo más oscuro: lo único que importa es la puerta.
            Rectangle {
                anchors.fill: parent
                color: Tokens.shadow
                opacity: veil.value * lean.value * 0.6
            }

            // La luz de la puerta: se enciende tras el campo, del acento (o del
            // crítico, si falla), y crece con cada tecla.
            Shape {
                id: glow

                readonly property real strength: Math.min(1, 0.55 * mode.value + 0.4 * Math.max(0, beat.value) + 0.25 * wave.value * (1 + Math.sin(root.phase * 2)))

                x: gate.x + gate.width / 2 - width / 2
                y: gate.y + gate.height / 2 - height / 2
                width: 560
                height: width
                opacity: veil.value * strength
                visible: root.main && opacity > 0.005
                transform: Scale {
                    origin.x: glow.width / 2
                    origin.y: glow.height / 2
                    xScale: 1.4 * (0.85 + 0.25 * glow.strength)
                    yScale: 0.55 * (0.85 + 0.25 * glow.strength)
                }

                ShapePath {
                    strokeWidth: -1
                    fillGradient: RadialGradient {
                        centerX: glow.width / 2
                        centerY: glow.height / 2
                        centerRadius: glow.width / 2
                        focalX: glow.width / 2
                        focalY: glow.height / 2

                        GradientStop { position: 0; color: Qt.rgba(root.ink.r, root.ink.g, root.ink.b, 0.32) }
                        GradientStop { position: 0.45; color: Qt.rgba(root.ink.r, root.ink.g, root.ink.b, 0.1) }
                        GradientStop { position: 1; color: Qt.rgba(root.ink.r, root.ink.g, root.ink.b, 0) }
                    }

                    PathLine { x: glow.width; y: 0 }
                    PathLine { x: glow.width; y: glow.height }
                    PathLine { x: 0; y: glow.height }
                    PathLine { x: 0; y: 0 }
                }
            }

            // Pulsar donde sea devuelve el teclado al campo.
            MouseArea {
                anchors.fill: parent
                onPressed: input.forceActiveFocus()
            }

            // Un título pequeño, en versalitas.
            component Caption: Ink {
                font.pixelSize: Tokens.tinySize
                font.weight: Font.DemiBold
                font.letterSpacing: 1.6
                font.capitalization: Font.AllUppercase
                dim: true
            }

            // --- A la derecha: la hora ---

            Column {
                id: face

                visible: root.main
                anchors.right: parent.right
                anchors.rightMargin: Math.round(parent.width * 0.07)
                transformOrigin: Item.Right
                y: parent.height * 0.42 - height / 2 - lean.value * 40 + (Lock.opening ? 0 : (1 - rise.value) * 48)
                opacity: Math.max(0, rise.value) * (1 - 0.35 * lean.value)
                // Llega desde algo más lejos; al acertar, se te acerca y se va.
                scale: (Lock.opening ? 1 + 0.12 * (1 - rise.value) : 0.92 + 0.08 * rise.value) * (1 - 0.07 * lean.value)
                spacing: 0
                // Una sombra suave: se lee sobre cualquier fondo.
                layer.enabled: true
                layer.effect: MultiEffect {
                    shadowEnabled: true
                    shadowColor: Qt.rgba(0, 0, 0, 0.45)
                    shadowBlur: 1
                    shadowVerticalOffset: 3
                }

                // El día de la semana, del acento, y la fecha.
                Row {
                    anchors.right: parent.right
                    spacing: 12

                    Ink {
                        text: Clock.now.toLocaleDateString(Calendar.locale, "dddd")
                        color: Tokens.accent
                        font.pixelSize: Tokens.fontSize
                        font.weight: Font.DemiBold
                        font.letterSpacing: 3
                        font.capitalization: Font.AllUppercase
                    }

                    Ink {
                        text: Clock.now.toLocaleDateString(Calendar.locale, Calendar.longFormat.replace(/^d{4}[,\s]*/, ""))
                        font.pixelSize: Tokens.fontSize
                        font.letterSpacing: 3
                        font.capitalization: Font.AllUppercase
                        opacity: 0.85
                    }
                }

                // La hora: las horas firmes, los minutos finos.
                Hour {
                    anchors.right: parent.right
                    // La cifra trae su propio aire a la derecha: que el borde
                    // de la tinta, no el de la caja, case con la fecha.
                    anchors.rightMargin: -Tokens.lockSize * 0.04
                    size: Tokens.lockSize
                    hours: Font.Medium
                    minutes: Font.ExtraLight
                }

                // Lo próximo, en una línea; o el temporizador; o nada.
                Row {
                    id: upcoming

                    anchors.right: parent.right
                    spacing: 10
                    opacity: 1 - lean.value
                    visible: opacity > 0

                    Rectangle {
                        anchors.verticalCenter: parent.verticalCenter
                        width: Tokens.dot
                        height: width
                        radius: width / 2
                        color: Agenda.imminent ? Tokens.attention : Tokens.accent
                        visible: !!Agenda.next
                    }

                    Icon {
                        anchors.verticalCenter: parent.verticalCenter
                        name: "timer"
                        color: Tokens.accent
                        visible: !Agenda.next && Countdown.running
                    }

                    Ink {
                        text: Agenda.next ? Agenda.next.title : Countdown.running ? Countdown.exact : root.today.length > 0 ? "Nada más por hoy" : "Día libre"
                        font.pixelSize: Tokens.promptSize
                        strong: !!Agenda.next || Countdown.running
                        dim: !Agenda.next && !Countdown.running
                    }

                    Ink {
                        visible: !!Agenda.next
                        text: Agenda.next ? (Calendar.at(Agenda.next.at) === Calendar.today ? "" : Calendar.format(Calendar.at(Agenda.next.at), "dddd") + " · ") + Agenda.clock(Agenda.next.at) + " · " + root.until(Agenda.next.at) : ""
                        font.pixelSize: Tokens.promptSize
                        color: Agenda.imminent ? Tokens.attention : Tokens.dim
                    }
                }
            }

            // --- A la izquierda: lo tuyo ---

            // Los agentes, el día, el temporizador y lo que ha llegado, en
            // una isla del centro del borde izquierdo: todo en la misma
            // columna, con la marca de cada fila sobre una línea.
            Flank {
                anchors.left: parent.left
                anchors.verticalCenter: parent.verticalCenter
                shown: root.shown && root.main && (root.working > 0 || root.rows.length > 0 || Countdown.running || root.kept > 0)
                settle: Tokens.arrive + Tokens.veil

                Column {
                    id: day

                    readonly property real rail: 64

                    leftPadding: Tokens.pad * 1.5
                    rightPadding: Tokens.pad + 4
                    topPadding: Tokens.pad
                    bottomPadding: Tokens.pad
                    spacing: 0

                    // --- Los agentes ---

                    Item {
                        visible: root.working > 0
                        width: Tokens.lockDay
                        height: Tokens.heading

                        Caption {
                            anchors.verticalCenter: parent.verticalCenter
                            text: "Agentes"
                            color: Tokens.accent
                        }

                        Caption {
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            text: Agents.needing > 0 ? (Agents.needing === 1 ? "1 te necesita" : `${Agents.needing} te necesitan`) : root.busy === 1 ? "1 trabajando" : root.busy > 1 ? `${root.busy} trabajando` : ""
                            color: Agents.needing > 0 ? Tokens.attention : Tokens.dim
                        }
                    }

                    Repeater {
                        model: Agents.list

                        Item {
                            id: agent

                            required property int index
                            required property string phase
                            required property string provider
                            required property string name
                            required property string detail
                            required property string model
                            required property double since
                            readonly property color brand: Agents.vendor(provider)
                            readonly property bool needs: phase === "needs"
                            // Desde cuándo está como está, en corto.
                            readonly property int minutes: Math.max(0, Math.floor((root.minute - since) / 60000))

                            visible: phase !== "gone" && index < 4
                            width: Tokens.lockDay
                            height: visible ? 46 : 0

                            Ink {
                                anchors.right: parent.left
                                anchors.rightMargin: -(day.rail - 18)
                                anchors.verticalCenter: parent.verticalCenter
                                text: agent.minutes < 1 ? "ahora" : agent.minutes < 60 ? `${agent.minutes} min` : agent.minutes < 1440 ? `${Math.floor(agent.minutes / 60)} h` : root.lapse(agent.minutes).split(" ").slice(0, 2).join(" ")
                                font.pixelSize: Tokens.smallSize
                                dim: true
                            }

                            Logo {
                                x: day.rail - width / 2
                                anchors.verticalCenter: parent.verticalCenter
                                width: Tokens.iconSize
                                height: width
                                source: Agents.logo(agent.provider)
                                color: agent.brand
                            }

                            Column {
                                x: day.rail + 18
                                width: parent.width - x - 26
                                anchors.verticalCenter: parent.verticalCenter

                                Ink {
                                    width: parent.width
                                    text: agent.name
                                    elide: Text.ElideRight
                                    strong: agent.needs
                                }

                                Ink {
                                    width: parent.width
                                    text: agent.needs ? "te necesita" : agent.detail || agent.model || (agent.phase === "working" ? "trabajando" : "en espera")
                                    elide: Text.ElideRight
                                    color: agent.needs ? Tokens.attention : Tokens.dim
                                    font.pixelSize: Tokens.smallSize
                                }
                            }

                            // Cómo va: un arco que gira, un punto de aviso o nada.
                            Item {
                                id: status

                                anchors.right: parent.right
                                anchors.verticalCenter: parent.verticalCenter
                                width: 14
                                height: 14

                                Rectangle {
                                    anchors.fill: parent
                                    radius: width / 2
                                    color: "transparent"
                                    border.width: 1.5
                                    border.color: Qt.rgba(agent.brand.r, agent.brand.g, agent.brand.b, 0.2)
                                    visible: agent.phase === "working"
                                }

                                Shape {
                                    anchors.fill: parent
                                    preferredRendererType: Shape.CurveRenderer
                                    visible: agent.phase === "working"

                                    ShapePath {
                                        fillColor: "transparent"
                                        strokeColor: agent.brand
                                        strokeWidth: 1.5
                                        capStyle: ShapePath.RoundCap

                                        PathAngleArc {
                                            centerX: 7
                                            centerY: 7
                                            radiusX: 6.25
                                            radiusY: 6.25
                                            startAngle: -90
                                            sweepAngle: 100
                                        }
                                    }

                                    RotationAnimation on rotation {
                                        running: agent.phase === "working" && root.shown
                                        loops: Animation.Infinite
                                        from: 0
                                        to: 360
                                        duration: Tokens.breath / 3
                                    }
                                }

                                Rectangle {
                                    anchors.centerIn: parent
                                    width: Tokens.dot
                                    height: width
                                    radius: width / 2
                                    color: Tokens.attention
                                    visible: agent.needs
                                }
                            }
                        }
                    }

                    // Entre una cosa y otra, una raya.
                    component Rule: Item {
                        width: Tokens.lockDay
                        height: Tokens.pad + 1

                        Rectangle {
                            anchors.verticalCenter: parent.verticalCenter
                            width: parent.width
                            height: Tokens.line
                            color: Tokens.border
                        }
                    }

                    Rule {
                        visible: root.working > 0 && root.rows.length > 0
                    }

                    // --- El día ---

                    Item {
                        visible: root.rows.length > 0
                        width: Tokens.lockDay
                        height: Tokens.heading

                        Caption {
                            anchors.verticalCenter: parent.verticalCenter
                            text: root.today.length > 0 ? "Hoy" : "Lo próximo"
                            color: Tokens.accent
                            visible: root.rows.length > 0
                        }

                        Caption {
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            visible: root.today.length > 0
                            text: root.today.length === 1 ? "1 evento" : `${root.today.length} eventos`
                        }
                    }

                    Repeater {
                        model: root.rows

                        Item {
                            id: row

                            required property var modelData
                            required property int index
                            readonly property var event: modelData.event ?? null
                            readonly property bool here: !!modelData.now
                            readonly property bool past: !!event && !event.allDay && event.at < root.now - Agenda.linger * 60000
                            readonly property bool next: !!event && Agenda.next?.id === event.id
                            readonly property bool other: !!event && Calendar.at(event.at) !== Calendar.today

                            width: Tokens.lockDay
                            height: here ? 28 : next || other ? 48 : 36

                            // La línea: del acento hasta el ahora.
                            Rectangle {
                                x: day.rail - 1
                                width: 2
                                height: parent.height / 2
                                visible: row.index > 0
                                color: row.index <= root.nowRow ? Tokens.accent : Tokens.border
                            }

                            Rectangle {
                                x: day.rail - 1
                                y: parent.height / 2
                                width: 2
                                height: parent.height / 2
                                visible: row.index < root.rows.length - 1
                                color: row.index < root.nowRow ? Tokens.accent : Tokens.border
                            }

                            // Cuándo, a la izquierda de la línea.
                            Column {
                                anchors.right: parent.left
                                anchors.rightMargin: -(day.rail - 14)
                                anchors.verticalCenter: parent.verticalCenter
                                visible: !row.here

                                Caption {
                                    anchors.right: parent.right
                                    visible: row.other
                                    text: row.other ? Calendar.format(Calendar.at(row.event.at), "ddd d") : ""
                                }

                                Ink {
                                    anchors.right: parent.right
                                    text: !row.event ? "" : row.event.allDay ? "todo el día" : Agenda.clock(row.event.at)
                                    font.pixelSize: row.event?.allDay ? Tokens.tinySize : Tokens.smallSize
                                    color: row.next ? Tokens.text : Tokens.dim
                                    opacity: row.past ? 0.6 : 1
                                }
                            }

                            Caption {
                                anchors.right: parent.left
                                anchors.rightMargin: -(day.rail - 14)
                                anchors.verticalCenter: parent.verticalCenter
                                visible: row.here
                                text: "ahora"
                                color: Tokens.accent
                            }

                            // El punto: hueco lo que viene, macizo lo próximo,
                            // pequeño y apagado lo que pasó.
                            Rectangle {
                                x: day.rail - width / 2
                                anchors.verticalCenter: parent.verticalCenter
                                width: row.here ? 10 : row.next ? 12 : row.past ? 6 : 10
                                height: width
                                radius: width / 2
                                // Lo que viene, del color de su calendario.
                                color: row.here ? Tokens.accent : row.next ? (Agenda.imminent ? Tokens.attention : Agenda.hue(row.event)) : row.past ? Tokens.dim : Tokens.plate
                                border.width: row.here || row.next || row.past ? 0 : 2
                                border.color: row.event ? Agenda.hue(row.event) : Tokens.dim

                                // El ahora late despacio.
                                Rectangle {
                                    anchors.centerIn: parent
                                    width: parent.width
                                    height: width
                                    radius: width / 2
                                    color: "transparent"
                                    border.width: 1
                                    border.color: Tokens.accent
                                    visible: row.here

                                    NumberAnimation on scale {
                                        running: row.here && root.shown
                                        from: 1
                                        to: 2.6
                                        duration: Tokens.breath
                                        easing.type: Easing.OutCubic
                                        loops: Animation.Infinite
                                    }

                                    NumberAnimation on opacity {
                                        running: row.here && root.shown
                                        from: 0.8
                                        to: 0
                                        duration: Tokens.breath
                                        easing.type: Easing.OutCubic
                                        loops: Animation.Infinite
                                    }
                                }
                            }

                            // El ahora: una raya del acento que se apaga hacia la derecha.
                            Rectangle {
                                x: day.rail + 14
                                anchors.verticalCenter: parent.verticalCenter
                                width: parent.width - x
                                height: 1
                                visible: row.here
                                gradient: Gradient {
                                    orientation: Gradient.Horizontal
                                    GradientStop { position: 0; color: Tokens.accent }
                                    GradientStop { position: 1; color: "transparent" }
                                }
                            }

                            Column {
                                x: day.rail + 18
                                width: parent.width - x
                                anchors.verticalCenter: parent.verticalCenter
                                visible: !row.here

                                Ink {
                                    width: parent.width
                                    text: row.event?.title ?? ""
                                    elide: Text.ElideRight
                                    strong: row.next
                                    dim: row.past
                                    font.strikeout: row.past
                                }

                                Ink {
                                    visible: row.next
                                    text: row.next ? root.until(row.event.at) : ""
                                    color: Agenda.imminent ? Tokens.attention : Tokens.accent
                                    font.pixelSize: Tokens.smallSize
                                }
                            }
                        }
                    }

                    Rule {
                        visible: (root.working > 0 || root.rows.length > 0) && (Countdown.running || root.kept > 0)
                    }

                    // El temporizador, si corre.
                    Item {
                        visible: Countdown.running
                        width: Tokens.lockDay
                        height: 36

                        Label {
                            anchors.verticalCenter: parent.verticalCenter
                            icon: "timer"
                            tone: Tokens.accent
                            text: "Temporizador"
                        }

                        Ink {
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            text: Countdown.exact
                        }
                    }

                    // Lo que ha llegado: cuántas, no qué; está bloqueada.
                    Item {
                        visible: root.kept > 0
                        width: Tokens.lockDay
                        height: 36

                        Label {
                            anchors.verticalCenter: parent.verticalCenter
                            icon: Notifications.quiet ? "bell-slash" : "bell"
                            tone: Tokens.dim
                            text: root.kept === 1 ? "1 notificación" : `${root.kept} notificaciones`
                        }
                    }
                }
            }

            // --- Bajo la hora: la puerta ---

            // El campo, alineado a la derecha con la hora. En reposo, tu
            // inicial y tu nombre; al escribir crece hacia la izquierda, letra
            // a letra, y sube a ocupar el sitio de lo próximo.
            Item {
                id: gate

                visible: root.main
                // A cada lado de los puntos, un icono.
                readonly property real side: Tokens.pad + Tokens.iconSize + 16
                readonly property real idle: greet.implicitWidth + Tokens.pad * 2 + Tokens.inset * 2
                readonly property real goal: root.typed ? Math.max(idle, root.count * root.step + side * 2) : idle

                anchors.right: face.right
                // La hora escala desde su derecha: el campo sigue su pie.
                y: face.y + face.height * (1 + face.scale) / 2 + Tokens.pad * 2 - lean.value * upcoming.height
                width: Math.max(0, brim.value)
                height: 58
                opacity: Math.max(0, rise.value)
                scale: (Lock.opening ? 1 : 0.94 + 0.06 * rise.value) * (1 + 0.025 * Math.max(-1, Math.min(1, beat.value)))
                transformOrigin: Item.Right
                transform: Translate {
                    x: shake.value
                }

                // Lo que mide, con un rebote leve al crecer o encoger.
                Spring {
                    id: brim
                    to: gate.goal
                    settle: Tokens.veil
                    bounce: 0.3
                }

                // Lo que ocupan los puntos: siempre centrados.
                Spring {
                    id: span
                    to: root.count * root.step
                    settle: Tokens.veil
                    bounce: 0.3
                }

                Rectangle {
                    anchors.fill: parent
                    radius: Tokens.round
                    color: Tokens.plate
                    border.width: Tokens.line
                    border.color: root.rim
                }

                Item {
                    id: well

                    anchors.fill: parent
                    clip: true

                    // Sin nada escrito: tu inicial y tu nombre, o lo que salió mal.
                    Row {
                        id: greet

                        anchors.centerIn: parent
                        spacing: 12
                        opacity: 1 - mode.value
                        visible: opacity > 0.01
                        scale: 0.9 + 0.1 * (1 - mode.value)

                        Rectangle {
                            anchors.verticalCenter: parent.verticalCenter
                            width: 32
                            height: width
                            radius: width / 2
                            color: Lock.note !== "" ? Tokens.critical : Tokens.accent

                            Behavior on color {
                                ColorAnimation { duration: Tokens.veil }
                            }

                            Ink {
                                anchors.centerIn: parent
                                visible: Lock.note === ""
                                text: root.user.charAt(0).toUpperCase()
                                color: Tokens.bg
                                font.pixelSize: Tokens.fontSize + 1
                                font.weight: Font.Bold
                            }

                            Icon {
                                anchors.centerIn: parent
                                visible: Lock.note !== ""
                                name: "warning"
                                color: Tokens.bg
                            }
                        }

                        Column {
                            anchors.verticalCenter: parent.verticalCenter

                            Ink {
                                text: root.user
                                strong: true
                            }

                            Ink {
                                text: Lock.note !== "" ? Lock.note : "Escribe tu contraseña"
                                color: Lock.note !== "" ? Tokens.critical : Tokens.dim
                                font.pixelSize: Tokens.smallSize
                            }
                        }
                    }

                    // Escribiendo: el candado, los puntos, el cursor e Intro.
                    Item {
                        anchors.fill: parent
                        opacity: mode.value
                        visible: opacity > 0.01

                        Icon {
                            id: padlock

                            property real spin: 0

                            x: Tokens.pad + 2
                            anchors.verticalCenter: parent.verticalCenter
                            name: Lock.checking ? "circle-notch" : root.wrong ? "warning-circle" : "lock"
                            color: root.wrong ? Tokens.critical : Lock.checking ? Tokens.accent : Tokens.dim
                            rotation: Lock.checking ? spin : 0

                            NumberAnimation on spin {
                                from: 0
                                to: 360
                                duration: Tokens.veil * 3
                                loops: Animation.Infinite
                                running: Lock.checking
                            }
                        }

                        Icon {
                            x: well.width - Tokens.pad - 2 - width
                            anchors.verticalCenter: parent.verticalCenter
                            name: "arrow-elbow-down-left"
                            opacity: Lock.checking || root.wrong ? 0 : 1

                            Behavior on opacity {
                                NumberAnimation { duration: Tokens.fast }
                            }
                        }

                        // Un punto por letra. Cada uno entra con un rebote desde
                        // abajo y se va hacia arriba; borrados de golpe, se van
                        // de derecha a izquierda, uno tras otro.
                        Repeater {
                            model: Tokens.lockDots

                            Rectangle {
                                id: dot

                                required property int index
                                readonly property bool on: index < root.count

                                Spring {
                                    id: here
                                    to: dot.on ? 1 : 0
                                    settle: dot.on ? Tokens.veil : Tokens.fast * 1.6 + Math.max(0, root.held - 1 - dot.index) * 22
                                    bounce: dot.on ? 0.45 : 0
                                }

                                x: well.width / 2 - span.value / 2 + index * root.step + (root.step - width) / 2
                                y: (well.height - height) / 2 + (1 - here.value) * (dot.on ? 12 : -12) - wave.value * 6 * Math.sin(root.phase - index * 0.55)
                                width: 10
                                height: width
                                radius: width / 2
                                color: root.ink
                                scale: Math.max(0, here.value)
                                opacity: Math.min(1, Math.max(0, here.value * 1.5))
                                visible: opacity > 0.01
                            }
                        }

                        // El cursor, tras el último punto.
                        Rectangle {
                            Spring {
                                id: caret
                                to: root.count > 0 && root.count < Tokens.lockDots && !Lock.checking && !root.wrong ? 1 : 0
                                settle: Tokens.fast
                            }

                            x: well.width / 2 + span.value / 2 + 1
                            anchors.verticalCenter: parent.verticalCenter
                            width: 2
                            height: 22
                            radius: 1
                            color: Tokens.accent
                            opacity: caret.value * root.blink
                            visible: opacity > 0.01
                        }
                    }
                }
            }

            // --- Arriba a la derecha: las excepciones, como en reposo ---

            Corner {
                anchors.right: parent.right
                mirrored: true
                shown: root.shown && root.main && exceptions.any

                Item {
                    width: exceptions.implicitWidth + Tokens.pad * 2
                    height: Tokens.island

                    Exceptions {
                        id: exceptions
                        x: Tokens.pad
                    }
                }
            }
        }
    }
}
