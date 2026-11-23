import QtQuick
import Quickshell
import Quickshell.Hyprland
import Quickshell.Wayland
import qs
import qs.state

// El estado base en un monitor: dos islas que salen de las esquinas de
// arriba, pegadas a los bordes de la pantalla. Sin barra.
Scope {
    id: root

    required property ShellScreen modelData
    readonly property HyprlandMonitor monitor: Hyprland.monitorFor(modelData)
    readonly property HyprlandWorkspace workspace: monitor?.activeWorkspace ?? null
    readonly property var levels: Contexts.levels(monitor)
    // Contexto y agentes solo en el monitor con foco; la hora, en todos.
    readonly property bool focused: monitor?.focused ?? false
    readonly property bool fullscreen: Contexts.fullscreen(monitor)
    // Bajo una isla, sitio para la curva con que se une al borde y un rebote.
    readonly property int below: Tokens.join + Tokens.gap

    // La isla de la hora se abre en el almanaque con el puntero encima: tras
    // un instante, para que cruzarla de paso no la abra, y se cierra con algo
    // de gracia al salir. Mientras se escribe en ella no se cierra.
    property bool almanac: false
    readonly property bool wanted: clock.hovered || calendar.typing
    onWantedChanged: {
        if (wanted) {
            grace.stop();
            if (!almanac)
                intent.restart();
        } else {
            intent.stop();
            if (almanac)
                grace.restart();
        }
    }
    onAlmanacChanged: if (almanac) calendar.reset()

    // La isla del contexto, igual: con el puntero encima se abre en la torre,
    // para cambiar de nivel solo con el ratón.
    property bool tower: false
    property real wheel: 0  // giro acumulado de la rueda, hasta un paso
    onTowerWantedChanged: {
        if (towerWanted) {
            towerGrace.stop();
            if (!root.tower)
                towerIntent.restart();
        } else {
            towerIntent.stop();
            if (root.tower)
                towerGrace.restart();
        }
    }
    readonly property bool towerWanted: context.hovered

    Timer {
        id: towerIntent
        interval: 60
        onTriggered: root.tower = true
    }

    Timer {
        id: towerGrace
        interval: 180
        onTriggered: root.tower = false
    }

    Timer {
        id: intent
        interval: 60
        onTriggered: root.almanac = true
    }

    Timer {
        id: grace
        interval: 180
        onTriggered: root.almanac = false
    }

    // Al cambiar de nivel, la isla del contexto acompaña el movimiento.
    property int level: 0
    onWorkspaceChanged: {
        const id = workspace?.id ?? 0;
        if (level > 0 && id > 0 && id !== level)
            context.nudge(id > level ? 1 : -1);
        level = id;
    }

    PanelWindow {
        screen: root.modelData
        anchors {
            top: true
            left: true
            right: true
        }
        // Hasta donde llega la isla del contexto abierta, rebote incluido.
        // (y la de un agente que pregunta, que crece hacia abajo).
        implicitHeight: Math.max(Tokens.island, tower.implicitHeight, agents.implicitHeight) + root.below
        // Se reserva el alto de una isla; las ventanas ya dejan `gap` debajo.
        exclusiveZone: Tokens.gapTop - Tokens.gap
        color: "transparent"
        // Solo la isla del contexto y los agentes reciben el puntero; lo
        // demás pasa a las ventanas.
        mask: Region {
            regions: [...(root.focused ? [lead.area] : []), ...(agents.shown ? agents.areas : [])]
        }

        // Sobre las ventanas, para poder abrirse encima de ellas; la pantalla
        // completa lo tapa igualmente.
        WlrLayershell.layer: WlrLayer.Top
        WlrLayershell.namespace: "veyl-base"
        // El teclado, solo con el puntero sobre un agente que pregunta: sus
        // teclas contestan. Al apartarlo, vuelve a la ventana de antes.
        WlrLayershell.keyboardFocus: agents.listening ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None

        // Superior izquierda: en qué nivel estás y qué hay en él. Vistazo: se
        // abre y enseña todos los niveles.
        Reveal {
            id: lead
            shown: root.focused

            // Lo que de ella recibe el puntero. Va aquí, junto a la isla, como
            // en los agentes: suelto en la ventana o en `root` no seguía a la
            // isla y la máscara se quedaba sin ella.
            readonly property Region area: Region {
                item: context
            }

            Island {
                id: context

                expanded: Glance.open || root.tower
                detail: Tower {
                    id: tower
                    monitor: root.monitor
                    levels: root.levels
                }

                // La rueda sube o baja de nivel.
                WheelHandler {
                    parent: context
                    acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
                    onWheel: event => {
                        root.wheel += event.angleDelta.y;
                        if (Math.abs(root.wheel) >= 120) {
                            Contexts.step(root.monitor, root.wheel > 0 ? -1 : 1);
                            root.wheel = 0;
                        }
                    }
                }

                Gauge {
                    anchors.verticalCenter: parent.verticalCenter
                    levels: root.levels
                }

                Item {
                    width: Tokens.pad - 6
                    height: 1
                }

                Roll {
                    text: Contexts.label(root.workspace)
                    order: root.workspace?.id ?? 0
                    strong: true
                }

                Reveal {
                    anchors.verticalCenter: parent.verticalCenter
                    shown: strip.tiles.length > 0
                    gap: Tokens.pad - 2
                    leading: true

                    Strip {
                        id: strip
                        workspace: root.workspace
                        monitor: root.monitor
                    }
                }

            }
        }

        // Tras ella, colgando del borde de arriba, los agentes en marcha; si
        // se abre, se corren con ella.
        Ribbons {
            id: agents
            x: lead.width + Tokens.join + Tokens.gap / 2
            shown: root.focused && !root.fullscreen
        }
    }

    // Superior derecha, en su propia capa: alta como la pantalla y de
    // tamaño fijo, para que el almanaque crezca dentro sin que la capa
    // cambie nunca de tamaño (Hyprland enseñaría un fotograma estirado).
    PanelWindow {
        screen: root.modelData
        anchors {
            top: true
            right: true
        }
        implicitWidth: root.modelData.width / 2
        implicitHeight: root.modelData.height
        // Pegada al borde de arriba, no bajo el hueco que reserva la base.
        exclusionMode: ExclusionMode.Ignore
        color: "transparent"
        // Solo la isla recibe el puntero; lo demás lo deja pasar.
        mask: Region {
            item: clock
        }
        WlrLayershell.layer: WlrLayer.Top
        WlrLayershell.namespace: "veyl-clock"
        // El teclado, solo si se pulsa el almanaque para escribir en él.
        WlrLayershell.keyboardFocus: root.almanac ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None

        // Superior derecha: la hora y, delante, las excepciones. Con el
        // puntero encima, el almanaque; al acercarse un evento, unos
        // segundos, su anuncio.
        Island {
            id: clock

            // El anuncio, solo si no está abierto el almanaque (que ya lo dice)
            // ni hay pantalla completa (ahí va la esquina).
            readonly property bool heralding: !!Agenda.herald && !root.almanac && !root.fullscreen

            anchors.right: parent.right
            mirrored: true
            expanded: root.almanac || heralding
            keep: true
            // Lo llamas tú, como la entrada: llega ya y sin apenas rebote. Lo
            // que llega solo, con algo más de calma.
            settle: root.almanac ? Tokens.summon : Tokens.veil
            bounce: root.almanac ? 0.2 : 0.3
            detail: Item {
                id: face

                // Lo que enseña cerrándose es lo último que enseñó abierta.
                property bool almanac: true
                readonly property Item shown: almanac ? calendar : herald

                width: shown.width
                height: shown.height

                Connections {
                    target: clock

                    function onHeraldingChanged(): void {
                        if (clock.heralding)
                            face.almanac = false;
                    }
                }

                Connections {
                    target: root

                    function onAlmanacChanged(): void {
                        if (root.almanac)
                            face.almanac = true;
                    }
                }

                Almanac {
                    id: calendar
                    room: clock.rest
                    visible: face.almanac
                }

                Herald {
                    id: herald
                    event: Agenda.herald
                    room: clock.rest
                    visible: !face.almanac
                }
            }

            Exceptions {}

            // Vistazo: volumen y batería exacta
            Reveal {
                anchors.verticalCenter: parent.verticalCenter
                shown: Glance.open && Sound.ready
                settle: Tokens.veil
                gap: Tokens.pad

                Label {
                    icon: Sound.muted ? "speaker-x" : Sound.volume > 50 ? "speaker-high" : Sound.volume > 0 ? "speaker-low" : "speaker-none"
                    text: Sound.volume
                    dim: true
                }
            }

            Reveal {
                anchors.verticalCenter: parent.verticalCenter
                shown: Glance.open && Battery.present && !Battery.low
                settle: Tokens.veil
                gap: Tokens.pad

                Label {
                    icon: !Battery.discharging ? "battery-charging" : Battery.percent > 80 ? "battery-full" : Battery.percent > 50 ? "battery-high" : "battery-medium"
                    text: `${Battery.percent} %`
                    dim: true
                }
            }

            // Vistazo: fecha; con el almanaque abierto ya la dice él.
            Reveal {
                anchors.verticalCenter: parent.verticalCenter
                shown: Glance.open && !root.almanac
                settle: Tokens.veil
                gap: Tokens.gap

                Ink {
                    text: Clock.now.toLocaleDateString(Qt.locale(), "ddd d MMM")
                    dim: true
                }
            }

            Hour {
                anchors.verticalCenter: parent.verticalCenter
            }
        }
    }

    // Modo escuchar: el borde alrededor de la pantalla, siguiendo a las
    // islas. En pantalla completa solo se ve la de la esquina.
    Listening {
        modelData: root.modelData
        left: root.fullscreen ? null : { w: lead.width, h: context.height, corner: context.corner }
        right: root.fullscreen
            ? (corner.progress > 0 ? { w: corner.width, h: edge.height, corner: edge.corner } : null)
            : { w: clock.width, h: clock.height, corner: clock.corner }
        hanging: root.fullscreen ? [] : agents.plates().map(p => Object.assign(p, { x: p.x + agents.x, join: Tokens.gap }))
    }

    // En pantalla completa solo queda, en una esquina y por encima de la
    // ventana, una isla con las excepciones y el punto de «te necesita».
    PanelWindow {
        screen: root.modelData
        visible: corner.progress > 0
        anchors {
            top: true
            right: true
        }
        implicitWidth: 640
        implicitHeight: Tokens.island + root.below
        exclusionMode: ExclusionMode.Ignore
        color: "transparent"
        mask: Region {}
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "veyl-corner"

        Reveal {
            id: corner

            readonly property bool needed: root.focused && Agents.needing > 0

            anchors.right: parent.right
            shown: root.fullscreen && (alerts.any || needed)

            Island {
                id: edge
                mirrored: true

                Exceptions {
                    id: alerts
                    tail: needs.progress
                }

                Reveal {
                    id: needs
                    anchors.verticalCenter: parent.verticalCenter
                    shown: corner.needed

                    Dot {
                        phase: "needs"
                    }
                }
            }
        }
    }
}
