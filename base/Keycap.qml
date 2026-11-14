import QtQuick
import qs

// Las teclas de un atajo, una al lado de otra, como las escribe la entrada:
// «return», «ctrl+k», «ctrl+shift+p».
Row {
    id: root

    property string keys
    // Las que se dibujan en vez de escribirse.
    readonly property var drawn: ({ return: "arrow-elbow-down-left", shift: "arrow-fat-up", backspace: "backspace" })
    readonly property var named: ({ ctrl: "Ctrl", alt: "Alt", esc: "Esc", delete: "Supr" })

    spacing: 3

    Repeater {
        model: root.keys === "" ? [] : root.keys.split("+")

        Rectangle {
            id: cap

            required property string modelData

            width: Math.max(height, (glyph.visible ? glyph.implicitWidth : word.implicitWidth) + 10)
            height: 20
            radius: Tokens.soft - 2
            color: Tokens.surface
            border.width: Tokens.line
            border.color: Tokens.border

            Icon {
                id: glyph
                anchors.centerIn: parent
                name: root.drawn[cap.modelData] ?? ""
                font.pixelSize: 12
                visible: name !== ""
            }

            Ink {
                id: word
                anchors.centerIn: parent
                text: root.named[cap.modelData] ?? cap.modelData.toUpperCase()
                font.pixelSize: 11
                dim: true
                visible: !glyph.visible
            }
        }
    }
}
