import QtQuick
import Quickshell
import qs
import qs.base

Rectangle {
    id: pill

    property alias text: label.text
    property string icon: ""
    // Los botones, siempre del acento del tema: el color de la tarjeta es
    // para iconos y resaltes, no para cambiar la estética.
    property color tone: Tokens.accent
    property bool solid: false
    signal pressed

    function soft(c: color, alpha: real): color {
        return Qt.rgba(c.r, c.g, c.b, alpha);
    }

    width: row.implicitWidth + 26
    height: 32
    radius: height / 2
    color: solid ? (area.containsMouse ? Qt.lighter(tone, 1.15) : tone) : (area.containsMouse ? soft(tone, 0.28) : soft(tone, 0.16))

    Behavior on color {
        ColorAnimation { duration: Tokens.fast }
    }

    Row {
        id: row
        anchors.centerIn: parent
        spacing: 6

        Icon {
            anchors.verticalCenter: parent.verticalCenter
            name: pill.icon
            color: pill.solid ? Tokens.bg : pill.tone
            visible: pill.icon !== ""
        }

        Ink {
            id: label
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(implicitWidth, 300)
            elide: Text.ElideRight
            font.pixelSize: Tokens.smallSize
            strong: true
            color: pill.solid ? Tokens.bg : Tokens.text
        }
    }

    MouseArea {
        id: area
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: pill.pressed()
    }
}
