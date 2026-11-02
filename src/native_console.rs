//! Safe ANSI-SGR to Pango markup for read-only terminal-like Quark logs.
//! Escape all user/compiler text before adding markup; do not interpret ANSI
//! cursor-control commands as literal visible characters.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Palette {
    Output,
    Tests,
}

#[cfg(any(target_os = "linux", test))]
fn escape(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '\'' => escaped.push_str("&apos;"),
            '"' => escaped.push_str("&quot;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}
#[cfg(any(target_os = "linux", test))]
const NORMAL: [&str; 8] = [
    "#161b22", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2", "#abb2bf",
];
#[cfg(any(target_os = "linux", test))]
const BRIGHT: [&str; 8] = [
    "#5c6370", "#ff7b86", "#b5e890", "#ffda83", "#83c5ff", "#d9a1ff", "#8be4ee", "#ffffff",
];
#[cfg(any(target_os = "linux", test))]
fn rgb(number: u16) -> String {
    if number < 16 {
        return if number < 8 {
            NORMAL[number as usize]
        } else {
            BRIGHT[(number - 8) as usize]
        }
        .into();
    }
    if number < 232 {
        let n = number - 16;
        let cube = |v: u16| if v == 0 { 0 } else { 55 + 40 * v };
        return format!(
            "#{:02x}{:02x}{:02x}",
            cube(n / 36),
            cube((n / 6) % 6),
            cube(n % 6)
        );
    }
    let shade = 8 + 10 * (number - 232).min(23);
    format!("#{shade:02x}{shade:02x}{shade:02x}")
}
/// Turn stdout/stderr pipe line endings into the CRLF normally produced by
/// an interactive terminal's ONLCR line discipline. A VT100 LF only moves the
/// cursor down; without CR, each Hydrogen progress message starts farther to
/// the right, producing a staircase across the Output pane.
///
/// The state crosses chunk boundaries so an already valid CRLF is left
/// untouched even when the two bytes arrive in separate reads. Bare CR
/// (spinners, progress overwrites), ANSI escapes, and UTF-8 bytes are
/// preserved exactly. This is for compiler *pipes*, never the real PTY.
#[derive(Default)]
pub struct PipeLineEndings {
    previous_was_cr: bool,
}

impl PipeLineEndings {
    pub fn normalize(&mut self, chunk: &[u8]) -> Vec<u8> {
        let mut output = Vec::with_capacity(chunk.len() + chunk.len() / 32 + 1);
        for &byte in chunk {
            if byte == b'\n' && !self.previous_was_cr {
                output.push(b'\r');
            }
            output.push(byte);
            self.previous_was_cr = byte == b'\r';
        }
        output
    }
}

/// Discard formatting/control escape sequences while preserving readable
/// UTF-8 text. Used for non-GTK fallback and pass/fail classification.
pub fn remove_escape_sequences(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut text = String::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            i += 1;
            if i < bytes.len() && bytes[i] == b'[' {
                i += 1;
                while i < bytes.len() && !(0x40..=0x7e).contains(&bytes[i]) {
                    i += 1;
                }
                if i < bytes.len() {
                    i += 1;
                }
            }
            continue;
        }
        let ch = raw[i..].chars().next().unwrap();
        if !ch.is_control() || ch == '\n' || ch == '\t' {
            text.push(ch);
        }
        i += ch.len_utf8();
    }
    text
}

#[cfg(any(target_os = "linux", test))]
pub fn markup(raw: &str, palette: Palette) -> String {
    let bytes = raw.as_bytes();
    let mut out = String::new();
    let mut current = String::new();
    let mut fg = String::from("#d9e1eb");
    let mut bold = false;
    fn push(out: &mut String, current: &mut String, fg: &str, bold: bool) {
        if current.is_empty() {
            return;
        }
        out.push_str(&format!(
            "<span foreground=\"{}\"{}>{}</span>",
            fg,
            if bold { " weight=\"bold\"" } else { "" },
            escape(current),
        ));
        current.clear();
    }
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            let start = i + 2;
            let mut end = start;
            while end < bytes.len()
                && end - start < 48
                && (bytes[end].is_ascii_digit() || bytes[end] == b';')
            {
                end += 1;
            }
            if end < bytes.len() {
                push(&mut out, &mut current, &fg, bold);
                let code = bytes[end];
                if code == b'm' {
                    let codes = String::from_utf8_lossy(&bytes[start..end]);
                    let entries = codes
                        .split(';')
                        .map(|v| v.parse::<u16>().unwrap_or(0))
                        .collect::<Vec<_>>();
                    let mut cursor = 0;
                    while cursor < entries.len() {
                        match entries[cursor] {
                            0 => {
                                fg = "#d9e1eb".into();
                                bold = false;
                            }
                            1 => bold = true,
                            22 => bold = false,
                            39 => fg = "#d9e1eb".into(),
                            v @ 30..=37 => fg = NORMAL[(v - 30) as usize].into(),
                            v @ 90..=97 => fg = BRIGHT[(v - 90) as usize].into(),
                            38 if cursor + 2 < entries.len() && entries[cursor + 1] == 5 => {
                                fg = rgb(entries[cursor + 2]);
                                cursor += 2;
                            }
                            38 if cursor + 4 < entries.len() && entries[cursor + 1] == 2 => {
                                fg = format!(
                                    "#{:02x}{:02x}{:02x}",
                                    entries[cursor + 2].min(255),
                                    entries[cursor + 3].min(255),
                                    entries[cursor + 4].min(255)
                                );
                                cursor += 4;
                            }
                            _ => {}
                        }
                        cursor += 1;
                    }
                }
                i = end + 1;
                continue;
            }
        }
        if bytes[i] == b'\r' {
            // Normalise CRLF rather than showing a return-control symbol.
            if i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                i += 1;
            }
            current.push('\n');
            i += 1;
            continue;
        }
        if bytes[i] == 0x1b {
            i += 1;
            continue;
        }
        if bytes[i].is_ascii_control() && bytes[i] != b'\n' && bytes[i] != b'\t' {
            i += 1;
            continue;
        }
        let ch = raw[i..].chars().next().unwrap();
        current.push(ch);
        i += ch.len_utf8();
    }
    push(&mut out, &mut current, &fg, bold);
    if palette == Palette::Tests {
        // Test runners may already colour output. Strip existing SGR codes
        // before applying consistent pass/fail colouring for each line.
        let plain = remove_escape_sequences(raw);
        out = plain
            .lines()
            .map(|line| {
                let low = line.to_ascii_lowercase();
                let successful_summary = low.contains("test result: ok")
                    || (low.contains("0 failed") && low.contains("pass"));
                let colour = if successful_summary {
                    "#b5e890"
                } else if low.contains("failed")
                    || low.contains("fail:")
                    || low.contains("error:")
                    || low.contains("panic")
                {
                    "#ff7b86"
                } else if low.contains("passed")
                    || low.contains(" ... ok")
                    || low.starts_with("pass")
                    || low.contains("success")
                {
                    "#b5e890"
                } else {
                    "#d9e1eb"
                };
                format!("<span foreground=\"{colour}\">{}</span>\n", escape(line))
            })
            .collect();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pipe_line_endings_keep_terminal_columns_aligned() {
        use std::num::NonZeroU16;
        let mut endings = PipeLineEndings::default();
        let mut screen = vt100::Parser::new(
            NonZeroU16::new(8).unwrap(),
            NonZeroU16::new(80).unwrap(),
            10,
        );
        for chunk in [
            &b"Compiled 590 methods\nCompiled 600 methods\n"[..],
            &b"Compiled 610 methods\n"[..],
        ] {
            screen.process(&endings.normalize(chunk));
        }
        assert_eq!(screen.screen().cell(0, 0).unwrap().contents(), "C");
        assert_eq!(screen.screen().cell(1, 0).unwrap().contents(), "C");
        assert_eq!(screen.screen().cell(2, 0).unwrap().contents(), "C");
        assert_eq!(screen.screen().cursor_position().1, 0);
        assert!(screen.screen().contents().contains("Compiled 610 methods"));
    }

    #[test]
    fn pipe_line_endings_preserve_spinner_overwrites_ansi_and_split_crlf() {
        let mut endings = PipeLineEndings::default();
        assert_eq!(
            endings.normalize(b"Building 10%\r\x1b[2KBuilding 20%"),
            b"Building 10%\r\x1b[2KBuilding 20%"
        );
        assert_eq!(endings.normalize(b"\r"), b"\r");
        assert_eq!(endings.normalize(b"\nNext\n"), b"\nNext\r\n");
        assert_eq!(endings.normalize(b"\x1b[31mERROR\x1b[0m"), b"\x1b[31mERROR\x1b[0m");
        // A partial UTF-8 scalar is never decoded or altered.
        assert_eq!(endings.normalize(&[0xe2, 0x98]), vec![0xe2, 0x98]);
        assert_eq!(endings.normalize(&[0x83, b'\n']), vec![0x83, b'\r', b'\n']);
    }

    #[test]
    fn sgr_colours_and_markup_escape_are_safe() {
        let m = markup("a <b>\x1b[31mERROR & more\x1b[0m", Palette::Output);
        assert!(m.contains("&lt;b&gt;"));
        assert!(m.contains("#e06c75"));
        assert!(m.contains("&amp;"));
        assert!(!m.contains("\x1b"));
    }
    #[test]
    fn strips_ansi_colours_and_private_cursor_codes() {
        assert_eq!(
            remove_escape_sequences("\x1b[?25l\x1b[31mFAIL\x1b[0m\n"),
            "FAIL\n"
        );
    }

    #[test]
    fn zero_failed_summary_is_green_not_red() {
        let text = "test result: ok. 12 passed; 0 failed; 0 ignored";
        let m = markup(text, Palette::Tests);
        assert!(m.contains("#b5e890"), "{m}");
        assert!(!m.contains("#ff7b86"), "{m}");
    }
    #[test]
    fn test_results_colours_match_pass_and_fail() {
        let m = markup("test one ... ok\ntest two ... FAILED", Palette::Tests);
        assert!(m.contains("#b5e890"));
        assert!(m.contains("#ff7b86"));
    }
}
