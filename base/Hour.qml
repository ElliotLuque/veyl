import QtQuick
import qs
import qs.state

// La hora, con su decoración: las horas en negrita, dos puntos apagados y
// los minutos. Al cambiar, cada cifra rueda. De fábrica, del tamaño de la
// isla; con otro `size`, todo crece con él.
Row {
    id: root

    readonly property date now: Clock.now
    property int size: Tokens.clockSize
    property int hours: Font.DemiBold   // peso de las horas
    property int minutes: Font.Medium   // y de los minutos
    readonly property real grow: size / Tokens.clockSize
    // Los dos puntos y el aire crecen menos que las cifras.
    readonly property real ease: Math.pow(grow, 0.6)

    spacing: Math.round(4 * ease)

    Roll {
        text: Qt.formatTime(root.now, "HH")
        size: root.size
        weight: root.hours
        implicitHeight: Math.max(Tokens.island, root.size * 1.15)
    }

    Column {
        anchors.verticalCenter: parent.verticalCenter
        spacing: Math.round(5 * root.ease)

        Repeater {
            model: 2

            Rectangle {
                width: Math.round(3 * root.ease)
                height: width
                radius: width / 2
                color: Tokens.accent
            }
        }
    }

    Roll {
        text: Qt.formatTime(root.now, "mm")
        size: root.size
        weight: root.minutes
        implicitHeight: Math.max(Tokens.island, root.size * 1.15)
    }
}
