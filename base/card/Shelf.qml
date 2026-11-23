import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

Row {
    id: shelf

    // La pieza (su Loader en Card: el bloque, su ancho y su tono) y la tarjeta.
    required property Loader block
    required property Item host

    width: block.width
    spacing: 8

    Repeater {
        model: block.modelData.items

        Item {
            id: shot

            required property var modelData

            width: (shelf.width - shelf.spacing * (block.modelData.items.length - 1)) / block.modelData.items.length
            height: 120

            Clip {
                anchors.fill: parent
                radius: Tokens.radius
                color: Tokens.surface

                Image {
                    anchors.fill: parent
                    source: shot.modelData.imagen
                    sourceSize.width: width * 2
                    fillMode: Image.PreserveAspectCrop
                    asynchronous: true
                }
            }

            MouseArea {
                anchors.fill: parent
                enabled: shot.modelData.url !== ""
                cursorShape: Qt.PointingHandCursor
                onClicked: {
                    Qt.openUrlExternally(shot.modelData.url);
                    host.done();
                }
            }
        }
    }
}
