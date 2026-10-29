//! Bounded, cancellable command jobs. This is a command runner, not a PTY.
use std::{
    io::Read,
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::SyncSender,
        Arc,
    },
    time::{Duration, Instant},
};

#[derive(Debug)]
pub enum Event {
    Output(String),
    Finished(Result<i32, String>),
}

pub fn append_bounded(output: &mut String, text: &str) {
    const LIMIT: usize = 256 * 1024;
    output.push_str(text);
    if output.len() > LIMIT {
        let mut cut = output.len() - LIMIT;
        while !output.is_char_boundary(cut) {
            cut += 1;
        }
        output.drain(..cut);
    }
}

fn stop_tree(child: &mut Child) {
    #[cfg(unix)]
    // The child starts its own process group; descendants inherit it. Negative
    // pid addresses only that group, never Quark's own process group.
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
}

fn stream(mut pipe: impl Read, tx: SyncSender<Event>) {
    let mut buffer = [0_u8; 4096];
    let mut pending = Vec::new();
    loop {
        let n = match pipe.read(&mut buffer) {
            Ok(0) => {
                if !pending.is_empty() {
                    let _ = tx.send(Event::Output(
                        String::from_utf8_lossy(&pending).into_owned(),
                    ));
                }
                break;
            }
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                let _ = tx.send(Event::Output(format!("\n[Output read failed: {e}]\n")));
                break;
            }
        };
        pending.extend_from_slice(&buffer[..n]);
        let mut decoded = String::new();
        loop {
            match std::str::from_utf8(&pending) {
                Ok(text) => {
                    decoded.push_str(text);
                    pending.clear();
                    break;
                }
                Err(e) => {
                    let valid = e.valid_up_to();
                    decoded.push_str(std::str::from_utf8(&pending[..valid]).unwrap_or_default());
                    pending.drain(..valid);
                    if let Some(len) = e.error_len() {
                        decoded.push('�');
                        pending.drain(..len);
                    } else {
                        break;
                    }
                }
            }
        }
        if !decoded.is_empty() && tx.send(Event::Output(decoded)).is_err() {
            break;
        }
    }
}

pub fn run_streamed(
    program: &str,
    args: &[String],
    cwd: &Path,
    tx: SyncSender<Event>,
    cancel: Arc<AtomicBool>,
    timeout: Duration,
) {
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Never launch hidden credential editors/prompts from this noninteractive UI.
    command
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_MERGE_AUTOEDIT", "no");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            let _ = tx.send(Event::Finished(Err(format!(
                "Failed to start {program}: {e}"
            ))));
            return;
        }
    };
    let mut readers = Vec::new();
    if let Some(pipe) = child.stdout.take() {
        let send = tx.clone();
        readers.push(std::thread::spawn(move || stream(pipe, send)));
    }
    if let Some(pipe) = child.stderr.take() {
        let send = tx.clone();
        readers.push(std::thread::spawn(move || stream(pipe, send)));
    }
    let start = Instant::now();
    let result = loop {
        let timed_out = !timeout.is_zero() && start.elapsed() >= timeout;
        if cancel.load(Ordering::Relaxed) || timed_out {
            stop_tree(&mut child);
            let _ = child.wait();
            break Err(if timed_out {
                "Command timed out"
            } else {
                "Command stopped"
            }
            .to_owned());
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                #[cfg(unix)]
                stop_tree(&mut child);
                break Ok(status.code().unwrap_or(-1));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => {
                stop_tree(&mut child);
                let _ = child.wait();
                break Err(format!("Process wait failed: {e}"));
            }
        }
    };
    // A subprocess can deliberately detach and keep its output pipes open.
    // Do not let that prevent completion/cancellation of the job itself.
    let drain_deadline = Instant::now() + Duration::from_secs(1);
    while readers.iter().any(|r| !r.is_finished()) && Instant::now() < drain_deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    for reader in readers {
        if reader.is_finished() {
            let _ = reader.join();
        }
    }
    let _ = tx.send(Event::Finished(result));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scrollback_is_bounded_at_a_utf8_boundary() {
        let mut output = "π".repeat(200_000);
        append_bounded(&mut output, "✓");
        assert!(output.len() <= 256 * 1024);
        assert!(output.ends_with('✓'));
    }
    #[test]
    fn missing_executable_returns_structured_failure() {
        let (tx, rx) = std::sync::mpsc::sync_channel(16);
        run_streamed(
            "quark-does-not-exist-assert",
            &[],
            Path::new("."),
            tx,
            Arc::new(AtomicBool::new(false)),
            Duration::ZERO,
        );
        assert!(matches!(rx.recv().unwrap(), Event::Finished(Err(_))));
    }
    #[cfg(unix)]
    fn job(script: &str, timeout: Duration, cancel_after: Option<Duration>) -> Vec<Event> {
        let (tx, rx) = std::sync::mpsc::sync_channel(16);
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let script = script.to_owned();
        let worker = std::thread::spawn(move || {
            run_streamed(
                "/bin/sh",
                &["-c".into(), script],
                Path::new("."),
                tx,
                flag,
                timeout,
            )
        });
        if let Some(delay) = cancel_after {
            std::thread::sleep(delay);
            cancel.store(true, Ordering::Relaxed);
        }
        let mut events = Vec::new();
        loop {
            let event = rx.recv_timeout(Duration::from_secs(4)).expect("job hung");
            let done = matches!(event, Event::Finished(_));
            events.push(event);
            if done {
                break;
            }
        }
        worker.join().unwrap();
        events
    }
    #[cfg(unix)]
    #[test]
    fn streams_partial_lines_stderr_and_real_exit_code() {
        let events = job(
            "printf 'prompt'; printf 'error' >&2; exit 7",
            Duration::ZERO,
            None,
        );
        let output: String = events
            .iter()
            .filter_map(|e| {
                if let Event::Output(s) = e {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert!(output.contains("prompt") && output.contains("error"));
        assert!(matches!(events.last(), Some(Event::Finished(Ok(7)))));
    }
    #[cfg(unix)]
    #[test]
    fn cancellation_kills_shell_descendants() {
        let events = job(
            "sleep 30 & wait",
            Duration::ZERO,
            Some(Duration::from_millis(80)),
        );
        assert!(matches!(events.last(), Some(Event::Finished(Err(e))) if e == "Command stopped"));
    }
    #[cfg(unix)]
    #[test]
    fn timeout_releases_the_job() {
        let events = job("sleep 30", Duration::from_millis(80), None);
        assert!(matches!(events.last(), Some(Event::Finished(Err(e))) if e == "Command timed out"));
    }
    #[test]
    fn streaming_preserves_split_utf8() {
        struct Bytes(Vec<u8>);
        impl Read for Bytes {
            fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
                if self.0.is_empty() {
                    Ok(0)
                } else {
                    b[0] = self.0.remove(0);
                    Ok(1)
                }
            }
        }
        let (tx, rx) = std::sync::mpsc::sync_channel(16);
        stream(Bytes("π✓".as_bytes().to_vec()), tx);
        let s: String = rx
            .into_iter()
            .filter_map(|e| {
                if let Event::Output(s) = e {
                    Some(s)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(s, "π✓");
    }
}
