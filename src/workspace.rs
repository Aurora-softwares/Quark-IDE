//! Workspace filesystem work stays off the GUI thread. Results are immutable
//! snapshots, refreshed explicitly or when Quark changes workspace contents.
use crate::{project, symbols};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub directory: bool,
}
#[derive(Default)]
pub struct Snapshot {
    pub directories: HashMap<PathBuf, Vec<Entry>>,
    pub files: Vec<PathBuf>,
    pub projects: Vec<project::Project>,
    pub outlines: HashMap<PathBuf, symbols::SymbolNode>,
    pub notices: Vec<String>,
}

pub fn scan(
    roots: &[PathBuf],
    projects: &[PathBuf],
    buffers: &[(PathBuf, String)],
    include_generated: bool,
    limit: usize,
) -> Snapshot {
    let mut snapshot = Snapshot::default();
    let mut seen = HashSet::new();
    let mut remaining = limit;
    for root in roots {
        visit(
            root,
            &mut snapshot,
            &mut seen,
            include_generated,
            &mut remaining,
            0,
        );
    }
    if remaining == 0 {
        snapshot.notices.push(format!(
            "Workspace index limited to {limit} entries. Increase the limit to include more files."
        ));
    }
    for path in projects {
        let p = project::load(path);
        outline(&p, &mut snapshot, buffers);
        snapshot.projects.push(p);
    }
    snapshot.files.sort();
    snapshot.files.dedup();
    snapshot
}
fn outline(project: &project::Project, snapshot: &mut Snapshot, buffers: &[(PathBuf, String)]) {
    let mut node = symbols::SymbolNode::default();
    for path in &project.sources {
        snapshot.files.push(path.clone());
        let source = buffers
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, s)| s.clone())
            .or_else(|| fs::read_to_string(path).ok());
        if let Some(source) = source {
            symbols::add_source(&mut node, path, &source);
        } else {
            snapshot
                .notices
                .push(format!("Cannot index {}", path.display()));
        }
    }
    snapshot.outlines.insert(project.path.clone(), node);
    for reference in &project.references {
        outline(reference, snapshot, buffers);
    }
}
fn visit(
    root: &Path,
    snapshot: &mut Snapshot,
    seen: &mut HashSet<PathBuf>,
    include_generated: bool,
    remaining: &mut usize,
    depth: usize,
) {
    if *remaining == 0 {
        return;
    }
    if depth > 64 {
        snapshot
            .notices
            .push(format!("Depth limit reached at {}", root.display()));
        return;
    }
    let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if !seen.insert(canonical) {
        snapshot.notices.push(format!(
            "Repeated/symlink folder skipped: {}",
            root.display()
        ));
        return;
    }
    let entries = match fs::read_dir(root) {
        Ok(e) => e,
        Err(e) => {
            snapshot.notices.push(format!("{}: {e}", root.display()));
            return;
        }
    };
    let mut children = Vec::new();
    for entry in entries {
        if *remaining == 0 {
            break;
        }
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                snapshot.notices.push(e.to_string());
                continue;
            }
        };
        let path = entry.path();
        if path.file_name().and_then(|s| s.to_str()) == Some(".git") {
            continue;
        }
        if !include_generated
            && matches!(
                path.file_name().and_then(|s| s.to_str()),
                Some(".quark" | "target" | "build" | "node_modules" | ".venv" | "__pycache__")
            )
        {
            continue;
        }
        *remaining -= 1;
        let directory = path.is_dir();
        children.push(Entry {
            path: path.clone(),
            directory,
        });
        if directory {
            visit(
                &path,
                snapshot,
                seen,
                include_generated,
                remaining,
                depth + 1,
            );
        } else {
            snapshot.files.push(path);
        }
    }
    children.sort_by(|a, b| (!a.directory, &a.path).cmp(&(!b.directory, &b.path)));
    snapshot.directories.insert(root.to_path_buf(), children);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn indexes_all_extensions_and_reports_caps_and_missing_roots() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("unknown.xyz"), "").unwrap();
        let full = scan(&[dir.path().to_path_buf()], &[], &[], false, 100);
        assert_eq!(full.files.len(), 1);
        let capped = scan(&[dir.path().to_path_buf()], &[], &[], false, 1);
        assert!(!capped.notices.is_empty());
        assert!(!scan(&[dir.path().join("missing")], &[], &[], false, 100)
            .notices
            .is_empty());
    }
    #[test]
    fn generated_folders_can_be_included() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("build")).unwrap();
        fs::write(dir.path().join("build/a.hy"), "").unwrap();
        assert!(scan(&[dir.path().to_path_buf()], &[], &[], false, 100)
            .files
            .is_empty());
        assert_eq!(
            scan(&[dir.path().to_path_buf()], &[], &[], true, 100)
                .files
                .len(),
            1
        );
    }
}
