import QtQuick
import Quickshell.Bluetooth
import qs
import qs.base
import qs.state

// Bluetooth: el adaptador, lo emparejado (conectar, desconectar, olvidar) y,
// mientras busca, lo que hay cerca para emparejar.
Column {
    id: root

    readonly property BluetoothAdapter adapter: Commands.bluetooth
    readonly property var all: adapter?.devices?.values ?? []
    readonly property var paired: all.filter(device => device.paired || device.bonded).sort((a, b) => b.connected - a.connected || a.name.localeCompare(b.name))
    readonly property var nearby: all.filter(device => !device.paired && !device.bonded && device.name)
    property string armed: ""  // el que espera una segunda pulsación para olvidarse

    spacing: 22

    // Al irse, que no se quede buscando.
    Component.onDestruction: if (adapter?.discovering) adapter.discovering = false

    function glyph(icon: string): string {
        return /headset|headphone|audio-card/.test(icon) ? "headphones" : /mouse/.test(icon) ? "mouse" : /keyboard/.test(icon) ? "keyboard" : /phone/.test(icon) ? "device-mobile" : /gaming|joystick/.test(icon) ? "game-controller" : /speaker|audio/.test(icon) ? "speaker-hifi" : /watch/.test(icon) ? "watch" : /printer/.test(icon) ? "printer" : "bluetooth";
    }

    Timer {
        id: disarm
        interval: 3000
        onTriggered: root.armed = ""
    }

    Group {
        title: "Adaptador"

        Option {
            glyph: root.adapter?.enabled ? "bluetooth" : "bluetooth-slash"
            title: root.adapter?.name || "Bluetooth"
            text: root.adapter?.enabled ? (root.adapter.discovering ? "Buscando dispositivos…" : `${root.paired.filter(device => device.connected).length} conectados`) : "Desactivado"

            Button {
                icon: "magnifying-glass"
                text: root.adapter?.discovering ? "Detener" : "Buscar"
                visible: root.adapter?.enabled ?? false
                onPressed: root.adapter.discovering = !root.adapter.discovering
            }

            Toggle {
                on: root.adapter?.enabled ?? false
                onToggled: on => root.adapter.enabled = on
            }
        }
    }

    Group {
        title: "Dispositivos emparejados"
        visible: root.adapter?.enabled ?? false
        note: root.paired.length === 0 ? "No hay dispositivos emparejados." : ""

        Repeater {
            model: root.paired

            Option {
                id: row

                required property BluetoothDevice modelData
                readonly property bool busy: modelData.state === BluetoothDeviceState.Connecting || modelData.state === BluetoothDeviceState.Disconnecting

                glyph: root.glyph(modelData.icon)
                tone: modelData.connected ? Tokens.accent : Tokens.dim
                title: modelData.name
                text: [modelData.state === BluetoothDeviceState.Connecting ? "Conectando…" : modelData.state === BluetoothDeviceState.Disconnecting ? "Desconectando…" : modelData.connected ? "Conectado" : "Desconectado", modelData.batteryAvailable ? `Batería ${Math.round(modelData.battery * 100)} %` : ""].filter(part => part).join(" · ")

                Button {
                    text: row.modelData.connected ? "Desconectar" : "Conectar"
                    strong: !row.modelData.connected
                    visible: !row.busy
                    onPressed: row.modelData.connected ? row.modelData.disconnect() : row.modelData.connect()
                }

                // Olvidar pide confirmarlo.
                Button {
                    icon: "trash"
                    text: root.armed === row.modelData.address ? "Olvidar" : ""
                    tone: root.armed === row.modelData.address ? Tokens.attention : "transparent"
                    onPressed: {
                        if (root.armed !== row.modelData.address) {
                            root.armed = row.modelData.address;
                            disarm.restart();
                            return;
                        }
                        root.armed = "";
                        row.modelData.forget();
                    }
                }
            }
        }
    }

    Group {
        title: "Dispositivos disponibles"
        visible: (root.adapter?.discovering ?? false) || root.nearby.length > 0
        note: root.nearby.length === 0 ? "Buscando dispositivos…" : ""

        Repeater {
            model: root.nearby

            Option {
                id: near

                required property BluetoothDevice modelData

                glyph: root.glyph(modelData.icon)
                title: modelData.name
                text: modelData.pairing ? "Emparejando…" : modelData.address

                Button {
                    text: "Emparejar"
                    visible: !near.modelData.pairing
                    onPressed: {
                        near.modelData.trusted = true;
                        near.modelData.pair();
                    }
                }
            }
        }
    }
}
