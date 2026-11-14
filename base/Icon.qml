import QtQuick
import qs

// Icono de Phosphor, por su nombre («microphone», «battery-low»…): la fuente
// lo resuelve con una ligadura.
Text {
    property string name

    text: name
    color: Tokens.dim
    font.family: Tokens.icons
    font.pixelSize: Tokens.iconSize
}
