import QtQuick
import QtQuick.Shapes
import Quickshell
import Quickshell.Hyprland
import Quickshell.Wayland
import qs
import qs.state

// El cajón: todas las notificaciones guardadas, en una isla que sale del
// centro del borde derecho. No se ve hasta que dejas el puntero en ese
// borde; se cierra con algo de gracia al salir. Las que siguen vivas
// conservan sus acciones y su respuesta; las demás solo se leen y se borran.
Scope {
    id: root

    required property ShellScreen modelData
    readonly property HyprlandMonitor monitor: Hyprland.monitorFor(modelData)
    readonly property bool calm: !Contexts.fullscreen(monitor) && !Launcher.open
    readonly property bool wanted: calm && (edge.hovered || inside.hovered || list.typist !== null)
    property bool open: false
    property double clock: Date.now()

    // Cuánto hace que llegó una.
    function ago(at: double): string {
        const minutes = Math.floor((clock - at) / 60000);
        return minutes < 1 ? "ahora" : minutes < 60 ? `hace ${minutes} min` : minutes < 1440 ? `hace ${Math.floor(minutes / 60)} h` : `hace ${Math.floor(minutes / 1440)} d`;
    }

    onWantedChanged: {
        if (wanted) {
            grace.stop();
            if (!open)
                intent.restart();
        } else {
            intent.stop();
            if (open)
                grace.restart();
        }
    }
    onCalmChanged: if (!calm) open = false
    onOpenChanged: {
        if (open) {
            clock = Date.now();
            scroller.contentY = 0;
        }
    }

    Timer {
        id: intent
        interval: 120
        onTriggered: root.open = true
    }

    Timer {
        id: grace
        interval: 260
        onTriggered: root.open = false
    }

    Timer {
        interval: 30000
        repeat: true
        running: root.open
        onTriggered: root.clock = Date.now()
    }

    // Alta como la pantalla y de tamaño fijo, como el centro de control: la
    // isla crece dentro sin que la capa cambie. Cerrada, solo recibe el
    // puntero la franja del borde.
    PanelWindow {
        screen: root.modelData
        anchors.right: true
        implicitWidth: Tokens.card + Tokens.gap * 3
        implicitHeight: root.modelData.height
        exclusionMode: ExclusionMode.Ignore
        color: "transparent"
        mask: Region {
            item: edge

            Region {
                item: island
            }
        }
        // Bajo la pantalla completa, como las islas de las esquinas.
        WlrLayershell.layer: WlrLayer.Top
        WlrLayershell.namespace: "veyl-drawer"
        // El teclado, solo mientras se escribe una respuesta.
        WlrLayershell.keyboardFocus: list.typist !== null ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None

        Item {
            id: edge

            readonly property alias hovered: hover.hovered

            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            width: Tokens.tab
            height: Tokens.ledge

            HoverHandler {
                id: hover
            }
        }

        // La isla: pegada al borde derecho, con una curva hacia fuera arriba
        // y abajo; sus dos esquinas libres son redondas. Nace del borde con
        // un rebote y se estira con lo que lleva.
        Item {
            id: island

            readonly property real progress: Math.max(0, appear.value)

            // Lo que asoma del borde derecho, para quien siga el marco.
            readonly property Rim rim: Rim {
                screen: root.modelData
                edge: "right"
                at: island.y
                extent: island.height
                depth: island.visible ? island.width : 0
                corner: plate.corner
                join: plate.join
            }

            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            width: Math.max(0, wide.value * appear.value)
            height: Math.max(0, tall.value * (0.7 + 0.3 * appear.value))
            visible: progress > 0

            HoverHandler {
                id: inside
            }

            Spring {
                id: appear
                to: root.open ? 1 : 0
                settle: Tokens.summon
                bounce: 0.2
            }

            Spring {
                id: wide
                to: holder.width
                settle: Tokens.veil
                bounce: 0.2
            }

            Spring {
                id: tall
                to: holder.height
                settle: Tokens.veil
                bounce: 0.2
            }

            Shape {
                id: plate

                readonly property real w: island.width
                readonly property real h: island.height
                // Recién nacida aún no tiene hondo para sus curvas: crecen con ella.
                readonly property real join: Math.min(Tokens.join, w / 2)
                readonly property real corner: Math.min(Tokens.round, w - join, h / 2)
                // El trazo empieza y acaba fuera de la pantalla: solo se ve el canto libre.
                readonly property real out: w + Tokens.line * 2

                preferredRendererType: Shape.CurveRenderer
                visible: w > 0

                ShapePath {
                    fillColor: Tokens.plate
                    strokeColor: Tokens.border
                    strokeWidth: Tokens.line
                    startX: plate.out
                    startY: -plate.join

                    PathLine { x: plate.out; y: plate.h + plate.join }
                    PathLine { x: plate.w; y: plate.h + plate.join }
                    PathArc { x: plate.w - plate.join; y: plate.h; radiusX: plate.join; radiusY: plate.join; direction: PathArc.Counterclockwise }
                    PathLine { x: plate.corner; y: plate.h }
                    PathArc { x: 0; y: plate.h - plate.corner; radiusX: plate.corner; radiusY: plate.corner }
                    PathLine { x: 0; y: plate.corner }
                    PathArc { x: plate.corner; y: 0; radiusX: plate.corner; radiusY: plate.corner }
                    PathLine { x: plate.w - plate.join; y: 0 }
                    PathArc { x: plate.w; y: -plate.join; radiusX: plate.join; radiusY: plate.join; direction: PathArc.Counterclockwise }
                    PathLine { x: plate.out; y: -plate.join }
                }
            }

            Item {
                anchors.fill: parent
                clip: true

                Column {
                    id: holder

                    // Pegado al borde: sale con la isla.
                    x: island.width - width
                    y: (island.height - height) / 2
                    width: Tokens.card
                    opacity: Math.min(1, Math.max(0, (island.progress - 0.35) * 2))

                    // Cuántas hay y, a la derecha, vaciarlo.
                    Item {
                        width: parent.width
                        height: 48

                        Ink {
                            x: Tokens.pad
                            anchors.verticalCenter: parent.verticalCenter
                            text: "Notificaciones"
                            strong: true
                        }

                        Ink {
                            anchors.right: clear.left
                            anchors.rightMargin: 4
                            anchors.verticalCenter: parent.verticalCenter
                            text: Notifications.notes.length
                            font.pixelSize: Tokens.smallSize
                            dim: true
                            visible: Notifications.notes.length > 0
                        }

                        Knob {
                            id: clear
                            anchors.right: parent.right
                            anchors.rightMargin: Tokens.pad - 6
                            anchors.verticalCenter: parent.verticalCenter
                            icon: "broom"
                            tint: Tokens.dim
                            visible: Notifications.notes.length > 0
                            onPressed: Notifications.clear()
                        }
                    }

                    Ink {
                        x: Tokens.pad
                        width: parent.width - Tokens.pad * 2
                        height: 56
                        text: "Nada por aquí"
                        dim: true
                        verticalAlignment: Text.AlignTop
                        visible: Notifications.notes.length === 0
                    }

                    // Todas, la más reciente arriba; si no caben, se desplazan.
                    Flickable {
                        id: scroller

                        width: parent.width
                        height: Math.min(list.height, root.modelData.height * 0.7)
                        contentHeight: list.height
                        clip: true
                        boundsBehavior: Flickable.StopAtBounds
                        visible: Notifications.notes.length > 0

                        Column {
                            id: list

                            property Item typist: null  // la tarjeta en la que se escribe

                            width: parent.width

                            Repeater {
                                model: Notifications.shelf

                                Item {
                                    id: slot

                                    required property int key
                                    required property int index

                                    width: list.width
                                    height: card.implicitHeight

                                    Notice {
                                        id: card

                                        note: Notifications.find(slot.key)
                                        stamp: root.open && note ? root.ago(note.at) : ""
                                        instant: true
                                        onTypingChanged: {
                                            if (typing)
                                                list.typist = card;
                                            else if (list.typist === card)
                                                list.typist = null;
                                        }
                                        Component.onDestruction: if (list.typist === card) list.typist = null

                                        // Se escribe en una sola a la vez.
                                        Connections {
                                            target: list

                                            function onTypistChanged(): void {
                                                if (list.typist && list.typist !== card)
                                                    card.typing = false;
                                            }
                                        }
                                    }

                                    // Entre tarjeta y tarjeta, un canto.
                                    Rectangle {
                                        x: Tokens.pad
                                        width: parent.width - Tokens.pad * 2
                                        height: Tokens.line
                                        color: Tokens.border
                                        visible: slot.index > 0
                                    }
                                }
                            }
                        }
                    }

                    Item {
                        width: parent.width
                        height: Tokens.gap
                    }
                }
            }
        }
    }
}
