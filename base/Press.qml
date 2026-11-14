import QtQuick
import qs

// Lo que se pulsa en Veyl: al pasar el puntero se aclara con un velo del
// color del texto y al pulsar se aprieta, siempre con la misma calma, sea
// cual sea su fondo. Lo que lleva va encima; un botón suyo (un aspa) se
// queda su pulsación, y el puntero sigue contando como encima.
Item {
    id: root

    property real radius: 0
    property color rest: "transparent"  // su fondo
    property bool active: true
    readonly property alias hovered: hover.hovered
    readonly property alias down: tap.pressed
    default property alias content: holder.data
    signal tapped
    signal context  // botón derecho

    Rectangle {
        anchors.fill: parent
        radius: root.radius
        color: root.rest

        Behavior on color {
            ColorAnimation { duration: Tokens.fast }
        }
    }

    Rectangle {
        anchors.fill: parent
        radius: root.radius
        color: Tokens.text
        opacity: !root.active ? 0 : root.down ? 0.12 : root.hovered ? 0.06 : 0

        Behavior on opacity {
            NumberAnimation { duration: Tokens.fast; easing.type: Easing.OutCubic }
        }
    }

    Item {
        id: holder
        anchors.fill: parent
    }

    HoverHandler {
        id: hover
        enabled: root.active
        cursorShape: Qt.PointingHandCursor
    }

    TapHandler {
        id: tap
        enabled: root.active
        gesturePolicy: TapHandler.ReleaseWithinBounds
        onTapped: root.tapped()
    }

    TapHandler {
        enabled: root.active
        acceptedButtons: Qt.RightButton
        onTapped: root.context()
    }
}
