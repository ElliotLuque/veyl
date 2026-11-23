//! Lo que se le dice: las órdenes que entiende la gramática, lo que va al
//! modelo (con sus pasos y su tarjeta), las vistas y la voz.

use super::*;

impl Daemon {
    /// Una orden: la gramática, y si no la entiende, el modelo (si hay).
    pub(super) fn say(&mut self, client: u64, id: Value, said: &str, source: &str, confirmed: bool) {
        let plan = orders::plan(self, said);
        let first = plan.as_ref().and_then(|p| p.first()).map(|p| p.intent.clone());
        if first.as_deref().is_none_or(|i| i != "confirm" && i != "cancel") {
            self.pending = None;
        }
        let Some(plan) = plan else {
            if self.thinks() {
                self.ask_model(client, id, said, source);
                return;
            }
            let outcome = Outcome::fail("No sé hacer eso todavía");
            self.answer(client, id, said, source, None, "none", outcome);
            return;
        };
        // Lo que hay que medir tarda: en un hilo, con «Pensando» mientras.
        if let [one] = plan.as_slice()
            && one.intent.starts_with("system.")
        {
            self.sense(client, id, said, source, one.clone());
            return;
        }
        let outcome = match first.as_deref() {
            Some("confirm") => match self.pending.take() {
                Some((pending, at, owner)) if owner == source && at.elapsed() < PATIENCE => self.perform(&pending, source, true),
                _ => Outcome::fail("No hay nada que confirmar"),
            },
            Some("cancel") => match self.pending.take() {
                Some(_) => Outcome::done("Cancelado"),
                None => Outcome::fail("No hay nada que cancelar"),
            },
            _ => {
                // Casa con la gramática pero no hay a qué aplicarlo («abre la puerta del
                // garaje»): mejor que lo intente el modelo, si lo hay.
                let ready = orders::describe_plan(self, &plan).ready;
                if !ready && self.thinks() {
                    self.ask_model(client, id, said, source);
                    return;
                }
                // Sin modelo, una canción que no se ha pedido como tal no se pone.
                if !ready && plan.iter().any(|p| p.intent == "play.search") {
                    Outcome::fail("No sé qué poner: di «pon … en Spotify» o «pon la canción …»")
                } else {
                    self.perform(&plan, source, confirmed)
                }
            }
        };
        let logged = match plan.as_slice() {
            [one] => one.clone(),
            many => Parse {
                intent: many.iter().map(|p| p.intent.as_str()).collect::<Vec<_>>().join("+"),
                args: [("plan".to_string(), json!(many))].into_iter().collect(),
                heard: many.iter().map(|p| p.heard.as_str()).collect::<Vec<_>>().join(" · "),
            },
        };
        // Lo que contesta con varias líneas (la agenda, el diario, los agentes)
        // se lee mejor en una tarjeta que en una notificación.
        if outcome.ok
            && source != "cli"
            && let Some(body) = outcome.answer.as_deref().filter(|b| !b.trim().is_empty())
            && let Some(card) = crate::card::from_answer(&outcome.say, orders::describe_plan(self, &plan).glyph, body)
        {
            self.present(card, &outcome, said);
        }
        self.answer(client, id, said, source, Some(&logged), "grammar", outcome);
        self.carded = false;
    }

    /// Mide lo que se pregunta del ordenador en un hilo aparte.
    pub(super) fn sense(&mut self, client: u64, id: Value, said: &str, source: &str, parse: Parse) {
        self.ponder(said);
        let windows = crate::system::seen(self);
        let (tx, said, source) = (self.tx.clone(), said.to_string(), source.to_string());
        thread::spawn(move || {
            let (outcome, card) = crate::system::run(&parse.intent, &windows);
            let _ = tx.send(Event::Sensed { client, id, said, source, parse, outcome, card });
        });
    }

    /// Enseña una tarjeta en la isla, en vez de la notificación, si hay un Veyl que la pinte.
    pub(super) fn present(&mut self, mut card: Value, outcome: &Outcome, said: &str) {
        if !outcome.ok || !self.clients.values().any(|c| c.shell) {
            return;
        }
        card["say"] = json!(outcome.say);
        card["said"] = json!(said);
        self.broadcast(&json!({ "card": card }), |c| c.shell);
        self.shown_card = Some(card);
        self.carded = true;
    }

    /// Hace las órdenes una tras otra. Si una es peligrosa y no está
    /// confirmada, se para ahí y espera un «sí» para ella y las siguientes;
    /// si una falla, las siguientes no se hacen.
    pub(super) fn perform(&mut self, plan: &[Parse], source: &str, confirmed: bool) -> Outcome {
        let mut said: Vec<String> = Vec::new();
        let mut bodies: Vec<String> = Vec::new();
        for (i, parse) in plan.iter().enumerate() {
            let shown = orders::describe(self, parse);
            if shown.confirm && !confirmed {
                self.pending = Some((plan[i..].to_vec(), Instant::now(), source.to_string()));
                said.push(format!("¿{}? Di «sí» para confirmar", shown.title));
                break;
            }
            let outcome = orders::run(self, parse, source);
            said.push(outcome.say);
            bodies.extend(outcome.answer);
            if !outcome.ok {
                let say = said.join(". ");
                return Outcome { ok: false, say, answer: (!bodies.is_empty()).then(|| bodies.join("\n")) };
            }
        }
        Outcome { ok: true, say: said.join(". "), answer: (!bodies.is_empty()).then(|| bodies.join("\n")) }
    }

    #[allow(clippy::too_many_arguments)]
    /// Contesta, lo apunta en el diario y devuelve su fila, si la hay.
    pub(super) fn answer(&mut self, client: u64, id: Value, said: &str, source: &str, parse: Option<&Parse>, handler: &str, outcome: Outcome) -> Option<i64> {
        let shown = self.shown_card.take();
        let card = shown.as_ref().map(|c| c.to_string()).unwrap_or_default();
        let mut logged = None;
        if self.settings.journal
            && let Some(journal) = &mut self.journal
        {
            let args = parse.map(|p| Value::Object(p.args.clone()).to_string()).unwrap_or_default();
            let answer = match &outcome.answer {
                Some(body) => format!("{}\n{body}", outcome.say),
                None => outcome.say.clone(),
            };
            logged = journal
                .order(now(), source, said, parse.map(|p| p.intent.as_str()), &args, handler, outcome.ok, &answer, &card)
                .and_then(|row| journal.asked(1, Some(row)).pop());
        }
        let row = logged.as_ref().and_then(|one| one["id"].as_i64());
        // Al historial del centro de control; lo que se pide desde la línea de órdenes no es una consulta tuya.
        if let Some(one) = logged
            && source != "cli"
        {
            self.broadcast(&json!({ "asked_one": one }), |c| c.shell);
        }
        let reply = json!({
            "id": id,
            "ok": outcome.ok,
            "say": outcome.say,
            "answer": outcome.answer,
            "intent": parse.map(|p| p.intent.clone()),
            "handler": handler,
            "answer_card": shown,
        });
        self.send(client, &reply);
        // Desde la línea de órdenes la respuesta se lee ahí; por voz, todo se
        // dice; desde la entrada, solo lo que contesta algo o lo que falla.
        let show = match source {
            "cli" | "diary" => false,
            "voice" => true,
            _ => outcome.answer.is_some() || !outcome.ok,
        };
        if show && !self.carded {
            notify(&outcome);
        }
        // A lo dicho por voz, Veyl contesta en voz alta: lo corto; lo largo, en la notificación.
        if source == "voice"
            && self.settings.speak
            && !self.shell.quiet
            && let Some(mouth) = &self.mouth
        {
            let _ = mouth.send(tts::Say::Text { text: tts::spoken(&outcome.say), style: self.settings.style() });
        }
        row
    }

    /// Lo que la gramática no entiende, al modelo: lo dicho por la entrada
    /// estándar de `llm`, y lo que escriba es la respuesta.
    /// Si hay alguien a quien preguntar lo que la gramática no entiende.
    pub fn thinks(&self) -> bool {
        !self.settings.brain.is_empty() || !self.settings.llm.is_empty()
    }

    /// Que Veyl enseñe que está pensando en algo. Devuelve su número.
    pub(super) fn ponder(&mut self, said: &str) -> u64 {
        self.ponder_as(said, json!({}))
    }

    /// Lo mismo, con algo más que enseñar (si se puede parar, lo que va haciendo).
    pub(super) fn ponder_as(&mut self, said: &str, extra: Value) -> u64 {
        self.pondering += 1;
        self.thought += 1;
        self.pondered = json!({ "said": said, "since": now(), "n": self.thought });
        if let (Some(all), Some(extra)) = (self.pondered.as_object_mut(), extra.as_object()) {
            all.extend(extra.clone());
        }
        let thinking = json!({ "thinking": self.pondered });
        self.broadcast(&thinking, |c| c.shell);
        self.thought
    }

    /// Lo que va haciendo el cerebro con la pregunta `n`, a lo que enseña Veyl:
    /// lo último que hace (sin repetir lo mismo dos veces seguidas) y las
    /// páginas que encuentra, una por sitio.
    pub(super) fn stepped(&mut self, n: u64, step: brain::Step) {
        if self.pondered.get("n").and_then(Value::as_u64) != Some(n) || !self.asking.contains_key(&n) {
            return;
        }
        let list = |key: &str, pondered: &Value| -> Vec<Value> { pondered.get(key).and_then(Value::as_array).cloned().unwrap_or_default() };
        match step {
            brain::Step::Doing { icon, text } => {
                let mut steps = list("steps", &self.pondered);
                if steps.last().is_some_and(|last| last["texto"] == text) {
                    return;
                }
                steps.push(json!({ "icono": icon, "texto": text, "at": now() }));
                let from = steps.len().saturating_sub(12);
                self.pondered["steps"] = json!(steps[from..]);
            }
            brain::Step::Found(found) => {
                let mut sources = list("sources", &self.pondered);
                let before = sources.len();
                for source in found {
                    let host = brain::host(&source.url);
                    if sources.len() < 8 && !host.is_empty() && !sources.iter().any(|s| s["host"] == host) {
                        sources.push(json!({ "titulo": source.title, "host": host, "url": source.url }));
                    }
                }
                if sources.len() == before {
                    return;
                }
                self.pondered["sources"] = json!(sources);
            }
        }
        let thinking = json!({ "thinking": self.pondered });
        self.broadcast(&thinking, |c| c.shell);
    }

    /// Deja de pensar la pregunta `n`: se le dice al modelo que pare y Veyl
    /// deja de enseñarla ya; cuando conteste, se tira.
    pub(super) fn stop_thinking(&mut self, n: u64) {
        let Some((stop, _)) = self.asking.remove(&n) else {
            return;
        };
        stop.store(true, Ordering::Relaxed);
        self.card = None;
        if self.cover.as_ref().is_some_and(|(at, _)| *at == n) {
            self.cover = None;
        }
        self.pondered_one();
    }

    /// La portada de la tarjeta de la pregunta `n`: vale la primera que llega.
    /// Se le pone a la tarjeta que se está componiendo o, si ya se apuntó en
    /// el diario, a la de allí; y Veyl se la pone a la que enseña, si es esa.
    pub(super) fn covered(&mut self, n: u64, url: String) {
        let Some((_, logged)) = self.cover.take_if(|(at, _)| *at == n) else {
            return;
        };
        if self.asking.contains_key(&n)
            && let Some(card) = self.card.as_mut()
        {
            card["imagen"] = json!(url);
        }
        if let Some((mut card, row)) = logged
            && let Some(journal) = &mut self.journal
        {
            card["imagen"] = json!(url);
            if journal.order_card(row, &card.to_string())
                && let Some(one) = journal.asked(1, Some(row)).pop()
            {
                self.broadcast(&json!({ "asked_one": one }), |c| c.shell);
            }
        }
        self.broadcast(&json!({ "card_patch": { "n": n, "imagen": url } }), |c| c.shell);
    }

    /// Ha acabado de pensar una; si no queda ninguna, Veyl deja de enseñarlo.
    pub(super) fn pondered_one(&mut self) {
        self.pondering = self.pondering.saturating_sub(1);
        if self.pondering == 0 {
            self.pondered = Value::Null;
            self.broadcast(&json!({ "thinking": null }), |c| c.shell);
        }
    }

    /// Lo que acaba más tarde que la orden que lo pidió: siempre se avisa
    /// y, si se dijo por voz, se dice.
    pub(super) fn later(&mut self, source: &str, outcome: &Outcome) {
        notify(outcome);
        if source == "voice"
            && self.settings.speak
            && !self.shell.quiet
            && let Some(mouth) = &self.mouth
        {
            let _ = mouth.send(tts::Say::Text { text: tts::spoken(&outcome.say), style: self.settings.style() });
        }
    }

    /// Las ventanas abiertas, con su carpeta (en una terminal), lo que tiene
    /// en marcha y su página (en un navegador): con lo que se elige una vista.
    /// Las privadas no salen.
    pub fn candidates(&mut self) -> Vec<Candidate> {
        self.processes = procs::all();
        self.children = procs::children(&self.processes);
        self.processes_read = Some(Instant::now());
        let level = |workspace: i64| if self.split > 0 && workspace > 0 { (workspace - 1) % self.split as i64 + 1 } else { workspace };
        let mut out = Vec::new();
        for window in self.hypr.clients() {
            if private(&window.class, &window.title) || window.workspace.starts_with("special:") {
                continue;
            }
            let place = if window.pid > 0 { procs::place(window.pid, &self.processes, &self.children) } else { procs::Place::default() };
            let url = self.web.url(&window.class, &window.title).unwrap_or_default();
            out.push(Candidate {
                app: apps::label(&self.apps, &window.class),
                folder: place.folder,
                command: place.command,
                url,
                level: level(window.workspace_id),
                window,
            });
        }
        out
    }

    /// Le pide al cerebro, aparte, qué ventanas van en la vista `name`; la
    /// vista se hace cuando contesta (`Event::ViewChosen`).
    pub fn think_view(&mut self, name: &str, candidates: Vec<Candidate>, here: i64, source: &str) {
        self.ponder(&format!("Qué va en la vista «{name}»"));
        let (tx, settings, name, source) = (self.tx.clone(), self.settings.clone(), name.to_string(), source.to_string());
        thread::spawn(move || {
            let asked = views::prompt(&name, &candidates);
            let choice = crate::brain::judge(&settings, views::SYSTEM, &asked)
                .and_then(|answer| views::read(&answer, &candidates).ok_or_else(|| format!("respuesta sin JSON: {answer}")));
            let _ = tx.send(Event::ViewChosen { name, candidates, here, source, choice });
        });
    }

    /// Las vistas, como las ve Veyl: con las que están abiertas ahora.
    pub fn views_listing(&self) -> Value {
        let open: HashMap<String, i64> = self
            .hypr
            .json("workspaces")
            .and_then(|w| w.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|w| Some((w.get("name")?.as_str()?.to_string(), w.get("windows")?.as_i64()?)))
            .collect();
        views::listing(&self.views, &open, &self.apps)
    }

    /// Cuenta a Veyl cómo quedan las vistas.
    pub fn views_changed(&mut self) {
        let views = json!({ "views": self.views_listing() });
        self.broadcast(&views, |c| c.shell);
    }

    pub(super) fn ask_model(&mut self, client: u64, id: Value, said: &str, source: &str) {
        // Que Veyl enseñe que está pensando y lo que va haciendo; se puede parar.
        self.card = None;
        self.cover = None;
        let n = self.ponder_as(said, json!({ "stop": true, "steps": [], "sources": [] }));
        let stop = Arc::new(AtomicBool::new(false));
        self.asking.insert(n, (stop.clone(), said.to_string()));
        let (tx, said, source) = (self.tx.clone(), said.to_string(), source.to_string());
        if self.settings.brain.is_empty() {
            let command = self.settings.llm.clone();
            thread::spawn(move || {
                let outcome = model(&command, &said, &source);
                let _ = tx.send(Event::Answered { client, id, said, source, outcome, n });
            });
            return;
        }
        // Lo hablado en los últimos diez minutos, para poder seguir la conversación.
        self.talk.retain(|(at, _, _)| at.elapsed() < Duration::from_secs(600));
        let history: Vec<(String, String)> = self.talk.iter().rev().take(6).rev().map(|(_, said, answer)| (said.clone(), answer.clone())).collect();
        self.talk_source = source.clone();
        let (settings, socket) = (self.settings.clone(), config::socket());
        // Lo que hay delante va ya con la pregunta: casi nunca hace falta pedirlo (una vuelta menos).
        let mut context = self.context();
        if source == "diary" {
            context["pregunta_desde"] = json!("la vista del diario: casi seguro que es sobre lo que hizo; usa la herramienta `diario`");
            if !self.diary_day.is_empty() {
                context["dia_que_mira_en_el_diario"] = json!(format!("{} (así vale en `cuando`)", self.diary_day));
            }
        }
        let context = context.to_string();
        let windows = crate::system::seen(self);
        thread::spawn(move || {
            let ask = brain::Ask { said: said.clone(), history, context, stop };
            let (tools, from, steps) = (tx.clone(), source.clone(), tx.clone());
            let progress = move |step: brain::Step| {
                let _ = steps.send(Event::Step { n, step });
            };
            let tool = move |name: &str, input: &Value| -> String {
                // Medir tarda: se mide en este hilo, sin parar al daemon.
                if name == "sistema" {
                    return crate::system::told(input, &windows);
                }
                let (reply, wait) = mpsc::channel();
                let asked = Event::Tool { name: name.to_string(), input: input.clone(), source: from.clone(), reply };
                if tools.send(asked).is_err() {
                    return "veyld se ha ido".into();
                }
                wait.recv_timeout(Duration::from_secs(60)).unwrap_or_else(|_| "Veyl no contesta".into())
            };
            let outcome = match brain::ask(&settings, &ask, &tool, &progress, &socket) {
                Ok(text) if !brain::plain(&text).is_empty() => {
                    let text = brain::plain(&text);
                    let (say, rest) = text.split_once('\n').map(|(a, b)| (a.trim().to_string(), Some(b.trim().to_string()))).unwrap_or((text.clone(), None));
                    Outcome { ok: true, say, answer: rest.filter(|r| !r.is_empty()) }
                }
                // Sin frase: quizá lo dijo todo en la tarjeta (se mira al llegar).
                Ok(_) => Outcome { ok: true, say: String::new(), answer: None },
                Err(why) => Outcome::fail(why),
            };
            let _ = tx.send(Event::Answered { client, id, said, source, outcome, n });
        });
    }

    /// Lo que contestó el modelo a la pregunta `n`. Si se paró, solo se le dice
    /// a quien preguntó: Veyl ya dejó de enseñarla.
    pub(super) fn answered(&mut self, client: u64, id: Value, said: &str, source: &str, outcome: Outcome, n: u64) {
        if self.asking.remove(&n).is_none() {
            self.send(client, &json!({ "id": id, "ok": false, "say": brain::STOPPED }));
            return;
        }
        self.pondered_one();
        if outcome.ok && !self.settings.brain.is_empty() {
            let answer = outcome.answer.as_ref().map_or(outcome.say.clone(), |a| format!("{}\n{a}", outcome.say));
            self.talk.push((Instant::now(), said.to_string(), answer));
        }
        let handler = if self.settings.brain.is_empty() { "llm".to_string() } else { format!("brain:{}", self.settings.brain) };
        let mut outcome = outcome;
        if outcome.ok && outcome.say.is_empty() {
            match self.card.as_ref().and_then(|c| c["titulo"].as_str()).filter(|t| !t.is_empty()) {
                Some(title) => outcome.say = title.to_string(),
                None => outcome = Outcome::fail("El modelo no ha dicho nada"),
            }
        }
        // Sin tarjeta del modelo, una respuesta larga no cabe en la
        // notificación: va en una tarjeta hecha con su texto.
        if self.card.is_none() && outcome.ok && (outcome.say.chars().count() > 110 || outcome.say.contains('\n')) {
            self.card = Some(crate::card::from_text(&outcome.say));
        }
        // Con tarjeta, Veyl la enseña en la isla en vez de la notificación. Si
        // ya la enseñaba mientras el modelo acababa (`live`), es la misma: solo
        // gana la frase. Si el modelo ha fallado después, se queda como estaba.
        let card = self.card.take();
        if !outcome.ok && card.is_some() {
            self.broadcast(&json!({ "card_patch": { "n": n, "live": false } }), |c| c.shell);
        }
        if let Some(mut card) = card.filter(|_| outcome.ok && (source == "diary" || self.clients.values().any(|c| c.shell))) {
            card["n"] = json!(n);
            if source == "diary" {
                // Desde la vista del diario la tarjeta va con la respuesta, ahí mismo.
                card["say"] = json!(outcome.say);
                card["said"] = json!(said);
                self.shown_card = Some(card);
            } else {
                self.present(card, &outcome, said);
            }
        }
        let shown = self.shown_card.clone();
        let row = self.answer(client, id, said, source, None, &handler, outcome);
        self.carded = false;
        // Si la portada aún no ha llegado, irá a la tarjeta apuntada en el diario.
        if let (Some(card), Some(row)) = (shown, row)
            && let Some((at, logged @ None)) = &mut self.cover
            && *at == n
        {
            *logged = Some((card, row));
        }
    }

    /// Lo que pide el cerebro: una orden, el contexto o el diario. Lo que
    /// contesta es texto para el modelo.
    pub fn tool(&mut self, name: &str, input: &Value, source: &str) -> String {
        let text = |key: &str| input.get(key).and_then(Value::as_str).unwrap_or("").trim().to_string();
        let told = |o: Outcome| match o.answer {
            Some(body) => format!("{}\n{body}", o.say),
            None => o.say,
        };
        match name {
            "veyl" => {
                let order = text("orden");
                match orders::plan(self, &order) {
                    Some(plan) => told(self.perform(&plan, source, false)),
                    None => format!("Veyl no entiende «{order}» como una orden suya."),
                }
            }
            "contexto" => self.context().to_string(),
            "mostrar" => match crate::card::clean(input) {
                Some(card) => {
                    // De la pregunta al cerebro en marcha más reciente.
                    let n = self.asking.keys().max().copied().unwrap_or(0);
                    // Lo que se le olvidó (las preguntas, los enlaces) se añade a la que ya había.
                    let card = self.card.as_ref().and_then(|old| crate::card::merge(old, &card)).unwrap_or(card);
                    // Sin portada, se busca en los tres primeros enlaces a la vez: vale la primera que llegue.
                    self.cover = None;
                    let pages = crate::card::links(&card);
                    if card["imagen"] == "" && !pages.is_empty() && n > 0 {
                        self.cover = Some((n, None));
                        for page in pages.into_iter().take(3) {
                            let tx = self.tx.clone();
                            thread::spawn(move || {
                                if let Some(url) = crate::card::picture(&page) {
                                    let _ = tx.send(Event::Cover { n, url });
                                }
                            });
                        }
                    }
                    // Se enseña ya, sin esperar a que el modelo acabe de escribir:
                    // al contestar se completa (la frase, la portada) sin rehacerse.
                    if source != "diary"
                        && let Some((_, said)) = self.asking.get(&n)
                        && self.clients.values().any(|c| c.shell)
                    {
                        let mut live = card.clone();
                        live["n"] = json!(n);
                        live["said"] = json!(said);
                        live["live"] = json!(true);
                        self.broadcast(&json!({ "card": live }), |c| c.shell);
                    }
                    self.card = Some(card);
                    "Tarjeta lista: el usuario ya la ve. Ahora contesta solo con tu frase corta.".into()
                }
                // La de antes, si la hay, se queda: no se tapa con una vacía.
                None if self.card.is_some() => "Esa tarjeta no tiene ninguna pieza que Veyl sepa pintar: se queda la que ya enseñaste. No la vuelvas a mandar; contesta con tu frase corta.".into(),
                None => "Esa tarjeta no tiene ninguna pieza que Veyl sepa pintar: no se enseña. Mándala otra vez con sus `bloques`.".into(),
            },
            "diario" => {
                let when = text("cuando");
                told(match text("que").as_str() {
                    "detalle" => orders::report(self, &when),
                    "rato" => orders::timeline(self, &when),
                    "indice" => orders::index(self, &when),
                    "buscar" => orders::find(self, &text("texto")),
                    _ => orders::recall(self, &when),
                })
            }
            other => format!("No hay ninguna herramienta «{other}»"),
        }
    }

    /// Lo que hay ahora en el escritorio, para el cerebro.
    pub(super) fn context(&self) -> Value {
        let level = |workspace: i64| if self.split > 0 && workspace > 0 { (workspace - 1) % self.split as i64 + 1 } else { workspace };
        let active = self.hypr.active();
        let windows: Vec<Value> = self
            .hypr
            .clients()
            .iter()
            .map(|w| json!({ "app": apps::label(&self.apps, &w.class), "titulo": w.title, "nivel": level(w.workspace_id), "foco": active.as_ref().is_some_and(|a| a.address == w.address) }))
            .collect();
        let agents: Vec<Value> = self.agents.iter().map(|a| json!({ "agente": a.label, "sesion": a.name, "carpeta": a.cwd, "estado": a.phase })).collect();
        let playing: Vec<Value> = self.playing.iter().map(|p| json!({ "reproductor": p.player, "titulo": p.title, "artista": p.artist })).collect();
        let today = chrono::Local::now().date_naive();
        let days = [today.format("%Y-%m-%d").to_string(), (today + chrono::Duration::days(1)).format("%Y-%m-%d").to_string()];
        let agenda: Vec<Value> =
            calendars::agenda().into_iter().filter(|e| e.get("day").and_then(Value::as_str).is_some_and(|d| days.iter().any(|x| x == d))).collect();
        let left = (self.shell.timer - now()) / 1000;
        json!({
            "ahora": chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
            "ventana_con_foco": active.map(|w| json!({ "app": apps::label(&self.apps, &w.class), "titulo": w.title, "nivel": level(w.workspace_id) })),
            "ventanas": windows,
            "agentes": agents,
            "suena": playing,
            "temporizador_segundos": if left > 0 { json!(left) } else { Value::Null },
            "no_molestar": self.shell.quiet,
            "agenda_hoy_y_manana": agenda,
        })
    }

    /// Escucha una frase.
    pub(super) fn listen(&mut self, demo: bool, hold: bool) {
        // Si estaba hablando, se calla: le estás hablando tú.
        if let Some(mouth) = &self.mouth {
            let _ = mouth.send(tts::Say::Hush);
            // Y si va a contestar en voz alta, que la voz se vaya cargando.
            if self.settings.speak {
                let _ = mouth.send(tts::Say::Warm(self.settings.style()));
            }
        }
        let prompt = self.prompt();
        let (model, mic) = (self.settings.voice_model.clone(), self.settings.voice_mic.clone());
        if let Some(ear) = &self.ear {
            self.listening = ear.send(voice::Order::Listen { demo, hold, prompt, model, mic }).is_ok();
        }
    }

    pub(super) fn hush(&mut self) {
        self.listening = false;
        if let Some(ear) = &self.ear {
            let _ = ear.send(voice::Order::Stop);
        }
    }

    /// Lo que se le dice a Whisper que puede oír: cómo suenan las órdenes y
    /// cómo se escriben las aplicaciones que hay abiertas, para que acierte
    /// con «kitty» o «Hyprland».
    pub(super) fn prompt(&self) -> String {
        let mut names: Vec<String> = Vec::new();
        for w in self.hypr.clients() {
            let name = apps::label(&self.apps, &w.class);
            if !name.is_empty() && !names.contains(&name) {
                names.push(name);
            }
        }
        for agent in &self.agents {
            if !names.contains(&agent.label) {
                names.push(agent.label.clone());
            }
        }
        names.truncate(20);
        // Y las palabras del vocabulario, empezando por «Veyl».
        for word in crate::vocab::Vocabulary::load().hints() {
            if names.len() < 30 && !names.contains(&word) {
                names.push(word);
            }
        }
        let mut prompt = voice::PROMPT.to_string();
        if !names.is_empty() {
            prompt += &format!(" {}.", names.join(", "));
        }
        prompt
    }

    pub(super) fn voice(&mut self, voice: Voice) {
        match voice {
            Voice::Level(level) => {
                let message = json!({ "voice": { "level": (level * 1000.0).round() / 1000.0 } });
                self.broadcast(&message, |c| c.shell);
            }
            Voice::Speaking => {
                self.broadcast(&json!({ "voice": { "speaking": true } }), |c| c.shell);
            }
            Voice::Heard(text) => {
                // Lo que Whisper oye mal («qué puede hacer bale»), con su palabra.
                let text = crate::vocab::Vocabulary::load().fix(&text);
                self.listening = false;
                self.broadcast(&json!({ "voice": { "heard": text } }), |c| c.shell);
                self.call("listen.set", json!([false]));
                self.say(0, Value::Null, &text, "voice", false);
            }
            Voice::Done(why) => {
                self.listening = false;
                self.call("listen.set", json!([false]));
                if !why.is_empty() {
                    notify(&Outcome::fail(why));
                }
            }
        }
    }
}

/// Pregunta al modelo; dos minutos como mucho.
fn model(command: &str, said: &str, source: &str) -> Outcome {
    let output = crate::process::output(Command::new("sh").args(["-c", command]).env("VEYL_SOURCE", source), said.as_bytes(), Duration::from_secs(120));
    match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let (say, rest) = text.split_once('\n').map(|(a, b)| (a.to_string(), Some(b.trim().to_string()))).unwrap_or((text, None));
            if say.is_empty() { Outcome::fail("El modelo no dijo nada") } else { Outcome { ok: true, say, answer: rest.filter(|r| !r.is_empty()) } }
        }
        _ => Outcome::fail("El modelo no contestó"),
    }
}

/// Una notificación de Veyl (que es el servidor), si hay notify-send.
fn notify(outcome: &Outcome) {
    let body = outcome.answer.clone().unwrap_or_default();
    let time = if body.len() > 80 { "15000" } else { "5000" };
    let urgency = if outcome.ok { "low" } else { "normal" };
    orders::detach(&["notify-send", "-a", "Veyl", "-u", urgency, "-t", time, &outcome.say, &body]);
}
