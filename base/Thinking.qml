import QtQuick
import qs
import qs.state

// Lo que hay en el velo mientras el cerebro piensa la respuesta a algo que
// la gramática no entendía. En reposo, una línea: tres puntos del acento que
// respiran en ola, «Pensando» y lo preguntado, en gris. En cuanto el modelo
// hace algo (busca en la web, lee una página, mira el diario, mide el
// ordenador), la isla se abre: arriba lo que hace ahora, que entra desde
// abajo al cambiar; debajo, lo preguntado; y, si encuentra páginas, sus
// sitios. A la derecha, cuánto lleva; con el puntero encima, un aspa para
// pararlo. La respuesta llega luego como una tarjeta, en esta misma isla.
Item {
    id: root

    property bool running: false
    readonly property var thought: Daemon.thinking
    readonly property string asked: thought?.said ?? ""
    readonly property var steps: thought?.steps ?? []
    readonly property var step: steps.length > 0 ? steps[steps.length - 1] : null
    readonly property bool busy: step !== null
    readonly property var sources: thought?.sources ?? []
    readonly property bool stoppable: !!thought?.stop

    // Cuánto lleva pensando, desde que se pregunta.
    property real now: Date.now()
    readonly property int elapsed: thought ? Math.max(0, Math.floor((now - thought.since) / 1000)) : 0
    readonly property string took: elapsed < 60 ? `${elapsed} s` : `${Math.floor(elapsed / 60)}:${elapsed % 60 < 10 ? "0" : ""}${elapsed % 60}`

    // Lo que no es texto (márgenes, lo de los lados) y lo que pide el texto
    // más largo, sin pasar de un ancho: lo que no cabe se corta.
    readonly property real widest: Tokens.answer - 100
    readonly property real fixed: Tokens.pad * 2 + lead.width + 10 + (side.width > 0 ? side.width + 12 : 0)
    readonly property real wanted: Math.max(first.implicitWidth + (beside.visible ? beside.implicitWidth + 8 : 0), second.visible ? second.implicitWidth : 0, third.visible ? third.implicitWidth + 20 : 0)
    readonly property real room: Math.max(60, Math.min(widest - fixed, wanted))

    implicitWidth: fixed + room
    implicitHeight: busy ? words.implicitHeight + 22 : Tokens.island

    // Cuatro veces por segundo: a uno por segundo, el temporizador (que no es
    // exacto) cae justo en la frontera del segundo y el número se queda dos
    // veces en el mismo o se salta uno.
    Timer {
        running: root.visible && root.thought !== null
        repeat: true
        triggeredOnStart: true
        interval: 250
        onTriggered: root.now = Date.now()
    }

    HoverHandler {
        id: hover
    }

    // Tres puntos del acento que respiran en ola; solo mientras se ve.
    Row {
        id: lead
        x: Tokens.pad
        y: words.y + (first.height - height) / 2
        spacing: 4

        Repeater {
            model: 3

            Rectangle {
                id: dot

                required property int index

                width: 6
                height: 6
                radius: 3
                color: Tokens.accent
                opacity: 0.25

                SequentialAnimation on opacity {
                    running: root.running
                    loops: Animation.Infinite
                    PauseAnimation { duration: dot.index * 160 }
                    NumberAnimation { to: 1; duration: 320; easing.type: Easing.OutSine }
                    NumberAnimation { to: 0.25; duration: 480; easing.type: Easing.InSine }
                    PauseAnimation { duration: (2 - dot.index) * 160 }
                }
            }
        }
    }

    Column {
        id: words
        x: lead.x + lead.width + 10
        y: root.busy ? 11 : (Tokens.island - first.height) / 2
        spacing: 3

        // Al abrirse o cerrarse, la línea de arriba se desliza a su sitio
        // con la isla, sin saltar.
        Behavior on y {
            NumberAnimation { duration: Tokens.veil; easing.type: Easing.OutCubic }
        }

        Row {
            spacing: 8

            // Lo que hace ahora; cada paso nuevo entra desde abajo.
            Ink {
                id: first
                width: Math.min(implicitWidth, root.room - (beside.visible ? beside.width + 8 : 0))
                elide: Text.ElideRight
                text: root.step?.texto ?? "Pensando"
                onTextChanged: if (root.busy) rise.restart()
                transform: Translate {
                    id: shift
                }

                ParallelAnimation {
                    id: rise
                    NumberAnimation { target: shift; property: "y"; from: 8; to: 0; duration: 280; easing.type: Easing.OutCubic }
                    NumberAnimation { target: first; property: "opacity"; from: 0.2; to: 1; duration: 220; easing.type: Easing.OutCubic }
                }
            }

            // En reposo, lo preguntado va en la misma línea.
            Ink {
                id: beside
                width: Math.min(implicitWidth, Math.max(0, root.room - first.implicitWidth - 8))
                dim: true
                elide: Text.ElideRight
                text: root.asked
                visible: !root.busy && text !== ""
            }
        }

        Ink {
            id: second
            opacity: visible ? 1 : 0
            Behavior on opacity {
                NumberAnimation { duration: Tokens.veil; easing.type: Easing.OutCubic }
            }
            width: Math.min(implicitWidth, root.room)
            dim: true
            font.pixelSize: Tokens.smallSize
            elide: Text.ElideRight
            text: root.asked
            visible: root.busy && text !== ""
        }

        // Los sitios de las páginas que ha encontrado o leído.
        Row {
            spacing: 6
            visible: root.busy && root.sources.length > 0

            Icon {
                anchors.verticalCenter: parent.verticalCenter
                name: "globe"
                font.pixelSize: Tokens.smallSize
            }

            Ink {
                id: third
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(implicitWidth, root.room - 20)
                dim: true
                font.pixelSize: Tokens.smallSize
                elide: Text.ElideRight
                text: root.sources.map(s => s.host).join(" · ")
            }
        }
    }

    // Cuánto lleva; con el puntero encima, el aspa que lo para.
    Item {
        id: side
        anchors.right: parent.right
        anchors.rightMargin: Tokens.pad
        y: words.y + (first.height - height) / 2
        width: root.took !== "" || root.stoppable ? Math.max(label.implicitWidth, cross.implicitWidth) : 0
        height: Math.max(label.implicitHeight, cross.implicitHeight)

        Ink {
            id: label
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            dim: true
            font.pixelSize: Tokens.smallSize
            text: root.took
            opacity: hover.hovered && root.stoppable ? 0 : 1

            Behavior on opacity {
                NumberAnimation { duration: Tokens.fast }
            }
        }

        Icon {
            id: cross
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            name: "x-circle"
            color: halt.containsMouse ? Tokens.text : Tokens.dim
            opacity: hover.hovered && root.stoppable ? 1 : 0
            visible: opacity > 0

            Behavior on opacity {
                NumberAnimation { duration: Tokens.fast }
            }

            MouseArea {
                id: halt
                anchors.fill: parent
                anchors.margins: -6
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: Daemon.stop()
            }
        }
    }
}
