//! AppKit syntax colouring for Hydrogen source and HYUI XML.
//! Uses the same tokeniser as Quark's GTK editor, but converts offsets to
//! UTF-16, the indexing scheme used by NSTextView and Cocoa text storage.
#![cfg(target_os = "macos")]

use crate::hydrogen_lexer::{self, TokenKind};
use objc2_app_kit::{NSColor, NSFont};
use objc2_foundation::NSRange;

fn color(kind: TokenKind) -> Option<(f64, f64, f64)> {
    Some(match kind {
        TokenKind::Keyword | TokenKind::Modifier => (0.70, 0.59, 0.98),
        TokenKind::Type => (0.46, 0.78, 0.91),
        TokenKind::Literal | TokenKind::Number => (0.90, 0.71, 0.49),
        TokenKind::String => (0.65, 0.84, 0.55),
        TokenKind::Comment => (0.62, 0.65, 0.70),
        TokenKind::Operator => (0.77, 0.78, 0.82),
        _ => return None,
    })
}

/// The range conversion is deliberately UTF-16 rather than byte or Unicode
/// scalar indexing. Emoji and non-BMP identifiers would otherwise corrupt
/// AppKit attribute ranges.
fn token_spans(source: &str) -> Vec<(TokenKind, usize, usize)> {
    let mut position = 0;
    hydrogen_lexer::tokenize(source)
        .into_iter()
        .filter_map(|(kind, text)| {
            let begin = position;
            position += text.encode_utf16().count();
            color(kind).map(|_| (kind, begin, position))
        })
        .collect()
}

fn xml_spans(source: &str) -> Vec<(TokenKind, usize, usize)> {
    let mut spans = Vec::new();
    let mut cursor = 0;
    let chars: Vec<char> = source.chars().collect();
    let mut utf16 = vec![0; chars.len() + 1];
    for (i, ch) in chars.iter().enumerate() {
        utf16[i + 1] = utf16[i] + ch.len_utf16();
    }
    while cursor < chars.len() {
        if chars[cursor] != '<' {
            cursor += 1;
            continue;
        }
        if chars[cursor..].starts_with(&['<', '!', '-', '-']) {
            let start = cursor;
            cursor += 4;
            while cursor + 2 < chars.len() && chars[cursor..cursor + 3] != ['-', '-', '>'] {
                cursor += 1;
            }
            cursor = (cursor + 3).min(chars.len());
            spans.push((TokenKind::Comment, utf16[start], utf16[cursor]));
            continue;
        }
        cursor += 1;
        if cursor < chars.len() && chars[cursor] == '/' { cursor += 1; }
        let name = cursor;
        while cursor < chars.len()
            && (chars[cursor].is_alphanumeric() || matches!(chars[cursor], '_' | '-' | ':'))
        { cursor += 1; }
        if name < cursor {
            spans.push((TokenKind::Keyword, utf16[name], utf16[cursor]));
        }
        while cursor < chars.len() && chars[cursor] != '>' {
            if matches!(chars[cursor], '"' | '\'') {
                let start = cursor;
                let quote = chars[cursor];
                cursor += 1;
                while cursor < chars.len() && chars[cursor] != quote { cursor += 1; }
                if cursor < chars.len() { cursor += 1; }
                spans.push((TokenKind::String, utf16[start], utf16[cursor]));
            } else if chars[cursor].is_alphabetic() {
                let start = cursor;
                while cursor < chars.len() &&
                    (chars[cursor].is_alphanumeric() || matches!(chars[cursor], '-' | '_' | ':'))
                { cursor += 1; }
                spans.push((TokenKind::Type, utf16[start], utf16[cursor]));
            } else { cursor += 1; }
        }
        if cursor < chars.len() { cursor += 1; }
    }
    spans
}

/// Attribute updates do not replace NSTextView's content or selection and
/// consequently preserve the edit caret, undo history and native IME.
pub fn highlight(editor: &naui::TextArea, text: &str, xml: bool) {
    // Don't freeze the UI when generated source exceeds half a megabyte.
    if text.len() > 512 * 1024 { return; }
    let view = editor.native_text_view();
    view.setFont(Some(&NSFont::monospacedSystemFontOfSize_weight(12.0, 0.0)));
    let Some(storage) = (unsafe { view.textStorage() }) else { return; };
    let length = storage.length();
    // set_text() may have replaced the buffer since the event was queued.
    if length != text.encode_utf16().count() { return; }
    let full = NSRange::new(0, length);
    let dark = view.effectiveAppearance().name().to_string().contains("Dark");
    let base = if dark { NSColor::textColor() } else { NSColor::textColor() };
    view.setTextColor_range(Some(&base), full);
    let spans = if xml { xml_spans(text) } else { token_spans(text) };
    storage.beginEditing();
    for (kind, begin, end) in spans {
        if end <= begin || end > length { continue; }
        let Some((r, g, b)) = color(kind) else { continue; };
        let (r, g, b) = if dark { (r, g, b) } else {
            ((r * 0.65).max(0.1), (g * 0.65).max(0.1), (b * 0.72).max(0.1))
        };
        let shade = NSColor::colorWithSRGBRed_green_blue_alpha(r, g, b, 1.0);
        view.setTextColor_range(Some(&shade), NSRange::new(begin, end - begin));
    }
    storage.endEditing();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn token_ranges_count_utf16_not_utf8() {
        let spans = token_spans("let emoji = \"😀\"; let count = 2;");
        assert!(spans.iter().any(|(kind, begin, end)| {
            *kind == TokenKind::Number && *end > *begin
        }));
    }
    #[test]
    fn xml_text_has_distinct_tag_attribute_and_string_spans() {
        let spans = xml_spans("<Window title=\"Hello\" width='500'/>");
        assert!(spans.iter().any(|(k, _, _)| *k == TokenKind::Keyword));
        assert!(spans.iter().any(|(k, _, _)| *k == TokenKind::Type));
        assert!(spans.iter().any(|(k, _, _)| *k == TokenKind::String));
    }
}
