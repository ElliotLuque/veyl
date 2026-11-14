pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io

// Lo que Veyl recuerda entre arranques: el esquema de color y el fondo
// elegidos, la voz, el teclado, las pantallas… Se cambia en los ajustes
// (dials/), con bin/veyl, editando el archivo a mano o declarándolo en Nix
// (programs.veyl.settings; lo aplica y lo compara Nix.qml).
Singleton {
    id: root

    // Lo de fábrica: lo que hay mientras no se cambia (y con lo que se
    // compara lo declarado en Nix, en Nix.qml).
    readonly property var defaults: ({
            scheme: "living-code-night",
            wallpaper: "",                          // ruta absoluta; vacío = solo el color de fondo
            webSearch: "https://duckduckgo.com/?q=%s", // dónde busca la entrada lo que no encuentra
            terminal: "",                           // para las aplicaciones de terminal; vacío = $TERMINAL o kitty
            weekNumbers: false,                     // el almanaque, con los números de semana
            quiet: false,                           // no molestar
            journal: true,                          // veyld lleva el diario de lo que miras
            llm: "",                                // lo que contesta lo que la gramática no entiende (lo dicho, por la entrada estándar); vacío = nadie
            notificationDays: 7,                    // los días que veyld guarda las notificaciones; 0 = siempre
            voice: {},                              // la voz, para veyld: { model, mic, speak, speed, volume, voz, engine } (ver el README)
            brain: {},                              // el cerebro, para veyld: { provider, model, url } (ver el README)
            input: {},                              // teclado y ratón, para Hyprland: { kb_layout, kb_options, repeat_rate, repeat_delay, sensitivity, natural_scroll, touchpad: { natural_scroll } }; lo que falte lo pone su configuración
            monitors: {},                           // pantallas, para Hyprland: { <salida>: { mode, scale, position } }
            lockAfter: 0,                           // minutos sin tocar nada tras los que se bloquea; 0 = nunca
            calendarTarget: "",                     // dónde se apunta lo nuevo: el nombre de un calendario de Google; vacío = en Veyl
            eventWarning: 10                        // minutos antes de un evento en que la isla de la hora avisa; 0 = no avisa
        })

    property alias scheme: memory.scheme
    property alias wallpaper: memory.wallpaper
    property alias webSearch: memory.webSearch
    property alias terminal: memory.terminal
    property alias weekNumbers: memory.weekNumbers
    property alias quiet: memory.quiet
    property alias journal: memory.journal
    property alias llm: memory.llm
    property alias brain: memory.brain
    property alias notificationDays: memory.notificationDays
    property alias voice: memory.voice
    property alias input: memory.input
    property alias monitors: memory.monitors
    property alias lockAfter: memory.lockAfter
    property alias calendarTarget: memory.calendarTarget
    property alias eventWarning: memory.eventWarning

    FileView {
        path: `${Quickshell.env("XDG_STATE_HOME") || Quickshell.env("HOME") + "/.local/state"}/veyl/settings.json`
        blockLoading: true
        printErrors: false
        watchChanges: true
        onFileChanged: reload()
        onAdapterUpdated: writeAdapter()

        JsonAdapter {
            id: memory

            property string scheme: root.defaults.scheme
            property string wallpaper: root.defaults.wallpaper
            property string webSearch: root.defaults.webSearch
            property string terminal: root.defaults.terminal
            property bool weekNumbers: root.defaults.weekNumbers
            property bool quiet: root.defaults.quiet
            property bool journal: root.defaults.journal
            property string llm: root.defaults.llm
            property int notificationDays: root.defaults.notificationDays
            property var voice: ({})
            property var brain: ({})
            property var input: ({})
            property var monitors: ({})
            property int lockAfter: root.defaults.lockAfter
            property string calendarTarget: root.defaults.calendarTarget
            property int eventWarning: root.defaults.eventWarning
        }
    }
}
