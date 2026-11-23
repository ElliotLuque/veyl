// Las islas son capas sin marco: que Qt no prepare decoraciones de ventana.
//@ pragma Env QT_WAYLAND_DISABLE_WINDOWDECORATION=1
import QtQuick
import Quickshell
import qs.base
import qs.dials
import qs.state

// Veyl · estado base: lo que se ve cuando no pasa nada.
ShellRoot {
    Component.onCompleted: Compositor.start()

    Variants {
        model: Quickshell.screens

        Backdrop {}
    }

    Variants {
        model: Quickshell.screens

        Base {}
    }

    // El velo: lo que llega, en el centro.
    Variants {
        model: Quickshell.screens

        Veil {}
    }

    // Volumen y brillo: al cambiar, en el borde derecho.
    Variants {
        model: Quickshell.screens

        Swell {}
    }

    // El destello al capturar la pantalla entera.
    Variants {
        model: Quickshell.screens

        Flash {}
    }

    // La entrada: una sola, en el monitor desde el que se llama.
    Palette {}

    // Los ajustes: también uno, del centro del borde de arriba.
    Dials {}

    // El diario: uno, del centro del borde de arriba.
    Diary {}

    // El bloqueo: tapa todos los monitores mientras dura.
    Seal {}
}
