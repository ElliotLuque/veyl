pragma Singleton

import QtQuick
import Quickshell

// La cuenta de los días. Un día es un número (los que van desde 1970 en el
// calendario local), y una semana y un mes también: así se suman y se
// comparan sin que la hora ni el cambio de horario se metan por medio.
Singleton {
    id: root

    readonly property var locale: Qt.locale()
    // La semana empieza donde diga el idioma (0 = domingo). El día 0 fue
    // jueves: `shift` es el primer día que abre una semana.
    readonly property int firstDay: locale.firstDayOfWeek
    readonly property int shift: (firstDay + 3) % 7
    readonly property int today: day(Clock.now)
    readonly property int thisMonth: monthOf(today)
    // Las iniciales de los días, en el orden de la semana
    readonly property list<string> weekdays: Array.from({ length: 7 }, (_, i) => locale.dayName((firstDay + i) % 7, Locale.NarrowFormat))
    // La fecha larga del idioma, sin el año: «lunes, 16 de noviembre»
    readonly property string longFormat: locale.dateFormat(Locale.LongFormat).replace(/[,\s]*(?:'[^']*'\s*)?y+/, "")

    function day(date: var): int {
        return Math.round(Date.UTC(date.getFullYear(), date.getMonth(), date.getDate()) / 86400000);
    }

    // El día en que cae un instante (ms desde epoch)
    function at(time: double): int {
        return day(new Date(time));
    }

    function parts(day: int): var {
        const date = new Date(day * 86400000);
        return { year: date.getUTCFullYear(), month: date.getUTCMonth(), date: date.getUTCDate(), weekday: date.getUTCDay() };
    }

    // Un momento de un día (minutos desde su medianoche), en ms desde epoch
    function moment(day: int, minutes: int): double {
        const date = parts(day);
        return new Date(date.year, date.month, date.date, Math.floor(minutes / 60), minutes % 60).getTime();
    }

    function week(day: int): int {
        return Math.floor((day - shift) / 7);
    }

    function weekStart(week: int): int {
        return week * 7 + shift;
    }

    // Número de semana ISO: la del jueves de esa semana, contada en su año.
    function weekNumber(day: int): int {
        const thursday = day - (parts(day).weekday + 6) % 7 + 3;
        const first = Math.round(Date.UTC(parts(thursday).year, 0, 1) / 86400000);
        return Math.floor((thursday - first) / 7) + 1;
    }

    // Un mes es año × 12 + mes.
    function monthOf(day: int): int {
        const date = parts(day);
        return date.year * 12 + date.month;
    }

    function monthStart(month: int): int {
        return Math.round(Date.UTC(Math.floor(month / 12), month % 12, 1) / 86400000);
    }

    function monthName(month: int, abridged: bool): string {
        return locale.standaloneMonthName(month % 12, abridged ? Locale.ShortFormat : Locale.LongFormat);
    }

    // Un día, escrito como pida `format` («ddd d», «d MMM»…)
    function format(day: int, format: string): string {
        return new Date(moment(day, 0)).toLocaleDateString(locale, format);
    }
}
