//! Conservative, dependency-aware Hydrogen diagnostics.
//!
//! This is not a type checker or language server. Unused suggestions are
//! deliberately labelled *possible*, and are emitted only when the complete
//! selected project's source/reference graph could be loaded. Public APIs,
//! entry points and referenced library declarations are never classified as
//! unused by this heuristic.
use crate::{
    editor::{self, TokenKind},
    project,
};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Hint,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub start: usize, // UTF-8 byte offset (inclusive)
    pub end: usize,   // UTF-8 byte offset (exclusive)
    pub line: usize,  // one based
    pub severity: Severity,
    pub message: String,
}

pub type ByFile = HashMap<PathBuf, Vec<Diagnostic>>;

#[derive(Debug)]
pub struct ScanResult {
    pub revision: u64,
    pub by_file: ByFile,
    pub files_scanned: usize,
    pub complete: bool,
}

#[derive(Clone)]
struct Token {
    word: String,
    kind: TokenKind,
    start: usize,
    end: usize,
    line: usize,
    paren_depth: usize,
}

fn significant_tokens(text: &str, diagnostics: &mut Vec<Diagnostic>) -> Vec<Token> {
    let mut tokens = Vec::new();
    let (mut byte, mut line, mut parens) = (0, 1, 0_usize);
    for (kind, fragment) in editor::tokenize(text) {
        let start = byte;
        let at_line = line;
        byte += fragment.len();
        line += fragment.as_bytes().iter().filter(|b| **b == b'\n').count();
        if kind == TokenKind::Comment && fragment.starts_with("/*") && !fragment.ends_with("*/") {
            diagnostics.push(Diagnostic {
                start,
                end: byte,
                line: at_line,
                severity: Severity::Error,
                message: "Unclosed block comment. Expected */ before end of file.".into(),
            });
        }
        if kind == TokenKind::String && (fragment.starts_with('"') || fragment.starts_with('\'')) {
            let quote = fragment.chars().next().unwrap_or('"');
            if fragment.chars().count() < 2 || !fragment.ends_with(quote) || {
                let mut escape_count = 0_usize;
                for ch in fragment[..fragment.len().saturating_sub(quote.len_utf8())]
                    .chars()
                    .rev()
                {
                    if ch == '\\' {
                        escape_count += 1;
                    } else {
                        break;
                    }
                }
                escape_count % 2 == 1
            } {
                diagnostics.push(Diagnostic {
                    start,
                    end: byte,
                    line: at_line,
                    severity: Severity::Error,
                    message: "Unterminated string or character literal.".into(),
                });
            }
        }
        if kind != TokenKind::Comment && kind != TokenKind::String && !fragment.trim().is_empty() {
            tokens.push(Token {
                word: fragment.to_owned(),
                kind,
                start,
                end: byte,
                line: at_line,
                paren_depth: parens,
            });
            match fragment {
                "(" => parens += 1,
                ")" => parens = parens.saturating_sub(1),
                _ => {}
            }
        }
    }
    tokens
}

fn is_identifier(t: &Token) -> bool {
    t.kind == TokenKind::Plain
        && t.word
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
        && t.word.chars().all(|c| c.is_alphanumeric() || c == '_')
}
fn is_type(t: &Token) -> bool {
    t.kind == TokenKind::Type || is_identifier(t)
}
fn is_boundary(s: &str) -> bool {
    matches!(s, ";" | "{" | "}" | ")")
}

// Identify an enclosing method body (not merely two nested namespace/class
// braces). Otherwise a namespaced class field could be mistaken for a local.
fn inside_method(tokens: &[Token], index: usize) -> bool {
    let mut closed_braces = 0_usize;
    for open in (0..index).rev() {
        match tokens[open].word.as_str() {
            "}" => closed_braces += 1,
            "{" => {
                if closed_braces > 0 {
                    closed_braces -= 1;
                    continue;
                }
                if open < 2 || tokens[open - 1].word != ")" {
                    continue;
                }
                let mut closed_parens = 0_usize;
                for start in (0..open).rev() {
                    match tokens[start].word.as_str() {
                        ")" => closed_parens += 1,
                        "(" => {
                            closed_parens -= 1;
                            if closed_parens == 0 {
                                if start < 2 {
                                    break;
                                }
                                let name = &tokens[start - 1];
                                let ret = &tokens[start - 2];
                                if is_identifier(name) && is_type(ret) {
                                    return true;
                                }
                                break;
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    false
}

#[derive(Clone)]
struct Candidate {
    name: String,
    start: usize,
    end: usize,
    line: usize,
    kind: &'static str,
}

/// Heuristic: private methods and simple typed local variables/fields only.
/// It deliberately misses complex declarations rather than inventing warnings.
fn candidates(tokens: &[Token]) -> Vec<Candidate> {
    let mut found = Vec::new();
    for i in 1..tokens.len() {
        let name = &tokens[i];
        if !is_identifier(name) || matches!(name.word.as_str(), "Main" | "Dispose" | "Finalize") {
            continue;
        }
        let previous = &tokens[i - 1];
        if !is_type(previous) {
            continue;
        }
        let next = tokens.get(i + 1).map(|t| t.word.as_str());
        if !matches!(next, Some("(") | Some("=") | Some(";")) {
            continue;
        }
        if i >= 2 && tokens[i - 2].word == "." {
            continue;
        }
        let mut back = i - 1;
        while back > 0 && !is_boundary(&tokens[back - 1].word) {
            back -= 1;
        }
        let declaration_prefix = &tokens[back..i];
        if declaration_prefix.iter().any(|t| {
            matches!(
                t.word.as_str(),
                "public"
                    | "protected"
                    | "internal"
                    | "override"
                    | "extern"
                    | "abstract"
                    | "delegate"
                    | "return"
                    | "new"
                    | "using"
                    | "namespace"
            )
        }) {
            continue;
        }
        let private = declaration_prefix.iter().any(|t| t.word == "private");
        let kind = if next == Some("(") {
            if !private {
                continue;
            }
            "method"
        } else if private {
            "field"
        } else {
            // Only analyse simple local declarations within a method body.
            // Only a genuine enclosing method body makes this a local.
            if name.paren_depth != 0
                || !inside_method(tokens, i)
                || !is_boundary_context(tokens, back)
            {
                continue;
            }
            "local variable"
        };
        found.push(Candidate {
            name: name.word.clone(),
            start: name.start,
            end: name.end,
            line: name.line,
            kind,
        });
    }
    found
}

fn is_boundary_context(tokens: &[Token], index: usize) -> bool {
    index == 0 || matches!(tokens[index - 1].word.as_str(), ";" | "{" | "}")
}

fn visit_warnings(p: &project::Project) -> bool {
    let invalid = p.warnings.iter().any(|w| {
        w.contains("Cannot read")
            || w.contains("Invalid .hyproj")
            || w.contains("Missing source")
            || w.contains("Incomplete project graph")
    });
    invalid || p.references.iter().any(visit_warnings)
}

/// All source files in the selected project AND its referenced projects are
/// searched; open unsaved buffers take precedence over on-disk copies.
/// Incomplete projects suppress unused-code hints rather than making guesses.
pub fn scan(revision: u64, selected: Option<&Path>, open_docs: &[(PathBuf, String)]) -> ScanResult {
    let mut by_file = HashMap::new();
    let mut paths = Vec::new();
    let mut complete = false;
    if let Some(manifest) = selected {
        let project = project::load(manifest);
        complete = !visit_warnings(&project);
        project::collect_sources(&project, &mut paths);
    }
    // Every open buffer gets lexical diagnostics. Only actual project members
    // participate in unused-reference counts and candidate analysis.
    let project_paths: std::collections::HashSet<_> = paths.iter().cloned().collect();
    paths.extend(open_docs.iter().map(|(path, _)| path.clone()));
    paths.sort();
    paths.dedup();
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut candidates_by_file = Vec::new();
    for path in paths {
        if path.extension().and_then(|e| e.to_str()) != Some("hy") {
            continue;
        }
        let loaded = open_docs
            .iter()
            .find(|(p, _)| p == &path)
            .map(|(_, text)| text.clone())
            .or_else(|| fs::read_to_string(&path).ok());
        let Some(source) = loaded else {
            complete = false;
            continue;
        };
        let mut diagnostics = Vec::new();
        let tokens = significant_tokens(&source, &mut diagnostics);
        for token in &tokens {
            if project_paths.contains(&path) && is_identifier(token) {
                *counts.entry(token.word.clone()).or_default() += 1;
            }
        }
        if project_paths.contains(&path) {
            candidates_by_file.push((path.clone(), candidates(&tokens)));
        }
        by_file.insert(path, diagnostics);
    }
    if complete {
        for (path, candidates) in candidates_by_file {
            for candidate in candidates {
                if counts.get(&candidate.name) != Some(&1) {
                    continue;
                }
                by_file.entry(path.clone()).or_default().push(Diagnostic {
                    start: candidate.start, end: candidate.end, line: candidate.line,
                    severity: if candidate.kind == "method" { Severity::Warning } else { Severity::Hint },
                    message: format!("Possibly unused {} '{}': no references were found in this project or its dependencies. This is a heuristic; generated code, reflection and external callers cannot be detected.",candidate.kind,candidate.name),
                });
            }
        }
    }
    let files_scanned = by_file.len();
    for diagnostics in by_file.values_mut() {
        diagnostics.sort_by_key(|d| (d.line, d.start));
    }
    ScanResult {
        revision,
        by_file,
        files_scanned,
        complete,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn temporary(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("quark-diag-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }
    #[test]
    fn private_unused_and_referenced_across_projects() {
        let root = temporary("project");
        fs::write(
            root.join("app.hyproj"),
            "name='App'\nsources=['main.hy','other.hy']\n",
        )
        .unwrap();
        fs::write(root.join("main.hy"),"class App { private int Unused() { int local = 1; return 1; } private int Used() { return 1; } }").unwrap();
        fs::write(
            root.join("other.hy"),
            "class Caller { void Test() { App.Used(); } }",
        )
        .unwrap();
        let result = scan(1, Some(&root.join("app.hyproj")), &[]);
        let hints: Vec<_> = result.by_file[&root.join("main.hy")]
            .iter()
            .filter(|d| d.severity != Severity::Error)
            .collect();
        assert!(result.complete);
        assert!(hints.iter().any(|d| d.message.contains("Unused")));
        assert!(hints.iter().any(|d| d.message.contains("local")));
        assert!(!hints.iter().any(|d| d.message.contains("Used'")));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn references_in_hyproj_dependencies_prevent_unused_hint() {
        let root = temporary("references");
        fs::create_dir_all(root.join("lib")).unwrap();
        fs::write(
            root.join("app.hyproj"),
            "sources=['app.hy']\nproject_references=['lib/lib.hyproj']\n",
        )
        .unwrap();
        fs::write(
            root.join("app.hy"),
            "class App { private int Called() { return 0; } }",
        )
        .unwrap();
        fs::write(root.join("lib/lib.hyproj"), "sources=['library.hy']\n").unwrap();
        fs::write(
            root.join("lib/library.hy"),
            "class Lib { void Run() { App.Called(); } }",
        )
        .unwrap();
        let r = scan(7, Some(&root.join("app.hyproj")), &[]);
        assert!(r.complete);
        assert_eq!(r.files_scanned, 2);
        assert!(r.by_file[&root.join("app.hy")]
            .iter()
            .all(|d| !d.message.contains("'Called'")));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn incomplete_project_does_not_guess_about_unused() {
        let root = temporary("missing");
        fs::write(root.join("a.hyproj"), "sources=['a.hy','missing.hy']\n").unwrap();
        fs::write(root.join("a.hy"), "class A { private void Hidden() {} }").unwrap();
        let r = scan(1, Some(&root.join("a.hyproj")), &[]);
        assert!(!r.complete);
        assert!(r
            .by_file
            .values()
            .flatten()
            .all(|d| !matches!(d.severity, Severity::Hint | Severity::Warning)));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn namespaced_fields_without_private_modifier_are_not_locals() {
        let root = temporary("fields");
        fs::write(root.join("p.hyproj"), "sources=['p.hy']\n").unwrap();
        fs::write(
            root.join("p.hy"),
            "namespace N { class C { int ExternallyVisible; void M() { int unusedLocal = 2; } } }",
        )
        .unwrap();
        let r = scan(1, Some(&root.join("p.hyproj")), &[]);
        let messages: Vec<_> = r
            .by_file
            .values()
            .flatten()
            .map(|d| d.message.as_str())
            .collect();
        assert!(!messages.iter().any(|m| m.contains("ExternallyVisible")));
        assert!(messages.iter().any(|m| m.contains("unusedLocal")));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn lexical_error_and_unicode_byte_span() {
        let root = temporary("utf8");
        let path = root.join("demo.hy");
        let code = "// π ✓\nstring x = \"unterminated";
        let r = scan(1, None, &[(path.clone(), code.into())]);
        let diags = &r.by_file[&path];
        assert!(diags
            .iter()
            .any(|d| d.severity == Severity::Error && d.line == 2));
        for diag in diags {
            assert!(code.is_char_boundary(diag.start) && code.is_char_boundary(diag.end));
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn ignores_comment_and_string_references() {
        let root = temporary("tokens");
        fs::write(root.join("p.hyproj"), "sources=['p.hy']\n").unwrap();
        fs::write(
            root.join("p.hy"),
            "class P { private void Hidden() { string v = \"Hidden\"; /* Hidden */ } }",
        )
        .unwrap();
        let r = scan(1, Some(&root.join("p.hyproj")), &[]);
        assert!(r.by_file[&root.join("p.hy")]
            .iter()
            .any(|d| d.message.contains("'Hidden'")));
        fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod open_buffer_tests {
    use super::*;
    #[test]
    fn unrelated_and_untitled_buffers_get_errors_with_a_project_selected() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("root.hyproj");
        fs::write(&p, "name='empty'\nsources=[]").unwrap();
        let outside = dir.path().join("outside.hy");
        let untitled = PathBuf::from("quark-untitled-99.hy");
        let result = scan(
            1,
            Some(&p),
            &[
                (outside.clone(), "/*".into()),
                (untitled.clone(), "\"unterminated".into()),
            ],
        );
        assert_eq!(result.by_file[&outside][0].severity, Severity::Error);
        assert_eq!(result.by_file[&untitled][0].severity, Severity::Error);
    }
}
