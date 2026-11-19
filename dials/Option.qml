import QtQuick
import qs
import qs.base

// Una fila de ajustes: su icono, qué es y, debajo, lo que conviene saber; a
// la derecha, lo que la cambia. En una lista para elegir (`pressable`), la
// elegida (`current`) lleva el icono del acento y una marca.
Item {
    id: root

    property string title
    property string text
    property string glyph
    property color tone: Tokens.dim
    property bool current: false
    property bool pressable: false
    readonly property alias hovered: hit.containsMouse
    default property alias control: aside.data
    signal clicked

    width: parent?.width ?? 0
    height: Math.max(Tokens.option, words.height + 22)

    // Entre una fila y la de encima, una línea fina.
    Rectangle {
        x: Tokens.pad
        width: parent.width - Tokens.pad * 2
        height: Tokens.line
        color: Tokens.border
        visible: root.y > 0
    }

    Rectangle {
        anchors.fill: parent
        anchors.margins: 4
        radius: Tokens.round - Tokens.inset - 4
        color: Tokens.border
        opacity: root.pressable && hit.containsMouse ? 0.6 : 0

        Behavior on opacity {
            NumberAnimation { duration: Tokens.fast }
        }
    }

    MouseArea {
        id: hit
        anchors.fill: parent
        enabled: root.pressable
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: root.clicked()
    }

    Icon {
        id: icon
        x: Tokens.pad
        anchors.verticalCenter: parent.verticalCenter
        name: root.glyph
        color: root.current ? Tokens.accent : root.tone
        visible: root.glyph !== ""
    }

    Column {
        id: words
        anchors.left: root.glyph !== "" ? icon.right : parent.left
        anchors.leftMargin: root.glyph !== "" ? 14 : Tokens.pad
        anchors.right: mark.visible ? mark.left : aside.left
        anchors.rightMargin: 16
        anchors.verticalCenter: parent.verticalCenter
        spacing: 2

        Ink {
            width: parent.width
            text: root.title
            elide: Text.ElideRight
        }

        Ink {
            width: parent.width
            text: root.text
            dim: true
            font.pixelSize: Tokens.smallSize
            wrapMode: Text.Wrap
            maximumLineCount: 3
            elide: Text.ElideRight
            visible: text !== ""
        }
    }

    Icon {
        id: mark
        anchors.right: aside.left
        anchors.rightMargin: aside.width > 0 ? 12 : 0
        anchors.verticalCenter: parent.verticalCenter
        name: "check"
        color: Tokens.accent
        visible: root.current
    }

    Row {
        id: aside
        anchors.right: parent.right
        anchors.rightMargin: Tokens.pad
        anchors.verticalCenter: parent.verticalCenter
        spacing: 8
    }
}
