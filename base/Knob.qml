import QtQuick
import qs

// Botón redondo con un icono
Rectangle {
    id: knob

    property alias icon: glyph.name
    property color tint: Tokens.text
    property color rest: "transparent"
    property color lit: Tokens.surface
    readonly property alias hovered: hit.containsMouse
    signal pressed

    width: Tokens.knob
    height: Tokens.knob
    radius: width / 2
    color: hit.containsMouse ? lit : rest

    Behavior on color {
        ColorAnimation { duration: Tokens.fast }
    }

    Icon {
        id: glyph
        anchors.centerIn: parent
        color: knob.tint
        font.pixelSize: Tokens.iconSize - 2
    }

    MouseArea {
        id: hit
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: knob.pressed()
    }
}
