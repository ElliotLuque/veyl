import QtQuick
import qs
import qs.state

// El almanaque: lo que enseña la isla de la hora al abrirse. Arriba, la fecha
// junto a la hora; debajo, una tira continua de semanas que la rueda desliza
// en vertical, como los niveles, y cuyo mes es el que ocupa el centro; luego,
// el día elegido
// con sus eventos y un campo para apuntar otro, lo que viene, y el
// temporizador.
Item {
    id: root

    property real room: 0       // lo que ocupa la fila de la isla, arriba a la derecha
    // La semana que va arriba en la tira: a donde va, con decimales mientras
    // un panel táctil la arrastra.
    property real scroll: Calendar.week(Calendar.monthStart(Calendar.thisMonth))
    // El mes a la vista (año × 12 + mes): el del centro de la tira
    readonly property int month: Calendar.monthOf(Calendar.weekStart(Math.round(scroll) + weeks / 2))
    property int selected: Calendar.today
    property bool years: false  // vista de año: los doce meses para saltar
    property int year: Math.floor(month / 12)
    readonly property bool typing: field.activeFocus
    readonly property int cell: 42       // alto de una semana
    // La columna de los números de semana, si se piden; sin ella, los días
    // se reparten su sitio.
    readonly property int numbers: Settings.weekNumbers ? 32 : 0
    readonly property int breadth: Math.floor((Tokens.almanac - Tokens.pad * 2 - numbers) / 7)  // ancho de un día
    readonly property int weeks: 6
    readonly property int card: Tokens.round - Tokens.inset  // radio de lo que va dentro
    readonly property var picked: Agenda.days[selected] ?? []
    readonly property var coming: Agenda.ahead.filter(event => Calendar.at(event.at) !== selected).slice(0, 3)
    readonly property int most: 4  // eventos de un día antes de resumir el resto
    property bool whole: false     // el día elegido, con todos sus eventos
    readonly property int shown: whole || picked.length <= most ? picked.length : most - 1

    onSelectedChanged: whole = false

    // Vuelve a hoy, sin animar: así se abre siempre.
    function reset(): void {
        years = false;
        whole = false;
        selected = Calendar.today;
        scroll = Calendar.week(Calendar.monthStart(Calendar.thisMonth));
        reel.velocity = 0;
        reel.value = reel.to;
        field.clear();
    }

    // Elige un día; si no está a la vista, va a su mes.
    function pick(day: int): void {
        selected = day;
        const week = Calendar.week(day) - Math.round(scroll);
        if (week < 0 || week >= weeks)
            go(Calendar.monthOf(day));
    }

    // A otro mes, con su primera semana arriba. Desde muy lejos no recorre
    // todo el camino: aparece cerca y llega rodando.
    function go(month: int): void {
        scroll = Calendar.week(Calendar.monthStart(month));
        const way = reel.to - reel.value;
        if (Math.abs(way) > 12)
            reel.value = reel.to - Math.sign(way) * 6;
    }

    function step(way: int): void {
        if (years)
            year += way;
        else
            go(month + way);
    }

    function title(day: int): string {
        const near = { [-1]: "ayer", 0: "hoy", 1: "mañana" }[day - Calendar.today];
        return near ?? Calendar.format(day, "dddd d 'de' MMMM");
    }

    // Cuándo es, contado desde ahora
    function when(event: var): string {
        const day = Calendar.at(event.at);
        const minutes = Math.ceil((event.at - Agenda.now) / 60000);
        if (!event.allDay && minutes <= 0)
            return "ahora";
        if (!event.allDay && minutes < 60)
            return `en ${minutes} min`;
        const name = day - Calendar.today < 2 ? title(day) : Calendar.format(day, day - Calendar.today < 7 ? "ddd d" : "d MMM").replace(".", "");
        return event.allDay ? name : `${name} · ${Agenda.clock(event.at)}`;
    }

    // «16:30 dentista» o «dentista 16:30»: con hora; sin ella, todo el día.
    function add(): void {
        const text = field.text.trim();
        if (text === "")
            return;
        const lead = text.match(/^(\d{1,2}[:.]\d{2})\s+(.+)$/);
        const tail = text.match(/^(.+?)\s+(\d{1,2}[:.]\d{2})$/);
        let time = -1;
        let name = text;
        if (lead && Agenda.parseTime(lead[1]) >= 0) {
            time = Agenda.parseTime(lead[1]);
            name = lead[2];
        } else if (tail && Agenda.parseTime(tail[2]) >= 0) {
            time = Agenda.parseTime(tail[2]);
            name = tail[1];
        }
        Agenda.put(name, Calendar.moment(selected, Math.max(0, time)), time < 0);
        field.clear();
    }

    implicitWidth: Tokens.almanac
    implicitHeight: column.height + Tokens.inset

    // Un día de la rejilla
    component Day: Item {
        id: cell

        required property int day
        readonly property bool today: day === Calendar.today
        readonly property bool chosen: day === root.selected
        readonly property bool inside: Calendar.monthOf(day) === root.month
        readonly property int count: Agenda.days[day]?.length ?? 0

        width: root.breadth
        height: root.cell

        Rectangle {
            anchors.centerIn: parent
            width: Tokens.day
            height: Tokens.day
            radius: width / 2
            color: cell.today ? Tokens.accent : cell.chosen || hit.containsMouse ? Tokens.surface : "transparent"
            border.width: cell.chosen && !cell.today ? Tokens.line : 0
            border.color: Tokens.accent

            Behavior on color {
                ColorAnimation { duration: Tokens.fast }
            }
        }

        Ink {
            anchors.centerIn: parent
            anchors.verticalCenterOffset: cell.count > 0 ? -2 : 0
            text: Calendar.parts(cell.day).date
            font.pixelSize: Tokens.fontSize
            strong: cell.today
            color: cell.today ? Tokens.bg : cell.inside ? Tokens.text : Tokens.dim
            opacity: cell.inside || cell.today ? 1 : 0.55

            Behavior on color {
                ColorAnimation { duration: Tokens.fast }
            }
        }

        // Un punto por evento, hasta tres
        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            y: parent.height / 2 + 9
            spacing: 2

            Repeater {
                model: (Agenda.days[cell.day] ?? []).slice(0, 3)

                Rectangle {
                    required property var modelData

                    width: 4
                    height: 4
                    radius: 2
                    // Del color de su calendario; sobre hoy, del fondo.
                    color: cell.today ? Tokens.bg : Agenda.hue(modelData)
                    opacity: cell.inside || cell.today ? 1 : 0.55
                }
            }
        }

        MouseArea {
            id: hit
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: root.pick(cell.day)
        }
    }

    // La tira de semanas: la primera fila a la vista, con decimales mientras rueda.
    Spring {
        id: reel
        to: root.scroll
        settle: Tokens.veil
    }

    Spring {
        id: zoom
        to: root.years ? 1 : 0
        settle: Tokens.veil
    }

    Column {
        id: column
        width: parent.width

        // Junto a la hora, la fecha entera
        Item {
            width: parent.width
            height: Tokens.island

            // Si las excepciones no le dejan sitio, la corta.
            Ink {
                id: date
                anchors.verticalCenter: parent.verticalCenter
                x: Tokens.pad
                width: Math.max(0, parent.width - root.room - x)
                text: Clock.now.toLocaleDateString(Calendar.locale, full.width <= width ? Calendar.longFormat : "ddd d MMM")
                elide: Text.ElideRight
                dim: true

                TextMetrics {
                    id: full
                    font: date.font
                    text: Clock.now.toLocaleDateString(Calendar.locale, Calendar.longFormat)
                }
            }
        }

        // El mes, para ir a otro; pulsado, el año entero.
        Item {
            width: parent.width
            height: Tokens.island

            Rectangle {
                id: heading
                x: Tokens.pad - 10
                anchors.verticalCenter: parent.verticalCenter
                width: name.width + 20
                height: Tokens.knob
                radius: height / 2
                color: chooser.containsMouse ? Tokens.surface : "transparent"

                Behavior on color {
                    ColorAnimation { duration: Tokens.fast }
                }

                Roll {
                    id: name
                    x: 10
                    anchors.verticalCenter: parent.verticalCenter
                    text: root.years ? String(root.year) : `${Calendar.monthName(root.month, false)} ${Math.floor(root.month / 12)}`
                    order: root.years ? root.year * 12 : root.month
                    strong: true
                    size: Tokens.clockSize
                }

                MouseArea {
                    id: chooser
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        root.year = Math.floor(root.month / 12);
                        root.years = !root.years;
                    }
                }
            }

            Row {
                anchors.right: parent.right
                anchors.rightMargin: Tokens.pad - 6
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2

                // Volver a hoy, si te has ido
                Reveal {
                    anchors.verticalCenter: parent.verticalCenter
                    shown: root.years || root.scroll !== Calendar.week(Calendar.monthStart(Calendar.thisMonth)) || root.selected !== Calendar.today
                    gap: 6

                    Rectangle {
                        width: back.width + 24
                        height: 24
                        radius: height / 2
                        color: home.containsMouse ? Tokens.border : Tokens.surface

                        Behavior on color {
                            ColorAnimation { duration: Tokens.fast }
                        }

                        Ink {
                            id: back
                            anchors.centerIn: parent
                            text: "hoy"
                            font.pixelSize: Tokens.smallSize
                        }

                        MouseArea {
                            id: home
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                root.years = false;
                                root.pick(Calendar.today);
                            }
                        }
                    }
                }

                Knob {
                    icon: "caret-up"
                    tint: Tokens.dim
                    onPressed: root.step(-1)
                }

                Knob {
                    icon: "caret-down"
                    tint: Tokens.dim
                    onPressed: root.step(1)
                }
            }
        }

        // Las semanas o el año. En las semanas la rueda desliza la tira sin
        // saltos, una semana por muesca y al píxel con un panel táctil, y al
        // soltarla se asienta en la semana más cercana. En el año, la rueda
        // pasa de año en año.
        Item {
            id: sheet

            property real spin: 0

            x: (parent.width - width) / 2
            width: root.numbers + root.breadth * 7
            height: days.height + root.cell * root.weeks

            WheelHandler {
                acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
                onWheel: event => {
                    if (!root.years) {
                        const pixels = event.pixelDelta.y || event.pixelDelta.x;
                        root.scroll -= pixels !== 0 ? pixels / root.cell : (event.angleDelta.y || event.angleDelta.x) / 120;
                        rest.restart();
                        return;
                    }
                    sheet.spin -= event.angleDelta.y || event.angleDelta.x;
                    while (Math.abs(sheet.spin) >= 120) {
                        const way = Math.sign(sheet.spin);
                        sheet.spin -= way * 120;
                        root.step(way);
                    }
                }
            }

            Timer {
                id: rest
                interval: 160
                onTriggered: root.scroll = Math.round(root.scroll)
            }

            // Meses
            Item {
                anchors.fill: parent
                opacity: 1 - zoom.value
                visible: opacity > 0
                scale: 1 + 0.06 * zoom.value

                // Las iniciales de los días
                Row {
                    id: days
                    x: root.numbers
                    height: 26

                    Repeater {
                        model: Calendar.weekdays

                        Ink {
                            required property string modelData
                            required property int index

                            width: root.breadth
                            horizontalAlignment: Text.AlignHCenter
                            text: modelData.toUpperCase()
                            font.pixelSize: Tokens.smallSize
                            strong: true
                            readonly property int weekday: (Calendar.firstDay + index) % 7

                            // El de hoy, del acento; el fin de semana, más apagado.
                            color: weekday === Calendar.parts(Calendar.today).weekday ? Tokens.accent : Tokens.dim
                            opacity: color === Tokens.accent || (weekday !== 0 && weekday !== 6) ? 1 : 0.6
                        }
                    }
                }

                Item {
                    y: days.height
                    width: parent.width
                    height: root.cell * root.weeks
                    clip: true

                    Repeater {
                        model: root.weeks + 2

                        Item {
                            id: line

                            required property int index
                            readonly property int first: Math.floor(reel.value)
                            readonly property int count: root.weeks + 2
                            readonly property int week: first + ((index - first) % count + count) % count
                            readonly property int start: Calendar.weekStart(week)
                            readonly property bool current: week === Calendar.week(Calendar.today)

                            y: (week - reel.value) * root.cell
                            width: parent.width
                            height: root.cell

                            Ink {
                                width: root.numbers
                                height: parent.height
                                horizontalAlignment: Text.AlignHCenter
                                verticalAlignment: Text.AlignVCenter
                                text: Calendar.weekNumber(line.start)
                                visible: root.numbers > 0
                                font.pixelSize: Tokens.smallSize
                                color: line.current ? Tokens.accent : Tokens.dim
                                opacity: line.current ? 1 : 0.6
                            }

                            Repeater {
                                model: 7

                                Day {
                                    required property int index

                                    x: root.numbers + index * root.breadth
                                    day: line.start + index
                                }
                            }
                        }
                    }
                }
            }

            // El año: los doce meses
            Grid {
                anchors.fill: parent
                columns: 4
                opacity: zoom.value
                visible: opacity > 0
                scale: 0.94 + 0.06 * zoom.value

                Repeater {
                    model: 12

                    Item {
                        id: slot

                        required property int index
                        readonly property int month: root.year * 12 + index
                        readonly property bool current: month === Calendar.thisMonth
                        readonly property bool chosen: month === root.month

                        width: sheet.width / 4
                        height: sheet.height / 3

                        Rectangle {
                            anchors.centerIn: parent
                            width: parent.width - 10
                            height: Tokens.island
                            radius: height / 2
                            color: slot.current ? Tokens.accent : slot.chosen || chose.containsMouse ? Tokens.surface : "transparent"
                            border.width: slot.chosen && !slot.current ? Tokens.line : 0
                            border.color: Tokens.accent

                            Behavior on color {
                                ColorAnimation { duration: Tokens.fast }
                            }
                        }

                        Ink {
                            anchors.centerIn: parent
                            text: Calendar.monthName(slot.month, true).replace(".", "")
                            strong: slot.current
                            color: slot.current ? Tokens.bg : Tokens.text
                        }

                        // Cuántos eventos tiene, en puntos
                        Row {
                            anchors.horizontalCenter: parent.horizontalCenter
                            y: parent.height / 2 + 12
                            spacing: 2

                            Repeater {
                                model: Agenda.events.filter(event => Calendar.monthOf(Calendar.at(event.at)) === slot.month).slice(0, 3)

                                Rectangle {
                                    required property var modelData

                                    width: 4
                                    height: 4
                                    radius: 2
                                    color: slot.current ? Tokens.bg : Agenda.hue(modelData)
                                }
                            }
                        }

                        MouseArea {
                            id: chose
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                root.go(slot.month);
                                root.years = false;
                            }
                        }
                    }
                }
            }
        }

        Item {
            width: 1
            height: Tokens.gap
        }

        // El día elegido: sus eventos y dónde apuntar otro
        Rectangle {
            x: Tokens.inset
            width: parent.width - Tokens.inset * 2
            height: daylist.height + 28
            radius: root.card
            color: Tokens.surface

            Column {
                id: daylist
                x: 14
                y: 14
                width: parent.width - 28
                spacing: 2

                Item {
                    width: parent.width
                    height: 30

                    Row {
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 8

                        Ink {
                            anchors.verticalCenter: parent.verticalCenter
                            text: root.title(root.selected)
                            strong: true
                        }

                        // Hoy, mañana y ayer dicen además qué día son.
                        Ink {
                            anchors.verticalCenter: parent.verticalCenter
                            text: Calendar.format(root.selected, "dddd d")
                            dim: true
                            visible: Math.abs(root.selected - Calendar.today) <= 1
                        }
                    }

                    Ink {
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        text: root.picked.length === 1 ? "1 evento" : `${root.picked.length} eventos`
                        dim: true
                        visible: root.picked.length > 0
                    }
                }

                Ink {
                    height: 34
                    verticalAlignment: Text.AlignVCenter
                    text: "sin eventos"
                    dim: true
                    visible: root.picked.length === 0
                }

                Repeater {
                    model: root.picked.slice(0, root.shown)

                    Item {
                        id: entry

                        required property var modelData
                        readonly property bool past: modelData.allDay ? Calendar.at(modelData.at) < Calendar.today : modelData.at + Agenda.linger * 60000 < Agenda.now

                        width: daylist.width
                        height: 34
                        opacity: past ? 0.5 : 1

                        HoverHandler {
                            id: over
                        }

                        Rectangle {
                            anchors.verticalCenter: parent.verticalCenter
                            width: 3
                            height: 16
                            radius: 1.5
                            // Lo de Veyl, del acento; lo de fuera, del color de su calendario.
                            color: entry.past ? Tokens.dim : Agenda.hue(entry.modelData)
                        }

                        Ink {
                            id: hour
                            x: 12
                            anchors.verticalCenter: parent.verticalCenter
                            width: 72
                            text: entry.modelData.allDay ? "todo el día" : Agenda.clock(entry.modelData.at)
                            font.pixelSize: entry.modelData.allDay ? Tokens.smallSize : Tokens.fontSize
                            dim: true
                        }

                        Ink {
                            anchors.left: hour.right
                            anchors.leftMargin: 6
                            anchors.right: drop.left
                            anchors.rightMargin: 8
                            anchors.verticalCenter: parent.verticalCenter
                            text: entry.modelData.place ? `${entry.modelData.title} · ${entry.modelData.place}` : entry.modelData.title
                            textFormat: Text.PlainText
                            elide: Text.ElideRight
                        }

                        Icon {
                            id: drop
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            name: "\uE4F6"  // aspa: de una sola letra, no tiene ligadura
                            color: rid.containsMouse ? Tokens.text : Tokens.dim
                            font.pixelSize: 14
                            // Lo de los calendarios de solo lectura se quita en ellos.
                            opacity: over.hovered && Agenda.removable(entry.modelData) ? 1 : 0
                            visible: Agenda.removable(entry.modelData)

                            Behavior on opacity {
                                NumberAnimation { duration: Tokens.fast }
                            }

                            MouseArea {
                                id: rid
                                anchors.fill: parent
                                anchors.margins: -6
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: Agenda.drop(entry.modelData.id)
                            }
                        }
                    }
                }

                // Pulsado, enseña el resto.
                Ink {
                    x: 12
                    height: 24
                    verticalAlignment: Text.AlignVCenter
                    text: `+${root.picked.length - root.shown} más`
                    font.pixelSize: Tokens.smallSize
                    color: more.containsMouse ? Tokens.text : Tokens.dim
                    visible: root.shown < root.picked.length

                    MouseArea {
                        id: more
                        anchors.fill: parent
                        anchors.margins: -4
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.whole = true
                    }
                }

                Item {
                    width: 1
                    height: 6
                }

                // Apuntar otro en este día
                Rectangle {
                    width: parent.width
                    height: 38
                    radius: height / 2
                    color: Tokens.bg
                    border.width: Tokens.line
                    border.color: field.activeFocus ? Tokens.accent : Tokens.border

                    Behavior on border.color {
                        ColorAnimation { duration: Tokens.fast }
                    }

                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.IBeamCursor
                        onClicked: field.forceActiveFocus()
                    }

                    Icon {
                        id: plus
                        x: 10
                        anchors.verticalCenter: parent.verticalCenter
                        name: "plus"
                        font.pixelSize: 14
                        color: field.activeFocus ? Tokens.accent : Tokens.dim
                    }

                    TextInput {
                        id: field
                        anchors.left: plus.right
                        anchors.leftMargin: 8
                        anchors.right: parent.right
                        anchors.rightMargin: 12
                        anchors.verticalCenter: parent.verticalCenter
                        clip: true
                        color: Tokens.text
                        selectionColor: Tokens.accent
                        selectedTextColor: Tokens.bg
                        font.family: Tokens.sans
                        font.pixelSize: Tokens.fontSize
                        Keys.onReturnPressed: root.add()
                        Keys.onEnterPressed: root.add()
                        Keys.onEscapePressed: {
                            clear();
                            focus = false;
                        }

                        Ink {
                            width: parent.width
                            text: "nuevo evento · 16:30 dentista"
                            elide: Text.ElideRight
                            dim: true
                            visible: field.text === ""
                        }
                    }
                }
            }
        }

        // Lo que viene después, fuera del día elegido
        Item {
            width: parent.width
            height: later.height + 8
            visible: root.coming.length > 0

            Column {
                id: later
                x: Tokens.inset + 12
                y: 8
                width: parent.width - x * 2

                Ink {
                    height: 26
                    verticalAlignment: Text.AlignVCenter
                    text: "próximo"
                    font.pixelSize: Tokens.smallSize
                    strong: true
                    dim: true
                }

                Repeater {
                    model: root.coming

                    Item {
                        id: soon

                        required property var modelData

                        width: later.width
                        height: 34

                        Rectangle {
                            anchors.fill: parent
                            anchors.leftMargin: -12
                            anchors.rightMargin: -12
                            radius: height / 2
                            color: jump.containsMouse ? Tokens.surface : "transparent"

                            Behavior on color {
                                ColorAnimation { duration: Tokens.fast }
                            }
                        }

                        // La marca de su calendario, como en la lista del día.
                        Rectangle {
                            id: mark
                            anchors.verticalCenter: parent.verticalCenter
                            width: 3
                            height: 16
                            radius: 1.5
                            color: Agenda.hue(soon.modelData)
                        }

                        Ink {
                            anchors.left: mark.right
                            anchors.leftMargin: 9
                            anchors.right: away.left
                            anchors.rightMargin: 12
                            anchors.verticalCenter: parent.verticalCenter
                            text: soon.modelData.title
                            textFormat: Text.PlainText
                            elide: Text.ElideRight
                        }

                        Ink {
                            id: away
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            text: root.when(soon.modelData)
                            color: soon.modelData === Agenda.next && Agenda.imminent ? Tokens.attention : Tokens.dim
                        }

                        MouseArea {
                            id: jump
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.pick(Calendar.at(soon.modelData.at))
                        }
                    }
                }
            }
        }

        Item {
            width: 1
            height: Tokens.gap
        }

        // El temporizador
        Rectangle {
            id: timer
            x: Tokens.inset
            width: parent.width - Tokens.inset * 2
            height: 52
            radius: root.card
            color: Tokens.surface
            clip: true

            Row {
                x: 14
                anchors.verticalCenter: parent.verticalCenter
                spacing: 10

                Icon {
                    anchors.verticalCenter: parent.verticalCenter
                    name: "timer"
                    color: Countdown.running ? Tokens.accent : Tokens.dim
                }

                Ink {
                    anchors.verticalCenter: parent.verticalCenter
                    text: Countdown.running ? Countdown.exact : "temporizador"
                    strong: Countdown.running
                    dim: !Countdown.running
                }

                Ink {
                    anchors.verticalCenter: parent.verticalCenter
                    text: `de ${Countdown.span} min`
                    dim: true
                    visible: Countdown.running
                }
            }

            Row {
                anchors.right: parent.right
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                spacing: 2

                Knob {
                    icon: "minus"
                    tint: Tokens.dim
                    lit: Tokens.border
                    visible: !Countdown.running
                    onPressed: Countdown.nudge(-1)
                }

                Ink {
                    anchors.verticalCenter: parent.verticalCenter
                    width: 64
                    horizontalAlignment: Text.AlignHCenter
                    text: `${Countdown.preset} min`
                    visible: !Countdown.running

                    WheelHandler {
                        property real spin: 0

                        onWheel: event => {
                            spin += event.angleDelta.y;
                            while (Math.abs(spin) >= 120) {
                                const way = Math.sign(spin);
                                spin -= way * 120;
                                Countdown.nudge(way);
                            }
                        }
                    }
                }

                Knob {
                    icon: "plus"
                    tint: Tokens.dim
                    lit: Tokens.border
                    visible: !Countdown.running
                    onPressed: Countdown.nudge(1)
                }

                Item {
                    width: 4
                    height: 1
                }

                // Empezar es lo tuyo: va del color del acento.
                Knob {
                    icon: Countdown.running ? "stop" : "play"
                    tint: Countdown.running ? Tokens.text : Tokens.bg
                    rest: Countdown.running ? Tokens.border : Tokens.accent
                    lit: Countdown.running ? Tokens.dim : Tokens.text
                    onPressed: Countdown.running ? Countdown.stop() : Countdown.start(Countdown.preset)
                }
            }

            // Lo que lleva hecho
            Rectangle {
                anchors.bottom: parent.bottom
                width: parent.width * Countdown.progress
                height: 2
                color: Tokens.accent
                visible: Countdown.running

                Behavior on width {
                    NumberAnimation { duration: 1000 }
                }
            }
        }
    }
}
