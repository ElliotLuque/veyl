pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io

// Sin red = sin ruta por defecto, ni IPv4 ni IPv6.
Singleton {
    id: root

    property bool offline: false

    Process {
        id: probe
        command: ["sh", "-c", `
            awk 'NR > 1 && $2 == "00000000" { found = 1 } END { exit !found }' /proc/net/route ||
            awk '$1 ~ /^0+$/ && $2 == "00" && $10 != "lo" { found = 1 } END { exit !found }' /proc/net/ipv6_route`]
        onExited: code => root.offline = code !== 0
    }

    Timer {
        interval: 5000
        running: true
        repeat: true
        triggeredOnStart: true
        onTriggered: probe.running = true
    }
}
