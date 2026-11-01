//! Lightweight Hydrogen code symbol discovery for the native navigation pane.
//! This is intentionally syntax-oriented rather than a replacement for the compiler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    pub line: usize,
    pub kind: &'static str,
}

pub fn discover(source: &str) -> Vec<Symbol> {
    let mut result = Vec::new();
    for (index, raw) in source.lines().enumerate() {
        let trimmed = raw.trim();
        if trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with('*') {
            continue;
        }
        let tokens = trimmed.split_whitespace().collect::<Vec<_>>();
        if tokens.is_empty() {
            continue;
        }
        for (i, token) in tokens.iter().enumerate() {
            let kind = match *token {
                "namespace" => "Namespace",
                "class" | "struct" | "interface" | "enum" => "Type",
                "fn" | "func" | "function" => "Function",
                _ => continue,
            };
            if let Some(next) = tokens.get(i + 1) {
                let name = next.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
                    .next().unwrap_or("");
                if !name.is_empty() {
                    result.push(Symbol { name: name.into(), line: index + 1, kind });
                }
            }
            break;
        }
        // Hydrogen methods may also use explicit return types, e.g.
        // "public static int Main(...)".
        if result.last().is_some_and(|s| s.line == index + 1) {
            continue;
        }
        if let Some(open) = trimmed.find('(') {
            let head = trimmed[..open].trim();
            let name = head.rsplit_whitespace().next().unwrap_or("");
            if !name.is_empty()
                && name.chars().next().is_some_and(|ch| ch.is_alphabetic() || ch == '_')
                && !matches!(name, "if" | "for" | "while" | "switch" | "catch" | "return")
                && head.split_whitespace().count() >= 2
            {
                result.push(Symbol { name: name.to_owned(), line: index + 1, kind: "Method" });
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lists_hydrogen_namespace_types_methods_and_functions() {
        let src = "namespace Studio {\npublic class Program {\n  public static int Main(string[] args) {\n  }\n}\nfn helper() {}";
        let symbols = discover(src);
        assert!(symbols.iter().any(|s| s.name == "Studio" && s.line == 1));
        assert!(symbols.iter().any(|s| s.name == "Program" && s.kind == "Type"));
        assert!(symbols.iter().any(|s| s.name == "Main" && s.kind == "Method"));
        assert!(symbols.iter().any(|s| s.name == "helper" && s.kind == "Function"));
    }
    #[test]
    fn ignores_comments_and_control_statements() {
        let symbols = discover("// class Fake\nif (ready) {}\nwhile (true) {}");
        assert!(symbols.is_empty());
    }
}
