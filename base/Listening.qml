import QtQuick
import QtQuick.Effects
import QtQuick.Shapes
import Quickshell
import Quickshell.Wayland
import qs
import qs.state

// Modo escuchar en un monitor: la pantalla, algo oscurecida, y alrededor un
// borde del color del acento con un halo hacia dentro que late con la voz.
// El borde no es un rectángulo: las islas son parte del marco, así que las
// rodea, con sus curvas, y ni el oscurecido ni el halo las tapan. Entra y
// sale con un fundido; no recibe el puntero.
PanelWindow {
    id: root

    required property ShellScreen modelData
    // Las islas de las esquinas de arriba, en píxeles de la pantalla:
    // { w, h, corner }, y las que cuelgan entre ellas, { x, w, h, corner,
    // join }. Las que salen de en medio de un borde se apuntan solas en Rims.
    property var left: null
    property var right: null
    property var hanging: []

    // El contorno, por dentro del borde de la pantalla a medio trazo y
    // alrededor de cada isla: en sus esquinas libres, su radio más medio
    // trazo; en las curvas con que se unen al borde, menos.
    function outline(): string {
        const r = Tokens.ring / 2;
        const j = Tokens.join;
        const W = width, H = height;
        const n = v => Math.round(v * 100) / 100;
        const out = [];
        const move = (x, y) => out.push(`M ${n(x)} ${n(y)}`);
        const line = (x, y) => out.push(`L ${n(x)} ${n(y)}`);
        // Con las agujas del reloj o al revés, a la vista.
        const arc = (radius, x, y, cw) => out.push(`A ${n(radius)} ${n(radius)} 0 0 ${cw ? 1 : 0} ${n(x)} ${n(y)}`);
        const real = p => p && p.w >= 1 && p.h >= 1;

        move(r, H - r);
        const L = left;
        if (real(L)) {
            const c = Math.min(L.corner, L.w, L.h);
            line(r, L.h + j);
            arc(j - r, j, L.h + r, true);
            line(L.w - c, L.h + r);
            arc(c + r, L.w + r, L.h - c, false);
            line(L.w + r, j);
            arc(j - r, L.w + j, r, true);
        } else {
            line(r, r);
        }
        const above = hanging.concat(Rims.on(modelData, "top").map(p => ({ x: p.at, w: p.extent, h: p.depth, corner: p.corner, join: p.join })));
        above.sort((a, b) => a.x - b.x);
        for (const p of above) {
            if (!real(p))
                continue;
            const k = p.join, c = Math.min(p.corner, p.w / 2, p.h);
            const x1 = p.x + p.w;
            line(p.x - k, r);
            arc(k - r, p.x - r, k, true);
            line(p.x - r, p.h - c);
            arc(c + r, p.x + c, p.h + r, false);
            line(x1 - c, p.h + r);
            arc(c + r, x1 + r, p.h - c, false);
            line(x1 + r, k);
            arc(k - r, x1 + k, r, true);
        }
        const R = right;
        if (real(R)) {
            const c = Math.min(R.corner, R.w, R.h);
            const x0 = W - R.w;
            line(x0 - j, r);
            arc(j - r, x0 - r, j, true);
            line(x0 - r, R.h - c);
            arc(c + r, x0 + c, R.h + r, false);
            line(W - j, R.h + r);
            arc(j - r, W - r, R.h + j, true);
        } else {
            line(W - r, r);
        }
        for (const p of Rims.on(modelData, "right")) {
            const k = p.join, d = p.depth, c = Math.min(p.corner, d, p.extent / 2);
            const y1 = p.at + p.extent;
            line(W - r, p.at - k);
            arc(k - r, W - k, p.at - r, true);
            line(W - d + c, p.at - r);
            arc(c + r, W - d - r, p.at + c, false);
            line(W - d - r, y1 - c);
            arc(c + r, W - d + c, y1 + r, false);
            line(W - k, y1 + r);
            arc(k - r, W - r, y1 + k, true);
        }
        line(W - r, H - r);
        out.push("Z");
        return out.join(" ");
    }

    screen: modelData
    visible: frame.opacity > 0
    anchors {
        top: true
        bottom: true
        left: true
        right: true
    }
    exclusionMode: ExclusionMode.Ignore
    color: "transparent"
    mask: Region {}
    // Por encima de todo, también de la pantalla completa.
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "veyl-listen"

    Item {
        id: frame

        readonly property color halo: Qt.alpha(Tokens.accent, 0.5)
        // Solo se sigue a las islas mientras se ve.
        readonly property string path: root.visible ? root.outline() : ""

        // Lo que se oye, suavizado entre ticks del micro.
        property real voice: Listen.level

        anchors.fill: parent
        opacity: Listen.active ? 1 : 0

        Behavior on opacity {
            NumberAnimation { duration: Tokens.veil; easing.type: Easing.InOutSine }
        }

        // Sigue a la voz sin saltos: como mucho, todo el recorrido en `tick`.
        Behavior on voice {
            SmoothedAnimation { duration: Tokens.tick; velocity: -1 }
        }

        // Oscurece la pantalla, no las islas, para que el borde destaque.
        Shape {
            anchors.fill: parent
            preferredRendererType: Shape.CurveRenderer

            ShapePath {
                fillColor: Tokens.scrim
                strokeColor: "transparent"
                PathSvg { path: frame.path }
            }
        }

        // Lo de dentro del contorno: el halo no se sale sobre las islas.
        Shape {
            id: inside
            anchors.fill: parent
            visible: false
            layer.enabled: true
            preferredRendererType: Shape.CurveRenderer

            ShapePath {
                fillColor: "white"
                strokeColor: "transparent"
                PathSvg { path: frame.path }
            }
        }

        // El halo: el mismo borde, más ancho y desenfocado. En silencio,
        // tenue; con la voz se enciende y entra más hacia el centro. El trazo
        // va centrado en el contorno y solo cuenta su mitad de dentro.
        Item {
            anchors.fill: parent
            opacity: 0.6 + 0.4 * frame.voice
            layer.enabled: true
            layer.effect: MultiEffect {
                maskEnabled: true
                maskSource: inside
            }

            Shape {
                anchors.fill: parent
                preferredRendererType: Shape.CurveRenderer
                layer.enabled: true
                layer.effect: MultiEffect {
                    autoPaddingEnabled: false
                    blurEnabled: true
                    blur: 1
                    blurMax: 64
                }

                ShapePath {
                    fillColor: "transparent"
                    strokeColor: frame.halo
                    strokeWidth: 2 * (Tokens.glow + (Tokens.shout - Tokens.glow) * frame.voice)
                    joinStyle: ShapePath.RoundJoin
                    PathSvg { path: frame.path }
                }
            }
        }

        // El borde
        Shape {
            anchors.fill: parent
            preferredRendererType: Shape.CurveRenderer

            ShapePath {
                fillColor: "transparent"
                strokeColor: Tokens.accent
                strokeWidth: Tokens.ring
                joinStyle: ShapePath.RoundJoin
                PathSvg { path: frame.path }
            }
        }
    }
}
