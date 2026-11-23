pragma Singleton

import QtQuick
import Quickshell
import qs.state

// Único archivo de tokens de Veyl. Todos los colores, el radio, los huecos,
// la tipografía y los tiempos salen de aquí, también los que se le aplican
// a Hyprland (ver state/Compositor.qml).
Singleton {
    // Color · por rol. El valor de cada rol lo pone el esquema activo
    // (schemes/*.json, ver state/Scheme.qml); de fábrica, Living Code Night.
    // Regla: pizarra primero, color solo con un trabajo.
    readonly property color bg: Scheme.colors.bg                // fondo
    readonly property color surface: Scheme.colors.surface      // lo que resalta dentro de una isla
    readonly property color border: Scheme.colors.border        // pizarra: canto de las islas, bordes en reposo
    readonly property color dim: Scheme.colors.dim              // texto secundario
    readonly property color text: Scheme.colors.text            // texto
    readonly property color accent: Scheme.colors.accent        // el color del esquema: lo actual, lo tuyo, lo que avanza
    readonly property color working: Scheme.colors.working      // agente trabajando
    readonly property color proposal: Scheme.colors.proposal    // propuesta (fantasma)
    readonly property color focus: Scheme.colors.focus          // indicadores de foco
    readonly property color attention: Scheme.colors.attention  // te necesita / aviso
    readonly property color critical: Scheme.colors.critical    // crítico, micro o cámara activos
    readonly property color done: Scheme.colors.done            // listo: un agente que ha terminado
    // Agentes · el color de quien los hace, parecido al de su marca
    readonly property color anthropic: Scheme.colors.anthropic  // cobre
    readonly property color openai: Scheme.colors.openai        // verde
    readonly property color google: Scheme.colors.google        // azul
    readonly property color github: Scheme.colors.github        // violeta

    // Isla · maciza y del color del fondo: sale del borde de la pantalla
    readonly property color plate: bg

    // Ventanas · lo que las despega del fondo
    readonly property color shadow: Qt.rgba(0, 0, 0, 0.35)
    // Modo escuchar · lo que oscurece la pantalla mientras escucha
    readonly property color scrim: Qt.rgba(0, 0, 0, 0.15)
    // Bloqueo · lo que oscurece el fondo, desenfocado, tras sus islas
    readonly property color hush: Qt.rgba(0, 0, 0, 0.4)
    readonly property int haze: 64        // desenfoque del fondo bloqueado
    readonly property int blur: 8          // desenfoque de lo que queda tras una ventana translúcida
    readonly property int shadowRange: 24  // alcance de su sombra

    // Forma
    readonly property int radius: 10  // ventanas
    readonly property int round: 22   // esquina libre de una isla abierta; en reposo, medio alto
    readonly property int small: 3    // miniaturas de ventana
    readonly property int soft: 7     // lo pequeño de la entrada: iconos, teclas
    readonly property int line: 1     // borde de ventanas y canto de las islas
    readonly property int gap: 8      // huecos
    readonly property int island: 36  // alto de una isla en reposo
    readonly property int pad: 16     // aire lateral dentro de una isla
    readonly property int inset: 7    // aire entre una isla abierta y lo que enseña
    readonly property int join: 14    // curva con la que una isla se une al borde de la pantalla
    readonly property int gapTop: island + gap  // hueco superior: las islas y su separación de las ventanas
    readonly property int dot: 8      // punto de agente
    readonly property int pod: 28     // alto de la isla de un agente: más ligera que las otras
    readonly property int mark: 12    // marca de nivel
    readonly property int tile: 14    // alto de la pantalla en la tira en miniatura
    readonly property int card: 400   // ancho de una tarjeta del velo
    readonly property int answer: 580 // ancho de la tarjeta que compone el cerebro
    readonly property int answerWide: 960 // y el de la ancha, en dos columnas
    readonly property int avatar: 48  // imagen o icono de una tarjeta
    readonly property int almanac: 400  // ancho del almanaque, bajo la hora
    readonly property int controls: 620 // ancho del centro de control, arriba en el centro
    readonly property int tab: 8        // alto de la franja del borde de arriba que lo llama
    readonly property int toggle: 56    // alto de un interruptor del centro de control
    readonly property int dials: 1080   // ancho de los ajustes, arriba en el centro: la isla más grande
    readonly property int dialsTall: 700 // su alto, como mucho
    readonly property int diaryTall: 780 // el alto del diario, como mucho
    readonly property int aisle: 232    // ancho de la columna de secciones de los ajustes
    readonly property int option: 58    // alto de una fila de los ajustes
    readonly property int day: 34     // círculo de un día en el calendario
    readonly property int knob: 28    // botón redondo
    readonly property int ring: 4     // borde del modo escuchar, alrededor de la pantalla
    readonly property int glow: 20    // halo de ese borde hacia dentro, en silencio
    readonly property int shout: 56   // hasta dónde entra con la voz plena
    readonly property int prompt: 680      // ancho de la entrada
    readonly property int promptWide: 900  // ancho con algo que enseñar al lado de la lista
    readonly property int field: 56   // alto del campo en el que escribes
    readonly property int row: 44     // alto de una fila de la entrada
    readonly property int heading: 30 // alto del título de un grupo de filas
    readonly property int figure: 92  // alto de la cuenta y su resultado
    readonly property int rows: 8     // filas a la vista, como mucho
    readonly property int bar: 40     // alto de la barra de acciones
    readonly property int emblem: 28  // icono de una fila
    readonly property int cell: 52    // casilla de la cuadrícula de emojis

    // Tipografía · las dos familias viajan con Veyl, en fonts/
    readonly property string sans: "Geist"      // todo el texto; las cifras, tabulares
    readonly property string icons: "Phosphor"  // iconos, por su nombre (ligaduras)
    readonly property int fontSize: 15
    readonly property int smallSize: 13  // texto menor: de quién es una tarjeta, sus botones
    readonly property int tinySize: 11   // lo más pequeño: iniciales de los días, números de semana
    readonly property int clockSize: 17  // la hora
    readonly property int promptSize: 18 // lo que escribes en la entrada
    readonly property int figureSize: 24 // la cuenta y su resultado
    readonly property int lockSize: 196  // la hora del bloqueo: lo más grande de Veyl
    readonly property int lockDay: 300   // ancho de la isla del día, en el bloqueo
    readonly property int lockDots: 28   // puntos de la contraseña a la vista, como mucho
    readonly property int iconSize: 18

    // Movimiento (ms)
    readonly property int step: 90      // la elección pasa de una fila a otra
    readonly property int fast: 150     // aparecer / desaparecer
    readonly property int veil: 300     // transiciones del velo
    readonly property int arrive: 520   // lo que llega al velo: nace con calma
    readonly property int summon: 220   // lo que llamas tú: llega ya
    readonly property int tick: 450     // el halo del modo escuchar sigue a la voz, sin saltos
    readonly property int breath: 3000  // respiración del agente trabajando

    FontLoader {
        source: "fonts/Geist.ttf"
    }

    FontLoader {
        source: "fonts/Phosphor.ttf"
    }
}
