//! Subprocesos con tiempo y salida limitados. Un descendiente que conserve
//! las tuberías abiertas tampoco puede bloquear al daemon indefinidamente.
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitStatus, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

const OUTPUT_LIMIT: u64 = 4 * 1024 * 1024;
/// Lo que puede escribir, como mucho, quien cuenta lo que hace línea a línea
/// (Claude Code con `stream-json` repite en ella lo que lee de la web).
const LINES_LIMIT: u64 = 32 * 1024 * 1024;

fn kill_group(pid: u32) {
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    // SAFETY: el hijo se lanzó en su propio grupo; SIGKILL no usa punteros.
    unsafe { kill(-(pid as i32), 9) };
}

fn read(pipe: impl Read) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    pipe.take(OUTPUT_LIMIT + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > OUTPUT_LIMIT {
        return Err(io::Error::other("la salida del proceso supera 4 MiB"));
    }
    Ok(bytes)
}

pub fn output(command: &mut Command, input: &[u8], timeout: Duration) -> io::Result<Output> {
    let mut child = command.process_group(0).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let pid = child.id();
    let input = input.to_vec();
    let mut stdin = child.stdin.take().expect("stdin canalizado");
    let stdout = child.stdout.take().expect("stdout canalizado");
    let stderr = child.stderr.take().expect("stderr canalizado");
    // Leer y escribir a la vez evita bloquear cuando el hijo escribe antes
    // de consumir toda la entrada.
    let writer = thread::spawn(move || stdin.write_all(&input));
    let out = thread::spawn(move || read(stdout));
    let err = thread::spawn(move || read(stderr));
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) if writer.is_finished() && out.is_finished() && err.is_finished() => break Ok(status),
            Ok(_) if start.elapsed() < timeout => thread::sleep(Duration::from_millis(10)),
            Ok(_) => break Err(io::Error::new(io::ErrorKind::TimedOut, "el proceso tardó demasiado")),
            Err(e) => break Err(e),
        }
    };
    // Si venció el plazo, incluye a los hijos que retienen las tuberías.
    // Si acabó bien, deja vivir a los procesos desacoplados (p. ej. wl-copy
    // debe seguir sirviendo el portapapeles después de salir la orden).
    if status.is_err() {
        kill_group(pid);
    }
    let _ = child.wait();
    let _ = writer.join();
    let stdout = out.join().map_err(|_| io::Error::other("falló la lectura de stdout"))?;
    let stderr = err.join().map_err(|_| io::Error::other("falló la lectura de stderr"))?;
    Ok(Output { status: status?, stdout: stdout?, stderr: stderr? })
}

/// Como `output`, pero da cada línea de la salida según llega (para enseñar lo
/// que va haciendo quien la escribe) y se puede parar desde fuera: con `stop`
/// a `true`, mata al proceso y a sus hijos y devuelve `Interrupted`. Devuelve
/// cómo acabó y lo que escribió por la salida de errores.
pub fn lines(command: &mut Command, timeout: Duration, stop: &AtomicBool, line: &mut dyn FnMut(&str)) -> io::Result<(ExitStatus, Vec<u8>)> {
    let mut child = command.process_group(0).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let pid = child.id();
    let stdout = child.stdout.take().expect("stdout canalizado");
    let stderr = child.stderr.take().expect("stderr canalizado");
    let (tx, rx) = mpsc::channel::<String>();
    let out = thread::spawn(move || {
        let mut reader = BufReader::new(stdout.take(LINES_LIMIT));
        let mut text = String::new();
        loop {
            text.clear();
            match reader.read_line(&mut text) {
                Ok(0) | Err(_) => break,
                Ok(_) if tx.send(text.trim_end().to_string()).is_err() => break,
                Ok(_) => {}
            }
        }
    });
    let err = thread::spawn(move || read(stderr));
    let start = Instant::now();
    let mut open = true;
    let ended = loop {
        if stop.load(Ordering::Relaxed) {
            break Err(io::Error::new(io::ErrorKind::Interrupted, "parado"));
        }
        if start.elapsed() >= timeout {
            break Err(io::Error::new(io::ErrorKind::TimedOut, "el proceso tardó demasiado"));
        }
        if open {
            match rx.recv_timeout(Duration::from_millis(20)) {
                Ok(text) => line(&text),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => open = false,
            }
            continue;
        }
        match child.try_wait() {
            Ok(Some(status)) if err.is_finished() => break Ok(status),
            Ok(_) => thread::sleep(Duration::from_millis(10)),
            Err(e) => break Err(e),
        }
    };
    if ended.is_err() {
        kill_group(pid);
    }
    let _ = child.wait();
    let _ = out.join();
    let stderr = err.join().map_err(|_| io::Error::other("falló la lectura de stderr"))?;
    Ok((ended?, stderr.unwrap_or_default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exchanges_input() {
        let out = output(Command::new("sh").args(["-c", "cat; printf error >&2"]), b"hola", Duration::from_secs(2)).unwrap();
        assert!(out.status.success());
        assert_eq!(out.stdout, b"hola");
        assert_eq!(out.stderr, b"error");
    }

    #[test]
    fn timeout_also_closes_descendant_pipes() {
        let start = Instant::now();
        let result = output(Command::new("sh").args(["-c", "sleep 30 & wait"]), b"", Duration::from_millis(50));
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn successful_commands_keep_detached_helpers_alive() {
        let marker = std::env::temp_dir().join(format!("veyld-detached-test-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        let result = output(
            Command::new("sh").args(["-c", "(sleep 0.1; printf done > \"$1\") >/dev/null 2>&1 < /dev/null &", "sh"]).arg(&marker),
            b"",
            Duration::from_secs(2),
        )
        .unwrap();
        assert!(result.status.success());
        let start = Instant::now();
        while !marker.exists() && start.elapsed() < Duration::from_secs(2) {
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(std::fs::read_to_string(&marker).unwrap(), "done");
        std::fs::remove_file(marker).unwrap();
    }

    #[test]
    fn lines_arrive_as_they_are_written() {
        let stop = AtomicBool::new(false);
        let mut seen = Vec::new();
        let (status, err) =
            lines(Command::new("sh").args(["-c", "echo uno; echo dos >&2; sleep 0.05; printf tres"]), Duration::from_secs(2), &stop, &mut |l| {
                seen.push(l.to_string())
            })
            .unwrap();
        assert!(status.success());
        assert_eq!(seen, ["uno", "tres"]);
        assert_eq!(err, b"dos\n");
    }

    #[test]
    fn lines_can_be_stopped_from_outside() {
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            flag.store(true, Ordering::Relaxed);
        });
        let start = Instant::now();
        let result = lines(Command::new("sh").args(["-c", "echo ya; sleep 30 & wait"]), Duration::from_secs(10), &stop, &mut |_| {});
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Interrupted);
        assert!(start.elapsed() < Duration::from_secs(2));
        let timed = lines(Command::new("sleep").arg("30"), Duration::from_millis(50), &AtomicBool::new(false), &mut |_| {});
        assert_eq!(timed.unwrap_err().kind(), io::ErrorKind::TimedOut);
    }

    #[test]
    fn bounds_output() {
        assert!(output(&mut Command::new("yes"), b"", Duration::from_secs(2)).is_err());
    }
}
