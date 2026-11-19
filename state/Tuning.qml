pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Hyprland
import Quickshell.Io
import Quickshell.Wayland
import qs

// Los ajustes: la isla grande que nace del centro del borde de arriba para
// cambiar Veyl y el sistema. Aquí, si está abierta, en qué monitor y en qué
// sección, y cómo se guarda lo que se cambia; lo que se ve, en dials/; lo
// que se pregunta al sistema, en Host.
Singleton {
    id: root

    property bool open: false
    property string monitor: ""  // en qué monitor se abrió
    property string section: "look"
    // Con qué tamaño nace: el del centro de control si viene de él, para que
    // sea esa misma isla la que crece.
    property size origin: Qt.size(0, 0)

    // Las secciones, de arriba abajo, como los niveles. Las que dependen de
    // algo que no hay (un adaptador Bluetooth) no salen.
    readonly property var sections: [
        { id: "look", title: "Apariencia", glyph: "palette" },
        { id: "sound", title: "Sonido", glyph: "speaker-high" },
        { id: "screens", title: "Pantallas", glyph: "monitor" },
        { id: "network", title: "Red", glyph: "globe" },
        { id: "bluetooth", title: "Bluetooth", glyph: "bluetooth", when: !!Commands.bluetooth },
        { id: "input", title: "Teclado y ratón", glyph: "keyboard" },
        { id: "alerts", title: "Notificaciones", glyph: "bell" },
        { id: "calendars", title: "Calendarios", glyph: "calendar" },
        { id: "voice", title: "Voz", glyph: "waveform" },
        { id: "journal", title: "Diario y órdenes", glyph: "notebook" },
        { id: "session", title: "Sesión y energía", glyph: "power" },
        { id: "nix", title: "Nix", glyph: "snowflake", when: Nix.active },
        { id: "about", title: "Sistema", glyph: "info" }
    ].filter(section => section.when ?? true)
    readonly property var current: sections.find(section => section.id === root.section) ?? sections[0]

    // Su ancho en un monitor de este ancho: lo más grande de Veyl, sin
    // pasar de lo que cabe.
    function span(room: real): real {
        return Math.min(Tokens.dials, room - (Tokens.join + Tokens.gap) * 2 - 80);
    }

    function show(monitor: string, section: string): void {
        if (Launcher.open)
            Launcher.hide();
        if (Recall.open)
            Recall.hide();
        Glance.release();
        root.monitor = monitor || Hyprland.focusedMonitor?.name || "";
        if (section && sections.some(other => other.id === section))
            root.section = section;
        open = true;
    }

    function hide(): void {
        open = false;
        origin = Qt.size(0, 0);
    }

    function toggle(section: string): void {
        if (open && (!section || section === root.section))
            hide();
        else
            show("", section);
    }

    // Una sección más arriba (-1) o más abajo (1), como cambiar de nivel.
    function step(way: int): void {
        const at = sections.indexOf(current);
        const next = sections[Math.max(0, Math.min(sections.length - 1, at + way))];
        if (next)
            section = next.id;
    }

    // Lo anidado de settings.json se cambia entero: así se guarda. Un valor
    // vacío (null, "") quita la clave y vuelve a lo de fábrica.
    function nest(object: var, key: string, value: var): var {
        const next = Object.assign({}, object ?? {});
        if (value === null || value === undefined || value === "")
            delete next[key];
        else
            next[key] = value;
        return next;
    }

    function voice(key: string, value: var): void {
        Settings.voice = nest(Settings.voice, key, value);
    }

    function input(key: string, value: var): void {
        Settings.input = nest(Settings.input, key, value);
    }

    // Bloquear tras un rato sin tocar nada (Settings.lockAfter, en minutos).
    // Lo que pide no dormirse (un vídeo a pantalla completa) lo aplaza.
    IdleMonitor {
        enabled: Settings.lockAfter > 0
        timeout: Settings.lockAfter * 60
        respectInhibitors: true
        onIsIdleChanged: if (isIdle && !Lock.locked) Lock.lock()
    }

    GlobalShortcut {
        appid: "veyl"
        name: "settings"
        description: "Abrir o cerrar los ajustes"
        onPressed: root.toggle("")
    }

    IpcHandler {
        target: "settings"

        function toggle(): void {
            root.toggle("");
        }

        function close(): void {
            root.hide();
        }

        // Abre una sección: look, sound, screens, network, bluetooth, input,
        // alerts, calendars, voice, journal, session, nix (en NixOS o con Veyl
        // declarado en Nix) o about.
        function open(section: string): string {
            if (section && !root.sections.some(other => other.id === section))
                return `no existe la sección ${section}`;
            root.show("", section);
            return "";
        }

        function get(): bool {
            return root.open;
        }

        function section(): string {
            return root.section;
        }
    }
}
