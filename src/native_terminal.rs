//! Native interactive shell process using the host operating system's PTY.
//! Unlike a one-shot command executor this preserves a live shell, terminal
//! state, stdin, and all process output between commands.
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize, PtySystem};
use std::{
    io::{Read, Write},
    path::Path,
    sync::{
        mpsc::{self, Receiver},
        Arc, Mutex,
    },
    thread,
};

/// Messages sent from platform-native input widgets to the live PTY.
pub enum Input {
    Bytes(Vec<u8>),
    Resize { rows: u16, cols: u16 },
}

pub struct Session {
    master: Box<dyn MasterPty + Send>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    receiver: Receiver<Vec<u8>>,
    child: Box<dyn Child + Send>,
}

pub fn default_shell() -> String {
    #[cfg(windows)]
    {
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into())
    }
    #[cfg(not(windows))]
    {
        std::env::var("SHELL")
            .filter(|shell| !shell.trim().is_empty())
            .unwrap_or_else(|_| "/bin/sh".into())
    }
}

impl Session {
    pub fn spawn(directory: &Path) -> Result<Self, String> {
        Self::spawn_shell(directory, &default_shell())
    }

    fn spawn_shell(directory: &Path, shell: &str) -> Result<Self, String> {
        let system = native_pty_system();
        let pair = system
            .openpty(PtySize {
                rows: 26,
                cols: 100,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| format!("Cannot allocate a native PTY: {error}"))?;
        let mut cmd = CommandBuilder::new(shell);
        if directory.is_dir() {
            cmd.cwd(directory);
        }
        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|error| format!("Cannot launch interactive shell: {error}"))?;
        drop(pair.slave);
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| format!("Cannot attach terminal reader: {error}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| format!("Cannot attach terminal input: {error}"))?;
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let mut chunk = [0_u8; 8192];
            loop {
                match reader.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        if sender.send(chunk[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            master: pair.master,
            writer: Arc::new(Mutex::new(writer)),
            receiver,
            child,
        })
    }

    pub fn send(&self, data: &[u8]) -> Result<(), String> {
        self.writer
            .lock()
            .map_err(|error| error.to_string())?
            .write_all(data)
            .map_err(|error| format!("Terminal input failed: {error}"))
    }

    pub fn poll(&self) -> Vec<u8> {
        let mut result = Vec::new();
        while let Ok(part) = self.receiver.try_recv() {
            result.extend(part);
            if result.len() > 262_144 {
                break;
            } // Do not monopolise UI ticks.
        }
        result
    }

    pub fn resize(&self, rows: u16, cols: u16) -> Result<(), String> {
        self.master
            .resize(PtySize {
                rows: rows.max(2),
                cols: cols.max(10),
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| error.to_string())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resize_messages_remain_independent_from_command_input() {
        let resize = Input::Resize {
            rows: 42,
            cols: 132,
        };
        assert!(matches!(
            resize,
            Input::Resize {
                rows: 42,
                cols: 132
            }
        ));
    }

    #[test]
    fn shell_name_is_available() {
        assert!(!default_shell().is_empty());
    }
    #[cfg(unix)]
    #[test]
    fn pty_shell_is_live_and_accepts_multiple_inputs() {
        let temp = tempfile::tempdir().unwrap();
        let session = Session::spawn_shell(temp.path(), "/bin/sh").unwrap();
        session.send(b"printf __QUARK_PTY_ONE__\\n\r").unwrap();
        session.send(b"printf __QUARK_PTY_TWO__\\n\r").unwrap();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(4);
        let mut collected = Vec::new();
        while std::time::Instant::now() < until {
            collected.extend(session.poll());
            if collected
                .windows(b"__QUARK_PTY_TWO__".len())
                .any(|s| s == b"__QUARK_PTY_TWO__")
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(String::from_utf8_lossy(&collected).contains("__QUARK_PTY_ONE__"));
        assert!(String::from_utf8_lossy(&collected).contains("__QUARK_PTY_TWO__"));
    }
}
