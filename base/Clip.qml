import QtQuick

// Un rectángulo que recorta lo que lleva dentro a su forma, esquinas
// redondeadas incluidas. Es el ClippingRectangle de Quickshell tal como lo
// arreglaron después de la 0.2.1: allí el ShaderEffectSource no tiene padre,
// no suelta la ventana de lo que recorta cuando la capa se recrea (cada vez
// que pasa a invisible) y Veyl caía al cambiar de monitor.
Item {
    id: root

    property color color: "white"
    property alias radius: shape.radius
    default property alias data: content.data

    Rectangle {
        id: shape
        anchors.fill: parent
        color: "#ffff0000"
        border.color: "#ff00ff00"
        border.width: 0
        layer.enabled: true
        visible: false
    }

    Item {
        anchors.fill: parent
        layer.enabled: true
        layer.samplerName: "content"
        layer.effect: ShaderEffect {
            fragmentShader: "qrc:/Quickshell/Widgets/shaders/cliprect.frag.qsb"
            property Rectangle rect: shape
            property color backgroundColor: root.color
            property color borderColor: "transparent"
        }

        Item {
            id: content
            anchors.fill: parent
        }
    }
}
