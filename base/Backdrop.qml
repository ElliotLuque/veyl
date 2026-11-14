import QtQuick
import Quickshell
import Quickshell.Wayland
import qs
import qs.state

// El fondo de un monitor: el color de fondo del esquema y, encima, la imagen
// elegida. Al cambiar de fondo, el nuevo se funde sobre el anterior.
PanelWindow {
    id: root

    required property ShellScreen modelData

    screen: modelData
    anchors {
        top: true
        bottom: true
        left: true
        right: true
    }
    exclusionMode: ExclusionMode.Ignore
    // La capa es transparente y el color lo pone un rectángulo: con una sola
    // capa opaca entre las transparentes, el EGL de NVIDIA no deja activar el
    // contexto al ocultar las otras (eglError 0x3009).
    color: "transparent"
    WlrLayershell.layer: WlrLayer.Background
    WlrLayershell.namespace: "veyl-backdrop"

    Rectangle {
        anchors.fill: parent
        color: Tokens.bg
    }

    Item {
        id: stage

        readonly property url wanted: Wallpaper.source

        anchors.fill: parent
        onWantedChanged: plane.createObject(stage, { source: wanted })
        Component.onCompleted: plane.createObject(stage, { source: wanted })

        // El plano ya tapa a los anteriores: sobran.
        function settle(top: Item): void {
            for (const other of children) {
                if (other === top)
                    break;
                other.destroy();
            }
        }
    }

    // Un fondo: opaco aunque no tenga imagen, o aunque no cargue.
    Component {
        id: plane

        Rectangle {
            id: self

            required property url source
            property bool born: false

            anchors.fill: parent
            color: Tokens.bg
            opacity: born && picture.status !== Image.Loading ? 1 : 0
            onOpacityChanged: if (opacity === 1) stage.settle(self)
            Component.onCompleted: born = true

            Behavior on opacity {
                NumberAnimation { duration: Tokens.veil * 2; easing.type: Easing.InOutSine }
            }

            Image {
                id: picture
                anchors.fill: parent
                source: self.source
                // No más píxeles de los que caben en el monitor
                sourceSize: Qt.size(root.modelData.width * root.modelData.devicePixelRatio, root.modelData.height * root.modelData.devicePixelRatio)
                fillMode: Image.PreserveAspectCrop
                asynchronous: true
            }
        }
    }
}
