pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Io

// Los emojis y los símbolos que se pueden pegar desde la entrada, con sus
// nombres en español (data/symbols.json, que sale de Unicode y CLDR con
// data/symbols.py). Se leen la primera vez que hacen falta.
Singleton {
    id: root

    property bool wanted: false
    property var groups: []
    // Cada uno: { text, name, words, group, tones }.
    property var all: []
    // Lo último que se pegó, primero.
    readonly property var recent: memory.recent ?? []
    readonly property var tones: ["claro", "claro medio", "medio", "oscuro medio", "oscuro"]

    function load(): void {
        wanted = true;
    }

    function use(text: string): void {
        memory.recent = [text].concat(recent.filter(old => old !== text)).slice(0, 24);
    }

    // «U+1F44D U+1F3FD»: cómo se escribe en Unicode.
    function code(text: string): string {
        const points = [];
        for (let at = 0; at < text.length; at++) {
            const point = text.codePointAt(at);
            if (point > 0xffff)
                at++;
            if (point !== 0xfe0f)
                points.push(`U+${point.toString(16).toUpperCase().padStart(4, "0")}`);
        }
        return points.join(" ");
    }

    function read(text: string): void {
        let data = null;
        try {
            data = JSON.parse(text);
        } catch (error) {}
        if (!Array.isArray(data?.groups) || !Array.isArray(data?.items))
            return;
        groups = data.groups;
        all = data.items.filter(item => Array.isArray(item) && typeof item[0] === "string" && typeof item[1] === "string").map(item => ({
                    text: item[0],
                    name: item[1],
                    words: typeof item[2] === "string" ? item[2] : "",
                    group: data.groups[item[3]] ?? "",
                    tones: Array.isArray(item[4]) ? item[4] : []
                }));
    }

    FileView {
        path: root.wanted ? `${Quickshell.shellDir}/data/symbols.json` : ""
        printErrors: false
        onLoaded: root.read(text())
    }

    FileView {
        path: `${Quickshell.env("XDG_STATE_HOME") || Quickshell.env("HOME") + "/.local/state"}/veyl/symbols.json`
        blockLoading: true
        printErrors: false
        onAdapterUpdated: writeAdapter()

        JsonAdapter {
            id: memory

            property var recent: []
        }
    }
}
