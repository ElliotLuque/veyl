pragma Singleton

import QtQuick
import Quickshell
import Quickshell.Hyprland
import Quickshell.Io
import Quickshell.Services.Pam
import qs

// El bloqueo de la sesión: mientras dura, el compositor solo enseña las
// superficies de base/Lock.qml y nada más recibe teclado ni puntero. Se
// cierra cuando PAM acepta la contraseña. Lo abren el atajo global
// `veyl:lock` (la tecla se la da el compositor), la entrada, el centro de
// control, `veyl lock` y el IPC.
Singleton {
    id: root

    // Bloqueada. Sobrevive a las recargas del shell: si no, cambiar
    // cualquier archivo de Veyl abriría la pantalla.
    property alias locked: kept.locked
    // El monitor que lleva la hora, el campo y lo demás: el que tenía el
    // foco al bloquear. Los otros solo se desenfocan.
    property alias screen: kept.screen
    // Lo que se lleva escrito, compartido por todos los monitores.
    property string secret: ""
    // Comprobando la contraseña.
    readonly property bool checking: pam.active
    // Lo último que salió mal, o lo que PAM quiera decir; vacío si nada.
    property string note: ""
    // Intentos fallidos seguidos: cada uno sacude el campo.
    property int misses: 0
    // Aceptada: las islas se retiran y, tras ellas, se suelta el bloqueo.
    property bool opening: false

    // La configuración de PAM: la de swaylock si existe (no admite contraseña
    // vacía), si no la de login. En NixOS sus módulos pueden estar hechos con
    // otra glibc que Quickshell y no cargar (ninguna contraseña valdría): se
    // copia, apuntando a los módulos del PAM con el que está hecho Quickshell.
    property string config: "login"
    property string configDirectory: "/etc/pam.d"

    signal missed

    PersistentProperties {
        id: kept
        reloadableId: "lock"

        property bool locked: false
        property string screen: ""
    }

    function lock(): void {
        if (locked)
            return;
        secret = "";
        note = "";
        misses = 0;
        opening = false;
        screen = Hyprland.focusedMonitor?.name ?? "";
        locked = true;
    }

    function submit(): void {
        if (!locked || opening || pam.active)
            return;
        if (secret === "") {
            note = "";
            return;
        }
        note = "";
        if (!pam.start())
            fail("No se pudo comprobar la contraseña");
    }

    function fail(text: string): void {
        secret = "";
        note = text;
        misses += 1;
        missed();
    }

    // Aceptada: se suelta cuando las islas ya se han retirado.
    onOpeningChanged: if (opening) release.restart()

    Timer {
        id: release
        interval: Tokens.veil + Tokens.fast
        onTriggered: {
            root.locked = false;
            root.opening = false;
        }
    }

    // Lo que pide PAM: la contraseña escrita; un aviso, al pie.
    PamContext {
        id: pam
        config: root.config
        configDirectory: root.configDirectory

        onPamMessage: {
            if (responseRequired)
                respond(root.secret);
            else if (message)
                root.note = message;
        }

        onCompleted: result => {
            if (result === PamResult.Success) {
                root.secret = "";
                root.note = "";
                root.misses = 0;
                root.opening = true;
            } else if (result === PamResult.MaxTries) {
                root.fail("Demasiados intentos: espera un poco");
            } else {
                root.fail("Contraseña incorrecta");
            }
        }

        onError: error => root.fail("No se pudo comprobar la contraseña")
    }

    Process {
        running: true
        command: ["sh", "-c", `
            name=login
            [ -e /etc/pam.d/swaylock ] && name=swaylock
            own=$(ldd "$(readlink -f /proc/$PPID/exe)" 2>/dev/null | sed -n 's|.*libpam\\.so\\.0 => \\(.*\\)/libpam\\.so\\.0 .*|\\1/security|p')
            if [ -n "$own" ] && [ -e "$own/pam_unix.so" ] && grep -q '^[^#]*/nix/store/[^ ]*-linux-pam-[^/]*/lib/security/' "/etc/pam.d/$name"; then
                dir=\${XDG_RUNTIME_DIR:-/tmp}/veyl/pam
                mkdir -p "$dir" && chmod 700 "$dir"
                sed "s|/nix/store/[^ ]*-linux-pam-[^/]*/lib/security/|$own/|g" "/etc/pam.d/$name" > "$dir/$name.tmp" && mv "$dir/$name.tmp" "$dir/$name"
                echo "$name $dir"
            else
                echo "$name /etc/pam.d"
            fi`]
        stdout: StdioCollector {
            onStreamFinished: {
                const [name, directory] = text.trim().split(" ");
                root.config = name || "login";
                root.configDirectory = directory || "/etc/pam.d";
            }
        }
    }

    // `loginctl lock-session` (lo que usan hypridle y compañía): logind lo
    // anuncia en la sesión, y Veyl bloquea. Necesita gdbus; atado a Veyl,
    // muere con él.
    Process {
        running: true
        command: ["sh", "-c", `
            command -v gdbus >/dev/null || exit 0
            session=$(busctl --system call org.freedesktop.login1 /org/freedesktop/login1 org.freedesktop.login1.Manager GetSession s auto 2>/dev/null | cut -d '"' -f 2)
            [ -n "$session" ] || exit 0
            exec setpriv --pdeathsig TERM gdbus monitor --system --dest org.freedesktop.login1 --object-path "$session"`]
        stdout: SplitParser {
            onRead: line => {
                if (line.includes("org.freedesktop.login1.Session.Lock "))
                    root.lock();
            }
        }
    }

    GlobalShortcut {
        appid: "veyl"
        name: "lock"
        description: "Bloquear la pantalla"
        onPressed: root.lock()
    }

    IpcHandler {
        target: "lock"

        function lock(): void {
            root.lock();
        }

        function get(): bool {
            return root.locked;
        }
    }
}
