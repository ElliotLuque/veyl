import QtQuick
import QtQuick.Effects
import qs
import qs.base
import qs.state

// Un desplegable: lo elegido y una flecha. Al pulsarlo, la lista crece desde
// su borde, por encima de todo lo demás de la ventana (no la corta la página
// que se desplaza), y hacia arriba si abajo no cabe. ↑↓ e Intro eligen; Esc o
// pulsar fuera la cierran sin cambiar nada.
// `options`: [{ value, label }].
Rectangle {
    id: root

    property var options: []
    property var value
    property string placeholder
    readonly property int index: options.findIndex(option => option.value === value)
    property bool shown: false
    property real reveal: shown ? 1 : 0
    property int lit: -1  // la fila resaltada, con el ratón o con las flechas
    property point origin  // dónde está, en la ventana, al abrirse
    property bool upward: false
    property Item restore: null  // quién tenía el teclado antes
    readonly property int line: 34
    signal picked(var value)

    Behavior on reveal {
        NumberAnimation { duration: Tokens.fast; easing.type: Easing.OutCubic }
    }

    function show(): void {
        const window = root.Window.contentItem;
        if (options.length === 0 || !window)
            return;
        origin = root.mapToItem(window, 0, 0);
        const below = window.height - origin.y - height - 18;
        const above = origin.y - 18;
        upward = below < sheet.natural && above > below;
        lit = Math.max(0, index);
        restore = root.Window.activeFocusItem;
        shown = true;
        layer.forceActiveFocus();
        reach(lit);
    }

    function hide(): void {
        if (!shown)
            return;
        shown = false;
        restore?.forceActiveFocus();
        restore = null;
    }

    function choose(row: int): void {
        const option = options[row];
        hide();
        if (option && option.value !== value)
            picked(option.value);
    }

    // Resalta otra fila y la deja a la vista.
    function reach(row: int): void {
        lit = Math.max(0, Math.min(options.length - 1, row));
        const top = lit * line;
        if (top < list.contentY)
            list.contentY = top;
        else if (top + line > list.contentY + list.height)
            list.contentY = top + line - list.height;
    }

    anchors.verticalCenter: parent?.verticalCenter
    width: 240
    height: 34
    radius: Tokens.soft + 3
    color: Tokens.bg
    border.width: Tokens.line
    border.color: shown ? Tokens.accent : hit.containsMouse ? Tokens.dim : Tokens.border
    opacity: options.length > 0 ? 1 : 0.5

    Behavior on border.color {
        ColorAnimation { duration: Tokens.fast }
    }

    Connections {
        target: Tuning

        function onOpenChanged(): void {
            if (!Tuning.open)
                root.hide();
        }
    }

    Ink {
        anchors.left: parent.left
        anchors.leftMargin: 12
        anchors.right: caret.left
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        text: root.options[root.index]?.label ?? (root.placeholder || String(root.value ?? ""))
        dim: root.index < 0
        font.pixelSize: Tokens.smallSize + 1
        elide: Text.ElideRight
    }

    Icon {
        id: caret
        anchors.right: parent.right
        anchors.rightMargin: 10
        anchors.verticalCenter: parent.verticalCenter
        name: "caret-down"
        rotation: (root.upward ? -180 : 180) * root.reveal
        font.pixelSize: Tokens.iconSize - 4
    }

    MouseArea {
        id: hit
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: root.options.length > 0 ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: root.shown ? root.hide() : root.show()
    }

    // La lista, en la ventana mientras se ve.
    FocusScope {
        id: layer

        parent: root.shown || root.reveal > 0 ? root.Window.contentItem : root
        width: parent?.width ?? 0
        height: parent?.height ?? 0
        visible: root.shown || root.reveal > 0
        z: 100

        Keys.onPressed: event => {
            event.accepted = true;
            if (event.key === Qt.Key_Escape || event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab)
                root.hide();
            else if (event.key === Qt.Key_Up)
                root.reach(root.lit - 1);
            else if (event.key === Qt.Key_Down)
                root.reach(root.lit + 1);
            else if (event.key === Qt.Key_Home || event.key === Qt.Key_PageUp)
                root.reach(0);
            else if (event.key === Qt.Key_End || event.key === Qt.Key_PageDown)
                root.reach(root.options.length - 1);
            else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space)
                root.choose(root.lit);
        }

        // Pulsar fuera la cierra; la rueda no mueve lo de detrás.
        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.AllButtons
            onPressed: root.hide()
            onWheel: wheel => wheel.accepted = true
        }

        Rectangle {
            id: sheet

            readonly property real natural: Math.min(list.contentHeight, root.line * 8) + 8
            readonly property real room: root.upward ? root.origin.y - 18 : layer.height - root.origin.y - root.height - 18
            readonly property real full: Math.min(natural, room)

            x: root.origin.x
            y: root.upward ? root.origin.y - 6 - height : root.origin.y + root.height + 6
            width: root.width
            height: full * root.reveal
            radius: Tokens.soft + 3
            color: Tokens.bg
            border.width: Tokens.line
            border.color: Tokens.border
            opacity: Math.min(1, root.reveal * 2)
            clip: true
            // Una sombra, para que se lea por encima de las filas de detrás.
            layer.enabled: true
            layer.effect: MultiEffect {
                shadowEnabled: true
                shadowColor: Tokens.shadow
                shadowBlur: 0.8
                shadowVerticalOffset: root.upward ? -4 : 4
            }

            MouseArea {
                anchors.fill: parent
                hoverEnabled: true
            }

            Flickable {
                id: list
                x: 4
                y: root.upward ? sheet.height - sheet.full + 4 : 4
                width: parent.width - 8
                height: sheet.full - 8
                contentHeight: rows.height
                boundsBehavior: Flickable.StopAtBounds
                clip: true

                Column {
                    id: rows
                    width: parent.width

                    Repeater {
                        model: root.options

                        Rectangle {
                            id: entry

                            required property var modelData
                            required property int index
                            readonly property bool chosen: index === root.index

                            width: parent.width
                            height: root.line
                            radius: Tokens.soft
                            color: root.lit === index ? Tokens.surface : "transparent"

                            Ink {
                                anchors.left: parent.left
                                anchors.leftMargin: 10
                                anchors.right: check.left
                                anchors.rightMargin: 8
                                anchors.verticalCenter: parent.verticalCenter
                                text: entry.modelData.label
                                color: entry.chosen ? Tokens.accent : Tokens.text
                                font.pixelSize: Tokens.smallSize + 1
                                elide: Text.ElideRight
                            }

                            Icon {
                                id: check
                                anchors.right: parent.right
                                anchors.rightMargin: 10
                                anchors.verticalCenter: parent.verticalCenter
                                name: "check"
                                color: Tokens.accent
                                font.pixelSize: Tokens.iconSize - 3
                                opacity: entry.chosen ? 1 : 0
                            }

                            MouseArea {
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onEntered: root.lit = entry.index
                                onClicked: root.choose(entry.index)
                            }
                        }
                    }
                }
            }
        }
    }
}
