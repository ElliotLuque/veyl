import QtQuick
import qs
import qs.base
import qs.state

// Red: lo conectado ahora, el Wi-Fi y sus redes, y las VPN y demás
// conexiones guardadas, que se encienden y se apagan.
Column {
    id: root

    property string asking: ""  // la red Wi-Fi nueva que pide contraseña
    property string joining: ""

    spacing: 22

    Component.onCompleted: Host.refresh("network")

    function kind(type: string): string {
        return { ethernet: "Cable", wifi: "Wi-Fi", "802-3-ethernet": "Cable", "802-11-wireless": "Wi-Fi", vpn: "VPN", wireguard: "WireGuard", gsm: "Móvil", bluetooth: "Bluetooth" }[type] ?? type;
    }

    function glyph(type: string): string {
        return /wifi|wireless/.test(type) ? "wifi-high" : /vpn|wireguard/.test(type) ? "shield-check" : /gsm|cdma/.test(type) ? "cell-signal-full" : /bluetooth/.test(type) ? "bluetooth" : "plugs-connected";
    }

    function state(state: string): string {
        return state.startsWith("connected") ? "Conectado" : state === "disconnected" ? "Desconectado" : state === "unavailable" ? "No disponible" : state === "unmanaged" ? "No gestionado" : state.startsWith("connecting") ? "Conectando…" : state;
    }

    function pick(network: var): void {
        if (network.used)
            return;
        const saved = Host.connections.find(connection => connection.name === network.ssid);
        if (saved) {
            Host.connection(saved.uuid, true);
        } else if (network.secure) {
            asking = network.ssid;
        } else {
            joining = network.ssid;
            Host.join(network.ssid, "", () => joining = "");
        }
    }

    Ink {
        x: 4
        width: parent.width - 8
        text: Host.failure
        color: Tokens.attention
        font.pixelSize: Tokens.smallSize
        wrapMode: Text.Wrap
        visible: text !== ""
    }

    Group {
        title: "Interfaces"
        note: Host.devices.length === 0 ? (Tools.has("nmcli") ? "Consultando NetworkManager…" : "NetworkManager (nmcli) no está disponible.") : Network.offline ? "Sin acceso a internet." : ""

        Repeater {
            model: Host.devices

            Option {
                required property var modelData
                readonly property bool up: modelData.state.startsWith("connected")

                glyph: root.glyph(modelData.type)
                tone: up ? Tokens.text : Tokens.dim
                title: up && modelData.connection ? modelData.connection : `${root.kind(modelData.type)} · ${modelData.device}`
                text: [up ? root.kind(modelData.type) : "", root.state(modelData.state), modelData.address].filter(part => part).join(" · ")
            }
        }
    }

    Group {
        title: "Wi-Fi"
        visible: Host.wireless

        Option {
            glyph: Commands.wifi ? "wifi-high" : "wifi-slash"
            title: "Wi-Fi"
            text: !Commands.wifi ? "Desactivado" : Host.scanning ? "Buscando redes…" : `${Host.networks.length} redes disponibles`

            Button {
                icon: "arrows-clockwise"
                text: "Buscar"
                visible: Commands.wifi
                onPressed: Host.scan(true)
            }

            Toggle {
                on: Commands.wifi
                onToggled: on => Host.wifi(on)
            }
        }

        Repeater {
            model: Commands.wifi ? Host.networks : []

            Option {
                id: network

                required property var modelData
                readonly property bool asked: root.asking === modelData.ssid

                glyph: modelData.signal > 70 ? "wifi-high" : modelData.signal > 45 ? "wifi-medium" : modelData.signal > 20 ? "wifi-low" : "wifi-none"
                title: modelData.ssid
                text: [`${modelData.signal} %`, root.joining === modelData.ssid ? "Conectando…" : modelData.used ? "Conectada" : Host.connections.some(connection => connection.name === modelData.ssid) ? "Guardada" : ""].filter(part => part).join(" · ")
                current: modelData.used
                pressable: !asked
                onClicked: root.pick(modelData)

                Icon {
                    anchors.verticalCenter: parent.verticalCenter
                    name: "lock-simple"
                    font.pixelSize: Tokens.iconSize - 4
                    visible: network.modelData.secure && !network.asked
                }

                // La contraseña de una red nueva, en la misma fila.
                Row {
                    visible: network.asked
                    spacing: 8
                    anchors.verticalCenter: parent.verticalCenter

                    Field {
                        id: password
                        width: 240
                        secret: true
                        placeholder: "Contraseña"
                        onVisibleChanged: if (visible) edit()
                        onCommitted: text => {
                            if (!text)
                                return;
                            root.joining = network.modelData.ssid;
                            root.asking = "";
                            Host.join(network.modelData.ssid, text, () => root.joining = "");
                        }
                    }

                    Button {
                        text: "Cancelar"
                        onPressed: root.asking = ""
                    }
                }
            }
        }
    }

    Group {
        title: "Conexiones guardadas"
        visible: Host.connections.length > 0

        Repeater {
            model: Host.connections.filter(connection => !/wireless|wifi/.test(connection.type))

            Option {
                id: saved

                required property var modelData

                glyph: root.glyph(modelData.type)
                tone: modelData.active ? Tokens.text : Tokens.dim
                title: modelData.name
                text: [root.kind(modelData.type), modelData.active ? modelData.device || "Activa" : ""].filter(part => part).join(" · ")

                Toggle {
                    on: saved.modelData.active
                    onToggled: on => Host.connection(saved.modelData.uuid, on)
                }
            }
        }
    }
}
