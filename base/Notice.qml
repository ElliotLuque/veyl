import QtQuick
import qs
import qs.state

// La tarjeta de una notificación, con todo lo que puede traer: de quién es,
// su icono o su imagen, título, cuerpo (con formato, enlaces e imágenes),
// avance, respuesta en línea y acciones. El icono es tan alto como el texto
// que lo acompaña, y el texto, si es largo, se corta. Pulsarla es su acción
// por defecto o, si no tiene, descartarla; el botón derecho la descarta siempre.
Item {
    id: root

    property var note: null      // la que se enseña, como la manda veyld, o null
    property int waiting: 0      // cuántas más esperan detrás
    property string stamp: ""    // cuándo llegó, si se dice
    // Lo último que se enseñó: se conserva mientras la tarjeta se va.
    readonly property var live: Notifications.read(note)
    property var view: null
    property bool typing: false  // escribiendo una respuesta
    readonly property bool busy: hover.hovered || typing
    readonly property bool critical: view?.urgency === "critical"
    readonly property int gap: 12
    // Lo más alto que llega a ser el texto: de quién es, una línea de título y
    // dos de cuerpo. Su ancho cuenta con un icono así de alto, para que el
    // icono pueda seguir al texto sin que el texto cambie al seguirlo.
    readonly property real reserve: 18 + 4 + tallest.implicitHeight + 4 + deepest.implicitHeight
    readonly property real side: Math.max(18, words.height)  // lado del icono
    readonly property real measure: Tokens.card - Tokens.pad * 2 - reserve - gap  // ancho del texto
    // Entrada: el icono crece con un rebote y, detrás, el texto baja a su sitio.
    readonly property real pop: enter.value
    readonly property real lag: Math.max(0, (enter.value - 0.2) / 0.8)

    function send(): void {
        if (reply.text.trim() === "")
            return;
        Notifications.reply(view.id, reply.text);
        reply.clear();
        typing = false;
    }

    implicitWidth: Tokens.card
    implicitHeight: gap + words.height + (extras.visible ? extras.height + 8 : 0) + Tokens.pad

    onLiveChanged: if (live) view = live
    onNoteChanged: {
        typing = false;
        reply.clear();
    }
    // Entra con su animación en cuanto existe, salvo que ya estuviera (`instant`).
    Component.onCompleted: if (!instant) Qt.callLater(() => arrived = true)

    property bool instant: false
    property bool arrived: instant

    component Button: Rectangle {
        id: button

        property alias text: label.text
        property url icon
        signal pressed

        width: Math.max(height, (picture.visible ? picture.width : label.implicitWidth) + root.gap * 2)
        height: 28
        radius: height / 2
        color: area.containsMouse ? Tokens.border : Tokens.surface

        Behavior on color {
            ColorAnimation { duration: Tokens.fast }
        }

        Image {
            id: picture
            anchors.centerIn: parent
            width: Tokens.iconSize
            height: width
            sourceSize: Qt.size(width * 2, height * 2)
            source: button.icon
            visible: status === Image.Ready
        }

        Ink {
            id: label
            anchors.centerIn: parent
            font.pixelSize: Tokens.smallSize
            visible: !picture.visible
        }

        MouseArea {
            id: area
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: button.pressed()
        }
    }

    // Las medidas del texto más alto que cabe.
    Ink {
        id: tallest
        visible: false
        text: "X"
        strong: true
    }

    Ink {
        id: deepest
        visible: false
        text: "X<br/>X"
        textFormat: Text.StyledText
        lineHeight: 1.2
    }

    Spring {
        id: enter
        to: root.arrived ? 1 : 0
        settle: Tokens.arrive
        bounce: 0.35
    }

    HoverHandler {
        id: hover
    }

    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
        cursorShape: root.view?.primary ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: mouse => {
            if (!root.view)
                return;
            if (mouse.button === Qt.LeftButton && root.view.primary)
                Notifications.act(root.view.id, "default");
            else
                Notifications.dismiss(root.view.id);
        }
    }

    Item {
        id: content

        anchors.fill: parent

        // Su imagen; si no trae, el icono de su programa; si tampoco, uno propio.
        Item {
            id: avatar

            readonly property string kind: (root.view?.image ?? "") !== "" && photo.status !== Image.Error ? "image" : (root.view?.icon ?? "") !== "" && emblem.status !== Image.Error ? "icon" : "glyph"

            x: Tokens.pad
            y: root.gap
            width: root.side
            height: root.side
            scale: 0.4 + 0.6 * root.pop
            opacity: Math.min(1, root.pop * 2)

            Clip {
                anchors.fill: parent
                radius: Tokens.radius
                color: Tokens.surface
                visible: avatar.kind === "image"

                Image {
                    id: photo
                    anchors.fill: parent
                    source: root.view?.image ?? ""
                    sourceSize: Qt.size(width * 2, height * 2)
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                }
            }

            Image {
                id: emblem
                anchors.fill: parent
                anchors.margins: 2
                source: root.view?.icon ?? ""
                sourceSize: Qt.size(width * 2, height * 2)
                fillMode: Image.PreserveAspectFit
                asynchronous: true
                visible: avatar.kind === "icon"
            }

            Rectangle {
                anchors.fill: parent
                radius: width / 2
                color: Tokens.surface
                visible: avatar.kind === "glyph"

                Icon {
                    anchors.centerIn: parent
                    name: root.critical ? "warning" : root.view?.glyph ?? "bell"
                    font.pixelSize: Math.round(avatar.height * 0.46)
                    color: root.critical ? Tokens.critical : Tokens.text
                }
            }
        }

        Column {
            id: words

            x: avatar.x + avatar.width + root.gap
            y: root.gap + 10 * (1 - root.lag)
            opacity: Math.min(1, root.lag)
            width: root.measure
            spacing: 4

            // De quién es y, a la derecha, cuántas esperan y el aspa
            Item {
                width: root.width - Tokens.pad - words.x
                height: 18

                Row {
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 6

                    // Con imagen, el icono del programa pasa aquí.
                    Image {
                        anchors.verticalCenter: parent.verticalCenter
                        width: 14
                        height: 14
                        source: avatar.kind === "image" ? root.view?.icon ?? "" : ""
                        sourceSize: Qt.size(width * 2, height * 2)
                        visible: status === Image.Ready
                    }

                    Icon {
                        anchors.verticalCenter: parent.verticalCenter
                        name: "warning"
                        color: Tokens.critical
                        font.pixelSize: 14
                        visible: root.critical && avatar.kind !== "glyph"
                    }

                    Ink {
                        anchors.verticalCenter: parent.verticalCenter
                        width: Math.min(implicitWidth, root.measure - 80)
                        text: root.view?.app ?? ""
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                        font.pixelSize: Tokens.smallSize
                        color: root.critical ? Tokens.critical : Tokens.dim
                    }

                    Ink {
                        anchors.verticalCenter: parent.verticalCenter
                        text: `· ${root.stamp}`
                        font.pixelSize: Tokens.smallSize
                        dim: true
                        visible: root.stamp !== ""
                    }
                }

                Row {
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 6

                    Ink {
                        anchors.verticalCenter: parent.verticalCenter
                        text: `+${root.waiting}`
                        font.pixelSize: Tokens.smallSize
                        dim: true
                        visible: root.waiting > 0
                    }

                    Icon {
                        anchors.verticalCenter: parent.verticalCenter
                        name: ""  // aspa: de una sola letra, no tiene ligadura
                        color: close.containsMouse ? Tokens.text : Tokens.dim
                        font.pixelSize: 14
                        opacity: hover.hovered ? 1 : 0

                        Behavior on opacity {
                            NumberAnimation { duration: Tokens.fast }
                        }

                        MouseArea {
                            id: close
                            anchors.fill: parent
                            anchors.margins: -6
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: if (root.view) Notifications.dismiss(root.view.id)
                        }
                    }
                }
            }

            Ink {
                width: parent.width
                text: root.view?.summary ?? ""
                textFormat: Text.PlainText
                maximumLineCount: 1
                elide: Text.ElideRight
                strong: true
                dim: root.view?.urgency === "low"
                visible: text !== ""
            }

            Ink {
                width: parent.width
                text: root.view?.body ?? ""
                textFormat: Text.StyledText
                wrapMode: Text.Wrap
                maximumLineCount: 2
                elide: Text.ElideRight
                lineHeight: 1.2
                linkColor: Tokens.accent
                dim: true
                visible: text !== ""
                onLinkActivated: link => Qt.openUrlExternally(link)
            }
        }

        // Bajo el icono y el texto, a todo lo ancho: imágenes, avance,
        // respuesta y acciones.
        Column {
            id: extras

            x: Tokens.pad
            y: words.y + words.height + 8
            opacity: words.opacity
            width: root.width - Tokens.pad * 2
            spacing: 4
            visible: (root.view?.pictures.length ?? 0) > 0 || (root.view?.progress ?? -1) >= 0 || (root.view?.reply ?? false) || (root.view?.actions.length ?? 0) > 0

            Repeater {
                model: root.view?.pictures ?? []

                Item {
                    id: frame

                    required property var modelData
                    readonly property bool failed: picture.status === Image.Error || frame.modelData.source === ""

                    width: extras.width
                    height: failed ? alt.height : picture.status === Image.Ready ? Math.min(120, picture.implicitHeight * Math.min(1, width / picture.implicitWidth)) : 0
                    visible: height > 0

                    Clip {
                        width: picture.paintedWidth
                        height: parent.height
                        radius: Tokens.radius
                        color: "transparent"
                        visible: !frame.failed

                        Image {
                            id: picture
                            width: frame.width
                            height: frame.height
                            source: frame.modelData.source
                            fillMode: Image.PreserveAspectFit
                            horizontalAlignment: Image.AlignLeft
                            asynchronous: true
                        }
                    }

                    // Lo que dice de sí misma una imagen que no se puede enseñar
                    Ink {
                        id: alt
                        width: parent.width
                        text: frame.modelData.alt
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                        dim: true
                        visible: frame.failed && text !== ""
                        height: visible ? implicitHeight : 0
                    }
                }
            }

            // Avance
            Item {
                width: parent.width
                height: 12
                visible: (root.view?.progress ?? -1) >= 0

                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width
                    height: 4
                    radius: 2
                    color: Tokens.surface

                    Rectangle {
                        width: parent.width * Math.max(0, root.view?.progress ?? 0)
                        height: parent.height
                        radius: 2
                        color: root.critical ? Tokens.critical : Tokens.accent

                        Behavior on width {
                            NumberAnimation { duration: Tokens.veil; easing.type: Easing.OutCubic }
                        }
                    }
                }
            }

            // Respuesta en línea
            Item {
                width: parent.width
                height: 36
                visible: root.view?.reply ?? false

                Rectangle {
                    y: 4
                    width: parent.width
                    height: 32
                    radius: height / 2
                    color: Tokens.surface
                    border.width: Tokens.line
                    border.color: root.typing ? Tokens.accent : Tokens.border

                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.IBeamCursor
                        onClicked: {
                            root.typing = true;
                            reply.forceActiveFocus();
                        }
                    }

                    TextInput {
                        id: reply
                        anchors.verticalCenter: parent.verticalCenter
                        x: root.gap
                        width: parent.width - root.gap * 2 - plane.width - 4
                        clip: true
                        enabled: root.typing
                        color: Tokens.text
                        selectionColor: Tokens.accent
                        selectedTextColor: Tokens.bg
                        font.family: Tokens.sans
                        font.pixelSize: Tokens.fontSize
                        Keys.onReturnPressed: root.send()
                        Keys.onEnterPressed: root.send()
                        Keys.onEscapePressed: root.typing = false

                        Ink {
                            width: parent.width
                            text: root.view?.placeholder || "Responder"
                            textFormat: Text.PlainText
                            elide: Text.ElideRight
                            dim: true
                            visible: reply.text === ""
                        }
                    }

                    Icon {
                        id: plane
                        anchors.right: parent.right
                        anchors.rightMargin: root.gap - 2
                        anchors.verticalCenter: parent.verticalCenter
                        name: "paper-plane-right"
                        color: reply.text.trim() !== "" ? Tokens.accent : Tokens.dim

                        MouseArea {
                            anchors.fill: parent
                            anchors.margins: -6
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.send()
                        }
                    }
                }
            }

            // Acciones
            Item {
                width: parent.width
                height: actions.height + 4
                visible: (root.view?.actions.length ?? 0) > 0

                Flow {
                    id: actions
                    y: 4
                    width: parent.width
                    spacing: 6

                    Repeater {
                        model: root.view?.actions ?? []

                        Button {
                            required property var modelData

                            text: modelData.text
                            icon: modelData.icon
                            onPressed: Notifications.act(root.view.id, modelData.id)
                        }
                    }
                }
            }
        }
    }
}
