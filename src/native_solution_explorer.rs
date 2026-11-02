//! Pure solution explorer projection: real folder hierarchy, compact list
//! mode, file-type icons and stable selection paths for nested TreeItems.
use naui::TreeItem;
use std::{collections::{BTreeMap, HashMap}, path::{Path, PathBuf}};

#[derive(Default)]
struct Folder {
    folders: BTreeMap<String, Folder>,
    files: Vec<PathBuf>,
}
fn icon(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("hyproj") => "⚙",
        Some("hyui") => "▣",
        Some("hy") => "◇",
        Some("json" | "toml") => "☷",
        _ => "▤",
    }
}
fn label(path: &Path) -> String {
    format!("{} {}", icon(path), path.file_name().unwrap_or_default().to_string_lossy())
}
fn compare(left: &str, right: &str, descending: bool) -> std::cmp::Ordering {
    let ordered = left.to_lowercase().cmp(&right.to_lowercase()).then_with(|| left.cmp(right));
    if descending { ordered.reverse() } else { ordered }
}
fn insert(root: &mut Folder, base: &Path, path: &Path) {
    let relative = path.strip_prefix(base).unwrap_or(path);
    let components = relative.components().filter_map(|part| match part {
        std::path::Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
        _ => None,
    }).collect::<Vec<_>>();
    if components.is_empty() { return; }
    let mut node = root;
    for segment in &components[..components.len() - 1] {
        node = node.folders.entry(segment.clone()).or_default();
    }
    if !node.files.contains(&path.to_path_buf()) {
        node.files.push(path.to_path_buf());
    }
}
fn subtree(
    folder: &Folder,
    ancestors: &[usize],
    descending: bool,
    index: &mut HashMap<Vec<usize>, PathBuf>,
) -> Vec<TreeItem> {
    let mut folders = folder.folders.iter().collect::<Vec<_>>();
    folders.sort_by(|(a, _), (b, _)| compare(a, b, descending));
    let mut files = folder.files.clone();
    files.sort_by(|a, b| {
        compare(
            &a.file_name().unwrap_or_default().to_string_lossy(),
            &b.file_name().unwrap_or_default().to_string_lossy(),
            descending,
        )
    });
    let mut children = Vec::new();
    for (name, nested) in folders {
        let mut path = ancestors.to_vec();
        path.push(children.len());
        children.push(TreeItem::new(format!("📁 {name}")).expanded(true).children(
            subtree(nested, &path, descending, index),
        ));
    }
    for path in files {
        let mut position = ancestors.to_vec();
        position.push(children.len());
        index.insert(position, path.clone());
        children.push(TreeItem::new(label(&path)));
    }
    children
}
/// Each returned path vector is an exact TreeItem selection coordinate:
/// [0, directory, subdirectory, ..., file]. Folders are intentionally not
/// mapped to files and never open a source editor when selected.
pub fn project(
    paths: &[PathBuf],
    base: &Path,
    title: &str,
    query: &str,
    descending: bool,
    list_mode: bool,
) -> (Vec<TreeItem>, HashMap<Vec<usize>, PathBuf>) {
    let query = query.trim().to_lowercase();
    let filtered: Vec<PathBuf> = paths.iter().filter(|path| {
        query.is_empty() || path.to_string_lossy().to_lowercase().contains(&query)
    }).cloned().collect();
    let mut index = HashMap::new();
    let children = if list_mode {
        let mut ordered = filtered;
        ordered.sort_by(|a, b| {
            let left = a.strip_prefix(base).unwrap_or(a).to_string_lossy();
            let right = b.strip_prefix(base).unwrap_or(b).to_string_lossy();
            compare(&left, &right, descending)
        });
        ordered.into_iter().enumerate().map(|(number, path)| {
            index.insert(vec![0, number], path.clone());
            let relative = path.strip_prefix(base).unwrap_or(&path);
            TreeItem::new(format!("{} {}", icon(&path), relative.display()))
        }).collect()
    } else {
        let mut root = Folder::default();
        for path in filtered { insert(&mut root, base, &path); }
        subtree(&root, &[0], descending, &mut index)
    };
    (vec![TreeItem::new(title.to_owned()).expanded(true).children(children)], index)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_directory_selection_resolves_correct_file() {
        let base = Path::new("/project");
        let paths = vec![
            base.join("src/main.hy"), base.join("src/ui/Main.hyui"),
            base.join("src/Other.hy"), base.join("Australis.hyproj"),
        ];
        let (_, index) = project(&paths, base, "Solution", "", false, false);
        assert_eq!(index.len(), 4);
        assert!(index.values().any(|p| p.ends_with("ui/Main.hyui")));
        assert!(index.keys().any(|key| key.len() == 4));
        assert!(!index.contains_key(&vec![0, 0]));
        let (_, list) = project(&paths, base, "Solution", "", true, true);
        assert_eq!(list.len(), 4);
        assert!(list.contains_key(&vec![0, 0]));
        let (_, filtered) = project(&paths, base, "Solution", "main", false, false);
        assert_eq!(filtered.len(), 2);
    }
}
