pragma Singleton

import QtQuick
import Quickshell

// Las aplicaciones instaladas, como las ofrece la entrada: por orden
// alfabético y con sus nombres ya plegados para buscar en ellos.
Singleton {
    id: root

    readonly property var list: DesktopEntries.applications.values.filter(entry => !entry.noDisplay && entry.name).map(item).sort((a, b) => a.title.localeCompare(b.title))
    // El terminal para lo que lo pide (Settings.terminal, $TERMINAL o kitty).
    readonly property string terminal: Settings.terminal || Quickshell.env("TERMINAL") || "kitty"

    function item(entry: DesktopEntry): var {
        const program = (entry.command[0] ?? "").split("/").pop();
        return {
            key: `app:${entry.id}`,
            kind: "app",
            title: entry.name,
            subtitle: entry.genericName !== entry.name ? entry.genericName : "",
            label: "Aplicación",
            detail: entry.comment,
            icon: entry.icon,
            glyph: "app-window",
            verb: "Abrir",
            fields: [
                { text: Match.fold(entry.name), weight: 1, loose: true },
                { text: Match.fold(entry.genericName), weight: 0.8, loose: false },
                { text: Match.fold(program), weight: 0.8, loose: false },
                { text: Match.fold(entry.keywords.join(" ")), weight: 0.6, loose: false }
            ],
            run: () => root.launch(entry),
            // Lo que la aplicación ofrece además de abrirse: ventana nueva, privada…
            more: () => entry.actions.map(action => ({
                        title: action.name,
                        glyph: "arrow-square-out",
                        run: () => action.execute()
                    }))
        };
    }

    function launch(entry: DesktopEntry): void {
        if (!entry.runInTerminal) {
            entry.execute();
            return;
        }
        const command = [terminal, "-e"].concat(entry.command);
        if (entry.workingDirectory)
            Quickshell.execDetached({ command, workingDirectory: entry.workingDirectory });
        else
            Quickshell.execDetached(command);
    }

    // La imagen de un icono por su nombre, o nada si el tema no lo tiene.
    function picture(icon: string): string {
        if (!icon)
            return "";
        return icon.startsWith("/") ? `file://${icon}` : Quickshell.iconPath(icon, true);
    }

    // El icono de la aplicación de una ventana, por su clase.
    function emblem(windowClass: string): string {
        return windowClass ? picture(DesktopEntries.heuristicLookup(windowClass)?.icon ?? "") : "";
    }
}
