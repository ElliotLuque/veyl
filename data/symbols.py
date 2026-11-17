#!/usr/bin/env python3
# Genera data/symbols.json, los emojis y símbolos de la entrada, con sus
# nombres y palabras en español. Sale de los datos de Unicode y CLDR:
#
#   https://unicode.org/Public/emoji/latest/emoji-test.txt
#   https://raw.githubusercontent.com/unicode-org/cldr/main/common/annotations/es.xml
#   https://raw.githubusercontent.com/unicode-org/cldr/main/common/annotationsDerived/es.xml
#
#   python3 data/symbols.py emoji-test.txt es.xml es-derived.xml > data/symbols.json
#
# El resultado: { groups: [nombre…], items: [[lo que se pega, nombre,
# palabras, grupo, tonos de piel o nada]…] }, en el orden de Unicode.
import json
import re
import sys
import unicodedata
import xml.etree.ElementTree as ElementTree

EMOJI_GROUPS = {
    "Smileys & Emotion": "Caras y emociones",
    "People & Body": "Personas y cuerpo",
    "Animals & Nature": "Animales y naturaleza",
    "Food & Drink": "Comida y bebida",
    "Travel & Places": "Viajes y lugares",
    "Activities": "Actividades",
    "Objects": "Objetos",
    "Symbols": "Símbolos",
    "Flags": "Banderas",
}
SYMBOL_GROUPS = ["Flechas", "Matemáticas", "Monedas", "Puntuación", "Otros símbolos"]
TONES = re.compile("[\U0001F3FB-\U0001F3FF]")


def annotations(*paths):
    """Lo que dice CLDR de cada cosa: su nombre y sus palabras."""
    names, words = {}, {}
    for path in paths:
        for node in ElementTree.parse(path).getroot().iter("annotation"):
            cp = node.get("cp")
            if node.get("type") == "tts":
                names.setdefault(cp, node.text.strip())
            else:
                words.setdefault(cp, [word.strip() for word in node.text.split("|")])
    return names, words


def lookup(table, text):
    return table.get(text) or table.get(text.replace("\ufe0f", ""))


def symbol_group(char):
    name = unicodedata.name(char, "")
    category = unicodedata.category(char)
    if "ARROW" in name:
        return "Flechas"
    if category == "Sm" or "∀" <= char <= "⋿":
        return "Matemáticas"
    if category == "Sc":
        return "Monedas"
    if category.startswith("P"):
        return "Puntuación"
    return "Otros símbolos"


def main():
    test, plain, derived = sys.argv[1:4]
    names, words = annotations(plain, derived)
    groups = list(EMOJI_GROUPS.values()) + SYMBOL_GROUPS
    items, seen, base = [], set(), {}
    group = None
    for line in open(test, encoding="utf-8"):
        if line.startswith("# group:"):
            group = EMOJI_GROUPS.get(line.split(":", 1)[1].strip())
            continue
        if line.startswith("#") or ";" not in line:
            continue
        char = "".join(chr(int(code, 16)) for code in line.split(";")[0].split())
        # Lo que ya es un emoji, aunque no salga (los tonos sueltos), no es un símbolo.
        seen.add(char.replace("\ufe0f", ""))
        name = lookup(names, char)
        if group is None or "; fully-qualified" not in line or not name:
            continue
        # Los tonos de piel no son filas: van con el emoji del que salen.
        if TONES.search(char):
            owner = base.get(TONES.sub("", char).replace("\ufe0f", ""))
            if owner is not None and len(TONES.findall(char)) == 1:
                owner[4].append(char)
            continue
        item = [char, name, " ".join(word for word in lookup(words, char) or [] if word != name), groups.index(group), []]
        base[char.replace("\ufe0f", "")] = item
        items.append(item)
    # Lo que CLDR nombra y no es un emoji: flechas, signos, monedas…
    for char, name in names.items():
        if char in seen or len(char) != 1 or unicodedata.category(char).startswith(("L", "N", "Z", "C")):
            continue
        if char.isascii() and char not in "^`~|\\":
            continue
        items.append([char, name, " ".join(word for word in words.get(char, []) if word != name), groups.index(symbol_group(char)), []])
    for item in items:
        if not item[4]:
            item.pop()
    json.dump({"groups": groups, "items": items}, sys.stdout, ensure_ascii=False, separators=(",", ":"))
    print()


if __name__ == "__main__":
    main()
