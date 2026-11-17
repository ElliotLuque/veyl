import QtQuick
import qs
import qs.state

// El anuncio de un evento que se acerca: lo que enseña la isla de la hora
// unos segundos al volverse inminente. Arriba, junto a la hora, cuánto
// falta; debajo, su título y a qué hora es (y dónde, si lo dice).
Item {
    id: root

    property var event: null
    property real room: 0  // lo que ocupa la fila de la isla, arriba a la derecha
    // Lo último anunciado: se queda mientras la isla se cierra.
    property var shown: null
    readonly property int minutes: shown ? Math.ceil((shown.at - Agenda.now) / 60000) : 0

    onEventChanged: if (event) shown = event

    implicitWidth: Math.max(320, Math.min(440, Math.max(title.implicitWidth, when.implicitWidth) + Tokens.pad * 2 + bar.width + 12), room + 120)
    implicitHeight: column.height + Tokens.inset

    Column {
        id: column
        width: parent.width

        // Junto a la hora, cuánto falta
        Item {
            width: parent.width
            height: Tokens.island

            Label {
                anchors.verticalCenter: parent.verticalCenter
                x: Tokens.pad
                icon: "bell-ringing"
                tone: Tokens.attention
                text: root.minutes > 0 ? `En ${root.minutes} min` : "Ahora"
            }
        }

        Item {
            width: parent.width
            height: lines.height + 14

            // Del color de su calendario
            Rectangle {
                id: bar
                x: Tokens.pad
                anchors.verticalCenter: lines.verticalCenter
                width: 4
                height: lines.height - 4
                radius: width / 2
                color: Agenda.hue(root.shown)
            }

            Column {
                id: lines
                x: bar.x + bar.width + 12
                y: 4
                width: parent.width - x - Tokens.pad
                spacing: 2

                Ink {
                    id: title
                    width: parent.width
                    text: root.shown?.title ?? ""
                    strong: true
                    font.pixelSize: Tokens.figureSize - 4
                    elide: Text.ElideRight
                }

                Ink {
                    id: when
                    width: parent.width
                    text: {
                        const event = root.shown;
                        if (!event)
                            return "";
                        const parts = [Agenda.clock(event.at) + (event.until > event.at ? ` – ${Agenda.clock(event.until)}` : "")];
                        if (event.place)
                            parts.push(event.place);
                        if (event.calendar)
                            parts.push(event.calendar);
                        return parts.join(" · ");
                    }
                    dim: true
                    elide: Text.ElideRight
                }
            }
        }
    }
}
