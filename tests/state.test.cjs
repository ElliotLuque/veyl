// Ejecuta las funciones JavaScript reales de QML, sin iniciar el escritorio.
// No sustituye las pruebas de integración con Qt/Hyprland.
const { readFileSync } = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const test = require('node:test');
const assert = require('node:assert/strict');

function load(name, values = {}) {
    const source = readFileSync(path.join(__dirname, '..', 'state', `${name}.qml`), 'utf8');
    const functions = [...source.matchAll(/^    function (\w+)\(([^\n]*)\): \w+ \{\n([\s\S]*?)^    \}/gm)];
    assert.ok(functions.length, `No se encontraron funciones en ${name}`);
    const context = vm.createContext({ console: { warn() {} }, ...values });
    context.root = context;
    for (const [, name, args, body] of functions) {
        const plain = args.replace(/:\s*\w+/g, '');
        vm.runInContext(`function ${name}(${plain}) {\n${body}\n}`, context);
    }
    return context;
}

function agenda() {
    const Calendar = load('Calendar');
    Calendar.today = Calendar.day(new Date(2026, 10, 17));
    const a = load('Agenda', {
        Calendar, mine: [], outside: [], calendars: [], sources: [], synced: 0, written: '',
        file: { setText() {} },
        sourceFile: { written: '', path: '/x/veyl/calendars.json', setText(text) { this.written = text; } },
        pending: '', target: '', writable: [], google: {},
        Daemon: { connected: false, sent: [], send(message) { this.sent.push(message); } },
        // Crear la carpeta acaba al momento: entonces se escribe.
        folder: { set running(on) { if (on) a.sourceFile.setText(a.pending); } },
        Qt: { formatTime: date => `${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}` },
    });
    return a;
}

test('rechaza días imposibles y admite años bisiestos', () => {
    const a = agenda();
    for (const day of ['2026-02-29', '2026-04-31', '2026-13-01', '2026-00-01', '2026-11-00', 'basura'])
        assert.equal(a.parseDay(day), -1, day);
    assert.equal(a.stamp(a.parseDay('2024-02-29')), '2024-02-29');
    assert.equal(a.parseDay('mañana'), a.Calendar.today + 1);
});

test('rechaza horas imposibles sin convertirlas en eventos de todo el día', () => {
    const a = agenda();
    a.read(JSON.stringify([{ title: 'mal', day: '2026-11-17', time: '25:00' }, { title: 'bien', day: '2026-11-17' }, null]));
    assert.equal(a.mine.length, 1);
    assert.equal(a.mine[0].title, 'bien');
    assert.equal(a.mine[0].allDay, true);
});

test('un JSON incompleto no borra la agenda que ya está cargada', () => {
    const a = agenda();
    a.read('[{"title":"Dentista","day":"2026-11-17"}]');
    a.read('{incompleto');
    a.read('null');
    assert.equal(a.mine.length, 1);
});

test('la agenda asigna identificadores únicos y válidos', () => {
    const a = agenda();
    a.read(JSON.stringify([1, 1, null, -3, 1.5, 2147483648].map(id => ({ id, title: 'evento', day: '2026-11-17' }))));
    assert.equal(a.mine.length, 6);
    assert.equal(new Set(a.mine.map(e => e.id)).size, 6);
    assert.ok(a.mine.every(e => Number.isInteger(e.id) && e.id > 0 && e.id <= 2147483647));
});

test('reutiliza un id libre si se llega al límite de int de QML', () => {
    const a = agenda();
    a.read('[{"id":2147483647,"title":"Dentista","day":"2026-11-17"}]');
    assert.equal(a.put('Otro', Date.now(), false), 1);
});

test('restaurar una versión anterior del archivo vuelve a cargarla', () => {
    const a = agenda();
    const first = '[{"title":"Primero","day":"2026-11-17"}]';
    a.read(first);
    a.read('[{"title":"Segundo","day":"2026-11-17"}]');
    a.read(first);
    assert.equal(a.mine[0].title, 'Primero');
});

test('no acepta eventos vacíos ni fechas no finitas', () => {
    const a = agenda();
    assert.notEqual(a.add('  ', 'hoy', ''), '');
    assert.equal(a.put('hola', Infinity, false), -1);
    assert.equal(a.put('hola', 1e20, false), -1);
    assert.equal(a.mine.length, 0);
});

test('guardar y recargar conserva el día, la hora y el id', () => {
    const a = agenda();
    assert.equal(a.add('Dentista', '2026-11-17', '16:30'), '');
    const b = agenda();
    b.read(a.written);
    assert.equal(b.mine[0].id, a.mine[0].id);
    assert.equal(b.mine[0].at, a.mine[0].at);
    assert.equal(b.mine[0].allDay, false);
});

test('el historial con todo fijado no deja un encabezado vacío seleccionable', () => {
    const l = load('Launcher', { clips: [{ entry: { pinned: true } }] });
    assert.equal(l.copied('').length, 1);
    l.clips.push({ entry: { pinned: false } });
    assert.equal(l.copied('').length, 4);
});

test('la calculadora descarta el resultado anterior mientras espera la cuenta nueva', () => {
    const old = { stale: false };
    let started = 0;
    const c = load('Calc', {
        pending: old, result: '8', asked: '4+4', expression: '2+2', available: true,
        delay: { stop() {}, restart() { started++; } },
    });
    c.refresh();
    assert.equal(old.stale, true);
    assert.equal(c.pending, null);
    assert.equal(c.result, '');
    assert.equal(c.asked, '');
    assert.equal(started, 1);
});

test('la calculadora no inicia consultas vacías o sin qalc', () => {
    let started = 0;
    const c = load('Calc', {
        pending: null, result: '8', asked: '4+4', expression: '', available: true,
        delay: { stop() {}, restart() { started++; } },
    });
    c.refresh();
    c.expression = '2+2';
    c.available = false;
    c.refresh();
    assert.equal(c.result, '');
    assert.equal(started, 0);
    c.available = true;
    c.refresh();
    assert.equal(started, 1);
});

test('el temporizador rechaza duraciones no positivas', () => {
    const c = load('Countdown', { span: 25, until: 123 });
    c.start(-1);
    c.start(0);
    assert.equal(c.until, 123);
    assert.equal(c.span, 25);
});

function clipboard() {
    const c = load('Clipboard', { Quickshell: { env: name => ({ HOME: '/home/ana' })[name] ?? '' } });
    const text = (text, extra = {}) => ({ kind: 'text', text, chars: [...text].length, ...extra });
    return { c, text };
}

test('lo copiado se clasifica por lo que es', () => {
    const { c, text } = clipboard();
    assert.equal(c.type({ kind: 'image' }), 'image');
    assert.equal(c.type({ kind: 'link', text: 'https://a.b' }), 'link');
    assert.equal(c.type(text('ana@example.org')), 'email');
    assert.equal(c.type(text('~/dev/veyl')), 'path');
    assert.equal(c.type(text('/etc/hosts\n')), 'path');
    assert.equal(c.type(text('file:///tmp/a%20b')), 'path');
    assert.equal(c.type(text('//comentario')), 'text');
    assert.equal(c.type(text('{"a": [1, 2]}')), 'json');
    assert.equal(c.type(text('[\n  1,\n  2\n]')), 'json');
    assert.equal(c.type(text('{ no es json }')), 'text');
    assert.equal(c.type(text('(3 + 4) * 2')), 'math');
    assert.equal(c.type(text('17/11/2026')), 'text');
    assert.equal(c.type(text('+34 600 123 456')), 'text');
    assert.equal(c.type(text('hola mundo')), 'text');
});

test('un JSON cortado en el índice no se toma por entero', () => {
    const { c, text } = clipboard();
    assert.equal(c.type(text('{"a": 1}', { chars: 5000 })), 'text');
    assert.equal(c.whole(text('abc', { chars: 5000 })), null);
});

test('los colores copiados se escriben como RGB y HSL', () => {
    const { c } = clipboard();
    assert.equal(c.rgb('#ff8000'), 'rgb(255, 128, 0)');
    assert.equal(c.rgb('#f00'), 'rgb(255, 0, 0)');
    assert.equal(c.rgb('#0000ff80'), 'rgb(0 0 255 / 0.5)');
    assert.equal(c.hsl('#ff0000'), 'hsl(0, 100%, 50%)');
    assert.equal(c.hsl('#ff00ff'), 'hsl(300, 100%, 50%)');
    assert.equal(c.hsl('#808080'), 'hsl(0, 0%, 50%)');
});

test('los enlaces pierden el rastreo y nada más', () => {
    const { c } = clipboard();
    assert.equal(c.clean('https://a.b/x?utm_source=y&id=3&fbclid=z#top'), 'https://a.b/x?id=3#top');
    assert.equal(c.clean('https://a.b/x?utm_source=y'), 'https://a.b/x');
    assert.equal(c.clean('https://a.b/x?id=3'), 'https://a.b/x?id=3');
    assert.equal(c.clean('https://a.b/x'), 'https://a.b/x');
});

test('las rutas copiadas se resuelven enteras', () => {
    const { c } = clipboard();
    assert.equal(c.place('~/dev'), '/home/ana/dev');
    assert.equal(c.place('file:///tmp/a%20b'), '/tmp/a b');
    assert.equal(c.place(' /etc/hosts '), '/etc/hosts');
});

test('los símbolos dicen su código y no se rompen con datos malos', () => {
    const s = load('Symbols', { groups: [], all: [] });
    assert.equal(s.code('👍🏽'), 'U+1F44D U+1F3FD');
    assert.equal(s.code('❤️'), 'U+2764');
    assert.equal(s.code('→'), 'U+2192');
    s.read('{roto');
    assert.equal(s.all.length, 0);
    s.read(JSON.stringify({ groups: ['Caras'], items: [['😀', 'cara', 'feliz', 0], [1, 2], ['👍', 'pulgar', '', 0, ['👍🏻']]] }));
    assert.equal(s.all.length, 2);
    assert.equal(s.all[0].group, 'Caras');
    assert.equal(s.all[1].tones.join(), '👍🏻');
});

test('data/symbols.json trae emojis con tonos y símbolos con nombre', () => {
    const data = JSON.parse(readFileSync(path.join(__dirname, '..', 'data', 'symbols.json'), 'utf8'));
    assert.ok(data.items.length > 1500);
    const find = text => data.items.find(item => item[0] === text);
    assert.equal(find('😀')[1], 'cara sonriendo');
    assert.equal(find('👍')[4].length, 5);
    assert.ok(find('→'));
    assert.ok(find('€'));
    assert.equal(find('🏽'), undefined);
    assert.ok(data.items.every(item => typeof item[1] === 'string' && data.groups[item[3]]));
});

test('la cuadrícula de símbolos reparte las casillas y se mueve por líneas', () => {
    const h = { kind: 'header' };
    const s = { kind: 'symbol' };
    const l = load('Launcher', {
        Tokens: { heading: 30, cell: 52, row: 44, figure: 92 },
        grid: true, columns: 3, index: 0, touched: false,
        items: [h, s, s, s, s, h, s, s],
        select(at) { this.index = at; },
    });
    l.select = at => { l.index = at; };
    l.measure();
    assert.equal(l.tops.join(), '0,30,30,30,82,134,164,164');
    assert.equal(l.cols.join(), '-1,0,1,2,0,-1,0,1');
    assert.equal(l.extent, 216);
    assert.equal(l.lead(3), 1);
    l.index = 3;
    l.step(1, false);
    assert.equal(l.index, 4);  // la línea de abajo solo tiene una casilla
    l.index = 2;
    l.step(2, false);
    assert.equal(l.index, 7);  // salta el título y cae en la misma columna
    l.step(-1, false);
    assert.equal(l.index, 4);
});

test('los eventos de los calendarios de fuera llegan con hora, final y sin tocar los de Veyl', () => {
    const a = agenda();
    a.put('mío', a.Calendar.moment(a.Calendar.today, 600), false);
    a.take(JSON.stringify({
        synced: 5,
        calendars: [{ name: 'Uni', count: 3 }],
        events: [
            { calendar: 'Uni', title: 'Clase', day: '2026-11-18', time: '09:00', end: '11:00', where: 'Aula 3' },
            { calendar: 'Uni', title: 'Viaje', day: '2026-11-19', time: '22:00', end: '2026-11-20 06:00' },
            { calendar: 'Uni', title: 'Festivo', day: '2026-12-08' },
            { calendar: 'Uni', title: 'Roto', day: 'ayer' },
        ],
    }));
    assert.equal(a.synced, 5);
    assert.equal(a.outside.length, 3);
    assert.ok(a.outside.every(e => e.id < 0 && e.calendar === 'Uni'));
    const [clase, viaje, festivo] = a.outside;
    assert.equal(clase.until - clase.at, 2 * 3600000);
    assert.equal(clase.place, 'Aula 3');
    assert.equal(viaje.until - viaje.at, 8 * 3600000);
    assert.equal(festivo.allDay, true);
    assert.equal(a.drop(clase.id), false);
    assert.equal(a.mine.length, 1);
    a.take('{a medio escribir');
    assert.equal(a.outside.length, 3);
});

test('apuntar un calendario de fuera comprueba la dirección y los repetidos', () => {
    const a = agenda();
    assert.notEqual(a.follow('Uni', 'calendar.google.com/x.ics', ''), '');
    assert.equal(a.follow('Uni', ' https://calendar.google.com/calendar/ical/x/basic.ics ', ''), '');
    assert.equal(a.follow('', 'webcal://example.org/cal.ics', ''), '');
    assert.equal(a.follow('Casa', '~/.local/share/calendars/casa', '#ff0000'), '');
    assert.equal(JSON.parse(a.sourceFile.written)[2].color, '#ff0000');
    assert.notEqual(a.follow('Otro', 'https://calendar.google.com/calendar/ical/x/basic.ics', ''), '');
    assert.notEqual(a.follow('Uni', 'https://otro.example/cal.ics', ''), '');
    assert.deepEqual(JSON.parse(a.sourceFile.written).map(s => s.name ?? ''), ['Uni', '', 'Casa']);
});

test('lo nuevo va al calendario elegido, y si Google falla se queda en Veyl', () => {
    const a = agenda();
    a.target = 'Elliot';
    a.writable = ['Elliot'];
    a.Daemon.connected = true;
    assert.equal(a.add('Dentista', '2026-11-20', '16:30'), '');
    assert.equal(a.mine.length, 0);
    assert.equal(JSON.stringify(a.Daemon.sent[0]), JSON.stringify({ calendar: { add: { calendar: 'Elliot', title: 'Dentista', day: '2026-11-20', time: '16:30' } } }));
    assert.equal(a.outside.length, 1);
    assert.ok(a.outside[0].pending);
    a.told({ failed: { calendar: 'Elliot', title: 'Dentista', day: '2026-11-20', time: '16:30', error: 'sin conexión' } });
    assert.equal(a.outside.length, 0);
    assert.equal(a.mine.length, 1);
    assert.equal(a.Daemon.sent.length, 1);
    a.target = '';
    a.add('Local', 'mañana', '');
    assert.equal(a.mine.length, 2);
});

test('los de Google en los que se puede escribir se borran allí', () => {
    const a = agenda();
    a.writable = ['Elliot'];
    a.Daemon.connected = true;
    a.take(JSON.stringify({ events: [
        { calendar: 'Elliot', id: 'abc', title: 'Clase', day: '2026-11-18', time: '09:00' },
        { calendar: 'Festivos', title: 'Fiesta', day: '2026-12-08' },
    ] }));
    const [clase, fiesta] = a.outside;
    assert.ok(a.removable(clase));
    assert.ok(!a.removable(fiesta));
    assert.equal(a.drop(fiesta.id), false);
    assert.equal(a.drop(clase.id), true);
    assert.equal(JSON.stringify(a.Daemon.sent[0]), JSON.stringify({ calendar: { remove: { calendar: 'Elliot', id: 'abc' } } }));
    assert.equal(a.outside.length, 1);
});

test('los eventos de un calendario oculto no se ven', () => {
    const a = agenda();
    a.take(JSON.stringify({ events: [
        { calendar: 'Uni', title: 'Clase', day: '2026-11-18', time: '09:00' },
        { calendar: 'Casa', title: 'Cena', day: '2026-11-18', time: '21:00' },
    ] }));
    a.sources = [{ name: 'Uni', url: 'https://x/a.ics', on: false }, { name: 'Casa', url: 'https://x/b.ics' }];
    const hidden = a.sources.map((source, at) => source.on === false ? source.name || a.calendars[at]?.name : null).filter(name => name);
    assert.equal(JSON.stringify(hidden), JSON.stringify(['Uni']));
});

const json = value => JSON.parse(JSON.stringify(value));

function nix(stored = {}, declared = {}) {
    const defaults = { scheme: 'living-code-night', wallpaper: '', quiet: false, voice: {}, monitors: {} };
    const Settings = Object.assign({ defaults }, JSON.parse(JSON.stringify(defaults)), stored);
    const n = load('Nix', {
        Settings, titles: {}, declared: true, settings: declared,
        memory: { applied: '', written: {} },
        Quickshell: { env: () => '/home/tú' },
    });
    Object.defineProperty(n, 'current', { get: () => Settings });
    return n;
}

test('con Nix, solo difiere lo que no coincide con lo declarado o con lo de fábrica', () => {
    const n = nix({ scheme: 'living-code-frost', voice: { speed: 1.3, mic: '' } }, { scheme: 'living-code-frost', voice: { volume: 0.4 } });
    const groups = n.compare(n.current, n.settings, {});
    assert.deepEqual(json(groups.map(g => g.key)), ['voice']);
    assert.deepEqual(json(groups[0].changes), [
        { path: ['voice', 'speed'], value: 1.3 },
        { path: ['voice', 'volume'], value: null, nix: 0.4 },
    ]);
});

test('lo guardado en Nix sin reconstruir no se ofrece otra vez', () => {
    const n = nix({ quiet: true });
    assert.equal(n.compare(n.current, {}, {})[0].written, false);
    assert.equal(n.compare(n.current, {}, { '["quiet"]': 'true' })[0].written, true);
});

test('lo declarado se aplica al cambiar, y lo que deja de declararse vuelve a lo de fábrica', () => {
    const n = nix({}, { scheme: 'living-code-frost', monitors: { 'HDMI-A-1': { scale: 1.5 } } });
    n.apply();
    assert.equal(n.Settings.scheme, 'living-code-frost');
    assert.deepEqual(json(n.Settings.monitors), { 'HDMI-A-1': { scale: 1.5 } });
    n.settings = { scheme: 'living-code-frost' };
    n.apply();
    assert.deepEqual(json(n.Settings.monitors), {});
    // Lo tocado a mano desde entonces no se pisa al dejar de declararse.
    n.Settings.scheme = 'catppuccin-mocha';
    n.settings = {};
    n.apply();
    assert.equal(n.Settings.scheme, 'catppuccin-mocha');
});

test('deshacer vuelve a lo declarado, o a lo de fábrica si no se declaró', () => {
    const n = nix({ scheme: 'a', voice: { speed: 2 } }, { scheme: 'b' });
    for (const group of n.compare(n.current, n.settings, {}))
        n.revert(group);
    assert.equal(n.Settings.scheme, 'b');
    assert.deepEqual(json(n.Settings.voice), {});
});

test('deshacer todo revierte solo los cambios sin guardar y limpia el error', () => {
    const n = nix({ scheme: 'a', quiet: true, voice: { speed: 2 } }, { scheme: 'b' });
    n.failure = 'error previo';
    n.memory.written = { '["quiet"]': 'true' };
    Object.defineProperty(n, 'unsaved', { get: () => n.compare(n.current, n.settings, n.memory.written).filter(g => !g.written) });
    n.revertAll();
    assert.equal(n.failure, '');
    assert.equal(n.Settings.scheme, 'b');
    assert.deepEqual(json(n.Settings.voice), {});
    assert.equal(n.Settings.quiet, true);
});

test('Nix resume el estado en vivo, la antigüedad y el tiempo transcurrido', () => {
    const n = nix();
    const now = 1_700_000_000_000;
    assert.equal(n.ageText(now - 20_000, now), 'ahora');
    assert.equal(n.ageText(now - 12 * 60_000, now), 'hace 12 min');
    assert.equal(n.ageText(now - 3 * 3600_000, now), 'hace 3 h');
    assert.equal(n.ageText(now - 5 * 86400_000, now), 'hace 5 d');
    assert.equal(n.elapsedText(now - 18_000, now), '18 s');
    assert.equal(n.elapsedText(now - 64_000, now), '1:04');

    Object.assign(n, {
        authNeeded: false, running: false, jobState: 'idle', job: null,
        updates: [{ name: 'nixpkgs' }, { name: 'veyl' }],
        pendingRebuild: false, lockNewer: false, unbuilt: [], rebootNeeded: false,
        dirty: 0, flakeName: 'nixdots', checking: false, checkedAt: now,
    });
    assert.equal(n.summaryLine(), '2 actualizaciones: nixpkgs, veyl');
    n.updates = [];
    n.pendingRebuild = true;
    n.lockNewer = true;
    assert.equal(n.summaryLine(), 'flake.lock actualizado · falta reconstruir');
    n.running = true;
    n.job = { state: 'running', phase: 'Compilando…' };
    assert.equal(n.summaryLine(), 'Compilando…');
    n.authNeeded = true;
    assert.equal(n.summaryLine(), 'Esperando contraseña de sudo');
});

function daemon() {
    const d = load('Daemon', {
        thinking: null, card: null, shut: null, query: '', serial: 0, connected: true, sent: [], Recall: {},
        link: { write(text) { d.sent.push(JSON.parse(text)); }, flush() {} },
    });
    return d;
}

test('la tarjeta del cerebro sale en cuanto se compone y se completa sin rehacerse', () => {
    const d = daemon();
    const say = message => d.receive(JSON.stringify(message));
    say({ thinking: { said: '¿cuándo sale GTA 6?', since: 1, n: 4, stop: true, steps: [] } });
    say({ card: { n: 4, live: true, titulo: 'GTA VI', bloques: [{ tipo: 'texto', texto: 'Sale en 2026' }] } });
    // Cada paso de la misma pregunta no se lleva la tarjeta.
    say({ thinking: { said: '¿cuándo sale GTA 6?', since: 1, n: 4, steps: [{ icono: 'globe', texto: 'Lee ign.com' }] } });
    assert.equal(d.card.titulo, 'GTA VI');
    say({ card_patch: { n: 4, imagen: 'https://ign.com/gta.jpg' } });
    say({ card_patch: { n: 9, imagen: 'https://otra.com/x.jpg' } });
    assert.equal(d.card.imagen, 'https://ign.com/gta.jpg');
    assert.equal(d.card.live, true);
    say({ card: { n: 4, titulo: 'GTA VI', say: 'Sale el 19 de noviembre', imagen: 'https://ign.com/gta.jpg', bloques: [] } });
    assert.equal(d.card.live, undefined);
    say({ thinking: null });
    assert.equal(d.card.say, 'Sale el 19 de noviembre');
});

test('una tarjeta cerrada no vuelve aunque le llegue algo; una pregunta nueva la recoge', () => {
    const d = daemon();
    const say = message => d.receive(JSON.stringify(message));
    say({ thinking: { said: 'x', since: 1, n: 5 } });
    say({ card: { n: 5, live: true, titulo: 'X', bloques: [] } });
    d.close();
    say({ card_patch: { n: 5, imagen: 'https://a/b.jpg' } });
    say({ card: { n: 5, titulo: 'X', say: 'Hecho', bloques: [] } });
    assert.equal(d.card, null);
    // Las de la gramática no llevan número: siempre salen.
    say({ card: { titulo: 'Hoy', bloques: [] } });
    assert.equal(d.card.titulo, 'Hoy');
    say({ thinking: { said: 'y', since: 2, n: 6, stop: true } });
    assert.equal(d.card, null);
    d.stop();
    assert.deepEqual(d.sent.at(-1), { stop: 6 });
    // Un veyld de antes no numera: cada pregunta recoge la tarjeta, como siempre.
    say({ card: { titulo: 'Z', bloques: [] } });
    say({ thinking: { said: 'z', since: 3 } });
    assert.equal(d.card, null);
});
