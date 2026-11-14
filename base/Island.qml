import QtQuick
import QtQuick.Shapes
import qs

// Isla: no flota. Sale de una esquina de la pantalla, pegada a sus dos bordes
// y del mismo color, como una muesca del marco; donde se une a cada borde
// hace una curva hacia fuera. En reposo lleva lo suyo en fila y se estira con
// ello; abierta, crece desde la esquina, con un rebote, hasta dar cabida a su
// detalle, que sustituye a la fila o, si esta se queda (`keep`), la rodea.
Item {
    id: root

    property bool mirrored: false  // sale de la esquina derecha, no de la izquierda
    property bool expanded: false
    property bool keep: false      // abierta, la fila sigue en su sitio
    property alias bounce: morph.bounce
    property alias settle: morph.settle  // ms en abrirse
    default property alias content: row.data
    property alias detail: holder.data  // lo que enseña abierta
    readonly property real open: morph.value
    readonly property real rest: row.width + Tokens.pad * 2
    readonly property alias hovered: hover.hovered
    // La esquina libre: medio alto en reposo, más redonda abierta.
    readonly property real corner: Math.min(height / 2, Tokens.island / 2 + (Tokens.round - Tokens.island / 2) * Math.max(0, open))

    // Un empujón hacia arriba (-1) o hacia abajo (1): cede y vuelve.
    function nudge(way: int): void {
        sway.velocity += way * 160;
    }

    // Una fila que se queda no cabe en menos de lo que ya ocupa.
    implicitWidth: rest + (Math.max(holder.width, keep ? rest : 0) - rest) * open
    implicitHeight: Tokens.island + (holder.height - Tokens.island) * open + sway.value

    Spring {
        id: morph
        to: root.expanded && holder.children.length > 0 ? 1 : 0
        settle: Tokens.veil
        bounce: 0.3
    }

    Spring {
        id: sway
        settle: Tokens.veil
        bounce: 0.3
    }

    Shape {
        id: plate

        readonly property real w: root.width
        readonly property real h: root.height
        readonly property real join: Tokens.join
        readonly property real corner: root.corner
        // El trazo empieza y acaba fuera de la pantalla: solo se ve el canto libre.
        readonly property real out: -Tokens.line * 2

        preferredRendererType: Shape.CurveRenderer
        transform: Scale {
            origin.x: root.width / 2
            xScale: root.mirrored ? -1 : 1
        }

        ShapePath {
            fillColor: Tokens.plate
            strokeColor: Tokens.border
            strokeWidth: Tokens.line
            startX: plate.out
            startY: plate.out

            PathLine { x: plate.w + plate.join; y: plate.out }
            PathLine { x: plate.w + plate.join; y: 0 }
            PathArc { x: plate.w; y: plate.join; radiusX: plate.join; radiusY: plate.join; direction: PathArc.Counterclockwise }
            PathLine { x: plate.w; y: plate.h - plate.corner }
            PathArc { x: plate.w - plate.corner; y: plate.h; radiusX: plate.corner; radiusY: plate.corner }
            PathLine { x: plate.join; y: plate.h }
            PathArc { x: 0; y: plate.h + plate.join; radiusX: plate.join; radiusY: plate.join; direction: PathArc.Counterclockwise }
            PathLine { x: plate.out; y: plate.h + plate.join }
            PathLine { x: plate.out; y: plate.out }
        }
    }

    HoverHandler {
        id: hover
    }

    // La fila y el detalle se quedan quietos en la pantalla, pegados a la
    // esquina de la que sale la isla; es la placa la que crece y los destapa.
    Item {
        anchors.fill: parent
        clip: true

        Item {
            id: holder
            x: root.mirrored ? root.width - width : 0
            width: childrenRect.width
            height: childrenRect.height
            // Entra mientras la placa crece, como en la isla del centro.
            opacity: Math.min(1, Math.max(0, (root.open - 0.35) * 2))
            visible: opacity > 0
            scale: 0.94 + 0.06 * Math.min(1, root.open)
            transformOrigin: root.mirrored ? Item.TopRight : Item.TopLeft
        }

        Row {
            id: row
            x: root.mirrored ? root.width - width - Tokens.pad : Tokens.pad
            y: (Tokens.island - height) / 2
            opacity: root.keep ? 1 : 1 - Math.min(1, Math.max(0, root.open * 2.5))
            visible: opacity > 0
        }
    }
}
