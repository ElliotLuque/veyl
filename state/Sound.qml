pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Services.Pipewire

// El sonido: la salida y la entrada por defecto, su volumen, y los
// dispositivos entre los que se puede elegir.
Singleton {
    id: root

    readonly property PwNode sink: Pipewire.defaultAudioSink
    readonly property bool ready: !!sink?.audio
    readonly property bool muted: sink?.audio?.muted ?? false
    readonly property int volume: Math.round((sink?.audio?.volume ?? 0) * 100)

    // El micro
    readonly property PwNode source: Pipewire.defaultAudioSource
    readonly property bool micReady: !!source?.audio
    readonly property bool micMuted: source?.audio?.muted ?? false
    readonly property int micVolume: Math.round((source?.audio?.volume ?? 0) * 100)

    readonly property list<PwNode> devices: Pipewire.nodes.values.filter(node => !node.isStream && node.audio)
    readonly property list<PwNode> sinks: devices.filter(node => node.isSink)
    readonly property list<PwNode> sources: devices.filter(node => !node.isSink)
    readonly property int step: 5  // puntos que sube o baja de una vez

    function label(node: PwNode): string {
        return node?.description || node?.nickname || node?.name || "";
    }

    // Sube o baja a saltos de `step`; subir quita el silencio.
    function nudge(node: PwNode, way: int): void {
        if (!node?.audio)
            return;
        const now = Math.round(node.audio.volume * 100);
        const next = way > 0 ? Math.floor(now / step) * step + step : Math.ceil(now / step) * step - step;
        node.audio.volume = Math.max(0, Math.min(100, next)) / 100;
        if (way > 0)
            node.audio.muted = false;
    }

    function mute(node: PwNode): void {
        if (node?.audio)
            node.audio.muted = !node.audio.muted;
    }

    // Lo pone por defecto: lo que suena, o lo que se graba, pasa a él.
    function use(node: PwNode): void {
        if (node.isSink)
            Pipewire.preferredDefaultAudioSink = node;
        else
            Pipewire.preferredDefaultAudioSource = node;
    }

    PwObjectTracker {
        objects: root.devices
    }
}
