import QtQuick
import QtQuick.Effects
import qs

// Logotipo de quien hace un agente, de un solo color (el que se le dé, del
// esquema). Si Veyl no lo trae, un icono de Phosphor.
Item {
    id: root

    property url source
    property string fallback: "robot"
    property color color: Tokens.text

    implicitWidth: Tokens.iconSize
    implicitHeight: Tokens.iconSize

    // El dibujo solo da la forma; el color lo pone una placa recortada con él.
    Image {
        id: art
        anchors.fill: parent
        source: root.source
        sourceSize: Qt.size(width * 2, height * 2)
        fillMode: Image.PreserveAspectFit
        visible: false
        layer.enabled: true
    }

    Rectangle {
        id: paint
        anchors.fill: parent
        color: root.color
        visible: false
        layer.enabled: true
    }

    MultiEffect {
        anchors.fill: parent
        visible: art.status === Image.Ready
        source: paint
        maskEnabled: true
        maskSource: art
    }

    Icon {
        anchors.centerIn: parent
        visible: art.status !== Image.Ready
        name: root.fallback
        color: root.color
    }
}
