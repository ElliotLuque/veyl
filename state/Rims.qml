pragma Singleton

import QtQuick
import Quickshell

// Lo que asoma de cada borde de la pantalla: las islas que salen de en medio
// de uno (base/Rim.qml) se apuntan aquí, para quien quiera seguir el contorno
// del marco con ellas, como el modo escuchar.
Singleton {
    id: root

    property list<QtObject> plates: []

    function add(plate: QtObject): void {
        plates = [...plates, plate];
    }

    function remove(plate: QtObject): void {
        plates = plates.filter(p => p !== plate);
    }

    // Las que asoman ahora del borde `edge` de `screen`, en orden a lo largo.
    function on(screen: ShellScreen, edge: string): var {
        return plates.filter(p => p.screen === screen && p.edge === edge && p.extent >= 1 && p.depth >= 1).sort((a, b) => a.at - b.at);
    }
}
