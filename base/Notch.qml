import QtQuick
import QtQuick.Shapes
import Quickshell
import qs

// Isla del centro: sale del borde de arriba como una muesca, con una curva
// hacia fuera a cada lado, y toma el tamaño de lo que lleva. Nace del borde
// con un rebote y, si lo que lleva cambia de tamaño, se estira o se encoge.
// Volteada (`flipped`), sale del borde de abajo.
Item {
    id: root

    property bool shown: false
    property bool flipped: false
    property real settle: Tokens.arrive  // ms en nacer
    property real stretch: Tokens.veil   // ms en cambiar de tamaño
    property real bounce: 0.3
    default property alias content: holder.data
    readonly property real progress: Math.max(0, appear.value)
    readonly property var host: QsWindow.window

    // Lo que asoma del borde de arriba, para quien siga el marco.
    readonly property Rim rim: Rim {
        screen: root.host?.screen ?? null
        at: start(root.host) + root.x
        extent: root.width
        depth: root.flipped || !root.visible || !root.host?.visible ? 0 : root.height
        corner: plate.corner
        join: plate.join
    }

    implicitWidth: Math.max(0, wide.value * (0.7 + 0.3 * appear.value))
    implicitHeight: Math.max(0, tall.value * appear.value)

    // Nace ya con un tamaño, el de otra isla que se convierte en esta: no
    // sale del borde, crece desde ahí.
    function from(size: size): void {
        appear.value = 1;
        appear.velocity = 0;
        wide.value = size.width;
        wide.velocity = 0;
        tall.value = size.height;
        tall.velocity = 0;
    }

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
        // Recién nacida aún no tiene alto para sus curvas: crecen con ella.
        readonly property real join: Math.min(Tokens.join, h / 2)
        readonly property real corner: Math.min(Tokens.round, h - join, w / 2)
        // El trazo empieza y acaba fuera de la pantalla: solo se ve el canto libre.
        readonly property real out: -Tokens.line * 2

        preferredRendererType: Shape.CurveRenderer
        visible: h > 0
        transform: Scale {
            origin.y: root.height / 2
            yScale: root.flipped ? -1 : 1
        }

        ShapePath {
            fillColor: Tokens.plate
            strokeColor: Tokens.border
            strokeWidth: Tokens.line
            startX: -plate.join
            startY: plate.out

            PathLine { x: plate.w + plate.join; y: plate.out }
            PathLine { x: plate.w + plate.join; y: 0 }
            PathArc { x: plate.w; y: plate.join; radiusX: plate.join; radiusY: plate.join; direction: PathArc.Counterclockwise }
            PathLine { x: plate.w; y: plate.h - plate.corner }
            PathArc { x: plate.w - plate.corner; y: plate.h; radiusX: plate.corner; radiusY: plate.corner }
            PathLine { x: plate.corner; y: plate.h }
            PathArc { x: 0; y: plate.h - plate.corner; radiusX: plate.corner; radiusY: plate.corner }
            PathLine { x: 0; y: plate.join }
            PathArc { x: -plate.join; y: 0; radiusX: plate.join; radiusY: plate.join; direction: PathArc.Counterclockwise }
            PathLine { x: -plate.join; y: plate.out }
        }
    }

    Item {
        anchors.fill: parent
        clip: true

        Item {
            id: holder
            x: (root.width - width) / 2
            y: root.flipped ? root.height - height : 0
            width: childrenRect.width
            height: childrenRect.height
            // Entra cuando la placa ya casi tiene su tamaño.
            opacity: Math.min(1, Math.max(0, (root.progress - 0.35) * 2))
            scale: 0.94 + 0.06 * Math.min(1, root.progress)
            transformOrigin: root.flipped ? Item.Bottom : Item.Top
        }
    }
}
