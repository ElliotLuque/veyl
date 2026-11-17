import QtQuick
import qs
import qs.base
import qs.state

// Calendarios: los números de semana del calendario de la hora y los
// calendarios de fuera que veyld trae (Google, Outlook, iCloud…, por su
// enlace iCal, o un .ics o una carpeta de vdirsyncer): si se ven, quitarlos
// y apuntar uno nuevo, paso a paso.
Column {
    id: root

    property int armed: -1        // quitar uno pide una segunda pulsación
    property string problem       // lo que falla al añadir uno
    property bool checking: false // mirando si el enlace es de verdad un calendario
    property bool connecting: false // esperando el permiso de Google en el navegador
    property bool own: false        // con un cliente OAuth propio en vez del de Veyl
    // Si se piden el ID y el secreto: cuando Veyl no trae cliente, o se quiere uno propio.
    readonly property bool asking: !connected && (!account.builtin || own)
    readonly property var account: Agenda.google ?? ({})
    readonly property bool connected: !!account.account || account.account === ""

    Component.onCompleted: Daemon.send({ google: "status" })

    Connections {
        target: Agenda

        function onGoogleChanged(): void {
            if (Agenda.google.login !== undefined)
                root.connecting = false;
        }
    }

    // Muestra u oculta un calendario de la cuenta. Al mostrarlo, el mismo por
    // enlace iCal sobra: se quita.
    function showAccount(calendar: var, on: bool): void {
        const ical = source => source.url && (source.url.includes(encodeURIComponent(calendar.id)) || (calendar.primary && source.url.includes(encodeURIComponent(account.account ?? "-"))));
        const rest = Agenda.sources.filter(source => source.google !== calendar.id && !(on && ical(source)));
        if (!on) {
            Agenda.keep(rest);
            return;
        }
        let name = calendar.summary || "Google";
        for (let n = 2, base = name; rest.some(source => source.name === name); n++)
            name = `${base} ${n}`;
        Agenda.keep(rest.concat([{ name, google: calendar.id, color: calendar.color || String(hues[rest.length % hues.length]) }]));
    }

    function connect(): void {
        clientId.commit();
        clientSecret.commit();
        const id = asking ? clientId.value.trim() : "";
        if (asking && !id)
            return;
        connecting = true;
        Agenda.google = Object.assign({}, Agenda.google, { login: undefined });
        Daemon.send({ google: "login", client_id: id, client_secret: asking ? clientSecret.value.trim() : "" });
    }
    property bool opened: false   // el formulario, cuando ya hay alguno
    readonly property bool adding: opened || Agenda.sources.length === 0
    property string service: "google"
    property int hue: -1          // el color elegido; -1, el siguiente libre
    readonly property string typed: urlField.typed.trim()
    readonly property var hues: [Tokens.google, Tokens.github, Tokens.done, Tokens.anthropic, Tokens.openai, Tokens.focus]
    readonly property int pickedHue: hue >= 0 ? hue : Agenda.sources.length % hues.length

    readonly property var services: [
        {
            value: "google",
            label: "Google Calendar",
            name: "Google",
            open: "https://calendar.google.com/calendar/r/settings",
            hint: "https://calendar.google.com/calendar/ical/…/basic.ics",
            steps: ["Abre Google Calendar y entra en Configuración.", "A la izquierda, en «Configuración de mis calendarios», elige el calendario.", "En «Integrar el calendario», copia la «Dirección secreta en formato iCal» y pégala abajo."]
        },
        {
            value: "outlook",
            label: "Outlook o Microsoft 365",
            name: "Outlook",
            open: "https://outlook.live.com/calendar/0/options/calendar/SharedCalendars",
            hint: "https://outlook.live.com/owa/calendar/…/calendar.ics",
            steps: ["Abre Outlook y ve a Configuración → Calendario → Calendarios compartidos.", "En «Publicar un calendario», elige el calendario y «Puede ver todos los detalles», y pulsa Publicar.", "Copia el enlace ICS y pégalo abajo."]
        },
        {
            value: "icloud",
            label: "iCloud",
            name: "iCloud",
            open: "https://www.icloud.com/calendar/",
            hint: "webcal://p…-caldav.icloud.com/published/…",
            steps: ["Abre iCloud Calendario y pulsa el icono de compartir junto al calendario.", "Activa «Calendario público».", "Copia el enlace (empieza por webcal://) y pégalo abajo."]
        },
        {
            value: "web",
            label: "Otro enlace iCal",
            name: "",
            open: "",
            hint: "https://…/calendario.ics o webcal://…",
            steps: ["Busca en tu calendario la opción de suscribirse, publicar o exportar en iCal (ICS).", "Copia el enlace (suele acabar en .ics o empezar por webcal://) y pégalo abajo."]
        },
        {
            value: "file",
            label: "Archivo o carpeta",
            name: "",
            open: "",
            hint: "~/calendarios/casa.ics o una carpeta",
            steps: ["Escribe la ruta de un archivo .ics o de una carpeta con ellos.", "Para que se sincronice también hacia allá (CalDAV), usa vdirsyncer y pon aquí su carpeta."]
        }
    ]
    readonly property var chosen: services.find(entry => entry.value === service) ?? services[0]

    spacing: 22

    Timer {
        id: disarm
        interval: 3000
        onTriggered: root.armed = -1
    }

    // Cómo fue la última vez que se trajo uno.
    function status(source: var, at: int): var {
        return Agenda.calendars.find(calendar => calendar.name === source.name) ?? (source.name ? null : Agenda.calendars[at]) ?? null;
    }

    function ago(time: double): string {
        const minutes = Math.floor((Date.now() - time) / 60000);
        if (minutes < 1)
            return "ahora mismo";
        if (minutes < 60)
            return `hace ${minutes} min`;
        if (minutes < 1440)
            return `hace ${Math.floor(minutes / 60)} h`;
        return `hace ${Math.floor(minutes / 1440)} días`;
    }

    function describe(source: var, last: var): string {
        if (source.on === false)
            return "Oculto";
        if (!last)
            return Daemon.connected ? "Sincronizando…" : "veyld no está en marcha";
        const count = last.count === 1 ? "1 evento" : `${last.count ?? 0} eventos`;
        if (last.error)
            return last.synced ? `${last.error} · ${count} de ${ago(last.synced)}` : last.error;
        return `${count} · ${ago(last.synced)}`;
    }

    // De qué servicio es un enlace.
    function detect(url: string): string {
        if (/^[~\/]|^file:\/\//.test(url))
            return "file";
        if (/calendar\.google\.com|googleusercontent/.test(url))
            return "google";
        if (/outlook\.(live|office|office365)\.com|office365\.com/.test(url))
            return "outlook";
        if (/icloud\.com/.test(url))
            return "icloud";
        return /^(https?|webcals?):\/\//.test(url) ? "web" : "";
    }

    // Lo que se le dice del enlace mientras lo escribe: [texto, si vale].
    function check(url: string): var {
        if (!url)
            return ["Pega aquí el enlace", false];
        const kind = detect(url);
        if (!kind && /^[^\s\/]+@[^\s\/]+$/.test(url))
            return ["Eso es el ID del calendario, no su enlace. Copia la «Dirección secreta en formato iCal», justo debajo en la misma página", false];
        if (!kind)
            return ["No es un enlace (https:// o webcal://) ni una ruta", false];
        if (kind === "google" && !url.includes("/ical/"))
            return ["Ese es el enlace para compartir o ver el calendario. En «Integrar el calendario», copia la «Dirección secreta en formato iCal» (acaba en basic.ics)", false];
        if (kind === "google" && url.includes("/public/"))
            return ["Es la dirección pública: solo funciona si el calendario es público. Mejor la secreta", true];
        if (Agenda.sources.some(source => source.url === url))
            return ["Ese calendario ya está", false];
        return [kind === "file" ? "Archivo o carpeta" : `Enlace de ${services.find(entry => entry.value === kind).label}`, true];
    }

    readonly property var verdict: check(typed)

    // El nombre que se propone: el del servicio o el del archivo.
    function suggest(url: string): string {
        if (service === "file" || detect(url) === "file")
            return url.replace(/\/+$/, "").split("/").pop().replace(/\.ics$/i, "") || "Calendario";
        return chosen.name || "Calendario";
    }

    function add(): void {
        if (checking)
            return;
        urlField.commit();
        nameField.commit();
        if (!verdict[1]) {
            problem = verdict[0];
            return;
        }
        const url = typed;
        if (detect(url) === "file") {
            follow(url);
            return;
        }
        // Antes de apuntarlo, se mira que de verdad sea un calendario.
        checking = true;
        const request = new XMLHttpRequest();
        request.onreadystatechange = () => {
            if (request.readyState !== XMLHttpRequest.DONE || !checking)
                return;
            checking = false;
            disarmCheck.stop();
            const why = refuse(request.status, request.responseText ?? "");
            // Sin el enlace, que es secreto.
            console.info(`calendarios: comprobado (${detect(url)}, HTTP ${request.status}): ${why || "vale"}`);
            if (why)
                problem = why;
            else if (url === typed)
                follow(url);
        };
        request.open("GET", url.replace(/^webcals?:\/\//, "https://"));
        request.send();
        disarmCheck.restart();
    }

    // Por qué lo traído no es un calendario; vacío, si lo es.
    function refuse(status: int, text: string): string {
        if (status === 0)
            return "No se ha podido descargar: ¿hay conexión y el enlace está completo?";
        if (status === 404)
            return "Ese enlace no existe (404): revisa que esté copiado entero";
        if (status === 401 || status === 403)
            return "El servicio no deja leerlo: copia la dirección secreta, no la pública";
        if (status >= 400)
            return `El servidor contesta con un error (${status})`;
        if (!text.includes("BEGIN:VCALENDAR"))
            return /<html/i.test(text) ? "Ese enlace abre una página web, no un calendario .ics" : "Lo que hay en ese enlace no es un calendario .ics";
        return "";
    }

    function follow(url: string): void {
        let name = nameField.value.trim() || suggest(url);
        // Dos con el mismo nombre se confundirían: «Google 2».
        for (let n = 2, base = name; Agenda.sources.some(source => source.name === name); n++)
            name = `${base} ${n}`;
        problem = Agenda.follow(name, url, String(hues[pickedHue]));
        if (problem)
            return;
        nameField.clear();
        urlField.clear();
        hue = -1;
        opened = false;
    }

    Timer {
        id: disarmCheck
        interval: 20000
        onTriggered: {
            if (!root.checking)
                return;
            root.checking = false;
            root.problem = "No contesta: el servidor tarda demasiado. Prueba otra vez";
        }
    }

    Group {
        title: "Vista"

        Option {
            glyph: "calendar"
            title: "Números de semana"
            text: "En el calendario que se abre desde la hora"

            Toggle {
                on: Settings.weekNumbers
                onToggled: on => Settings.weekNumbers = on
            }
        }

        Option {
            glyph: Settings.eventWarning > 0 ? "bell-ringing" : "bell-slash"
            title: "Avisar antes de un evento"
            text: "La isla de la hora se abre con su hora y su título, y luego lo deja delante de la hora hasta que empieza"

            Select {
                width: 200
                options: [0, 1, 5, 10, 15, 30, 60].map(value => ({ value, label: value === 0 ? "No avisar" : value === 60 ? "1 hora antes" : `${value} min antes` }))
                value: Settings.eventWarning
                onPicked: value => Settings.eventWarning = value
            }
        }
    }

    Group {
        title: "Eventos nuevos"
        note: Agenda.writable.length === 0 ? "Para apuntar en Google, conecta tu cuenta (abajo) y muestra el calendario." : ""

        Option {
            glyph: "calendar-plus"
            title: "Apuntar en"
            text: "Lo que apuntas en el calendario, por voz o con veyl event"

            Select {
                width: 300
                options: [{ value: "", label: "Veyl (solo este ordenador)" }].concat(Agenda.writable.map(name => ({ value: name, label: name })))
                value: Agenda.target
                onPicked: value => Settings.calendarTarget = value
            }
        }
    }

    Group {
        title: "Cuenta de Google"
        note: root.connected ? "Los calendarios de la cuenta se leen al momento y, en los tuyos, Veyl puede apuntar y borrar." : root.asking ? "Google pide que el permiso pase por un cliente OAuth. Con uno tuyo, se crea una vez en Google Cloud." : "Se abre el navegador para elegir tu cuenta y dar permiso. Veyl solo ve tus calendarios y sus eventos."

        // Conectada: la cuenta y sus calendarios.
        Option {
            glyph: "google-logo"
            tone: Tokens.google
            title: root.account.account || "Cuenta de Google"
            text: root.account.error ? root.account.error : `${(root.account.calendars ?? []).length} calendarios`
            visible: root.connected

            Button {
                text: "Desconectar"
                onPressed: Daemon.send({ google: "logout" })
            }
        }

        Repeater {
            model: root.connected ? (root.account.calendars ?? []) : []

            Option {
                id: owned

                required property var modelData
                readonly property bool shown: Agenda.sources.some(source => source.google === modelData.id)

                glyph: "calendar"
                tone: owned.modelData.color || Tokens.dim
                title: owned.modelData.summary
                text: owned.modelData.writable ? (Agenda.target && Agenda.sources.find(source => source.google === owned.modelData.id)?.name === Agenda.target ? "Aquí se apunta lo nuevo" : "Puedes apuntar en él") : "Solo lectura"

                Toggle {
                    on: owned.shown
                    onToggled: on => root.showAccount(owned.modelData, on)
                }
            }
        }

        // Sin conectar: cómo se consigue el cliente OAuth.
        Item {
            width: parent.width
            height: oauthSteps.height + 28
            visible: root.asking

            Column {
                id: oauthSteps
                x: Tokens.pad
                y: 14
                width: parent.width - Tokens.pad * 2
                spacing: 10

                Repeater {
                    model: [
                        { text: "En Google Cloud, crea un proyecto y activa la API de Google Calendar.", open: "https://console.cloud.google.com/apis/library/calendar-json.googleapis.com", button: "Activar la API" },
                        { text: "En «Google Auth Platform», configura la marca (tipo Externo, con tu correo) y en «Público» pulsa «Publicar la aplicación»: si se queda en prueba, el permiso caduca cada 7 días.", open: "https://console.cloud.google.com/auth/audience", button: "Abrir Público" },
                        { text: "En «Clientes», crea un cliente OAuth de tipo «Aplicación de escritorio» y copia aquí su ID y su secreto.", open: "https://console.cloud.google.com/auth/clients", button: "Abrir Clientes" },
                        { text: "Pulsa «Conectar», elige tu cuenta y da permiso. Como la aplicación es tuya y no está verificada, Google avisa: «Configuración avanzada» → «Ir a…».", open: "", button: "" }
                    ]

                    Row {
                        id: oauthStep

                        required property var modelData
                        required property int index

                        width: oauthSteps.width
                        spacing: 12

                        Rectangle {
                            width: 22
                            height: 22
                            radius: 11
                            color: Tokens.bg

                            Ink {
                                anchors.centerIn: parent
                                text: oauthStep.index + 1
                                font.pixelSize: Tokens.tinySize + 1
                                dim: true
                            }
                        }

                        Column {
                            width: oauthSteps.width - 34
                            spacing: 8

                            Ink {
                                width: parent.width
                                text: oauthStep.modelData.text
                                font.pixelSize: Tokens.smallSize
                                wrapMode: Text.Wrap
                            }

                            Button {
                                anchors.verticalCenter: undefined
                                icon: "arrow-square-out"
                                text: oauthStep.modelData.button
                                visible: oauthStep.modelData.open !== ""
                                onPressed: Qt.openUrlExternally(oauthStep.modelData.open)
                            }
                        }
                    }
                }
            }
        }

        Option {
            glyph: "identification-card"
            title: "ID de cliente"
            visible: root.asking

            Field {
                id: clientId
                width: 400
                placeholder: "…apps.googleusercontent.com"
                onCommitted: text => value = text
            }
        }

        Option {
            glyph: "key"
            title: "Secreto de cliente"
            visible: root.asking

            Field {
                id: clientSecret
                width: 400
                secret: true
                placeholder: "GOCSPX-…"
                onCommitted: text => value = text
            }
        }

        Option {
            glyph: root.connecting ? "hourglass" : root.account.login && root.account.login !== "ok" ? "warning" : "google-logo"
            tone: root.account.login && root.account.login !== "ok" && !root.connecting ? Tokens.attention : Tokens.dim
            title: root.connecting ? "Esperando el permiso en el navegador…" : root.account.login && root.account.login !== "ok" ? "No se ha conectado" : root.asking ? "" : "Tu cuenta de Google"
            text: root.connecting ? "Se abre una pestaña: elige tu cuenta y da permiso" : root.account.login && root.account.login !== "ok" ? root.account.login : ""
            visible: !root.connected

            Button {
                icon: "google-logo"
                text: root.connecting ? "Esperando…" : root.asking ? "Conectar" : "Conectar con Google"
                strong: !root.connecting && (!root.asking || clientId.typed.trim() !== "")
                onPressed: if (!root.connecting) root.connect()
            }
        }
    }

    Group {
        visible: !root.connected && !!root.account.builtin

        Option {
            glyph: "wrench"
            title: "Usar mi propio cliente OAuth"
            text: "Para quien prefiera su propio proyecto de Google Cloud"

            Toggle {
                on: root.own
                onToggled: on => root.own = on
            }
        }
    }

    Group {
        title: "Tus calendarios"
        note: "Se sincronizan cada 15 minutos. Los que van por enlace solo se leen; lo que apuntas va a donde diga «Apuntar en»."
        visible: Agenda.sources.length > 0

        Repeater {
            model: Agenda.sources

            Option {
                id: row

                required property var modelData
                required property int index
                readonly property var last: root.status(modelData, index)
                readonly property string label: modelData.name || last?.name || modelData.url

                glyph: row.last?.error ? "warning" : "calendar"
                tone: row.last?.error ? Tokens.attention : modelData.on === false ? Tokens.dim : Agenda.tone(row.last?.name ?? row.label)
                title: row.label
                text: root.describe(row.modelData, row.last)

                Toggle {
                    on: row.modelData.on !== false
                    onToggled: on => Agenda.keep(Agenda.sources.map((source, at) => at === row.index ? Object.assign({}, source, { on }) : source))
                }

                Button {
                    icon: "trash"
                    text: root.armed === row.index ? "Quitar" : ""
                    tone: root.armed === row.index ? Tokens.attention : "transparent"
                    onPressed: {
                        if (root.armed !== row.index) {
                            root.armed = row.index;
                            disarm.restart();
                            return;
                        }
                        root.armed = -1;
                        Agenda.keep(Agenda.sources.filter((_, at) => at !== row.index));
                    }
                }
            }
        }

        Option {
            glyph: "arrows-clockwise"
            title: "Sincronizar ahora"
            text: Agenda.synced > 0 ? `Última vez ${root.ago(Agenda.synced)}` : ""

            Button {
                text: "Sincronizar"
                onPressed: Daemon.send({ calendars: "sync" })
            }
        }

        Option {
            glyph: "plus"
            tone: Tokens.accent
            title: "Añadir un calendario"
            pressable: true
            visible: !root.adding
            onClicked: root.opened = true
        }
    }

    Group {
        title: "Añadir un calendario"
        note: root.service === "file" ? "" : "El enlace es privado: quien lo tenga ve tu calendario. Si se filtra, se puede cambiar por otro desde el mismo sitio."
        visible: root.adding

        Option {
            glyph: "plugs"
            title: "Desde"

            Select {
                width: 280
                options: root.services.map(entry => ({ value: entry.value, label: entry.label }))
                value: root.service
                onPicked: value => {
                    root.service = value;
                    root.problem = "";
                }
            }
        }

        // Los pasos para conseguir el enlace en ese servicio.
        Item {
            width: parent.width
            height: steps.height + 28

            Rectangle {
                x: Tokens.pad
                width: parent.width - Tokens.pad * 2
                height: Tokens.line
                color: Tokens.border
            }

            Column {
                id: steps
                x: Tokens.pad
                y: 14
                width: parent.width - Tokens.pad * 2
                spacing: 10

                Repeater {
                    model: root.chosen.steps

                    Row {
                        id: step

                        required property string modelData
                        required property int index

                        width: steps.width
                        spacing: 12

                        Rectangle {
                            width: 22
                            height: 22
                            radius: 11
                            color: Tokens.bg

                            Ink {
                                anchors.centerIn: parent
                                text: step.index + 1
                                font.pixelSize: Tokens.tinySize + 1
                                dim: true
                            }
                        }

                        Ink {
                            anchors.verticalCenter: parent.verticalCenter
                            width: steps.width - 34
                            text: step.modelData
                            font.pixelSize: Tokens.smallSize
                            wrapMode: Text.Wrap
                        }
                    }
                }

                Button {
                    anchors.verticalCenter: undefined
                    x: 34
                    icon: "arrow-square-out"
                    text: `Abrir ${root.chosen.name}`
                    visible: root.chosen.open !== ""
                    onPressed: Qt.openUrlExternally(root.chosen.open)
                }
            }
        }

        Option {
            glyph: root.typed && root.verdict[1] ? "check" : "link"
            tone: root.typed && root.verdict[1] ? Tokens.done : Tokens.dim
            title: root.service === "file" ? "Ruta" : "Enlace"
            text: root.typed && root.verdict[1] ? root.verdict[0] : root.service === "file" ? "Un archivo .ics o una carpeta" : "El que acabas de copiar"

            Field {
                id: urlField
                width: 400
                placeholder: root.chosen.hint
                onTypedChanged: {
                    root.problem = "";
                    // Lo pegado dice de dónde es.
                    const kind = root.detect(typed.trim());
                    if (kind && kind !== root.service && !(kind === "web" && root.service !== "file"))
                        root.service = kind;
                }
                onCommitted: text => value = text
            }
        }

        // Por qué no vale lo pegado, bien a la vista.
        Option {
            glyph: "warning"
            tone: Tokens.attention
            title: "Así no se puede añadir"
            text: root.problem || root.verdict[0]
            visible: root.problem !== "" || (root.typed !== "" && !root.verdict[1])
        }

        Option {
            glyph: "tag"
            title: "Nombre"
            text: "Como saldrá en los ajustes"

            Field {
                id: nameField
                width: 400
                placeholder: root.suggest(root.typed)
                onCommitted: text => value = text
            }
        }

        Option {
            glyph: "palette"
            title: "Color"
            text: "La marca de sus eventos en el calendario"

            Row {
                anchors.verticalCenter: parent.verticalCenter
                spacing: 8

                Repeater {
                    model: root.hues

                    Rectangle {
                        id: swatch

                        required property color modelData
                        required property int index
                        readonly property bool picked: root.pickedHue === index

                        width: 26
                        height: 26
                        radius: 13
                        color: "transparent"
                        border.width: 2
                        border.color: picked ? Tokens.text : "transparent"

                        Rectangle {
                            anchors.centerIn: parent
                            width: 18
                            height: 18
                            radius: 9
                            color: swatch.modelData
                        }

                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.hue = swatch.index
                        }
                    }
                }
            }
        }

        Option {
            title: ""

            Button {
                text: "Cancelar"
                visible: Agenda.sources.length > 0
                onPressed: {
                    root.opened = false;
                    root.problem = "";
                    urlField.clear();
                    nameField.clear();
                }
            }

            Button {
                icon: "plus"
                text: root.checking ? "Comprobando…" : "Añadir"
                strong: root.verdict[1] && !root.checking
                onPressed: root.add()
            }
        }
    }
}
