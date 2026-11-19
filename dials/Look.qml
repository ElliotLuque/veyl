import QtQuick
import Quickshell
import qs
import qs.base
import qs.state

// Apariencia: el esquema de color, el fondo de pantalla y el calendario.
Column {
    id: root

    spacing: 22

    Component.onCompleted: {
        Scheme.scan();
        Host.refresh("look");
    }

    Group {
        title: "Esquema de color"

        Flow {
            x: 10
            width: parent.width - 20
            topPadding: 10
            bottomPadding: 10
            spacing: 10

            Repeater {
                model: Scheme.all

                // Un esquema: sus colores en franjas, su nombre debajo.
                Rectangle {
                    id: scheme

                    required property var modelData
                    readonly property bool chosen: Scheme.name === modelData.id

                    width: (parent.width - 30) / 4
                    height: 92
                    radius: Tokens.soft + 4
                    color: Tokens.bg
                    border.width: chosen ? 2 : Tokens.line
                    border.color: chosen ? Tokens.accent : hit.containsMouse ? Tokens.dim : Tokens.border

                    Behavior on border.color {
                        ColorAnimation { duration: Tokens.fast }
                    }

                    Row {
                        x: 8
                        y: 8
                        width: parent.width - 16
                        height: 46

                        Repeater {
                            model: scheme.modelData.colors

                            Rectangle {
                                required property var modelData
                                required property int index
                                width: parent.width / 5
                                height: parent.height
                                color: modelData
                                // Las de los extremos, redondeadas por fuera.
                                radius: index === 0 || index === 4 ? 5 : 0

                                Rectangle {
                                    width: parent.width / 2
                                    height: parent.height
                                    x: parent.index === 0 ? parent.width / 2 : 0
                                    color: parent.color
                                    visible: parent.index === 0 || parent.index === 4
                                }
                            }
                        }
                    }

                    Ink {
                        x: 10
                        anchors.bottom: parent.bottom
                        anchors.bottomMargin: 10
                        width: parent.width - 36
                        text: scheme.modelData.name
                        font.pixelSize: Tokens.smallSize
                        elide: Text.ElideRight
                    }

                    Icon {
                        anchors.right: parent.right
                        anchors.rightMargin: 10
                        anchors.bottom: parent.bottom
                        anchors.bottomMargin: 9
                        name: "check"
                        color: Tokens.accent
                        font.pixelSize: Tokens.iconSize - 2
                        visible: scheme.chosen
                    }

                    MouseArea {
                        id: hit
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: Settings.scheme = scheme.modelData.id
                    }
                }
            }
        }
    }

    Group {
        title: "Fondo de pantalla"

        Option {
            glyph: "image"
            title: "Archivo"

            Field {
                width: 360
                value: Settings.wallpaper
                placeholder: "Ruta de la imagen"
                onCommitted: text => Settings.wallpaper = text.replace(/^~(?=\/|$)/, Quickshell.env("HOME")).trim()
            }
        }

        Flow {
            x: 10
            width: parent.width - 20
            bottomPadding: 10
            spacing: 10

            // Sin fondo: el color del esquema.
            Rectangle {
                width: (parent.width - 30) / 4
                height: width * 9 / 16
                radius: Tokens.soft + 4
                color: Tokens.bg
                border.width: !Settings.wallpaper ? 2 : Tokens.line
                border.color: !Settings.wallpaper ? Tokens.accent : none.containsMouse ? Tokens.dim : Tokens.border

                Label {
                    anchors.centerIn: parent
                    icon: "prohibit"
                    text: "Sin imagen"
                    dim: true
                }

                MouseArea {
                    id: none
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: Settings.wallpaper = ""
                }
            }

            Repeater {
                model: Host.wallpapers

                Item {
                    id: picture

                    required property string modelData
                    readonly property bool chosen: Settings.wallpaper === modelData

                    width: (parent.width - 30) / 4
                    height: width * 9 / 16

                    Clip {
                        anchors.fill: parent
                        radius: Tokens.soft + 4
                        color: Tokens.bg

                        Image {
                            anchors.fill: parent
                            source: "file://" + picture.modelData.split("/").map(encodeURIComponent).join("/")
                            sourceSize.width: 360
                            fillMode: Image.PreserveAspectCrop
                            asynchronous: true
                            cache: false
                            opacity: status === Image.Ready ? 1 : 0

                            Behavior on opacity {
                                NumberAnimation { duration: Tokens.fast }
                            }
                        }
                    }

                    Rectangle {
                        anchors.fill: parent
                        radius: Tokens.soft + 4
                        color: "transparent"
                        border.width: picture.chosen ? 2 : Tokens.line
                        border.color: picture.chosen ? Tokens.accent : look.containsMouse ? Tokens.text : Tokens.border
                    }

                    Rectangle {
                        anchors.right: parent.right
                        anchors.top: parent.top
                        anchors.margins: 6
                        width: 22
                        height: 22
                        radius: 11
                        color: Tokens.accent
                        visible: picture.chosen

                        Icon {
                            anchors.centerIn: parent
                            name: "check"
                            color: Tokens.bg
                            font.pixelSize: 13
                        }
                    }

                    MouseArea {
                        id: look
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: Settings.wallpaper = picture.modelData
                    }
                }
            }
        }
    }
}
