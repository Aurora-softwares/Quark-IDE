//! Pure solution explorer projection: real folder hierarchy, compact list
//! mode, file-type icons and stable selection paths for nested TreeItems.
use naui::TreeItem;
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
};

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
    format!(
        "{} {}",
        icon(path),
        path.file_name().unwrap_or_default().to_string_lossy()
    )
}
fn compare(left: &str, right: &str, descending: bool) -> std::cmp::Ordering {
    let ordered = left
        .to_lowercase()
        .cmp(&right.to_lowercase())
        .then_with(|| left.cmp(right));
    if descending {
        ordered.reverse()
    } else {
        ordered
    }
}
fn insert(root: &mut Folder, base: &Path, path: &Path) {
    let relative = path.strip_prefix(base).unwrap_or(path);
    let components = relative
        .components()
        .filter_map(|part| match part {
            std::path::Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if components.is_empty() {
        return;
    }
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
    directory: &Path,
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
        let folder_path = directory.join(name);
        index.insert(path.clone(), folder_path.clone());
        children.push(
            TreeItem::new(format!("📁 {name}"))
                .expanded(true)
                .children(subtree(nested, &path, &folder_path, descending, index)),
        );
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
/// [0, directory, subdirectory, ..., file]. Both folders and files are
/// selectable; callers must only open regular files as source documents.
pub fn project(
    paths: &[PathBuf],
    base: &Path,
    title: &str,
    query: &str,
    descending: bool,
    list_mode: bool,
) -> (Vec<TreeItem>, HashMap<Vec<usize>, PathBuf>) {
    let query = query.trim().to_lowercase();
    let filtered: Vec<PathBuf> = paths
        .iter()
        .filter(|path| query.is_empty() || path.to_string_lossy().to_lowercase().contains(&query))
        .cloned()
        .collect();
    let mut index = HashMap::new();
    index.insert(vec![0], base.to_path_buf());
    let children = if list_mode {
        let mut ordered = filtered;
        ordered.sort_by(|a, b| {
            let left = a.strip_prefix(base).unwrap_or(a).to_string_lossy();
            let right = b.strip_prefix(base).unwrap_or(b).to_string_lossy();
            compare(&left, &right, descending)
        });
        ordered
            .into_iter()
            .enumerate()
            .map(|(number, path)| {
                index.insert(vec![0, number], path.clone());
                let relative = path.strip_prefix(base).unwrap_or(&path);
                TreeItem::new(format!("{} {}", icon(&path), relative.display()))
            })
            .collect()
    } else {
        let mut root = Folder::default();
        for path in filtered {
            insert(&mut root, base, &path);
        }
        subtree(&root, &[0], base, descending, &mut index)
    };
    (
        vec![TreeItem::new(title.to_owned())
            .expanded(true)
            .children(children)],
        index,
    )
}

/// Explorer actions invoked through the native context menu. File mutations
/// always go through file_action() with a checked workspace root.
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    Open,
    NewFile,
    NewFolder,
    Rename,
    Delete,
    Reveal,
}

#[cfg(target_os = "linux")]
pub type MenuCallback = std::rc::Rc<std::cell::RefCell<Option<std::rc::Rc<dyn Fn(MenuAction)>>>>;

/// Attach the OS's native secondary-click menu to the Solution Explorer.
/// The IDE supplies a callback to bind the selection to safe file operations.
#[cfg(target_os = "linux")]
pub fn install_context_menu(tree: &gtk4::Widget, callback: MenuCallback) {
    use gtk4::prelude::*;
    let Ok(list) = tree.clone().downcast::<gtk4::ListBox>() else {
        return;
    };
    let popover = gtk4::Popover::new();
    popover.set_has_arrow(true);
    popover.set_parent(tree);
    let items = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    for (title, command) in [
        ("Open", MenuAction::Open),
        ("New File…", MenuAction::NewFile),
        ("New Folder…", MenuAction::NewFolder),
        ("Rename…", MenuAction::Rename),
        ("Delete…", MenuAction::Delete),
        ("Show in File Manager", MenuAction::Reveal),
    ] {
        let item = gtk4::Button::with_label(title);
        item.add_css_class("flat");
        let pop = popover.clone();
        let callback = callback.clone();
        item.connect_clicked(move |_| {
            pop.popdown();
            if let Some(action) = callback.borrow().as_ref() {
                action(command);
            }
        });
        items.append(&item);
    }
    popover.set_child(Some(&items));
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(3);
    gesture.set_propagation_phase(gtk4::PropagationPhase::Capture);
    gesture.connect_pressed(move |_, _, x, y| {
        let Some(clicked_row) = list.row_at_y(y as i32) else {
            return;
        };
        let selected = list.selected_row().map(|row| row.index());
        if !crate::native_model::menu_targets_selected(Some(clicked_row.index()), selected) {
            // Context actions must target the row beneath the pointer,
            // even when it was not previously selected.
            list.select_row(Some(&clicked_row));
        }
        popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        popover.popup();
    });
    tree.add_controller(gesture);
}

/// Non-blocking native modal prompt. GtkDialog is deprecated in GTK 4.10;
/// a regular transient GtkWindow keeps the implementation supported.
#[cfg(target_os = "linux")]
pub fn ask_name(
    anchor: &gtk4::Widget,
    title: &str,
    suggested: &str,
    apply: impl Fn(String) + 'static,
) {
    use gtk4::prelude::*;
    let dialog = gtk4::Window::new();
    dialog.set_title(Some(title));
    dialog.set_modal(true);
    dialog.set_default_size(360, 130);
    if let Some(window) = anchor
        .root()
        .and_then(|root| root.downcast::<gtk4::Window>().ok())
    {
        dialog.set_transient_for(Some(&window));
    }
    let contents = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    contents.set_margin_top(12);
    contents.set_margin_bottom(12);
    contents.set_margin_start(12);
    contents.set_margin_end(12);
    let entry = gtk4::Entry::new();
    entry.set_text(suggested);
    contents.append(&entry);
    let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let cancel = gtk4::Button::with_label("Cancel");
    let confirm = gtk4::Button::with_label("Apply");
    let apply = std::rc::Rc::new(apply);
    let dialog_on_enter = dialog.clone();
    let apply_on_enter = apply.clone();
    entry.connect_activate(move |entry| {
        apply_on_enter(entry.text().to_string());
        dialog_on_enter.close();
    });
    let dialog_on_apply = dialog.clone();
    confirm.connect_clicked(move |_| {
        apply(entry.text().to_string());
        dialog_on_apply.close();
    });
    let dialog_on_cancel = dialog.clone();
    cancel.connect_clicked(move |_| dialog_on_cancel.close());
    actions.append(&cancel);
    actions.append(&confirm);
    contents.append(&actions);
    dialog.set_child(Some(&contents));
    dialog.present();
}

#[cfg(target_os = "linux")]
pub fn confirm_delete(anchor: &gtk4::Widget, file: &Path, apply: impl Fn() + 'static) {
    use gtk4::prelude::*;
    let dialog = gtk4::Window::new();
    dialog.set_title(Some("Confirm deletion"));
    dialog.set_modal(true);
    dialog.set_default_size(420, 140);
    if let Some(window) = anchor
        .root()
        .and_then(|root| root.downcast::<gtk4::Window>().ok())
    {
        dialog.set_transient_for(Some(&window));
    }
    let contents = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    contents.set_margin_top(12);
    contents.set_margin_bottom(12);
    contents.set_margin_start(12);
    contents.set_margin_end(12);
    let warning = gtk4::Label::new(Some(&format!(
        "Permanently delete {}? This cannot be undone.",
        file.display()
    )));
    warning.set_wrap(true);
    contents.append(&warning);
    let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let cancel = gtk4::Button::with_label("Cancel");
    let confirm = gtk4::Button::with_label("Delete");
    confirm.add_css_class("destructive-action");
    let dialog_on_delete = dialog.clone();
    confirm.connect_clicked(move |_| {
        apply();
        dialog_on_delete.close();
    });
    let dialog_on_cancel = dialog.clone();
    cancel.connect_clicked(move |_| dialog_on_cancel.close());
    actions.append(&cancel);
    actions.append(&confirm);
    contents.append(&actions);
    dialog.set_child(Some(&contents));
    dialog.present();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_directory_selection_resolves_correct_file() {
        let base = Path::new("/project");
        let paths = vec![
            base.join("src/main.hy"),
            base.join("src/ui/Main.hyui"),
            base.join("src/Other.hy"),
            base.join("Australis.hyproj"),
        ];
        let (_, index) = project(&paths, base, "Solution", "", false, false);
        assert_eq!(index.len(), 7);
        assert!(index.values().any(|p| p.ends_with("ui/Main.hyui")));
        assert!(index.keys().any(|key| key.len() == 4));
        assert!(index.contains_key(&vec![0, 0]));
        let (_, list) = project(&paths, base, "Solution", "", true, true);
        assert_eq!(list.len(), 5);
        assert!(list.contains_key(&vec![0, 0]));
        let (_, filtered) = project(&paths, base, "Solution", "main", false, false);
        assert_eq!(filtered.len(), 5);
    }
}
