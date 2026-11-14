import QtQuick
import qs

// Punto de agente. Trabajando respira despacio; cuando te necesita da un
// único pulso y se queda fijo; en reposo es gris.
Item {
    id: root

    property string phase: "idle"  // working | needs | idle
    property real breath: 1

    implicitWidth: Tokens.dot
    implicitHeight: Tokens.dot

    onPhaseChanged: if (phase === "needs") pulse.restart()

    // Onda del pulso
    Rectangle {
        id: ring
        anchors.centerIn: parent
        width: Tokens.dot
        height: width
        radius: width / 2
        color: "transparent"
        border.width: 1
        border.color: Tokens.attention
        opacity: 0
    }

    Rectangle {
        anchors.fill: parent
        radius: width / 2
        color: root.phase === "needs" ? Tokens.attention : root.phase === "working" ? Tokens.working : Tokens.dim
        opacity: root.phase === "working" ? root.breath : 1

        Behavior on color {
            ColorAnimation { duration: Tokens.fast }
        }
    }

    SequentialAnimation on breath {
        running: root.phase === "working"
        loops: Animation.Infinite
        NumberAnimation { to: 0.35; duration: Tokens.breath / 2; easing.type: Easing.InOutSine }
        NumberAnimation { to: 1; duration: Tokens.breath / 2; easing.type: Easing.InOutSine }
    }

    ParallelAnimation {
        id: pulse
        NumberAnimation { target: ring; property: "scale"; from: 1; to: 3.2; duration: Tokens.veil * 2; easing.type: Easing.OutCubic }
        NumberAnimation { target: ring; property: "opacity"; from: 0.9; to: 0; duration: Tokens.veil * 2; easing.type: Easing.OutCubic }
    }
}
