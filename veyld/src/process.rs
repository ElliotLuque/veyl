//! Subprocesos con tiempo y salida limitados. Un descendiente que conserve
//! las tuberías abiertas tampoco puede bloquear al daemon indefinidamente.
use std::io::{self, Read, Write};
use std::os::unix::process::CommandExt;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const OUTPUT_LIMIT: u64 = 4 * 1024 * 1024;

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
    fn bounds_output() {
        assert!(output(&mut Command::new("yes"), b"", Duration::from_secs(2)).is_err());
    }
}
