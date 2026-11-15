import QtQuick
import QtQuick.Shapes
import qs
import qs.state

// Un agente, en su propia isla: más baja y ligera que las otras, pero como
// ellas sale del borde de la pantalla, maciza y del color del fondo: cuelga
// del de arriba, con una curva hacia fuera a cada lado donde se une a él.
// Pequeña, lleva el logotipo de quien lo hace, quieto y del color de su
// marca, y a su derecha cómo va: un spinner mientras trabaja, un check verde
// al acabar y un punto de aviso si te necesita. Con el puntero encima (o
// Super mantenido) se estira y enseña entre los dos en qué trabaja, en
// corto. Pulsarla te lleva a él.
Item {
    id: root

    required property string agent
    required property string phase      // working | needs | idle | gone
    required property string provider
    required property string name
    required property string window
    readonly property bool needs: phase === "needs"
    readonly property color brand: Agents.vendor(provider)
    readonly property bool open: hover.hovered || Glance.open

    // Acaba de terminar sin que lo miraras: se queda un momento a decirlo.
    property bool fresh: false
    property string was: phase
    onPhaseChanged: {
        if (was === "working" && phase === "idle" && Agents.active !== window) {
            fresh = true;
            done.restart();
        } else if (phase !== "idle") {
            fresh = false;
        }
        if (phase === "needs")
            call();
        was = phase;
    }

    Timer {
        id: done
        interval: 6000
        onTriggered: root.fresh = false
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

    implicitWidth: row.x * 2 + row.width
    implicitHeight: Tokens.pod + sway.value

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
        readonly property real corner: Math.max(0, Math.min(h / 2, w / 2))
        // El trazo empieza y acaba fuera de la pantalla: solo se ve el canto libre.
        readonly property real out: -Tokens.line * 2

        preferredRendererType: Shape.CurveRenderer

        ShapePath {
            fillColor: Tokens.plate
            strokeColor: root.needs ? Tokens.attention : Tokens.border
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
        cursorShape: root.window ? Qt.PointingHandCursor : Qt.ArrowCursor
    }

    TapHandler {
        onTapped: Agents.focus(root.agent)
    }

    Row {
        id: row
        x: Tokens.pad - 4
        anchors.verticalCenter: parent.verticalCenter

        Logo {
            anchors.verticalCenter: parent.verticalCenter
            width: Tokens.smallSize + 1
            height: width
            source: Agents.logo(root.provider)
            color: root.brand
        }

        // En qué trabaja: solo abierta. Se abre su hueco y el texto entra
        // cuando ya casi cabe.
        Item {
            anchors.verticalCenter: parent.verticalCenter
            // Plegada, solo un poco de aire entre logotipo y estado.
            width: Tokens.gap - 2 + (label.width + Tokens.gap + 2) * Math.max(0, morph.value)
            height: label.height
            clip: true

            Ink {
                id: label
                x: Tokens.gap
                text: root.name
                color: root.needs ? Tokens.attention : Tokens.text
                font.pixelSize: Tokens.smallSize
                elide: Text.ElideRight
                width: Math.min(implicitWidth, 130)
                opacity: Math.min(1, Math.max(0, (morph.value - 0.5) * 2.5))
            }
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
}
