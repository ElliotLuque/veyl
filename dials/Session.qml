import QtQuick
import qs
import qs.base
import qs.state

// Sesión y energía: cuándo se bloquea sola, el perfil de energía, la batería
// y salir, reiniciar o apagar.
Column {
    id: root

    property string armed: ""  // lo peligroso pide una segunda pulsación
    readonly property var session: ["lock", "suspend", "logout", "reboot", "poweroff"].map(id => Commands.list.find(order => order.key === `order:${id}`)).filter(order => order)
    readonly property var names: ({ "power-saver": "Ahorro", balanced: "Equilibrado", performance: "Rendimiento" })

    spacing: 22

    Component.onCompleted: Host.refresh("session")

    Timer {
        id: disarm
        interval: 3000
        onTriggered: root.armed = ""
    }

    Group {
        title: "Bloqueo"

        Option {
            glyph: "lock"
            title: "Bloqueo automático por inactividad"

            Select {
                width: 200
                options: [0, 1, 2, 5, 10, 15, 30, 60].map(value => ({ value, label: value === 0 ? "Nunca" : value === 1 ? "1 minuto" : value === 60 ? "1 hora" : `${value} minutos` }))
                value: Settings.lockAfter
                onPicked: value => Settings.lockAfter = value
            }
        }
    }

    Group {
        title: "Energía"

        Option {
            glyph: "lightning"
            title: "Perfil de energía"
            text: Host.profiles.length > 0 ? "" : "Requiere power-profiles-daemon"

            Select {
                width: 200
                visible: Host.profiles.length > 0
                options: Host.profiles.map(value => ({ value, label: root.names[value] ?? value }))
                value: Host.profile
                onPicked: value => Host.setProfile(value)
            }
        }

        Option {
            visible: Battery.present
            glyph: !Battery.discharging ? "battery-charging" : Battery.critical ? "battery-warning" : Battery.low ? "battery-low" : "battery-full"
            tone: Battery.critical ? Tokens.critical : Battery.low ? Tokens.attention : Tokens.dim
            title: "Batería"
            text: Battery.discharging ? `${Battery.percent} %` : `${Battery.percent} % · Conectada a la corriente`
        }
    }

    Group {
        title: "Sesión"

        Item {
            width: parent.width
            height: 64

            Row {
                x: Tokens.pad
                anchors.verticalCenter: parent.verticalCenter
                spacing: 8

                Repeater {
                    model: root.session

                    Button {
                        required property var modelData
                        readonly property bool waiting: root.armed === modelData.key

                        icon: modelData.glyph
                        text: waiting ? "Confirmar" : modelData.title
                        tone: waiting ? Tokens.attention : "transparent"
                        onPressed: {
                            if (modelData.confirm && !waiting) {
                                root.armed = modelData.key;
                                disarm.restart();
                                return;
                            }
                            root.armed = "";
                            Tuning.hide();
                            modelData.run();
                        }
                    }
                }
            }
        }
    }
}
