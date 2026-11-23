import QtQuick
import QtQuick.Shapes
import qs
import qs.state

// Un agente, en su propia isla: más baja y ligera que las otras, pero como
// ellas sale del borde de la pantalla, maciza y del color del fondo: cuelga
// del de arriba, con una curva hacia fuera a cada lado donde se une a él.
// Pequeña, lleva el logotipo de quien lo hace, quieto y del color de su
// marca, cuánto lleva en lo que está y, a su derecha, cómo va: un spinner
// mientras trabaja, un check verde al acabar y un punto de aviso si te
// necesita. Con el puntero encima (o Super mantenido) se estira y enseña en
// qué trabaja, en corto. Si te necesita y se ve qué pregunta (un permiso en
// su terminal), crece hacia abajo con la pregunta y sus opciones: pulsar una
// la contesta allí. Pulsar su cabeza te lleva a él.
Item {
    id: root

    required property string agent
    required property string phase      // working | needs | idle | gone
    required property string provider
    required property string name
    required property string window
    required property double since
    required property string ask
    required property string label
    required property string cwd
    readonly property bool needs: phase === "needs"
    readonly property color brand: Agents.vendor(provider)
    readonly property var question: {
        try {
            return ask ? JSON.parse(ask) : null;
        } catch (e) {
            return null;
        }
    }
    readonly property bool asking: needs && (question?.options?.length ?? 0) > 0
    // Vive en herdr: ir a él es ir a su pestaña.
    readonly property bool focusable: (Agents.list.get(Agents.find(agent))?.focus ?? "").indexOf("herdr") >= 0
    readonly property bool open: hover.hovered || Glance.open || asking
    // La opción que se acaba de pulsar, hasta que el agente cambia de pregunta.
    property string sent: ""
    onAskChanged: sent = ""

    // Con el puntero encima, sus teclas contestan: la de cada opción la
    // elige, Intro te lleva a él y Esc suelta el teclado (hasta que el
    // puntero salga y vuelva).
    property bool muted: false
    readonly property bool listening: asking && hover.hovered && sent === "" && !muted
    onListeningChanged: if (listening) Qt.callLater(() => keys.forceActiveFocus())

    function choose(key: string): void {
        Agents.answer(agent, key);
    }

    // Contestado desde donde sea (aquí, con sus teclas o con un atajo).
    Connections {
        target: Agents

        function onAnswered(agent: string, key: string): void {
            if (agent === root.agent)
                root.sent = key;
        }
    }

    Item {
        id: keys
        focus: root.listening
        Keys.onPressed: event => {
            if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                Agents.focus(root.agent);
            } else if (event.key === Qt.Key_Escape) {
                root.muted = true;
            } else {
                const option = (root.question?.options ?? []).find(o => o.key.toLowerCase() === event.text.toLowerCase());
                if (!option)
                    return;
                root.choose(option.key);
            }
            event.accepted = true;
        }
    }

    // Acaba de terminar sin que lo miraras: se queda a decirlo hasta que vas
    // a su ventana o la pulsas.
    property bool fresh: false
    property string was: phase
    onPhaseChanged: {
        if (was === "working" && phase === "idle" && Agents.active !== window) {
            fresh = true;
        } else if (phase !== "idle") {
            fresh = false;
        }
        if (phase === "needs")
            call();
        was = phase;
    }

    Connections {
        target: Agents

        function onActiveChanged(): void {
            if (Agents.active === root.window)
                root.fresh = false;
        }
    }

    // Te necesita: un tirón hacia abajo y un único pulso, y se queda fija.
    function call(): void {
        sway.velocity += 220;
        pulse.restart();
    }

    Component.onCompleted: if (needs) Qt.callLater(call)

    // Pequeña, la fila de siempre; preguntando, crece desde ella: la fila es su
    // cabeza (con dónde está a la derecha) y debajo salen la orden y las opciones.
    implicitWidth: (row.x * 2 + row.width) * (1 - grow.value) + Math.max(panel.width + panel.x * 2, row.x + row.width + where.implicitWidth + 30) * grow.value
    implicitHeight: Tokens.pod + (panel.implicitHeight + 14) * grow.value + sway.value

    // Dónde está, a la derecha de la cabeza, cuando pregunta.
    Text {
        id: where
        z: 1
        x: root.width - width - 14
        y: (Tokens.pod - height) / 2
        font.family: "JetBrains Mono"
        font.pixelSize: Tokens.smallSize
        color: Tokens.dim
        text: Agents.place(root.cwd, root.window)
        opacity: panel.opacity
        visible: opacity > 0
    }

    // Lo que crece cuando pregunta.
    Spring {
        id: grow
        to: root.asking ? 1 : 0
        settle: Tokens.veil
        bounce: 0.2
    }

    Spring {
        id: morph
        to: root.open ? 1 : 0
        settle: Tokens.veil
        bounce: 0.3
    }

    Spring {
        id: sway
        settle: Tokens.veil
        bounce: 0.3
    }

    Shape {
        id: plate

        readonly property real w: root.width
        readonly property real h: root.height
        readonly property real join: Tokens.gap
        readonly property real corner: Math.max(0, Math.min(h / 2, w / 2, Tokens.round))
        // El trazo empieza y acaba fuera de la pantalla: solo se ve el canto libre.
        readonly property real out: -Tokens.line * 2

        preferredRendererType: Shape.CurveRenderer

        ShapePath {
            fillColor: Tokens.plate
            strokeColor: root.needs && !root.asking ? Tokens.attention : Tokens.border
            strokeWidth: Tokens.line
            startX: -plate.join
            startY: plate.out

            PathLine { x: -plate.join; y: 0 }
            PathArc { x: 0; y: plate.join; radiusX: plate.join; radiusY: plate.join }
            PathLine { x: 0; y: plate.h - plate.corner }
            PathArc { x: plate.corner; y: plate.h; radiusX: plate.corner; radiusY: plate.corner; direction: PathArc.Counterclockwise }
            PathLine { x: plate.w - plate.corner; y: plate.h }
            PathArc { x: plate.w; y: plate.h - plate.corner; radiusX: plate.corner; radiusY: plate.corner; direction: PathArc.Counterclockwise }
            PathLine { x: plate.w; y: plate.join }
            PathArc { x: plate.w + plate.join; y: 0; radiusX: plate.join; radiusY: plate.join }
            PathLine { x: plate.w + plate.join; y: plate.out }
            PathLine { x: -plate.join; y: plate.out }
        }
    }

    HoverHandler {
        id: hover
        onHoveredChanged: if (!hovered) root.muted = false
        cursorShape: root.window ? Qt.PointingHandCursor : Qt.ArrowCursor
    }

    // La cabeza lleva a él; lo de debajo, si pregunta, son sus opciones.
    Item {
        width: parent.width
        height: Tokens.pod
        enabled: !root.asking

        TapHandler {
            onTapped: {
                root.fresh = false;
                Agents.focus(root.agent);
            }
        }
    }

    Row {
        id: row
        x: Tokens.pad - 4
        y: (Tokens.pod - height) / 2

        Logo {
            anchors.verticalCenter: parent.verticalCenter
            width: Tokens.smallSize + 1
            height: width
            source: Agents.logo(root.provider)
            color: root.brand
            visible: root.provider !== "pi"
        }

        // pi no tiene logotipo: su letra.
        Ink {
            anchors.verticalCenter: parent.verticalCenter
            width: Tokens.smallSize + 1
            horizontalAlignment: Text.AlignHCenter
            strong: true
            font.variableAxes: ({ "wght": 650 })
            color: root.brand
            text: "π"
            visible: root.provider === "pi"
        }

        // En qué trabaja: solo abierta. Se abre su hueco y el texto entra
        // cuando ya casi cabe.
        Item {
            anchors.verticalCenter: parent.verticalCenter
            // Plegada, solo un poco de aire entre logotipo y estado.
            width: Tokens.gap - 2 + (label.width + Tokens.gap + 2) * Math.max(0, morph.value)
            height: label.height
            clip: true

            // Si pregunta, su pregunta, con sus palabras y en negrita.
            Ink {
                id: label
                x: Tokens.gap
                text: root.asking ? (root.question?.question || root.question?.title || root.name) : root.name
                color: root.needs && !root.asking ? Tokens.attention : Tokens.text
                font.pixelSize: root.asking ? Tokens.fontSize : Tokens.smallSize
                strong: root.asking
                font.variableAxes: ({ "wght": root.asking ? 650 : 500 })
                elide: Text.ElideRight
                // Preguntando, todo lo que deje libre la cabeza de la isla abierta.
                width: Math.min(implicitWidth, root.asking ? panel.width + panel.x * 2 - row.x - 2 * Tokens.smallSize - clock.width - where.implicitWidth - Tokens.gap * 5 : 130)
                opacity: Math.min(1, Math.max(0, (morph.value - 0.5) * 2.5))
            }
        }

        // Cuánto lleva trabajando (o esperándote): siempre igual, gris y
        // pegado a cómo va.
        Ink {
            id: clock
            anchors.verticalCenter: parent.verticalCenter
            rightPadding: Tokens.gap - 2
            font.pixelSize: Tokens.smallSize - 1
            dim: true
            text: Agents.elapsed(root.since)
            visible: root.phase === "working" || root.needs
        }

        // Cómo va: spinner, check o aviso, en el mismo sitio.
        Item {
            id: status

            readonly property real size: Tokens.smallSize + 1

            anchors.verticalCenter: parent.verticalCenter
            width: size
            height: size

            // Trabajando: un arco del color de su marca que gira sobre un
            // carril apenas visible.
            Item {
                anchors.fill: parent
                opacity: root.phase === "working" ? 1 : 0
                visible: opacity > 0

                Behavior on opacity {
                    NumberAnimation { duration: Tokens.fast }
                }

                Rectangle {
                    anchors.fill: parent
                    radius: width / 2
                    color: "transparent"
                    border.width: 1.5
                    border.color: Qt.alpha(root.brand, 0.2)
                }

                Shape {
                    id: spinner

                    anchors.fill: parent
                    preferredRendererType: Shape.CurveRenderer

                    ShapePath {
                        fillColor: "transparent"
                        strokeColor: root.brand
                        strokeWidth: 1.5
                        capStyle: ShapePath.RoundCap

                        PathAngleArc {
                            centerX: status.size / 2
                            centerY: status.size / 2
                            radiusX: status.size / 2 - 0.75
                            radiusY: status.size / 2 - 0.75
                            startAngle: -90
                            sweepAngle: 100
                        }
                    }

                    RotationAnimation on rotation {
                        running: spinner.visible
                        loops: Animation.Infinite
                        from: 0
                        to: 360
                        duration: Tokens.breath / 3
                    }
                }
            }

            // Listo: un check verde.
            Icon {
                anchors.centerIn: parent
                name: "check"
                color: Tokens.done
                font.pixelSize: status.size
                font.weight: Font.Bold
                opacity: root.fresh ? 1 : 0
                scale: root.fresh ? 1 : 0.4

                Behavior on opacity {
                    NumberAnimation { duration: Tokens.fast }
                }

                Behavior on scale {
                    NumberAnimation { duration: Tokens.veil; easing.type: Easing.OutBack }
                }
            }

            // Te necesita: un punto de aviso, con un único pulso.
            Rectangle {
                anchors.centerIn: parent
                width: Tokens.dot
                height: width
                radius: width / 2
                color: Tokens.attention
                opacity: root.needs ? 1 : 0

                Behavior on opacity {
                    NumberAnimation { duration: Tokens.fast }
                }
            }

            Rectangle {
                id: ring
                anchors.centerIn: parent
                width: Tokens.dot
                height: width
                radius: width / 2
                color: "transparent"
                border.width: Tokens.line
                border.color: Tokens.attention
                opacity: 0
            }

            ParallelAnimation {
                id: pulse
                NumberAnimation { target: ring; property: "scale"; from: 1; to: 3; duration: Tokens.veil * 2; easing.type: Easing.OutCubic }
                NumberAnimation { target: ring; property: "opacity"; from: 0.9; to: 0; duration: Tokens.veil * 2; easing.type: Easing.OutCubic }
            }
        }
    }

    // Lo que pregunta, debajo de la cabeza de la isla (que lleva su pregunta):
    // de qué va y sobre qué (la orden, el archivo, con lo que añade) y sus
    // opciones, todo con sus palabras, cada opción con su tecla y la primera
    // en el acento. A la derecha, ir a él.
    // Si es una orden, lleva su «$» delante.
    readonly property bool command: /bash|command|shell|exec/.test(`${question?.title ?? ""} ${question?.question ?? ""}`.toLowerCase())

    Column {
        id: panel

        x: 12
        y: Tokens.pod + 2
        width: 520
        spacing: 12
        opacity: Math.min(1, Math.max(0, (grow.value - 0.5) * 2))
        visible: opacity > 0

        // De qué va («Bash command») y sobre qué: la orden (con su «$») o el
        // archivo, y debajo lo que añade.
        Rectangle {
            width: parent.width
            height: about.implicitHeight + 24
            radius: Tokens.radius
            color: Tokens.surface
            visible: (root.question?.subject ?? "") !== "" || (root.question?.title ?? "") !== ""

            Column {
                id: about
                x: 14
                y: 12
                width: parent.width - 28
                spacing: 4

            Ink {
                width: parent.width
                dim: true
                font.pixelSize: Tokens.smallSize - 1
                elide: Text.ElideRight
                text: root.question?.title ?? ""
                visible: text !== "" && text !== root.question?.question
            }

            Text {
                id: subject
                width: parent.width
                wrapMode: Text.WrapAnywhere
                maximumLineCount: 3
                elide: Text.ElideRight
                font.family: "JetBrains Mono"
                font.pixelSize: Tokens.fontSize
                color: Tokens.text
                textFormat: Text.StyledText
                text: (root.command ? `<font color="${Tokens.dim}">$</font> ` : "") + (root.question?.subject ?? "").replace(/&/g, "&amp;").replace(/</g, "&lt;")
                visible: (root.question?.subject ?? "") !== ""
            }

            Ink {
                width: parent.width
                dim: true
                font.pixelSize: Tokens.smallSize
                wrapMode: Text.WordWrap
                maximumLineCount: 2
                elide: Text.ElideRight
                text: root.question?.note ?? ""
                visible: text !== ""
            }
            }
        }

        // Sus opciones, en fila, y a la derecha ir a él.
        Item {
            width: parent.width
            height: Math.max(choices.height, 34)

            Flow {
                id: choices
                width: parent.width - open.width - 12
                spacing: 8

                Repeater {
                    model: root.question?.options ?? []

                    Rectangle {
                        id: choice

                        required property var modelData
                        required property int index
                        readonly property bool first: index === 0
                        readonly property bool chosen: root.sent === modelData.key

                        // Lo que mide su texto entero, en una línea: si cabe, el botón
                        // es justo eso; si no, lo que quede, y el texto baja de línea.
                        width: Math.min(choices.width, Math.ceil(Math.max(fit.advanceWidth, fitMore.advanceWidth)) + key.implicitWidth + 34)
                        height: Math.max(34, caption.implicitHeight + 14)
                        radius: Tokens.radius
                        color: first ? (tap.containsMouse ? Qt.lighter(Tokens.accent, 1.12) : Tokens.accent) : (tap.containsMouse ? Qt.lighter(Tokens.surface, 1.35) : Tokens.surface)
                        opacity: root.sent !== "" && !chosen ? 0.4 : 1

                        Behavior on color {
                            ColorAnimation { duration: Tokens.fast }
                        }

                        Text {
                            id: key
                            x: 12
                            anchors.verticalCenter: parent.verticalCenter
                            font.family: "JetBrains Mono"
                            font.pixelSize: Tokens.smallSize - 1
                            color: choice.first ? Qt.alpha(Tokens.bg, 0.7) : root.listening ? Tokens.accent : Tokens.dim
                            font.weight: root.listening ? Font.Bold : Font.Normal
                            text: choice.modelData.key
                        }

                        Column {
                            id: caption
                            x: key.x + key.implicitWidth + 7
                            width: parent.width - x - 12
                            anchors.verticalCenter: parent.verticalCenter

                            TextMetrics {
                                id: fit
                                font: words.font
                                text: words.text
                            }

                            TextMetrics {
                                id: fitMore
                                font: more.font
                                text: more.text
                            }

                            Ink {
                                id: words
                                width: parent.width
                                wrapMode: Text.WordWrap
                                maximumLineCount: 2
                                elide: Text.ElideRight
                                color: choice.first ? Tokens.bg : Tokens.text
                                text: choice.modelData.label
                            }

                            Ink {
                                id: more
                                width: parent.width
                                font.pixelSize: Tokens.smallSize - 1
                                elide: Text.ElideRight
                                color: choice.first ? Qt.alpha(Tokens.bg, 0.7) : Tokens.dim
                                text: choice.modelData.detail ?? ""
                                visible: text !== ""
                            }
                        }

                        MouseArea {
                            id: tap
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: root.sent === "" ? Qt.PointingHandCursor : Qt.ArrowCursor
                            enabled: root.sent === ""
                            onClicked: root.choose(choice.modelData.key)
                        }
                    }
                }
            }

            // Ir a él, a su pestaña.
            Row {
                id: open
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                spacing: 7

                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    width: 22
                    height: 22
                    radius: 5
                    color: Tokens.surface

                    Icon {
                        anchors.centerIn: parent
                        name: "arrow-elbow-down-left"
                        font.pixelSize: 13
                        color: root.listening ? Tokens.accent : Tokens.dim
                    }
                }

                Ink {
                    anchors.verticalCenter: parent.verticalCenter
                    font.pixelSize: Tokens.smallSize
                    color: go.containsMouse ? Tokens.text : Tokens.dim
                    text: root.focusable ? "abrir en herdr" : "abrir"
                }
            }

            MouseArea {
                id: go
                anchors.fill: open
                anchors.margins: -6
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: Agents.focus(root.agent)
            }
        }
    }
}
