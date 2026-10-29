//! Lightweight namespace/type outline for Hydrogen source files.
//! This is a lexical explorer, not a full compiler parser. Ignore braces and
//! declarations inside strings/comments; let the compiler/LSP own semantics.
use crate::editor::{tokenize, TokenKind};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct SymbolItem {
    pub name: String,
    pub path: PathBuf,
    pub kind: &'static str,
}

#[derive(Debug, Clone, Default)]
pub struct SymbolNode {
    pub branches: BTreeMap<String, SymbolNode>,
    pub items: Vec<SymbolItem>,
}
impl SymbolNode {
    fn branch(&mut self, path: &[String]) -> &mut SymbolNode {
        let mut node = self;
        for part in path {
            node = node.branches.entry(part.clone()).or_default();
        }
        node
    }
    fn add(&mut self, path: &[String], item: SymbolItem) {
        self.branch(path).items.push(item);
    }
}

#[derive(Clone)]
enum Pending {
    Namespace(Vec<String>),
    Type(String),
}

enum Scope {
    Namespace(usize),
    Type,
    Other,
}

/// Each declaration becomes a selectable item under its namespace and any
/// containing types. Files without type declarations appear under namespace.
pub fn add_source(outline: &mut SymbolNode, path: &Path, source: &str) {
    let tokens: Vec<&str> = tokenize(source)
        .into_iter()
        .filter_map(|(kind, s)| {
            if kind == TokenKind::Comment || kind == TokenKind::String || s.trim().is_empty() {
                None
            } else {
                Some(s)
            }
        })
        .collect();
    let mut namespace: Vec<String> = vec![];
    let mut types: Vec<String> = vec![];
    let mut stack: Vec<Scope> = vec![];
    let mut pending: Option<Pending> = None;
    let mut found_type = false;
    let mut declared_namespace: Option<Vec<String>> = None;
    let mut idx = 0;
    while idx < tokens.len() {
        match tokens[idx] {
            "namespace" => {
                let mut name = String::new();
                let mut look = idx + 1;
                while look < tokens.len()
                    && (tokens[look] == "."
                        || tokens[look]
                            .chars()
                            .all(|c| c.is_alphanumeric() || c == '_'))
                {
                    name.push_str(tokens[look]);
                    look += 1;
                }
                let components: Vec<String> = name
                    .split('.')
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect();
                if !components.is_empty() {
                    if tokens.get(look) == Some(&";") {
                        namespace.extend(components);
                        declared_namespace = Some(namespace.clone());
                        idx = look;
                    } else if tokens.get(look) == Some(&"{") {
                        pending = Some(Pending::Namespace(components));
                        let mut full = namespace.clone();
                        if let Some(Pending::Namespace(parts)) = &pending {
                            full.extend(parts.iter().cloned());
                        }
                        declared_namespace = Some(full);
                        idx = look.saturating_sub(1);
                    }
                }
            }
            "class" | "struct" | "interface" | "enum" => {
                if let Some(name) = tokens.get(idx + 1) {
                    if name
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_alphabetic() || c == '_')
                        && name.chars().all(|c| c.is_alphanumeric() || c == '_')
                    {
                        let mut branch = namespace.clone();
                        branch.extend(types.iter().cloned());
                        let kind: &'static str = match tokens[idx] {
                            "class" => "class",
                            "struct" => "struct",
                            "interface" => "interface",
                            "enum" => "enum",
                            _ => unreachable!(),
                        };
                        outline.add(
                            &branch,
                            SymbolItem {
                                name: (*name).to_owned(),
                                path: path.to_path_buf(),
                                kind,
                            },
                        );
                        pending = Some(Pending::Type((*name).to_owned()));
                        found_type = true;
                        idx += 1;
                    }
                }
            }
            "{" => match pending.take() {
                Some(Pending::Namespace(parts)) => {
                    let count = parts.len();
                    namespace.extend(parts);
                    stack.push(Scope::Namespace(count));
                }
                Some(Pending::Type(name)) => {
                    types.push(name);
                    stack.push(Scope::Type);
                }
                None => stack.push(Scope::Other),
            },
            "}" => {
                match stack.pop() {
                    Some(Scope::Namespace(count)) => {
                        for _ in 0..count {
                            namespace.pop();
                        }
                    }
                    Some(Scope::Type) => {
                        types.pop();
                    }
                    _ => {}
                }
                pending = None;
            }
            ";" => {
                pending = None;
            }
            _ => {}
        }
        idx += 1;
    }
    if !found_type {
        let file_name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        outline.add(
            declared_namespace.as_deref().unwrap_or(&namespace),
            SymbolItem {
                name: file_name,
                path: path.to_path_buf(),
                kind: "file",
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn groups_namespace_and_class() {
        let mut tree = SymbolNode::default();
        add_source(
            &mut tree,
            Path::new("IO.hy"),
            "namespace B.Files { public class IO { string x = \"class Fake {}\"; } }",
        );
        assert_eq!(tree.branches["B"].branches["Files"].items[0].name, "IO");
        assert_eq!(tree.branches["B"].branches["Files"].items.len(), 1);
    }
    #[test]
    fn no_class_files_are_kept_in_namespace() {
        let mut tree = SymbolNode::default();
        add_source(
            &mut tree,
            Path::new("Free.hy"),
            "namespace A.B; // no class",
        );
        assert_eq!(tree.branches["A"].branches["B"].items[0].name, "Free.hy");
    }
    #[test]
    fn handles_nested_types_and_multiple_files() {
        let mut tree = SymbolNode::default();
        add_source(
            &mut tree,
            Path::new("a.hy"),
            "namespace X { public class Outer { class Inner {} } }",
        );
        add_source(
            &mut tree,
            Path::new("b.hy"),
            "namespace X { class Other {} }",
        );
        assert_eq!(tree.branches["X"].items.len(), 2);
        assert_eq!(tree.branches["X"].branches["Outer"].items[0].name, "Inner");
    }
}
