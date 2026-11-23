import QtQuick
import Quickshell
import Quickshell.Wayland
import qs
import qs.state

// La entrada, en el monitor desde el que se llamó: nace del centro del borde
// de arriba, como lo que llega al velo, pero ya, que la has llamado tú.
// Mientras está abierta se queda con el teclado, y pulsar fuera la cierra.
PanelWindow {
    id: root

    screen: Quickshell.screens.find(screen => screen.name === Launcher.monitor) ?? null
    visible: Launcher.open || notch.progress > 0
    anchors {
        top: true
        bottom: true
        left: true
        right: true
    }
    exclusionMode: ExclusionMode.Ignore
    color: "transparent"
    // Abierta, toda la pantalla es suya; al irse, ya solo la isla.
    mask: Region {
        item: Launcher.open ? outside : notch
    }
    // Por encima de todo, también de la pantalla completa.
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "veyl-launcher"
    WlrLayershell.keyboardFocus: Launcher.open ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None

    // Al capturar desde aquí, se aparta de golpe y no vuelve hasta la próxima.
    property bool tucked: false
    contentItem.opacity: tucked ? 0 : 1

    Connections {
        target: Shot

        function onHidingChanged(): void {
            if (Shot.hiding)
                root.tucked = true;
        }
    }

    Connections {
        target: Launcher

        function onOpenChanged(): void {
            if (Launcher.open)
                root.tucked = false;
        }
    }

    MouseArea {
        id: outside
        anchors.fill: parent
        acceptedButtons: Qt.AllButtons
        onPressed: Launcher.hide()
    }

    // Lo de detrás, algo oscurecido: ahora mismo no es con ello.
    Rectangle {
        anchors.fill: parent
        color: Tokens.scrim
        opacity: Math.min(1, notch.progress)
    }

    Notch {
        id: notch

        anchors.horizontalCenter: parent.horizontalCenter
        shown: Launcher.open
        settle: Tokens.summon
        stretch: Tokens.summon
        bounce: 0.2

        Prompt {
            revealed: notch.height
        }
    }
}
