//! Lo que veyld nota solo: el micro y la cámara, lo que suena, lo que se
//! abrió hace poco, si estás delante y lo que cuenta Hyprland.

use super::*;

impl Daemon {
    /// Lo que está pasando ahora y no avisa cuando el diario se pone en marcha:
    /// los agentes, lo que suena, los micros.
    pub(super) fn feed(&mut self, now: i64) {
        let devices = self.devices();
        let playing = self.audible();
        if let Some(journal) = &mut self.journal {
            journal.agents(now, &self.agents);
            journal.media(now, &playing);
            for (kind, on) in devices {
                journal.device(now, kind, on);
            }
        }
    }

    pub(super) fn devices(&self) -> [(&'static str, bool); 3] {
        [(DEVICES[0], self.shell.mic && !self.listening), (DEVICES[1], self.shell.camera), (DEVICES[2], self.shell.screen)]
    }

    /// Lo que suena y se puede apuntar: si hay una ventana privada de un
    /// navegador, lo que suene en él puede ser de ella.
    pub(super) fn audible(&self) -> Vec<Playing> {
        if self.playing.is_empty() {
            return Vec::new();
        }
        let hidden: Vec<&'static str> =
            self.hypr.clients().iter().filter(|w| private(&w.class, &w.title)).filter_map(|w| crate::web::browser(&w.class)).collect();
        self.playing
            .iter()
            .filter(|p| {
                let player = format!("{} {}", p.player, p.bus).to_lowercase();
                !hidden.iter().any(|b| player.contains(b))
            })
            .cloned()
            .collect()
    }

    pub(super) fn media_changed(&mut self, playing: Vec<Playing>) {
        self.playing = playing;
        if !self.settings.journal {
            return;
        }
        let audible = self.audible();
        if let Some(journal) = &mut self.journal {
            journal.media(now(), &audible);
        }
    }

    /// Los archivos que han abierto las aplicaciones, si la lista cambió.
    pub(super) fn read_recent(&mut self, now: i64) {
        let path = recent::path();
        let Ok(modified) = fs::metadata(&path).and_then(|m| m.modified()) else {
            return;
        };
        if self.recent == Some(modified) {
            return;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            return;
        };
        self.recent = Some(modified);
        if let Some(journal) = &mut self.journal {
            journal.files(&recent::read(&text), now - journal::KEEP);
        }
    }

    /// Si la máquina ha dormido desde lo último que pasó, lo apunta: lo
    /// abierto acaba al dormirse y vuelve a empezar al despertar.
    pub(super) fn woke(&mut self) {
        let (boot, mono, wall) = (clock(libc::CLOCK_BOOTTIME), clock(libc::CLOCK_MONOTONIC), now());
        let before = self.clock.replace((boot, mono, wall));
        let Some((b, m, w)) = before else {
            return;
        };
        let slept = (boot - b) - (mono - m);
        if slept < 20_000 || !self.settings.journal {
            return;
        }
        let reason = self.absence();
        let Some(journal) = &mut self.journal else {
            return;
        };
        let (from, to) = (w, (w + slept).min(wall));
        journal.slept(from, to);
        if let Some(reason) = reason {
            journal.away(to, Some(reason));
        }
        self.feed(wall);
        self.look(wall);
    }

    /// Lo que pide que no se bloquee, para Veyl.
    pub(super) fn awake(&self) -> Value {
        json!({ "awake": self.held.iter().map(|h| json!({ "app": h.app, "reason": h.reason })).collect::<Vec<_>>() })
    }

    /// Por qué no estás, si no estás.
    pub(super) fn absence(&self) -> Option<&'static str> {
        if self.shell.locked {
            Some("bloqueado")
        } else if self.idle && self.held.is_empty() {
            Some("inactivo")
        } else {
            None
        }
    }

    /// Lo que se apunta de la ventana con el foco.
    pub(super) fn seen(&mut self, window: &Window) -> Seen {
        let private = private(&window.class, &window.title);
        let place = if window.pid > 0 && !private {
            // Una terminal puede cambiar el título muchas veces por segundo.
            // Reutiliza el árbol, pero sigue leyendo la carpeta actual.
            if self.processes_read.is_none_or(|at| at.elapsed() >= Duration::from_secs(1)) {
                self.processes = procs::all();
                self.children = procs::children(&self.processes);
                self.processes_read = Some(Instant::now());
            }
            procs::place(window.pid, &self.processes, &self.children)
        } else {
            procs::Place::default()
        };
        Seen {
            window: window.address.clone(),
            app: window.class.clone(),
            title: if private { String::new() } else { window.title.clone() },
            folder: place.folder,
            command: place.command,
            workspace: window.workspace.clone(),
            pid: window.pid,
        }
    }

    /// Apunta lo que tiene el foco, si estás y se lleva el diario.
    pub(super) fn look(&mut self, now: i64) {
        if !self.settings.journal || self.journal.is_none() || self.absence().is_some() {
            return;
        }
        let seen = self.current.clone().map(|w| self.seen(&w));
        if let Some(journal) = &mut self.journal {
            journal.focus(now, seen);
        }
    }

    /// Algo cambió en si estás (`before` es cómo estaba).
    pub(super) fn presence(&mut self, now: i64, before: Option<&'static str>) {
        if !self.settings.journal {
            return;
        }
        let after = self.absence();
        let Some(journal) = &mut self.journal else {
            return;
        };
        match (before, after) {
            (None, Some(reason)) => {
                // La inactividad se nota cuando ya ha pasado un rato.
                let since = if reason == "inactivo" { now - idle::AWAY.as_millis() as i64 } else { now };
                journal.away(since, Some(reason));
            }
            (Some(_), None) => {
                journal.away(now, None);
                self.look(now);
            }
            _ => {}
        }
    }

    pub(super) fn hypr_event(&mut self, name: &str, data: &str) {
        let now = now();
        let address = |hex: &str| format!("0x{}", hex.trim_start_matches("0x"));
        match name {
            "activewindowv2" => {
                self.current = if data.trim().is_empty() || data == "," { None } else { self.hypr.active() };
                self.look(now);
            }
            "windowtitlev2" => {
                if let Some((hex, title)) = data.split_once(',')
                    && let Some(current) = &mut self.current
                    && current.address == address(hex)
                {
                    current.title = title.to_string();
                    self.look(now);
                }
            }
            "openwindow" if self.settings.journal => {
                let parts: Vec<&str> = data.splitn(4, ',').collect();
                if let ([hex, workspace, class, title], Some(journal)) = (parts.as_slice(), &mut self.journal) {
                    journal.opened(now, &address(hex), class, if private(class, title) { "" } else { title }, workspace);
                }
            }
            "closewindow" if self.settings.journal => {
                if let Some(journal) = &mut self.journal {
                    journal.closed(now, &address(data));
                }
            }
            "movewindowv2" => {
                let parts: Vec<&str> = data.splitn(3, ',').collect();
                if let ([hex, _, workspace], Some(current)) = (parts.as_slice(), &mut self.current)
                    && current.address == address(hex)
                {
                    current.workspace = workspace.to_string();
                    self.look(now);
                }
            }
            "configreloaded" => self.split = self.hypr.split(),
            // Una vista abre o cierra con su espacio.
            "createworkspacev2" | "destroyworkspacev2" | "renameworkspace" => self.views_changed(),
            _ => {}
        }
    }
}
