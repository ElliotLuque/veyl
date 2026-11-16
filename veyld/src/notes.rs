//! Las notificaciones: las que llegan por D-Bus (`bus.rs`) se guardan aquí,
//! en SQLite ($XDG_STATE_HOME/veyl/notifications.db), hasta que las borras
//! o pasan los días que se guardan (`notificationDays` en settings.json).
//! Una nota está viva (`live`) mientras su programa puede recibir lo que
//! hagas con ella, y sale como aviso (`popup`) hasta que caduca: entonces
//! se queda en el cajón. Veyl solo las pinta; lo que pasa con ellas se
//! decide aquí.

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

/// Lo que dura un aviso que no pide plazo.
pub const PATIENCE: Duration = Duration::from_millis(6000);

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS notes (
    id INTEGER PRIMARY KEY,              -- el de D-Bus
    at INTEGER NOT NULL,                 -- ms desde 1970
    data TEXT NOT NULL                   -- la nota, en JSON
);
CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value INTEGER NOT NULL
);
";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Action {
    pub id: String,
    pub text: String,
}

/// Una notificación, tal como la recibe Veyl.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Note {
    pub id: u32,
    pub app: String,
    /// El icono que pide (`app_icon`): un nombre del tema, una ruta o una URL.
    pub icon: String,
    /// Su `desktop-entry`, para buscar su nombre y su icono.
    pub entry: String,
    /// Su imagen: una ruta o una URL (la que viene en datos se guarda en un PNG).
    pub image: String,
    pub summary: String,
    pub body: String,
    /// Sin la de por defecto ni la de responder: esas van aparte.
    pub actions: Vec<Action>,
    /// Pulsarla hace algo (tiene acción por defecto).
    pub primary: bool,
    /// El identificador de cada acción es el nombre de su icono.
    pub action_icons: bool,
    /// low, normal o critical
    pub urgency: String,
    pub category: String,
    /// Avance de 0 a 100, o -1.
    pub progress: i32,
    /// Se puede responder desde ella.
    pub reply: bool,
    pub placeholder: String,
    pub at: i64,
    /// Su programa aún la escucha.
    pub live: bool,
    /// Sale como aviso.
    pub popup: bool,
    /// Se queda viva aunque se pulse una acción.
    pub resident: bool,
    /// No se guarda: cuando caduca, se va.
    pub transient: bool,
    /// El plazo que pide, en ms: -1, el de Veyl; 0, sin plazo.
    pub timeout: i32,
}

impl Note {
    pub fn critical(&self) -> bool {
        self.urgency == "critical"
    }

    /// Lo que dura como aviso; sin plazo, nada.
    fn span(&self) -> Option<Duration> {
        if self.critical() || self.timeout == 0 {
            None
        } else if self.timeout > 0 {
            Some(Duration::from_millis(self.timeout as u64))
        } else {
            Some(PATIENCE)
        }
    }
}

/// Por qué se cierra una, como lo cuenta D-Bus.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reason {
    Expired = 1,
    Dismissed = 2,
    Closed = 3,
}

/// Lo que hay que contar después de un cambio.
#[derive(Clone, Debug, PartialEq)]
pub enum Out {
    /// A Veyl: es nueva o ha cambiado.
    Show(Box<Note>),
    /// A Veyl: ya no está.
    Gone(u32),
    /// A su programa: se ha cerrado.
    Closed(u32, Reason),
}

pub struct Notes {
    db: Option<Connection>,
    /// De la más reciente a la más antigua.
    list: Vec<Note>,
    due: HashMap<u32, Instant>,
    held: bool,
    next: u32,
}

impl Notes {
    pub fn open(path: &Path) -> Result<Notes, Box<dyn std::error::Error>> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        {
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            let file = std::fs::OpenOptions::new().write(true).create(true).truncate(false).mode(0o600).open(path)?;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
        let db = Connection::open(path)?;
        db.busy_timeout(Duration::from_secs(2))?;
        db.pragma_update(None, "journal_mode", "WAL")?;
        db.pragma_update(None, "synchronous", "NORMAL")?;
        Ok(Notes::with(Some(db)))
    }

    /// Sin guardar nada: si no se puede abrir la base, o para las pruebas.
    pub fn memory() -> Notes {
        Notes::with(None)
    }

    fn with(db: Option<Connection>) -> Notes {
        let mut notes = Notes { db, list: Vec::new(), due: HashMap::new(), held: false, next: 1 };
        if let Some(db) = &notes.db {
            if let Err(e) = db.execute_batch(SCHEMA) {
                eprintln!("veyld: notificaciones sin guardar: {e}");
                notes.db = None;
                return notes;
            }
            // Las guardadas ya no las escucha nadie.
            if let Ok(mut query) = db.prepare("SELECT data FROM notes ORDER BY at DESC, id DESC")
                && let Ok(rows) = query.query_map([], |row| row.get::<_, String>(0))
            {
                notes.list =
                    rows.flatten().filter_map(|data| serde_json::from_str::<Note>(&data).ok()).map(|note| Note { live: false, popup: false, ..note }).collect();
            }
            let next: Option<i64> = db.query_row("SELECT value FROM meta WHERE key = 'next'", [], |row| row.get(0)).ok();
            notes.next = next.map(|n| n as u32).unwrap_or(1);
        }
        let highest = notes.list.iter().map(|n| n.id).max().unwrap_or(0);
        notes.next = notes.next.max(highest.wrapping_add(1)).max(1);
        notes
    }

    /// De la más reciente a la más antigua.
    pub fn all(&self) -> &[Note] {
        &self.list
    }

    pub fn get(&self, id: u32) -> Option<&Note> {
        self.list.iter().find(|n| n.id == id)
    }

    /// El primer id libre; el bus reparte desde aquí.
    pub fn next_id(&self) -> u32 {
        self.next
    }

    fn save(&self, note: &Note) {
        let Some(db) = &self.db else { return };
        if note.transient {
            return;
        }
        let data = serde_json::to_string(note).unwrap_or_default();
        let _ = db.execute("INSERT OR REPLACE INTO notes (id, at, data) VALUES (?1, ?2, ?3)", params![note.id, note.at, data]);
    }

    fn forget(&mut self, id: u32) -> Option<Note> {
        self.due.remove(&id);
        let index = self.list.iter().position(|n| n.id == id)?;
        if let Some(db) = &self.db {
            let _ = db.execute("DELETE FROM notes WHERE id = ?1", params![id]);
        }
        Some(self.list.remove(index))
    }

    /// Llega una (o una que sustituye a otra con su id). En no molestar
    /// solo sale como aviso lo crítico.
    pub fn arrive(&mut self, mut note: Note, quiet: bool, now: Instant) -> Vec<Out> {
        let mut out = Vec::new();
        note.live = true;
        note.popup = !quiet || note.critical();
        if note.id >= self.next {
            self.next = note.id.wrapping_add(1).max(1);
            if let Some(db) = &self.db {
                let _ = db.execute("INSERT OR REPLACE INTO meta (key, value) VALUES ('next', ?1)", params![self.next]);
            }
        }
        // La que sustituye ocupa su sitio arriba, como recién llegada.
        self.list.retain(|n| n.id != note.id);
        self.due.remove(&note.id);
        if note.popup
            && !self.held
            && let Some(span) = note.span()
        {
            self.due.insert(note.id, now + span);
        }
        self.save(&note);
        self.list.insert(0, note.clone());
        out.push(Out::Show(Box::new(note)));
        out
    }

    /// Borra las que llegaron antes de `before` (ms), salvo un aviso que
    /// aún está a la vista.
    pub fn prune(&mut self, before: i64) -> Vec<Out> {
        let old: Vec<u32> = self.list.iter().filter(|n| n.at < before && !n.popup).map(|n| n.id).collect();
        old.into_iter().flat_map(|id| self.remove(id, Reason::Expired)).collect()
    }

    /// Se va del todo: se cierra si vivía y se borra.
    fn remove(&mut self, id: u32, reason: Reason) -> Vec<Out> {
        let Some(note) = self.forget(id) else { return Vec::new() };
        let mut out = vec![Out::Gone(id)];
        if note.live {
            out.push(Out::Closed(id, reason));
        }
        out
    }

    /// Su programa la retira: se queda en el cajón, ya sin vida, salvo una
    /// pasajera, que se va.
    pub fn withdraw(&mut self, id: u32) -> Vec<Out> {
        // Una que ya no está (la descartaste, o pulsaste una acción) ya se cerró.
        let Some(note) = self.get(id).cloned() else { return Vec::new() };
        if note.transient {
            return self.remove(id, Reason::Closed);
        }
        self.due.remove(&id);
        let note = Note { live: false, popup: false, ..note };
        self.replace(note.clone());
        vec![Out::Show(Box::new(note)), Out::Closed(id, Reason::Closed)]
    }

    fn replace(&mut self, note: Note) {
        self.save(&note);
        if let Some(slot) = self.list.iter_mut().find(|n| n.id == note.id) {
            *slot = note;
        }
    }

    /// La descartas tú: se cierra y se borra.
    pub fn dismiss(&mut self, id: u32) -> Vec<Out> {
        self.remove(id, Reason::Dismissed)
    }

    pub fn clear(&mut self) -> Vec<Out> {
        let ids: Vec<u32> = self.list.iter().map(|n| n.id).collect();
        ids.into_iter().flat_map(|id| self.remove(id, Reason::Dismissed)).collect()
    }

    /// Pulsas una de sus acciones: si su programa la escucha, se le cuenta
    /// (devuelve true) y, salvo que quiera quedarse, se va.
    pub fn act(&mut self, id: u32) -> (bool, Vec<Out>) {
        let Some(note) = self.get(id).cloned() else { return (false, Vec::new()) };
        if !note.live {
            return (false, Vec::new());
        }
        if note.resident {
            return (true, Vec::new());
        }
        (true, self.remove(id, Reason::Dismissed))
    }

    /// Con el puntero encima (o escribiendo), los avisos no caducan; al
    /// soltarlos, cada uno vuelve a empezar su plazo.
    pub fn hold(&mut self, on: bool, now: Instant) {
        if on == self.held {
            return;
        }
        self.held = on;
        self.due.clear();
        if !on {
            for note in &self.list {
                if note.popup
                    && let Some(span) = note.span()
                {
                    self.due.insert(note.id, now + span);
                }
            }
        }
    }

    /// Cuándo caduca el próximo aviso.
    pub fn next_due(&self) -> Option<Instant> {
        self.due.values().min().copied()
    }

    /// Los avisos que han caducado pasan al cajón; uno pasajero se va.
    pub fn expire(&mut self, now: Instant) -> Vec<Out> {
        let mut gone: Vec<u32> = self.due.iter().filter(|(_, at)| **at <= now).map(|(&id, _)| id).collect();
        gone.sort_unstable();
        let mut out = Vec::new();
        for id in gone {
            self.due.remove(&id);
            let Some(note) = self.get(id).cloned() else { continue };
            if note.transient {
                out.extend(self.remove(id, Reason::Expired));
            } else {
                let note = Note { popup: false, ..note };
                self.replace(note.clone());
                out.push(Out::Show(Box::new(note)));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(id: u32) -> Note {
        Note { id, app: "prueba".into(), summary: format!("nota {id}"), urgency: "normal".into(), progress: -1, timeout: -1, ..Note::default() }
    }

    #[test]
    fn popups_expire_into_the_drawer() {
        let mut notes = Notes::memory();
        let t = Instant::now();
        notes.arrive(note(1), false, t);
        assert!(notes.get(1).unwrap().popup);
        assert_eq!(notes.next_due(), Some(t + PATIENCE));
        assert!(notes.expire(t + Duration::from_millis(10)).is_empty());
        let out = notes.expire(t + PATIENCE);
        let kept = notes.get(1).unwrap();
        assert!(!kept.popup && kept.live);
        assert_eq!(out, vec![Out::Show(Box::new(kept.clone()))]);
        assert_eq!(notes.next_due(), None);
    }

    #[test]
    fn critical_and_sticky_never_expire() {
        let mut notes = Notes::memory();
        let t = Instant::now();
        notes.arrive(Note { urgency: "critical".into(), ..note(1) }, false, t);
        notes.arrive(Note { timeout: 0, ..note(2) }, false, t);
        assert_eq!(notes.next_due(), None);
    }

    #[test]
    fn quiet_keeps_all_but_critical_out_of_sight() {
        let mut notes = Notes::memory();
        let t = Instant::now();
        notes.arrive(note(1), true, t);
        notes.arrive(Note { urgency: "critical".into(), ..note(2) }, true, t);
        assert!(!notes.get(1).unwrap().popup);
        assert!(notes.get(2).unwrap().popup);
        assert_eq!(notes.all().len(), 2);
    }

    #[test]
    fn holding_pauses_and_restarts_every_popup() {
        let mut notes = Notes::memory();
        let t = Instant::now();
        notes.arrive(note(1), false, t);
        notes.hold(true, t);
        assert_eq!(notes.next_due(), None);
        notes.arrive(note(2), false, t);
        assert_eq!(notes.next_due(), None);
        let later = t + Duration::from_secs(60);
        assert!(notes.expire(later).is_empty());
        notes.hold(false, later);
        assert_eq!(notes.next_due(), Some(later + PATIENCE));
    }

    #[test]
    fn replacing_keeps_the_id_and_moves_it_to_the_top() {
        let mut notes = Notes::memory();
        let t = Instant::now();
        notes.arrive(note(1), false, t);
        notes.arrive(note(2), false, t);
        notes.arrive(Note { summary: "otra".into(), ..note(1) }, false, t);
        let ids: Vec<u32> = notes.all().iter().map(|n| n.id).collect();
        assert_eq!(ids, vec![1, 2]);
        assert_eq!(notes.get(1).unwrap().summary, "otra");
        assert_eq!(notes.next_id(), 3);
    }

    #[test]
    fn withdrawn_stay_in_the_drawer_but_transient_ones_leave() {
        let mut notes = Notes::memory();
        let t = Instant::now();
        notes.arrive(note(1), false, t);
        notes.arrive(Note { transient: true, ..note(2) }, false, t);
        let out = notes.withdraw(1);
        assert!(out.contains(&Out::Closed(1, Reason::Closed)));
        let kept = notes.get(1).unwrap();
        assert!(!kept.live && !kept.popup);
        assert_eq!(notes.withdraw(2), vec![Out::Gone(2), Out::Closed(2, Reason::Closed)]);
        assert!(notes.get(2).is_none());
        // Una que ya no está no se cierra dos veces.
        assert_eq!(notes.withdraw(9), vec![]);
    }

    #[test]
    fn transient_popups_close_when_they_expire() {
        let mut notes = Notes::memory();
        let t = Instant::now();
        notes.arrive(Note { transient: true, ..note(1) }, false, t);
        assert_eq!(notes.expire(t + PATIENCE), vec![Out::Gone(1), Out::Closed(1, Reason::Expired)]);
    }

    #[test]
    fn acting_closes_unless_resident_and_only_while_live() {
        let mut notes = Notes::memory();
        let t = Instant::now();
        notes.arrive(note(1), false, t);
        notes.arrive(Note { resident: true, ..note(2) }, false, t);
        assert_eq!(notes.act(1), (true, vec![Out::Gone(1), Out::Closed(1, Reason::Dismissed)]));
        assert_eq!(notes.act(2), (true, vec![]));
        assert!(notes.get(2).is_some());
        notes.withdraw(2);
        assert_eq!(notes.act(2), (false, vec![]));
    }

    #[test]
    fn dismissing_a_dead_note_only_forgets_it() {
        let mut notes = Notes::memory();
        notes.arrive(note(1), false, Instant::now());
        notes.withdraw(1);
        assert_eq!(notes.dismiss(1), vec![Out::Gone(1)]);
        assert!(notes.all().is_empty());
    }

    #[test]
    fn old_notes_are_cleaned_up_but_not_visible_popups() {
        let mut notes = Notes::memory();
        let t = Instant::now();
        notes.arrive(Note { at: 100, ..note(1) }, false, t);
        notes.arrive(Note { at: 100, ..note(2) }, false, t);
        notes.arrive(Note { at: 900, ..note(3) }, false, t);
        notes.expire(t + PATIENCE);
        notes.withdraw(2);
        // La 1 sigue viva: se cierra al borrarla; la 2 ya estaba cerrada.
        let mut out = notes.prune(500);
        out.sort_by_key(|o| format!("{o:?}"));
        assert_eq!(out, vec![Out::Closed(1, Reason::Expired), Out::Gone(1), Out::Gone(2)]);
        assert_eq!(notes.all().iter().map(|n| n.id).collect::<Vec<_>>(), vec![3]);
        // Un aviso que sigue a la vista espera a caducar.
        notes.arrive(Note { at: 100, urgency: "critical".into(), ..note(4) }, false, t);
        assert!(notes.prune(500).is_empty());
    }

    #[test]
    fn saved_notes_survive_a_restart_without_life() {
        let dir = std::env::temp_dir().join(format!("veyld-notes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("notifications.db");
        {
            let mut notes = Notes::open(&path).unwrap();
            notes.arrive(note(7), false, Instant::now());
            notes.arrive(Note { transient: true, ..note(8) }, false, Instant::now());
        }
        let notes = Notes::open(&path).unwrap();
        assert_eq!(notes.all().len(), 1);
        let kept = notes.get(7).unwrap();
        assert!(!kept.live && !kept.popup);
        // Los ids no se repiten aunque la pasajera no se guardara.
        assert_eq!(notes.next_id(), 9);
        let mode = std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(&path).unwrap().permissions());
        assert_eq!(mode & 0o777, 0o600);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
