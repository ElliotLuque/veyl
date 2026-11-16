pragma Singleton

import QtQuick
import Quickshell

// Cuánto se parece lo escrito a un nombre. Todo se compara plegado: en
// minúsculas y sin tildes, para que «configuracion» encuentre «Configuración».
Singleton {
    readonly property var edge: /[\s\-_.\/:()]/  // lo que separa palabras

    function fold(text: string): string {
        return text.toLowerCase().normalize("NFD").replace(/[̀-ͯ]/g, "");
    }

    // De 0 (nada) a 100 (idéntico): al principio del nombre vale más que al
    // principio de una palabra, eso más que en medio, y en medio más que las
    // letras sueltas en orden («ffx» → «firefox»), que solo valen con `loose`.
    function score(needle: string, hay: string, loose: bool): real {
        if (!needle || !hay)
            return 0;
        let at = hay.indexOf(needle);
        if (at === 0)
            return needle.length === hay.length ? 100 : 90 - Math.min(10, (hay.length - needle.length) / 4);
        let inside = at > 0;
        while (at > 0) {
            if (edge.test(hay[at - 1]))
                return 76 - Math.min(6, at / 8);
            at = hay.indexOf(needle, at + 1);
        }
        // Las iniciales: «vsc» → «visual studio code».
        if (needle.length > 1 && initials(hay).startsWith(needle))
            return 70;
        if (inside)
            return 55;
        return loose && needle.length > 1 ? scatter(needle, hay) : 0;
    }

    function initials(hay: string): string {
        let letters = "";
        for (let i = 0; i < hay.length; i++)
            if (!edge.test(hay[i]) && (i === 0 || edge.test(hay[i - 1])))
                letters += hay[i];
        return letters;
    }

    // Las letras, en orden y con otras por medio: mejor cuanto más juntas y
    // más a principio de palabra caigan.
    function scatter(needle: string, hay: string): real {
        let from = 0;
        let last = -2;
        let points = 0;
        for (let i = 0; i < needle.length; i++) {
            const at = hay.indexOf(needle[i], from);
            if (at < 0)
                return 0;
            if (at === last + 1)
                points += 3;
            else if (at === 0 || edge.test(hay[at - 1]))
                points += 2;
            else
                points -= Math.min(3, (at - last - 1) / 2);
            last = at;
            from = at + 1;
        }
        return Math.max(18, Math.min(48, 30 + points * 18 / (needle.length * 3)));
    }

    // Lo escrito contra los campos de algo ({ text, weight, loose }): cuenta
    // el campo que mejor lo recoge. Con varias palabras, cada una tiene que
    // estar en algún campo.
    function rank(query: string, fields: var): real {
        let whole = 0;
        for (const field of fields)
            whole = Math.max(whole, score(query, field.text, field.loose) * field.weight);
        const words = query.split(" ").filter(word => word);
        if (words.length < 2)
            return whole;
        let total = 0;
        for (const word of words) {
            let best = 0;
            for (const field of fields)
                best = Math.max(best, score(word, field.text, field.loose) * field.weight);
            if (best === 0)
                return whole;
            total += best;
        }
        return Math.max(whole, total / words.length * 0.9);
    }
}
