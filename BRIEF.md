# Veyl · estado base (brief)

## Objetivo

Lo que se ve cuando no pasa nada: lo mínimo para saber dónde estoy, qué hora es y si algo me necesita. Sin barra: dos islas que salen de las esquinas de arriba, pegadas a los bordes de la pantalla como muescas del marco, y que cambian de forma según lo que tengan que enseñar.

## Disposición

| Zona               | Contenido                                                                      |
| ------------------ | ------------------------------------------------------------------------------ |
| Superior derecha   | **Hora** (`HH:MM`). En la misma isla, delante, las excepciones cuando existan  |
| Superior izquierda | Niveles del monitor, contexto actual y su tira de ventanas                       |
| Borde izquierdo    | Agentes activos, cada uno en su isla ligera, en el tercio de abajo             |
| Superior centro    | Vacío hasta que llega algo: ahí nace el velo. Las notificaciones que llegan (apiladas), la entrada, el centro de control o los ajustes |

Hueco superior reservado: 44 px (una isla de 36 px pegada al borde y 8 px hasta las ventanas). Resto de huecos: 8 px.

## Niveles y tiras

El espacio se piensa en dos ejes, y Veyl los dibuja así siempre:

- **Vertical: niveles.** Los contextos de un monitor se apilan, el primero arriba; cambiar de contexto es subir o bajar. Nunca se listan de izquierda a derecha, y lo que se mueve al cambiar de nivel se mueve en vertical.
- **Horizontal: la tira.** Las ventanas de un nivel van una al lado de otra y la pantalla enseña un trozo (disposición `scrolling` de Hyprland).

En reposo, la isla del contexto lo resume: una marca por nivel (larga la del actual, apagada la de un nivel vacío), el nombre o número del nivel y su tira en miniatura, con un marco sobre lo que cabe en pantalla.

## Qué se muestra y cuándo

- **Siempre, como texto:** hora, contexto.
- **Siempre, ambiental:** niveles y tira del nivel actual, agentes activos (su logotipo en un loader del color de su marca), ventana con foco (borde).
- **Solo por excepción:** micro/cámara/pantalla compartida activos, batería baja, sin red, evento inminente, temporizador en marcha.
- **Cuando llega:** una notificación, en una isla que nace del centro del borde de arriba, en el monitor con foco; si ese centro lo ocupa la entrada, el centro de control o los ajustes, nace a su derecha, del mismo borde, para no taparlos ni quedar tapada. Si llegan varias, se apilan desde ese mismo sitio, la última arriba. Se va sola, salvo que sea crítica o tenga el puntero encima, y se queda guardada en Avisos, en el centro de control.
- **Mientras piensa:** cuando una pregunta va al modelo, una isla pequeña en ese mismo sitio, con tres puntos del acento que respiran en ola, «Pensando» y lo preguntado en gris. Cuando el modelo hace algo (buscar, leer una página, mirar el diario), la isla se abre en dos o tres líneas: lo que hace ahora, con su icono, lo preguntado y los sitios que ha encontrado; a la derecha, cuánto lleva, y con el puntero encima un aspa para pararlo. Cede ante lo que llega, la entrada y los ajustes. Si la respuesta gana con forma, la isla se convierte en una tarjeta que el modelo compone para esa pregunta (texto, lista, datos, una cifra grande del acento, una barra, botones, enlaces y preguntas para seguir, con piezas fijas que Veyl sabe pintar), en cuanto la compone, con las piezas entrando una tras otra; se va sola al rato, salvo con el puntero encima. Si no, la respuesta es una notificación.
- **Cuando la pides (Super+L):** el bloqueo: en todos los monitores, el fondo desenfocado; solo en el principal (el que tenía el foco al bloquear), a la derecha y sin isla, la hora enorme (horas firmes, minutos finos) con la fecha encima y lo próximo debajo. Del centro del borde izquierdo, lo tuyo: los agentes en marcha, el día como una línea con el ahora entre los eventos, el temporizador y cuántas notificaciones esperan. Bajo la hora, la puerta: tu inicial y, al escribir, el campo, que crece letra a letra con puntos que rebotan y una luz del acento tras él. Las excepciones, arriba a la derecha.
- **Cuando la pides (Super+Espacio):** la entrada, en el monitor con foco, del mismo centro del borde de arriba.
- **Bajo demanda (Super mantenido):** fecha, volumen, batería exacta y la torre: la isla del contexto se abre y enseña todos los niveles del monitor, cada uno con su tira y lo que tiene abierto. Los marcos de todas las tiras caen en la misma columna.
- **Bajo demanda (puntero en el centro del borde de arriba):** el centro de control, del mismo centro que el velo, en tres páginas entre las que la isla cambia de forma. Inicio: fecha, interruptores (Wi-Fi, Bluetooth, no molestar, escuchar), volumen de salida y micro, lo que suena, los próximos eventos y el temporizador, los agentes con lo que les queda de cuota, atajos de pantalla y la sesión. Veyl: lo que le has pedido, por días, con su respuesta, para volver a verla, repetirla u olvidarla. Avisos: todas las notificaciones guardadas. Cede ante lo que llega y ante la entrada.
- **Cuando los pides (engranaje del centro de control, la entrada o `veyl:settings`):** los ajustes, la isla más grande, del mismo centro del borde de arriba; si vienen del centro de control, es esa isla la que crece. A la izquierda las secciones, apiladas como niveles (la actual con su marca larga del acento); a la derecha la sección, que entra en vertical. Veyl y el sistema: apariencia, sonido, pantallas (a prueba, se deshacen solas si no se confirman), red, Bluetooth, teclado y ratón, notificaciones, voz, diario y órdenes, sesión y energía, y la máquina.
- **Bajo demanda (puntero sobre la hora):** el almanaque. La isla de la hora se abre hacia abajo, con la hora en su sitio: la fecha, el mes (una tira continua de semanas que la rueda desliza en vertical, sin saltar de mes; el mes es el que ocupa el centro, y su nombre abre el año), el día elegido con sus eventos y un campo para apuntar otro, lo próximo y un temporizador. Hoy, del color del acento.

## Color · esquemas

Regla del tema: pizarra primero, color solo con un trabajo. En reposo todo es fondo, superficie y gris, salvo el acento.

El acento (`accent`) es el color propio del esquema y la firma del shell: marca lo actual, lo tuyo y lo que avanza (el nivel en el que estás, el marco de la pantalla en la tira, los dos puntos de la hora, una barra de progreso, el campo en el que escribes). Todo lo nuevo que tenga algo así lo lleva.

Veyl no nombra colores sino **roles**; un esquema es un JSON que le da un color a cada rol. De fábrica, Living Code sabor Night (hex oficiales de `palette.json`); cualquier otro esquema es otro archivo, sin tocar código.

| Rol         | Trabajo en Veyl                                | Night     |
| ----------- | ---------------------------------------------- | --------- |
| `bg`        | Islas; fondo cuando no hay imagen              | `#05080b` |
| `surface`   | Lo que resalta dentro de una isla              | `#12171a` |
| `border`    | Canto de las islas, borde de ventana sin foco  | `#1d2226` |
| `dim`       | Texto secundario, iconos, agente en reposo     | `#747c80` |
| `text`      | Texto                                          | `#e2e9ee` |
| `accent`    | Lo actual, lo tuyo, lo que avanza              | `#00a692` |
| `working`   | Agente trabajando                              | `#00a692` |
| `proposal`  | Propuesta (fantasma)                           | `#52d7c1` |
| `focus`     | Borde con foco                                 | `#80b6d3` |
| `attention` | Te necesita / aviso                            | `#f0be6d` |
| `critical`  | Crítico, micro o cámara activos                | `#e96560` |

## Fondo

Una imagen a pantalla completa (recortada para llenar), elegida por el usuario; sin imagen, el color `bg`. Cambia con un fundido.

## Forma

- **Ventanas:** bordes redondeados (`radius`, valor inicial 10 px). Puntos: círculo.
- **Isla:** no flota. Sale de una esquina de la pantalla, pegada a sus dos bordes, maciza y del color del fondo (`bg`), sin sombra; donde se une a cada borde hace una curva hacia fuera (`join`, 14 px) y su única esquina libre es redonda. Un canto de 1 px recorre solo el contorno libre. No hay piezas sueltas: son dos, una por esquina, y cada una crece o encoge con lo que lleva. Una excepción entra en la isla de la hora, con su icono teñido del color de su rol; el vistazo la estira, y a la del contexto la abre desde la esquina (esquina libre `round`, 22 px).
- Borde de ventana de 1 px: `border` sin foco, `focus` con foco, `attention` si su agente me necesita. Las ventanas llevan sombra; las islas, no.
- Tipografía: Geist (sans, no monoespaciada) a 15 px, con cifras tabulares para que la hora no baile. Iconos: Phosphor. Las dos fuentes viajan con Veyl.

- **Hora:** es lo más grande de la pantalla en reposo (17 px). Las horas van en negrita, los minutos en peso normal y, entre ellas, dos puntos del color del acento; el resto, pizarra.

## Movimiento

- Aparecer/desaparecer: ~150 ms. Transiciones del velo: ~300 ms. Muelles, interrumpibles.
- Una isla cambia de forma con un rebote leve; lo que lleva dentro se funde.
- Cambiar de minuto: las cifras que cambian ruedan hacia arriba.
- Cambiar de nivel: el nombre rueda en vertical hacia donde te has movido y la isla se estira o se encoge un poco en ese sentido.
- Agente trabajando: respiración lenta (~3 s). Te necesita: un único pulso y se queda fijo.

## Entrada

Lo que se abre para pedir algo escribiendo. Es una isla más: nace del centro del borde de arriba, como el velo, pero sin esperar, porque la has llamado tú. Arriba el campo; debajo, las filas; al pie, qué hace Intro con la fila elegida y si tiene más acciones (Ctrl+K). Detrás, la pantalla se oscurece un poco; pulsar fuera o Esc la cierra.

- **Una sola búsqueda:** aplicaciones, órdenes de Veyl y ventanas abiertas a la vez, por parecido (inicio del nombre, inicio de palabra, iniciales, letras sueltas en orden), sin tildes ni mayúsculas. Sube lo que más usas y, sobre todo, lo que elegiste con esas mismas letras.
- **Sin nada escrito:** sugerencias (lo más usado), las órdenes y las aplicaciones.
- **Cuentas:** si lo escrito es una cuenta, una conversión de unidades o de moneda (qalculate), su resultado encabeza la lista en una tarjeta propia: la cuenta a la izquierda, el resultado a la derecha. Intro copia el resultado.
- **Vistas:** algunas órdenes abren otra lista dentro de la misma isla (el portapapeles, la salida y la entrada de audio, los esquemas), con una píldora para volver. Esc o Retroceso con el campo vacío deshacen el último paso.
- **Portapapeles:** el historial, con lo fijado delante; al lado, lo copiado a la vista (texto, imagen o color) y lo que se sabe de ello. Intro lo pega en la ventana en la que estabas.
- **Lo peligroso** (apagar, vaciar el historial) pide Intro dos veces; mientras espera, la fila y el pie se tiñen de `attention`.
- **Órdenes con palabras:** si el daemon entiende lo escrito como una orden («sube el volumen a 85») y se puede hacer, la orden encabeza la lista como una fila más, con su icono; lo peligroso, igual que las demás, pide Intro dos veces.
- **Rápida:** nada se crea al teclear; las mismas filas enseñan otra cosa. La elección acompaña a las teclas y el puntero elige solo si se mueve.

## Comportamientos

- **Pantalla completa:** desaparece todo salvo excepciones y el punto de "te necesita", en una isla en la esquina.
- **Varios monitores:** hora en todos; contexto y agentes solo en el que tiene el foco.
- Sin red o sin modelo, el estado base funciona igual.

## Fuera de alcance (v0)

Bandeja de decisiones, widgets generados, fondo como instrumento.

## Hecho cuando

1. Hora arriba a la derecha y contexto arriba a la izquierda, legibles sobre cualquier fondo.
2. Una excepción real (batería baja) aparece y desaparece sola.
3. Super mantenido revela el vistazo y al soltar se va.
4. Los niveles se leen de arriba abajo y las ventanas de izquierda a derecha, en reposo y en el vistazo.
5. Todos los colores y el radio salen de un único archivo de tokens, y los colores, de un esquema intercambiable.
