import QtQuick
import Quickshell.Hyprland
import qs
import qs.state

// La torre: lo que enseña la isla del contexto al abrirse. Todos los niveles
// del monitor, uno encima de otro, cada uno con su tira de ventanas. Los
// marcos de todas las tiras caen en la misma columna: es la pantalla, y
// subir o bajar de nivel es moverse por ella.
Item {
    id: root

    property HyprlandMonitor monitor
    property var levels: []  // ver Contexts.levels
    readonly property int row: Tokens.island
    readonly property int inset: Tokens.inset
    readonly property int current: levels.findIndex(level => level.current)
    // Sitio común a los lados del marco, para que todos se alineen.
    property real before: 0
    property real after: 0

    function align(): void {
        let lead = 0;
        let trail = 0;
        for (let i = 0; i < rows.count; i++) {
            const strip = rows.itemAt(i)?.strip;
            lead = Math.max(lead, strip?.lead ?? 0);
            trail = Math.max(trail, strip?.trail ?? 0);
        }
        before = lead;
        after = trail;
    }

    implicitWidth: column.width + inset * 2
    implicitHeight: column.height + inset * 2

    // Con el ratón: el nivel bajo el puntero se marca y, pulsado, se va a él.
    function at(y: real): int {
        const index = Math.floor((y - inset) / row);
        return index >= 0 && index < levels.length ? index : -1;
    }

    readonly property int pointed: hover.hovered ? at(hover.point.position.y) : -1

    HoverHandler {
        id: hover
        cursorShape: root.pointed >= 0 && root.pointed !== root.current ? Qt.PointingHandCursor : Qt.ArrowCursor
    }

    TapHandler {
        onTapped: point => Contexts.go(root.levels[root.at(point.position.y)])
    }

    Rectangle {
        x: root.inset
        y: root.inset + Math.max(0, root.pointed) * root.row
        width: parent.width - root.inset * 2
        height: root.row
        radius: Tokens.round - root.inset
        color: Qt.alpha(Tokens.text, 0.06)
        visible: root.pointed >= 0 && root.pointed !== root.current
    }

    // El nivel en el que estás
    Rectangle {
        x: root.inset
        y: root.inset + Math.max(0, root.current) * root.row
        width: parent.width - root.inset * 2
        height: root.row
        radius: Tokens.round - root.inset
        color: Tokens.surface
        visible: root.current >= 0

        Behavior on y {
            NumberAnimation { duration: Tokens.veil; easing.type: Easing.OutCubic }
        }
    }

    Column {
        id: column
        x: root.inset
        y: root.inset

        Repeater {
            id: rows
            model: root.levels.length
            onItemAdded: root.align()
            onItemRemoved: root.align()

            Row {
                id: line

                required property int index
                readonly property var level: root.levels[index] ?? null
                readonly property bool current: level?.current ?? false
                readonly property alias strip: strip

                height: root.row
                leftPadding: Tokens.pad - root.inset
                rightPadding: Tokens.pad - root.inset
                spacing: Tokens.pad - 2

                Ink {
                    anchors.verticalCenter: parent.verticalCenter
                    width: Tokens.fontSize
                    horizontalAlignment: Text.AlignHCenter
                    // Una vista no tiene número: su inicial.
                    text: line.level?.number || (line.level?.name ?? "").charAt(0).toUpperCase()
                    strong: line.current
                    color: line.current ? Tokens.accent : Tokens.dim
                }

                Strip {
                    id: strip
                    anchors.verticalCenter: parent.verticalCenter
                    workspace: line.level?.workspace ?? null
                    monitor: root.monitor
                    unit: Math.round(Tokens.tile * 1.375)
                    lit: line.current
                    before: root.before
                    after: root.after
                    onLeadChanged: root.align()
                    onTrailChanged: root.align()
                }

                // Su nombre, si lo tiene, y lo que hay abierto en él
                Ink {
                    anchors.verticalCenter: parent.verticalCenter
                    text: [line.level?.name ?? ""].concat(Windows.apps(line.level?.workspace ?? null)).filter(part => part !== "").join("  ·  ")
                    dim: !line.current
                    visible: text !== ""
                }
            }
        }
    }
}
