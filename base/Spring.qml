import QtQuick
import qs

// Muelle entre 0 y 1. Interrumpible: si `to` cambia a mitad de camino sigue
// desde donde está y conserva la velocidad. Sin rebote llega y se queda; con
// él se pasa un poco y vuelve.
QtObject {
    id: root

    property real to: 0
    property real settle: Tokens.fast  // ms hasta asentarse
    property real bounce: 0            // 0 = amortiguación crítica
    property real value: 0
    property real velocity: 0
    property bool ready: false

    Component.onCompleted: {
        value = to;
        ready = true;
    }

    property FrameAnimation driver: FrameAnimation {
        running: root.ready && (root.value !== root.to || root.velocity !== 0)
        onTriggered: {
            // Solución exacta del muelle: estable con cualquier paso.
            const dt = Math.min(frameTime, 0.05);
            const w = 6600 / root.settle;
            const x = root.value - root.to;
            const v = root.velocity;
            let nx, nv;
            if (root.bounce > 0) {
                const a = (1 - root.bounce) * w;
                const b = Math.sqrt(w * w - a * a);
                const c = (v + a * x) / b;
                const e = Math.exp(-a * dt);
                const cos = Math.cos(b * dt);
                const sin = Math.sin(b * dt);
                nx = (x * cos + c * sin) * e;
                nv = (v * cos - (a * c + b * x) * sin) * e;
            } else {
                const c = v + w * x;
                const e = Math.exp(-w * dt);
                nx = (x + c * dt) * e;
                nv = (v - w * c * dt) * e;
            }
            const rest = Math.abs(nx) < 0.001 && Math.abs(nv) < 0.01;
            root.value = rest ? root.to : root.to + nx;
            root.velocity = rest ? 0 : nv;
        }
    }
}
