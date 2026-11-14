import QtQuick
import qs

// Texto de Veyl. Las cifras son tabulares: la hora no baila al cambiar.
Text {
    property bool dim: false
    property bool strong: false

    color: dim ? Tokens.dim : Tokens.text
    font.family: Tokens.sans
    font.pixelSize: Tokens.fontSize
    font.weight: strong ? Font.DemiBold : Font.Medium
    font.features: ({ "tnum": 1 })
}
