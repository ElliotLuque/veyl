//! La voz: mientras Veyl escucha, veyld abre el micro (`pw-record`), le
//! cuenta a Veyl cuánto se oye (para el halo), nota con Silero cuándo
//! empiezas a hablar y cuándo acabas, y lo dicho lo transcribe Whisper
//! (whisper.cpp, en local). Lo que entiende vuelve al daemon como una orden
//! dicha (`source: "voice"`), que pasa por la gramática como cualquier otra.
//!
//! Se escucha una frase por vez. Con la tecla mantenida (`hold`), hasta que
//! se suelta (`Finish`); si no, hasta que acabas de hablar o, si en un rato
//! no se oye nada, hasta entonces. Luego el micro se cierra. Los modelos (`ggml-*.bin`) viven en
//! `$XDG_DATA_HOME/veyl/models`; los trae `veyld model get`. Se cargan al
//! empezar a escuchar y se sueltan tras un rato sin usarlos.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperVadContext, WhisperVadContextParams};

use crate::daemon::Event;

pub const RATE: usize = 16_000;
/// Cada cuánto se le dice a Veyl cuánto se oye: 50 ms.
const TICK: usize = RATE / 20;
/// El nivel, en dBFS: por debajo, silencio; por encima, voz plena.
const FLOOR: f32 = -50.0;
const CEIL: f32 = -15.0;
/// Cuánto queda del nivel anterior en cada tick: sube al momento y cae despacio.
const FALL: f32 = 0.9;
/// Silero mira de 512 en 512 muestras (32 ms).
const FRAME: usize = 512;
/// Lo que se le da a Silero cada vez: la última segunda y media.
const WINDOW: usize = RATE * 3 / 2;
/// Cada cuánto se mira si hay voz.
const STEP: usize = RATE / 4;
/// Lo que tiene que durar el silencio para que la frase acabe.
const SILENCE: usize = RATE * 4 / 5;
/// Lo que se guarda antes y después de la voz, para no comerse sílabas.
const PAD: usize = RATE * 3 / 10;
/// Sin oír nada en este rato, se deja de escuchar.
const PATIENCE: usize = RATE * 8;
/// Una frase no dura más que esto (con la tecla mantenida, el doble).
const LONGEST: usize = RATE * 15;
/// Al soltar la tecla, lo que se sigue oyendo: lo que aún venía por el micro.
const TAIL: usize = RATE * 3 / 10;
/// Los modelos se sueltan tras este rato sin usarlos.
const KEEP: Duration = Duration::from_secs(300);
/// Cómo suenan las órdenes, para que Whisper las espere.
pub const PROMPT: &str = "Sube el volumen a 85. Cambia a Firefox. Ve al nivel 2. Pon un temporizador de 10 minutos. ¿Qué estuve mirando ayer?";
/// El modelo de Silero que entiende whisper.cpp.
pub const VAD: &str = "silero-v5.1.2";

/// Lo que se le pide.
pub enum Order {
    /// Escuchar una frase. `prompt`, palabras que se esperan (nombres de
    /// aplicaciones, órdenes) para que Whisper acierte con ellas.
    /// `hold`: con la tecla mantenida; el silencio no acaba la frase, la acaba `Finish`.
    Listen {
        demo: bool,
        hold: bool,
        prompt: String,
        model: String,
        /// El micro (un nodo de PipeWire); vacío, el de por defecto.
        mic: String,
    },
    /// Se suelta la tecla: lo oído hasta ahora es la frase.
    Finish,
    Stop,
}

/// Lo que cuenta.
#[derive(Debug)]
pub enum Voice {
    /// Cuánto se oye, de 0 a 1.
    Level(f32),
    /// Ha empezado a hablar.
    Speaking,
    /// Lo dicho, ya en texto.
    Heard(String),
    /// Deja de escuchar sin nada que hacer, y por qué (vacío: no se oyó nada).
    Done(String),
}

enum Input {
    Order(Order),
    Audio(Vec<i16>),
    Closed,
}

pub fn models() -> PathBuf {
    let data = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| crate::config::home().join(".local/share"));
    data.join("veyl/models")
}

pub fn model_path(name: &str) -> PathBuf {
    models().join(format!("ggml-{name}.bin"))
}

/// Si veyld se compiló para la GPU (CUDA o Vulkan).
pub const GPU: bool = cfg!(any(feature = "cuda", feature = "vulkan"));

/// El modelo que se usa: el pedido o, si no, el mejor de los que hay para
/// esta máquina. Con GPU, el grande; sin ella, `small` va de sobra.
pub fn choose(wanted: &str) -> Option<PathBuf> {
    if !wanted.is_empty() {
        let path = if wanted.contains('/') { PathBuf::from(wanted) } else { model_path(wanted) };
        return path.exists().then_some(path);
    }
    let gpu = ["large-v3-turbo-q5_0", "large-v3-turbo-q8_0", "large-v3-turbo", "medium-q5_0", "small-q5_1", "small", "base-q5_1", "base", "tiny"];
    let cpu = ["small-q5_1", "small", "base-q5_1", "base", "large-v3-turbo-q5_0", "medium-q5_0", "tiny-q5_1", "tiny"];
    (if GPU { &gpu[..] } else { &cpu[..] }).iter().map(|n| model_path(n)).find(|p| p.exists())
}

/// Lo que se oye, de 0 a 1, de un trozo de audio.
fn loudness(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f64 = samples.iter().map(|&s| (s as f64) * (s as f64)).sum();
    let rms = (sum / samples.len() as f64).sqrt() / 32768.0;
    let db = if rms > 0.0 { 20.0 * rms.log10() as f32 } else { FLOOR };
    ((db - FLOOR) / (CEIL - FLOOR)).clamp(0.0, 1.0)
}

/// El hilo de la voz: recibe órdenes y manda lo que oye.
pub fn spawn(tx: Sender<Event>) -> Sender<Order> {
    let (orders, inbox) = mpsc::channel::<Input>();
    let (out, forward) = mpsc::channel::<Order>();
    {
        let orders = orders.clone();
        thread::spawn(move || {
            for order in forward {
                if orders.send(Input::Order(order)).is_err() {
                    return;
                }
            }
        });
    }
    thread::spawn(move || Ear::new(tx, orders).run(inbox));
    out
}

/// Los modelos cargados.
struct Brain {
    path: PathBuf,
    whisper: WhisperContext,
    vad: Option<WhisperVadContext>,
    used: Instant,
}

struct Ear {
    tx: Sender<Event>,
    /// Para que el hilo que lee el micro mande el audio.
    inbox: Sender<Input>,
    brain: Option<Brain>,
    /// Lo que escucha ahora: el micro (o la voz inventada) y lo oído.
    child: Option<Child>,
    demo: Option<Sender<()>>,
    heard: Vec<f32>,
    checked: usize,
    first: Option<usize>,
    last: usize,
    prompt: String,
    model: String,
    hold: bool,
    level: f32,
    /// Si el micro ya da sonido (no solo ceros).
    ready: bool,
    /// Se soltó la tecla: la frase acaba cuando se haya oído hasta aquí.
    closing: Option<usize>,
}

impl Ear {
    fn new(tx: Sender<Event>, inbox: Sender<Input>) -> Ear {
        whisper_rs::install_logging_hooks();
        Ear {
            tx,
            inbox,
            brain: None,
            child: None,
            demo: None,
            heard: Vec::new(),
            checked: 0,
            first: None,
            last: 0,
            prompt: String::new(),
            model: String::new(),
            hold: false,
            level: 0.0,
            ready: false,
            closing: None,
        }
    }

    fn tell(&self, voice: Voice) {
        let _ = self.tx.send(Event::Voice(voice));
    }

    fn run(mut self, inbox: Receiver<Input>) {
        loop {
            let input = match inbox.recv_timeout(Duration::from_secs(60)) {
                Ok(input) => input,
                Err(RecvTimeoutError::Timeout) => {
                    if self.child.is_none() && self.brain.as_ref().is_some_and(|b| b.used.elapsed() > KEEP) {
                        self.brain = None;
                    }
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => return,
            };
            match input {
                Input::Order(Order::Listen { demo, hold, prompt, model, mic }) => self.listen(demo, hold, prompt, model, &mic),
                Input::Order(Order::Finish) => self.finish(),
                Input::Order(Order::Stop) => self.stop(),
                Input::Audio(samples) if self.child.is_some() => self.hear(&samples),
                Input::Audio(_) => {}
                Input::Closed if self.child.is_some() && self.closing.is_some() => self.end(),
                Input::Closed if self.child.is_some() => {
                    self.stop();
                    self.tell(Voice::Done("El micro se ha cerrado".into()));
                }
                Input::Closed => {}
            }
        }
    }

    fn listen(&mut self, demo: bool, hold: bool, prompt: String, model: String, mic: &str) {
        self.stop();
        if demo {
            let (stop, wait) = mpsc::channel();
            let tx = self.tx.clone();
            thread::spawn(move || pretend(tx, wait));
            self.demo = Some(stop);
            return;
        }
        // El micro; VEYLD_RECORD lo cambia por otra orden que dé lo mismo (s16, 16 kHz, mono), para probar.
        let command = std::env::var("VEYLD_RECORD").ok().filter(|c| !c.is_empty());
        let mut cmd = match &command {
            Some(c) => {
                let mut cmd = Command::new("sh");
                cmd.args(["-c", c]);
                cmd
            }
            None => {
                let mut cmd = Command::new("pw-record");
                if !mic.is_empty() {
                    cmd.args(["--target", mic]);
                }
                cmd.args(["--rate", "16000", "--channels", "1", "--format", "s16", "-"]);
                cmd
            }
        };
        let mut child = match cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn() {
            Ok(child) => child,
            Err(e) => {
                self.tell(Voice::Done(format!("No se puede abrir el micro: {e}")));
                return;
            }
        };
        let Some(mut stdout) = child.stdout.take() else {
            let _ = child.kill();
            return;
        };
        let inbox = self.inbox.clone();
        thread::spawn(move || {
            let mut bytes = vec![0u8; TICK * 2];
            loop {
                if stdout.read_exact(&mut bytes).is_err() {
                    let _ = inbox.send(Input::Closed);
                    return;
                }
                let samples = bytes.as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b)).collect();
                if inbox.send(Input::Audio(samples)).is_err() {
                    return;
                }
            }
        });
        self.child = Some(child);
        (self.heard, self.checked, self.first, self.last, self.level) = (Vec::new(), 0, None, 0, 0.0);
        (self.ready, self.closing) = (false, None);
        (self.prompt, self.model, self.hold) = (prompt, model, hold);
        // Se carga mientras hablas: el audio espera en la cola.
        if let Err(why) = self.wake() {
            self.stop();
            self.tell(Voice::Done(why));
        }
    }

    fn stop(&mut self) {
        self.closing = None;
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(demo) = self.demo.take() {
            let _ = demo.send(());
        }
        self.heard.clear();
    }

    /// Carga los modelos, si no lo están ya.
    fn wake(&mut self) -> Result<(), String> {
        let Some(path) = choose(&self.model) else {
            return Err(if self.model.is_empty() {
                "Falta el modelo de voz: veyl voice get small-q5_1".to_string()
            } else {
                format!("No está el modelo de voz «{}»", self.model)
            });
        };
        if self.brain.as_ref().is_some_and(|b| b.path == path) {
            return Ok(());
        }
        self.brain = None;
        let mut params = WhisperContextParameters::default();
        params.use_gpu(GPU);
        let whisper = WhisperContext::new_with_params(&path, params).map_err(|e| format!("El modelo de voz no se carga: {e}"))?;
        let vad = Some(model_path(VAD)).filter(|p| p.exists()).and_then(|p| {
            let mut params = WhisperVadContextParams::new();
            params.set_n_threads(2);
            params.set_use_gpu(false);
            WhisperVadContext::new(&p.to_string_lossy(), params).ok()
        });
        self.brain = Some(Brain { path, whisper, vad, used: Instant::now() });
        Ok(())
    }

    fn hear(&mut self, samples: &[i16]) {
        // Mientras el micro despierta da ceros: eso no es silencio, es nada.
        // Un micro Bluetooth da ceros mientras despierta: eso no es
        // silencio, es nada, y no se guarda.
        if !self.ready {
            if samples.iter().filter(|&&s| s == 0).count() * 2 > samples.len() {
                return;
            }
            self.ready = true;
        }
        self.level = loudness(samples).max(self.level * FALL);
        self.tell(Voice::Level(self.level));
        self.heard.extend(samples.iter().map(|&s| s as f32 / 32768.0));
        if let Some(until) = self.closing {
            if self.heard.len() >= until {
                self.find_voice();
                self.end();
            }
            return;
        }
        if self.heard.len() - self.checked < STEP {
            return;
        }
        self.checked = self.heard.len();
        let was = self.first.is_some();
        self.find_voice();
        if !was && self.first.is_some() {
            self.tell(Voice::Speaking);
        }
        let now = self.heard.len();
        let longest = if self.hold { LONGEST * 2 } else { LONGEST };
        let over = match self.first {
            _ if now >= longest => true,
            // Con la tecla mantenida, se espera a que se suelte.
            _ if self.hold => false,
            None => now >= PATIENCE,
            Some(_) => now - self.last >= SILENCE,
        };
        if over {
            self.end();
        }
    }

    /// Se suelta la tecla: lo que se haya oído es la frase.
    fn finish(&mut self) {
        if self.child.is_none() || self.closing.is_some() {
            return;
        }
        if !self.ready {
            // Se soltó antes de que el micro diera nada.
            self.stop();
            self.tell(Voice::Done(String::new()));
            return;
        }
        self.closing = Some(self.heard.len() + TAIL);
    }

    /// La frase ha acabado: se cierra el micro y se transcribe lo dicho.
    fn end(&mut self) {
        let now = self.heard.len();
        // Con la tecla, lo dicho es todo lo que se oyó mientras estaba pulsada
        // (desde un poco antes de la voz, si se notó): el detector no recorta
        // nada. Sin ella, de la voz a la voz.
        let range = match self.first {
            Some(first) if self.hold => Some((first.saturating_sub(PAD * 2), now)),
            Some(first) => Some((first.saturating_sub(PAD), (self.last + PAD).min(now))),
            None if self.hold && loudness(&self.heard.iter().map(|s| (s * 32768.0) as i16).collect::<Vec<_>>()) > 0.3 => Some((0, now)),
            None => None,
        };
        let Some((from, to)) = range else {
            self.stop();
            self.tell(Voice::Done(String::new()));
            return;
        };
        let audio: Vec<f32> = self.heard[from..to].to_vec();
        keep(&audio);
        // El micro se cierra antes de pensar: ya no hace falta.
        self.stop();
        match self.transcribe(&audio) {
            Ok(text) if !text.is_empty() => self.tell(Voice::Heard(text)),
            Ok(_) => self.tell(Voice::Done(String::new())),
            Err(why) => self.tell(Voice::Done(why)),
        }
    }

    /// Dónde hay voz en lo último que se oyó: con Silero si está su modelo
    /// y, si no, por lo fuerte que suena.
    fn find_voice(&mut self) {
        let now = self.heard.len();
        let start = now.saturating_sub(WINDOW) / FRAME * FRAME;
        let window = &self.heard[start..now];
        let mut vad = self.brain.as_mut().and_then(|b| b.vad.as_mut());
        let silero = vad.as_mut().map(|vad| vad.detect_speech(window).is_ok()).unwrap_or(false);
        let frames: Vec<bool> = match vad {
            Some(vad) if silero => vad.probabilities().iter().map(|p| *p > 0.5).collect(),
            _ => window
                .chunks(FRAME)
                .map(|f| {
                    let samples: Vec<i16> = f.iter().map(|s| (s * 32768.0) as i16).collect();
                    loudness(&samples) > 0.35
                })
                .collect(),
        };
        for (i, voice) in frames.iter().enumerate() {
            if *voice {
                let at = start + i * FRAME;
                self.first = Some(self.first.map_or(at, |f| f.min(at)));
                self.last = self.last.max((at + FRAME).min(now));
            }
        }
    }

    fn transcribe(&mut self, audio: &[f32]) -> Result<String, String> {
        self.wake()?;
        let prompt = self.prompt.clone();
        let brain = self.brain.as_mut().ok_or("Sin modelo de voz")?;
        brain.used = Instant::now();
        transcribe(&brain.whisper, audio, &prompt, trims(&brain.path))
    }
}

/// Si al modelo se le puede recortar la ventana de audio: a los pequeños sí;
/// los grandes (`large-v3-turbo` sobre todo) se repiten y se inventan cosas.
pub fn trims(model: &Path) -> bool {
    let name = model.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    !name.contains("large") && !name.contains("medium")
}

/// La última frase oída, en `$XDG_RUNTIME_DIR/veyl/voz.wav` (en memoria y
/// solo tuya): para saber qué oyó de verdad cuando entiende mal
/// (`veyld hear` la vuelve a transcribir).
fn keep(audio: &[f32]) {
    let path = crate::config::runtime().join("veyl/voz.wav");
    let mut bytes = Vec::with_capacity(44 + audio.len() * 2);
    let data = (audio.len() * 2) as u32;
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&(RATE as u32).to_le_bytes());
    bytes.extend_from_slice(&(RATE as u32 * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data.to_le_bytes());
    for s in audio {
        bytes.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    use std::os::unix::fs::OpenOptionsExt;
    let written =
        std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&path).and_then(|mut f| std::io::Write::write_all(&mut f, &bytes));
    if let Err(e) = written {
        eprintln!("veyld: no se guarda la última frase ({}): {e}", path.display());
    }
}

/// Lo dicho en `audio` (16 kHz, mono), en español. `trim`: mirar solo lo
/// que dura (ver `trims`).
pub fn transcribe(whisper: &WhisperContext, audio: &[f32], prompt: &str, trim: bool) -> Result<String, String> {
    let mut state = whisper.create_state().map_err(|e| e.to_string())?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some("es"));
    params.set_n_threads(thread::available_parallelism().map(|n| n.get().min(8)).unwrap_or(4) as i32);
    params.set_no_context(true);
    params.set_single_segment(true);
    params.set_no_timestamps(true);
    params.set_suppress_blank(true);
    params.set_suppress_nst(true);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_special(false);
    params.set_print_timestamps(false);
    if !prompt.is_empty() {
        params.set_initial_prompt(prompt);
    }
    // Whisper quiere al menos un segundo: lo que falte, silencio.
    let mut audio = audio.to_vec();
    if audio.len() < RATE + RATE / 10 {
        audio.resize(RATE + RATE / 10, 0.0);
    }
    // Mira solo lo que dura lo dicho (50 por segundo, y algo de margen), no
    // los 30 s de siempre: con una orden corta es varias veces más rápido.
    if trim {
        params.set_audio_ctx(((audio.len() * 50 / RATE) as i32 + 128).min(1500));
    }
    // Una orden no pasa de unas pocas palabras: así no se embala repitiendo.
    params.set_max_tokens(64);
    state.full(params, &audio).map_err(|e| e.to_string())?;
    let mut text = String::new();
    for segment in state.as_iter() {
        if segment.no_speech_probability() > 0.6 {
            continue;
        }
        if let Ok(part) = segment.to_str_lossy() {
            text.push_str(&part);
        }
    }
    Ok(clean(&text))
}

/// Lo que Whisper se inventa cuando no hay nada que oír.
const GHOSTS: &[&str] = &[
    "subtitulos realizados por la comunidad de amara.org",
    "subtitulos por la comunidad de amara.org",
    "gracias por ver el video",
    "gracias por ver",
    "suscribete",
    "musica",
    "aplausos",
];

/// Whisper a veces se queda en bucle («Abajo. Abajo. Abajo», «apagar
/// apagar apagar»): las frases y las palabras repetidas seguidas, una vez.
fn unrepeat(text: &str) -> String {
    let fold = crate::words::fold;
    let mut sentences: Vec<&str> = Vec::new();
    let mut start = 0;
    for (i, c) in text.char_indices() {
        let next = text[i + c.len_utf8()..].chars().next();
        if matches!(c, '.' | '!' | '?' | ',') && next.is_none_or(char::is_whitespace) {
            sentences.push(&text[start..i + c.len_utf8()]);
            start = i + c.len_utf8();
        }
    }
    sentences.push(&text[start..]);
    let key = |s: &str| fold(s).chars().filter(|c| c.is_alphanumeric() || *c == ' ').collect::<String>().trim().to_string();
    let mut kept: Vec<&str> = Vec::new();
    for sentence in sentences.into_iter().filter(|s| !key(s).is_empty()) {
        if kept.last().is_none_or(|last| key(last) != key(sentence)) {
            kept.push(sentence);
        }
    }
    let joined = kept.iter().map(|s| s.trim()).collect::<Vec<_>>().join(" ");
    // Tres veces o más la misma palabra seguida es un bucle; dos puede ser a propósito.
    let words: Vec<&str> = joined.split_whitespace().collect();
    let mut out: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let same = words[i..].iter().take_while(|w| key(w) == key(words[i])).count();
        out.push(words[i]);
        i += if same >= 3 { same } else { 1 };
    }
    out.join(" ")
}

/// Lo transcrito, sin lo que no es habla («[Música]», «...») ni lo inventado.
pub fn clean(text: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in text.chars() {
        match c {
            '[' | '(' => depth += 1,
            ']' | ')' => depth = (depth - 1).max(0),
            '*' | '♪' => {}
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    let out = unrepeat(&out.split_whitespace().collect::<Vec<_>>().join(" "));
    let out = out.trim_matches(|c: char| c.is_whitespace() || ".,¡!¿?…-".contains(c)).to_string();
    let folded = crate::words::fold(&out);
    let bare = folded.trim_matches(|c: char| !c.is_alphanumeric());
    if bare.is_empty() || GHOSTS.contains(&bare) { String::new() } else { out }
}

/// Sin micro: una voz inventada, sílabas y pausas, para ver el halo.
fn pretend(tx: Sender<Event>, stop: Receiver<()>) {
    let mut seed: u64 = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1) | 1;
    let mut random = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % 10_000) as f32 / 10_000.0
    };
    let mut level = 0.0f32;
    let tick = |level: f32| tx.send(Event::Voice(Voice::Level(level))).is_ok() && stop.recv_timeout(Duration::from_millis(50)).is_err();
    loop {
        for _ in 0..3 + (random() * 7.0) as usize {
            let peak = 0.45 + random() * 0.55;
            for _ in 0..2 + (random() * 4.0) as usize {
                level = (peak * (0.75 + random() * 0.25)).max(level * FALL);
                if !tick(level) {
                    return;
                }
            }
            for _ in 0..(random() * 3.0) as usize {
                level *= FALL;
                if !tick(level) {
                    return;
                }
            }
        }
        for _ in 0..8 + (random() * 18.0) as usize {
            level *= FALL;
            if !tick(level) {
                return;
            }
        }
    }
}

/// Un WAV PCM de 16 bits, en mono a 16 kHz (mezcla los canales y cambia la frecuencia).
pub fn wav(path: &Path) -> Result<Vec<f32>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("no es un WAV".into());
    }
    let (mut at, mut channels, mut rate, mut bits, mut data) = (12, 0usize, 0usize, 0u16, None);
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = u32::from_le_bytes([bytes[at + 4], bytes[at + 5], bytes[at + 6], bytes[at + 7]]) as usize;
        let body = &bytes[at + 8..(at + 8 + size).min(bytes.len())];
        if id == b"fmt " && body.len() >= 16 {
            channels = u16::from_le_bytes([body[2], body[3]]) as usize;
            rate = u32::from_le_bytes([body[4], body[5], body[6], body[7]]) as usize;
            bits = u16::from_le_bytes([body[14], body[15]]);
        } else if id == b"data" {
            data = Some(body);
        }
        at += 8 + size + (size & 1);
    }
    let data = data.ok_or("WAV sin datos")?;
    if bits != 16 || channels == 0 || rate == 0 {
        return Err("solo WAV PCM de 16 bits".into());
    }
    let mono: Vec<f32> = data
        .chunks_exact(2 * channels)
        .map(|frame| frame.as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b) as f32 / 32768.0).sum::<f32>() / channels as f32)
        .collect();
    if rate == RATE {
        return Ok(mono);
    }
    let length = mono.len() * RATE / rate;
    Ok((0..length)
        .map(|i| {
            let x = i as f64 * rate as f64 / RATE as f64;
            let (j, frac) = (x as usize, (x - x.floor()) as f32);
            let a = mono[j.min(mono.len() - 1)];
            let b = mono[(j + 1).min(mono.len() - 1)];
            a + (b - a) * frac
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleaning() {
        assert_eq!(clean(" Sube el volumen a 85."), "Sube el volumen a 85");
        assert_eq!(clean("[Música]"), "");
        assert_eq!(clean("(aplausos) Cambia a Firefox"), "Cambia a Firefox");
        assert_eq!(clean("Subtítulos realizados por la comunidad de Amara.org"), "");
        assert_eq!(clean("¿Qué hora es?"), "Qué hora es");
        assert_eq!(clean("..."), "");
        assert_eq!(clean("Abajo. Abajo. Abajo. Abajo"), "Abajo");
        assert_eq!(clean("apagar apagar apagar apagar apagar apagar"), "apagar");
        assert_eq!(clean("Sube un nivel. Sube un nivel."), "Sube un nivel");
        assert_eq!(clean("Abre Spotify y muévelo al nivel 4"), "Abre Spotify y muévelo al nivel 4");
        assert_eq!(clean("baja baja"), "baja baja");
    }

    #[test]
    fn levels() {
        assert_eq!(loudness(&[0; 800]), 0.0);
        assert!(loudness(&[16000; 800]) > 0.9);
        assert!(loudness(&[30; 800]) < 0.1);
    }
}
