//! UI-independent Hydrogen lexical support. This is not a semantic language server.
use eframe::egui::{self, Color32};
use std::collections::BTreeSet;

pub const KEYWORDS: &[&str] = &[
    "namespace",
    "using",
    "public",
    "private",
    "protected",
    "internal",
    "static",
    "class",
    "struct",
    "interface",
    "enum",
    "void",
    "int",
    "long",
    "bool",
    "string",
    "byte",
    "char",
    "float",
    "double",
    "return",
    "if",
    "else",
    "while",
    "for",
    "break",
    "continue",
    "new",
    "true",
    "false",
    "null",
    "this",
    "base",
    "var",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Plain,
    Keyword,
    String,
    Comment,
    Number,
}

/// Lexes UTF-8 safely: byte boundaries always come from `char_indices`.
pub fn tokenize(source: &str) -> Vec<(TokenKind, &str)> {
    let mut spans = Vec::new();
    let mut i = 0;
    while i < source.len() {
        let tail = &source[i..];
        let kind;
        let n;
        if tail.starts_with("//") {
            kind = TokenKind::Comment;
            n = tail.find('\n').unwrap_or(tail.len());
        } else if tail.starts_with("/*") {
            kind = TokenKind::Comment;
            n = tail.find("*/").map(|j| j + 2).unwrap_or(tail.len());
        } else if tail.starts_with('"') || tail.starts_with('\'') {
            kind = TokenKind::String;
            let quote = tail.chars().next().unwrap();
            let mut escaped = false;
            let mut end = tail.len();
            for (offset, ch) in tail.char_indices().skip(1) {
                if escaped {
                    escaped = false;
                    continue;
                }
                if ch == '\\' {
                    escaped = true;
                    continue;
                }
                if ch == quote {
                    end = offset + ch.len_utf8();
                    break;
                }
                if ch == '\n' {
                    end = offset;
                    break;
                }
            }
            n = end.max(1);
        } else {
            let first = tail.chars().next().unwrap();
            if first.is_ascii_digit() {
                kind = TokenKind::Number;
                n = tail
                    .char_indices()
                    .skip(1)
                    .find(|(_, c)| !c.is_ascii_alphanumeric() && *c != '_')
                    .map(|(j, _)| j)
                    .unwrap_or(tail.len());
            } else if first.is_alphabetic() || first == '_' {
                n = tail
                    .char_indices()
                    .skip(1)
                    .find(|(_, c)| !c.is_alphanumeric() && *c != '_')
                    .map(|(j, _)| j)
                    .unwrap_or(tail.len());
                kind = if KEYWORDS.contains(&&tail[..n]) {
                    TokenKind::Keyword
                } else {
                    TokenKind::Plain
                };
            } else {
                kind = TokenKind::Plain;
                n = first.len_utf8();
            }
        }
        spans.push((kind, &source[i..i + n]));
        i += n;
    }
    spans
}

pub fn highlight_hydrogen(text: &str, plain: Color32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    for (kind, content) in tokenize(text) {
        let color = match kind {
            TokenKind::Plain => plain,
            TokenKind::Keyword => Color32::from_rgb(125, 175, 255),
            TokenKind::Comment => Color32::from_rgb(116, 160, 122),
            TokenKind::String => Color32::from_rgb(210, 165, 110),
            TokenKind::Number => Color32::from_rgb(180, 150, 220),
        };
        job.append(
            content,
            0.0,
            egui::TextFormat {
                font_id: egui::FontId::monospace(14.0),
                color,
                ..Default::default()
            },
        );
    }
    job
}

/// Local-only fallback until the Hydrogen LSP supplies semantic completion.
pub fn completion_candidates(source: &str, prefix: &str) -> Vec<String> {
    let mut names: BTreeSet<String> = KEYWORDS.iter().map(|s| s.to_string()).collect();

    for (kind, s) in tokenize(source) {
        if kind == TokenKind::Plain
            && s.chars()
                .next()
                .is_some_and(|c| c.is_alphabetic() || c == '_')
            && s.chars().all(|c| c.is_alphanumeric() || c == '_')
        {
            names.insert(s.to_string());
        }
    }

    let prefix_lower = prefix.to_lowercase();
    let mut seen = BTreeSet::new();

    names
        .into_iter()
        .filter(|s| {
            !prefix_lower.is_empty()
                && s.to_lowercase().starts_with(&prefix_lower)
                && seen.insert(s.to_lowercase())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn completion_handles_case_insensitive_duplicates() {
        let source = "Renderer renderer RENDERER Window window";

        let results = completion_candidates(source, "ren");

        assert_eq!(results, vec!["RENDERER"]);

        let windows = completion_candidates(source, "win");

        assert_eq!(windows, vec!["Window"]);
    }
    #[test]
    fn highlights_keywords_and_names() {
        let tokens = tokenize("public class Hello { }");
        assert!(tokens.contains(&(TokenKind::Keyword, "public")));
        assert!(tokens.contains(&(TokenKind::Keyword, "class")));
        assert!(tokens.contains(&(TokenKind::Plain, "Hello")));
    }
    #[test]
    fn comments_do_not_highlight_embedded_keywords() {
        assert_eq!(
            tokenize("// public class"),
            vec![(TokenKind::Comment, "// public class")]
        );
    }
    #[test]
    fn strings_and_escapes() {
        let tokens = tokenize("\"hello \\\"class\\\"\" // trailing");
        assert_eq!(tokens[0].0, TokenKind::String);
        assert_eq!(tokens.last().unwrap().0, TokenKind::Comment);
    }
    #[test]
    fn preserves_unicode_and_entire_input() {
        let sample = "π = \"✓\"; // 世界\n";
        assert_eq!(
            tokenize(sample).iter().map(|(_, s)| *s).collect::<String>(),
            sample
        );
    }
    #[test]
    fn completion_deduplicates_and_filters() {
        let words = completion_candidates("class Camera {} Camera camera", "Cam");
        assert_eq!(words, vec!["Camera"]);
        assert!(completion_candidates("class Camera", "zzzz").is_empty());
    }
    #[test]
    fn tokenizes_unterminated_comment_and_string() {
        assert_eq!(tokenize("/* open")[0].0, TokenKind::Comment);
        assert_eq!(tokenize("\"open")[0].0, TokenKind::String);
    }
}
