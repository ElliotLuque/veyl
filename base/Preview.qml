import QtQuick
import qs
import qs.state

// Lo copiado, a la vista: el texto, la imagen o el color de la fila elegida
// del portapapeles y, debajo, lo que se sabe de ello.
Item {
    id: root

    property var item  // la fila elegida, o null
    readonly property var entry: item?.entry ?? null
    readonly property string kind: entry?.kind ?? ""
    readonly property string type: entry ? Clipboard.type(entry) : ""
    readonly property var facts: {
        if (!entry)
            return [];
        const count = number => Number(number ?? 0).toLocaleString(Qt.locale(), "f", 0);
        const facts = [];
        if (kind === "image") {
            facts.push({ name: "Tipo", value: `Imagen ${entry.mime.split("/").pop().toUpperCase()}` });
            if (entry.width > 0)
                facts.push({ name: "Medidas", value: `${entry.width} × ${entry.height}` });
        } else {
            facts.push({ name: "Tipo", value: ({ link: "Enlace", color: "Color", email: "Correo", path: "Ruta", json: "JSON", math: "Cuenta" })[type] ?? "Texto" });
            facts.push({ name: "Largo", value: `${count(entry.chars)} ${entry.chars === 1 ? "carácter" : "caracteres"}` + (entry.lines > 1 ? ` en ${count(entry.lines)} líneas` : "") });
        }
        facts.push({ name: "Tamaño", value: Clipboard.size(entry.bytes) });
        facts.push({ name: "Copiado", value: Clipboard.ago(entry.time, Launcher.opened) });
        return facts;
    }

    Item {
        id: stage

        x: Tokens.pad
        y: Tokens.pad - 4
        width: parent.width - Tokens.pad * 2
        height: about.y - y - Tokens.pad + 4

        Ink {
            anchors.fill: parent
            text: root.kind === "text" || root.kind === "link" ? root.entry.text : ""
            font.family: root.type === "json" || root.type === "path" ? "monospace" : Tokens.sans
            textFormat: Text.PlainText
            wrapMode: Text.WrapAtWordBoundaryOrAnywhere
            elide: Text.ElideRight
            lineHeight: 1.3
            font.pixelSize: Tokens.smallSize
            color: root.kind === "link" ? Tokens.accent : Tokens.text
            visible: text !== ""
        }

        Clip {
            anchors.centerIn: parent
            width: picture.paintedWidth
            height: picture.paintedHeight
            radius: Tokens.radius
            color: "transparent"
            visible: root.kind === "image" && picture.status === Image.Ready

            Image {
                id: picture
                anchors.centerIn: parent
                width: stage.width
                height: stage.height
                source: root.kind === "image" ? Clipboard.file(root.entry.id) : ""
                sourceSize: Qt.size(width * 2, height * 2)
                fillMode: Image.PreserveAspectFit
                asynchronous: true
            }
        }

        Column {
            anchors.centerIn: parent
            spacing: 14
            visible: root.kind === "color"

            Rectangle {
                anchors.horizontalCenter: parent.horizontalCenter
                width: 96
                height: 96
                radius: Tokens.round
                color: root.item?.swatches?.[0] ?? "transparent"
                border.width: Tokens.line
                border.color: Tokens.border
            }

            Ink {
                anchors.horizontalCenter: parent.horizontalCenter
                text: root.kind === "color" ? root.entry.text.trim() : ""
                textFormat: Text.PlainText
                strong: true
            }
        }
    }

    Column {
        id: about

        x: Tokens.pad
        y: parent.height - height - 2
        width: parent.width - Tokens.pad * 2

        Repeater {
            model: root.facts

            Item {
                id: fact

                required property var modelData

                width: about.width
                height: 30

                Rectangle {
                    width: parent.width
                    height: Tokens.line
                    color: Tokens.border
                }

                Ink {
                    anchors.verticalCenter: parent.verticalCenter
                    text: fact.modelData.name
                    font.pixelSize: Tokens.smallSize
                    dim: true
                }

                Ink {
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    text: fact.modelData.value
                    textFormat: Text.PlainText
                    font.pixelSize: Tokens.smallSize
                }
            }
        }
    }
}
