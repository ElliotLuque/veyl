import QtQuick
import qs
import qs.base

// Una barra que se arrastra o se pulsa, a saltos de `step`; la rueda no la
// mueve, que desplaza la página. Mientras se arrastra avisa con `moved`; al
// soltar, con `picked`, que es cuando se guarda lo que haya que guardar. A la
// derecha, `shown`.
Item {
    id: root

    property real from: 0
    property real to: 1
    property real step: 0
    property real value: 0
    property string shown: ""
    readonly property bool dragging: drag.pressed
    property real held: 0
    readonly property real current: dragging ? held : value
    readonly property real fraction: to > from ? (Math.max(from, Math.min(to, current)) - from) / (to - from) : 0
    signal moved(real value)
    signal picked(real value)

    function at(x: real): real {
        let value = from + Math.max(0, Math.min(1, x / track.width)) * (to - from);
        if (step > 0)
            value = from + Math.round((value - from) / step) * step;
        return Math.max(from, Math.min(to, value));
    }

    anchors.verticalCenter: parent?.verticalCenter
    width: 300
    height: 28

    Item {
        id: track
        anchors.left: parent.left
        anchors.right: amount.left
        anchors.rightMargin: 12
        anchors.verticalCenter: parent.verticalCenter
        height: 6

        Rectangle {
            anchors.fill: parent
            radius: height / 2
            color: Tokens.border
        }

        Rectangle {
            width: Math.max(height, parent.width * root.fraction)
            height: parent.height
            radius: height / 2
            color: Tokens.accent

            Behavior on width {
                enabled: !root.dragging
                NumberAnimation { duration: Tokens.fast; easing.type: Easing.OutCubic }
            }
        }

        Rectangle {
            x: parent.width * root.fraction - width / 2
            anchors.verticalCenter: parent.verticalCenter
            width: drag.containsMouse || drag.pressed ? 16 : 12
            height: width
            radius: width / 2
            color: Tokens.text

            Behavior on width {
                NumberAnimation { duration: Tokens.fast }
            }
        }

        MouseArea {
            id: drag
            anchors.fill: parent
            anchors.margins: -10
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onPressed: event => {
                root.held = root.at(event.x - 10);
                root.moved(root.held);
            }
            onPositionChanged: event => {
                if (!pressed)
                    return;
                const value = root.at(event.x - 10);
                if (value !== root.held) {
                    root.held = value;
                    root.moved(value);
                }
            }
            onReleased: root.picked(root.held)
        }
    }

    Ink {
        id: amount
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        width: 56
        horizontalAlignment: Text.AlignRight
        text: root.shown
        dim: true
        font.pixelSize: Tokens.smallSize
    }
}
