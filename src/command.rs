use std::{
    io::{BufRead, BufReader},
    path::Path,
    process::{Command, Stdio},
    sync::mpsc::Sender,
};

/// Streams process output as it is produced, rather than waiting for termination.
pub fn run_streamed(program: &str, args: &[String], cwd: &Path, tx: Sender<String>) {
    let child = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let Ok(mut child) = child else {
        let _ = tx.send(format!(
            "Failed to start {program}: {}\n",
            child.unwrap_err()
        ));
        return;
    };
    let mut readers = Vec::new();
    if let Some(stdout) = child.stdout.take() {
        let send = tx.clone();
        readers.push(std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let _ = send.send(format!("{line}\n"));
            }
        }));
    }
    if let Some(stderr) = child.stderr.take() {
        let send = tx.clone();
        readers.push(std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let _ = send.send(format!("{line}\n"));
            }
        }));
    }
    let status = child.wait();
    for reader in readers {
        let _ = reader.join();
    }
    let _ = tx.send(format!("[Exit: {:?}]\n", status));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_executable_returns_diagnostic() {
        let (tx, rx) = std::sync::mpsc::channel();
        run_streamed("quark-does-not-exist-assert", &[], Path::new("."), tx);
        assert!(rx.into_iter().any(|s| s.contains("Failed to start")));
    }
    #[cfg(unix)]
    #[test]
    fn streams_stdout_stderr_and_exit() {
        let (tx, rx) = std::sync::mpsc::channel();
        run_streamed(
            "/bin/sh",
            &["-c".into(), "echo out; echo err >&2".into()],
            Path::new("."),
            tx,
        );
        let output = rx.into_iter().collect::<String>();
        assert!(output.contains("out"));
        assert!(output.contains("err"));
        assert!(output.contains("Exit"));
    }
}
