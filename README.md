# Veyl · estado base

Lo que se ve cuando no pasa nada: la hora, el contexto y, solo si ocurre, lo que te necesita. Sin barra: dos islas que salen de las esquinas de arriba, pegadas a los bordes de la pantalla como muescas del marco, y que cambian de forma según lo que tengan que enseñar. La especificación está en [BRIEF.md](BRIEF.md).

Hecho con [Quickshell](https://quickshell.org) sobre Hyprland (configuración Lua).

## Arrancar

```sh
bin/veyl          # o: qs -p .
```

Veyl se apoya en **veyld**, su daemon (en Rust, en [veyld/](veyld)): lleva el diario de lo que miras, encuentra los agentes, entiende las órdenes y es el servidor de notificaciones. Hay que compilarlo una vez:

```sh
veyld/build.sh                        # con una NVIDIA, con la voz en la GPU; o, con Nix, ver «Con Nix»
```

Veyl lo arranca si no está en marcha (el de `$PATH` o, si no hay, el de `veyld/target/release/`) y vive aparte: una recarga del shell no lo para, y se va cuando se va Hyprland. Sin él, Veyl funciona igual salvo los agentes, el diario, las órdenes dichas con palabras y las notificaciones.

Al arrancar, Veyl le pide a Hyprland en caliente lo que el brief fija para las ventanas: huecos, borde de 1 px (con el color de foco del esquema), radio, sombra, desenfoque, la tecla del vistazo (Super) y las de contestar a un agente (Super+Y, Super+A, Super+N y Super+O). No toca ningún archivo de configuración: lo pone en caliente y lo vuelve a poner cada vez que Hyprland recarga la suya (si no, Super dejaría de abrir el vistazo).

### Con Nix

El repositorio es un flake: `packages.veyl` (el shell, con su Quickshell 0.2.1 y su veyld; `nix run` lo arranca), `packages.veyld`, `overlays.default`, un `devShell` para compilar y pasar las comprobaciones, y los módulos `homeManagerModules.veyl` y `nixosModules.veyl`, que dan lo mismo: `programs.veyl`.

```nix
# flake.nix: inputs.veyl.url = "github:…/veyl";  e importar inputs.veyl.homeManagerModules.veyl
programs.veyl = {
  enable = true;
  acceleration = "cuda";             # la voz en la GPU (o "vulkan"); sin nada, en la CPU
  settings = {                       # las claves de settings.json
    scheme = "living-code-night";
    wallpaper = "${inputs.living-code}/night/switzerland.png";
    webSearch = "https://google.com/search?q=%s";
    brain.provider = "claude-code";
    voice.speed = 1.3;
    monitors."HDMI-A-1" = { mode = "3840x2160@60.00"; scale = 1.5; position = "0x0"; };
  };
  schemes.mio = { bg = "#05080b"; accent = "#00a692"; };  # ~/.config/veyl/schemes/mio.json (solo Home Manager)
  systemd.enable = true;             # arrancarlo con la sesión gráfica, si no lo hace el compositor (solo Home Manager)
};
```

Lo declarado manda: al reconstruir, Veyl lo ve llegar (en marcha, en unos segundos) y lo pone encima de lo que hubiera en `settings.json`; lo que deja de declararse vuelve a lo de fábrica, salvo que lo hayas cambiado desde entonces. Lo que no declaras sigue siendo cosa de los ajustes. El módulo deja lo declarado en `~/.config/veyl/nix.json` (el de NixOS, en `/etc/veyl/nix.json`, para todos; gana el de Home Manager), junto con los archivos en los que se declaró `programs.veyl`.

**Que no se separen.** Si cambias en los ajustes algo que no coincide con lo declarado (o que no está declarado y no es lo de fábrica), al pie de la isla sale cuántos ajustes no coinciden con tu configuración de Nix, con «Guardar en Nix»; en la sección **Nix**, uno a uno, con lo que vale ahora y lo que dice tu configuración, para guardarlo o deshacerlo. Guardar lo escribe veyld (`veyld nix write`) en el mismo archivo en el que declaraste `programs.veyl`, sin reescribirlo: cambia solo ese valor, o lo añade en el bloque más hondo que ya exista, con su sangría (escribas `programs.veyl.settings.x` con puntos, en bloques anidados, dentro de `home-manager.users.<tú>`, de un `lib.mkIf` o de una función de módulo). Lo que calcula una expresión (`"${inputs.x}/a.png"`, `cfg.algo`) no se pisa: dice que se cambie a mano. Lo guardado queda «falta reconstruir» hasta que lo haces; «Reconstruir» abre una terminal con la orden (`nh os switch`, `sudo nixos-rebuild switch` o `home-manager switch`, según lo que haya; `programs.veyl.nix.rebuild` la fija).

Con un flake, el sistema de módulos da el archivo dentro del almacén; veyld busca el mismo camino en la carpeta del flake: `programs.veyl.nix.flake`, `$NH_FLAKE`, `$FLAKE`, `/etc/nixos` o `~/.config/home-manager`. `programs.veyl.nix.file` fija el archivo a mano. Sin el módulo pero en NixOS, la sección **Nix** copia lo que tienes como un bloque `programs.veyl = { … };` para pegarlo en tu configuración.

`veyld nix` dice dónde está declarado; `veyld nix show '<ajustes en JSON>'`, cómo se escribirían.


## Qué muestra

| Dónde | Qué | Cuándo |
| --- | --- | --- |
| Detrás de todo | Fondo de pantalla, o el color de fondo del esquema | Siempre |
| Arriba a la derecha | Hora | Siempre, en todos los monitores |
| En la misma isla, delante de la hora | Micro, cámara, pantalla compartida, batería baja, sin red, evento inminente, temporizador | Solo mientras ocurre |
| Bajo la hora | El almanaque: calendario, eventos, lo próximo y el temporizador | Con el puntero sobre la hora |
| Bajo la hora | El aviso de un evento: a qué hora es, su título, dónde y cuánto falta | Unos 9 s cuando se acerca (10 min antes de fábrica; **Ajustes → Calendarios → Avisar antes de un evento**, o `eventWarning` en `settings.json`, 0 = nunca); luego sigue delante de la hora hasta 2 min después de empezar |
| Arriba a la izquierda | Niveles, contexto y su tira de ventanas | Siempre, en el monitor con foco |
| En el borde izquierdo, en su tercio de arriba | Los agentes en marcha, uno por fila | Mientras haya alguno, en el monitor con foco |
| Arriba en el centro | Las notificaciones que acaban de llegar, apiladas | Mientras duran, en el monitor con foco |
| Arriba en el centro | La entrada | Con `Super+Espacio`, en el monitor con foco |
| Arriba en el centro | El centro de control | Con el puntero en el centro del borde de arriba |
| En el centro del borde derecho | Volumen o brillo, con su barra; la rueda encima lo sigue moviendo | Al cambiar, unos 1,6 s, en el monitor con foco (también en pantalla completa) |
| En sitio | Fecha, volumen, batería exacta y la torre de niveles | Con Super mantenido |

En pantalla completa solo queda, en la esquina superior derecha, una isla con las excepciones y el punto de «te necesita».

### Niveles y tiras

Un contexto es un espacio de trabajo de Hyprland: su nombre si lo tiene, su número si no. Los de un monitor son **niveles**, apilados con el primero arriba, y las ventanas de cada nivel forman una **tira** horizontal (disposición `scrolling`).

La isla de la izquierda lo cuenta de un vistazo:

- **Marcas de nivel**, una por nivel: larga y del color del acento la del actual, gris si el nivel tiene ventanas y apagada si está vacío.
- **El contexto**, que rueda hacia arriba o hacia abajo al cambiar de nivel.
- **La tira** del nivel en miniatura: cada ventana es una pieza, el marco es lo que cabe en pantalla y lo que queda fuera se ve más tenue. La ventana activa es la clara.

Con Super mantenido la isla se abre en la **torre**: todos los niveles del monitor, cada uno con su tira y las aplicaciones que tiene abiertas. Como Super es también la tecla con la que se cambia de nivel, la torre acompaña mientras te mueves.

Con hyprsplit los niveles son los espacios fijos de cada monitor, numerados desde 1; sin él, los espacios que existan.

### Vistas

Una **vista** es un escritorio con nombre hecho con lo que ya tienes abierto: «crea la vista estudio de uni» y Veyl junta en un nivel nuevo, que nace al vuelo con ese nombre, las ventanas que son de eso (el campus virtual, los apuntes, la terminal de la práctica) y deja donde estaban las demás (YouTube, la música). Es como una vista de una base de datos: se guarda la consulta, no el resultado. De cada ventana se apunta su aplicación y unas palabras que la reconocen aunque su título cambie (el sitio, la asignatura, la carpeta), y con ellas la vista se puede **recrear** otro día: trae las ventanas que casan, estén en el nivel que estén, y vuelve a abrir las que faltan (la terminal en su carpeta, el navegador en su página), directas a la vista aunque mientras tanto te vayas a otro nivel.

Qué ventanas van lo decide el cerebro si lo hay (una pregunta corta, sin herramientas, mirando títulos, carpetas y páginas); mientras piensa, sale la isla de «Pensando». Sin cerebro, o si falla, las que nombran alguna palabra de la vista y, si ninguna, las del nivel en el que estás.

Las vistas abiertas son niveles más, debajo de los numerados, con su nombre en la isla y su inicial en la torre. Se manejan con palabras o desde la entrada (orden «Vistas»: Intro va a la vista o la recrea si está cerrada; Ctrl+K, el resto; escribir un nombre que no existe la crea):

| Dicho | Hace |
| --- | --- |
| crea la vista estudio de uni | Una vista nueva con lo de eso que haya abierto |
| guarda este nivel como la vista trabajo | Una vista con todo lo de este nivel |
| abre la vista trabajo | Va a ella; si está cerrada, la recrea |
| recrea la vista trabajo | Trae lo suyo y abre lo que falta, aunque esté abierta |
| guarda esta vista | Lo que tiene ahora pasa a ser la vista |
| deshaz esta vista | Cada ventana vuelve al nivel del que vino (la vista queda guardada) |
| olvida la vista trabajo | La borra (pide confirmación) |

Se guardan en `$XDG_STATE_HOME/veyl/views.json`.

## Almanaque

Con el puntero sobre la hora, su isla se abre hacia abajo sin que la hora se mueva:

- **La fecha** entera, junto a la hora.
- **El mes**, con hoy en un círculo del color del acento. Las semanas son una tira continua que la rueda desliza en vertical, como los niveles, sin saltos de mes: una semana por muesca, al píxel con un panel táctil, y al soltar se asienta en la semana más cercana. El mes a la vista es el que ocupa el centro: da el título y queda claro, y los demás días se atenúan. Las flechas saltan al mes anterior o siguiente y «hoy» vuelve. Pulsar el nombre del mes abre el año entero, para saltar lejos (ahí la rueda pasa de año en año). Los días con eventos llevan un punto por evento, hasta tres. Con `"weekNumbers": true` en `settings.json`, también los números de semana.
- **El día elegido** (hoy, al abrirse) con sus eventos: hora o «todo el día», el aspa para borrarlo y un campo para apuntar otro: «16:30 dentista» o «dentista 16:30»; sin hora, es de todo el día.
- **Lo próximo**: los tres siguientes fuera de ese día, con cuánto falta. Pulsar uno lleva a su día.
- **El temporizador**: se elige la duración (con − y +, o con la rueda) y se pone en marcha. Mientras corre va en la isla de la hora, junto a las excepciones, y al acabar avisa con una notificación que no caduca y suena. Sobrevive a una recarga del shell.

Se cierra al sacar el puntero, salvo que estés escribiendo; Esc deja el campo.

Los eventos se guardan en `$XDG_STATE_HOME/veyl/agenda.json`: una lista de `{ "title", "day": "AAAA-MM-DD", "time": "HH:MM" }` (sin `time`, todo el día) que también se puede escribir desde fuera; Veyl la relee al cambiar.

### Calendarios de fuera

Los de Google, Outlook, iCloud, Nextcloud o cualquiera que dé un enlace iCalendar se ven en el almanaque junto a lo tuyo, con la marca del color de su calendario y sin aspa: se quitan en su calendario. Se apuntan en los ajustes (**Calendarios**) o en `~/.config/veyl/calendars.json`:

```json
[
  { "name": "Uni", "url": "https://calendar.google.com/calendar/ical/…/private-…/basic.ics", "color": "#7aa2f7" },
  { "name": "Casa", "url": "~/.local/share/calendars/casa" },
  { "name": "Festivos", "url": "webcal://…", "on": false }
]
```

`url` es una dirección `https://` o `webcal://`, un `.ics` o una carpeta de ellos; `color`, si no se pone, sale del esquema; `"on": false` lo oculta sin olvidarlo. veyld los trae al arrancar, cada 15 minutos y en cuanto cambia la lista («Sincronizar» en los ajustes, o `veyl event calendars sync`, los trae ya; `veyld calendars` lo hace sin el daemon y dice cómo fue). Despliega lo que se repite (semanal, mensual, «el último viernes», con sus excepciones y las veces movidas) entre dos meses atrás y un año adelante, pasa cada hora a la zona de aquí y lo deja en `$XDG_STATE_HOME/veyl/events.json`, sin las direcciones. Si uno no se puede traer, se queda con lo de la última vez y los ajustes dicen por qué. «¿Qué tengo mañana?» y el cerebro también los ven.

**Apuntar en Google.** Con la cuenta de Google conectada (**Ajustes → Calendarios → Cuenta de Google**, o `veyld google login <id> <secreto>`), sus calendarios se leen por la API, al momento y sin el retraso del enlace iCal, y en los tuyos se puede escribir: en «Eventos nuevos → Apuntar en» se elige dónde va todo lo que apuntes (desde el almanaque, por voz o con `veyl event`): en Veyl, solo en este ordenador, o en uno de esos calendarios (`calendarTarget` en `settings.json`). Lo apuntado se ve al momento y llega a Google en segundos; si Google no contesta, se queda en Veyl y avisa. Sus eventos llevan aspa y se borran allí. Con el cliente OAuth de Veyl (ver abajo) basta con pulsar «Conectar con Google», elegir la cuenta y dar permiso; una compilación sin él, o quien lo prefiera («Usar mi propio cliente OAuth»), usa uno suyo de Google Cloud: se activa la API de Google Calendar, se publica la aplicación (en prueba, el permiso caduca cada 7 días) y se crea un cliente de tipo «Aplicación de escritorio»; los ajustes lo explican paso a paso. Veyl solo pide ver la lista de calendarios y leer y escribir sus eventos (`calendar.calendarlist.readonly` y `calendar.events`). Al conectar, veyld abre el navegador, recibe la vuelta en `127.0.0.1` (con PKCE) y guarda el permiso en `~/.config/veyl/keys.json`, solo para ti. En `calendars.json` esos calendarios van como `{ "name": "Elliot", "google": "primary" }` (o el ID del calendario).

**El cliente de Google de Veyl** (para quien distribuya Veyl). Es lo que hace que conectar sea un solo botón, como en el móvil: se crea una vez en Google Cloud (API de Google Calendar activada; cliente OAuth de tipo «Aplicación de escritorio»; en «Google Auth Platform», la marca con la página de Veyl, su política de privacidad y su dominio verificado, y los dos permisos de arriba) y se manda a verificar. Son permisos «sensibles»: Google revisa la aplicación (justificación de cada permiso y un vídeo de cómo se usa), pero no pide la auditoría de seguridad de los «restringidos». Hasta que esté verificada sirve para 100 usuarios, con el aviso de «aplicación no verificada». El ID y el secreto se ponen al compilar: `VEYL_GOOGLE_CLIENT_ID=… VEYL_GOOGLE_CLIENT_SECRET=… ./build.sh`. En una aplicación de escritorio el secreto no protege nada (lo hace PKCE), así que puede ir en el binario.

**Cómo funciona la sincronización por enlace.** Es de solo lectura, sin cuentas ni contraseñas: la dirección iCal es un archivo que el servicio regenera y que cualquiera con el enlace puede leer, así que es secreta (`calendars.json` queda solo para ti; si se filtra, Google deja restablecerla). Lo que apuntas no va a esos calendarios (para eso, la cuenta de Google). Google tarda en reflejar los cambios en ese archivo (de minutos a varias horas): para algo al momento, o para escribir en ellos, sincronízalos con [vdirsyncer](https://vdirsyncer.pimutils.org) (CalDAV, en los dos sentidos) y pon aquí su carpeta.

- **Google Calendar**: en calendar.google.com, Configuración → el calendario (en «Configuración de mis calendarios») → *Integrar el calendario* → **Dirección secreta en formato iCal**. Una por calendario.
- **Outlook / Microsoft 365**: Configuración → Calendario → Calendarios compartidos → *Publicar un calendario* → «Puede ver todos los detalles» → el enlace ICS.
- **iCloud**: en la app Calendario, compartir el calendario como *Calendario público* y copiar el enlace (`webcal://`).
- **Nextcloud / CalDAV**: el enlace de suscripción del calendario (… → *Copiar enlace de suscripción*), o vdirsyncer.

## Notificaciones

veyld es el servidor de notificaciones (`org.freedesktop.Notifications`): las recibe, las guarda, decide cuáles salen como aviso y cuándo caducan, y le cuenta a cada programa lo que haces con las suyas. Veyl solo las pinta. Si ya hay otro servidor en marcha, veyld espera turno y toma el relevo cuando ese se va. La que llega nace del centro del borde de arriba, en una isla que toma el tamaño de lo que trae; si el centro está ocupado (la entrada, los ajustes o el centro de control), sale a su derecha, también del borde de arriba, y vuelve al centro cuando se libera:

- **De quién es:** nombre del programa y su icono, tan alto como el texto que lo acompaña; si trae imagen (un avatar, una carátula), la imagen, y el icono pasa junto al nombre. Sin ninguno, un icono según su categoría.
- **Título y cuerpo**, con formato, enlaces e imágenes. El título ocupa una línea y el cuerpo dos; lo que no cabe se corta con «…».
- **Avance** (pista `value`), **respuesta en línea** y **acciones**, como botones (con su icono si el programa los pide).
- **Urgencia:** una crítica se tiñe y no caduca; una baja va apagada.
- **Sonido** (`sound-file`, `sound-name`), salvo que pida silencio.

Pulsarla es su acción por defecto o, si no tiene, descartarla; el botón derecho o el aspa la descartan siempre. Si llegan varias seguidas se apilan desde el mismo sitio: la última sale arriba y empuja a las demás hacia abajo. Caben tres a la vista; la de abajo lleva un `+N` por las que esperan detrás. Cada aviso caduca cuando él pide o, si no pide nada, a los 6 s; con el puntero encima o escribiendo una respuesta, esperan todos. Uno que caduca no se pierde: se queda en Avisos, en el centro de control, y su programa aún recibe lo que hagas con él. Una pasajera (pista `transient`) sí se va.

Las notificaciones se guardan en `$XDG_STATE_HOME/veyl/notifications.db` (las imágenes que llegan en datos, en `$XDG_CACHE_HOME/veyl/notifications/`) y sobreviven a un reinicio, ya sin acciones: su programa no las escucha. Las que tienen más de 7 días se borran solas; se cambia con `notificationDays` en `settings.json` (0, nunca).

## Centro de control

Al subir el puntero al centro del borde de arriba (una franja de 8 px, y tras un instante, para que pasar por ahí no lo abra), nace de ese borde una isla ancha. Arriba, **la fecha**, con el día en grande y la batería si la hay; a la derecha, **las páginas** (Inicio, Veyl y Avisos, con una placa que se desliza a la elegida) y el engranaje. Al cambiar de página, la isla cambia de forma y la página entra desde su lado. Tras un rato cerrado vuelve a Inicio.

Todo lo que se pulsa tiene el mismo tacto: con el puntero encima se aclara poco a poco y al pulsar se aprieta; lo que solo aparece con el puntero (el aspa de una notificación, las acciones de una consulta) ocupa el sitio de lo que sustituye, sin mover nada.

**Inicio:**

- **Interruptores**, en una fila: Wi-Fi (con `nmcli`), Bluetooth (si hay adaptador), no molestar y modo escuchar; encendido, su icono va sobre el acento.
- **Sonido:** la salida y el micro, uno junto al otro, con el nombre del dispositivo; se arrastra, se pulsa o se mueve con la rueda, y el botón silencia.
- **Lo que suena,** si hay un reproductor: carátula, título, artista, anterior, reproducir/pausar y siguiente, con lo que lleva sonado.
- **Lo próximo** (tres eventos, con cuándo; los de hoy con una barra del acento y lo inminente, del color de aviso) junto al **temporizador**: lo que queda, o la duración (con la rueda) y 5, 15 o 25 min para empezar ya.
- **Los agentes:** lo que les queda de cuota a las suscripciones (Claude Code y Codex: la ventana de 5 h y la de la semana, con su barra, que se tiñe de aviso por debajo del 35 % y de crítico por debajo del 15 %, y cuándo se renueva la que más aprieta), y los que trabajan o te necesitan (logotipo, en qué están, modelo y cuánto llevan; pulsar uno te lleva a su ventana).
- **Al pie,** los atajos (recortar una zona, capturar la pantalla y el cuentagotas, si están `grim`/`slurp`/`hyprpicker`; el centro se va antes) y la sesión: bloquear, suspender, cerrar la sesión, reiniciar y apagar. El nombre del botón bajo el puntero sale a su izquierda; los tres últimos piden una segunda pulsación, teñida de aviso.

**Veyl:** lo que le has pedido, por voz o desde la entrada, de lo más reciente a lo más antiguo y por días, con «Todo», «Dicho» y «Escrito» para filtrar. Cada consulta lleva de dónde vino (el icono va en el acento si contestó el cerebro, y en crítico si falló), lo dicho, la primera línea de la respuesta y la hora. Pulsarla la abre con la respuesta entera y, si el cerebro compuso una tarjeta, «Ver la tarjeta» la vuelve a sacar. «Volver a preguntar» abre la entrada con lo dicho, para cambiarlo o repetirlo con Intro; la papelera la olvida del diario. Arriba, una fila para preguntar algo nuevo (abre la entrada). Lo guarda el diario de veyld (sin él, o con el diario apagado, la página lo dice); lo pedido con `veyl say` no sale.

**Avisos:** todas las notificaciones guardadas, con no molestar y «Borrar todo». Pulsar una hace su acción por defecto si sigue viva; el aspa o el botón derecho la borran.

Se cierra al sacar el puntero; tras pulsar algo dentro espera un poco más, por si la isla encoge y el puntero queda fuera sin haberse ido. `veyl controls [home|veyl|notes]` lo abre en el monitor con foco (en una página) hasta que el puntero entra y sale, o hasta `veyl controls close`. Si se abre la entrada, el centro cede su sitio; lo que llega mientras está abierto sale a su lado. En pantalla completa no se abre. El engranaje lo convierte en los ajustes: la misma isla crece hasta ser la suya.

**La cuota de los agentes.** Veyl no la pide con tus credenciales: la cuenta cada CLI. Codex la apunta en sus sesiones (`~/.codex/sessions`), y veyld lee la última. Claude Code se la pasa a su línea de estado: para verla, pon veyld como línea de estado en `~/.claude/settings.json`:

```json
"statusLine": { "type": "command", "command": "veyld statusline", "refreshInterval": 60 }
```

(o la ruta entera, `…/veyld/target/release/veyld statusline`). Guarda los límites en `$XDG_STATE_HOME/veyl/usage/claude.json` y escribe una línea corta: el modelo, la carpeta y lo que queda de las 5 h. Lo de una ventana que ya se renovó cuenta como entero.

## Ajustes

La isla más grande de Veyl: nace del centro del borde de arriba, como el centro de control, y se abre con el engranaje de este (es esa isla la que crece), con la orden «Ajustes» de la entrada, con el atajo global `veyl:settings` (en Hyprland, `hl.bind("SUPER + COMMA", hl.dsp.global("veyl:settings"))`) o con `veyl settings [<sección>]`. Detrás, la pantalla se oscurece; pulsar fuera, el aspa o Esc la cierran. Lo que llega mientras está abierta sale a su derecha, por encima de lo oscurecido, y se puede pulsar sin cerrarla.

A la izquierda, las secciones, de arriba abajo como los niveles (la actual, con su marca larga del acento; ↑↓ o Tab pasan de una a otra, y la nueva entra en vertical hacia donde vas); a la derecha, la sección, que se desplaza con la rueda. Para elegir entre varias cosas, desplegables: la lista crece desde el propio selector (hacia arriba si abajo no cabe), ↑↓ e Intro eligen y Esc o pulsar fuera la cierran sin cambiar nada. Los textos de la interfaz son los justos: cada fila dice qué es, y lo que hace cada ajuste se explica aquí. Todo se guarda al momento en `settings.json` (los campos de texto, con Intro o al salir de ellos; Esc los deja como estaban):

- **Apariencia** (`look`): el esquema de color, con sus colores; el fondo, entre las imágenes que hay junto al de ahora y en `~/Pictures/Wallpapers`, `~/Imágenes/Fondos` y `~/.config/veyl/wallpapers`, o por su ruta.
- **Sonido** (`sound`): el volumen y el dispositivo de salida y de entrada.
- **Pantallas** (`screens`): cómo están colocadas y, de cada una, resolución, frecuencia y escala (solo las que dan un tamaño entero), y el brillo si hay retroiluminación. Un cambio es una prueba: si en 15 s no pulsas «Mantener», vuelve a como estaba. Las pantallas en fila se recolocan una tras otra para que no se pisen.
- **Red** (`network`): lo conectado, con su dirección; el Wi-Fi (encenderlo, buscar redes, conectarse; una nueva protegida pide la contraseña en su fila) y las demás conexiones de NetworkManager (VPN, WireGuard), que se encienden y se apagan. Con `nmcli`.
- **Bluetooth** (`bluetooth`, si hay adaptador): encenderlo, buscar, emparejar lo de cerca, conectar, desconectar y olvidar (dos pulsaciones) lo emparejado, con su batería.
- **Teclado y ratón** (`input`): distribución y opciones de XKB, repetición y retardo de las teclas, velocidad y desplazamiento natural del ratón, y del panel táctil.
- **Notificaciones** (`alerts`): no molestar, cuántos días se guardan y borrarlas todas.
- **Calendarios** (`calendars`): los números de semana del almanaque; los de fuera (ver [Calendarios de fuera](#calendarios-de-fuera)), cuántos eventos trajo cada uno y cuándo, o por qué falló; mostrarlos u ocultarlos, quitarlos (dos pulsaciones), sincronizar ya y añadir uno: se elige el servicio, se siguen sus pasos para copiar el enlace, y se le da nombre y color. Antes de apuntarlo, Veyl descarga el enlace y comprueba que sea de verdad un calendario `.ics`; si no, dice por qué (no existe, no deja leerlo, es una página web…).
- **Voz** (`voice`): el modo escuchar, el modelo y el micro con los que oye, y si contesta, con qué motor, voz, velocidad, volumen y cuánto robot; «Reproducir» lo prueba.
- **Diario y órdenes** (`journal`): llevar el diario, la orden que contesta lo que la gramática no entiende (`llm`), la búsqueda web y la terminal de la entrada.
- **Sesión y energía** (`session`): bloquearse sola tras un rato sin tocar nada (`lockAfter`, en minutos; no mientras ves algo: un vídeo que pide no dormirse, por Wayland o por D-Bus, o un reproductor sonando con su ventana a la vista), el perfil de energía (con power-profiles-daemon), la batería y salir, suspender, reiniciar o apagar.
- **Nix** (`nix`, en NixOS o con Veyl declarado en Nix): el archivo de tu configuración donde está `programs.veyl`, lo que no coincide con lo declarado (guardarlo ahí o deshacerlo), lo guardado que falta por reconstruir y la orden para reconstruir. Ver [Con Nix](#con-nix).
- **Sistema** (`about`): la máquina (procesador, gráfica, memoria, disco, núcleo, cuánto lleva encendida), las versiones de Hyprland y Quickshell, si veyld contesta, `settings.json` para abrirlo y recargar Veyl.

Lo que se le cambia a Hyprland (teclado, ratón y pantallas) se aplica en caliente y se guarda en `settings.json` (`input`, `monitors`); Veyl lo vuelve a poner en cada arranque, por encima de su configuración. Lo que no se toca sigue siendo lo de la configuración de Hyprland.

**No molestar** se recuerda (`quiet` en `settings.json`): lo que llega no se enseña ni suena, salvo lo crítico, y caduca como si se hubiera visto. Mientras está puesto, la isla de la hora lleva una campana tachada.

## La entrada

`Super+Espacio` la abre y la cierra; `Super+Mayús+V`, directamente en el portapapeles. Es un lanzador al estilo de Raycast: se escribe y se elige.

- **Busca** a la vez aplicaciones, órdenes y ventanas abiertas. Lo que usas sube, y lo que elegiste con unas letras vuelve a salir primero con esas letras.
- **Hace cuentas** con `qalc`: `2^10*3`, `sqrt(2)`, `5 km to mi`, `100 eur to usd`. Intro copia el resultado; Ctrl+Intro, la cuenta entera.
- **Abre enlaces** escritos (`quickshell.org`) y, si no hay nada, busca en la web.
- **Pregunta a Veyl** lo escrito, si hay cerebro (`veyld brain`): va directo al modelo, sin pasar por la gramática, y contesta en la isla como cualquier pregunta. A lo copiado también se le puede preguntar, desde sus acciones.
- **Órdenes:** historial del portapapeles; subir, bajar y silenciar el volumen; cambiar la salida o la entrada de audio; el micro (silenciar, subir, bajar); reproducir, siguiente y anterior si suena algo; esquema de color; modo escuchar; recortar una zona o capturar la pantalla (se guarda en `~/Pictures/Screenshots`, se copia y se avisa con la imagen, que se abre al pulsar el aviso; la de la pantalla entera destella un instante; también `veyl shot [area|screen]`); elegir un color de la pantalla; Wi-Fi y Bluetooth; bloquear, suspender, cerrar la sesión, reiniciar y apagar; actualizar los tipos de cambio; recargar Veyl. A la derecha de cada una, su estado en vivo (el volumen, el dispositivo en uso…).
- **Archivos:** lo que hay en la carpeta personal, por su nombre (con `fd`, que deja fuera lo oculto y lo que ignora git; sin él, `find`). Sin nada escrito, lo último que se abrió. Intro lo abre; Ctrl+O, su carpeta; Ctrl+Intro pega o copia la ruta. Desde la raíz, «Buscar … en los archivos» entra con lo escrito.
- **Portapapeles:** el historial (texto, enlaces, colores e imágenes: lo fijado y las últimas 200 cosas), con lo fijado delante y una vista previa al lado. Intro lo pega en la ventana en la que estabas (en una terminal, con Ctrl+Mayús+V); sin ventana, lo copia.
- **Entiende órdenes dichas con palabras:** si veyld entiende lo escrito («sube el volumen a 85», «manda firefox al nivel 3», «pon un temporizador de media hora») y se puede hacer, la orden encabeza la lista con su icono. Lo peligroso pide Intro dos veces, como las demás órdenes; lo que contesta algo («qué estuve mirando ayer») lo dice en una notificación.

| Tecla | Qué hace |
| --- | --- |
| `↑` `↓`, `Tab`, `Ctrl+N` `Ctrl+P` | Elegir fila (de la última se pasa a la primera) |
| `Re Pág` `Av Pág`, `Ctrl+Inicio` `Ctrl+Fin` | Saltar |
| `Intro` | Lo que dice el pie |
| `Ctrl+K` | Las demás acciones de la fila |
| Las de cada acción (`Ctrl+X`, `Ctrl+Mayús+P`…) | Directamente, sin abrir la lista |
| `Esc` | Deshace lo último: la confirmación, lo escrito, la vista; y cierra |
| `Retroceso` con el campo vacío | Vuelve de una vista |

Veyl no le pone teclas: cada compositor se las da con los atajos globales `veyl:launcher`, `veyl:clipboard`, `veyl:files` y `veyl:symbols` (en Hyprland, `hl.bind("SUPER + SPACE", hl.dsp.global("veyl:launcher"))`). En `settings.json`: `webSearch` (con `%s`; vacío, sin búsqueda web) y `terminal` (para las aplicaciones de terminal; vacío, `$TERMINAL` o kitty).

```sh
veyl launcher                  # abre o cierra
veyl launcher clipboard        # en una vista: clipboard, outputs, inputs, schemes
veyl launcher = 2+2            # ya con algo escrito
```

Lo que falte no rompe nada: sin `qalc` no hay cuentas, sin `wl-clipboard` y `python3` no hay historial, y las órdenes que necesitan algo (`grim` y `slurp`, `hyprpicker`, `nmcli`) solo aparecen si está. El historial lo guarda `bin/veyl-clip` en `~/.local/state/veyl/clipboard/`; lo usado, en `launcher.json` al lado.

## Agentes

Veyl encuentra solo los agentes que corren en la máquina, se lancen desde una terminal, un editor, tmux o herdr: Claude Code, Codex, Gemini, opencode, pi, Aider, Cursor, Copilot, Goose, Amp, Crush, Qwen y Droid. Los busca veyld, que mira los procesos cada segundo:

- **En un panel de herdr**, manda lo que herdr ve en su pantalla (`herdr agent list`): trabajando, esperándote (`blocked`) o en reposo. Es lo más fiable.
- **Claude Code** dice él mismo en qué está (`~/.claude/sessions/<pid>.json`): trabajando, esperando algo de ti (y qué) o en reposo. Si su trabajo pasa a otro proceso (al mandarlo a segundo plano, `parkedJobId`), su estado se queda congelado y vale el del proceso que se lo llevó, que no sale aparte; los procesos de reserva de Claude Code (`spare`) no son agentes.
- **Codex**, por el registro de la sesión que tiene abierto (`~/.codex/sessions/…/rollout-*.jsonl`).
- **Los demás**, por lo que trabaja su árbol de procesos.
- **Cualquiera** te necesita si su ventana pide atención (la campana de la terminal), hasta que vas a ella.

Cada uno va en su propia isla, más baja y ligera que las otras, que cuelga del borde de arriba tras la de los niveles, una al lado de otra; si la de los niveles se abre, se corren con ella. Solo están los que trabajan o te necesitan: uno en reposo no se ve, y el que acaba sin que lo estuvieras mirando se queda unos segundos a decir «listo» y se va. Los niveles, la tira y la torre no dicen nada de agentes.

Pequeña, cada isla lleva el logotipo de quien hace el agente (Claude, OpenAI, Gemini, opencode, Cursor, Copilot, Goose, Amp, Qwen, Droid; los demás, un robot), quieto y del color de su marca, cuánto lleva trabajando o esperándote («42 s», «4:05», «1 h 05») y a su derecha cómo va; con el puntero encima o Super mantenido se estira y enseña entre los dos en qué trabaja, en corto (el título de la sesión o su carpeta). Cómo va: un spinner del color de la marca mientras trabaja, un check verde (rol `done`) al acabar y un punto de aviso, con un pulso, si te necesita; entonces el texto y el canto de la isla se tiñen de aviso, la isla da un tirón y el borde de su ventana también se tiñe. Los colores de marca son roles del esquema (`anthropic` cobre, `openai` verde, `google` azul, `github` violeta; los demás, el acento): Night los trae y otro esquema puede darles los suyos. Pulsar la isla te lleva a su ventana y, dentro de herdr o tmux, a su pestaña.

**Si te pide algo.** Cuando un agente que vive en herdr o tmux te necesita, veyld lee su pantalla y, si ve un diálogo (opciones numeradas, «❯ 1. Yes», o con su letra, «[Y] Permitir», con una pregunta encima: los permisos de Claude Code o Codex, una pregunta de varias opciones…), su propia isla crece hacia abajo desde su fila: la cabeza pasa a llevar la pregunta del agente, con sus palabras, y la carpeta y el nivel a la derecha; debajo, de qué va («Bash command»), sobre qué en un bloque (la orden, con su `$`, o el archivo) y lo que añade; y sus opciones en fila, tal como las ofrece el agente en ese momento, cada una con su tecla (y su explicación, si la trae), la primera en el acento, y «abrir en herdr» para ir a él. Pulsar una opción manda su tecla a la terminal del agente (`herdr pane send-keys` o `tmux send-keys`; una letra, en minúscula), y solo si es una de las que enseña en ese momento. Sin ratón, con los atajos globales `veyl:agent-accept` (la primera opción), `veyl:agent-second` (la segunda), `veyl:agent-deny` (la que dice que no, o la última) y `veyl:agent-open` (ir a él), que Veyl enlaza él mismo en Hyprland, como Super para el vistazo: **Super+Y** la primera opción, **Super+A** la segunda, **Super+N** la que dice que no y **Super+O** ir a él (sin nadie preguntando, no hacen nada). Contestan al que pregunta (al último que empezó, si son varios); por debajo, `veyl agent answer <tecla|accept|second|deny>` y `veyl agent open`. Con el puntero encima, la isla toma el teclado y sus teclas se encienden: la de cada opción (1, 2, 3, Y, N…) la elige, Intro te lleva a él y Esc suelta el teclado; al apartar el puntero, el teclado vuelve a donde estaba. El tiempo sigue donde siempre, gris y junto a cómo va. `veyl agent demo` saca dos agentes de ejemplo, uno trabajando y otro preguntando, pasados por veyld con pantallas de ejemplo de verdad (`agents::SAMPLES`): cada vez otra (un comando de Claude, una edición, Codex, pi con letras, una pregunta de varias opciones, una web y «¿confías en esta carpeta?»); `veyl agent demo <n>` va a una y `veyl agent demo off` los quita.

## veyld

El daemon de Veyl. Uno por sesión de Hyprland, con su socket en `$XDG_RUNTIME_DIR/veyl/veyld-<sesión>.sock`, por el que se habla en líneas JSON (el protocolo está al principio de [veyld/src/daemon/mod.rs](veyld/src/daemon/mod.rs)). Lo que escribe va a `~/.local/state/veyl/veyld.log`.

### El diario

veyld apunta, en `~/.local/state/veyl/journal.db` (SQLite, solo legible por ti), qué tuvo el foco y cuándo, y lo que se sabe de ello:

- **La aplicación y el título** de la ventana, y el nivel.
- **En una terminal**, la carpeta y la orden de lo que tiene en primer plano la pestaña en la que se escribe («cargo test», «nvim main.rs», «claude»). De la orden se guardan el programa y como mucho dos palabras más, hasta la primera opción o lo que no parezca un nombre (una clave, una dirección, `a=b`); la shell esperando no es nada.
- **En Firefox** (y LibreWolf, Zen o Floorp), la página y su sitio, de la sesión que el navegador guarda cada 15 s. Sin el fragmento (`#…`) ni lo que parezca una clave en la dirección (`token`, `code`, `session`…). Las ventanas privadas no van a esa sesión.
- **Lo que suena** (cualquier reproductor MPRIS: Spotify, el navegador…), con su título y su artista.
- **Cuándo estuvieron en uso el micro, la cámara o la pantalla compartida**, y qué tenía el foco mientras: así se ven las llamadas.
- **Los archivos que abren las aplicaciones** (los de `~/.local/share/recently-used.xbel`, que apuntan GTK y las que siguen su norma). La primera vez recoge los de los últimos 90 días.
- **Cuándo no estabas**: dos minutos sin tocar nada (salvo que algo impida el reposo, como un vídeo), la pantalla bloqueada o el ordenador suspendido. Lo que estaba abierto se cierra cuando te fuiste, no cuando se notó.
- También qué ventanas se abrieron y se cerraron, cuándo trabajó cada agente y lo que se le pidió a Veyl.

Para preguntarle después:

```sh
veyl recall el jueves por la tarde
# El jueves 12 de noviembre por la tarde (15:00–21:00): 3 h 40 min de pantalla
# De 15:04 a 20:41, con 40 min con la pantalla bloqueada
# Firefox, 1 h 25 min: doc.rust-lang.org («Rust by Example»), youtube.com («Fallout 3»)
# kitty, 55 min, en ~/dev/veyl (claude, cargo)
# Claude trabajó 1 h 10 min en ~/dev/veyl («Daemon de Veyl en Rust»)
# Sonó 40 min: «Song» de Band
# Micro abierto 45 min en 2 veces, sobre todo en Firefox (meet.google.com)
# Archivos: informe.pdf

veyl timeline hoy               # rato a rato: lo que más tuvo el foco en cada rato
# 13:32–13:45  kitty · ~/dev/veyl · claude
# 13:51–14:00  Firefox · claude.ai · «Shell Linux reinventado con IA»
# 14:00–14:30  sin tocar nada

veyl find fallout               # cuándo salió algo: títulos, páginas, carpetas, órdenes, canciones, archivos
# «fallout»: la última vez, hoy a las 15:13, en Firefox
# Hoy, 13:50–14:10: Firefox · youtube.com · «We're Decompiling Fallout 3 With AI»
```

Con todo el detalle, `veyl recall --full` (o «cuéntame con detalle qué hice ayer», «dame un informe de esta semana»):

```text
Hoy: 2 h 54 min de pantalla, de 09:29 a 23:19
Ritmo
  4 ratos seguidos: 13:20–16:02 (2 h 42 min), 16:57–17:33 (36 min), …; y 3 vistazos sueltos
  Pausas: 10 h 47 min sin tocar nada y 2 min con la pantalla bloqueada
  Cambiaste de ventana 255 veces (una cada 40 s de media)
Por horas
  09:00–13:00  ·
  14:00       █████████░     55 min  kitty · ~/dev/veyl
  16:00       █░░░░░░░░░      6 min  Firefox · reddit.com
Proyectos
  ~/dev/veyl  1 h 25 min (kitty 1 h 25 min) · claude 19 min, nvim 4 min · los agentes trabajaron 2 h 16 min
Aplicaciones
  Firefox  1 h 14 min
    chatgpt.com  12 min: «Partes de Veyl» 8 min, «Mejorar gramática de Veyl» 2 min
    otras páginas  27 min: «Veyl · Modelo operativo» 3 min, …
Niveles · Agentes · Sonó · Micro, cámara y pantalla · Archivos · Ventanas · Le pediste a Veyl
```

- **Ritmo:** los ratos seguidos delante (una pausa de más de 10 minutos los parte), las pausas y cada cuánto cambias de ventana.
- **Por horas** (o por días, si es más de un día): cuánto, y qué aplicación y dónde (sitio o carpeta) llenaron cada hora.
- **Proyectos:** la raíz git de cada carpeta o, si no hay, la más alta con `README.md`, `flake.nix`, `Cargo.toml`…; con las órdenes que corrieron y lo que trabajaron los agentes en él.
- **Agentes:** cada sesión (la misma, aunque se reinicie, junta), su modelo, de qué hora a qué hora, y cuántas veces y cuánto te esperó.
- **Llamadas:** cada vez que se abrió el micro, la cámara o se compartió la pantalla, con lo que había delante.
- **Ventanas** abiertas y cerradas, y **lo que le pediste a Veyl**, con lo que contestó.

`veyl recall --json` da lo mismo en datos, para otros programas.

Lo mismo se le puede decir: «¿qué estuve mirando ayer?», «¿qué hice ayer hora a hora?», «¿cuándo vi el vídeo de Fallout?», «¿cuánto tiempo estuve en YouTube esta semana?», «¿cuánto rato llevo delante del ordenador?», «¿qué música escuché ayer por la tarde?», «¿qué archivos abrí la semana pasada?».

Entiende «hoy», «ayer», «esta mañana», «anoche», «el lunes pasado», «el 12 de noviembre», «ayer a las cinco», «hace dos horas», «en la última hora», «la semana pasada»… La tarde va de 15 a 21 h; la mañana, de 6 a 13; la noche, de 21 a 2. Los ratos de la línea del tiempo son de 5 minutos si se pregunta por unas horas, de 15 por un día y de una hora por más.

- **Lo privado no se apunta:** de una ventana de navegación privada o de un gestor de contraseñas se guarda la aplicación, no el título ni la carpeta; mientras haya una ventana privada abierta, lo que suene en ese navegador tampoco.
- **Se olvida** lo que tiene más de 90 días. Con `"journal": false` en `settings.json` no se apunta nada.
- Se puede leer con cualquier cliente de SQLite, con los tiempos en ms desde 1970: `focus` (lo que tuvo el foco, con `folder`, `command`, `url` y `site`), `windows`, `agents`, `media`, `devices`, `files`, `away` (con `reason`: inactivo, bloqueado o suspendido) y `orders`.

### Órdenes sin modelo

Lo que se le dice a Veyl pasa primero por una gramática: si cumple alguna de sus reglas, veyld lo hace él mismo, al momento y sin red; solo lo que no entiende iría a un modelo. Lo del sistema lo hace veyld (el sonido con `wpctl`, la música con `playerctl`, las ventanas y los niveles con Hyprland, el Wi-Fi con `nmcli`, el Bluetooth con `bluetoothctl`); lo que es de Veyl (bloquear, el temporizador, la agenda, el esquema, no molestar, la entrada) se lo pide a Veyl.

```sh
veyl say sube el volumen a ochenta y cinco
veyl say cambia a firefox                   # o lo abre, si no tiene ventana
veyl say manda la terminal al nivel 3
veyl say apunta dentista el jueves a las cinco y media
veyl say qué estuve mirando ayer por la tarde
veyl say apaga el ordenador                 # pregunta: «¿Apagar el ordenador? Di «sí» para confirmar»
veyl say sí
```

La gramática de fábrica está en [veyld/grammar/es.txt](veyld/grammar/es.txt): sonido y micro, música, ventanas y aplicaciones, niveles, sesión, radios, no molestar, temporizador, agenda, el diario, la hora, los agentes, esquemas y capturas. Cada regla es un patrón y lo que hace:

```text
[pon|sube|baja] [el] (volumen|sonido) (a|al|hasta) <n:pct>   =>  volume.set
(enciende|activa) [el] wifi                                 =>  radio device=wifi on=true
```

Lee números con palabras («ochenta y cinco»), formas del verbo («súbelo», «subir», «¿puedes subir…?»), duraciones («hora y media») y cuándo («mañana a las cinco»), y no le importan las tildes ni «por favor». Tampoco lo que envuelve la orden: «oye Veyl, ¿me dices qué hora es?», «perdona, ¿sabes qué día es?» o «sube el volumen a 50, si puedes» son lo mismo que la orden sola (en la gramática, `filler` se quita en cualquier sitio, `lead` solo al principio y `tail` solo al final, para no comerse palabras de un título). Lo que dice algo más que una orden («¿qué hora es en Tokio?», «no subas el volumen») no se fuerza a una: va al modelo de `llm`. Para añadir frases, `~/.config/veyl/grammar.txt` con el mismo formato (gana a la de fábrica a igualdad); `veyld grammar` la comprueba y `veyld parse <texto>` dice qué entiende sin hacerlo.

**Varias órdenes seguidas.** Lo dicho se parte por «y», «luego», «después» o una coma, si cada trozo es una orden; se hacen en orden, y si una falla, las siguientes no. Un trozo sin verbo toma el de la orden de antes. Lo que no se puede partir sigue siendo una sola orden («apunta comprar pan y leche mañana»).

```sh
veyl say abre Firefox y muévelo al nivel dos
veyl say baja el volumen al 20 % y activa no molestar
veyl say pon Spotify a la izquierda y Discord a la derecha   # en tu nivel; si están en otro, se traen
veyl say mueve Firefox al nivel tres y después ve allí
veyl say cierra todas las ventanas menos esta              # pide un «sí»; también «de este nivel», «menos Firefox»
veyl say mueve esto al monitor de la izquierda            # o «al otro monitor», «al segundo monitor», «a la pantalla de arriba»
```

**Buscar y poner.** «Busca recetas de lentejas» abre los resultados de Google; «… en YouTube», «en la Wikipedia», «en Amazon», «en GitHub», «en Reddit», «en Google Maps», «busca imágenes de…», en ese sitio. «Pon Bohemian Rhapsody en Spotify» la pone; «pon … en YouTube» abre su primer vídeo, que se saca de la página de resultados, sin clave. «Abre YouTube», «abre Gmail» o «abre GitHub» abren esas webs. Veyl conoce unas treinta por su nombre (Prime Video, Netflix, Disney+, Max, Twitch, WhatsApp Web, ChatGPT…) y abre cualquier dirección («abre marca.com»). «Abre X» o «pon X», si X no es una aplicación que tengas pero sí una web, abre la web: «abre Firefox y pon Prime Video» va a primevideo.com en ese Firefox. «Abre X en Firefox» y «abre la web de X» abren también las que no conoce: van a la primera web que da DuckDuckGo (su «\», voy a tener suerte). Encadenado, lo de antes dice dónde: «abre Firefox y busca X» busca en ese Firefox, cuando sale su ventana; «abre Spotify y pon X» lo pone en Spotify, cuando despierta; «abre YouTube y pon X» va derecho al vídeo, sin una pestaña de más. «Pon X» a secas va a lo último que se nombró. Si no se nombró nada y X no suena a música («pon música de Estopa» sí), se pregunta al modelo, porque «pon una alarma» no es una canción. Lo que va detrás de «busca» o «pon» no se parte por «y»: «busca pan y leche» es una búsqueda y «pon Simon y Garfunkel» una canción (la regla lleva `whole=true`). «Busca gatos y abre kitty» sí son dos órdenes.

Spotify se maneja por MPRIS (`playerctl open spotify:track:…`), y para eso hace falta saber qué es X. Con una clave de su API es seguro: `veyld spotify login` pide el Client ID y el secreto de una app gratuita de developer.spotify.com, que solo sirve para buscar y no entra en tu cuenta. Sin clave, Veyl lo busca en Brave o en DuckDuckGo, que se cansan si se les pregunta mucho. Si no sale nada, abre la búsqueda dentro de Spotify. La lógica está en `veyld/src/play.rs`.

```sh
veyl say abre firefox y busca el horario del metro de valencia
veyl say abre spotify y pon la playlist de lofi
veyl say pon el vídeo de la final del mundial en youtube
veyl say busca hoteles en benidorm en google maps
```

**Lo de antes.** Veyl recuerda durante cinco minutos la última aplicación (y su ventana) y el último nivel que salieron en una orden. «Muévelo», «ponla», «ciérralo», «lo» o «él» señalan esa ventana; «esto» y «eso», la que tienes delante. «Ve allí» va al último nivel o, si no hay, a la última aplicación. Tras «abre Firefox», lo siguiente espera a que salga su ventana nueva.

```text
«Abre Firefox.»                → app.open app=firefox
«Muévelo al nivel tres.»       → window.move app=firefox n=3
«Y ponlo a pantalla completa.» → window.fullscreen app=firefox on=true   (si ya lo estaba, se queda)
```

**El ordenador por dentro.** Lo que preguntas del propio ordenador lo mide veyld, sin modelo (`veyld/src/system.rs`), en un hilo aparte y con «Pensando» mientras tanto. Contesta con un diagnóstico en una frase y una tarjeta con las cifras, una gráfica de lo que más gasta y botones para pasar a lo demás (y para cerrar al culpable, si tiene ventana):

- «¿Por qué va lento el ordenador?», «¿qué proceso está ralentizando esto?»: la presión de CPU, memoria y disco (`/proc/pressure`), la temperatura y el disco lleno deciden qué falla. La gráfica es de quien tiene la culpa.
- «¿Qué está usando la CPU?»: el % de cada programa, medido durante 0,8 s, con todos sus procesos juntos (las pestañas de Firefox son Firefox; `python server.py` es server).
- «¿Qué se está comiendo la RAM?»: la memoria de cada programa (PSS), lo libre y la swap.
- «¿Qué está ocupando más en el ordenador?», «¿cuánto espacio me queda?»: lo libre y lo que más ocupa en tu carpeta (du en paralelo, 5 s como mucho). Avisa si `~/.cache` se puede vaciar.
- «¿Por qué va lenta la red?»: mide el router, internet (TCP a 1.1.1.1), el DNS (un nombre que no está en ninguna caché), la señal del wifi y qué programa mueve más datos (`ss`, conexión a conexión). El diagnóstico dice dónde está el problema: no hay red, no llega al router, el router no da salida, el DNS falla, el wifi es débil o algo se está comiendo la conexión.
- «¿Qué está usando la gráfica?» (NVIDIA), «¿qué temperatura tiene la CPU?», «¿cuánta batería queda?».

El cerebro tiene lo mismo como herramienta (`sistema`): para lo que no está en la gramática («¿puedo cerrar algo para que vaya mejor?»), mide y parte de esa tarjeta. Y lo que la gramática contesta en varias líneas (la agenda, el diario, los agentes, el rato de pantalla) también sale en una tarjeta en vez de una notificación: lo que va por horas, como cronología, y lo que dura un rato, como una lista con el rato a la derecha.

### El cerebro

Lo que la gramática no entiende, o entiende pero no tiene a qué aplicar («abre la puerta del garaje»), se lo pregunta a un modelo. El modelo ve lo que tienes delante (la ventana con el foco, las ventanas por nivel, los agentes, lo que suena, el temporizador, la agenda de hoy y mañana), consulta el diario y le da órdenes a Veyl con las mismas palabras que tú, las de la gramática. Lo peligroso sigue esperando tu «sí»: el modelo no puede saltárselo. Recuerda lo hablado en los últimos diez minutos, para poder seguir («¿y mañana?»), también si veyld se ha reiniciado entre medias (lo saca del diario). La primera línea de lo que contesta se dice en voz alta; el resto va en la notificación.

```sh
veyl say ¿qué ventanas tengo abiertas?
veyl say ¿cuánto he estado hoy en Firefox y en qué webs?
veyl say organízame el escritorio para programar
```

Se elige con `veyld brain`:

| Proveedor | Cómo entra | |
|---|---|---|
| `claude-code` | tu suscripción de Claude, por Claude Code (`claude -p`), con la sesión que ya tengas iniciada | `veyld brain use claude-code` |
| `codex` | tu suscripción de ChatGPT, por Codex (`codex exec`), en solo lectura | `veyld brain use codex` |
| `anthropic` | la API de Claude: con una clave (`veyld brain login anthropic`, o `ANTHROPIC_API_KEY`) o con tu cuenta por OAuth (`ant auth login`, el programa de Anthropic) | `veyld brain use anthropic [modelo]` |
| `openai` | cualquier API compatible con la de OpenAI (OpenAI, OpenRouter, Ollama…), con su clave (`veyld brain login openai`) y, si no es OpenAI, `"url"` | `veyld brain use openai <modelo>` |

`veyld brain` dice cuál contesta y `veyld brain off` lo apaga. Con Claude Code y Codex, el modelo no tiene terminal ni archivos: solo las herramientas de Veyl, que le llegan por MCP (`veyld mcp`), y con Claude Code, además, buscar y leer en la web. Para que conteste rápido y gaste poco, el modelo de fábrica es el más rápido y barato de Claude: Haiku (en Claude Code, `haiku`; por la API, `claude-haiku-5-5`, con poco esfuerzo), y lo que tienes delante va ya con la pregunta, para que casi nunca necesite una vuelta más. Una pregunta tarda unos 3–4 s por Claude Code; `veyld brain use claude-code sonnet` (u `opus`) cambia el modelo, más lento y caro.

**Mientras piensa.** Una isla pequeña en el centro del borde de arriba, con tres puntos del acento que respiran en ola, dice «Pensando» y lo preguntado. En cuanto el modelo hace algo, la isla se abre y lo cuenta según pasa: «Busca «fecha de lanzamiento GTA 6»», «Lee rockstargames.com», «Mira el diario · ayer», «Mide la memoria», «Sube el volumen a 30»; cada paso entra desde abajo, y debajo quedan lo preguntado y los sitios de las páginas que ha encontrado o leído. A la derecha, cuánto lleva; con el puntero encima, un ✕ lo para (mata a Claude Code o a Codex; por las API, no da otra vuelta, y lo que conteste se tira). Con Claude Code se lee de su salida según llega (`--output-format stream-json`), con Codex igual (`--json`) y con las API, en cada vuelta de herramientas.

**Tarjetas.** El modelo contesta siempre con una tarjeta a la medida de lo preguntado (salvo cuando solo hace una orden), y la isla de «Pensando» se convierte en ella en cuanto la compone, sin esperar a que acabe de escribir su frase: sus piezas entran una tras otra y, mientras el modelo acaba, su icono respira; la frase y la portada llegan luego a esa misma tarjeta, sin rehacerla: «¿cuándo sale GTA 6?» da la portada del juego, la fecha en grande, etiquetas con las plataformas, una lista con lo que se sabe y los enlaces de donde lo sacó; «compara el iPhone 17 con el Pixel 10», una tabla con el mejor de cada fila resaltado y los pros y contras; «¿cómo ha ido el bitcoin este mes?», métricas en mosaico y una gráfica de líneas; «¿qué tiempo hará esta semana?», una tira por días. No es código: el modelo elige un icono, una imagen, el estilo de la cabecera y piezas de un vocabulario fijo (`veyld/src/card.rs`): texto, lista con icono o miniatura, datos, cifra, métricas (con su minigráfica si trae una serie), cuenta atrás que se mantiene al día, gráficas (barras, líneas y anillo), tabla comparativa, versus (dos frente a frente, con barras enfrentadas si los valores son cifras), pros y contras, previsión del tiempo (la temperatura de ahora en grande y una fila por día con su tramo de mínima a máxima), marcador de un partido, medidores (anillos que se llenan), barras de progreso con nombre, casillas que se marcan al pulsarlas (los ingredientes de una receta), definición, aviso, paleta de colores (pulsar una muestra copia su código), tira, pasos, cronología, valoración con estrellas, cita, código, galería, etiquetas, imagen, barra, botones que mandan una orden a Veyl, enlaces y preguntas para seguir. La cabecera va como **ficha** (icono, título y subtítulo), **portada** (la imagen a sangre, de borde a borde, y el título grande debajo) o **titular** (un título grande sin imagen). El prompt le dice qué pieza va con cada tipo de pregunta, que abra con la que más impacte y que no repita; la lista y el texto son el último recurso. Lo que el modelo escribe a su manera se entiende igual: un icono en español o con emoji («nube lluvia», «☀️») pasa al suyo de Phosphor, una `tira` que en realidad es el tiempo se pinta como previsión, y una segunda tarjeta solo con preguntas o enlaces se añade a la primera en vez de taparla. Los colores son los del tema, lisos, sin degradados: todo va en el acento y el modelo solo puede marcar cosas pequeñas con un color con sentido (bien, aviso, mal, neutro), que sale del esquema.

**Galería de tarjetas.** Para ver todo lo que el cerebro sabe componer sin preguntarle nada: `veyl cards` (o «Galería de tarjetas» en la entrada) saca en la isla el primero de 15 ejemplos, uno por cada tipo de respuesta (el tiempo, un partido, un versus, el ordenador, una definición, una receta, una paleta, una película en portada, una cotización, una cuenta atrás, horarios, el rato de pantalla, una orden de terminal, una comparativa y una cifra); sus flechas pasan de uno a otro y no se cierran solos. `veyl cards <n>` va a uno, `veyl cards list` los lista y `veyl cards demo [<n>]` lo reproduce entero: «Pensando» con sus pasos y sus fuentes y luego la tarjeta. Están en `data/cards.json`, y una prueba de veyld comprueba que cada uno es una tarjeta que veyld dejaría tal cual (tras editarlos, `GALLERY_WRITE=1 cargo test the_gallery` los limpia).

Al pie, tres cosas que no se confunden: justo bajo el contenido, **las fuentes**, pequeñas y solo subrayadas («Fuentes · AEMET · Meteosat»); luego **los botones** de lo que probablemente quieras hacer; y, tras una raya y con su rótulo, **«Seguir preguntando»**: dos o tres preguntas («¿Y en PC?») y, debajo, un campo para escribir la tuya (Intro la manda; mientras escribes, la tarjeta no se va). Las dos van directas al cerebro, con lo hablado (desde la vista del diario, se contestan allí). En una definición, si el título ya es la palabra, no se repite. veyld lo limpia (longitudes, cuántas, iconos y colores que existen, solo URLs web) y, si no trae imagen, saca la portada (`og:image`) de sus tres primeros enlaces a la vez, sin hacer esperar a la tarjeta: la primera que llega se le pone a la que se ve (abriéndose hueco poco a poco) y a la que quedó en el diario; Veyl lo pinta (`base/Card.qml`, con una pieza por archivo en `base/card/`). Con poco, la tarjeta mide 580 px de ancho; con mucho, se ensancha a 960 y reparte las piezas en dos columnas equilibradas, con el pie a todo lo ancho, en vez de crecer hacia abajo. Nunca pasa del alto de la pantalla: si no cabe, se desplaza dentro. La primera frase se sigue diciendo en voz alta. Se cierra con su ✕ o sola al rato (20 s más 4 por pieza, contados desde que el modelo acaba), y no mientras tengas el puntero encima; una cerrada no vuelve aunque le llegue algo después; un botón hace su orden y un enlace se abre en el navegador. Si el modelo no compone tarjeta y la respuesta es larga, veyld hace una con su texto; solo las respuestas cortas van como notificación. Las claves se guardan en `~/.config/veyl/keys.json`, solo para ti, nunca en `settings.json`. Sin cerebro, lo que no entiende va a la orden de `llm` si la hay (recibe lo dicho por la entrada estándar y `VEYL_SOURCE`; lo que escriba es la respuesta) y, si no, Veyl dice que no sabe hacerlo. Todo queda en la tabla `orders` del diario.

## Contarle cosas

Para lo que Veyl no sabe ver solo, quien sepa algo se lo anuncia; un agente anunciado va en la misma isla.

```sh
veyl agent claude working      # punto que respira
veyl agent claude needs        # un pulso, punto de aviso fijo y borde de aviso en su ventana
veyl agent claude idle         # punto gris
veyl agent claude done         # el punto se va
veyl event "Reunión" 16:30     # hoy (mañana si ya pasó); también +15 (minutos). Avisa 10 min antes (eventWarning)
veyl event "Cumple" 2026-11-20 # un día; con HH:MM detrás, a esa hora
veyl event list                # lo que queda, con su número
veyl event remove 3
veyl event clear               # todos (los de Veyl)
veyl event calendars sync      # vuelve a traer los calendarios de fuera
veyl timer 25                  # temporizador; stop lo para y sin nada dice lo que le queda
veyl context brief             # nombra el contexto actual; sin nombre, lo quita
veyl say <orden>               # una orden con palabras, como si se dijera (ver veyld)
veyl recall [<cuándo>]         # qué estuviste mirando; sin nada, hoy (--full: todo; --json: en datos)
veyl timeline [<cuándo>]       # lo mismo, rato a rato
veyl find <texto>              # cuándo salió algo en el diario
```

La ventana de un agente es aquella desde la que se llama a `veyl`; `--window <dirección>` la fija a mano. `bin/veyl` necesita `jq`.

Por debajo es el IPC de Quickshell (`qs -p . ipc show`): `agents`, `agenda` (también `sync` y `calendars`), `timer`, `glance`, `launcher`, `lock`, `scheme` y `wallpaper`.

## Bloqueo

`Super+L` bloquea la pantalla; también la orden «Bloquear la pantalla» de la entrada y del centro de control, `veyl lock` y `loginctl lock-session` (lo que usan hypridle y compañía; Veyl lo oye si hay `gdbus`). Es un bloqueo de verdad (`ext-session-lock`): mientras dura, Hyprland no enseña ni deja tocar nada más, y si Veyl se cae la sesión sigue bloqueada.

El bloqueo es instantáneo: nace con el fondo de pantalla nítido, el mismo que ya está cargado en el escritorio, y poco a poco se desenfoca y se oscurece. En todos los monitores pasa lo mismo (y se oscurece algo más al escribir); los controles solo salen en el principal, el que tenía el foco al bloquear (si ya no está, el primero). En él, a la derecha y sin isla, la hora enorme, con las horas firmes y los minutos finos, el día de la semana (del acento) y la fecha encima, y lo próximo debajo (o el temporizador, o «Día libre»), todo alineado a la derecha. Del centro del borde izquierdo sale **lo tuyo**, en una columna: los **agentes** en marcha (hasta cuatro: logotipo, en qué trabajan, desde cuándo y cómo van, con un arco que gira o un punto de aviso si te necesitan); **el día**, una línea con los de todo el día arriba y los demás alrededor del **ahora**, que late entre ellos (la línea va en el acento hasta él, lo pasado sale tachado y el próximo dice cuánto falta, en aviso si es inminente; si hoy no hay nada, lo próximo); el temporizador si corre, y cuántas notificaciones hay guardadas (cuántas, no cuáles). Si no hay nada de eso, no sale. Arriba a la derecha, las excepciones. Bajo la hora, alineada con ella, **la puerta**: en reposo, tu inicial y tu nombre; al escribir se convierte en el campo, sube al sitio de lo próximo y crece hacia la izquierda letra a letra, con un punto que entra con rebote por cada una, un cursor que los sigue y una luz del acento que se enciende tras él a cada tecla, mientras la hora se aparta un poco y el fondo se oscurece. Intro la comprueba (con PAM: la configuración de `swaylock` si existe, que no admite contraseñas vacías; si no, la de `login`. En NixOS Veyl la copia a `$XDG_RUNTIME_DIR/veyl/pam/` apuntando a los módulos del PAM con el que está hecho Quickshell, porque los del sistema pueden estar hechos con otra glibc y no cargar) y, mientras, los puntos ondean y el candado gira. Si no vale, los puntos se tiñen de crítico, el campo se sacude, se van de derecha a izquierda y queda el aviso; Esc borra lo escrito. Si vale, la hora se acerca y se desvanece, las islas vuelven a sus bordes, el fondo se aclara y se suelta el bloqueo.

Sobrevive a las recargas del shell: editar un archivo de Veyl con la pantalla bloqueada no la abre. Veyl no le pone tecla: cada compositor le da la suya con el atajo global `veyl:lock` (en Hyprland, `hl.bind("SUPER + L", hl.dsp.global("veyl:lock"))`). Por debajo, el IPC `lock` (`lock`, `get`).

## Modo escuchar

```sh
veyl listen          # alterna; también on / off
veyl listen demo     # sin micro: una voz inventada, para ver cómo late
```

Veyl oye una frase, la transcribe y la hace como si la hubieras escrito con `veyl say`: lo que entiende la gramática, al momento; lo demás, al modelo de `llm` si lo hay. La pantalla se oscurece un poco y la rodea un borde del color del acento, con un halo hacia dentro que late con la voz: tenue en silencio, más vivo y más hondo según hablas. Lo que contesta sale como una notificación.

- **Con la tecla** (el atajo global `veyl:listen`), escucha mientras la mantienes pulsada: puedes pararte a pensar sin que corte, y al soltarla lo dicho es la frase.
- **Con `veyl listen`, el IPC o el centro de control**, que lo encienden y apagan, deja de escuchar solo al acabar de hablar (algo menos de un segundo de silencio) o si en 8 s no oye nada.

Todo lo hace veyld, en local y sin red:

- **El micro** (`pw-record`) se abre solo mientras escucha.
- **Silero** nota cuándo empiezas a hablar y cuándo acabas, y no confunde la voz con el teclado o el ventilador. Sin su modelo, se guía por lo fuerte que suena.
- **Whisper** (whisper.cpp) lo pasa a texto, en español, con una pista de cómo suenan las órdenes y de cómo se llaman las aplicaciones que tienes abiertas. Lo que Whisper se inventa en silencio («Subtítulos realizados por…») se tira.
- **Vocabulario** (`veyld/src/vocab.rs`): palabras que Whisper no conoce y cómo las oye mal. Van en la pista y, además, lo oído mal se cambia por su palabra antes de leerlo: «¿qué puede hacer bale?» llega como «¿qué puede hacer Veyl?». De fábrica trae Veyl (bale, vail, beil…), Hyprland, Quickshell, kitty y Claude. Lo tuyo va en `~/.config/veyl/vocabulary.txt`, una palabra por línea y, si la oye mal, cómo (`Spotify = espotifai | spoti fai`); se lee en cada frase, sin reiniciar. `veyld hear` usa lo mismo y dice qué se oyó antes de corregir. Y por si algo se cuela, el cerebro sabe que lo que suena a Veyl es él.

Los modelos no vienen con Veyl; se traen una vez, a `~/.local/share/veyl/models`:

```sh
veyl voice                      # los que hay, y cuál se usa (●)
veyl voice get small-q5_1       # 190 MB: el bueno sin GPU (una orden en ~0,3 s)
veyl voice get base-q5_1        # 57 MB: para una CPU modesta
veyl voice get large-v3-turbo-q5_0   # 547 MB: el mejor, con GPU
```

`get` trae también el de Silero (1 MB). veyld usa el mejor que haya para la máquina (con GPU, el grande; sin ella, `small`); `"voice": {"model": "base-q5_1"}` en `settings.json` lo fija (o una ruta a otro `ggml-*.bin`). Los modelos se cargan al empezar a escuchar y se sueltan tras cinco minutos sin usarlos. Para probar sin hablar, `veyld hear frase.wav` dice lo que oye en un WAV, cuánto tarda y qué entiende la gramática.

`veyld/build.sh` compila con la transcripción en la GPU si hay una NVIDIA (trae el kit de CUDA de nixpkgs a `veyld/.cuda/` y compila solo para la arquitectura de la tarjeta); `veyld/build.sh cpu`, o un `cargo build --release` a secas, en la CPU. Con Nix, `callPackage ./veyld/package.nix { acceleration = "cuda"; }` (o `"vulkan"`, para cualquier tarjeta). Ojo: en la CPU el modelo bueno es `small`, que con un micro malo falla mucho; con la GPU, `large-v3-turbo` acierta bastante más y tarda unos 0,2 s.

**Micros Bluetooth.** Con Bluetooth clásico, el micro de unos auriculares solo funciona en manos libres: al abrirlo, los auriculares cambian de modo (y la música suena peor mientras tanto), tardan casi un segundo en despertar y oyen a calidad de teléfono. No hay un modo «normal» con micro; lo que da mejores resultados es hablar por otro micro (USB, de cable, el de la webcam) y dejar los auriculares para escuchar. `veyld mic` dice qué micros hay y cuál se usa (el de por defecto de PipeWire, o el de `"voice": {"mic": "<nombre>"}` en `settings.json`). Lo que da un micro Bluetooth mientras despierta (silencio digital) no se guarda, para no confundir a Whisper. Con la tecla, al soltarla aún se escuchan 300 ms, para no comerse la última sílaba. La última frase oída queda en `$XDG_RUNTIME_DIR/veyl/voz.wav` (en memoria, solo tuya): `veyld hear` la vuelve a transcribir, para ver qué oyó de verdad cuando entiende mal.

**La voz de Veyl.** A lo que le dices por voz, Veyl contesta también en voz alta: la línea corta («Son las 16:20», «¿Apagar el ordenador? Di sí»); lo largo sigue en la notificación. Es una voz de robot de película: la dice XTTS-v2 con la voz «Damien Black», algo rápida, y le pasa un tratamiento de cine (una capa una octava más grave casi inaudible, compresión y ecualización de voz de cine, chorus y una sala metálica corta). Corre en local, en un servidor que veyld carga en la GPU al arrancar (unos 8 s) y deja cargado (~2 GB de memoria de vídeo), para que conteste al momento: desde que sueltas la tecla, unos 0,4 s en entender lo dicho y alrededor de un segundo en tener la voz lista. Lo corto que ya se dijo («Nivel 2», «Hecho», «Volumen al 50 %») queda guardado en `~/.cache/veyl/voz` (las 500 últimas) y la siguiente vez suena sin esperar. Lo que va entre paréntesis en la respuesta no se lee en voz alta. `"voice": {"unload": 10}` suelta la voz tras 10 minutos sin hablar, a cambio de unos 8 s de espera la próxima vez. Se calla en cuanto vuelves a pulsar la tecla, y con «no molestar» no habla.

```sh
veyl voice get xtts                # la instala (~1,8 GB + PyTorch); el modelo es de Coqui, solo para uso no comercial
veyld speak hola, soy Veyl         # para oírla; --wav <archivo> la guarda
```

Sin XTTS, habla con [Piper](https://github.com/rhasspy/piper) (`veyl voice get voz-davefx`, 63 MB, sin GPU) y un filtro de robot suave.

En `settings.json`, dentro de `"voice"`: `"speak": false` la calla, `"speed": 1.3` la acelera (1,2 de fábrica), `"volume": 0.4` la baja (de 0 a 1; 0,6 de fábrica), `"voz"` cambia de voz (otra de las de XTTS, o de Piper) y `"engine": "piper"` usa Piper aunque esté XTTS.

Veyl no le pone tecla: cada compositor le da la suya con el atajo global `veyl:listen` (en Hyprland, `hl.bind("code:49", hl.dsp.global("veyl:listen"))`; Hyprland le cuenta también cuándo se suelta). Por debajo, el IPC `listen` (`toggle`, `start`, `stop`, `demo`, `get`).

## Esquemas de color

```sh
veyl scheme                    # lista los esquemas; el activo, con *
veyl scheme catppuccin-mocha   # cambia en caliente, ventanas incluidas
```

Un esquema es un JSON que le da un color a cada rol: `bg`, `surface`, `border`, `dim`, `text`, `accent`, `working`, `proposal`, `focus`, `attention` y `critical` (qué hace cada uno, en el [brief](BRIEF.md)). Veyl trae los cuatro sabores de Living Code (Night es el de fábrica) y Catppuccin Mocha, en [schemes/](schemes).

Para añadir uno, deja `<nombre>.json` en `~/.config/veyl/schemes/`; si se llama como uno de los que trae Veyl, lo sustituye. Los roles que falten se toman de Night, y editar el archivo del esquema activo se ve al momento.

## Fondo de pantalla

```sh
veyl wallpaper ~/imágenes/lago.png   # llena la pantalla, con fundido
veyl wallpaper clear                 # vuelve al color de fondo del esquema
```

El esquema y el fondo elegidos se recuerdan en `~/.local/state/veyl/settings.json` (o `$XDG_STATE_HOME`).

## Tokens

Los colores, el radio, los huecos, las fuentes y los tiempos salen de [Tokens.qml](Tokens.qml); los colores los lee del esquema activo. Nada más en el código nombra un color ni un radio.

Las fuentes viajan en [fonts/](fonts): Geist para el texto (SIL OFL) y Phosphor para los iconos (MIT).

## Comprobar cambios

```sh
sh tests/check.sh
```

Ejecuta formato, Clippy, pruebas Rust y regresiones de portapapeles y lógica QML, sin arrancar el escritorio. Requisitos, cobertura y límites en [tests/README.md](tests/README.md).


## Estructura

- `Tokens.qml`: los tokens.
- `state/`: lo que Veyl sabe (hora, calendario, contextos, ventanas, agentes, batería, red, privacidad, sonido, agenda, temporizador, vistazo), lo que recuerda (esquema, fondo), lo que le pide al compositor y cómo habla con veyld (`Daemon`).
- `base/`: lo que se ve. El velo (`Veil`, `Notch`, `Notice`, `Controls`) y la entrada (`Palette`, `Prompt`, `Entry`, `Sum`, `Preview`, `Keycap`) son lo que nace del centro. Las tarjetas del cerebro, en `Card`, con sus piezas en `base/card/`.
- `dials/`: los ajustes. `Dials` (la capa) y `Panel` (la isla), una sección por archivo y sus piezas (`Group`, `Option`, `Toggle`, `Slider`, `Select`, `Field`, `Button`); lo que saben, en `state/Tuning` (qué sección, cómo se guarda) y `state/Host` (lo que se le pregunta y se le pide al sistema).
- `schemes/`: los esquemas de color.
- `flake.nix` y `nix/`: el paquete (`package.nix`) y los módulos de Home Manager y NixOS (`home-manager.nix`, `nixos.nix`), con lo que comparten (`settings.nix`: las opciones y el manifiesto). Lo que lee el manifiesto y escribe en tu configuración, en `state/Nix.qml` y `veyld/src/nix.rs`.
- `fonts/`: las fuentes.
- `logos/`: los logotipos de los agentes, de un color (de cada marca; del registro de agentes de Zed).
- `bin/veyl`: la línea de órdenes. `bin/veyl-clip`: el historial del portapapeles.
- `veyld/`: el daemon, en Rust. `src/` (`daemon/` junta los hilos y habla por el socket (`sense`, lo que nota solo; `talk`, lo que se le dice); `journal`, el diario; `agents`, los agentes; `grammar`, `words` y `when`, el motor de la gramática; `orders/`, lo que hace cada orden, con `windows` para ventanas y vistas y `diary` para las preguntas al diario) y `grammar/es.txt`, la gramática.

`VEYL_POWER_SUPPLY` apunta la batería a otro árbol que no sea `/sys/class/power_supply`, para probarla en una máquina sin ella.
