import QtQuick
import QtQuick.Effects
import QtQuick.Shapes
import Quickshell
import Quickshell.Hyprland
import Quickshell.Wayland
import qs
import qs.state

// Al subir o bajar el volumen o el brillo, en el monitor con foco: una isla
// que sale del centro del borde derecho, en tira vertical, maciza y del color
// del fondo, con una curva hacia fuera donde se une a él. Arriba la cifra,
// en medio cuánto, en una barra ancha y de color, abajo de qué. Sale y se
// mete en el borde sin rebote: se cambia a menudo y no debe saltar. Lo que
// se mueve es la barra, llena de líquido: su superficie es lisa y, a cada
// cambio, se inclina hacia el lado al que va, se balancea y vuelve a nivel.
// Con el puntero encima se queda, y la rueda sigue subiendo o bajando.
Scope {
    id: root

    required property ShellScreen modelData
    readonly property HyprlandMonitor monitor: Hyprland.monitorFor(modelData)
    readonly property bool focused: monitor?.focused ?? false

    property string kind: "volume"  // volume | brightness
    property bool shown: false
    readonly property bool held: shown && hover.hovered
    readonly property bool sound: kind === "volume"
    readonly property bool muted: sound && Sound.muted
    readonly property int percent: sound ? Sound.volume : Brightness.percent
    readonly property string icon: sound ? (Sound.muted ? "speaker-x" : Sound.volume > 50 ? "speaker-high" : Sound.volume > 0 ? "speaker-low" : "speaker-none") : Brightness.percent > 50 ? "sun" : "sun-dim"

    function show(what: string, way: int): void {
        if (!focused)
            return;
        kind = what;
        shown = true;
        // Con la tecla mantenida no se acumula: un vaivén, no un columpio.
        slosh.velocity = Math.max(-8, Math.min(8, slosh.velocity + way * 5));
        rest.restart();
    }

    function nudge(way: int): void {
        if (sound)
            Sound.nudge(Sound.sink, way);
        else
            Brightness.nudge(way);
    }

    onHeldChanged: if (!held) rest.restart()

    Timer {
        id: rest
        interval: 1600
        onTriggered: if (!root.held) root.shown = false
    }

    // Lo que cambia la misma salida; elegir otra no es subir ni bajar.
    property var sink: null
    property int volume: -1
    property int bright: -1

    Connections {
        target: Sound

        function onVolumeChanged(): void {
            if (Sound.ready && Sound.sink === root.sink && root.volume >= 0)
                root.show("volume", Math.sign(Sound.volume - root.volume));
            root.sink = Sound.sink;
            root.volume = Sound.ready ? Sound.volume : -1;
        }

        function onMutedChanged(): void {
            if (Sound.ready && Sound.sink === root.sink)
                root.show("volume", 0);
        }

        function onSinkChanged(): void {
            root.sink = Sound.sink;
            root.volume = Sound.ready ? Sound.volume : -1;
        }
    }

    Connections {
        target: Brightness

        function onPercentChanged(): void {
            if (Brightness.present && root.bright >= 0 && Brightness.percent !== root.bright)
                root.show("brightness", Math.sign(Brightness.percent - root.bright));
            root.bright = Brightness.present && Brightness.raw > 0 ? Brightness.percent : -1;
        }
    }

    Component.onCompleted: {
        sink = Sound.sink;
        volume = Sound.ready ? Sound.volume : -1;
    }

    Spring {
        id: appear
        to: root.shown ? 1 : 0
        settle: Tokens.summon
    }

    Spring {
        id: fill
        to: Math.min(1, root.percent / 100)
        settle: Tokens.arrive
    }

    // La cifra que se ve: sigue a la de verdad con el mismo muelle que la barra.
    Spring {
        id: count
        to: root.percent
        settle: Tokens.arrive
    }

    // El vaivén del líquido: se inclina, va y viene unas veces y se calma.
    Spring {
        id: slosh
        settle: Tokens.arrive * 2
        bounce: 0.75  // el rebote va de lado: se balancea un par de veces
    }

    PanelWindow {
        screen: root.modelData
        visible: appear.value > 0.001 || root.shown
        // Solo el borde derecho: la capa queda centrada en él.
        anchors.right: true
        implicitWidth: plate.wide + Tokens.join * 2
        implicitHeight: plate.tall + (Tokens.join + Tokens.gap) * 2
        exclusionMode: ExclusionMode.Ignore
        color: "transparent"
        mask: Region {
            item: plate
        }
        // Por encima de todo: también en pantalla completa.
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "veyl-swell"

        Item {
            id: plate

            readonly property real wide: Tokens.island + 28
            readonly property real tall: 232

            // Lo que asoma del borde derecho, para quien siga el marco.
            readonly property Rim rim: Rim {
                screen: root.modelData
                edge: "right"
                at: (root.modelData.height - plate.parent.height) / 2 + plate.y
                extent: plate.height
                depth: plate.width * Math.max(0, appear.value)
                corner: shape.corner
                join: shape.join
            }

            width: wide
            height: tall
            anchors.verticalCenter: parent.verticalCenter
            // Sale del borde y vuelve a meterse en él.
            x: parent.width - width * Math.max(0, appear.value)

            Shape {
                id: shape

                readonly property real w: plate.width
                readonly property real h: plate.height
                readonly property real join: Tokens.join
                readonly property real corner: Math.min(w / 2, h / 2)
                // El trazo empieza y acaba fuera de la pantalla: solo se ve el canto libre.
                readonly property real out: -Tokens.line * 2

                preferredRendererType: Shape.CurveRenderer
                // Se dibuja contra el borde izquierdo y se voltea.
                transform: Scale {
                    origin.x: plate.width / 2
                    xScale: -1
                }

                ShapePath {
                    fillColor: Tokens.plate
                    strokeColor: Tokens.border
                    strokeWidth: Tokens.line
                    startX: shape.out
                    startY: -shape.join

                    PathLine { x: 0; y: -shape.join }
                    PathArc { x: shape.join; y: 0; radiusX: shape.join; radiusY: shape.join; direction: PathArc.Counterclockwise }
                    PathLine { x: shape.w - shape.corner; y: 0 }
                    PathArc { x: shape.w; y: shape.corner; radiusX: shape.corner; radiusY: shape.corner }
                    PathLine { x: shape.w; y: shape.h - shape.corner }
                    PathArc { x: shape.w - shape.corner; y: shape.h; radiusX: shape.corner; radiusY: shape.corner }
                    PathLine { x: shape.join; y: shape.h }
                    PathArc { x: 0; y: shape.h + shape.join; radiusX: shape.join; radiusY: shape.join; direction: PathArc.Counterclockwise }
                    PathLine { x: shape.out; y: shape.h + shape.join }
                    PathLine { x: shape.out; y: -shape.join }
                }
            }

            HoverHandler {
                id: hover
            }

            WheelHandler {
                target: null
                property real spin: 0

                onWheel: event => {
                    spin += event.angleDelta.y;
                    while (Math.abs(spin) >= 120) {
                        const way = Math.sign(spin);
                        spin -= way * 120;
                        root.nudge(way);
                    }
                }
            }

            Column {
                x: (plate.wide - width) / 2
                anchors.verticalCenter: parent.verticalCenter
                spacing: Tokens.gap + 2
                opacity: Math.min(1, Math.max(0, (appear.value - 0.3) * 2))

                Ink {
                    anchors.horizontalCenter: parent.horizontalCenter
                    // Cuenta de una en una hasta llegar, al paso de la barra.
                    text: root.muted ? "—" : Math.round(count.value)
                    dim: root.muted
                    font.pixelSize: Tokens.clockSize
                    font.weight: Font.DemiBold
                }

                Item {
                    id: track

                    // Un degradado de dos roles del esquema a lo largo de toda
                    // la barra: al llenarse se destapa, más vivo cuanto más alto.
                    readonly property color low: root.muted ? Tokens.dim : root.sound ? Tokens.accent : Tokens.anthropic
                    readonly property color high: root.muted ? Tokens.dim : root.sound ? Tokens.proposal : Tokens.attention
                    readonly property real reach: Math.min(1, Math.max(0, fill.value))
                    // La superficie: lleno del todo, la onda queda por encima.
                    readonly property real level: height + 2 - reach * (height + 6)
                    // Cuánto se inclina la superficie: alto de un lado menos el del otro.
                    readonly property real tilt: Math.max(-0.25, Math.min(0.25, slosh.value * 0.2)) * width / 2

                    anchors.horizontalCenter: parent.horizontalCenter
                    width: 22
                    height: plate.tall - 88

                    Rectangle {
                        anchors.fill: parent
                        radius: width / 2
                        color: Tokens.surface
                    }

                    // El líquido se dibuja suelto y se recorta con la forma de la barra.
                    Item {
                        id: liquid
                        anchors.fill: parent
                        visible: false
                        layer.enabled: true

                        Shape {
                            anchors.fill: parent
                            preferredRendererType: Shape.CurveRenderer

                            ShapePath {
                                strokeColor: "transparent"
                                fillGradient: LinearGradient {
                                    x1: 0; y1: track.height
                                    x2: 0; y2: 0
                                    GradientStop { position: 0; color: track.low }
                                    GradientStop { position: 1; color: track.high }
                                }

                                startX: 0
                                startY: track.level + track.tilt

                                PathLine { x: track.width; y: track.level - track.tilt }
                                PathLine { x: track.width; y: track.height + 2 }
                                PathLine { x: 0; y: track.height + 2 }
                            }
                        }
                    }

                    Rectangle {
                        id: pill
                        anchors.fill: parent
                        radius: width / 2
                        visible: false
                        layer.enabled: true
                    }

                    MultiEffect {
                        anchors.fill: parent
                        source: liquid
                        maskEnabled: true
                        maskSource: pill
                        visible: track.reach > 0
                    }
                }

                Icon {
                    anchors.horizontalCenter: parent.horizontalCenter
                    name: root.icon
                    color: root.muted ? Tokens.critical : Tokens.text

                    TapHandler {
                        enabled: root.sound
                        onTapped: Sound.mute(Sound.sink)
                    }
                }
            }
        }
    }
}
