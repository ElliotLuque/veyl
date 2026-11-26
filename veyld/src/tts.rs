//! La voz de Veyl: lo que contesta a lo que se le dice por voz, en voz alta.
//!
//! Con XTTS-v2 instalado (`speech/install.sh`), lo dice un servidor que se
//! queda cargado en la GPU (`speech/xtts.py`), con la voz «Damien Black» y un
//! tratamiento de robot de película. Si no, Piper (en local; las voces,
//! `piper-*.onnx`, viven con los modelos de `voice.rs`) con un filtro de robot
//! suave hecho aquí. Lo reproduce `pw-play`; volver a hablar a Veyl la calla.

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use crate::voice;

/// La voz de Piper que se usa si no se pide otra.
pub const VOICE: &str = "es_ES-davefx-medium";

/// La voz de XTTS de fábrica.
pub const SPEAKER: &str = "Damien Black";

/// Cómo se habla: con qué motor («xtts», «piper»; vacío, el mejor que haya),
/// qué voz, a qué velocidad (XTTS), cuánto robot (Piper) y a qué volumen.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub engine: String,
    pub voice: String,
    pub speed: f32,
    pub robot: f32,
    pub volume: f32,
    /// Minutos sin hablar tras los que se suelta XTTS (unos 2 GB de memoria de
    /// vídeo); 0, nunca: así contesta al momento.
    pub unload: u32,
}

/// Lo que se le pide.
pub enum Say {
    Text {
        text: String,
        style: Style,
    },
    /// Que se vaya cargando: alguien va a hablar a Veyl.
    Warm(Style),
    /// Callarse.
    Hush,
}

/// La raíz del repositorio en desarrollo, cuando se corre `veyld/target/release/veyld`.
fn dev_home() -> Option<PathBuf> {
    let home = std::env::current_exe().ok()?.parent()?.parent()?.parent()?.to_path_buf();
    (home.join("Cargo.toml").is_file() || home.join(".tools").is_dir() || home.join("speech").is_dir()).then_some(home)
}

/// Dónde viven los datos editables del usuario (`$XDG_DATA_HOME/veyl`).
fn data_dir() -> PathBuf {
    let data = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| crate::config::home().join(".local/share"));
    data.join("veyl")
}

/// Dónde está un binario en `$PATH`.
fn in_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|path| std::env::split_paths(&path).map(|d| d.join(name)).find(|p| p.is_file()))
}

/// Un archivo de `speech/` (`xtts.py`, `install.sh`): en el paquete de Nix
/// (`$VEYLD_SHARE/speech/` o `<bin>/../share/veyld/speech/`) o en el árbol de desarrollo.
pub fn speech_file(name: &str) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(share) = std::env::var_os("VEYLD_SHARE").filter(|v| !v.is_empty()) {
        candidates.push(PathBuf::from(share).join("speech").join(name));
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(prefix) = exe.parent().and_then(|bin| bin.parent())
    {
        candidates.push(prefix.join("share/veyld/speech").join(name));
    }
    if let Some(home) = dev_home() {
        candidates.push(home.join("speech").join(name));
    }
    candidates.into_iter().find(|p| p.is_file())
}

/// Carpeta editable con el entorno de XTTS (`~/.local/share/veyl/xtts`, o el
/// `veyld/.tools` clásico si ya estaba ahí).
fn xtts_tools() -> Option<PathBuf> {
    let user = data_dir().join("xtts");
    if user.join("venv/bin/python").is_file() || user.join("xtts/bin/python").is_file() {
        return Some(user);
    }
    dev_home().map(|h| h.join(".tools")).filter(|d| d.join("xtts/bin/python").is_file())
}

/// Lo que hace falta para XTTS, si está todo: python, el servidor, el modelo y sox.
fn xtts_parts() -> Option<(PathBuf, PathBuf, PathBuf, PathBuf)> {
    let tools = xtts_tools()?;
    let python = [tools.join("venv/bin/python"), tools.join("xtts/bin/python")].into_iter().find(|p| p.is_file())?;
    let script = speech_file("xtts.py")?;
    let model = voice::models().join("xtts-v2");
    let sox = in_path("sox").or_else(|| [tools.join("sox/bin/sox")].into_iter().find(|p| p.is_file()))?;
    model.join("model.pth").is_file().then_some((python, script, model, sox))
}

fn xtts_libs(tools: &std::path::Path) -> String {
    let mut dirs = Vec::new();
    if let Ok(extra) = std::env::var("VEYLD_CC_LIB")
        && !extra.is_empty()
    {
        dirs.push(extra);
    }
    let bundled = tools.join("cc-lib-lib/lib");
    if bundled.is_dir() {
        dirs.push(bundled.display().to_string());
    }
    dirs.push("/run/opengl-driver/lib".into());
    dirs.join(":")
}

/// Si se habla con XTTS.
fn uses_xtts(style: &Style) -> bool {
    style.engine != "piper" && xtts_parts().is_some()
}

/// El servidor de XTTS, cargado en la GPU.
struct Xtts {
    child: Child,
    stdin: ChildStdin,
    replies: Receiver<String>,
    voice: String,
    speed: f32,
    used: Instant,
    unload: u32,
}

/// Cuánto se espera a que cargue y a una frase.
const LOADING: Duration = Duration::from_secs(120);
const SAYING: Duration = Duration::from_secs(30);
/// Lo corto se guarda ya dicho («Nivel 2», «Hecho»): la próxima vez suena al momento.
const SHORT: usize = 80;
const KEPT: usize = 500;
/// La versión del tratamiento de cine de `speech/xtts.py`.
const CINEMA: &str = "cine-suave-1";

impl Xtts {
    fn start(style: &Style) -> Result<Xtts, String> {
        let (python, script, model, sox) = xtts_parts().ok_or("XTTS no está instalado (veyl voice get xtts)")?;
        let voice = if style.voice.is_empty() { SPEAKER.to_string() } else { style.voice.clone() };
        let libs = xtts_tools().map(|tools| xtts_libs(&tools)).unwrap_or_else(|| "/run/opengl-driver/lib".into());
        let mut child = Command::new(python)
            .arg("-I")
            .arg(script)
            .arg("--model")
            .arg(model)
            .args(["--speaker", &voice, "--speed", &format!("{:.2}", style.speed)])
            .arg("--sox")
            .arg(sox)
            .env("LD_LIBRARY_PATH", libs)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("XTTS: {e}"))?;
        let stdin = child.stdin.take().ok_or("XTTS sin entrada")?;
        let stdout = child.stdout.take().ok_or("XTTS sin salida")?;
        let (tx, replies) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    return;
                }
            }
        });
        let mut server = Xtts { child, stdin, replies, voice, speed: style.speed, used: Instant::now(), unload: style.unload };
        let ready = server.reply(LOADING)?;
        if ready.get("ready").and_then(serde_json::Value::as_bool) != Some(true) {
            return Err(format!("XTTS no arranca: {}", ready.get("error").and_then(serde_json::Value::as_str).unwrap_or("?")));
        }
        Ok(server)
    }

    fn reply(&mut self, wait: Duration) -> Result<serde_json::Value, String> {
        let line = self.replies.recv_timeout(wait).map_err(|_| "XTTS no contesta".to_string())?;
        serde_json::from_str(&line).map_err(|e| format!("XTTS: {e}"))
    }

    /// Lo dice a un WAV y devuelve dónde.
    fn say(&mut self, text: &str) -> Result<PathBuf, String> {
        self.used = Instant::now();
        let saved = (text.chars().count() <= SHORT).then(|| said_before(&self.voice, self.speed, text));
        if let Some(path) = saved.as_ref().filter(|p| p.is_file()) {
            return Ok(path.clone());
        }
        let out = saved.unwrap_or_else(|| crate::config::runtime().join("veyl/habla.wav"));
        let order = serde_json::json!({ "text": text, "out": out });
        writeln!(self.stdin, "{order}").and_then(|_| self.stdin.flush()).map_err(|e| format!("XTTS: {e}"))?;
        let reply = self.reply(SAYING)?;
        if reply.get("ok").and_then(serde_json::Value::as_bool) == Some(true) {
            Ok(out)
        } else {
            Err(format!("XTTS: {}", reply.get("error").and_then(serde_json::Value::as_str).unwrap_or("?")))
        }
    }

    fn fits(&self, style: &Style) -> bool {
        // (la velocidad y la voz cambian lo dicho; el volumen no)
        let voice = if style.voice.is_empty() { SPEAKER } else { style.voice.as_str() };
        self.voice == voice && (self.speed - style.speed).abs() < 0.01
    }
}

impl Drop for Xtts {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// El modelo de una voz y su configuración.
pub fn voice_path(name: &str) -> PathBuf {
    voice::models().join(format!("piper-{name}.onnx"))
}

/// La voz pedida o, si no, la de fábrica o cualquiera de las que haya.
pub fn choose(wanted: &str) -> Option<PathBuf> {
    if !wanted.is_empty() {
        let path = if wanted.contains('/') { PathBuf::from(wanted) } else { voice_path(wanted) };
        return path.exists().then_some(path);
    }
    let default = voice_path(VOICE);
    if default.exists() {
        return Some(default);
    }
    let mut found: Vec<PathBuf> = std::fs::read_dir(voice::models())
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("piper-")) && p.extension().is_some_and(|e| e == "onnx"))
        .collect();
    found.sort();
    found.into_iter().next()
}

/// La frecuencia a la que habla una voz (de su `.onnx.json`).
fn rate(model: &std::path::Path) -> u32 {
    let mut config = model.as_os_str().to_os_string();
    config.push(".json");
    std::fs::read_to_string(config)
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|j| j.pointer("/audio/sample_rate")?.as_u64())
        .unwrap_or(22050) as u32
}

/// Dónde está piper: en $PATH o junto a veyld (`build.sh` lo trae a `.tools/`).
fn piper() -> Option<PathBuf> {
    if let Some(found) = in_path("piper") {
        return Some(found);
    }
    let near = dev_home()?.join(".tools/piper/bin/piper");
    near.is_file().then_some(near)
}

/// Lo que dice Piper, sin filtro: muestras de -1 a 1 y su frecuencia.
/// `robot` (0 a 1) la hace también más rápida, porque luego se reproduce
/// más lenta y grave (`deep`).
pub fn synth(text: &str, model: &std::path::Path, robot: f32) -> Result<(Vec<f32>, u32), String> {
    let piper = piper().ok_or("No está piper (veyld/build.sh lo trae)")?;
    let mut cmd = Command::new(piper);
    // Poco ruido en el generador: menos expresiva, más de máquina. Más
    // rápida, porque luego se reproduce más lenta (`DEEPER`).
    let length = format!("{:.2}", 0.92 * (1.0 - (1.0 - DEEPER) * robot.clamp(0.0, 1.0)));
    cmd.arg("-m").arg(model).args(["--output-raw", "--length-scale", &length, "--noise-scale", "0.3", "--noise-w-scale", "0.3", "--sentence-silence", "0.15"]);
    let out = crate::process::output(&mut cmd, text.as_bytes(), Duration::from_secs(20)).map_err(|e| format!("piper: {e}"))?;
    if !out.status.success() {
        return Err("piper no ha podido decirlo".into());
    }
    let samples = out.stdout.as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b) as f32 / 32768.0).collect();
    Ok((samples, rate(model)))
}

/// Cuánto más grave: se reproduce a esta fracción de su frecuencia (unos
/// dos semitonos abajo). Piper habla más deprisa para que no se alargue.
pub const DEEPER: f32 = 0.89;

/// A qué frecuencia se reproduce lo que Piper dijo a `rate`: más lenta (y
/// grave) cuanto más robot.
pub fn deep(rate: u32, robot: f32) -> u32 {
    (rate as f32 * (1.0 - (1.0 - DEEPER) * robot.clamp(0.0, 1.0))).round() as u32
}

/// Lo que suena, a un volumen de 0 a 1.
pub fn loud(samples: &mut [f32], volume: f32) {
    let volume = volume.clamp(0.0, 1.0);
    samples.iter_mut().for_each(|s| *s *= volume);
}

/// El filtro de robot, suave: un resonador metálico ligero, un doble con
/// chorus lento y un paso bajo que quita los siseos. `amount` de 0 (la voz
/// tal cual) a 1.
pub fn robot(input: &[f32], rate: u32, amount: f32) -> Vec<f32> {
    let amount = amount.clamp(0.0, 1.0);
    if amount == 0.0 || input.is_empty() {
        return input.to_vec();
    }
    let r = rate as f32;
    let tau = std::f32::consts::TAU;
    // Un resonador de 7 ms (unos 140 Hz) con poca realimentación: el timbre de máquina, sin zumbido.
    let comb = (0.007 * r) as usize;
    let mut resonated = vec![0.0f32; input.len()];
    // Paso bajo de un polo a unos 4 kHz: lo áspero de arriba fuera.
    let cut = 1.0 - (-tau * 4000.0 / r).exp();
    let mut low = 0.0f32;
    let mut out = Vec::with_capacity(input.len());
    for (i, &x) in input.iter().enumerate() {
        let t = i as f32 / r;
        let fed = if i >= comb { resonated[i - comb] } else { 0.0 };
        resonated[i] = x + fed * 0.35 * amount;
        let mut y = x * (1.0 - 0.3 * amount) + resonated[i] * 0.3 * amount;
        // El doble, entre 9 y 13 ms detrás, moviéndose despacio (0,3 Hz).
        let lag = (0.011 + 0.002 * (tau * 0.3 * t).sin()) * r;
        let at = i as f32 - lag;
        if at >= 1.0 {
            let (j, frac) = (at as usize, at.fract());
            y += (input[j] * (1.0 - frac) + input[j + 1] * frac) * 0.35 * amount;
        }
        low += (y - low) * cut;
        out.push(y * (1.0 - amount) + low * amount);
    }
    // Tan fuerte como entró (por su energía, no por el pico), con un
    // recorte suave para que los picos no rompan.
    let rms = |v: &[f32]| (v.iter().map(|s| s * s).sum::<f32>() / v.len() as f32).sqrt().max(1e-6);
    let gain = (rms(input) / rms(&out)).min(4.0);
    out.iter_mut().for_each(|s| *s = (*s * gain).tanh() * 0.97);
    out
}

/// Lo reproduce por la salida de por defecto.
fn play(samples: &[f32], rate: u32) -> Option<Child> {
    let mut child = Command::new("pw-play")
        .args(["--raw", "--rate", &rate.to_string(), "--channels", "1", "--format", "s16", "--media-role", "Communication", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdin = child.stdin.take()?;
    let bytes: Vec<u8> = samples.iter().flat_map(|s| ((s * 32767.0) as i16).to_le_bytes()).collect();
    thread::spawn(move || {
        let _ = stdin.write_all(&bytes);
    });
    Some(child)
}

/// Dónde se guarda lo ya dicho con una voz, una velocidad y un texto.
fn said_before(voice: &str, speed: f32, text: &str) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    // Con el tratamiento en la clave: si cambia `speech/xtts.py`, lo guardado no vale.
    (CINEMA, voice, format!("{speed:.2}"), text.trim().to_lowercase()).hash(&mut hash);
    let dir = crate::config::cache().join("voz");
    let _ = std::fs::create_dir_all(&dir);
    dir.join(format!("{:016x}.wav", hash.finish()))
}

/// Que lo guardado no crezca sin fin: se quedan las `KEPT` más recientes.
fn tidy_said() {
    let dir = crate::config::cache().join("voz");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = entries.flatten().filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path()))).collect();
    if files.len() <= KEPT {
        return;
    }
    files.sort();
    for (_, path) in &files[..files.len() - KEPT] {
        let _ = std::fs::remove_file(path);
    }
}

/// Reproduce un WAV a un volumen de 0 a 1.
fn play_file(path: &std::path::Path, volume: f32) -> Option<Child> {
    Command::new("pw-play")
        .args(["--volume", &format!("{:.2}", volume.clamp(0.0, 1.0)), "--media-role", "Communication"])
        .arg(path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()
}

/// El hilo de la voz: dice lo que le llega y se calla si se lo piden.
pub fn spawn() -> Sender<Say> {
    let (tx, rx) = mpsc::channel::<Say>();
    thread::spawn(move || {
        let mut speaking: Option<Child> = None;
        let mut server: Option<Xtts> = None;
        let hush = |speaking: &mut Option<Child>| {
            if let Some(mut child) = speaking.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        };
        // El servidor de XTTS con este estilo, cargándolo si hace falta.
        let ready = |server: &mut Option<Xtts>, style: &Style| -> Result<(), String> {
            if server.as_ref().is_none_or(|s| !s.fits(style)) {
                *server = None;
                *server = Some(Xtts::start(style)?);
            }
            Ok(())
        };
        loop {
            let order = match rx.recv_timeout(Duration::from_secs(60)) {
                Ok(order) => order,
                Err(RecvTimeoutError::Timeout) => {
                    if let Some(s) = &server
                        && s.unload > 0
                        && s.used.elapsed() > Duration::from_secs(u64::from(s.unload) * 60)
                    {
                        server = None;
                    }
                    tidy_said();
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => return,
            };
            match order {
                Say::Hush => hush(&mut speaking),
                Say::Warm(style) => {
                    if uses_xtts(&style)
                        && let Err(why) = ready(&mut server, &style)
                    {
                        eprintln!("veyld: {why}");
                    }
                }
                Say::Text { text, style } => {
                    if uses_xtts(&style) {
                        match ready(&mut server, &style).and_then(|_| server.as_mut().map_or(Err("XTTS".into()), |s| s.say(&text))) {
                            Ok(wav) => {
                                hush(&mut speaking);
                                speaking = play_file(&wav, style.volume);
                                continue;
                            }
                            Err(why) => {
                                // Algo falla con XTTS: Piper, como siempre.
                                eprintln!("veyld: {why}");
                                server = None;
                            }
                        }
                    }
                    let Some(model) = choose(if style.engine == "piper" { &style.voice } else { "" }) else {
                        eprintln!("veyld: no hay voz para hablar (veyl voice get xtts, o voz-davefx)");
                        continue;
                    };
                    match synth(&text, &model, style.robot) {
                        Ok((samples, rate)) => {
                            hush(&mut speaking);
                            let mut samples = robot(&samples, rate, style.robot);
                            loud(&mut samples, style.volume);
                            speaking = play(&samples, deep(rate, style.robot));
                        }
                        Err(why) => eprintln!("veyld: {why}"),
                    }
                }
            }
        }
    });
    tx
}

/// Lo dice como lo diría Veyl, en un WAV (para `veyld speak`): con XTTS si
/// está y se quiere, si no con Piper. Sin el volumen: eso es del reproductor.
pub fn render(text: &str, style: &Style) -> Result<PathBuf, String> {
    if uses_xtts(style) {
        let mut server = Xtts::start(style)?;
        return server.say(text);
    }
    let model = choose(if style.engine == "piper" { &style.voice } else { "" }).ok_or("no hay voz: veyl voice get xtts")?;
    let (samples, rate) = synth(text, &model, style.robot)?;
    let samples = robot(&samples, rate, style.robot);
    let out = crate::config::runtime().join("veyl/habla.wav");
    write_wav(&out, &samples, deep(rate, style.robot))?;
    Ok(out)
}

/// Un WAV de 16 bits, mono.
pub fn write_wav(path: &std::path::Path, samples: &[f32], rate: u32) -> Result<(), String> {
    let data = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    std::fs::write(path, out).map_err(|e| format!("{}: {e}", path.display()))
}

/// Lo que se dice en voz alta de una respuesta: sin comillas ni lo que no se lee bien.
pub fn spoken(text: &str) -> String {
    // Lo que va entre paréntesis («(15:00–21:00)») se lee en la notificación, no se dice.
    let mut out = String::new();
    let mut depth = 0;
    for c in text.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth = (depth - 1).max(0),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    let out = out.replace(['«', '»', '"'], "").replace(" · ", ", ").replace('—', ",");
    out.split_whitespace().collect::<Vec<_>>().join(" ").replace(" :", ":").replace(" ,", ",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn robot_keeps_length_and_level() {
        let tone: Vec<f32> = (0..22050).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        let out = robot(&tone, 22050, 1.0);
        assert_eq!(out.len(), tone.len());
        let rms = |v: &[f32]| (v.iter().map(|s| s * s).sum::<f32>() / v.len() as f32).sqrt();
        assert!((rms(&out) / rms(&tone) - 1.0).abs() < 0.2, "{} {}", rms(&out), rms(&tone));
        assert!(out.iter().all(|s| s.abs() < 1.0));
        assert_eq!(robot(&tone, 22050, 0.0), tone);
    }

    #[test]
    fn spoken_text() {
        assert_eq!(spoken("Abro «Firefox» · nivel 2"), "Abro Firefox, nivel 2");
        assert_eq!(spoken("Ayer por la tarde (15:00–21:00): nada en el diario"), "Ayer por la tarde: nada en el diario");
    }
}
