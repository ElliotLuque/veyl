import QtQuick
import qs

// Un icono y, si lo hay, su texto.
Row {
    id: root

    property alias icon: icon.name
    property alias text: ink.text
    property bool dim: false
    property color tone: dim ? Tokens.dim : Tokens.text  // color del icono

    spacing: 8

    Icon {
        id: icon
        anchors.verticalCenter: parent.verticalCenter
        color: root.tone
    }

    Ink {
        id: ink
        anchors.verticalCenter: parent.verticalCenter
        dim: root.dim
        visible: text !== ""
    }
}
