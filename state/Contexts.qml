pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Hyprland
import Quickshell.Io

// Un contexto es, en v0, un espacio de trabajo de Hyprland, y los de un
// monitor son niveles: se apilan y se cambia de uno a otro subiendo o bajando.
Singleton {
    id: root

    // Espacios por monitor cuando se usa hyprsplit (0 = sin dividir). Con él,
    // el espacio 5 es el «1» del segundo monitor.
    property int split: 0

    // Nombre visible: el del espacio si lo tiene; si no, su número.
    function label(workspace: HyprlandWorkspace): string {
        if (!workspace)
            return "";
        if (String(workspace.id) !== workspace.name)
            return workspace.name;
        return String(number(workspace.id));
    }

    // El número de un espacio dentro de su monitor.
    function number(id: int): int {
        return split > 0 ? (id - 1) % split + 1 : id;
    }

    // Los niveles de un monitor, de arriba abajo. Con hyprsplit son siempre
    // los mismos, tengan ventanas o no; sin él, los espacios que existan.
    // Debajo, las vistas abiertas (espacios con nombre, que nacen al vuelo),
    // en el orden en que nacieron; esas no tienen número.
    // Cada uno: { id, number, name, workspace, current }.
    function levels(monitor: HyprlandMonitor): var {
        if (!monitor)
            return [];
        const own = Hyprland.workspaces.values.filter(w => w.monitor === monitor && w.id > 0);
        const current = monitor.activeWorkspace?.id ?? 0;
        let ids = own.map(w => w.id);
        if (split > 0 && current > 0) {
            const first = current - (current - 1) % split;
            ids = Array.from({ length: split }, (_, i) => first + i);
        }
        const numbered = ids.sort((a, b) => a - b).map(id => {
            const workspace = own.find(w => w.id === id) ?? null;
            const named = !!workspace && String(id) !== workspace.name;
            return { id, number: number(id), name: named ? workspace.name : "", workspace, current: id === current };
        });
        const views = Hyprland.workspaces.values.filter(w => w.monitor === monitor && w.id < 0 && !w.name.startsWith("special:"));
        return numbered.concat(views.sort((a, b) => b.id - a.id).map(workspace => ({ id: workspace.id, number: "", name: workspace.name, workspace, current: workspace.id === current })));
    }

    // Ir a un nivel (uno de los de `levels`). Los numerados, por su id, que
    // con hyprsplit ya es el del monitor; las vistas, por su nombre.
    function go(level: var): void {
        if (!level || level.current)
            return;
        const target = level.id > 0 ? String(level.id) : `name:${level.name}`;
        Compositor.run(`hl.dispatch(hl.dsp.focus({ workspace = ${JSON.stringify(target)} }))`);
    }

    // Subir (-1) o bajar (1) un nivel en un monitor, sin dar la vuelta.
    function step(monitor: HyprlandMonitor, way: int): void {
        const all = levels(monitor);
        const at = all.findIndex(level => level.current);
        const next = all[Math.max(0, Math.min(all.length - 1, at + way))];
        if (at >= 0)
            go(next);
    }

    // Pantalla completa de verdad; una ventana maximizada respeta los huecos.
    function fullscreen(monitor: HyprlandMonitor): bool {
        const workspace = monitor?.activeWorkspace;
        return !!workspace && workspace.hasFullscreen && workspace.toplevels.values.some(t => t.wayland?.fullscreen ?? false);
    }

    Process {
        running: true
        command: ["hyprctl", "repl", 'local ok, h = pcall(require, "hyprsplit") return ok and h._config.num_workspaces or 0']
        stdout: StdioCollector {
            onStreamFinished: root.split = parseInt(text) || 0
        }
    }
}
