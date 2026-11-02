//! GTK syntax-coloured editing for Hydrogen and Hydrogen UI XML.
//! Quark's shared lexer remains the source of truth for Hydrogen tokens.
#![cfg(target_os = "linux")]

use gtk4::prelude::*;
use naui::Widget as _;

use crate::hydrogen_lexer::{self, TokenKind};

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
    fn color(self, dark: bool) -> &'static str {
        if dark {
            match self {
                Self::Keyword | Self::Modifier | Self::Tag => "#b49cfa",
                Self::Type | Self::Attribute => "#76c7e9",
                Self::Literal | Self::Number => "#e5b77e",
                Self::String => "#a6d58c",
                Self::Comment => "#9da4b2",
                Self::Operator => "#c9cbd1",
            }
        } else {
            // Light backgrounds need saturated, darker syntax colours.
            match self {
                Self::Keyword | Self::Modifier | Self::Tag => "#6334ac",
                Self::Type | Self::Attribute => "#086b94",
                Self::Literal | Self::Number => "#97500f",
                Self::String => "#24712c",
                Self::Comment => "#667085",
                Self::Operator => "#394150",
            }
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
    for (kind, token) in hydrogen_lexer::tokenize(source) {
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
    let dark = gtk4::Settings::default()
        .map(|s| s.is_gtk_application_prefer_dark_theme())
        .unwrap_or(false);
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
            table.add(&tag);
            tag
        };
        tag.set_property("foreground", kind.color(dark));
        buffer.apply_tag(
            &tag,
            &buffer.iter_at_offset(begin as i32),
            &buffer.iter_at_offset(end as i32),
        );
    }
}

/// GTK TextView's tab stops are pixel positions. The source editor uses a
/// monospaced 8px base cell for default text scaling and repeats that width.
fn editor_tab_stops(width: u8) -> gtk4::pango::TabArray {
    let mut stops = gtk4::pango::TabArray::new(1, true);
    stops.set_tab(
        0,
        gtk4::pango::TabAlign::Left,
        i32::from(width.clamp(1, 16)) * 8,
    );
    stops
}

fn configure_editor(view: &gtk4::TextView, preferences: &crate::native_settings::Editor) {
    view.set_wrap_mode(if preferences.word_wrap {
        gtk4::WrapMode::WordChar
    } else {
        gtk4::WrapMode::None
    });
    view.set_accepts_tab(true);
    view.set_tabs(&editor_tab_stops(preferences.tab_width));
}

/// Apply saved wrapping/tab stops initially, then reload if Settings changes
/// while the editor is open. The weak TextView ends polling on window close.
pub fn apply_editor_preferences(
    area: &naui::TextArea,
    preferences: &crate::native_settings::Editor,
) {
    let Some(view) = text_view(&area.native_widget()) else {
        return;
    };
    configure_editor(&view, preferences);
    let view = view.downgrade();
    let mut previous = preferences.clone();
    gtk4::glib::timeout_add_local(std::time::Duration::from_millis(400), move || {
        let Some(view) = view.upgrade() else {
            return gtk4::glib::ControlFlow::Break;
        };
        let file = crate::workspace_file().with_file_name("native-preferences.json");
        if let Ok(next) = crate::native_settings::try_load(&file) {
            if next.editor != previous {
                configure_editor(&view, &next.editor);
                previous = next.editor;
            }
        }
        gtk4::glib::ControlFlow::Continue
    });
}

pub fn install(area: &naui::TextArea, xml: bool) {
    if let Some(view) = text_view(&area.native_widget()) {
        view.set_monospace(true);
        view.set_wrap_mode(gtk4::WrapMode::None);
        let buffer = view.buffer();
        buffer.connect_changed(move |buffer| apply_highlight(buffer, xml));
        apply_highlight(&buffer, xml);
        if let Some(settings) = gtk4::Settings::default() {
            settings.connect_notify_local(
                Some("gtk-application-prefer-dark-theme"),
                move |_, _| {
                    apply_highlight(&buffer, xml);
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn editor_tab_width_controls_actual_pango_tab_stop() {
        assert_eq!(editor_tab_stops(4).tab(0).1, 32);
        assert_eq!(editor_tab_stops(8).tab(0).1, 64);
        assert_eq!(editor_tab_stops(0).tab(0).1, 8);
        assert_eq!(editor_tab_stops(100).tab(0).1, 128);
    }

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
