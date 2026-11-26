import QtQuick
import QtQuick.Shapes
import qs
import qs.state

// Isla de NixOS / Flake: cuelga del borde de arriba junto a la hora, con la
// misma geometría ligera (`Tokens.pod`) que los agentes.
//
// En reposo enseña lo mínimo (generación actual en pizarra, o cuántas
// actualizaciones hay, o «Reconstruir» si el flake cambió). Mientras actualiza
// o reconstruye enseña la fase, el tiempo y un spinner del acento; si `sudo`
// pide contraseña, se abre sola hacia abajo con su campo integrado.
// Con el puntero encima (o al pulsarla) se abre el cajón con las entradas
// desactualizadas, la salida en vivo y las acciones rápidas.
Item {
    id: root

    property bool shown: true
    property bool open: false
    property bool pinned: false
    property double now: Date.now()
    readonly property bool authNeeded: Nix.authNeeded
    readonly property bool running: Nix.running
    readonly property bool failed: Nix.jobState === "error"
    readonly property bool done: Nix.jobState === "done"
    readonly property bool expanded: authNeeded || pinned || open
    readonly property bool typing: authNeeded || passField.activeFocus

    readonly property bool wanted: hover.hovered || authNeeded || pinned || passField.activeFocus
    onWantedChanged: {
        if (wanted) {
            grace.stop();
            if (!open && !pinned && !authNeeded)
                intent.restart();
        } else {
            intent.stop();
            if (open)
                grace.restart();
        }
    }

    onAuthNeededChanged: {
        if (authNeeded) {
            passField.text = "";
            nudge();
            Qt.callLater(() => passField.forceActiveFocus());
        } else {
            passField.text = "";
        }
    }

    onFailedChanged: if (failed) nudge()
    onDoneChanged: if (done) nudge()

    function nudge(): void {
        sway.velocity += 200;
        pulse.restart();
    }

    // El icono principal y su color según el rol de lo que pasa.
    readonly property color iconTone: {
        if (authNeeded || Nix.rebootNeeded)
            return Tokens.attention;
        if (failed)
            return Tokens.critical;
        if (done)
            return Tokens.done;
        if (running || Nix.updates.length > 0 || Nix.pendingRebuild)
            return Tokens.accent;
        return Tokens.dim;
    }

    readonly property string pillText: {
        if (authNeeded)
            return "Contraseña";
        if (running)
            return Nix.job?.phase || "En marcha…";
        if (failed)
            return "Error";
        if (done)
            return Nix.job?.summary || "Listo";
        if (Nix.updates.length > 0)
            return `↑ ${Nix.updates.length}`;
        if (Nix.pendingRebuild)
            return "Reconstruir";
        if (Nix.rebootNeeded)
            return "Reiniciar";
        if (Nix.dirty > 0 && Glance.open)
            return `${Nix.dirty} mod.`;
        if (Nix.generation > 0)
            return `#${Nix.generation}`;
        return Nix.flakeName || "Nix";
    }

    readonly property color pillTextColor: {
        if (authNeeded || Nix.rebootNeeded)
            return Tokens.attention;
        if (failed)
            return Tokens.critical;
        if (done)
            return Tokens.done;
        if (running || Nix.updates.length > 0 || Nix.pendingRebuild)
            return Tokens.text;
        return Tokens.dim;
    }

    readonly property real pillWidth: headRow.implicitWidth + (Tokens.pad - 4) * 2
    readonly property real drawerWidth: 384

    implicitWidth: (pillWidth * (1 - grow.value) + Math.max(pillWidth, drawerWidth) * grow.value) * appear.value
    implicitHeight: (Tokens.pod + (drawer.implicitHeight + 14) * grow.value + sway.value) * appear.value
    visible: appear.value > 0.01

    Timer {
        id: intent
        interval: 120
        onTriggered: root.open = true
    }

    Timer {
        id: grace
        interval: 280
        onTriggered: root.open = false
    }

    Timer {
        running: root.visible && (root.running || root.expanded)
        interval: root.running ? 250 : 30000
        repeat: true
        triggeredOnStart: true
        onTriggered: root.now = Date.now()
    }

    Spring {
        id: appear
        to: root.shown ? 1 : 0
        settle: Tokens.veil
    }

    Spring {
        id: grow
        to: root.expanded ? 1 : 0
        settle: Tokens.veil
        bounce: 0.2
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
        readonly property real join: Math.min(Tokens.gap, h / 2)
        readonly property real corner: Math.max(0, Math.min(h / 2, w / 2, Tokens.round))
        readonly property real out: -Tokens.line * 2

        preferredRendererType: Shape.CurveRenderer
        visible: h > 0 && w > 0

        ShapePath {
            fillColor: Tokens.plate
            strokeColor: root.authNeeded ? Tokens.attention : root.failed ? Tokens.critical : Tokens.border
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
    }

    // Pulsar la cabeza fija o suelta el cajón.
    Item {
        width: parent.width
        height: Tokens.pod

        TapHandler {
            onTapped: {
                if (root.authNeeded)
                    return;
                root.pinned = !root.pinned;
                if (!root.pinned)
                    root.open = false;
            }
        }
    }

    // Fila de la cabeza: pegada a la derecha para no saltar al desplegarse.
    Row {
        id: headRow
        anchors.right: parent.right
        anchors.rightMargin: Tokens.pad - 4
        y: (Tokens.pod - height) / 2
        spacing: 6

        Icon {
            anchors.verticalCenter: parent.verticalCenter
            name: "snowflake"
            color: root.iconTone
            font.pixelSize: Tokens.smallSize + 1
        }

        Ink {
            anchors.verticalCenter: parent.verticalCenter
            text: root.pillText
            color: root.pillTextColor
            font.pixelSize: Tokens.smallSize
            elide: Text.ElideRight
            width: Math.min(implicitWidth, 160)
        }

        Ink {
            anchors.verticalCenter: parent.verticalCenter
            text: Nix.elapsedText(Nix.job?.since ?? 0, root.now)
            dim: true
            font.pixelSize: Tokens.smallSize - 1
            visible: root.running && text !== ""
        }

        // Indicador final: spinner, check, aviso o punto de contraseña.
        Item {
            id: indicator

            readonly property real size: Tokens.smallSize + 1

            anchors.verticalCenter: parent.verticalCenter
            width: root.running || root.done || root.failed || root.authNeeded ? size : 0
            height: size
            visible: width > 0

            // En marcha: arco girando, como en los agentes.
            Item {
                anchors.fill: parent
                visible: root.running

                Rectangle {
                    anchors.fill: parent
                    radius: width / 2
                    color: "transparent"
                    border.width: 1.5
                    border.color: Qt.alpha(Tokens.accent, 0.2)
                }

                Shape {
                    id: spinner
                    anchors.fill: parent
                    preferredRendererType: Shape.CurveRenderer

                    ShapePath {
                        fillColor: "transparent"
                        strokeColor: Tokens.accent
                        strokeWidth: 1.5
                        capStyle: ShapePath.RoundCap

                        PathAngleArc {
                            centerX: indicator.size / 2
                            centerY: indicator.size / 2
                            radiusX: indicator.size / 2 - 0.75
                            radiusY: indicator.size / 2 - 0.75
                            startAngle: -90
                            sweepAngle: 100
                        }
                    }

                    RotationAnimation on rotation {
                        running: root.running && root.visible
                        loops: Animation.Infinite
                        from: 0
                        to: 360
                        duration: Tokens.breath / 3
                    }
                }
            }

            Icon {
                anchors.centerIn: parent
                name: "check"
                color: Tokens.done
                font.pixelSize: indicator.size
                font.weight: Font.Bold
                visible: root.done
            }

            Icon {
                anchors.centerIn: parent
                name: "warning"
                color: Tokens.critical
                font.pixelSize: indicator.size
                visible: root.failed
            }

            Rectangle {
                anchors.centerIn: parent
                width: Tokens.dot
                height: width
                radius: width / 2
                color: Tokens.attention
                visible: root.authNeeded
            }

            Rectangle {
                id: ring
                anchors.centerIn: parent
                width: Tokens.dot
                height: width
                radius: width / 2
                color: "transparent"
                border.width: Tokens.line
                border.color: root.failed ? Tokens.critical : Tokens.attention
                opacity: 0
            }

            ParallelAnimation {
                id: pulse
                NumberAnimation { target: ring; property: "scale"; from: 1; to: 3; duration: Tokens.veil * 2; easing.type: Easing.OutCubic }
                NumberAnimation { target: ring; property: "opacity"; from: 0.9; to: 0; duration: Tokens.veil * 2; easing.type: Easing.OutCubic }
            }
        }
    }

    // Título a la izquierda de la cabeza cuando está desplegada.
    Row {
        x: 14
        y: (Tokens.pod - height) / 2
        spacing: 6
        opacity: drawer.opacity
        visible: opacity > 0

        Ink {
            anchors.verticalCenter: parent.verticalCenter
            text: Nix.nixos ? "NixOS" : "Nix"
            strong: true
            font.pixelSize: Tokens.smallSize
        }

        Ink {
            anchors.verticalCenter: parent.verticalCenter
            text: Nix.flakeName ? `· ${Nix.flakeName}` : ""
            dim: true
            font.pixelSize: Tokens.smallSize
            visible: text !== ""
        }
    }

    // Un botón compacto de acción dentro del cajón.
    component Action: Press {
        id: btn

        property string icon
        property string text
        property bool primary: false
        property bool danger: false
        property bool off: false

        height: 32
        width: text ? btnRow.implicitWidth + 22 : height
        radius: height / 2
        active: !off
        opacity: off ? 0.45 : 1
        rest: primary ? Tokens.accent : danger ? Qt.alpha(Tokens.critical, 0.18) : Tokens.surface

        Row {
            id: btnRow
            anchors.centerIn: parent
            spacing: 6

            Icon {
                anchors.verticalCenter: parent.verticalCenter
                name: btn.icon
                color: btn.primary ? Tokens.bg : btn.danger ? Tokens.critical : Tokens.dim
                font.pixelSize: Tokens.iconSize - 3
                visible: btn.icon !== ""
            }

            Ink {
                anchors.verticalCenter: parent.verticalCenter
                text: btn.text
                color: btn.primary ? Tokens.bg : btn.danger ? Tokens.critical : Tokens.text
                font.pixelSize: Tokens.smallSize
                font.weight: btn.primary ? Font.DemiBold : Font.Normal
                visible: btn.text !== ""
            }
        }
    }

    // Cajón desplegable con estado, contraseña de sudo, log en vivo, actualizaciones y botones.
    Item {
        id: clipBox
        anchors.fill: parent
        clip: true

        Column {
            id: drawer
            x: 12
            y: Tokens.pod + 6
            width: root.drawerWidth - 24
            spacing: 8
            opacity: Math.min(1, Math.max(0, (grow.value - 0.45) * 2))
            visible: opacity > 0

            // Cabecera: generación, versión y atajos (comprobar, terminal, ajustes).
            Item {
                width: parent.width
                height: 36

                Column {
                    anchors.left: parent.left
                    anchors.right: topTools.left
                    anchors.rightMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 1

                    Ink {
                        width: parent.width
                        text: Nix.generation > 0 ? `Generación #${Nix.generation}` : (Nix.flakeName || "Sistema Nix")
                        strong: true
                        elide: Text.ElideRight
                    }

                    Ink {
                        width: parent.width
                        text: [Nix.version, Nix.builtAt ? Nix.ageText(Nix.builtAt, root.now) : "", Nix.flake ? Nix.flake.replace(Quickshell.env("HOME"), "~") : ""].filter(p => p).join(" · ")
                        dim: true
                        font.pixelSize: Tokens.tinySize
                        elide: Text.ElideRight
                        visible: text !== ""
                    }
                }

                Row {
                    id: topTools
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 4

                    Press {
                        width: 28
                        height: 28
                        radius: 14
                        rest: Tokens.surface
                        visible: Nix.flake !== ""
                        active: !Nix.checking
                        onTapped: Nix.checkUpdates()

                        Icon {
                            anchors.centerIn: parent
                            name: "arrows-clockwise"
                            color: Nix.checking ? Tokens.accent : Tokens.dim
                            font.pixelSize: 15

                            RotationAnimation on rotation {
                                running: Nix.checking && root.visible
                                loops: Animation.Infinite
                                from: 0
                                to: 360
                                duration: 1000
                            }
                        }
                    }

                    Press {
                        width: 28
                        height: 28
                        radius: 14
                        rest: Tokens.surface
                        visible: Nix.flake !== ""
                        onTapped: {
                            root.pinned = false;
                            root.open = false;
                            Nix.openFlakeTerminal();
                        }

                        Icon {
                            anchors.centerIn: parent
                            name: "terminal"
                            color: Tokens.dim
                            font.pixelSize: 15
                        }
                    }

                    Press {
                        width: 28
                        height: 28
                        radius: 14
                        rest: Tokens.surface
                        onTapped: {
                            root.pinned = false;
                            root.open = false;
                            Tuning.show("", "nix");
                        }

                        Icon {
                            anchors.centerIn: parent
                            name: "gear-six"
                            color: Tokens.dim
                            font.pixelSize: 15
                        }
                    }
                }
            }

            // Campo de contraseña de sudo cuando `nh` / `sudo` pide elevación.
            Rectangle {
                width: parent.width
                height: authCol.implicitHeight + 20
                radius: Tokens.radius
                color: Tokens.surface
                border.width: Tokens.line
                border.color: Tokens.attention
                visible: root.authNeeded

                Column {
                    id: authCol
                    x: 12
                    y: 10
                    width: parent.width - 24
                    spacing: 8

                    Row {
                        spacing: 6

                        Icon {
                            anchors.verticalCenter: parent.verticalCenter
                            name: "lock"
                            color: Tokens.attention
                            font.pixelSize: Tokens.smallSize + 1
                        }

                        Ink {
                            anchors.verticalCenter: parent.verticalCenter
                            text: (Nix.auth?.attempt ?? 1) > 1 ? "Contraseña incorrecta, inténtalo de nuevo" : "Contraseña de sudo para activar"
                            color: Tokens.attention
                            font.pixelSize: Tokens.smallSize
                            strong: true
                        }
                    }

                    Rectangle {
                        width: parent.width
                        height: 36
                        radius: Tokens.soft
                        color: Tokens.bg
                        border.width: Tokens.line
                        border.color: passField.activeFocus ? Tokens.accent : Tokens.border

                        TextInput {
                            id: passField
                            anchors.fill: parent
                            anchors.leftMargin: 10
                            anchors.rightMargin: 10
                            verticalAlignment: TextInput.AlignVCenter
                            echoMode: TextInput.Password
                            passwordCharacter: "•"
                            color: Tokens.text
                            selectionColor: Tokens.accent
                            selectedTextColor: Tokens.bg
                            font.family: Tokens.sans
                            font.pixelSize: Tokens.fontSize
                            clip: true
                            onAccepted: {
                                if (text.length > 0) {
                                    const pwd = text;
                                    text = "";
                                    Nix.submitAuth(pwd);
                                }
                            }
                            Keys.onEscapePressed: {
                                text = "";
                                Nix.cancelAuth();
                            }
                        }

                        Ink {
                            anchors.left: parent.left
                            anchors.leftMargin: 10
                            anchors.verticalCenter: parent.verticalCenter
                            text: "Escribe tu contraseña y pulsa Intro"
                            dim: true
                            font.pixelSize: Tokens.smallSize
                            visible: passField.text.length === 0
                        }
                    }

                    Row {
                        anchors.right: parent.right
                        spacing: 6

                        Action {
                            text: "Cancelar"
                            onTapped: {
                                passField.text = "";
                                Nix.cancelAuth();
                            }
                        }

                        Action {
                            icon: "check"
                            text: "Confirmar"
                            primary: true
                            off: passField.text.length === 0
                            onTapped: {
                                const pwd = passField.text;
                                passField.text = "";
                                Nix.submitAuth(pwd);
                            }
                        }
                    }
                }
            }

            // Trabajo en marcha o recién acabado (con cola de salida en vivo).
            Rectangle {
                width: parent.width
                height: jobCol.implicitHeight + 20
                radius: Tokens.radius
                color: Tokens.surface
                border.width: root.failed ? Tokens.line : 0
                border.color: Tokens.critical
                visible: Nix.job !== null

                Column {
                    id: jobCol
                    x: 12
                    y: 10
                    width: parent.width - 24
                    spacing: 8

                    Item {
                        width: parent.width
                        height: 22

                        Row {
                            anchors.left: parent.left
                            anchors.right: jobBtns.left
                            anchors.rightMargin: 8
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 6

                            Icon {
                                anchors.verticalCenter: parent.verticalCenter
                                name: root.failed ? "warning" : root.done ? "check-circle" : (Nix.job?.kind === "update" ? "arrows-clockwise" : "hammer")
                                color: root.failed ? Tokens.critical : root.done ? Tokens.done : Tokens.accent
                                font.pixelSize: 15
                            }

                            Ink {
                                anchors.verticalCenter: parent.verticalCenter
                                width: parent.width - 22
                                text: root.running ? (Nix.job?.phase || "En marcha…") : (Nix.job?.summary || Nix.job?.phase || "")
                                color: root.failed ? Tokens.critical : Tokens.text
                                font.pixelSize: Tokens.smallSize
                                strong: true
                                elide: Text.ElideRight
                            }
                        }

                        Row {
                            id: jobBtns
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 6

                            Ink {
                                anchors.verticalCenter: parent.verticalCenter
                                text: Nix.elapsedText(Nix.job?.since ?? 0, root.now)
                                dim: true
                                font.pixelSize: Tokens.tinySize
                                visible: root.running
                            }

                            Press {
                                width: 22
                                height: 22
                                radius: 11
                                visible: root.running && !(Nix.job?.external ?? false)
                                onTapped: Nix.stopJob()

                                Icon {
                                    anchors.centerIn: parent
                                    name: "x-circle"
                                    color: Tokens.dim
                                    font.pixelSize: 15
                                }
                            }

                            Press {
                                width: 22
                                height: 22
                                radius: 11
                                visible: !root.running
                                onTapped: Nix.dismissJob()

                                Icon {
                                    anchors.centerIn: parent
                                    name: "x"
                                    color: Tokens.dim
                                    font.pixelSize: 14
                                }
                            }
                        }
                    }

                    // Últimas líneas de salida.
                    Rectangle {
                        width: parent.width
                        height: logText.implicitHeight + 12
                        radius: Tokens.soft
                        color: Tokens.bg
                        visible: (Nix.job?.log?.length ?? 0) > 0

                        Text {
                            id: logText
                            x: 8
                            y: 6
                            width: parent.width - 16
                            font.family: "JetBrains Mono"
                            font.pixelSize: Tokens.tinySize
                            color: Tokens.dim
                            wrapMode: Text.WrapAnywhere
                            maximumLineCount: 6
                            elide: Text.ElideRight
                            text: (Nix.job?.log ?? []).slice(-6).join("\n")
                        }
                    }

                    // Acciones de log / reintento.
                    Row {
                        spacing: 6
                        visible: !root.running || !!(Nix.job?.logFile)

                        Action {
                            icon: "file-text"
                            text: "Ver registro"
                            visible: !!(Nix.job?.logFile) && (Nix.job?.log?.length ?? 0) > 0
                            onTapped: Nix.openLog()
                        }

                        Action {
                            icon: "terminal"
                            text: "En terminal"
                            visible: root.failed && Nix.rebuild !== ""
                            onTapped: {
                                root.pinned = false;
                                root.open = false;
                                Nix.build();
                            }
                        }
                    }
                }
            }

            // Avisos rápidos del sistema (reinicio por kernel, flake pendiente, ajustes sin guardar).
            Column {
                width: parent.width
                spacing: 6
                visible: Nix.rebootNeeded || (Nix.pendingRebuild && !root.running) || (Nix.dirty > 0 && !Nix.lockNewer && !root.running) || (Nix.declared && Nix.unsaved.length > 0)

                Rectangle {
                    width: parent.width
                    height: 34
                    radius: Tokens.radius
                    color: Qt.alpha(Tokens.attention, 0.12)
                    visible: Nix.rebootNeeded

                    Row {
                        x: 12
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 8

                        Icon {
                            anchors.verticalCenter: parent.verticalCenter
                            name: "arrow-clockwise"
                            color: Tokens.attention
                            font.pixelSize: 15
                        }

                        Ink {
                            anchors.verticalCenter: parent.verticalCenter
                            text: "Kernel nuevo · reinicia para aplicarlo"
                            color: Tokens.attention
                            font.pixelSize: Tokens.smallSize
                        }
                    }
                }

                Rectangle {
                    width: parent.width
                    height: 34
                    radius: Tokens.radius
                    color: Qt.alpha(Tokens.accent, 0.12)
                    visible: Nix.pendingRebuild && !root.running

                    Row {
                        x: 12
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 8

                        Icon {
                            anchors.verticalCenter: parent.verticalCenter
                            name: "hammer"
                            color: Tokens.accent
                            font.pixelSize: 15
                        }

                        Ink {
                            anchors.verticalCenter: parent.verticalCenter
                            text: Nix.lockNewer ? "flake.lock actualizado · falta reconstruir" : `${Nix.unbuilt.length} ajustes guardados · falta reconstruir`
                            color: Tokens.text
                            font.pixelSize: Tokens.smallSize
                        }
                    }
                }

                Rectangle {
                    width: parent.width
                    height: 32
                    radius: Tokens.radius
                    color: Tokens.surface
                    visible: Nix.dirty > 0 && !Nix.lockNewer && !root.running

                    Row {
                        x: 12
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 8

                        Icon {
                            anchors.verticalCenter: parent.verticalCenter
                            name: "git-diff"
                            color: Tokens.dim
                            font.pixelSize: 15
                        }

                        Ink {
                            anchors.verticalCenter: parent.verticalCenter
                            text: `${Nix.dirty} ${Nix.dirty === 1 ? "cambio local" : "cambios locales"} en ${Nix.flakeName || "flake"}`
                            dim: true
                            font.pixelSize: Tokens.smallSize
                        }
                    }
                }

                Press {
                    width: parent.width
                    height: 34
                    radius: Tokens.radius
                    rest: Qt.alpha(Tokens.attention, 0.12)
                    visible: Nix.declared && Nix.unsaved.length > 0
                    onTapped: {
                        root.pinned = false;
                        root.open = false;
                        Tuning.show("", "nix");
                    }

                    Row {
                        x: 12
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 8

                        Icon {
                            anchors.verticalCenter: parent.verticalCenter
                            name: "sliders"
                            color: Tokens.attention
                            font.pixelSize: 15
                        }

                        Ink {
                            anchors.verticalCenter: parent.verticalCenter
                            text: `${Nix.unsaved.length} ${Nix.unsaved.length === 1 ? "ajuste" : "ajustes"} de Veyl sin guardar en Nix`
                            color: Tokens.attention
                            font.pixelSize: Tokens.smallSize
                        }
                    }
                }
            }

            // Actualizaciones de entradas del flake.
            Rectangle {
                width: parent.width
                height: updCol.implicitHeight + 20
                radius: Tokens.radius
                color: Tokens.surface
                visible: Nix.flake !== ""

                Column {
                    id: updCol
                    x: 12
                    y: 10
                    width: parent.width - 24
                    spacing: 6

                    Item {
                        width: parent.width
                        height: 20

                        Row {
                            anchors.left: parent.left
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 6

                            Icon {
                                anchors.verticalCenter: parent.verticalCenter
                                name: Nix.updates.length > 0 ? "arrow-circle-up" : "check-circle"
                                color: Nix.updates.length > 0 ? Tokens.accent : Tokens.dim
                                font.pixelSize: 15
                            }

                            Ink {
                                anchors.verticalCenter: parent.verticalCenter
                                text: Nix.checking ? "Comprobando entradas…" : Nix.updates.length > 0 ? `${Nix.updates.length} ${Nix.updates.length === 1 ? "actualización" : "actualizaciones"}` : Nix.checkedAt > 0 ? "Flake al día" : "Actualizaciones del flake"
                                font.pixelSize: Tokens.smallSize
                                strong: Nix.updates.length > 0
                            }
                        }

                        Ink {
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            text: Nix.checkedAt > 0 && !Nix.checking ? Nix.ageText(Nix.checkedAt, root.now) : ""
                            dim: true
                            font.pixelSize: Tokens.tinySize
                            visible: text !== ""
                        }
                    }

                    Repeater {
                        model: Nix.updates.slice(0, 5)

                        Item {
                            required property var modelData

                            width: parent.width
                            height: 26

                            Row {
                                anchors.left: parent.left
                                anchors.right: updRight.left
                                anchors.rightMargin: 8
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: 6

                                Ink {
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: modelData.name
                                    font.pixelSize: Tokens.smallSize
                                    strong: modelData.name === "nixpkgs"
                                }

                                Ink {
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: modelData.branch
                                    dim: true
                                    font.pixelSize: Tokens.tinySize
                                    visible: text !== ""
                                }
                            }

                            Row {
                                id: updRight
                                anchors.right: parent.right
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: 6

                                Ink {
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: Nix.ageText(modelData.lastModified, root.now)
                                    dim: true
                                    font.pixelSize: Tokens.tinySize
                                    visible: text !== ""
                                }

                                Press {
                                    width: 24
                                    height: 24
                                    radius: 12
                                    rest: Tokens.bg
                                    active: !root.running
                                    opacity: root.running ? 0.4 : 1
                                    onTapped: Nix.updateFlake(modelData.name)

                                    Icon {
                                        anchors.centerIn: parent
                                        name: "arrow-down"
                                        color: Tokens.accent
                                        font.pixelSize: 13
                                    }
                                }
                            }
                        }
                    }

                    Ink {
                        text: `+ ${Nix.updates.length - 5} entradas más`
                        dim: true
                        font.pixelSize: Tokens.tinySize
                        visible: Nix.updates.length > 5
                    }
                }
            }

            // Barra de acciones: siempre a un clic.
            Row {
                width: parent.width
                spacing: 6

                Action {
                    icon: "arrows-clockwise"
                    text: "Actualizar"
                    off: root.running || Nix.flake === ""
                    onTapped: Nix.updateFlake("")
                }

                Action {
                    icon: "hammer"
                    text: "Reconstruir"
                    primary: (Nix.pendingRebuild || Nix.dirty > 0 || (Nix.job?.kind === "update" && root.done)) && Nix.updates.length === 0
                    off: root.running || Nix.rebuild === ""
                    onTapped: Nix.rebuildSystem()
                }

                Action {
                    icon: "sparkle"
                    text: "Act. + Rebuild"
                    primary: Nix.updates.length > 0
                    visible: Nix.updates.length > 0 && Nix.flake !== "" && Nix.rebuild !== ""
                    off: root.running
                    onTapped: Nix.upgradeSystem()
                }
            }
        }
    }
}
