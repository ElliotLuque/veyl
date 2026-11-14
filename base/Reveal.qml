import QtQuick
import qs

// Aparece y desaparece con muelle, cediendo su hueco a los vecinos.
Item {
    id: root

    property bool shown: true
    property real gap: 0          // separación con el vecino
    property bool leading: false  // la separación va delante en vez de detrás
    property alias settle: spring.settle
    readonly property real progress: spring.value
    default property alias content: holder.data

    implicitWidth: (holder.width + gap) * spring.value
    implicitHeight: holder.height
    visible: spring.value > 0
    opacity: spring.value

    Spring {
        id: spring
        to: root.shown ? 1 : 0
    }

    Item {
        id: holder
        x: root.leading ? root.gap : 0
        width: childrenRect.width
        height: childrenRect.height
    }
}
