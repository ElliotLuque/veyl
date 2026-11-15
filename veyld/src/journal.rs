//! El diario: qué tuvo el foco y cuándo, con su título, su carpeta y lo que
//! corría en ella, y en un navegador, la página; qué ventanas se abrieron y
//! se cerraron; cuándo trabajaron los agentes; qué sonó; cuándo estuvieron
//! en uso el micro, la cámara o la pantalla compartida; qué archivos se
//! abrieron; cuándo no estabas (y si la máquina dormía); y lo que se le pidió
//! a Veyl y qué hizo. En SQLite ($XDG_STATE_HOME/veyl/journal.db), para poder
//! preguntarle después «¿qué estuve mirando el jueves por la tarde?».

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{Connection, OpenFlags, params};
use serde::Serialize;

use crate::agents::Agent;
use crate::media::Playing;
use crate::recent::Used;
use crate::words::fold;

/// Lo que se guarda: 90 días.
pub const KEEP: i64 = 90 * 24 * 3600 * 1000;
/// Un tramo más corto que esto no se apunta (pasar de largo por una ventana).
const GLANCE: i64 = 1000;
/// Un título que cambia nada más llegar a la ventana sustituye al anterior.
const SETTLE: i64 = 3000;
/// Una pista (o un micro) que dura menos que esto no se apunta: pasar de canción.
const SKIP: i64 = 5000;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS focus (
    id INTEGER PRIMARY KEY,
    start INTEGER NOT NULL,              -- ms desde 1970
    end INTEGER NOT NULL,
    window TEXT NOT NULL,                -- dirección en Hyprland, «0x…»
    app TEXT NOT NULL,                   -- su clase
    title TEXT NOT NULL,
    folder TEXT NOT NULL DEFAULT '',     -- la carpeta en la que se trabaja
    workspace TEXT NOT NULL DEFAULT '',
    pid INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS focus_time ON focus (start, end);
CREATE TABLE IF NOT EXISTS windows (
    id INTEGER PRIMARY KEY,
    window TEXT NOT NULL,
    app TEXT NOT NULL,
    title TEXT NOT NULL,
    workspace TEXT NOT NULL DEFAULT '',
    opened INTEGER NOT NULL,
    closed INTEGER
);
CREATE INDEX IF NOT EXISTS windows_time ON windows (opened);
CREATE TABLE IF NOT EXISTS agents (
    id INTEGER PRIMARY KEY,
    agent TEXT NOT NULL,                 -- «claude:1234»
    provider TEXT NOT NULL,
    label TEXT NOT NULL,
    name TEXT NOT NULL,
    folder TEXT NOT NULL,
    model TEXT NOT NULL,
    phase TEXT NOT NULL,                 -- working o needs
    start INTEGER NOT NULL,
    end INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS agents_time ON agents (start, end);
CREATE TABLE IF NOT EXISTS away (
    id INTEGER PRIMARY KEY,
    start INTEGER NOT NULL,
    end INTEGER NOT NULL,
    reason TEXT NOT NULL                 -- inactivo o bloqueado
);
CREATE TABLE IF NOT EXISTS orders (
    id INTEGER PRIMARY KEY,
    time INTEGER NOT NULL,
    source TEXT NOT NULL,                -- voice, launcher, cli
    said TEXT NOT NULL,
    intent TEXT,                         -- vacío: la gramática no lo entendió
    args TEXT,
    handler TEXT NOT NULL,               -- grammar, llm o none
    ok INTEGER NOT NULL,
    answer TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS orders_time ON orders (time);
";

/// Lo que se ha ido añadiendo, en orden: la base de datos sabe por dónde va
/// (`PRAGMA user_version`).
const MIGRATIONS: &[&str] = &[
    "
ALTER TABLE focus ADD COLUMN command TEXT NOT NULL DEFAULT '';  -- lo que corre en la terminal: «cargo test»
ALTER TABLE focus ADD COLUMN url TEXT NOT NULL DEFAULT '';      -- en un navegador, la página
ALTER TABLE focus ADD COLUMN site TEXT NOT NULL DEFAULT '';     -- y su sitio: «github.com»
CREATE TABLE IF NOT EXISTS media (
    id INTEGER PRIMARY KEY,
    start INTEGER NOT NULL,
    end INTEGER NOT NULL,
    player TEXT NOT NULL,                -- «Spotify», «Mozilla Firefox»
    title TEXT NOT NULL,
    artist TEXT NOT NULL DEFAULT '',
    album TEXT NOT NULL DEFAULT '',
    url TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS media_time ON media (start, end);
CREATE TABLE IF NOT EXISTS devices (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL,                  -- micro, cámara o pantalla (compartida)
    start INTEGER NOT NULL,
    end INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS devices_time ON devices (start, end);
CREATE TABLE IF NOT EXISTS files (
    id INTEGER PRIMARY KEY,
    time INTEGER NOT NULL,               -- la última vez que esa aplicación lo abrió
    path TEXT NOT NULL,
    app TEXT NOT NULL,
    UNIQUE (path, app, time)
);
CREATE INDEX IF NOT EXISTS files_time ON files (time);
CREATE INDEX IF NOT EXISTS away_time ON away (start, end);
",
    "
ALTER TABLE orders ADD COLUMN card TEXT NOT NULL DEFAULT '';    -- la tarjeta que compuso el cerebro, en JSON
",
];

/// Las clases de ventana que son terminales. Antes de la versión 1 se
/// apuntaba la carpeta de cualquier programa (la de quien lo lanzó).
const TERMINALS: &[&str] = &[
    "kitty",
    "foot",
    "alacritty",
    "wezterm",
    "ghostty",
    "konsole",
    "terminal",
    "console",
    "xterm",
    "terminator",
    "tilix",
    "rio",
    "st-256color",
    "contour",
    "blackbox",
];

fn migrate(db: &Connection) -> rusqlite::Result<()> {
    let version: usize = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version) {
        let tx = db.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        if i == 0 {
            let apps: Vec<String> = {
                let mut q = tx.prepare("SELECT DISTINCT app FROM focus")?;
                q.query_map([], |r| r.get(0))?.flatten().collect()
            };
            for app in apps.iter().filter(|a| !TERMINALS.iter().any(|t| a.to_lowercase().contains(t))) {
                tx.execute("UPDATE focus SET folder = '' WHERE app = ?1", [app])?;
            }
        }
        tx.execute_batch(&format!("PRAGMA user_version = {}", i + 1))?;
        tx.commit()?;
    }
    Ok(())
}

/// Lo que tiene el foco ahora.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Seen {
    pub window: String,
    pub app: String,
    pub title: String,
    pub folder: String,
    /// Lo que corre en primer plano, en una terminal.
    pub command: String,
    pub workspace: String,
    pub pid: i32,
}

struct Open {
    seen: Seen,
    start: i64,
    row: Option<i64>,
}

pub struct Journal {
    db: Connection,
    open: Option<Open>,
    /// Agente → (fila, fase) de lo que está haciendo.
    agents: HashMap<String, (i64, String)>,
    away: Option<i64>,
    windows: HashMap<String, i64>,
    /// Reproductor → (fila, título y artista) de lo que suena.
    media: HashMap<String, (i64, (String, String))>,
    /// Micro, cámara, pantalla → fila mientras están en uso.
    devices: HashMap<String, i64>,
}

pub fn open_db(path: &Path, write: bool) -> rusqlite::Result<Connection> {
    let flags = if write { OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE } else { OpenFlags::SQLITE_OPEN_READ_ONLY };
    let db = Connection::open_with_flags(path, flags)?;
    db.busy_timeout(std::time::Duration::from_secs(2))?;
    if write {
        db.pragma_update(None, "journal_mode", "WAL")?;
        db.pragma_update(None, "synchronous", "NORMAL")?;
        db.execute_batch(SCHEMA)?;
        migrate(&db)?;
    }
    Ok(db)
}

impl Journal {
    #[cfg(test)]
    pub fn memory() -> Journal {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(SCHEMA).unwrap();
        migrate(&db).unwrap();
        Journal::with(db)
    }

    pub fn open(path: &Path) -> Result<Journal, Box<dyn std::error::Error>> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // Fija los permisos ANTES de abrir SQLite: WAL y SHM heredan los de
        // la base de datos y también pueden contener títulos privados.
        {
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            let file = std::fs::OpenOptions::new().write(true).create(true).truncate(false).mode(0o600).open(path)?;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
            for suffix in ["-wal", "-shm"] {
                let mut sidecar = path.as_os_str().to_os_string();
                sidecar.push(suffix);
                match std::fs::set_permissions(&sidecar, std::fs::Permissions::from_mode(0o600)) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e.into()),
                }
            }
        }
        let db = open_db(path, true)?;
        // Lo que quedó abierto la última vez acabó cuando se apuntó por última vez.
        Ok(Journal::with(db))
    }

    fn with(db: Connection) -> Journal {
        Journal { db, open: None, agents: HashMap::new(), away: None, windows: HashMap::new(), media: HashMap::new(), devices: HashMap::new() }
    }

    fn insert_focus(&self, start: i64, end: i64, s: &Seen) -> Option<i64> {
        self.db
            .execute(
                "INSERT INTO focus (start, end, window, app, title, folder, command, workspace, pid) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![start, end, s.window, s.app, s.title, s.folder, s.command, s.workspace, s.pid],
            )
            .ok()
            .map(|_| self.db.last_insert_rowid())
    }

    /// Lo que tiene el foco desde `now` (nada: no se mira nada).
    pub fn focus(&mut self, now: i64, seen: Option<Seen>) {
        if let (Some(open), Some(seen)) = (&mut self.open, &seen) {
            if open.seen == *seen {
                return;
            }
            // El mismo sitio con otro nombre recién llegado: el nombre bueno es este.
            if open.seen.window == seen.window && now - open.start < SETTLE {
                open.seen = seen.clone();
                if let Some(row) = open.row {
                    let _ = self.db.execute(
                        "UPDATE focus SET title = ?1, folder = ?2, command = ?3, workspace = ?4, app = ?5, pid = ?6, url = '', site = '' WHERE id = ?7",
                        params![seen.title, seen.folder, seen.command, seen.workspace, seen.app, seen.pid, row],
                    );
                }
                return;
            }
        }
        self.close(now);
        self.open = seen.map(|seen| Open { seen, start: now, row: None });
    }

    fn close(&mut self, now: i64) {
        let Some(open) = self.open.take() else {
            return;
        };
        let end = now.max(open.start);
        match open.row {
            Some(row) if end - open.start < GLANCE => {
                let _ = self.db.execute("DELETE FROM focus WHERE id = ?1", params![row]);
            }
            Some(row) => {
                let _ = self.db.execute("UPDATE focus SET end = ?1 WHERE id = ?2", params![end, row]);
            }
            None if end - open.start >= GLANCE => {
                self.insert_focus(open.start, end, &open.seen);
            }
            None => {}
        }
    }

    /// Cada poco, lo abierto llega hasta ahora: si veyld se cae, se pierde poco.
    pub fn beat(&mut self, now: i64) {
        match self.open.as_ref().map(|o| (o.row, o.start)) {
            Some((Some(row), start)) => {
                let _ = self.db.execute("UPDATE focus SET end = ?1 WHERE id = ?2", params![now.max(start), row]);
            }
            Some((None, start)) if now - start >= GLANCE => {
                let seen = self.open.as_ref().map(|o| o.seen.clone()).unwrap_or_default();
                let row = self.insert_focus(start, now, &seen);
                if let Some(open) = &mut self.open {
                    open.row = row;
                }
            }
            _ => {}
        }
        for (row, _) in self.agents.values() {
            let _ = self.db.execute("UPDATE agents SET end = ?1 WHERE id = ?2", params![now, row]);
        }
        for (row, _) in self.media.values() {
            let _ = self.db.execute("UPDATE media SET end = ?1 WHERE id = ?2", params![now, row]);
        }
        for row in self.devices.values() {
            let _ = self.db.execute("UPDATE devices SET end = ?1 WHERE id = ?2", params![now, row]);
        }
        if let Some(row) = self.away {
            let _ = self.db.execute("UPDATE away SET end = ?1 WHERE id = ?2", params![now, row]);
        }
    }

    /// No estás desde `since` (por `reason`), o vuelves (`None`).
    pub fn away(&mut self, since: i64, reason: Option<&str>) {
        match (reason, self.away) {
            (Some(reason), None) => {
                self.close(since);
                // El aviso llega dos minutos tarde. También hay que recortar
                // los tramos ya cerrados por cambios de título durante ese rato.
                let _ = self.db.execute("DELETE FROM focus WHERE start >= ?1", params![since]);
                let _ = self.db.execute("UPDATE focus SET end = ?1 WHERE end > ?1", params![since]);
                if self.db.execute("INSERT INTO away (start, end, reason) VALUES (?1, ?1, ?2)", params![since, reason]).is_ok() {
                    self.away = Some(self.db.last_insert_rowid());
                }
            }
            (None, Some(row)) => {
                let _ = self.db.execute("UPDATE away SET end = ?1 WHERE id = ?2", params![since, row]);
                self.away = None;
            }
            _ => {}
        }
    }

    pub fn opened(&mut self, now: i64, window: &str, app: &str, title: &str, workspace: &str) {
        if self.windows.contains_key(window) {
            return;
        }
        if self
            .db
            .execute("INSERT INTO windows (window, app, title, workspace, opened) VALUES (?1, ?2, ?3, ?4, ?5)", params![window, app, title, workspace, now])
            .is_ok()
        {
            self.windows.insert(window.to_string(), self.db.last_insert_rowid());
        }
    }

    pub fn closed(&mut self, now: i64, window: &str) {
        if let Some(row) = self.windows.remove(window) {
            let _ = self.db.execute("UPDATE windows SET closed = ?1 WHERE id = ?2", params![now, row]);
        }
    }

    /// Al arrancar: las ventanas que siguen abiertas siguen con su fila; las
    /// demás se cerraron mientras veyld no miraba.
    pub fn windows_now(&mut self, now: i64, open: &[(String, String, String, String)]) {
        let rows: Vec<(i64, String)> = {
            let Ok(mut q) = self.db.prepare("SELECT id, window FROM windows WHERE closed IS NULL") else {
                return;
            };
            q.query_map([], |r| Ok((r.get(0)?, r.get(1)?))).map(|rows| rows.flatten().collect()).unwrap_or_default()
        };
        for (row, window) in rows {
            if open.iter().any(|(w, ..)| *w == window) {
                self.windows.insert(window, row);
            } else {
                let _ = self.db.execute("UPDATE windows SET closed = ?1 WHERE id = ?2", params![now, row]);
            }
        }
        for (window, app, title, workspace) in open {
            self.opened(now, window, app, title, workspace);
        }
    }

    /// Los agentes que trabajan o esperan, cada uno en su fila por fase.
    pub fn agents(&mut self, now: i64, agents: &[Agent]) {
        let mut live = Vec::new();
        for agent in agents.iter().filter(|a| a.phase == "working" || a.phase == "needs") {
            live.push(agent.id.clone());
            if self.agents.get(&agent.id).is_some_and(|(_, phase)| *phase == agent.phase) {
                continue;
            }
            if let Some((row, _)) = self.agents.remove(&agent.id) {
                let _ = self.db.execute("UPDATE agents SET end = ?1 WHERE id = ?2", params![now, row]);
            }
            if self
                .db
                .execute(
                    "INSERT INTO agents (agent, provider, label, name, folder, model, phase, start, end) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                    params![agent.id, agent.provider, agent.label, agent.name, agent.cwd, agent.model, agent.phase, now],
                )
                .is_ok()
            {
                self.agents.insert(agent.id.clone(), (self.db.last_insert_rowid(), agent.phase.clone()));
            }
        }
        let gone: Vec<String> = self.agents.keys().filter(|id| !live.contains(id)).cloned().collect();
        for id in gone {
            if let Some((row, _)) = self.agents.remove(&id) {
                let _ = self.db.execute("UPDATE agents SET end = ?1 WHERE id = ?2", params![now, row]);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn order(
        &mut self,
        now: i64,
        source: &str,
        said: &str,
        intent: Option<&str>,
        args: &str,
        handler: &str,
        ok: bool,
        answer: &str,
        card: &str,
    ) -> Option<i64> {
        self.db
            .execute(
                "INSERT INTO orders (time, source, said, intent, args, handler, ok, answer, card) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![now, source, said, intent, args, handler, ok, answer, card],
            )
            .ok()
            .map(|_| self.db.last_insert_rowid())
    }

    /// Lo último que se le pidió a Veyl, de lo más reciente a lo más antiguo
    /// (o solo la consulta `id`): para el historial del centro de control.
    pub fn asked(&self, limit: usize, id: Option<i64>) -> Vec<serde_json::Value> {
        let sql =
            "SELECT id, time, source, said, intent, handler, ok, answer, card FROM orders WHERE ?1 IS NULL OR id = ?1 ORDER BY time DESC, id DESC LIMIT ?2";
        let Ok(mut q) = self.db.prepare(sql) else {
            return Vec::new();
        };
        q.query_map(params![id, limit as i64], |r| {
            let answer: String = r.get(7)?;
            let card: String = r.get(8)?;
            let (say, more) = answer.split_once('\n').unwrap_or((&answer, ""));
            Ok(serde_json::json!({
                "id": r.get::<_, i64>(0)?,
                "time": r.get::<_, i64>(1)?,
                "source": r.get::<_, String>(2)?,
                "said": r.get::<_, String>(3)?,
                "intent": r.get::<_, Option<String>>(4)?,
                "handler": r.get::<_, String>(5)?,
                "ok": r.get::<_, bool>(6)?,
                "say": say,
                "more": more.trim(),
                "card": serde_json::from_str::<serde_json::Value>(&card).unwrap_or(serde_json::Value::Null),
            }))
        })
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
    }

    /// Olvida una consulta del historial.
    pub fn forget_order(&mut self, id: i64) -> bool {
        self.db.execute("DELETE FROM orders WHERE id = ?1", params![id]).is_ok_and(|n| n > 0)
    }

    /// Pone su página a lo mirado en un navegador desde `since`, cuando
    /// `url(app, título)` la sabe: el navegador la guarda unos segundos tarde.
    pub fn pages(&mut self, since: i64, mut url: impl FnMut(&str, &str) -> Option<String>) {
        let rows: Vec<(i64, String, String)> = {
            let Ok(mut q) = self.db.prepare("SELECT id, app, title FROM focus WHERE url = '' AND title != '' AND end >= ?1") else {
                return;
            };
            q.query_map([since], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).map(|rows| rows.flatten().collect()).unwrap_or_default()
        };
        for (row, app, title) in rows {
            if let Some(url) = url(&app, &title) {
                let _ = self.db.execute("UPDATE focus SET url = ?1, site = ?2 WHERE id = ?3", params![url, crate::web::site(&url), row]);
            }
        }
    }

    /// Lo que suena ahora (nada: no suena nada).
    pub fn media(&mut self, now: i64, playing: &[Playing]) {
        for p in playing {
            let key = (p.title.clone(), p.artist.clone());
            if self.media.get(&p.bus).is_some_and(|(_, k)| *k == key) {
                continue;
            }
            self.silence(now, &p.bus);
            if self
                .db
                .execute(
                    "INSERT INTO media (start, end, player, title, artist, album, url) VALUES (?1, ?1, ?2, ?3, ?4, ?5, ?6)",
                    params![now, p.player, p.title, p.artist, p.album, p.url],
                )
                .is_ok()
            {
                self.media.insert(p.bus.clone(), (self.db.last_insert_rowid(), key));
            }
        }
        let gone: Vec<String> = self.media.keys().filter(|bus| !playing.iter().any(|p| p.bus == **bus)).cloned().collect();
        for bus in gone {
            self.silence(now, &bus);
        }
    }

    fn silence(&mut self, now: i64, bus: &str) {
        if let Some((row, _)) = self.media.remove(bus) {
            let _ = self.db.execute("UPDATE media SET end = ?1 WHERE id = ?2", params![now, row]);
            let _ = self.db.execute("DELETE FROM media WHERE id = ?1 AND end - start < ?2", params![row, SKIP]);
        }
    }

    /// El micro, la cámara o la pantalla compartida, en uso o no.
    pub fn device(&mut self, now: i64, kind: &str, on: bool) {
        match (on, self.devices.remove(kind)) {
            (true, None) => {
                if self.db.execute("INSERT INTO devices (kind, start, end) VALUES (?1, ?2, ?2)", params![kind, now]).is_ok() {
                    self.devices.insert(kind.to_string(), self.db.last_insert_rowid());
                }
            }
            (true, Some(row)) => {
                self.devices.insert(kind.to_string(), row);
            }
            (false, Some(row)) => {
                let _ = self.db.execute("UPDATE devices SET end = ?1 WHERE id = ?2", params![now, row]);
                let _ = self.db.execute("DELETE FROM devices WHERE id = ?1 AND end - start < ?2", params![row, SKIP]);
            }
            (false, None) => {}
        }
    }

    /// Los archivos abiertos desde `since`; los ya apuntados no se repiten.
    pub fn files(&mut self, used: &[Used], since: i64) -> usize {
        let Ok(tx) = self.db.unchecked_transaction() else {
            return 0;
        };
        let mut added = 0;
        for u in used.iter().filter(|u| u.time >= since) {
            added += tx.execute("INSERT OR IGNORE INTO files (time, path, app) VALUES (?1, ?2, ?3)", params![u.time, u.path, u.app]).unwrap_or(0);
        }
        if tx.commit().is_err() { 0 } else { added }
    }

    /// La máquina durmió de `from` a `to`: lo abierto acabó al dormirse.
    pub fn slept(&mut self, from: i64, to: i64) {
        self.finish(from);
        let _ = self.db.execute("INSERT INTO away (start, end, reason) VALUES (?1, ?2, 'suspendido')", params![from, to.max(from)]);
    }

    /// Olvida lo que tiene más de 90 días.
    pub fn prune(&mut self, now: i64) {
        let before = now - KEEP;
        for sql in [
            "DELETE FROM focus WHERE end < ?1",
            "DELETE FROM windows WHERE closed < ?1",
            "DELETE FROM agents WHERE end < ?1",
            "DELETE FROM away WHERE end < ?1",
            "DELETE FROM orders WHERE time < ?1",
            "DELETE FROM media WHERE end < ?1",
            "DELETE FROM devices WHERE end < ?1",
            "DELETE FROM files WHERE time < ?1",
        ] {
            let _ = self.db.execute(sql, params![before]);
        }
    }

    /// Deja de registrar sin mantener ventanas o agentes abiertos en memoria.
    pub fn pause(&mut self, now: i64) {
        self.finish(now);
        for (_, row) in self.windows.drain() {
            let _ = self.db.execute("UPDATE windows SET closed = ?1 WHERE id = ?2", params![now, row]);
        }
    }

    pub fn db(&self) -> &Connection {
        &self.db
    }

    /// Al salir: lo abierto se cierra ahora.
    pub fn finish(&mut self, now: i64) {
        self.close(now);
        self.agents(now, &[]);
        self.away(now, None);
        self.media(now, &[]);
        let kinds: Vec<String> = self.devices.keys().cloned().collect();
        for kind in kinds {
            self.device(now, &kind, false);
        }
    }
}

/// Un tramo de lo que tuvo el foco, recortado al tiempo por el que se pregunta.
#[derive(Clone, Debug, Default)]
pub struct Row {
    pub start: i64,
    pub end: i64,
    pub app: String,
    pub title: String,
    pub folder: String,
    pub command: String,
    pub url: String,
    pub site: String,
}

impl Row {
    /// De una consulta que pide, por orden: start, end, app, title, folder, command, url, site.
    fn read(r: &rusqlite::Row) -> rusqlite::Result<Row> {
        Ok(Row { start: r.get(0)?, end: r.get(1)?, app: r.get(2)?, title: r.get(3)?, folder: r.get(4)?, command: r.get(5)?, url: r.get(6)?, site: r.get(7)? })
    }

    /// Dónde: el sitio en un navegador, la carpeta en una terminal.
    pub fn place(&self) -> &str {
        if self.site.is_empty() { &self.folder } else { &self.site }
    }

    /// Lo que se dice de él para buscar, sin tildes ni mayúsculas.
    pub fn hay(&self, label: &str) -> String {
        fold(&[self.app.as_str(), label, &self.title, &self.site, &self.url, &self.folder, &self.command].join(" "))
    }
}

/// Lo que tuvo el foco entre `from` y `to` (ms), por orden.
pub fn rows(db: &Connection, from: i64, to: i64) -> rusqlite::Result<Vec<Row>> {
    let mut q = db.prepare(
        "SELECT MAX(start, ?1), MIN(end, ?2), app, title, folder, command, url, site FROM focus
         WHERE end > ?1 AND start < ?2 ORDER BY start",
    )?;
    let rows = q.query_map(params![from, to], Row::read)?;
    rows.collect()
}

/// Lo que hubo en un tramo de tiempo.
#[derive(Debug, Default, Serialize)]
pub struct Recall {
    /// ms con algo delante.
    pub total: i64,
    /// Lo primero y lo último que tuvo el foco (ms; 0, nada).
    pub first: i64,
    pub last: i64,
    pub apps: Vec<AppTime>,
    pub agents: Vec<AgentTime>,
    pub media: Vec<MediaTime>,
    pub devices: Vec<DeviceTime>,
    /// Los archivos abiertos, el último primero.
    pub files: Vec<Used>,
    /// Lo que no estuviste, por qué: inactivo, bloqueado, suspendido.
    pub away: Vec<(String, i64)>,
}

#[derive(Debug, Default, Serialize)]
pub struct AppTime {
    pub app: String,
    pub ms: i64,
    pub titles: Vec<(String, i64)>,
    /// Las carpetas, con las órdenes que corrieron en cada una.
    pub folders: Vec<Part>,
    /// Los sitios, con sus páginas.
    pub sites: Vec<Part>,
}

/// Una parte del tiempo de una aplicación y lo que hubo dentro.
#[derive(Debug, Default, PartialEq, Serialize)]
pub struct Part {
    pub name: String,
    pub ms: i64,
    pub within: Vec<(String, i64)>,
}

#[derive(Debug, Default, Serialize)]
pub struct AgentTime {
    pub label: String,
    pub name: String,
    pub folder: String,
    pub ms: i64,
}

#[derive(Debug, Default, Serialize)]
pub struct MediaTime {
    pub player: String,
    pub title: String,
    pub artist: String,
    pub ms: i64,
}

#[derive(Debug, Default, Serialize)]
pub struct DeviceTime {
    /// micro, cámara o pantalla.
    pub kind: String,
    pub ms: i64,
    /// Cuántas veces.
    pub times: usize,
    /// Lo que más tuvo el foco mientras: la aplicación y su sitio.
    pub app: String,
    pub site: String,
}

fn add(list: &mut Vec<(String, i64)>, key: &str, ms: i64) {
    match list.iter_mut().find(|(k, _)| k == key) {
        Some((_, total)) => *total += ms,
        None => list.push((key.to_string(), ms)),
    }
}

fn add_part(parts: &mut Vec<Part>, name: &str, within: &str, ms: i64) {
    let part = match parts.iter().position(|p| p.name == name) {
        Some(i) => &mut parts[i],
        None => {
            parts.push(Part { name: name.to_string(), ..Part::default() });
            parts.last_mut().expect("recién puesta")
        }
    };
    part.ms += ms;
    if !within.is_empty() {
        add(&mut part.within, within, ms);
    }
}

fn by_time(list: &mut [(String, i64)]) {
    list.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
}

/// Lo que tuvo el foco entre `from` y `to` (ms), por aplicación, y lo demás
/// que pasó mientras.
pub fn recall(db: &Connection, from: i64, to: i64) -> rusqlite::Result<Recall> {
    let mut out = Recall::default();
    if from >= to {
        return Ok(out);
    }
    let focus = rows(db, from, to)?;
    let mut apps: Vec<AppTime> = Vec::new();
    for row in &focus {
        let ms = row.end - row.start;
        out.total += ms;
        out.first = if out.first == 0 { row.start } else { out.first.min(row.start) };
        out.last = out.last.max(row.end);
        let app = match apps.iter().position(|a| a.app == row.app) {
            Some(i) => &mut apps[i],
            None => {
                apps.push(AppTime { app: row.app.clone(), ..AppTime::default() });
                apps.last_mut().expect("recién puesta")
            }
        };
        app.ms += ms;
        if !row.title.is_empty() {
            add(&mut app.titles, &row.title, ms);
        }
        if !row.folder.is_empty() {
            add_part(&mut app.folders, &row.folder, row.command.split(' ').next().unwrap_or(""), ms);
        }
        if !row.site.is_empty() {
            add_part(&mut app.sites, &row.site, &row.title, ms);
        }
    }
    apps.sort_by_key(|a| std::cmp::Reverse(a.ms));
    for app in &mut apps {
        by_time(&mut app.titles);
        for parts in [&mut app.folders, &mut app.sites] {
            parts.sort_by_key(|p| std::cmp::Reverse(p.ms));
            for part in parts.iter_mut() {
                by_time(&mut part.within);
            }
        }
    }
    out.apps = apps;

    let mut q = db.prepare(
        "SELECT label, name, folder, SUM(MIN(end, ?2) - MAX(start, ?1)) AS ms FROM agents
         WHERE phase = 'working' AND end > ?1 AND start < ?2 GROUP BY label, name, folder ORDER BY ms DESC",
    )?;
    let rows = q.query_map(params![from, to], |r| Ok(AgentTime { label: r.get(0)?, name: r.get(1)?, folder: r.get(2)?, ms: r.get(3)? }))?;
    out.agents = rows.collect::<Result<_, _>>()?;

    let mut q = db.prepare(
        "SELECT player, title, artist, SUM(MIN(end, ?2) - MAX(start, ?1)) AS ms FROM media
         WHERE end > ?1 AND start < ?2 GROUP BY player, title, artist ORDER BY ms DESC",
    )?;
    let rows = q.query_map(params![from, to], |r| Ok(MediaTime { player: r.get(0)?, title: r.get(1)?, artist: r.get(2)?, ms: r.get(3)? }))?;
    out.media = rows.collect::<Result<_, _>>()?;

    let mut q = db.prepare("SELECT kind, MAX(start, ?1), MIN(end, ?2) FROM devices WHERE end > ?1 AND start < ?2 ORDER BY start")?;
    let spans = q.query_map(params![from, to], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)))?;
    let mut devices: Vec<(DeviceTime, Vec<(String, i64)>)> = Vec::new();
    for span in spans {
        let (kind, start, end) = span?;
        let (device, apps) = match devices.iter().position(|(d, _)| d.kind == kind) {
            Some(i) => &mut devices[i],
            None => {
                devices.push((DeviceTime { kind: kind.clone(), ..DeviceTime::default() }, Vec::new()));
                devices.last_mut().expect("recién puesto")
            }
        };
        device.ms += end - start;
        device.times += 1;
        for row in focus.iter().filter(|r| r.end > start && r.start < end) {
            add(apps, &format!("{}\n{}", row.app, row.site), row.end.min(end) - row.start.max(start));
        }
    }
    for (mut device, mut apps) in devices {
        by_time(&mut apps);
        if let Some((top, _)) = apps.first() {
            let (app, site) = top.split_once('\n').unwrap_or((top, ""));
            (device.app, device.site) = (app.to_string(), site.to_string());
        }
        out.devices.push(device);
    }

    let mut q = db.prepare("SELECT path, app, MAX(time) AS last FROM files WHERE time >= ?1 AND time < ?2 GROUP BY path ORDER BY last DESC")?;
    let rows = q.query_map(params![from, to], |r| Ok(Used { path: r.get(0)?, app: r.get(1)?, time: r.get(2)? }))?;
    out.files = rows.collect::<Result<_, _>>()?;

    // Las pausas: lo que no estuviste entre lo primero y lo último.
    let mut q =
        db.prepare("SELECT reason, SUM(MIN(end, ?2) - MAX(start, ?1)) AS ms FROM away WHERE end > ?1 AND start < ?2 GROUP BY reason ORDER BY ms DESC")?;
    let rows = q.query_map(params![out.first, out.last], |r| Ok((r.get(0)?, r.get(1)?)))?;
    out.away = rows.collect::<Result<_, _>>()?;
    Ok(out)
}

/// Todo lo que se sabe de un tramo de tiempo, para contarlo con detalle.
#[derive(Debug, Default, Serialize)]
pub struct Report {
    pub from: i64,
    pub to: i64,
    #[serde(flatten)]
    pub recall: Recall,
    /// Los ratos seguidos delante (sin pausas de más de diez minutos).
    pub sessions: Vec<(i64, i64)>,
    /// Cuántas veces cambió la ventana con el foco.
    pub switches: usize,
    /// El tiempo por horas (o por días, si es más de un día) y lo que más hubo en cada una.
    pub slices: Vec<Slice>,
    /// Por proyecto (la raíz git de la carpeta, o la carpeta).
    pub projects: Vec<Project>,
    /// Por nivel (espacio de trabajo).
    pub levels: Vec<(String, i64)>,
    pub agents: Vec<AgentDetail>,
    /// Cada vez que se abrió el micro, la cámara o se compartió la pantalla.
    pub calls: Vec<Call>,
    /// Las ventanas que se abrieron, por aplicación, y cuántas se cerraron.
    pub opened: Vec<(String, i64)>,
    pub closed: i64,
    /// Lo que se le pidió a Veyl.
    pub orders: Vec<Asked>,
}

#[derive(Debug, Default, Serialize)]
pub struct Slice {
    pub start: i64,
    pub end: i64,
    pub ms: i64,
    pub app: String,
    /// Dónde, dentro de esa aplicación: el sitio o la carpeta que más.
    pub place: String,
}

#[derive(Debug, Default, Serialize)]
pub struct Project {
    pub path: String,
    /// Delante, en ventanas que trabajaban en él.
    pub ms: i64,
    pub apps: Vec<(String, i64)>,
    pub commands: Vec<(String, i64)>,
    /// Lo que trabajaron los agentes en él.
    pub agents: i64,
}

#[derive(Debug, Default, Serialize)]
pub struct AgentDetail {
    pub id: String,
    pub label: String,
    pub name: String,
    pub folder: String,
    pub model: String,
    pub working: i64,
    /// Lo que estuvo esperándote, y cuántas veces.
    pub waiting: i64,
    pub waits: usize,
    pub first: i64,
    pub last: i64,
}

#[derive(Debug, Default, Serialize)]
pub struct Call {
    pub kind: String,
    pub start: i64,
    pub end: i64,
    pub app: String,
    pub site: String,
}

#[derive(Debug, Default, Serialize)]
pub struct Asked {
    pub time: i64,
    pub source: String,
    pub said: String,
    pub ok: bool,
    pub answer: String,
}

/// Lo que hace de una carpeta un proyecto, si no es un repositorio git.
const MARKS: &[&str] = &["flake.nix", "Cargo.toml", "package.json", "pyproject.toml", "go.mod", "Makefile", "CMakeLists.txt", "README.md"];

/// La raíz del proyecto de una carpeta: la primera de arriba con `.git`; si
/// no hay, la más alta (por debajo de casa) con algo de `MARKS`; si tampoco,
/// la carpeta misma (pero no la de casa ni la raíz).
pub fn project(folder: &str, home: &str, cache: &mut HashMap<String, Option<String>>) -> Option<String> {
    if let Some(known) = cache.get(folder) {
        return known.clone();
    }
    let (mut git, mut marked) = (None, None);
    let mut at = Some(Path::new(folder));
    while let Some(dir) = at {
        let text = dir.to_string_lossy();
        if text == home || text == "/" || !text.starts_with(home) {
            break;
        }
        if dir.join(".git").exists() {
            git = Some(text.into_owned());
            break;
        }
        if MARKS.iter().any(|m| dir.join(m).exists()) {
            marked = Some(text.into_owned());
        }
        at = dir.parent();
    }
    let found = git.or(marked).or_else(|| Some(folder.to_string()).filter(|f| f != home && f != "/" && !f.is_empty()));
    cache.insert(folder.to_string(), found.clone());
    found
}

/// Todo lo de un tramo. `slice` es el tamaño de los trozos de `slices`,
/// contados desde `origin` (la medianoche).
pub fn report(db: &Connection, from: i64, to: i64, slice: i64, origin: i64, home: &str) -> rusqlite::Result<Report> {
    let mut out = Report { from, to, recall: recall(db, from, to)?, ..Report::default() };
    if from >= to {
        return Ok(out);
    }
    let focus = rows(db, from, to)?;

    // Sesiones y cambios de ventana.
    const PAUSE: i64 = 10 * 60 * 1000;
    let mut q = db.prepare("SELECT window FROM focus WHERE end > ?1 AND start < ?2 ORDER BY start")?;
    let windows: Vec<String> = q.query_map(params![from, to], |r| r.get(0))?.collect::<Result<_, _>>()?;
    out.switches = windows.windows(2).filter(|w| w[0] != w[1]).count();
    for row in &focus {
        match out.sessions.last_mut() {
            Some((_, end)) if row.start - *end <= PAUSE => *end = (*end).max(row.end),
            _ => out.sessions.push((row.start, row.end)),
        }
    }

    // Por horas (o días): el tiempo y la aplicación que más.
    if slice > 0 {
        let first = from - (from - origin).rem_euclid(slice);
        let mut start = first;
        while start < to {
            let end = start + slice;
            let mut apps: Vec<(String, i64)> = Vec::new();
            let mut places: Vec<(String, i64)> = Vec::new();
            let mut ms = 0;
            for row in focus.iter().filter(|r| r.end > start && r.start < end) {
                let part = row.end.min(end) - row.start.max(start);
                add(&mut apps, &row.app, part);
                add(&mut places, &format!("{}\n{}", row.app, row.place()), part);
                ms += part;
            }
            by_time(&mut apps);
            by_time(&mut places);
            let app = apps.into_iter().next().map(|(a, _)| a).unwrap_or_default();
            let place = places.into_iter().find_map(|(k, _)| k.strip_prefix(&format!("{app}\n")).map(str::to_string)).unwrap_or_default();
            out.slices.push(Slice { start: start.max(from), end: end.min(to), ms, app, place });
            start = end;
        }
    }

    // Por proyecto.
    let mut cache = HashMap::new();
    let mut projects: Vec<Project> = Vec::new();
    for row in focus.iter().filter(|r| !r.folder.is_empty()) {
        let Some(path) = project(&row.folder, home, &mut cache) else {
            continue;
        };
        let ms = row.end - row.start;
        let p = match projects.iter().position(|p| p.path == path) {
            Some(i) => &mut projects[i],
            None => {
                projects.push(Project { path, ..Project::default() });
                projects.last_mut().expect("recién puesto")
            }
        };
        p.ms += ms;
        add(&mut p.apps, &row.app, ms);
        if !row.command.is_empty() {
            add(&mut p.commands, &row.command, ms);
        }
    }

    // Los agentes, sesión a sesión.
    let mut q = db.prepare(
        "SELECT agent, label, name, folder, model, phase, MAX(start, ?1), MIN(end, ?2) FROM agents
         WHERE end > ?1 AND start < ?2 ORDER BY start",
    )?;
    let rows_a = q.query_map(params![from, to], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, i64>(6)?,
            r.get::<_, i64>(7)?,
        ))
    })?;
    for row in rows_a {
        let (id, label, name, folder, model, phase, start, end) = row?;
        // Un mismo agente reiniciado es otra sesión con el mismo nombre: se junta.
        let a = match out.agents.iter().position(|a| a.id == id || (a.label == label && a.name == name && a.folder == folder && !name.is_empty())) {
            Some(i) => &mut out.agents[i],
            None => {
                out.agents.push(AgentDetail { id: id.clone(), first: start, ..AgentDetail::default() });
                out.agents.last_mut().expect("recién puesto")
            }
        };
        a.first = a.first.min(start);
        (a.label, a.name, a.folder) = (label, name, folder);
        if !model.is_empty() {
            a.model = model;
        }
        a.last = a.last.max(end);
        if phase == "needs" {
            a.waiting += end - start;
            a.waits += 1;
        } else {
            a.working += end - start;
        }
    }
    out.agents.retain(|a| a.working + a.waiting >= 30_000);
    out.agents.sort_by_key(|a| std::cmp::Reverse(a.working));
    for agent in &out.agents {
        if let Some(path) = project(&agent.folder, home, &mut cache)
            && let Some(p) = projects.iter_mut().find(|p| p.path == path)
        {
            p.agents += agent.working;
        } else if let Some(path) = project(&agent.folder, home, &mut cache) {
            projects.push(Project { path, agents: agent.working, ..Project::default() });
        }
    }
    projects.sort_by_key(|p| std::cmp::Reverse(p.ms + p.agents / 2));
    for p in &mut projects {
        by_time(&mut p.apps);
        by_time(&mut p.commands);
    }
    out.projects = projects;

    // Por nivel.
    let mut q = db.prepare("SELECT workspace, SUM(MIN(end, ?2) - MAX(start, ?1)) AS ms FROM focus WHERE end > ?1 AND start < ?2 AND workspace != '' GROUP BY workspace ORDER BY ms DESC")?;
    out.levels = q.query_map(params![from, to], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;

    // Micro, cámara y pantalla, vez a vez.
    let mut q = db.prepare("SELECT kind, MAX(start, ?1), MIN(end, ?2) FROM devices WHERE end > ?1 AND start < ?2 ORDER BY start")?;
    let spans: Vec<(String, i64, i64)> = q.query_map(params![from, to], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<Result<_, _>>()?;
    for (kind, start, end) in spans {
        let mut apps: Vec<(String, i64)> = Vec::new();
        for row in focus.iter().filter(|r| r.end > start && r.start < end) {
            add(&mut apps, &format!("{}\n{}", row.app, row.site), row.end.min(end) - row.start.max(start));
        }
        by_time(&mut apps);
        let top = apps.into_iter().next().map(|(k, _)| k).unwrap_or_default();
        let (app, site) = top.split_once('\n').unwrap_or((&top, ""));
        out.calls.push(Call { kind, start, end, app: app.to_string(), site: site.to_string() });
    }

    // Ventanas abiertas y cerradas.
    let mut q = db.prepare("SELECT app, COUNT(*) AS n FROM windows WHERE opened >= ?1 AND opened < ?2 GROUP BY app ORDER BY n DESC")?;
    out.opened = q.query_map(params![from, to], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
    out.closed = db.query_row("SELECT COUNT(*) FROM windows WHERE closed >= ?1 AND closed < ?2", params![from, to], |r| r.get(0))?;

    // Lo que se le pidió a Veyl.
    let mut q = db.prepare("SELECT time, source, said, ok, answer FROM orders WHERE time >= ?1 AND time < ?2 ORDER BY time")?;
    out.orders = q
        .query_map(params![from, to], |r| {
            let answer: String = r.get(4)?;
            Ok(Asked { time: r.get(0)?, source: r.get(1)?, said: r.get(2)?, ok: r.get(3)?, answer: answer.lines().next().unwrap_or("").to_string() })
        })?
        .collect::<Result<_, _>>()?;
    Ok(out)
}

/// Un trozo de la línea del tiempo: lo que más tuvo el foco en un rato, o
/// por qué no hubo nada.
#[derive(Debug, Default, PartialEq, Serialize)]
pub struct Block {
    pub start: i64,
    pub end: i64,
    /// La aplicación; vacía, no había nadie.
    pub app: String,
    /// El sitio o la carpeta.
    pub place: String,
    /// Lo que más hubo dentro: la orden (`command`) o el título.
    pub detail: String,
    pub command: bool,
    /// La página de ese título, en un navegador.
    pub url: String,
    /// Sin aplicación: inactivo, bloqueado, suspendido, o nada si no hay
    /// diario de ese rato.
    pub away: String,
}

/// Lo que pasó entre `from` y `to` en ratos de `slot` ms, contados desde
/// `origin` (la medianoche, para que caigan en horas redondas), con los ratos
/// seguidos iguales juntos.
pub fn timeline(db: &Connection, from: i64, to: i64, slot: i64, origin: i64) -> rusqlite::Result<Vec<Block>> {
    if from >= to || slot <= 0 {
        return Ok(Vec::new());
    }
    let focus = rows(db, from, to)?;
    let mut q = db.prepare("SELECT MAX(start, ?1), MIN(end, ?2), reason FROM away WHERE end > ?1 AND start < ?2 ORDER BY start")?;
    let away: Vec<(i64, i64, String)> = q.query_map(params![from, to], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<Result<_, _>>()?;

    let first = from - (from - origin).rem_euclid(slot);
    let count = ((to - first) + slot - 1) / slot;
    let mut labels: Vec<(String, String, String)> = Vec::with_capacity(count as usize);
    for i in 0..count {
        let (start, end) = (first + i * slot, first + (i + 1) * slot);
        // Gana la aplicación con más tiempo y, dentro de ella, el sitio o la carpeta.
        let mut apps: Vec<(String, i64)> = Vec::new();
        let mut keys: Vec<(String, i64)> = Vec::new();
        let mut seen = 0;
        for row in focus.iter().filter(|r| r.end > start && r.start < end) {
            let ms = row.end.min(end) - row.start.max(start);
            add(&mut apps, &row.app, ms);
            add(&mut keys, &format!("{}\n{}", row.app, row.place()), ms);
            seen += ms;
        }
        by_time(&mut apps);
        keys.retain(|(key, _)| apps.first().is_some_and(|(app, _)| key.split('\n').next() == Some(app.as_str())));
        let mut reasons: Vec<(String, i64)> = Vec::new();
        let mut gone = 0;
        for (s, e, reason) in away.iter().filter(|(s, e, _)| *e > start && *s < end) {
            let ms = (*e).min(end) - (*s).max(start);
            add(&mut reasons, reason, ms);
            gone += ms;
        }
        by_time(&mut keys);
        by_time(&mut reasons);
        let length = end.min(to) - start.max(from);
        let label = if seen * 4 >= length && !keys.is_empty() {
            let (app, place) = keys[0].0.split_once('\n').unwrap_or((&keys[0].0, ""));
            (app.to_string(), place.to_string(), String::new())
        } else if gone * 4 >= length && !reasons.is_empty() {
            (String::new(), String::new(), reasons[0].0.clone())
        } else {
            Default::default()
        };
        labels.push(label);
    }

    let mut blocks: Vec<Block> = Vec::new();
    for (i, (app, place, why)) in labels.into_iter().enumerate() {
        let (start, end) = ((first + i as i64 * slot).max(from), (first + (i as i64 + 1) * slot).min(to));
        match blocks.last_mut() {
            Some(last) if last.app == app && last.place == place && last.away == why => last.end = end,
            _ => blocks.push(Block { start, end, app, place, away: why, ..Block::default() }),
        }
    }
    // Lo de antes de lo primero y después de lo último no cuenta.
    while blocks.first().is_some_and(|b| b.app.is_empty() && b.away.is_empty()) {
        blocks.remove(0);
    }
    while blocks.last().is_some_and(|b| b.app.is_empty() && b.away.is_empty()) {
        blocks.pop();
    }
    for block in &mut blocks {
        let (mut details, mut span, mut urls) = (Vec::new(), None::<(i64, i64)>, HashMap::new());
        if block.app.is_empty() {
            for (s, e, _) in away.iter().filter(|(s, e, reason)| *e > block.start && *s < block.end && *reason == block.away) {
                span = Some(span.map_or((*s, *e), |(a, b)| (a.min(*s), b.max(*e))));
            }
        } else {
            for row in focus.iter().filter(|r| r.end > block.start && r.start < block.end && r.app == block.app && r.place() == block.place) {
                let ms = row.end.min(block.end) - row.start.max(block.start);
                let what = if row.command.is_empty() { format!("t{}", row.title) } else { format!("c{}", row.command) };
                if what.len() > 1 {
                    if !row.url.is_empty() {
                        urls.entry(what.clone()).or_insert_with(|| row.url.clone());
                    }
                    add(&mut details, &what, ms);
                }
                span = Some(span.map_or((row.start, row.end), |(a, b)| (a.min(row.start), b.max(row.end))));
            }
        }
        by_time(&mut details);
        if let Some((what, _)) = details.into_iter().next() {
            block.command = what.starts_with('c');
            block.url = urls.remove(&what).unwrap_or_default();
            block.detail = what[1..].to_string();
        }
        // Las horas de lo que de verdad hubo, no las del rato entero.
        if let Some((s, e)) = span {
            (block.start, block.end) = (s.max(block.start), e.min(block.end));
        }
    }
    Ok(blocks)
}

/// Algo del diario que casa con lo que se busca.
#[derive(Debug, Default, PartialEq, Serialize)]
pub struct Hit {
    pub start: i64,
    pub end: i64,
    /// focus (lo mirado), media (lo que sonó) o file (un archivo abierto).
    pub kind: &'static str,
    pub app: String,
    pub title: String,
    /// El sitio, la carpeta, el artista o la ruta.
    pub place: String,
    /// La página, si era un navegador.
    pub url: String,
    /// Cuántas de las palabras buscadas tiene.
    pub words: usize,
}

/// Lo mismo seguido, con menos de un cuarto de hora en medio, es una vez:
/// se junta con lo que ya había (si es de lo último que se juntó). `true`, si
/// es una vez nueva.
fn gather(hits: &mut Vec<Hit>, hit: Hit) -> bool {
    if let Some(last) = hits.iter_mut().rev().take(20).find(|h| h.kind == hit.kind && h.app == hit.app && h.title == hit.title && h.place == hit.place)
        && last.start - hit.end <= 15 * 60 * 1000
    {
        last.start = last.start.min(hit.start);
        return false;
    }
    hits.push(hit);
    true
}

/// Cuántas de `words` (ya sin tildes) salen en `hay`.
pub fn matches(hay: &str, words: &[String]) -> usize {
    words.iter().filter(|w| hay.contains(w.as_str())).count()
}

/// Dónde salen `words` (ya sin tildes), lo último primero. Si nada las tiene
/// todas y son tres o más, vale con que falte una. `label` da el nombre de
/// una aplicación por su clase.
pub fn find(db: &Connection, words: &[String], label: impl Fn(&str) -> String, limit: usize) -> rusqlite::Result<Vec<Hit>> {
    if words.is_empty() {
        return Ok(Vec::new());
    }
    let need = if words.len() >= 3 { words.len() - 1 } else { words.len() };
    // De cada clase basta con lo más reciente: con esto ya sobra.
    let enough = limit * 4;
    let mut hits: Vec<Hit> = Vec::new();

    let mut q = db.prepare("SELECT start, end, app, title, folder, command, url, site FROM focus ORDER BY start DESC")?;
    let rows = q.query_map([], Row::read)?;
    let mut labels: HashMap<String, String> = HashMap::new();
    let mut full = 0;
    for row in rows {
        let row = row?;
        let name = labels.entry(row.app.clone()).or_insert_with(|| label(&row.app)).clone();
        let n = matches(&row.hay(&name), words);
        if n >= need {
            let title = if row.title.is_empty() { row.command.clone() } else { row.title.clone() };
            let hit = Hit {
                start: row.start,
                end: row.end,
                kind: "focus",
                app: row.app.clone(),
                place: row.place().to_string(),
                url: row.url.clone(),
                title,
                words: n,
            };
            if gather(&mut hits, hit) && n == words.len() {
                full += 1;
                if full >= enough {
                    break;
                }
            }
        }
    }
    let mut q = db.prepare("SELECT start, end, player, title, artist FROM media ORDER BY start DESC")?;
    let rows = q.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?)))?;
    let mut full = 0;
    for row in rows {
        let (start, end, player, title, artist) = row?;
        let n = matches(&fold(&format!("{player} {title} {artist}")), words);
        if n >= need
            && gather(&mut hits, Hit { start, end, kind: "media", app: player, title, place: artist, url: String::new(), words: n })
            && n == words.len()
        {
            full += 1;
            if full >= enough {
                break;
            }
        }
    }
    let mut q = db.prepare("SELECT time, path, app FROM files ORDER BY time DESC")?;
    let rows = q.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
    let mut full = 0;
    for row in rows {
        let (time, path, app) = row?;
        let n = matches(&fold(&path), words);
        if n >= need {
            let name = Path::new(&path).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
            if gather(&mut hits, Hit { start: time, end: time, kind: "file", app, title: name, place: path, url: String::new(), words: n }) && n == words.len()
            {
                full += 1;
                if full >= enough {
                    break;
                }
            }
        }
    }
    // Primero lo que tiene todas; luego, lo más reciente.
    let best = hits.iter().map(|h| h.words).max().unwrap_or(0);
    hits.retain(|h| h.words == best);
    hits.sort_by_key(|h| std::cmp::Reverse(h.end));
    hits.truncate(limit);
    Ok(hits)
}

/// Cuánto hubo algo delante entre `from` y `to` (ms).
pub fn screen(db: &Connection, from: i64, to: i64) -> rusqlite::Result<i64> {
    db.query_row("SELECT COALESCE(SUM(MIN(end, ?2) - MAX(start, ?1)), 0) FROM focus WHERE end > ?1 AND start < ?2", params![from, to], |r| r.get(0))
}

/// Una cosa distinta de un tramo, para el índice: una página o un título de
/// una aplicación, una orden en una carpeta, una pista o un archivo.
#[derive(Debug, Default, PartialEq, Serialize)]
pub struct Entry {
    /// focus, media o file.
    pub kind: &'static str,
    pub app: String,
    pub title: String,
    /// El sitio, la carpeta, el artista o la ruta.
    pub place: String,
    pub url: String,
    pub first: i64,
    pub last: i64,
    pub ms: i64,
    /// Cuántas veces se volvió a ello (con más de un cuarto de hora en medio).
    pub times: usize,
}

/// Todo lo distinto que hubo entre `from` y `to`, para que un modelo busque
/// por lo que significa y no solo por las palabras: lo de más rato primero,
/// `limit` como mucho (lo de menos de `GLANCE` × 5 no entra).
pub fn index(db: &Connection, from: i64, to: i64, limit: usize) -> rusqlite::Result<Vec<Entry>> {
    let mut out: Vec<Entry> = Vec::new();
    let mut at: HashMap<String, usize> = HashMap::new();
    let mut put = |kind: &'static str, app: &str, title: &str, place: &str, url: &str, start: i64, end: i64, out: &mut Vec<Entry>| {
        let key = format!("{kind}\n{app}\n{title}\n{place}");
        match at.get(&key) {
            Some(&i) => {
                let e = &mut out[i];
                if start - e.last > 15 * 60 * 1000 {
                    e.times += 1;
                }
                (e.first, e.last, e.ms) = (e.first.min(start), e.last.max(end), e.ms + (end - start));
                if e.url.is_empty() {
                    e.url = url.to_string();
                }
            }
            None => {
                at.insert(key, out.len());
                out.push(Entry {
                    kind,
                    app: app.to_string(),
                    title: title.to_string(),
                    place: place.to_string(),
                    url: url.to_string(),
                    first: start,
                    last: end,
                    ms: end - start,
                    times: 1,
                });
            }
        }
    };
    for row in rows(db, from, to)? {
        let title = if row.command.is_empty() { row.title.as_str() } else { row.command.as_str() };
        put("focus", &row.app, title, row.place(), &row.url, row.start, row.end, &mut out);
    }
    let mut q = db.prepare("SELECT player, title, artist, MAX(start, ?1), MIN(end, ?2) FROM media WHERE end > ?1 AND start < ?2 ORDER BY start")?;
    let media: Vec<(String, String, String, i64, i64)> =
        q.query_map(params![from, to], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?.collect::<Result<_, _>>()?;
    for (player, title, artist, start, end) in media {
        put("media", &player, &title, &artist, "", start, end, &mut out);
    }
    let mut q = db.prepare("SELECT time, path, app FROM files WHERE time >= ?1 AND time < ?2 ORDER BY time")?;
    let files: Vec<(i64, String, String)> = q.query_map(params![from, to], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<Result<_, _>>()?;
    for (time, path, app) in files {
        let name = Path::new(&path).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
        put("file", &app, &name, &path, "", time, time, &mut out);
    }
    out.retain(|e| e.kind == "file" || e.ms >= GLANCE * 5);
    out.sort_by_key(|e| std::cmp::Reverse(e.ms));
    out.truncate(limit);
    out.sort_by_key(|e| e.first);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(window: &str, app: &str, title: &str, folder: &str) -> Option<Seen> {
        Some(Seen { window: window.into(), app: app.into(), title: title.into(), folder: folder.into(), ..Seen::default() })
    }

    #[test]
    fn short_focus_is_not_recorded_by_a_heartbeat() {
        let mut j = Journal::memory();
        j.focus(0, seen("1", "app", "title", ""));
        j.beat(500);
        j.focus(600, None);
        assert_eq!(recall(j.db(), 0, 1000).unwrap().total, 0);
    }

    #[test]
    fn idle_rewinds_already_closed_spans() {
        let mut j = Journal::memory();
        j.focus(0, seen("1", "app", "first", ""));
        j.focus(90_000, seen("1", "app", "second", ""));
        j.beat(150_000);
        j.away(60_000, Some("inactivo"));
        let r = recall(j.db(), 0, 200_000).unwrap();
        assert_eq!(r.total, 60_000);
        assert_eq!(r.apps[0].titles, vec![("first".into(), 60_000)]);
    }

    #[test]
    fn settled_focus_updates_all_metadata() {
        let mut j = Journal::memory();
        j.focus(0, seen("1", "app", "title", ""));
        j.beat(1000);
        let mut next = seen("1", "app", "updated", "").unwrap();
        next.workspace = "2".into();
        j.focus(1500, Some(next));
        j.finish(4000);
        let workspace: String = j.db().query_row("SELECT workspace FROM focus", [], |r| r.get(0)).unwrap();
        assert_eq!(workspace, "2");
    }

    #[test]
    fn pause_closes_live_records() {
        let mut j = Journal::memory();
        j.opened(0, "1", "app", "title", "1");
        j.focus(0, seen("1", "app", "title", ""));
        j.pause(2000);
        j.beat(5000);
        assert_eq!(recall(j.db(), 0, 6000).unwrap().total, 2000);
        let closed: i64 = j.db().query_row("SELECT closed FROM windows", [], |r| r.get(0)).unwrap();
        assert_eq!(closed, 2000);
        assert!(j.windows.is_empty());
        assert_eq!(recall(j.db(), 3000, 1000).unwrap().total, 0);
    }

    fn row(j: &Journal, sql: &str) -> Vec<String> {
        let mut q = j.db().prepare(sql).unwrap();
        let n = q.column_count();
        let text = |v: rusqlite::types::ValueRef| match v {
            rusqlite::types::ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned(),
            rusqlite::types::ValueRef::Integer(n) => n.to_string(),
            other => format!("{other:?}"),
        };
        q.query_map([], |r| Ok((0..n).map(|i| r.get_ref(i).map(text).unwrap_or_default()).collect::<Vec<_>>().join("|"))).unwrap().flatten().collect()
    }

    #[test]
    fn old_diaries_are_migrated() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(SCHEMA).unwrap();
        db.execute_batch(
            "INSERT INTO focus (start, end, window, app, title, folder) VALUES (0, 5000, '0x1', 'firefox', 'Página', '/home/u/dev');
             INSERT INTO focus (start, end, window, app, title, folder) VALUES (5000, 9000, '0x2', 'kitty', 'fish', '/home/u/dev');",
        )
        .unwrap();
        migrate(&db).unwrap();
        migrate(&db).unwrap();
        let j = Journal::with(db);
        assert_eq!(row(&j, "SELECT app, folder, command, url FROM focus ORDER BY start"), vec!["firefox|||", "kitty|/home/u/dev||"]);
        let version: i64 = j.db().query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(version, MIGRATIONS.len() as i64);
    }

    fn playing(bus: &str, title: &str) -> Playing {
        Playing { bus: bus.into(), player: "Spotify".into(), title: title.into(), artist: "Band".into(), ..Playing::default() }
    }

    #[test]
    fn media_devices_and_sleep() {
        let mut j = Journal::memory();
        j.focus(0, seen("0x1", "firefox", "Llamada — Mozilla Firefox", ""));
        j.media(0, &[playing("a", "Una")]);
        // Pasar de canción no cuenta.
        j.media(60_000, &[playing("a", "Dos")]);
        j.media(62_000, &[playing("a", "Tres")]);
        j.device(10_000, "micro", true);
        j.device(10_500, "micro", true);
        j.beat(100_000);
        // La máquina duerme de 100 s a 1000 s: todo lo abierto acaba al dormirse.
        j.slept(100_000, 1_000_000);
        j.media(1_000_000, &[playing("a", "Tres")]);
        j.media(1_030_000, &[]);
        let r = recall(j.db(), 0, 2_000_000).unwrap();
        assert_eq!(r.total, 100_000);
        let tracks: Vec<(&str, i64)> = r.media.iter().map(|m| (m.title.as_str(), m.ms)).collect();
        assert_eq!(tracks, vec![("Tres", 38_000 + 30_000), ("Una", 60_000)]);
        assert_eq!(r.devices.len(), 1);
        assert_eq!((r.devices[0].ms, r.devices[0].times, r.devices[0].app.as_str()), (90_000, 1, "firefox"));
        assert_eq!(row(&j, "SELECT reason, start, end FROM away"), vec!["suspendido|100000|1000000"]);
    }

    #[test]
    fn pages_commands_and_files() {
        let mut j = Journal::memory();
        j.focus(0, seen("0x1", "firefox", "Fallout 3 - YouTube — Mozilla Firefox", ""));
        let mut term = seen("0x2", "kitty", "nixos: veyl", "/home/u/dev/veyl").unwrap();
        term.command = "cargo test".into();
        j.focus(120_000, Some(term.clone()));
        term.command = "nvim main.rs".into();
        j.focus(200_000, Some(term));
        j.finish(260_000);
        j.pages(0, |app, title| (app == "firefox" && title.starts_with("Fallout")).then(|| "https://www.youtube.com/watch?v=1".to_string()));
        j.files(
            &[
                Used { path: "/home/u/informe.pdf".into(), app: "Papers".into(), time: 50_000 },
                Used { path: "/home/u/viejo.pdf".into(), app: "Papers".into(), time: -1 },
            ],
            0,
        );
        // Lo ya apuntado no se repite.
        assert_eq!(j.files(&[Used { path: "/home/u/informe.pdf".into(), app: "Papers".into(), time: 50_000 }], 0), 0);
        let r = recall(j.db(), 0, 300_000).unwrap();
        assert_eq!(r.apps[0].app, "kitty");
        assert_eq!(
            r.apps[0].folders,
            vec![Part { name: "/home/u/dev/veyl".into(), ms: 140_000, within: vec![("cargo".into(), 80_000), ("nvim".into(), 60_000)] }]
        );
        assert_eq!(r.apps[1].sites[0].name, "youtube.com");
        assert_eq!(r.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), vec!["/home/u/informe.pdf"]);

        let hits = find(j.db(), &["fallout".into()], |a| a.to_string(), 5).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!((hits[0].kind, hits[0].place.as_str(), hits[0].start, hits[0].end), ("focus", "youtube.com", 0, 120_000));
        assert_eq!(find(j.db(), &["informe".into()], |a| a.to_string(), 5).unwrap()[0].kind, "file");
        assert!(find(j.db(), &["nada".into()], |a| a.to_string(), 5).unwrap().is_empty());

        // El índice: cada cosa distinta una vez, por orden.
        let found = index(j.db(), 0, 300_000, 50).unwrap();
        let shown: Vec<(&str, &str, &str)> = found.iter().map(|e| (e.kind, e.title.as_str(), e.place.as_str())).collect();
        assert_eq!(
            shown,
            vec![
                ("focus", "Fallout 3 - YouTube — Mozilla Firefox", "youtube.com"),
                ("file", "informe.pdf", "/home/u/informe.pdf"),
                ("focus", "cargo test", "/home/u/dev/veyl"),
                ("focus", "nvim main.rs", "/home/u/dev/veyl"),
            ]
        );
        assert_eq!(found[0].url, "https://www.youtube.com/watch?v=1");
        assert_eq!(screen(j.db(), 0, 300_000).unwrap(), 260_000);
    }

    #[test]
    fn timeline_blocks() {
        let mut j = Journal::memory();
        let min = 60_000;
        let mut term = seen("0x2", "kitty", "fish", "/home/u/dev/veyl").unwrap();
        term.command = "claude".into();
        j.focus(2 * min, Some(term.clone()));
        // Un vistazo corto al navegador no rompe el rato.
        j.focus(14 * min, seen("0x1", "firefox", "Docs — Mozilla Firefox", ""));
        j.focus(15 * min, Some(term));
        j.focus(31 * min, seen("0x1", "firefox", "Docs — Mozilla Firefox", ""));
        j.focus(44 * min, None);
        j.away(44 * min, Some("bloqueado"));
        j.away(70 * min, None);
        j.finish(70 * min);
        let blocks = timeline(j.db(), 0, 80 * min, 15 * min, 0).unwrap();
        let shown: Vec<(i64, i64, &str, &str, &str)> =
            blocks.iter().map(|b| (b.start / min, b.end / min, b.app.as_str(), b.detail.as_str(), b.away.as_str())).collect();
        assert_eq!(shown, vec![(2, 30, "kitty", "claude", ""), (31, 44, "firefox", "Docs — Mozilla Firefox", ""), (45, 70, "", "", "bloqueado"),]);
        assert!(blocks[0].command && !blocks[1].command);
    }

    #[test]
    fn the_app_wins_before_its_sites() {
        let mut j = Journal::memory();
        let min = 60_000;
        j.focus(0, seen("0x2", "kitty", "fish", "/home/u"));
        j.focus(6 * min, seen("0x1", "firefox", "A — Mozilla Firefox", ""));
        j.focus(11 * min, seen("0x1", "firefox", "B — Mozilla Firefox", ""));
        j.finish(15 * min);
        j.pages(0, |_, title| Some(format!("https://{}.example/", title[..1].to_lowercase())));
        let blocks = timeline(j.db(), 0, 15 * min, 15 * min, 0).unwrap();
        assert_eq!((blocks[0].app.as_str(), blocks[0].place.as_str()), ("firefox", "a.example"));
    }

    #[test]
    fn report_has_everything() {
        let dir = std::env::temp_dir().join(format!("veyld-report-{}", std::process::id()));
        let repo = dir.join("dev/veyl");
        std::fs::create_dir_all(repo.join("veyld/src")).unwrap();
        std::fs::write(repo.join("README.md"), "").unwrap();
        let home = dir.to_string_lossy().into_owned();
        let sub = repo.join("veyld/src").to_string_lossy().into_owned();
        let top = repo.to_string_lossy().into_owned();

        let mut j = Journal::memory();
        let min = 60_000;
        let mut term = seen("0x2", "kitty", "fish", &sub).unwrap();
        term.command = "cargo test".into();
        j.focus(0, Some(term));
        j.focus(20 * min, seen("0x1", "firefox", "Docs — Mozilla Firefox", ""));
        j.focus(30 * min, None);
        // Una pausa larga parte los ratos.
        j.focus(60 * min, seen("0x1", "firefox", "Docs — Mozilla Firefox", ""));
        j.finish(70 * min);
        let agent = |phase: &str, pid: &str| Agent {
            id: format!("claude:{pid}"),
            label: "Claude".into(),
            name: "veyld".into(),
            cwd: top.clone(),
            phase: phase.into(),
            ..Agent::default()
        };
        j.agents(0, &[agent("working", "1")]);
        j.agents(5 * min, &[agent("needs", "1")]);
        j.agents(6 * min, &[agent("working", "1")]);
        j.agents(10 * min, &[]);
        // El mismo agente, reiniciado.
        j.agents(40 * min, &[agent("working", "2")]);
        j.agents(45 * min, &[]);
        j.opened(0, "0x9", "kitty", "fish", "1");
        j.closed(min, "0x9");
        let id = j.order(min, "voice", "sube el volumen", Some("volume.up"), "{}", "grammar", true, "Volumen al 50 %\nmás", "").unwrap();
        let asked = j.asked(10, None);
        assert_eq!((asked[0]["id"].as_i64(), asked[0]["say"].as_str(), asked[0]["more"].as_str()), (Some(id), Some("Volumen al 50 %"), Some("más")));
        assert!(asked[0]["card"].is_null());

        let r = report(j.db(), 0, 80 * min, 30 * min, 0, &home).unwrap();
        assert_eq!(r.sessions, vec![(0, 30 * min), (60 * min, 70 * min)]);
        assert_eq!(r.switches, 1);
        assert_eq!(r.slices.iter().map(|s| (s.ms / min, s.app.as_str())).collect::<Vec<_>>(), [(30, "kitty"), (0, ""), (10, "firefox")]);
        assert_eq!(r.slices[0].place, sub);
        // La subcarpeta cuenta para el proyecto de arriba (con README), y los agentes también.
        assert_eq!(r.projects.len(), 1);
        assert_eq!((r.projects[0].path.as_str(), r.projects[0].ms, r.projects[0].agents), (top.as_str(), 20 * min, 14 * min));
        assert_eq!(r.agents.len(), 1);
        assert_eq!((r.agents[0].working, r.agents[0].waiting, r.agents[0].waits), (14 * min, min, 1));
        assert_eq!((r.opened.clone(), r.closed), (vec![("kitty".to_string(), 1)], 1));
        assert_eq!(r.orders[0].answer, "Volumen al 50 %");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn spans() {
        let dir = std::env::temp_dir().join(format!("veyld-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("journal.db");
        let _ = std::fs::remove_file(&path);
        let mut j = Journal::open(&path).unwrap();
        use std::os::unix::fs::PermissionsExt;
        for file in [path.clone(), path.with_file_name("journal.db-wal"), path.with_file_name("journal.db-shm")] {
            assert_eq!(std::fs::metadata(file).unwrap().permissions().mode() & 0o777, 0o600);
        }
        j.focus(0, seen("0x1", "firefox", "Cargando…", ""));
        // El título bueno llega enseguida: sustituye al primero.
        j.focus(500, seen("0x1", "firefox", "Rust by Example", ""));
        j.focus(60_000, seen("0x2", "kitty", "fish", "/home/u/dev/veyl"));
        // Pasar de largo por una ventana no cuenta.
        j.focus(120_000, seen("0x3", "zed", "main.rs", ""));
        j.focus(120_400, seen("0x2", "kitty", "fish", "/home/u/dev/veyl"));
        j.beat(150_000);
        j.away(180_000, Some("inactivo"));
        j.away(300_000, None);
        j.focus(300_000, seen("0x2", "kitty", "fish", "/home/u/dev/veyl"));
        j.finish(330_000);

        let r = recall(j.db(), 0, 400_000).unwrap();
        assert_eq!(r.total, 60_000 + 60_000 + 59_600 + 30_000);
        assert_eq!(r.apps[0].app, "kitty");
        assert_eq!((r.apps[0].folders[0].name.as_str(), r.apps[0].folders[0].ms), ("/home/u/dev/veyl", 149_600));
        assert_eq!(r.apps[1].titles, vec![("Rust by Example".to_string(), 60_000)]);
        assert_eq!(r.apps.len(), 2);
        // Medio tramo de Firefox cae dentro.
        assert_eq!(recall(j.db(), 30_000, 60_000).unwrap().total, 30_000);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
