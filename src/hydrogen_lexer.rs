//! UI-independent Hydrogen lexical support and completions shared by the
//! platform-native GTK4, AppKit and WinUI 3 code editors.
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

/// Do not offer code completions inside a comment or string token.
pub fn completion_context(source: &str, cursor_char: usize) -> bool {
    let byte = source
        .char_indices()
        .nth(cursor_char)
        .map_or(source.len(), |(b, _)| b);
    let mut end = 0;
    for (kind, fragment) in tokenize(source) {
        let start = end;
        end += fragment.len();
        if byte > start && byte <= end {
            return !matches!(kind, TokenKind::Comment | TokenKind::String);
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_lexer_reconstructs_unicode_source_exactly() {
        let source = "namespace Ω { string 名 = \"✅\"; } // note";
        assert_eq!(
            tokenize(source)
                .into_iter()
                .map(|(_, token)| token)
                .collect::<String>(),
            source
        );
    }
    #[test]
    fn native_completion_respects_utf8_caret_and_literal_context() {
        let mut source = "π st world".to_string();
        assert_eq!(prefix_at(&source, 4), "st");
        assert_eq!(insert_completion(&mut source, 4, "st", "static"), 8);
        assert_eq!(source, "π static world");
        assert!(!completion_context("\"str", 4));
    }
}
