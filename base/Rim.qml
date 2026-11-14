import QtQuick
import Quickshell
import qs.state

// Lo que asoma una isla de en medio de un borde, apuntado en Rims: desde
// dónde (`at`, a lo largo del borde, en píxeles de la pantalla), cuánto
// ocupa a lo largo y cuánto entra, el radio de sus esquinas libres y el de
// las curvas con que se une al borde.
QtObject {
    id: root

    property ShellScreen screen: null
    property string edge: "top"  // top | right
    property real at: 0
    property real extent: 0
    property real depth: 0
    property real corner: 0
    property real join: 0

    // Dónde empieza su capa a lo largo del borde: pegada a un extremo o, sin
    // anclar a ninguno, centrada.
    function start(window: var): real {
        if (!window?.screen)
            return 0;
        if (edge === "right")
            return window.anchors.top ? 0 : window.anchors.bottom ? window.screen.height - window.height : (window.screen.height - window.height) / 2;
        return window.anchors.left ? 0 : window.anchors.right ? window.screen.width - window.width : (window.screen.width - window.width) / 2;
    }

    Component.onCompleted: Rims.add(root)
    Component.onDestruction: Rims.remove(root)
}
