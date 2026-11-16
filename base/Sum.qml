import QtQuick
import qs
import qs.state

// La cuenta que se ha escrito y, al otro lado de la flecha, lo que da.
Item {
    id: root

    property var item              // la fila de la cuenta, o null
    property bool selected: false
    // Aire encima y debajo de la tarjeta, dentro de su fila.
    readonly property real above: 2
    readonly property real below: 6

    component Figure: Ink {
        y: root.above
        width: (root.width - arrow.width) / 2 - Tokens.gap - Tokens.pad * 2
        height: root.height - root.above - root.below
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        textFormat: Text.PlainText
        font.pixelSize: Tokens.figureSize
        // Una cuenta larga se encoge antes de cortarse.
        fontSizeMode: Text.HorizontalFit
        minimumPixelSize: Tokens.smallSize
        elide: Text.ElideRight
        maximumLineCount: 1
    }

    // Elegida la tapa el resalte de la lista; si no, su contorno la sigue
    // marcando como lo que es.
    Rectangle {
        x: Tokens.gap
        y: root.above
        width: parent.width - Tokens.gap * 2
        height: parent.height - root.above - root.below
        radius: Tokens.radius
        color: "transparent"
        border.width: Tokens.line
        border.color: Tokens.border
        visible: !root.selected
    }

    Figure {
        x: Tokens.gap + Tokens.pad
        text: root.item?.title ?? ""
        dim: true
    }

    Rectangle {
        id: arrow
        anchors.horizontalCenter: parent.horizontalCenter
        y: root.above + (parent.height - root.above - root.below - height) / 2
        width: 32
        height: 32
        radius: width / 2
        color: root.selected ? Tokens.border : Tokens.surface

        Icon {
            anchors.centerIn: parent
            name: "arrow-right"
            color: Tokens.accent
            font.pixelSize: 16
        }
    }

    Figure {
        x: parent.width - width - Tokens.gap - Tokens.pad
        text: root.item?.result ?? ""
        strong: true
    }
}
