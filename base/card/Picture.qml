import QtQuick
import Quickshell
import qs
import qs.base

// Una imagen de la web, con esquinas redondeadas; sin alto hasta que carga
// (si no carga, no está).
Clip {
    id: frame

    property alias source: art.source
    property int tall: 200
    readonly property bool ready: art.status === Image.Ready

    height: ready ? tall : 0
    visible: ready
    radius: Tokens.radius
    color: Tokens.surface

    Image {
        id: art
        anchors.fill: parent
        sourceSize.width: frame.width * 2
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        cache: true
    }
}
