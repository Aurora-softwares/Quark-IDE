//! GTK syntax-coloured editing for Hydrogen and Hydrogen UI XML.
//! Quark's shared lexer remains the source of truth for Hydrogen tokens.
#![cfg(target_os = "linux")]

use gtk4::prelude::*;
use naui::Widget as _;

use crate::editor::{self, TokenKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Syntax {
    Keyword,
    Modifier,
    Type,
    Literal,
    String,
    Comment,
    Number,
    Operator,
    Tag,
    Attribute,
}

impl Syntax {
    fn color(self) -> &'static str {
        match self {
            Self::Keyword | Self::Modifier | Self::Tag => "#b49cfa",
            Self::Type | Self::Attribute => "#76c7e9",
            Self::Literal | Self::Number => "#e5b77e",
            Self::String => "#a6d58c",
            Self::Comment => "#888e99",
            Self::Operator => "#c9cbd1",
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Keyword => "quark-keyword",
            Self::Modifier => "quark-modifier",
            Self::Type => "quark-type",
            Self::Literal => "quark-literal",
            Self::String => "quark-string",
            Self::Comment => "quark-comment",
            Self::Number => "quark-number",
            Self::Operator => "quark-operator",
            Self::Tag => "quark-xml-tag",
            Self::Attribute => "quark-xml-attribute",
        }
    }
}

fn syntax_kind(kind: TokenKind) -> Option<Syntax> {
    match kind {
        TokenKind::Keyword => Some(Syntax::Keyword),
        TokenKind::Modifier => Some(Syntax::Modifier),
        TokenKind::Type => Some(Syntax::Type),
        TokenKind::Literal => Some(Syntax::Literal),
        TokenKind::String => Some(Syntax::String),
        TokenKind::Comment => Some(Syntax::Comment),
        TokenKind::Number => Some(Syntax::Number),
        TokenKind::Operator => Some(Syntax::Operator),
        _ => None,
    }
}

fn hydrogen_spans(source: &str) -> Vec<(Syntax, usize, usize)> {
    let mut start = 0;
    let mut spans = Vec::new();
    for (kind, token) in editor::tokenize(source) {
        let end = start + token.chars().count();
        if let Some(style) = syntax_kind(kind) {
            spans.push((style, start, end));
        }
        start = end;
    }
    spans
}

/// UTF-8-safe XML highlighter: tags, attributes, quoted strings and comments.
/// The output is byte-independent so GTK's character offsets stay correct.
fn xml_spans(source: &str) -> Vec<(Syntax, usize, usize)> {
    let chars: Vec<char> = source.chars().collect();
    let mut spans = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '<' {
            i += 1;
            continue;
        }
        let begin = i;
        if chars[i..].starts_with(&['<', '!', '-', '-']) {
            i += 4;
            while i + 2 < chars.len() && chars[i..i + 3] != ['-', '-', '>'] {
                i += 1;
            }
            i = (i + 3).min(chars.len());
            spans.push((Syntax::Comment, begin, i));
            continue;
        }
        i += 1;
        if i < chars.len() && chars[i] == '/' {
            i += 1;
        }
        let tag_start = i;
        while i < chars.len() && (chars[i].is_alphanumeric() || matches!(chars[i], '-' | '_' | ':'))
        {
            i += 1;
        }
        if tag_start != i {
            spans.push((Syntax::Tag, tag_start, i));
        }
        while i < chars.len() && chars[i] != '>' {
            if chars[i].is_whitespace() || matches!(chars[i], '/' | '?' | '=') {
                i += 1;
                continue;
            }
            if chars[i] == '"' || chars[i] == '\'' {
                let quote = chars[i];
                let start = i;
                i += 1;
                while i < chars.len() && chars[i] != quote {
                    i += 1;
                }
                if i < chars.len() {
                    i += 1;
                }
                spans.push((Syntax::String, start, i));
            } else {
                let start = i;
                while i < chars.len()
                    && !chars[i].is_whitespace()
                    && !matches!(chars[i], '=' | '>' | '/' | '"' | '\'')
                {
                    i += 1;
                }
                if i == start {
                    i += 1;
                } else {
                    spans.push((Syntax::Attribute, start, i));
                }
            }
        }
        i = (i + 1).min(chars.len());
    }
    spans
}

fn text_view(root: &gtk4::Widget) -> Option<gtk4::TextView> {
    if let Ok(view) = root.clone().downcast::<gtk4::TextView>() {
        return Some(view);
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(view) = text_view(&widget) {
            return Some(view);
        }
        child = widget.next_sibling();
    }
    None
}

fn apply_highlight(buffer: &gtk4::TextBuffer, xml: bool) {
    let text = buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), false)
        .to_string();
    // Avoid lengthy updates on enormous generated files.
    if text.len() > 512 * 1024 {
        return;
    }
    let table = buffer.tag_table();
    buffer.remove_all_tags(&buffer.start_iter(), &buffer.end_iter());
    let spans = if xml {
        xml_spans(&text)
    } else {
        hydrogen_spans(&text)
    };
    for (kind, begin, end) in spans {
        let tag = if let Some(tag) = table.lookup(kind.name()) {
            tag
        } else {
            let tag = gtk4::TextTag::new(Some(kind.name()));
            tag.set_property("foreground", kind.color());
            table.add(&tag);
            tag
        };
        buffer.apply_tag(
            &tag,
            &buffer.iter_at_offset(begin as i32),
            &buffer.iter_at_offset(end as i32),
        );
    }
}

pub fn install(area: &naui::TextArea, xml: bool) {
    if let Some(view) = text_view(&area.native_widget()) {
        view.set_monospace(true);
        view.set_wrap_mode(gtk4::WrapMode::None);
        let buffer = view.buffer();
        buffer.connect_changed(move |buffer| apply_highlight(buffer, xml));
        apply_highlight(&buffer, xml);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn xml_highlights_tag_attributes_and_escaped_strings() {
        let spans = xml_spans("<Window title=\"Hello &amp; world\" width='800'/>");
        assert!(spans.iter().any(|x| x.0 == Syntax::Tag));
        assert!(spans.iter().any(|x| x.0 == Syntax::Attribute));
        assert!(spans.iter().any(|x| x.0 == Syntax::String));
    }
    #[test]
    fn hydrogen_highlights_language_keywords() {
        let spans = hydrogen_spans("public class Program { string text = \"hello\"; }");
        assert!(spans
            .iter()
            .any(|x| x.0 == Syntax::Keyword || x.0 == Syntax::Modifier));
        assert!(spans.iter().any(|x| x.0 == Syntax::String));
    }
}
