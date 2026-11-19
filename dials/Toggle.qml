import QtQuick
import qs

// Un interruptor: encendido, del acento, que es lo tuyo.
Rectangle {
    id: root

    property bool on: false
    signal toggled(bool on)

    anchors.verticalCenter: parent?.verticalCenter
    width: 42
    height: 24
    radius: height / 2
    color: on ? Tokens.accent : Tokens.border

    Behavior on color {
        ColorAnimation { duration: Tokens.fast }
    }

    Rectangle {
        y: 3
        x: root.on ? root.width - width - 3 : 3
        width: root.height - 6
        height: width
        radius: width / 2
        color: root.on ? Tokens.bg : Tokens.dim

        Behavior on x {
            NumberAnimation { duration: Tokens.fast; easing.type: Easing.OutCubic }
        }
    }

    MouseArea {
        anchors.fill: parent
        anchors.margins: -6
        cursorShape: Qt.PointingHandCursor
        onClicked: root.toggled(!root.on)
    }
}
