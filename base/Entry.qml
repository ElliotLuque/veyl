import QtQuick
import qs
import qs.state

// Una fila de la entrada: su icono, su nombre y, a la derecha, lo que haya
// que saber de ella de un vistazo: cómo está, su atajo, si es la actual.
Item {
    id: root

    property var item              // la fila (ver state/Launcher.qml), o null
    property bool selected: false
    // Lo que cambia con la fila a la vista se pregunta aquí: así sigue vivo.
    readonly property string status: item?.state ? item.state() : ""
    readonly property bool current: item?.current ? item.current() : false
    readonly property string picture: !item ? "" : item.icon ? Apps.picture(item.icon) : item.emblem ? Apps.emblem(item.emblem) : ""
    readonly property var swatches: item?.swatches ?? null
    readonly property real room: side.x - title.x - 16  // sitio para el nombre

    // El icono de su aplicación; si no tiene, lo copiado en pequeño, sus
    // colores, el emoji o símbolo que es o, a falta de todo, un icono propio.
    Item {
        id: emblem

        readonly property string kind: (root.item?.thumb ?? "") !== "" ? "thumb" : root.swatches ? "swatch" : (root.item?.character ?? "") !== "" ? "character" : root.picture !== "" && image.status !== Image.Error ? "image" : "glyph"

        x: Tokens.gap + 10
        anchors.verticalCenter: parent.verticalCenter
        width: Tokens.emblem
        height: Tokens.emblem

        Image {
            id: image
            anchors.fill: parent
            source: root.picture
            sourceSize: Qt.size(width * 2, height * 2)
            fillMode: Image.PreserveAspectFit
            visible: emblem.kind === "image"
        }

        Clip {
            anchors.fill: parent
            radius: Tokens.soft
            color: root.selected ? Tokens.border : Tokens.surface
            visible: emblem.kind === "thumb"

            Image {
                anchors.fill: parent
                source: root.item?.thumb ?? ""
                sourceSize: Qt.size(width * 2, height * 2)
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
            }
        }

        // Un color, o un esquema entero: su fondo, su canto y su acento.
        Rectangle {
            anchors.fill: parent
            radius: Tokens.soft
            color: root.swatches?.[0] ?? "transparent"
            border.width: Tokens.line
            border.color: root.swatches?.[2] ?? Tokens.border
            visible: emblem.kind === "swatch"

            Rectangle {
                anchors.centerIn: parent
                width: 10
                height: 10
                radius: width / 2
                color: root.swatches?.[3] ?? "transparent"
                visible: (root.swatches?.length ?? 0) > 3
            }
        }

        Text {
            anchors.centerIn: parent
            text: emblem.kind === "character" ? root.item.character : ""
            textFormat: Text.PlainText
            color: Tokens.text
            font.family: Tokens.sans
            font.pixelSize: 22
            visible: emblem.kind === "character"
        }

        Rectangle {
            anchors.fill: parent
            radius: Tokens.soft
            color: root.selected ? Tokens.border : Tokens.surface
            visible: emblem.kind === "glyph"

            Icon {
                anchors.centerIn: parent
                name: root.item?.glyph ?? ""
                color: Tokens.text
                font.pixelSize: 16
            }
        }
    }

    Ink {
        id: title
        x: emblem.x + emblem.width + 12
        anchors.verticalCenter: parent.verticalCenter
        width: Math.min(implicitWidth, root.room)
        text: root.item?.title ?? ""
        textFormat: Text.PlainText
        elide: Text.ElideRight
        maximumLineCount: 1
    }

    Ink {
        x: title.x + title.width + 10
        anchors.verticalCenter: parent.verticalCenter
        width: root.room - title.width - 10
        text: root.item?.subtitle ?? ""
        textFormat: Text.PlainText
        elide: Text.ElideRight
        maximumLineCount: 1
        dim: true
        visible: text !== "" && width > 48
    }

    Row {
        id: side

        x: parent.width - width - Tokens.gap - 12
        anchors.verticalCenter: parent.verticalCenter
        spacing: 10

        Icon {
            anchors.verticalCenter: parent.verticalCenter
            name: "push-pin"
            color: Tokens.accent
            font.pixelSize: 14
            visible: root.item?.entry?.pinned ?? false
        }

        // Cómo está; si no hay nada que decir, qué es.
        Ink {
            anchors.verticalCenter: parent.verticalCenter
            width: Math.min(implicitWidth, 220)
            text: root.status !== "" ? root.status : root.item?.label ?? ""
            textFormat: Text.PlainText
            elide: Text.ElideRight
            font.pixelSize: Tokens.smallSize
            dim: true
            visible: text !== ""
        }

        Keycap {
            anchors.verticalCenter: parent.verticalCenter
            keys: root.item?.keys ?? ""
            visible: keys !== ""
        }

        Icon {
            anchors.verticalCenter: parent.verticalCenter
            name: "check"
            color: Tokens.accent
            font.pixelSize: 16
            visible: root.current
        }
    }
}
