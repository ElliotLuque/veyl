import QtQuick
import qs
import qs.state

// Las notificaciones apiladas en el velo, todas desde el mismo sitio: la que
// llega sale del borde de arriba y empuja hacia abajo a las que ya estaban.
// Caben unas pocas; las demás esperan detrás y se cuentan en la de abajo.
Column {
    id: root

    property Item typist: null  // la tarjeta en la que se escribe una respuesta
    readonly property bool typing: typist !== null
    readonly property bool busy: hover.hovered || typing

    width: Tokens.card

    HoverHandler {
        id: hover
    }

    Repeater {
        model: Notifications.deck

        Item {
            id: slot

            required property int key
            required property bool gone
            readonly property var note: gone ? null : Notifications.find(key)
            readonly property int rank: note ? Notifications.front.indexOf(key) : -1
            property bool born: false

            width: root.width
            height: Math.round(card.implicitHeight * fold.value)
            clip: true

            Component.onCompleted: Qt.callLater(() => born = true)

            // Se abre al llegar y se pliega al irse, o al quedarse detrás.
            Spring {
                id: fold
                to: slot.born && slot.rank >= 0 ? 1 : 0
                settle: Tokens.arrive
            }

            Notice {
                id: card

                // Baja desde el borde mientras su hueco se abre.
                y: slot.height - implicitHeight
                note: slot.note
                waiting: slot.rank >= 0 && slot.rank === Notifications.front.length - 1 ? Notifications.waiting : 0
                onTypingChanged: {
                    if (typing)
                        root.typist = card;
                    else if (root.typist === card)
                        root.typist = null;
                }
                Component.onDestruction: if (root.typist === card) root.typist = null

                // Se escribe en una sola a la vez.
                Connections {
                    target: root

                    function onTypistChanged(): void {
                        if (root.typist && root.typist !== card)
                            card.typing = false;
                    }
                }
            }

            // Entre tarjeta y tarjeta, un canto.
            Rectangle {
                x: Tokens.pad
                width: parent.width - Tokens.pad * 2
                height: Tokens.line
                color: Tokens.border
                visible: slot.rank > 0
            }
        }
    }
}
