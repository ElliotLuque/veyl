import QtQuick
import Quickshell
import qs
import qs.base

// Un icono en su círculo del color de la pieza.
Rectangle {
    property string icon: ""
    property color tone: Tokens.accent
    property int size: 30

    function soft(c: color, alpha: real): color {
        return Qt.rgba(c.r, c.g, c.b, alpha);
    }

    width: size
    height: size
    radius: size / 2
    color: soft(tone, 0.18)

    Icon {
        anchors.centerIn: parent
        name: parent.icon
        color: parent.tone
        font.pixelSize: Math.round(parent.size * 0.55)
    }
}
