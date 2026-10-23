//! UI-independent Hydrogen lexical support and lightweight completion.
//! This matches the syntax used by the Hydrogen compiler samples; semantic
//! symbols and compiler diagnostics will eventually come from the LSP.
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
    "abstract",
    "sealed",
    "partial",
    "virtual",
    "override",
    "readonly",
    "const",
    "extern",
    "class",
    "struct",
    "interface",
    "enum",
    "delegate",
    "void",
    "int",
    "uint",
    "long",
    "ulong",
    "short",
    "ushort",
    "byte",
    "sbyte",
    "nint",
    "nuint",
    "bool",
    "string",
    "char",
    "float",
    "double",
    "decimal",
    "object",
    "return",
    "if",
    "else",
    "while",
    "do",
    "for",
    "foreach",
    "in",
    "switch",
    "case",
    "default",
    "break",
    "continue",
    "new",
    "true",
    "false",
    "null",
    "this",
    "base",
    "var",
    "is",
    "as",
    "try",
    "catch",
    "finally",
    "throw",
];
const MODIFIERS: &[&str] = &[
    "public",
    "private",
    "protected",
    "internal",
    "static",
    "abstract",
    "sealed",
    "partial",
    "virtual",
    "override",
    "readonly",
    "const",
    "extern",
];
const TYPES: &[&str] = &[
    "void", "int", "uint", "long", "ulong", "short", "ushort", "byte", "sbyte", "nint", "nuint",
    "bool", "string", "char", "float", "double", "decimal", "object", "var",
];
const LITERALS: &[&str] = &["true", "false", "null", "this", "base"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Plain,
    Keyword,
    Modifier,
    Type,
    Literal,
    String,
    Comment,
    Number,
    Operator,
    Punctuation,
}

fn ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}
fn ident_continue(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// UTF-8-safe lexer. Every returned slice is on a valid char boundary and
/// token concatenation exactly reproduces the input, including invalid code.
pub fn tokenize(source: &str) -> Vec<(TokenKind, &str)> {
    let mut spans = Vec::new();
    let mut i = 0;
    while i < source.len() {
        let tail = &source[i..];
        let (kind, n) = if tail.starts_with("//") {
            (
                TokenKind::Comment,
                tail.find('\n').unwrap_or(tail.len()).max(2),
            )
        } else if tail.starts_with("/*") {
            (
                TokenKind::Comment,
                tail.find("*/").map(|j| j + 2).unwrap_or(tail.len()),
            )
        } else if tail.starts_with('"') || tail.starts_with('\'') {
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
            (TokenKind::String, end.max(1))
        } else {
            let first = tail.chars().next().unwrap();
            if first.is_ascii_digit() {
                let n = if tail.starts_with("0x") || tail.starts_with("0X") {
                    2 + tail[2..]
                        .char_indices()
                        .take_while(|(_, c)| c.is_ascii_hexdigit() || *c == '_')
                        .map(|(_, c)| c.len_utf8())
                        .sum::<usize>()
                } else if tail.starts_with("0b") || tail.starts_with("0B") {
                    2 + tail[2..]
                        .char_indices()
                        .take_while(|(_, c)| *c == '0' || *c == '1' || *c == '_')
                        .map(|(_, c)| c.len_utf8())
                        .sum::<usize>()
                } else {
                    tail.char_indices()
                        .skip(1)
                        .find(|(_, c)| !c.is_ascii_alphanumeric() && *c != '_' && *c != '.')
                        .map(|(j, _)| j)
                        .unwrap_or(tail.len())
                };
                (TokenKind::Number, n)
            } else if ident_start(first) {
                let n = tail
                    .char_indices()
                    .skip(1)
                    .find(|(_, c)| !ident_continue(*c))
                    .map(|(j, _)| j)
                    .unwrap_or(tail.len());
                let ident = &tail[..n];
                let kind = if MODIFIERS.contains(&ident) {
                    TokenKind::Modifier
                } else if TYPES.contains(&ident) {
                    TokenKind::Type
                } else if LITERALS.contains(&ident) {
                    TokenKind::Literal
                } else if KEYWORDS.contains(&ident) {
                    TokenKind::Keyword
                } else {
                    TokenKind::Plain
                };
                (kind, n)
            } else if "{}[]();,.:".contains(first) {
                (TokenKind::Punctuation, first.len_utf8())
            } else if "+-*/%=!<>&|^~?".contains(first) {
                (TokenKind::Operator, first.len_utf8())
            } else {
                (TokenKind::Plain, first.len_utf8())
            }
        };
        spans.push((kind, &source[i..i + n]));
        i += n;
    }
    spans
}

pub fn highlight_hydrogen(text: &str, plain: Color32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let tokens = tokenize(text);
    let mut after_declaration = false;
    for (idx, &(kind, content)) in tokens.iter().enumerate() {
        let significant_next = tokens[idx + 1..]
            .iter()
            .find(|(_, s)| !s.trim().is_empty())
            .map(|(_, s)| *s);
        let declaration = matches!(
            content,
            "class" | "struct" | "interface" | "enum" | "namespace"
        );
        let declared_name = after_declaration && kind == TokenKind::Plain;
        let function_name = kind == TokenKind::Plain
            && significant_next == Some("(")
            && !matches!(content, "if" | "while" | "for" | "switch" | "catch");
        let color = if declared_name {
            Color32::from_rgb(125, 210, 207)
        } else if function_name {
            Color32::from_rgb(225, 195, 137)
        } else {
            match kind {
                TokenKind::Plain => plain,
                TokenKind::Keyword => Color32::from_rgb(176, 157, 235),
                TokenKind::Modifier => Color32::from_rgb(140, 178, 235),
                TokenKind::Type => Color32::from_rgb(106, 207, 193),
                TokenKind::Literal => Color32::from_rgb(223, 151, 168),
                TokenKind::Comment => Color32::from_rgb(116, 148, 127),
                TokenKind::String => Color32::from_rgb(212, 172, 121),
                TokenKind::Number => Color32::from_rgb(200, 162, 207),
                TokenKind::Operator => Color32::from_rgb(166, 194, 218),
                TokenKind::Punctuation => Color32::from_rgb(150, 159, 178),
            }
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
        if declaration {
            after_declaration = true;
        } else if after_declaration
            && (declared_name || !content.trim().is_empty() && content != ".")
        {
            after_declaration = false;
        }
    }
    job
}

/// Returns the identifier segment immediately before the caret (char index).
pub fn prefix_at(source: &str, cursor_char: usize) -> String {
    let before: String = source.chars().take(cursor_char).collect();
    let start = before
        .char_indices()
        .rev()
        .find(|(_, c)| !ident_continue(*c))
        .map(|(byte, c)| byte + c.len_utf8())
        .unwrap_or(0);
    before[start..].to_owned()
}

/// Replace only the prefix at the caret, not an unrelated occurrence or EOF.
/// Returns the new caret *character* index.
pub fn insert_completion(
    source: &mut String,
    cursor_char: usize,
    prefix: &str,
    candidate: &str,
) -> usize {
    let prefix_len = prefix.chars().count();
    let start_char = cursor_char.saturating_sub(prefix_len);
    let start_byte = source
        .char_indices()
        .nth(start_char)
        .map(|(i, _)| i)
        .unwrap_or(source.len());
    let end_byte = source
        .char_indices()
        .nth(cursor_char)
        .map(|(i, _)| i)
        .unwrap_or(source.len());
    if source.get(start_byte..end_byte) == Some(prefix) {
        source.replace_range(start_byte..end_byte, candidate);
        start_char + candidate.chars().count()
    } else {
        cursor_char
    }
}

/// Local keyword + identifier completion; a compiler LSP can replace this later.
pub fn completion_candidates(source: &str, prefix: &str) -> Vec<String> {
    let mut names: BTreeSet<String> = KEYWORDS.iter().map(|s| s.to_string()).collect();
    for (kind, s) in tokenize(source) {
        if kind == TokenKind::Plain
            && s.chars().next().is_some_and(ident_start)
            && s.chars().all(ident_continue)
        {
            names.insert(s.to_string());
        }
    }
    let prefix_lower = prefix.to_lowercase();
    let mut normalized = BTreeSet::new();
    names
        .into_iter()
        .filter(|s| {
            let lower = s.to_lowercase();
            s.len() > prefix.len() && lower.starts_with(&prefix_lower) && normalized.insert(lower)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn highlights_keywords_and_names() {
        let tokens = tokenize("public class Hello { }");
        assert!(tokens.contains(&(TokenKind::Modifier, "public")));
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
    #[test]
    fn numeric_and_modifiers() {
        let t = tokenize("static int x = 0xDEAD; bool y = true;");
        assert!(t.contains(&(TokenKind::Modifier, "static")));
        assert!(t.contains(&(TokenKind::Number, "0xDEAD")));
        assert!(t.contains(&(TokenKind::Literal, "true")));
    }
    #[test]
    fn completion_replaces_at_caret_unicode_safe() {
        let mut text = "π st world".to_string();
        assert_eq!(prefix_at(&text, 4), "st");
        let index = insert_completion(&mut text, 4, "st", "static");
        assert_eq!(index, 8);
        assert_eq!(text, "π static world");
    }
    #[test]
    fn completion_examples_include_all_three() {
        let results = completion_candidates("", "st");
        assert!(results.contains(&"static".to_string()));
        assert!(results.contains(&"string".to_string()));
        assert!(results.contains(&"struct".to_string()));
    }
}
