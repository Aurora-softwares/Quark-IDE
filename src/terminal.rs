//! A persistent OS shell on a real PTY (Unix PTY / Windows ConPTY), rendered
//! through a VT terminal screen. It is independent of tool-output jobs.
use crate::theme;
use eframe::egui::{self, Color32, Event, Key, Modifiers};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use std::{
    io::{Read, Write},
    num::NonZeroU16,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, SyncSender},
    thread,
};

fn nz(n: u16) -> NonZeroU16 {
    NonZeroU16::new(n.max(1)).unwrap()
}
#[derive(Default)]
struct Replies {
    bytes: Vec<u8>,
}
impl vt100::Callbacks for Replies {
    fn unhandled_csi(
        &mut self,
        screen: &mut vt100::Screen,
        i1: Option<u8>,
        _: Option<u8>,
        params: &[&[u16]],
        c: char,
    ) {
        let param = params.first().and_then(|p| p.first()).copied().unwrap_or(0);
        let reply = match (c, param) {
            ('n', 6) => {
                let (r, c) = screen.cursor_position();
                format!("\x1b[{};{}R", r + 1, c + 1)
            }
            ('n', 5) => "\x1b[0n".into(),
            ('c', _) => {
                if i1 == Some(b'>') {
                    "\x1b[>0;1;0c".into()
                } else {
                    "\x1b[?1;2c".into()
                }
            }
            _ => String::new(),
        };
        if self.bytes.len() < 4096 {
            self.bytes.extend_from_slice(reply.as_bytes());
        }
    }
}
enum Update {
    Bytes(Vec<u8>),
    Error(String),
}
pub struct Terminal {
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    writer: Option<SyncSender<Vec<u8>>>,
    reader: Option<Receiver<Update>>,
    parser: vt100::Parser<Replies>,
    pub cwd: PathBuf,
    pub shell: String,
    pub exited: Option<String>,
    selection: Option<((u16, u16), (u16, u16))>,
    pub error: Option<String>,
    focus: bool,
}
impl Terminal {
    pub fn spawn(cwd: &Path, ctx: egui::Context) -> Result<Self, String> {
        #[cfg(windows)]
        let shell = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into());
        #[cfg(not(windows))]
        let shell = std::env::var("SHELL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "/bin/sh".into());
        Self::spawn_shell(cwd, &shell, ctx)
    }
    fn spawn_shell(cwd: &Path, shell: &str, ctx: egui::Context) -> Result<Self, String> {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| e.to_string())?;
        let mut command = CommandBuilder::new(shell);
        command.cwd(cwd);
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        #[cfg(unix)]
        command.arg("-i");
        let mut read = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let mut write = pair.master.take_writer().map_err(|e| e.to_string())?;
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|e| e.to_string())?;
        drop(pair.slave);
        let (tx, rx) = mpsc::sync_channel(128);
        let error_tx = tx.clone();
        let (input, commands) = mpsc::sync_channel::<Vec<u8>>(64);
        thread::spawn(move || {
            while let Ok(bytes) = commands.recv() {
                if let Err(e) = write.write_all(&bytes).and_then(|_| write.flush()) {
                    let _ = error_tx.send(Update::Error(format!("Terminal input: {e}")));
                    break;
                }
            }
        });
        thread::spawn(move || {
            let mut b = [0u8; 4096];
            loop {
                match read.read(&mut b) {
                    Ok(0) => break,
                    Ok(n) => {
                        if tx.send(Update::Bytes(b[..n].to_vec())).is_err() {
                            break;
                        }
                        ctx.request_repaint();
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            master: pair.master,
            child,
            writer: Some(input),
            reader: Some(rx),
            parser: vt100::Parser::new_with_callbacks(nz(24), nz(80), 2000, Replies::default()),
            cwd: cwd.to_path_buf(),
            shell: shell.into(),
            exited: None,
            selection: None,
            error: None,
            focus: true,
        })
    }
    pub fn send(&mut self, bytes: Vec<u8>) {
        if let Some(writer) = &self.writer {
            if let Err(e) = writer.try_send(bytes) {
                self.error = Some(format!("Terminal input could not be queued: {e}"));
            }
        }
    }
    pub fn poll(&mut self) {
        if let Some(rx) = &self.reader {
            for _ in 0..64 {
                match rx.try_recv() {
                    Ok(Update::Bytes(bytes)) => self.parser.process(&bytes),
                    Ok(Update::Error(e)) => self.error = Some(e),
                    Err(_) => break,
                }
            }
        }
        let replies = std::mem::take(&mut self.parser.callbacks_mut().bytes);
        if !replies.is_empty() {
            self.send(replies);
        }
        if self.exited.is_none() {
            match self.child.try_wait() {
                Ok(Some(status)) => {
                    self.exited = Some(format!("Shell exited: {status}"));
                    self.writer = None;
                }
                Err(e) => self.error = Some(e.to_string()),
                _ => {}
            }
        }
    }
    pub fn focused(&self, ctx: &egui::Context) -> bool {
        ctx.memory(|m| m.has_focus(egui::Id::new("os-terminal-screen")))
    }
    pub fn request_focus(&mut self) {
        self.focus = true;
    }
    pub fn resize(&mut self, rows: u16, cols: u16) -> Result<(), String> {
        let rows = rows.clamp(2, 120);
        let cols = cols.clamp(2, 400);
        if self.parser.screen().size() != (nz(rows), nz(cols)) {
            self.master
                .resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .map_err(|e| e.to_string())?;
            self.parser.set_size(nz(rows), nz(cols));
            self.selection = None;
        }
        Ok(())
    }
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let c = theme::colors(ui.ctx());
        let font = egui::FontId::monospace(14.0);
        let cell = ui.fonts(|f| egui::vec2(f.glyph_width(&font, 'M'), f.row_height(&font)));
        let available = ui.available_size().max(egui::vec2(30.0, 30.0));
        if let Err(e) = self.resize(
            (available.y / cell.y).floor() as u16,
            (available.x / cell.x).floor() as u16,
        ) {
            self.error = Some(e);
        }
        let (rect, response) = ui.allocate_exact_size(available, egui::Sense::click_and_drag());
        let response = ui.interact(rect, egui::Id::new("os-terminal-screen"), response.sense);
        if self.focus || response.clicked() {
            response.request_focus();
            self.focus = false;
        }
        let (rows, cols) = self.parser.screen().size();
        let (rows, cols) = (rows.get(), cols.get());
        let pointer_cell = |p: egui::Pos2| {
            (
                ((p.y - rect.top()) / cell.y)
                    .floor()
                    .clamp(0.0, (rows - 1) as f32) as u16,
                ((p.x - rect.left()) / cell.x)
                    .floor()
                    .clamp(0.0, (cols - 1) as f32) as u16,
            )
        };
        let mouse_mode = self.parser.screen().mouse_protocol_mode();
        let mouse_capture =
            mouse_mode != vt100::MouseProtocolMode::None && !ui.input(|i| i.modifiers.shift);
        if response.drag_started() && !mouse_capture {
            if let Some(p) = response.interact_pointer_pos() {
                let at = pointer_cell(p);
                self.selection = Some((at, at));
            }
        }
        if response.dragged() && !mouse_capture {
            if let (Some((start, _)), Some(p)) = (self.selection, response.interact_pointer_pos()) {
                self.selection = Some((start, pointer_cell(p)));
            }
        }
        if response.clicked() {
            self.selection = None;
        }
        if response.hovered() {
            let delta = ui.input(|i| i.smooth_scroll_delta.y);
            if delta.abs() > 0.0 {
                if mouse_capture {
                    if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                        let (row, col) = pointer_cell(pos);
                        self.send(mouse_bytes(
                            if delta > 0.0 { 64 } else { 65 },
                            row,
                            col,
                            true,
                            self.parser.screen().mouse_protocol_encoding(),
                        ));
                    }
                } else {
                    let screen = self.parser.screen_mut();
                    let offset = (screen.scrollback() as f32 + delta / cell.y).max(0.0) as usize;
                    screen.set_scrollback(offset);
                    self.selection = None;
                }
            }
        }
        if response.has_focus() {
            let events = ui.input_mut(|i| std::mem::take(&mut i.events));
            for event in events {
                let bytes = match &event {
                    Event::Text(text) => {
                        let modifiers = ui.input(|i| i.modifiers);
                        let mut bytes = text.as_bytes().to_vec();
                        if modifiers.alt && !modifiers.ctrl && !modifiers.mac_cmd {
                            bytes.insert(0, 27);
                        }
                        Some(bytes)
                    }
                    Event::Ime(egui::ImeEvent::Commit(text)) => Some(text.as_bytes().to_vec()),
                    Event::Cut => {
                        if cfg!(target_os = "macos") {
                            None
                        } else {
                            Some(vec![24])
                        }
                    }
                    Event::Paste(text) => Some(paste(text, self.parser.screen().bracketed_paste())),
                    Event::Copy => {
                        // egui-winit turns Ctrl+C into Copy before exposing key
                        // events. With no selection it must still interrupt the
                        // foreground OS process, rather than disappear here.
                        if let Some((a, b)) = self.selection {
                            let (a, b) = if a <= b { (a, b) } else { (b, a) };
                            ui.ctx().copy_text(self.parser.screen().contents_between(
                                a.0,
                                a.1,
                                b.0,
                                (b.1 + 1).min(cols),
                            ));
                            None
                        } else if !cfg!(target_os = "macos") && !ui.input(|i| i.modifiers.shift) {
                            Some(vec![3])
                        } else {
                            None
                        }
                    }
                    Event::PointerButton {
                        pos,
                        button,
                        pressed,
                        modifiers,
                    } if mouse_capture && rect.contains(*pos) => {
                        let (row, col) = pointer_cell(*pos);
                        let code = match button {
                            egui::PointerButton::Primary => 0,
                            egui::PointerButton::Middle => 1,
                            egui::PointerButton::Secondary => 2,
                            _ => 3,
                        };
                        let mods = 4 * u8::from(modifiers.shift)
                            + 8 * u8::from(modifiers.alt)
                            + 16 * u8::from(modifiers.ctrl);
                        if *pressed || mouse_mode != vt100::MouseProtocolMode::Press {
                            Some(mouse_bytes(
                                code + mods,
                                row,
                                col,
                                *pressed,
                                self.parser.screen().mouse_protocol_encoding(),
                            ))
                        } else {
                            None
                        }
                    }
                    Event::PointerMoved(pos) if mouse_capture && rect.contains(*pos) => {
                        let button = ui.input(|i| {
                            if i.pointer.primary_down() {
                                Some(0)
                            } else if i.pointer.middle_down() {
                                Some(1)
                            } else if i.pointer.secondary_down() {
                                Some(2)
                            } else {
                                None
                            }
                        });
                        if mouse_mode == vt100::MouseProtocolMode::AnyMotion
                            || (mouse_mode == vt100::MouseProtocolMode::ButtonMotion
                                && button.is_some())
                        {
                            let (row, col) = pointer_cell(*pos);
                            Some(mouse_bytes(
                                32 + button.unwrap_or(3),
                                row,
                                col,
                                true,
                                self.parser.screen().mouse_protocol_encoding(),
                            ))
                        } else {
                            None
                        }
                    }
                    Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => {
                        if *key == Key::PageUp && modifiers.shift {
                            let off = self.parser.screen().scrollback() + rows as usize;
                            self.parser.screen_mut().set_scrollback(off);
                            None
                        } else if *key == Key::PageDown && modifiers.shift {
                            let off = self
                                .parser
                                .screen()
                                .scrollback()
                                .saturating_sub(rows as usize);
                            self.parser.screen_mut().set_scrollback(off);
                            None
                        } else {
                            key_bytes(*key, *modifiers, self.parser.screen().application_cursor())
                        }
                    }
                    _ => None,
                };
                if let Some(bytes) = bytes {
                    self.parser.screen_mut().set_scrollback(0);
                    self.selection = None;
                    if self.exited.is_none() {
                        self.send(bytes);
                    }
                }
            }
        }
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0, c.terminal.color());
        let selected = self
            .selection
            .map(|(a, b)| if a <= b { (a, b) } else { (b, a) });
        for row in 0..rows {
            for col in 0..cols {
                let Some(v) = self.parser.screen().cell(row, col) else {
                    continue;
                };
                let (mut fg, mut bg) = (
                    color(v.fgcolor(), c.terminal_text.color(), &c),
                    color(v.bgcolor(), c.terminal.color(), &c),
                );
                if v.inverse() {
                    std::mem::swap(&mut fg, &mut bg);
                }
                if v.dim() {
                    fg = fg.gamma_multiply(0.65);
                }
                if selected.is_some_and(|(a, b)| (row, col) >= a && (row, col) <= b) {
                    bg = c.selection.color();
                }
                let pos = rect.min + egui::vec2(col as f32 * cell.x, row as f32 * cell.y);
                let r = egui::Rect::from_min_size(pos, cell);
                if bg != c.terminal.color() {
                    painter.rect_filled(r, 0, bg);
                }
                if !v.is_wide_continuation() && v.has_contents() {
                    painter.text(pos, egui::Align2::LEFT_TOP, v.contents(), font.clone(), fg);
                    if v.bold() {
                        painter.text(
                            pos + egui::vec2(0.4, 0.0),
                            egui::Align2::LEFT_TOP,
                            v.contents(),
                            font.clone(),
                            fg,
                        );
                    }
                    if v.underline() {
                        painter.line_segment(
                            [r.left_bottom(), r.right_bottom()],
                            egui::Stroke::new(1.0_f32, fg),
                        );
                    }
                }
            }
        }
        if response.has_focus()
            && !self.parser.screen().hide_cursor()
            && self.parser.screen().scrollback() == 0
            && self.exited.is_none()
        {
            let (r, col) = self.parser.screen().cursor_position();
            let p = rect.min + egui::vec2(col as f32 * cell.x, r as f32 * cell.y);
            painter.rect_stroke(
                egui::Rect::from_min_size(p, cell),
                0,
                egui::Stroke::new(1.2_f32, c.accent.color()),
                egui::StrokeKind::Inside,
            );
        }
        if let Some(error) = &self.error {
            response.on_hover_text(error);
        }
    }
    pub fn stop(&mut self) {
        if self.exited.is_some() {
            return;
        }
        #[cfg(unix)]
        unsafe {
            // Interactive jobs have a foreground group distinct from the shell.
            if let Some(fd) = self.master.as_raw_fd() {
                let fg = libc::tcgetpgrp(fd);
                if fg > 0 {
                    libc::kill(-fg, libc::SIGKILL);
                }
            }
            if let Some(pid) = self.child.process_id() {
                libc::kill(-(pid as i32), libc::SIGKILL);
            }
        }
        #[cfg(windows)]
        if let Some(pid) = self.child.process_id() {
            let _ = std::process::Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
        let _ = self.child.kill();
        self.writer = None;
        self.reader = None;
        let _ = self.child.wait();
        self.exited = Some("Session ended".into());
    }
}
impl Drop for Terminal {
    fn drop(&mut self) {
        self.stop();
    }
}
pub fn paste(text: &str, bracketed: bool) -> Vec<u8> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    if bracketed {
        format!("\x1b[200~{}\x1b[201~", text.replace('\x1b', "")).into_bytes()
    } else {
        text.replace('\n', "\r").into_bytes()
    }
}
pub fn key_bytes(key: Key, m: Modifiers, application: bool) -> Option<Vec<u8>> {
    if m.ctrl && !m.alt && !m.mac_cmd {
        let control = match key {
            Key::A => 1,
            Key::B => 2,
            Key::C => 3,
            Key::D => 4,
            Key::E => 5,
            Key::F => 6,
            Key::G => 7,
            Key::H => 8,
            Key::I => 9,
            Key::J => 10,
            Key::K => 11,
            Key::L => 12,
            Key::M => 13,
            Key::N => 14,
            Key::O => 15,
            Key::P => 16,
            Key::Q => 17,
            Key::R => 18,
            Key::S => 19,
            Key::T => 20,
            Key::U => 21,
            Key::V => 22,
            Key::W => 23,
            Key::X => 24,
            Key::Y => 25,
            Key::Z => 26,
            Key::Space => 0,
            _ => 255,
        };
        if control != 255 {
            return Some(vec![control]);
        }
    }
    let modifier = 1 + u8::from(m.shift) + 2 * u8::from(m.alt) + 4 * u8::from(m.ctrl);
    let arrow = match key {
        Key::ArrowUp => Some('A'),
        Key::ArrowDown => Some('B'),
        Key::ArrowRight => Some('C'),
        Key::ArrowLeft => Some('D'),
        Key::Home => Some('H'),
        Key::End => Some('F'),
        _ => None,
    };
    if let Some(a) = arrow {
        return Some(
            if modifier > 1 {
                format!("\x1b[1;{modifier}{a}")
            } else {
                format!("\x1b{}{a}", if application { 'O' } else { '[' })
            }
            .into_bytes(),
        );
    }
    let s = match key {
        Key::Enter => "\r",
        Key::Backspace => "\x7f",
        Key::Tab if m.shift => "\x1b[Z",
        Key::Tab => "\t",
        Key::Escape => "\x1b",
        Key::Insert => "\x1b[2~",
        Key::Delete => "\x1b[3~",
        Key::PageUp => "\x1b[5~",
        Key::PageDown => "\x1b[6~",
        Key::F1 => "\x1bOP",
        Key::F2 => "\x1bOQ",
        Key::F3 => "\x1bOR",
        Key::F4 => "\x1bOS",
        Key::F5 => "\x1b[15~",
        Key::F6 => "\x1b[17~",
        Key::F7 => "\x1b[18~",
        Key::F8 => "\x1b[19~",
        Key::F9 => "\x1b[20~",
        Key::F10 => "\x1b[21~",
        Key::F11 => "\x1b[23~",
        Key::F12 => "\x1b[24~",
        _ => return None,
    };
    Some(s.as_bytes().to_vec())
}
fn mouse_bytes(
    code: u8,
    row: u16,
    col: u16,
    pressed: bool,
    encoding: vt100::MouseProtocolEncoding,
) -> Vec<u8> {
    match encoding {
        vt100::MouseProtocolEncoding::Sgr => format!(
            "\x1b[<{code};{};{}{}",
            col + 1,
            row + 1,
            if pressed { 'M' } else { 'm' }
        )
        .into_bytes(),
        vt100::MouseProtocolEncoding::Utf8 => {
            let mut bytes = b"\x1b[M".to_vec();
            for n in [
                u32::from(if pressed { code } else { 3 }) + 32,
                u32::from(col) + 33,
                u32::from(row) + 33,
            ] {
                if let Some(ch) = char::from_u32(n) {
                    let mut out = [0u8; 4];
                    bytes.extend_from_slice(ch.encode_utf8(&mut out).as_bytes());
                }
            }
            bytes
        }
        _ => vec![
            27,
            b'[',
            b'M',
            if pressed { code } else { 3 } + 32,
            (col.min(222) + 33) as u8,
            (row.min(222) + 33) as u8,
        ],
    }
}
fn color(color: vt100::Color, default: Color32, c: &theme::Palette) -> Color32 {
    match color {
        vt100::Color::Default => default,
        vt100::Color::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
        vt100::Color::Idx(i) if i < 16 => c.ansi[i as usize].color(),
        vt100::Color::Idx(i) if i >= 232 => {
            let v = 8 + (i - 232) * 10;
            Color32::from_rgb(v, v, v)
        }
        vt100::Color::Idx(i) => {
            let n = i - 16;
            let level = |n: u8| if n == 0 { 0 } else { 55 + 40 * n };
            Color32::from_rgb(level(n / 36), level((n / 6) % 6), level(n % 6))
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mouse_reporting_uses_terminal_coordinates_and_release_codes() {
        assert_eq!(
            mouse_bytes(0, 2, 4, true, vt100::MouseProtocolEncoding::Sgr),
            b"\x1b[<0;5;3M"
        );
        assert_eq!(
            mouse_bytes(0, 2, 4, false, vt100::MouseProtocolEncoding::Sgr),
            b"\x1b[<0;5;3m"
        );
    }
    #[test]
    fn control_keys_and_paste_reach_the_os() {
        assert_eq!(key_bytes(Key::C, Modifiers::CTRL, false), Some(vec![3]));
        assert_eq!(
            key_bytes(Key::ArrowUp, Modifiers::NONE, true),
            Some(b"\x1bOA".to_vec())
        );
        assert_eq!(paste("a\nb", true), b"\x1b[200~a\nb\x1b[201~");
    }
    #[test]
    fn ansi_screen_tracks_colours_cursor_and_alternate_screen() {
        let mut p = vt100::Parser::new(nz(8), nz(40), 20);
        p.process(b"\x1b[31mred\x1b[0m\x1b[2;4Hxy");
        assert_eq!(
            p.screen().cell(0, 0).unwrap().fgcolor(),
            vt100::Color::Idx(1)
        );
        assert_eq!(p.screen().cursor_position(), (1, 5));
        p.process(b"\x1b[?1049happ\x1b[?1049l");
        assert!(p.screen().contents().contains("red"));
    }
    #[cfg(unix)]
    #[test]
    fn real_pty_persists_directory_environment_and_interactive_input() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut t = Terminal::spawn_shell(dir.path(), "/bin/sh", ctx).unwrap();
        t.send(b"test -t 0 && echo QUARK_IS_TTY; QUARK_TEST=kept; export QUARK_TEST\rcd /\rprintf 'PERSIST=%s:%s\\n' \"$PWD\" \"$QUARK_TEST\"\rread value; printf 'READ=%s\\n' \"$value\"\ranswer\r".to_vec());
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::time::Instant::now() < until {
            t.poll();
            if t.parser.screen().contents().contains("READ=answer") {
                break;
            }
            thread::sleep(std::time::Duration::from_millis(10));
        }
        let text = t.parser.screen().contents();
        assert!(text.contains("QUARK_IS_TTY"));
        assert!(text.contains("PERSIST=/:kept"), "{text}");
        assert!(text.contains("READ=answer"), "{text}");
        t.resize(12, 95).unwrap();
        t.send(b"stty size\r".to_vec());
        let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while std::time::Instant::now() < until {
            t.poll();
            if t.parser.screen().contents().contains("12 95") {
                break;
            }
            thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(t.parser.screen().contents().contains("12 95"));
        t.send(b"sleep 30\r".to_vec());
        thread::sleep(std::time::Duration::from_millis(80));
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 300.0),
            )),
            events: vec![Event::Copy],
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| t.ui(ui));
        });
        t.send(b"printf '\\nAFTER_INTERRUPT\\n'\r".to_vec());
        let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while std::time::Instant::now() < until {
            t.poll();
            if t.parser
                .screen()
                .contents()
                .lines()
                .any(|l| l == "AFTER_INTERRUPT")
            {
                break;
            }
            thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            t.parser
                .screen()
                .contents()
                .lines()
                .any(|l| l == "AFTER_INTERRUPT"),
            "{}",
            t.parser.screen().contents()
        );
        t.stop();
        assert!(t.child.try_wait().unwrap().is_some());
    }
}

#[cfg(test)]
mod feature_coverage {
    use super::*;
    #[test]
    fn normal_paste_normalises_newlines_to_shell_returns() {
        assert_eq!(paste("a\r\nb\rc\nd", false), b"a\rb\rc\rd");
        assert!(paste("", false).is_empty());
    }
    #[test]
    fn bracketed_paste_cannot_inject_an_early_terminator() {
        let bytes = paste("a\x1b[201~\r\nb", true);
        assert_eq!(bytes, b"\x1b[200~a[201~\nb\x1b[201~");
    }
    #[test]
    fn normal_arrow_and_navigation_keys_use_csi_sequences() {
        for (key, expected) in [
            (Key::ArrowLeft, "\x1b[D"),
            (Key::ArrowRight, "\x1b[C"),
            (Key::ArrowDown, "\x1b[B"),
            (Key::Home, "\x1b[H"),
            (Key::End, "\x1b[F"),
        ] {
            assert_eq!(
                key_bytes(key, Modifiers::NONE, false).unwrap(),
                expected.as_bytes()
            );
        }
    }
    #[test]
    fn modified_arrows_preserve_control_and_shift() {
        assert_eq!(
            key_bytes(Key::ArrowLeft, Modifiers::CTRL | Modifiers::SHIFT, false).unwrap(),
            b"\x1b[1;6D"
        );
        assert_eq!(
            key_bytes(Key::ArrowDown, Modifiers::ALT, true).unwrap(),
            b"\x1b[1;3B"
        );
    }
    #[test]
    fn application_cursor_mode_uses_ss3_for_each_direction() {
        for (key, last) in [
            (Key::ArrowUp, b'A'),
            (Key::ArrowDown, b'B'),
            (Key::ArrowRight, b'C'),
            (Key::ArrowLeft, b'D'),
        ] {
            assert_eq!(
                key_bytes(key, Modifiers::NONE, true).unwrap(),
                vec![27, b'O', last]
            );
        }
    }
    #[test]
    fn control_characters_and_function_keys_have_distinct_encodings() {
        assert_eq!(key_bytes(Key::A, Modifiers::CTRL, false), Some(vec![1]));
        assert_eq!(key_bytes(Key::Z, Modifiers::CTRL, false), Some(vec![26]));
        assert_eq!(key_bytes(Key::Space, Modifiers::CTRL, false), Some(vec![0]));
        assert_eq!(
            key_bytes(Key::F5, Modifiers::NONE, false).unwrap(),
            b"\x1b[15~"
        );
        assert_eq!(
            key_bytes(Key::F12, Modifiers::NONE, false).unwrap(),
            b"\x1b[24~"
        );
    }
    #[test]
    fn editing_keys_and_backtab_are_not_printable_text() {
        for (key, expected) in [
            (Key::Enter, b"\r".as_slice()),
            (Key::Backspace, b"\x7f"),
            (Key::Delete, b"\x1b[3~"),
            (Key::PageUp, b"\x1b[5~"),
            (Key::PageDown, b"\x1b[6~"),
        ] {
            assert_eq!(key_bytes(key, Modifiers::NONE, false).unwrap(), expected);
        }
        assert_eq!(
            key_bytes(Key::Tab, Modifiers::SHIFT, false).unwrap(),
            b"\x1b[Z"
        );
        assert_eq!(key_bytes(Key::A, Modifiers::NONE, false), None);
    }
    #[test]
    fn legacy_mouse_encoding_clamps_large_cells_and_encodes_release() {
        assert_eq!(
            mouse_bytes(0, 500, 500, true, vt100::MouseProtocolEncoding::Default),
            vec![27, b'[', b'M', 32, 255, 255]
        );
        assert_eq!(
            mouse_bytes(2, 0, 0, false, vt100::MouseProtocolEncoding::Default),
            vec![27, b'[', b'M', 35, 33, 33]
        );
    }
    #[test]
    fn utf8_mouse_encoding_retains_coordinates_beyond_legacy_limit() {
        let b = mouse_bytes(0, 300, 400, true, vt100::MouseProtocolEncoding::Utf8);
        let expected = format!(
            "\x1b[M {}{}",
            char::from_u32(433).unwrap(),
            char::from_u32(333).unwrap()
        );
        assert_eq!(b, expected.as_bytes());
    }
    #[test]
    fn terminal_palette_resolves_default_ansi_rgb_and_greyscale() {
        let c = theme::Preset::Dawn.definition().colors;
        assert_eq!(
            color(vt100::Color::Default, c.terminal_text.color(), &c),
            c.terminal_text.color()
        );
        assert_eq!(
            color(vt100::Color::Idx(3), Color32::BLACK, &c),
            c.ansi[3].color()
        );
        assert_eq!(
            color(vt100::Color::Idx(232), Color32::BLACK, &c),
            Color32::from_rgb(8, 8, 8)
        );
        assert_eq!(
            color(vt100::Color::Rgb(1, 2, 3), Color32::BLACK, &c),
            Color32::from_rgb(1, 2, 3)
        );
    }
    #[test]
    fn ansi_palette_changes_with_theme_while_truecolour_stays_exact() {
        let a = theme::Preset::Aurora.definition().colors;
        let b = theme::Preset::Dawn.definition().colors;
        assert_ne!(
            color(vt100::Color::Idx(4), Color32::BLACK, &a),
            color(vt100::Color::Idx(4), Color32::BLACK, &b)
        );
        assert_eq!(
            color(vt100::Color::Idx(196), Color32::BLACK, &a),
            Color32::from_rgb(255, 0, 0)
        );
        assert_eq!(
            color(vt100::Color::Rgb(1, 2, 3), Color32::BLACK, &a),
            color(vt100::Color::Rgb(1, 2, 3), Color32::BLACK, &b)
        );
    }
    #[test]
    fn vt_screen_handles_unicode_wide_cells_erase_and_cursor() {
        let mut p = vt100::Parser::new(nz(4), nz(30), 10);
        p.process("π界x".as_bytes());
        assert_eq!(p.screen().cursor_position(), (0, 4));
        assert!(p.screen().cell(0, 2).unwrap().is_wide_continuation());
        p.process(b"\r\x1b[2Kdone");
        assert_eq!(p.screen().contents(), "done");
    }
    #[test]
    fn alternate_screen_restores_primary_contents_and_has_separate_cursor() {
        let mut p = vt100::Parser::new(nz(4), nz(20), 10);
        p.process(b"primary\x1b[?1049h\x1b[2J\x1b[Happ");
        assert_eq!(p.screen().contents(), "app");
        p.process(b"\x1b[?1049l");
        assert_eq!(p.screen().contents(), "primary");
        assert_eq!(p.screen().cursor_position(), (0, 7));
    }
    #[test]
    fn terminal_scrollback_is_bounded_and_can_return_to_live_screen() {
        let mut p = vt100::Parser::new(nz(2), nz(10), 3);
        p.process(b"a\r\nb\r\nc\r\nd\r\ne\r\nf");
        p.screen_mut().set_scrollback(100);
        assert_eq!(p.screen().scrollback(), 3);
        assert!(!p.screen().contents().contains('f'));
        p.screen_mut().set_scrollback(0);
        assert!(p.screen().contents().contains('f'));
    }
    #[test]
    fn terminal_scrollback_selection_reads_visible_unicode() {
        let mut p = vt100::Parser::new(nz(2), nz(20), 10);
        p.process("old π\r\nline\r\nnew".as_bytes());
        p.screen_mut().set_scrollback(1);
        assert_eq!(p.screen().contents_between(0, 0, 0, 5), "old π");
    }
    #[test]
    fn terminal_status_and_cursor_queries_receive_protocol_replies() {
        let mut p = vt100::Parser::new_with_callbacks(nz(4), nz(20), 0, Replies::default());
        p.process(b"\x1b[2;4H\x1b[6n\x1b[5n");
        assert_eq!(p.callbacks().bytes, b"\x1b[2;4R\x1b[0n");
    }
    #[test]
    fn terminal_device_queries_reply_but_clipboard_osc_is_ignored() {
        let mut p = vt100::Parser::new_with_callbacks(nz(4), nz(20), 0, Replies::default());
        p.process(b"\x1b[c\x1b[>c");
        assert!(p.callbacks().bytes.starts_with(b"\x1b[?1;2c"));
        let before = p.callbacks().bytes.clone();
        p.process(b"\x1b]52;c;secret\x07");
        assert_eq!(p.callbacks().bytes, before);
    }
    #[test]
    fn missing_os_shell_reports_spawn_failure() {
        let dir = tempfile::tempdir().unwrap();
        assert!(Terminal::spawn_shell(
            dir.path(),
            &dir.path().join("missing-shell").to_string_lossy(),
            egui::Context::default()
        )
        .is_err());
    }
    #[cfg(unix)]
    fn shell() -> (tempfile::TempDir, Terminal) {
        let dir = tempfile::tempdir().unwrap();
        let t = Terminal::spawn_shell(dir.path(), "/bin/sh", egui::Context::default()).unwrap();
        (dir, t)
    }
    #[cfg(unix)]
    fn wait_for(t: &mut Terminal, predicate: impl Fn(&Terminal) -> bool) {
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_secs(4) {
            t.poll();
            if predicate(t) {
                return;
            }
            thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("Terminal timed out: {}", t.parser.screen().contents());
    }
    #[cfg(unix)]
    #[test]
    fn terminal_resize_clamps_geometry_and_child_observes_minimum() {
        let (_dir, mut t) = shell();
        t.resize(0, 0).unwrap();
        assert_eq!(t.parser.screen().size(), (nz(2), nz(2)));
        t.resize(u16::MAX, u16::MAX).unwrap();
        assert_eq!(t.parser.screen().size(), (nz(120), nz(400)));
        t.resize(10, 50).unwrap();
        t.send(b"stty size\r".to_vec());
        wait_for(&mut t, |t| {
            t.parser.screen().contents().lines().any(|l| l == "10 50")
        });
    }
    #[cfg(unix)]
    #[test]
    fn terminal_stop_is_idempotent_and_reaps_child() {
        let (_dir, mut t) = shell();
        t.stop();
        t.stop();
        assert!(t.child.try_wait().unwrap().is_some());
        assert!(t.exited.is_some());
    }
    #[cfg(unix)]
    #[test]
    fn natural_shell_exit_reports_status_and_closes_input() {
        let (_dir, mut t) = shell();
        t.send(b"exit 7\r".to_vec());
        wait_for(&mut t, |t| t.exited.is_some());
        assert!(t.exited.as_ref().unwrap().contains('7'));
        assert!(t.writer.is_none());
    }
    #[cfg(unix)]
    #[test]
    fn clipboard_copy_with_selection_copies_text_instead_of_interrupting() {
        let (_dir, mut t) = shell();
        t.parser.process(b"\x1b[2J\x1b[Hhello");
        t.selection = Some(((0, 0), (0, 4)));
        let ctx = egui::Context::default();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(500., 200.),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| t.ui(ui));
            },
        );
        t.parser.process(b"\x1b[2J\x1b[Hhello");
        t.selection = Some(((0, 0), (0, 4)));
        let out = ctx.run(
            egui::RawInput {
                events: vec![Event::Copy],
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(500., 200.),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| t.ui(ui));
            },
        );
        assert!(out
            .platform_output
            .commands
            .iter()
            .any(|c| matches!(c,egui::OutputCommand::CopyText(s) if s=="hello")));
        assert!(t.exited.is_none());
    }
    #[cfg(unix)]
    #[test]
    fn terminal_session_restart_has_fresh_environment_and_directory() {
        let (dir, mut first) = shell();
        first.send(b"export QUARK_SESSION_VALUE=old\rcd /\r".to_vec());
        first.stop();
        let mut second =
            Terminal::spawn_shell(dir.path(), "/bin/sh", egui::Context::default()).unwrap();
        second.send(b"printf 'SESSION=%s\\n' \"${QUARK_SESSION_VALUE-fresh}\"\r".to_vec());
        wait_for(&mut second, |t| {
            t.parser
                .screen()
                .contents()
                .lines()
                .any(|l| l.ends_with("SESSION=fresh"))
        });
        assert_eq!(second.cwd, dir.path());
    }
}
