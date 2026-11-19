import QtQuick
import Quickshell
import Quickshell.Wayland
import qs
import qs.base
import qs.state

// Los ajustes, en el monitor desde el que se llamaron: la isla más grande,
// que nace del centro del borde de arriba como el centro de control y, si
// vienen de él, es esa misma isla la que crece. Mientras están abiertos se
// quedan con el teclado; pulsar fuera o Esc los cierra.
PanelWindow {
    id: root

    screen: Quickshell.screens.find(screen => screen.name === Tuning.monitor) ?? null
    visible: Tuning.open || notch.progress > 0
    anchors {
        top: true
        bottom: true
        left: true
        right: true
    }
    exclusionMode: ExclusionMode.Ignore
    color: "transparent"
    // Abiertos, toda la pantalla es suya; al irse, ya solo la isla.
    mask: Region {
        item: Tuning.open ? outside : notch
    }
    // Por encima de todo, también de la pantalla completa: los has llamado tú.
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "veyl-dials"
    WlrLayershell.keyboardFocus: Tuning.open ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None

    Connections {
        target: Tuning

        function onOpenChanged(): void {
            if (!Tuning.open)
                return;
            if (Tuning.origin.width > 0)
                notch.from(Tuning.origin);
            panel.forceActiveFocus();
        }
    }

    MouseArea {
        id: outside
        anchors.fill: parent
        acceptedButtons: Qt.AllButtons
        onPressed: Tuning.hide()
    }

    // Lo de detrás, algo oscurecido.
    Rectangle {
        anchors.fill: parent
        color: Tokens.scrim
        opacity: Math.min(1, notch.progress)
    }

    Notch {
        id: notch

        anchors.horizontalCenter: parent.horizontalCenter
        shown: Tuning.open
        settle: Tokens.summon
        stretch: Tokens.veil
        bounce: 0.2

        Panel {
            id: panel
            wide: Tuning.span(root.width)
            tall: Math.min(Tokens.dialsTall, root.height - Tokens.gapTop * 2)
            live: Tuning.open || notch.progress > 0
            focus: true
        }
    }
}
