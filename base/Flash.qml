import QtQuick
import Quickshell
import Quickshell.Wayland
import qs.state

// Al capturar la pantalla entera, un destello blanco y breve sobre ella,
// como el obturador de un móvil: sube casi de golpe, poco, y se apaga
// despacio. Solo existe mientras dura y no recibe el puntero.
PanelWindow {
    id: root

    required property ShellScreen modelData

    screen: modelData
    visible: glow.opacity > 0
    anchors {
        top: true
        bottom: true
        left: true
        right: true
    }
    exclusionMode: ExclusionMode.Ignore
    color: "transparent"
    mask: Region {}
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "veyl-flash"

    Connections {
        target: Shot

        function onFlashed(monitor: string): void {
            if (monitor === "" || monitor === root.modelData.name)
                blink.restart();
        }
    }

    Rectangle {
        id: glow

        anchors.fill: parent
        color: "white"
        opacity: 0
    }

    SequentialAnimation {
        id: blink

        NumberAnimation {
            target: glow
            property: "opacity"
            to: 0.32
            duration: 50
            easing.type: Easing.OutQuad
        }
        NumberAnimation {
            target: glow
            property: "opacity"
            to: 0
            duration: 380
            easing.type: Easing.OutCubic
        }
    }
}
