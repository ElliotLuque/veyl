import QtQuick
import Quickshell
import qs
import qs.state

// Los agentes en marcha, cada uno en su isla, colgando del borde de arriba
// en fila tras la de los niveles. Solo están los que hacen algo o te
// necesitan; uno que acaba se queda un momento a decirlo y se va.
Item {
    id: root

    property bool shown: true
    property list<Region> areas: []

    function collect(): void {
        const out = [];
        for (let i = 0; i < islands.count; i++) {
            const area = islands.itemAt(i)?.area;
            if (area)
                out.push(area);
        }
        areas = out;
    }

    // Lo que asoma de cada isla, para quien siga su contorno (el modo
    // escuchar): { x, w, h, corner }, desde el borde de arriba.
    function plates(): var {
        const out = [];
        if (!visible)
            return out;
        for (let i = 0; i < islands.count; i++) {
            const slot = islands.itemAt(i);
            const island = slot?.plate;
            if (!slot || !island || !slot.visible)
                continue;
            const h = island.y + island.height;
            if (h <= 0)
                continue;
            out.push({
                x: slot.x + island.x,
                w: island.width,
                h,
                corner: Math.min(h, island.height / 2, island.width / 2)
            });
        }
        return out;
    }

    implicitWidth: row.width
    implicitHeight: Tokens.pod + Tokens.gap
    opacity: show.value
    visible: opacity > 0 && Agents.list.count > 0

    Spring {
        id: show
        to: root.shown ? 1 : 0
    }

    Row {
        id: row

        Repeater {
            id: islands

            model: Agents.list
            onItemAdded: Qt.callLater(root.collect)
            onItemRemoved: Qt.callLater(root.collect)

            // Entra bajando del borde y se va subiendo a él; las de la
            // derecha se corren a ocupar su hueco.
            Item {
                id: slot

                required property string agent
                required property string phase
                required property string provider
                required property string name
                required property string window
                readonly property Item plate: island
                readonly property Region area: Region {
                    item: island
                }
                readonly property bool wanted: born && (phase === "working" || phase === "needs" || island.fresh)
                property bool born: false

                Component.onCompleted: Qt.callLater(() => born = true)

                // Entre una y otra, sitio para sus dos curvas y un poco de aire.
                width: (island.width + Tokens.gap * 2 + Tokens.gap / 2) * appear.value
                height: island.height
                visible: appear.value > 0

                Spring {
                    id: appear
                    to: slot.wanted ? 1 : 0
                    settle: Tokens.veil
                }

                Ribbon {
                    id: island

                    x: Tokens.gap
                    y: -(height + Tokens.gap) * (1 - appear.value)
                    agent: slot.agent
                    phase: slot.phase
                    provider: slot.provider
                    name: slot.name
                    window: slot.window
                }
            }
        }
    }
}
