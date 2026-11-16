import QtQuick
import qs
import qs.state

// Lo que se ve de la entrada: arriba, el campo en el que escribes; debajo,
// las filas; al pie, qué hace Intro con la fila elegida. En el portapapeles,
// al lado de las filas va lo copiado, a la vista; los emojis y símbolos van
// en cuadrícula. Qué filas hay y qué hace
// cada tecla lo sabe state/Launcher.qml.
Item {
    id: root

    // Cuánto se ve ya de la placa: el pie va pegado a su borde mientras crece.
    property real revealed: height
    readonly property var view: Launcher.views[Launcher.view]
    readonly property bool wide: Launcher.view === "clipboard"
    readonly property bool empty: Launcher.items.length === 0
    readonly property real margin: 6  // aire encima y debajo de las filas
    // Las filas que caben, con el título de su grupo encima.
    readonly property real room: Tokens.heading + Tokens.rows * Tokens.row
    readonly property real body: (empty ? 132 : wide ? room : Math.min(room, Launcher.extent)) + margin * 2
    readonly property bool armed: Launcher.armed !== ""
    // Lo que el pie dice a la izquierda: lo que espera, lo que acaba de
    // pasar o algo más de la fila elegida.
    readonly property string caption: armed ? "Intro otra vez para confirmar" : Launcher.note !== "" ? Launcher.note : Launcher.current?.detail || (wide && !empty ? Launcher.destination : "")

    function press(event: var): void {
        const ctrl = (event.modifiers & Qt.ControlModifier) !== 0;
        const plain = (event.modifiers & (Qt.ControlModifier | Qt.AltModifier | Qt.ShiftModifier)) === 0;
        const grid = Launcher.grid;
        const page = grid ? Math.max(1, Math.floor(root.room / Tokens.cell) - 1) : Tokens.rows - 1;
        event.accepted = true;
        switch (event.key) {
        case Qt.Key_Escape:
            Launcher.cancel();
            return;
        case Qt.Key_Tab:
            Launcher.move(1, !event.isAutoRepeat);
            return;
        case Qt.Key_Backtab:
            Launcher.move(-1, !event.isAutoRepeat);
            return;
        case Qt.Key_Down:
            Launcher.step(1, !event.isAutoRepeat);
            return;
        case Qt.Key_Up:
            Launcher.step(-1, !event.isAutoRepeat);
            return;
        // En la cuadrícula las flechas de los lados mueven la elección, no el cursor.
        case Qt.Key_Right:
        case Qt.Key_Left:
            if (grid && plain) {
                Launcher.move(event.key === Qt.Key_Right ? 1 : -1, false);
                return;
            }
            break;
        case Qt.Key_PageDown:
            Launcher.step(page, false);
            return;
        case Qt.Key_PageUp:
            Launcher.step(-page, false);
            return;
        case Qt.Key_Home:
        case Qt.Key_End:
            if (ctrl) {
                Launcher.move(event.key === Qt.Key_End ? Launcher.items.length : -Launcher.items.length, false);
                return;
            }
            break;
        case Qt.Key_Backspace:
            // Sin nada que borrar, deshace la vista.
            if (plain && input.text === "" && Launcher.stack.length > 0) {
                Launcher.back();
                return;
            }
            break;
        }
        const chord = Launcher.chord(event);
        if (chord === "return")
            Launcher.activate(Launcher.current, null);
        else if (chord === "ctrl+n")
            Launcher.step(1, !event.isAutoRepeat);
        else if (chord === "ctrl+p")
            Launcher.step(-1, !event.isAutoRepeat);
        else if (chord === "ctrl+k")
            Launcher.more();
        else if (!chord.includes("+") || !Launcher.shortcut(chord))
            event.accepted = false;
    }

    // Lo que dice el pie que hace una tecla; pulsarlo es pulsarla.
    component Hint: Item {
        id: hint

        property alias text: label.text
        property alias keys: cap.keys
        property bool dim: false
        signal pressed

        width: pair.width
        height: Tokens.bar

        Row {
            id: pair

            anchors.verticalCenter: parent.verticalCenter
            spacing: 8

            Ink {
                id: label
                anchors.verticalCenter: parent.verticalCenter
                textFormat: Text.PlainText
                font.pixelSize: Tokens.smallSize
                dim: hint.dim && !area.containsMouse
            }

            Keycap {
                id: cap
                anchors.verticalCenter: parent.verticalCenter
            }
        }

        MouseArea {
            id: area
            anchors.fill: parent
            anchors.leftMargin: -6
            anchors.rightMargin: -6
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: hint.pressed()
        }
    }

    width: Launcher.span
    height: Tokens.field + Tokens.line + body + Tokens.bar

    Binding {
        target: Launcher
        property: "columns"
        value: Math.max(1, Math.floor((list.width - Tokens.gap * 2) / Tokens.cell))
    }

    Connections {
        target: Launcher

        function onOpenChanged(): void {
            if (!Launcher.open)
                return;
            list.pointer = null;
            input.forceActiveFocus();
        }

        function onQueryChanged(): void {
            if (input.text !== Launcher.query)
                input.text = Launcher.query;
        }

        function onChosen(): void {
            list.follow();
        }
    }

    // Pulsar la placa no es pulsar fuera: no cierra, y el teclado sigue aquí.
    MouseArea {
        anchors.fill: parent
        onPressed: input.forceActiveFocus()
    }

    // --- El campo ---

    Item {
        id: field

        width: parent.width
        height: Tokens.field

        Icon {
            x: Tokens.gap + 10 + (Tokens.emblem - width) / 2
            anchors.verticalCenter: parent.verticalCenter
            name: "magnifying-glass"
            font.pixelSize: 20
            visible: !crumb.visible
        }

        // En una vista: cuál es, y la vuelta atrás.
        Rectangle {
            id: crumb

            x: Tokens.gap + 6
            anchors.verticalCenter: parent.verticalCenter
            width: trail.width + 24
            height: 28
            radius: height / 2
            color: back.containsMouse ? Tokens.border : Tokens.surface
            visible: Launcher.stack.length > 0

            Behavior on color {
                ColorAnimation { duration: Tokens.fast }
            }

            Row {
                id: trail

                anchors.centerIn: parent
                spacing: 6

                Icon {
                    anchors.verticalCenter: parent.verticalCenter
                    name: "arrow-left"
                    color: Tokens.text
                    font.pixelSize: 14
                }

                Ink {
                    anchors.verticalCenter: parent.verticalCenter
                    width: Math.min(implicitWidth, 220)
                    text: (Launcher.view === "actions" ? Launcher.subject?.title : root.view.title) ?? ""
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    maximumLineCount: 1
                    font.pixelSize: Tokens.smallSize
                }
            }

            MouseArea {
                id: back
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: Launcher.back()
            }
        }

        TextInput {
            id: input

            x: crumb.visible ? crumb.x + crumb.width + 12 : Tokens.gap + 10 + Tokens.emblem + 12
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - x - Tokens.pad
            clip: true
            focus: true
            enabled: Launcher.open
            color: Tokens.text
            selectionColor: Tokens.accent
            selectedTextColor: Tokens.bg
            font.family: Tokens.sans
            font.pixelSize: Tokens.promptSize
            font.weight: Font.Medium
            // El campo en el que escribes: su cursor, del color del acento.
            cursorDelegate: Rectangle {
                width: 2
                color: Tokens.accent
                visible: input.activeFocus

                SequentialAnimation on opacity {
                    running: input.activeFocus
                    loops: Animation.Infinite

                    PauseAnimation { duration: 550 }
                    NumberAnimation { to: 0; duration: Tokens.fast }
                    PauseAnimation { duration: 350 }
                    NumberAnimation { to: 1; duration: Tokens.fast }
                }
            }
            onTextEdited: Launcher.query = text
            Keys.onPressed: event => root.press(event)

            Ink {
                width: parent.width
                text: root.view.hint
                textFormat: Text.PlainText
                elide: Text.ElideRight
                font.pixelSize: Tokens.promptSize
                dim: true
                visible: input.text === "" && input.preeditText === ""
            }
        }
    }

    Rectangle {
        y: Tokens.field
        width: parent.width
        height: Tokens.line
        color: Tokens.border
    }

    // --- Las filas ---

    // Solo existen las que caben a la vista: al deslizar, o al cambiar lo
    // escrito, las mismas pasan a enseñar otra cosa. Nada se crea al teclear.
    Flickable {
        id: list

        // Lo que ocupa una casilla de la cuadrícula, a lo ancho.
        readonly property real cell: (width - Tokens.gap * 2) / Launcher.columns
        readonly property int pool: Launcher.grid ? (Math.ceil(root.room / Tokens.cell) + 2) * Launcher.columns + 2 : Math.ceil(root.room / Tokens.heading) + 2
        // La primera fila a la vista (en la cuadrícula, la primera de su línea).
        readonly property int first: {
            const tops = Launcher.tops;
            let low = 0;
            let high = tops.length - 1;
            while (low < high) {
                const middle = (low + high + 1) >> 1;
                if (tops[middle] <= contentY)
                    low = middle;
                else
                    high = middle - 1;
            }
            return Launcher.lead(low);
        }
        // Dónde estaba el puntero: solo elige si de verdad se mueve, no si
        // son las filas las que pasan por debajo.
        property var pointer: null

        function hover(at: int, where: point): void {
            const moved = pointer !== null && (where.x !== pointer.x || where.y !== pointer.y);
            pointer = where;
            if (moved)
                Launcher.point(at);
        }

        // Lleva el resalte a la fila elegida y la deja a la vista: con las
        // teclas la acompaña; si las filas han cambiado, salta.
        function follow(): void {
            // Las filas recién cambiadas: Launcher.current puede ir aún un paso atrás.
            const item = Launcher.items[Launcher.index];
            const keys = Launcher.manner === "keys";
            if (!item || item.kind === "header") {
                slide.stop();
                scroll.stop();
                contentY = 0;
                return;
            }
            const card = item.kind === "sum";
            const col = Launcher.cols[Launcher.index] ?? -1;
            const top = Launcher.tops[Launcher.index];
            const tall = Launcher.tall(item);
            const y = top + (card ? sum.above : 0);
            spot.height = tall - (card ? sum.above + sum.below : 0);
            spot.gliding = keys && spot.visible;
            spot.col = col;
            if (keys && spot.visible) {
                slide.to = y;
                slide.restart();
            } else {
                slide.stop();
                spot.y = y;
            }
            if (Launcher.manner === "pointer")
                return;
            const limit = Math.max(0, Launcher.extent - height);
            let wanted = scroll.running ? scroll.to : contentY;
            // El título de su grupo, si es la primera de él, entra con ella.
            const above = Launcher.items[Launcher.lead(Launcher.index) - 1]?.kind === "header" ? Tokens.heading : 0;
            if (top - above < wanted)
                wanted = top - above;
            else if (top + tall > wanted + height)
                wanted = top + tall - height;
            wanted = Math.max(0, Math.min(limit, wanted));
            if (keys && wanted !== contentY) {
                scroll.to = wanted;
                scroll.restart();
            } else if (!keys) {
                scroll.stop();
                contentY = wanted;
            }
        }

        y: Tokens.field + Tokens.line + root.margin
        width: root.wide ? Math.round(root.width * 0.42) : root.width
        height: root.body - root.margin * 2
        contentWidth: width
        contentHeight: Launcher.extent
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        interactive: contentHeight > height
        visible: !root.empty

        NumberAnimation {
            id: slide
            target: spot
            property: "y"
            duration: Tokens.step
            easing.type: Easing.OutCubic
        }

        NumberAnimation {
            id: scroll
            target: list
            property: "contentY"
            duration: Tokens.step
            easing.type: Easing.OutCubic
        }

        // El resalte de la fila elegida.
        Rectangle {
            id: spot

            // En la cuadrícula, la columna de la casilla elegida.
            property int col: -1
            property bool gliding: false

            x: Tokens.gap + Math.max(0, col) * list.cell
            width: col >= 0 ? list.cell : list.width - Tokens.gap * 2
            radius: Tokens.radius
            color: Tokens.surface
            border.width: Tokens.line
            border.color: root.armed ? Tokens.attention : Tokens.surface
            visible: !!Launcher.current && Launcher.current.kind !== "header"

            Behavior on x {
                enabled: spot.gliding

                NumberAnimation {
                    duration: Tokens.step
                    easing.type: Easing.OutCubic
                }
            }
        }

        Repeater {
            model: list.pool

            Item {
                id: slot

                required property int index
                // Qué fila le toca: cada una se queda en su hueco mientras
                // siga a la vista.
                readonly property int at: list.first + ((index - list.first) % list.pool + list.pool) % list.pool
                readonly property var item: Launcher.items[at] ?? null
                readonly property string kind: item?.kind ?? ""
                readonly property bool cell: kind === "symbol" && Launcher.grid
                readonly property bool plain: kind !== "" && kind !== "header" && kind !== "sum" && !cell
                readonly property bool selected: at === Launcher.index

                x: cell ? Tokens.gap + Math.max(0, Launcher.cols[at] ?? 0) * list.cell : 0
                y: Launcher.tops[at] ?? 0
                width: cell ? list.cell : list.width
                height: item ? Launcher.tall(item) : 0
                visible: item !== null

                // El título de un grupo de filas.
                Ink {
                    x: Tokens.gap + 10
                    y: parent.height - height - 5
                    text: slot.kind === "header" ? slot.item.title : ""
                    font.pixelSize: Tokens.smallSize
                    dim: true
                    visible: slot.kind === "header"
                }

                Sum {
                    anchors.fill: parent
                    item: slot.kind === "sum" ? slot.item : null
                    selected: slot.selected
                    visible: slot.kind === "sum"
                }

                Entry {
                    anchors.fill: parent
                    item: slot.plain ? slot.item : null
                    selected: slot.selected
                    visible: slot.plain
                }

                // Una casilla de la cuadrícula: el emoji o el símbolo, grande.
                Text {
                    anchors.centerIn: parent
                    text: slot.cell ? slot.item.character : ""
                    textFormat: Text.PlainText
                    color: Tokens.text
                    font.family: Tokens.sans
                    font.pixelSize: 28
                    visible: slot.cell
                }

                MouseArea {
                    anchors.fill: parent
                    enabled: slot.kind !== "header"
                    hoverEnabled: true
                    onPositionChanged: mouse => list.hover(slot.at, mapToItem(null, mouse.x, mouse.y))
                    onClicked: {
                        Launcher.point(slot.at);
                        Launcher.activate(slot.item, null);
                    }
                }
            }
        }
    }

    // Solo para saber cuánto aire deja la cuenta alrededor de su tarjeta.
    Sum {
        id: sum
        visible: false
    }

    // Nada que enseñar.
    Column {
        y: Tokens.field + Tokens.line + (root.body - height) / 2
        anchors.horizontalCenter: parent.horizontalCenter
        spacing: 10
        visible: root.empty

        Icon {
            anchors.horizontalCenter: parent.horizontalCenter
            name: Launcher.query.trim() !== "" ? "magnifying-glass" : root.view.glyph ?? "magnifying-glass"
            font.pixelSize: 28
        }

        Ink {
            anchors.horizontalCenter: parent.horizontalCenter
            text: Launcher.query.trim() !== "" ? root.view.busy || "Nada con ese nombre" : root.view.none ?? "Nada que enseñar"
            dim: true
        }
    }

    // --- Lo copiado, a la vista ---

    Rectangle {
        x: list.width
        y: Tokens.field + Tokens.line
        width: Tokens.line
        height: root.body
        color: Tokens.border
        visible: preview.visible
    }

    Preview {
        id: preview

        x: list.width + Tokens.line
        y: Tokens.field + Tokens.line
        width: root.width - x
        height: root.body
        item: Launcher.current?.kind === "clip" ? Launcher.current : null
        visible: root.wide && !root.empty
    }

    // --- El pie ---

    Item {
        id: foot

        y: Math.max(Tokens.field, root.revealed - height)
        width: parent.width
        height: Tokens.bar

        // Tapa las filas que la placa aún no ha descubierto.
        Rectangle {
            x: Tokens.line
            width: parent.width - Tokens.line * 2
            height: parent.height - Tokens.line
            color: Tokens.plate
            bottomLeftRadius: Tokens.round - Tokens.line
            bottomRightRadius: Tokens.round - Tokens.line
        }

        Rectangle {
            width: parent.width
            height: Tokens.line
            color: Tokens.border
        }

        Row {
            x: Tokens.gap + 12
            anchors.verticalCenter: parent.verticalCenter
            spacing: 8

            Icon {
                anchors.verticalCenter: parent.verticalCenter
                name: root.armed ? "warning" : "check-circle"
                color: root.armed ? Tokens.attention : Tokens.accent
                font.pixelSize: 15
                visible: root.armed || Launcher.note !== ""
            }

            Ink {
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(implicitWidth, foot.width - hints.width - 80)
                text: root.caption
                textFormat: Text.PlainText
                elide: Text.ElideRight
                maximumLineCount: 1
                font.pixelSize: Tokens.smallSize
                color: root.armed ? Tokens.attention : Tokens.dim
            }
        }

        Row {
            id: hints

            x: parent.width - width - Tokens.gap - 8
            anchors.verticalCenter: parent.verticalCenter
            spacing: 14

            Hint {
                anchors.verticalCenter: parent.verticalCenter
                text: root.armed ? "Confirmar" : Launcher.current?.verb ?? ""
                keys: "return"
                visible: !!Launcher.current?.run
                onPressed: Launcher.activate(Launcher.current, null)
            }

            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                width: Tokens.line
                height: 14
                color: Tokens.border
                visible: Launcher.extras || Launcher.view === "actions"
            }

            Hint {
                anchors.verticalCenter: parent.verticalCenter
                text: "Acciones"
                keys: "ctrl+k"
                dim: true
                visible: Launcher.extras
                onPressed: Launcher.more()
            }

            Hint {
                anchors.verticalCenter: parent.verticalCenter
                text: "Volver"
                keys: "esc"
                dim: true
                visible: Launcher.view === "actions"
                onPressed: Launcher.back()
            }
        }
    }
}
