import QtQuick
import qs
import qs.base
import qs.state

// Lo que lleva la isla de los ajustes: a la izquierda, las secciones de
// arriba abajo, como los niveles (la actual, con su marca larga del acento);
// a la derecha, la sección, que entra en vertical hacia donde te mueves.
FocusScope {
    id: root

    required property real wide
    required property real tall
    property bool live: false  // cargada: solo mientras se ve
    readonly property real page: wide - Tokens.aisle - Tokens.inset * 3
    property int was: 0  // la sección de antes, para saber hacia dónde entra

    implicitWidth: wide
    implicitHeight: tall

    Keys.onEscapePressed: Tuning.hide()
    Keys.onUpPressed: Tuning.step(-1)
    Keys.onDownPressed: Tuning.step(1)
    Keys.onTabPressed: Tuning.step(1)
    Keys.onBacktabPressed: Tuning.step(-1)

    // Al cambiar de sección, la nueva llega desde el lado hacia el que vas.
    Connections {
        target: Tuning

        function onSectionChanged(): void {
            const at = Tuning.sections.indexOf(Tuning.current);
            slide.velocity = 0;
            slide.value = (at >= root.was ? 1 : -1);
            root.was = at;
            flick.contentY = 0;
        }
    }

    Spring {
        id: slide
        to: 0
        settle: Tokens.veil
        bounce: 0.15
    }

    // Las secciones
    Item {
        id: aisle
        x: Tokens.inset
        y: Tokens.inset
        width: Tokens.aisle
        height: root.tall - Tokens.inset * 2

        Row {
            x: Tokens.pad - Tokens.inset + 2
            height: 54
            spacing: 12

            Icon {
                anchors.verticalCenter: parent.verticalCenter
                name: "gear-six"
                color: Tokens.accent
                font.pixelSize: 24
            }

            Ink {
                anchors.verticalCenter: parent.verticalCenter
                text: "Ajustes"
                strong: true
                font.pixelSize: Tokens.promptSize
            }
        }

        Column {
            y: 62
            width: parent.width
            spacing: 2

            Repeater {
                model: Tuning.sections

                Rectangle {
                    id: entry

                    required property var modelData
                    readonly property bool chosen: Tuning.current?.id === modelData.id

                    width: parent.width
                    height: 40
                    radius: Tokens.round - Tokens.inset
                    color: chosen ? Tokens.surface : hit.containsMouse ? Qt.alpha(Tokens.surface, 0.6) : "transparent"

                    Behavior on color {
                        ColorAnimation { duration: Tokens.fast }
                    }

                    // La marca del nivel: larga y del acento, la actual.
                    Rectangle {
                        x: 6
                        anchors.verticalCenter: parent.verticalCenter
                        width: 3
                        height: entry.chosen ? 18 : 6
                        radius: 1.5
                        color: entry.chosen ? Tokens.accent : Tokens.border

                        Behavior on height {
                            NumberAnimation { duration: Tokens.fast; easing.type: Easing.OutCubic }
                        }
                    }

                    Icon {
                        id: glyph
                        x: 20
                        anchors.verticalCenter: parent.verticalCenter
                        name: entry.modelData.glyph
                        color: entry.chosen ? Tokens.accent : Tokens.dim
                        font.pixelSize: Tokens.iconSize - 1
                    }

                    Ink {
                        anchors.left: glyph.right
                        anchors.leftMargin: 12
                        anchors.right: badge.visible ? badge.left : parent.right
                        anchors.rightMargin: badge.visible ? 8 : 10
                        anchors.verticalCenter: parent.verticalCenter
                        text: entry.modelData.title
                        dim: !entry.chosen && !hit.containsMouse
                        elide: Text.ElideRight
                    }

                    // Cuántos cambios pendientes o actualizaciones hay en Nix.
                    Rectangle {
                        id: badge

                        readonly property int count: entry.modelData.id === "nix" && Nix.systemActive ? (Nix.active ? Nix.drift.length : 0) + Nix.updates.length : 0

                        anchors.right: parent.right
                        anchors.rightMargin: 10
                        anchors.verticalCenter: parent.verticalCenter
                        width: Math.max(20, badgeText.implicitWidth + 10)
                        height: 20
                        radius: 10
                        color: Qt.alpha(Nix.active && Nix.unsaved.length > 0 ? Tokens.attention : Tokens.accent, 0.2)
                        visible: count > 0

                        Ink {
                            id: badgeText
                            anchors.centerIn: parent
                            text: String(badge.count)
                            color: Nix.active && Nix.unsaved.length > 0 ? Tokens.attention : Tokens.accent
                            font.pixelSize: Tokens.tinySize
                            strong: true
                        }
                    }

                    MouseArea {
                        id: hit
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: Tuning.section = entry.modelData.id
                    }
                }
            }
        }

        // Al pie, cómo se mueve uno por aquí.
        Row {
            x: Tokens.pad - Tokens.inset + 2
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 8
            spacing: 6

            Keycap {
                anchors.verticalCenter: parent.verticalCenter
                keys: "↑↓"
            }

            Ink {
                anchors.verticalCenter: parent.verticalCenter
                text: "sección"
                dim: true
                font.pixelSize: Tokens.tinySize + 1
            }

            Item {
                width: 8
                height: 1
            }

            Keycap {
                anchors.verticalCenter: parent.verticalCenter
                keys: "esc"
            }

            Ink {
                anchors.verticalCenter: parent.verticalCenter
                text: "cerrar"
                dim: true
                font.pixelSize: Tokens.tinySize + 1
            }
        }
    }

    // La sección
    Rectangle {
        id: sheet
        x: aisle.x + aisle.width + Tokens.inset
        y: Tokens.inset
        width: root.wide - x - Tokens.inset
        height: root.tall - Tokens.inset * 2
        radius: Tokens.round - Tokens.inset
        color: Qt.alpha(Tokens.surface, 0.35)
        clip: true

        Item {
            id: head
            width: parent.width
            height: 64
            y: slide.value * 14
            opacity: 1 - Math.abs(slide.value)

            Ink {
                x: Tokens.pad + 4
                anchors.verticalCenter: parent.verticalCenter
                text: Tuning.current?.title ?? ""
                strong: true
                font.pixelSize: Tokens.promptSize + 2
            }

            Knob {
                anchors.right: parent.right
                anchors.rightMargin: Tokens.pad
                anchors.verticalCenter: parent.verticalCenter
                icon: "x-circle"
                tint: Tokens.dim
                lit: Tokens.border
                onPressed: Tuning.hide()
            }
        }

        Flickable {
            id: flick
            anchors.top: head.bottom
            anchors.bottom: drift.top
            width: parent.width
            contentHeight: body.height + Tokens.pad * 2
            boundsBehavior: Flickable.StopAtBounds
            clip: true

            Loader {
                id: body
                x: Tokens.pad
                y: slide.value * 40
                width: flick.width - Tokens.pad * 2
                opacity: 1 - Math.abs(slide.value)
                active: root.live
                sourceComponent: ({
                        look: look,
                        sound: sound,
                        screens: screens,
                        network: network,
                        bluetooth: bluetooth,
                        input: input,
                        alerts: alerts,
                        calendars: calendars,
                        voice: voice,
                        journal: journal,
                        session: session,
                        nix: declared,
                        about: about
                    })[Tuning.current?.id] ?? null
            }
        }

        // Al pie, en Nix: siempre visible mientras haya cambios por aplicar
        // (sin guardar en tu .nix, guardados pendientes de reconstruir, o error
        // al guardar), salvo dentro de la propia pestaña Nix.
        Item {
            id: drift

            readonly property bool failed: Nix.failure !== ""
            readonly property bool pendingSave: Nix.unsaved.length > 0
            readonly property bool pendingBuild: !pendingSave && Nix.unbuilt.length > 0
            readonly property bool shown: Nix.active && Tuning.current?.id !== "nix" && (failed || pendingSave || pendingBuild)
            readonly property int unsavedCount: Nix.unsaved.length
            readonly property int unbuiltCount: Nix.unbuilt.length

            anchors.bottom: parent.bottom
            width: parent.width
            height: shown ? 60 : 0
            clip: true

            Behavior on height {
                NumberAnimation { duration: Tokens.fast; easing.type: Easing.OutCubic }
            }

            Rectangle {
                x: Tokens.pad
                width: parent.width - Tokens.pad * 2
                height: Tokens.line
                color: Tokens.border
            }

            Icon {
                id: flake
                x: Tokens.pad + 4
                anchors.verticalCenter: parent.verticalCenter
                name: drift.failed ? "warning" : drift.pendingBuild ? "hammer" : "snowflake"
                color: drift.failed || drift.pendingSave ? Tokens.attention : Tokens.accent
            }

            Ink {
                anchors.left: flake.right
                anchors.leftMargin: 12
                anchors.right: actions.left
                anchors.rightMargin: 12
                anchors.verticalCenter: parent.verticalCenter
                color: drift.failed ? Tokens.attention : Tokens.text
                text: {
                    if (drift.failed)
                        return Nix.failure;
                    if (drift.pendingSave) {
                        const extra = drift.unbuiltCount > 0 ? ` (+${drift.unbuiltCount} por reconstruir)` : "";
                        return drift.unsavedCount === 1 ? `${Nix.unsaved[0].title} · cambio por aplicar en Nix${extra}` : `${drift.unsavedCount} cambios por aplicar en Nix${extra}`;
                    }
                    if (drift.pendingBuild)
                        return drift.unbuiltCount === 1 ? "1 cambio guardado en Nix · falta reconstruir" : `${drift.unbuiltCount} cambios guardados en Nix · falta reconstruir`;
                    return "";
                }
                elide: Text.ElideRight
            }

            Row {
                id: actions
                anchors.right: parent.right
                anchors.rightMargin: Tokens.pad
                anchors.verticalCenter: parent.verticalCenter
                spacing: 8

                Button {
                    icon: "arrow-counter-clockwise"
                    text: "Deshacer"
                    visible: drift.pendingSave && !drift.failed
                    onPressed: Nix.revertAll()
                }

                Button {
                    text: "Revisar"
                    onPressed: Tuning.section = "nix"
                }

                Button {
                    icon: "arrow-square-out"
                    text: "Abrir"
                    visible: drift.failed && Nix.file !== ""
                    onPressed: {
                        Tuning.hide();
                        Qt.openUrlExternally(`file://${Nix.file}`);
                    }
                }

                Button {
                    icon: "floppy-disk"
                    text: Nix.busy ? "Guardando…" : "Guardar en Nix"
                    strong: true
                    visible: drift.pendingSave && !drift.failed && Nix.file !== ""
                    onPressed: Nix.save(Nix.unsaved)
                }

                Button {
                    icon: "hammer"
                    text: "Reconstruir"
                    strong: true
                    visible: drift.pendingBuild && !drift.failed && Nix.rebuild !== ""
                    onPressed: {
                        Tuning.hide();
                        Nix.build();
                    }
                }
            }
        }

        // Cuánto queda por ver: una barra fina a la derecha.
        Rectangle {
            readonly property real share: flick.height / Math.max(flick.height, flick.contentHeight)

            anchors.right: parent.right
            anchors.rightMargin: 4
            y: flick.y + flick.visibleArea.yPosition * flick.height
            width: 3
            height: share * flick.height
            radius: 1.5
            color: Tokens.border
            visible: share < 1
        }
    }

    Component {
        id: look
        Look {}
    }

    Component {
        id: sound
        Audio {}
    }

    Component {
        id: screens
        Displays {}
    }

    Component {
        id: network
        Networks {}
    }

    Component {
        id: bluetooth
        Devices {}
    }

    Component {
        id: input
        Inputs {}
    }

    Component {
        id: alerts
        Alerts {}
    }

    Component {
        id: calendars
        Calendars {}
    }

    Component {
        id: voice
        Voice {}
    }

    Component {
        id: journal
        Journal {}
    }

    Component {
        id: session
        Session {}
    }

    Component {
        id: declared
        Declared {}
    }

    Component {
        id: about
        About {}
    }
}
