import QtQuick
import Quickshell
import Quickshell.Hyprland
import Quickshell.Wayland
import qs
import qs.state

// El velo en un monitor: lo que nace del centro del borde de arriba. Las
// notificaciones que acaban de llegar, lo que el cerebro está pensando y, si
// subes el puntero a ese borde, el centro de control; en el monitor con foco. Desde el centro de
// control se llega a los ajustes (dials/), que crecen de esa misma isla. Si
// el centro lo ocupa ya algo (la entrada, los ajustes, el centro de control),
// lo que llega sale a su derecha, también del borde de arriba.
Scope {
    id: root

    required property ShellScreen modelData
    readonly property HyprlandMonitor monitor: Hyprland.monitorFor(modelData)
    readonly property bool focused: monitor?.focused ?? false

    // El centro de control se abre con el puntero en el borde de arriba,
    // tras un instante para que pasar por ahí no lo abra, y se cierra con algo
    // de gracia al salir. Pedido desde fuera (Center.pinned), sale en el
    // monitor con foco y se queda hasta que el puntero entra y vuelve a salir.
    // Tras pulsar algo dentro espera un poco más: si la isla encoge (se borra
    // una fila, se cambia de página), el puntero puede quedar fuera sin
    // haberse ido.
    readonly property bool calm: !Launcher.open && !Tuning.open && !Recall.open && !Contexts.fullscreen(monitor)
    readonly property bool pinned: Center.pinned && focused
    property bool controls: false
    property double closed: 0
    readonly property bool wanted: calm && (edge.hovered || center.hovered || panel.busy || pinned || linger.running)
    onPinnedChanged: if (pinned) controls = calm
    onWantedChanged: {
        if (wanted) {
            grace.stop();
            if (!controls)
                intent.restart();
        } else {
            intent.stop();
            if (controls)
                grace.restart();
        }
    }
    onCalmChanged: if (!calm) controls = false
    onControlsChanged: {
        if (controls) {
            // Tras un rato cerrado vuelve a lo de siempre.
            if (!pinned && Date.now() - closed > 20000)
                Center.page = "home";
            panel.reset();
        } else {
            closed = Date.now();
            if (focused)
                Center.pinned = false;
        }
    }

    // Lo pedido desde fuera se suelta en cuanto el puntero ha entrado y se va.
    Connections {
        target: inside

        function onHoveredChanged(): void {
            if (!inside.hovered && root.pinned)
                Center.pinned = false;
        }
    }

    Timer {
        id: intent
        interval: 140
        onTriggered: root.controls = true
    }

    Timer {
        id: grace
        interval: 380
        onTriggered: root.controls = false
    }

    Timer {
        id: linger
        interval: 900
    }

    // Lo que ocupa ahora el centro del borde de arriba en este monitor: su
    // ancho, o 0 si está libre.
    readonly property real occupied: Tuning.open && Tuning.monitor === modelData.name ? Tuning.span(modelData.width) : Recall.open && Recall.monitor === modelData.name ? Math.min(Tokens.dials, modelData.width - (Tokens.join + Tokens.gap) * 2 - 80) : Launcher.open && Launcher.monitor === modelData.name ? Launcher.span : controls ? Tokens.controls : 0
    // Dónde va el centro de lo que llega: en el centro si está libre; si no, a
    // la derecha de lo que lo ocupa, con su hueco, sin salirse de la pantalla.
    readonly property real beside: occupied > 0 ? Math.min(modelData.width / 2 + occupied / 2 + Tokens.join * 2 + Tokens.gap + Tokens.card / 2, modelData.width - Tokens.card / 2 - Tokens.join - Tokens.gap) : modelData.width / 2

    // Lo que llega espera mientras lo atiendes.
    Binding {
        target: Notifications
        property: "held"
        value: notices.busy
        when: root.focused
    }

    Spring {
        id: slide
        to: root.beside
        settle: Tokens.veil
        bounce: 0.2
    }

    // La entrada y los ajustes son capas a pantalla completa (oscurecen lo de
    // detrás y pulsar fuera los cierra), y de las capas de arriba manda la
    // última que sale: si ya había algo a la vista, se vuelve a sacar para que
    // quede encima y no lo tapen.
    property bool lift: false

    function raise(open: bool): void {
        if (!open || notch.progress <= 0)
            return;
        lift = true;
        Qt.callLater(() => lift = false);
    }

    Connections {
        target: Launcher

        function onOpenChanged(): void {
            root.raise(Launcher.open);
        }
    }

    Connections {
        target: Tuning

        function onOpenChanged(): void {
            root.raise(Tuning.open);
        }
    }

    Connections {
        target: Recall

        function onOpenChanged(): void {
            root.raise(Recall.open);
        }
    }

    PanelWindow {
        screen: root.modelData
        visible: notch.progress > 0 && !root.lift
        // De lado a lado: la isla va donde haya sitio.
        anchors {
            top: true
            left: true
            right: true
        }
        // Sitio para un rebote.
        implicitHeight: notices.implicitHeight + Tokens.gap * 2
        // Pegada al borde de la pantalla, no bajo el hueco reservado.
        exclusionMode: ExclusionMode.Ignore
        color: "transparent"
        // Solo la isla recibe el puntero; lo demás lo deja pasar.
        mask: Region {
            item: notch
        }
        // Por encima de todo: una notificación también llega en pantalla completa.
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "veyl-veil"
        // El teclado, solo mientras se escribe una respuesta.
        WlrLayershell.keyboardFocus: notices.typing ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None

        Notch {
            id: notch
            x: slide.value - width / 2
            shown: root.focused && Notifications.loud

            Notices {
                id: notices
            }
        }
    }

    // Mientras el cerebro piensa la respuesta, una isla pequeña que cuenta lo
    // que va haciendo (y se puede parar); luego, en cuanto la compone, la
    // tarjeta con ella, que se va sola cuando el modelo ha acabado (salvo con
    // el puntero encima). Ceden ante lo que llega, la entrada y los ajustes.
    PanelWindow {
        screen: root.modelData
        visible: thought.progress > 0
        anchors.top: true
        // De tamaño fijo, como la pantalla: si la capa siguiera a la isla,
        // Hyprland enseñaría un fotograma estirado en cada cambio y la isla
        // daría saltos. El puntero solo lo recibe la isla.
        implicitWidth: root.modelData.width
        implicitHeight: root.modelData.height
        exclusionMode: ExclusionMode.Ignore
        color: "transparent"
        mask: Region {
            item: thought
        }
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "veyl-thinking"
        // El teclado, al pulsar el campo de la tarjeta para seguir preguntando.
        WlrLayershell.keyboardFocus: answer.typing ? WlrKeyboardFocus.Exclusive : Daemon.card ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None

        Notch {
            id: thought
            anchors.horizontalCenter: parent.horizontalCenter
            shown: root.focused && (Daemon.thinking !== null || Daemon.card !== null) && !Notifications.loud && !Launcher.open && !Tuning.open && !Recall.open && !root.controls

            HoverHandler {
                id: over
            }

            // La isla mide todo lo que lleva, también lo oculto: lo que no se
            // ve no ocupa, para que lo que se ve quede centrado.
            Thinking {
                id: musing
                width: visible ? implicitWidth : 0
                height: visible ? implicitHeight : 0
                running: thought.progress > 0 && Daemon.card === null
                visible: Daemon.card === null
            }

            Card {
                id: answer
                width: visible ? implicitWidth : 0
                height: visible ? implicitHeight : 0
                card: Daemon.card
                // Ni más ancha ni más alta que la pantalla: si no cabe, se desplaza.
                room: root.modelData.width - (Tokens.join + Tokens.gap) * 2 - 80
                limit: root.modelData.height - Tokens.gap * 2 - 60
                visible: Daemon.card !== null
                onDone: Daemon.close()
            }
        }

        // La tarjeta se va sola, cuando el modelo ha acabado: más rato cuanto más lleva.
        Timer {
            running: Daemon.card !== null && !Daemon.card.live && !Daemon.card.demo && !over.hovered && !answer.typing
            interval: 20000 + 4000 * (Daemon.card?.bloques?.length ?? 0)
            onTriggered: Daemon.close()
        }
    }

    // El centro de control, en su propia capa: alta como la pantalla y de
    // tamaño fijo, para que la isla crezca dentro sin que la capa cambie de
    // tamaño. Cerrada, solo recibe el puntero una franja fina del borde.
    PanelWindow {
        screen: root.modelData
        visible: root.focused
        anchors.top: true
        implicitWidth: Tokens.controls + (Tokens.join + Tokens.gap) * 2
        implicitHeight: root.modelData.height
        exclusionMode: ExclusionMode.Ignore
        color: "transparent"
        mask: Region {
            item: edge

            Region {
                item: center
            }
        }
        // Bajo la pantalla completa, como las islas de las esquinas.
        WlrLayershell.layer: WlrLayer.Top
        WlrLayershell.namespace: "veyl-controls"

        Item {
            id: edge

            readonly property alias hovered: hover.hovered

            anchors.horizontalCenter: parent.horizontalCenter
            width: Tokens.card
            height: Tokens.tab

            HoverHandler {
                id: hover
            }
        }

        Notch {
            id: center

            readonly property alias hovered: inside.hovered

            // Al capturar desde aquí, se aparta de golpe y no vuelve hasta la próxima.
            property bool tucked: false

            anchors.horizontalCenter: parent.horizontalCenter
            opacity: tucked ? 0 : 1
            shown: root.controls
            onShownChanged: if (shown) tucked = false
            settle: Tokens.summon
            stretch: Tokens.veil
            bounce: 0.2

            HoverHandler {
                id: inside
            }

            Connections {
                target: Shot

                function onHidingChanged(): void {
                    if (Shot.hiding)
                        center.tucked = true;
                }
            }

            Controls {
                id: panel
                onDone: root.controls = false
                onTouched: linger.restart()
            }
        }
    }
}
