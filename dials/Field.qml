import QtQuick
import qs
import qs.base

// Un campo de texto. Lo escrito se guarda con Intro o al salir del campo;
// Esc lo deja como estaba.
Rectangle {
    id: root

    property string value
    property string placeholder
    property bool secret: false
    property alias editing: input.activeFocus
    readonly property alias typed: input.text  // lo escrito, antes de guardarlo
    signal committed(string text)

    function commit(): void {
        if (input.text !== value)
            committed(input.text);
    }

    // Lo deja vacío, aunque no se hubiera guardado nada.
    function clear(): void {
        value = "";
        input.text = "";
    }

    function edit(): void {
        input.forceActiveFocus();
    }

    onValueChanged: input.text = value

    anchors.verticalCenter: parent?.verticalCenter
    width: 320
    height: 34
    radius: Tokens.soft + 3
    color: Tokens.bg
    border.width: Tokens.line
    border.color: input.activeFocus ? Tokens.accent : Tokens.border

    Behavior on border.color {
        ColorAnimation { duration: Tokens.fast }
    }

    MouseArea {
        anchors.fill: parent
        cursorShape: Qt.IBeamCursor
        onClicked: input.forceActiveFocus()
    }

    TextInput {
        id: input
        anchors.fill: parent
        anchors.leftMargin: 12
        anchors.rightMargin: 12
        verticalAlignment: TextInput.AlignVCenter
        clip: true
        text: root.value
        echoMode: root.secret ? TextInput.Password : TextInput.Normal
        color: Tokens.text
        selectionColor: Tokens.accent
        selectedTextColor: Tokens.bg
        selectByMouse: true
        font.family: Tokens.sans
        font.pixelSize: Tokens.smallSize + 1
        Keys.onReturnPressed: {
            root.commit();
            focus = false;
        }
        Keys.onEnterPressed: {
            root.commit();
            focus = false;
        }
        Keys.onEscapePressed: {
            text = root.value;
            focus = false;
        }
        // Las flechas, para el texto: no cambian de sección mientras escribes.
        Keys.onUpPressed: {}
        Keys.onDownPressed: {}
        onActiveFocusChanged: if (!activeFocus) root.commit()

        Ink {
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width
            text: root.placeholder
            dim: true
            font.pixelSize: Tokens.smallSize + 1
            elide: Text.ElideRight
            visible: input.text === "" && !input.preeditText
        }
    }
}
