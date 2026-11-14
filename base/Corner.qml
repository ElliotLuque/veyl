import QtQuick
import QtQuick.Shapes
import qs

// Isla que nace de una esquina, la que sea: pegada a sus dos bordes, con una
// curva hacia fuera donde se une a cada uno y redonda su única esquina libre.
// Aparece creciendo desde la esquina con un rebote, toma el tamaño de lo que
// lleva y, si cambia, se estira o se encoge; al irse vuelve a la esquina.
Item {
    id: root

    property bool shown: false
    property bool low: false       // sale de una esquina de abajo
    property bool mirrored: false  // sale de una esquina de la derecha
    property real settle: Tokens.arrive
    property real stretch: Tokens.veil
    property real bounce: 0.25
    default property alias content: holder.data
    readonly property real progress: Math.max(0, appear.value)

    implicitWidth: Math.max(0, wide.value * (0.55 + 0.45 * appear.value))
    implicitHeight: Math.max(0, tall.value * appear.value)

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
        // Recién nacida aún no tiene sitio para sus curvas: crecen con ella.
        readonly property real join: Math.min(Tokens.join, h / 2, w / 2)
        readonly property real corner: Math.min(Tokens.round, h / 2, w / 2)
        // El trazo empieza y acaba fuera de la pantalla: solo se ve el canto libre.
        readonly property real out: -Tokens.line * 2

        preferredRendererType: Shape.CurveRenderer
        visible: h > 0
        transform: Scale {
            origin.x: root.width / 2
            origin.y: root.height / 2
            xScale: root.mirrored ? -1 : 1
            yScale: root.low ? -1 : 1
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

    // Lo que lleva se queda pegado a la esquina: la placa crece y lo destapa.
    Item {
        anchors.fill: parent
        clip: true

        Item {
            id: holder
            x: root.mirrored ? root.width - width : 0
            y: root.low ? root.height - height : 0
            width: childrenRect.width
            height: childrenRect.height
            opacity: Math.min(1, Math.max(0, (root.progress - 0.4) * 2.2))
            visible: opacity > 0
        }
    }
}
