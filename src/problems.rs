//! Structured, clickable diagnostics extracted from Hydrogen compiler output.
//! These are intentionally separate from the editor's heuristic quick checks.
//! Only explicit file/line error and warning messages become Problems.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub path: PathBuf,
    pub line: usize,
    pub column: Option<usize>,
    pub severity: Severity,
    pub message: String,
}

fn location(text: &str) -> Option<(&str, usize, Option<usize>)> {
    // Windows/MSVC-style: source.hy(12,5): error: message
    if let Some((file, suffix)) = text.rsplit_once('(') {
        if let Some(coords) = suffix.strip_suffix(')') {
            let mut parts = coords.split(',');
            let line = parts.next()?.trim().parse::<usize>().ok()?;
            let column = parts
                .next()
                .map(|value| value.trim().parse::<usize>())
                .transpose()
                .ok()?;
            if parts.next().is_none() && line > 0 && column.is_none_or(|col| col > 0) {
                return Some((file.trim(), line, column));
            }
        }
    }
    // GCC/Clang-style: source.hy:12:5: error: message, or source.hy:12: error:...
    // Split from the right so Windows drive-letter colons are retained.
    let (left, last) = text.rsplit_once(':')?;
    let value = last.trim().parse::<usize>().ok()?;
    if let Some((file, line)) = left.rsplit_once(':') {
        if let Ok(line) = line.trim().parse::<usize>() {
            if line > 0 && value > 0 {
                return Some((file.trim(), line, Some(value)));
            }
        }
    }
    (value > 0).then_some((left.trim(), value, None))
}

fn parse_line(raw: &str, cwd: &Path) -> Option<Problem> {
    let line = raw.trim();
    let lower = line.to_ascii_lowercase();
    let (position, marker, severity) = [(": error:", Severity::Error), (": warning:", Severity::Warning)]
        .into_iter()
        .find_map(|(marker, severity)| {
            lower.find(marker).map(|position| (position, marker, severity))
        })?;
    let (file, line_number, column) = location(&line[..position])?;
    // Keep unrelated program logs and terminal output out of the Error List.
    let file_lower = file.to_ascii_lowercase();
    if file.is_empty() || !(file_lower.ends_with(".hy") || file_lower.ends_with(".hyproj")) {
        return None;
    }
    let message = line[position + marker.len()..].trim();
    if message.is_empty() {
        return None;
    }
    let path = Path::new(file);
    Some(Problem {
        path: if path.is_absolute() {
            path.to_path_buf()
        } else {
            cwd.join(path)
        },
        line: line_number,
        column,
        severity,
        message: message.to_owned(),
    })
}

/// Parse the bounded output of the latest command, not an accumulated history
/// of commands run from potentially different working directories.
pub fn parse(output: &str, cwd: &Path) -> Vec<Problem> {
    const MAX_PROBLEMS: usize = 1000;
    output
        .lines()
        .filter_map(|line| parse_line(line, cwd))
        .take(MAX_PROBLEMS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gcc_style_errors_and_warnings() {
        let cwd = Path::new("/workspace");
        let errors = parse(
            "src/Main.hy:12:5: error: Unknown identifier\nlib/util.hy:17: warning: Unused variable",
            cwd,
        );
        assert_eq!(errors.len(), 2);
        assert_eq!(errors[0].path, cwd.join("src/Main.hy"));
        assert_eq!(errors[0].line, 12);
        assert_eq!(errors[0].column, Some(5));
        assert_eq!(errors[0].message, "Unknown identifier");
        assert_eq!(errors[1].severity, Severity::Warning);
        assert_eq!(errors[1].column, None);
    }

    #[test]
    fn parses_parenthesised_positions_and_manifest_errors() {
        let errors = parse(
            "app.hy(3,9): error: Expected token\napp.hyproj(8): warning: Missing reference",
            Path::new("."),
        );
        assert_eq!(errors.len(), 2);
        assert_eq!((errors[0].line, errors[0].column), (3, Some(9)));
        assert_eq!((errors[1].line, errors[1].column), (8, None));
    }

    #[test]
    fn accepts_windows_drive_colons_without_confusing_line_numbers() {
        let errors = parse(r"C:\Projects\My App\Main.hy:29:4: error: Bad value", Path::new("."));
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line, 29);
        assert_eq!(errors[0].column, Some(4));
    }

    #[test]
    fn avoids_unrelated_messages_and_zero_positions() {
        let errors = parse(
            "error: generic failure\nfoo.rs:12:3: error: not Hydrogen\na.hy:0:1: error: invalid position\na.hy:2: warning: valid",
            Path::new("."),
        );
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].line, 2);
    }

    #[test]
    fn caps_number_of_problems() {
        let output = "a.hy:2: error: Broken\n".repeat(1200);
        assert_eq!(parse(&output, Path::new(".")).len(), 1000);
    }
}
