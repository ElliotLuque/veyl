import QtQuick
import qs
import qs.base

// Un grupo de ajustes: su título, pequeño y apagado, una tarjeta con sus
// filas y, si conviene, una nota debajo.
Column {
    id: root

    property string title
    property string note
    default property alias rows: body.data

    width: parent?.width ?? 0
    spacing: 8

    Ink {
        x: 4
        text: root.title
        dim: true
        font.pixelSize: Tokens.smallSize
        visible: text !== ""
    }

    Rectangle {
        width: parent.width
        height: body.height
        radius: Tokens.round - Tokens.inset
        color: Tokens.surface
        visible: body.height > 0

        Column {
            id: body
            width: parent.width
        }
    }

    Ink {
        x: 4
        width: parent.width - 8
        text: root.note
        dim: true
        font.pixelSize: Tokens.tinySize + 1
        wrapMode: Text.Wrap
        lineHeight: 1.15
        visible: text !== ""
    }
}
