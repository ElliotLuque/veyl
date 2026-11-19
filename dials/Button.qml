import QtQuick
import qs
import qs.base

// Una píldora que se pulsa: un icono y su texto. `strong`, sobre el acento
// (lo que se recomienda); `tone`, del color de un rol (aviso, crítico) que la tiñe.
Rectangle {
    id: root

    property string icon
    property string text
    property bool strong: false
    property color tone: "transparent"
    readonly property bool tinted: tone.a > 0
    signal pressed

    anchors.verticalCenter: parent?.verticalCenter
    width: text ? row.width + 26 : height
    height: 32
    radius: height / 2
    color: strong ? Tokens.accent : tinted ? tone : hit.containsMouse ? Tokens.border : Tokens.bg
    opacity: strong || tinted ? (hit.containsMouse ? 0.85 : 1) : 1

    Behavior on color {
        ColorAnimation { duration: Tokens.fast }
    }

    Row {
        id: row
        anchors.centerIn: parent
        spacing: 6

        Icon {
            anchors.verticalCenter: parent.verticalCenter
            name: root.icon
            color: root.strong || root.tinted ? Tokens.bg : Tokens.dim
            font.pixelSize: Tokens.iconSize - 2
            visible: root.icon !== ""
        }

        Ink {
            anchors.verticalCenter: parent.verticalCenter
            text: root.text
            color: root.strong || root.tinted ? Tokens.bg : Tokens.text
            font.pixelSize: Tokens.smallSize
            visible: root.text !== ""
        }
    }

    MouseArea {
        id: hit
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: root.pressed()
    }
}
