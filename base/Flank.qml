import QtQuick
import QtQuick.Shapes
import qs

// Isla que sale de en medio de un borde lateral, el izquierdo o, volteada
// (`mirrored`), el derecho: pegada a él, con una curva hacia fuera arriba y
// abajo, donde se une, y redondas sus dos esquinas libres. Nace del borde con
// un rebote, toma el tamaño de lo que lleva y, si cambia, se estira o se
// encoge; al irse vuelve a meterse en él.
Item {
    id: root

    property bool shown: false
    property bool mirrored: false
    property real settle: Tokens.arrive  // ms en nacer
    property real stretch: Tokens.veil   // ms en cambiar de tamaño
    property real bounce: 0.25
    default property alias content: holder.data
    readonly property real progress: Math.max(0, appear.value)

    implicitWidth: Math.max(0, wide.value * appear.value)
    implicitHeight: Math.max(0, tall.value * (0.6 + 0.4 * appear.value))

    Spring {
        id: appear
        to: root.shown ? 1 : 0
        settle: root.settle
        bounce: root.bounce
    }

    Spring {
        id: wide
        to: holder.width
        settle: root.stretch
        bounce: root.bounce
    }

    Spring {
        id: tall
        to: holder.height
        settle: root.stretch
        bounce: root.bounce
    }

    Shape {
        id: plate

        readonly property real w: root.width
        readonly property real h: root.height
        // Recién nacida aún no tiene ancho para sus curvas: crecen con ella.
        readonly property real join: Math.min(Tokens.join, w / 2)
        readonly property real corner: Math.min(Tokens.round, w - join, h / 2)
        // El trazo empieza y acaba fuera de la pantalla: solo se ve el canto libre.
        readonly property real out: -Tokens.line * 2

        preferredRendererType: Shape.CurveRenderer
        visible: w > 0
        // Se dibuja contra el borde izquierdo y, en el derecho, se voltea.
        transform: Scale {
            origin.x: root.width / 2
            xScale: root.mirrored ? -1 : 1
        }

        ShapePath {
            fillColor: Tokens.plate
            strokeColor: Tokens.border
            strokeWidth: Tokens.line
            startX: plate.out
            startY: -plate.join

            PathLine { x: 0; y: -plate.join }
            PathArc { x: plate.join; y: 0; radiusX: plate.join; radiusY: plate.join; direction: PathArc.Counterclockwise }
            PathLine { x: plate.w - plate.corner; y: 0 }
            PathArc { x: plate.w; y: plate.corner; radiusX: plate.corner; radiusY: plate.corner }
            PathLine { x: plate.w; y: plate.h - plate.corner }
            PathArc { x: plate.w - plate.corner; y: plate.h; radiusX: plate.corner; radiusY: plate.corner }
            PathLine { x: plate.join; y: plate.h }
            PathArc { x: 0; y: plate.h + plate.join; radiusX: plate.join; radiusY: plate.join; direction: PathArc.Counterclockwise }
            PathLine { x: plate.out; y: plate.h + plate.join }
            PathLine { x: plate.out; y: -plate.join }
        }
    }

    // Lo que lleva se queda pegado al borde: la placa crece y lo destapa.
    Item {
        anchors.fill: parent
        clip: true

        Item {
            id: holder
            x: root.mirrored ? root.width - width : 0
            y: (root.height - height) / 2
            width: childrenRect.width
            height: childrenRect.height
            opacity: Math.min(1, Math.max(0, (root.progress - 0.4) * 2.2))
            visible: opacity > 0
        }
    }
}
