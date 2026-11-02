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

/// Keep the native popover under a ListBoxRow; naui's Tree::rebuild removes
/// every direct ListBox child as a row and cannot remove a Popover.
#[cfg(target_os = "linux")]
fn anchor_context_menu(popover: &gtk4::Popover, row: &gtk4::ListBoxRow, x: i32, y: i32) {
    use gtk4::prelude::*;
    popover.popdown();
    if popover.parent().is_some() {
        popover.unparent();
    }
    popover.set_parent(row);
    // naui's Tree::rebuild destroys ListBoxRows. A GtkPopover is a native
    // child of its anchor, not a GtkListBoxRow child managed by naui. Release
    // it as soon as the row leaves its list, before GTK finalizes the row.
    // Keep only a weak reference so this observer cannot retain the popup.
    let popup = popover.downgrade();
    row.connect_notify_local(Some("parent"), move |row, _| {
        if row.parent().is_some() {
            return;
        }
        if let Some(popup) = popup.upgrade() {
            let anchor: gtk4::Widget = row.clone().upcast();
            if popup.parent().as_ref() == Some(&anchor) {
                popup.popdown();
                popup.unparent();
            }
        }
    });
    popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(
        x,
        (y - row.allocation().y()).max(0),
        1,
        1,
    )));
}

#[cfg(target_os = "linux")]
pub fn install_context_menu(
    tree: &gtk4::Widget,
    callback: MenuCallback,
    context_selection: std::rc::Rc<std::cell::Cell<bool>>,
) {
    use gtk4::prelude::*;
    let Ok(list) = tree.clone().downcast::<gtk4::ListBox>() else {
        return;
    };
    let popover = gtk4::Popover::new();
    popover.set_has_arrow(true);
    // Do not parent a popover to GtkListBox itself. naui 0.8's Tree::rebuild
    // walks first_child() and removes each as a row; a popover is not a
    // GtkListBoxRow, so remove() warns repeatedly without removing anything.
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
            // GTK selection callbacks normally open documents. A secondary
            // click changes selection only; the user chooses Open from the menu.
            context_selection.set(true);
            list.select_row(Some(&clicked_row));
            context_selection.set(false);
        }
        anchor_context_menu(&popover, &clicked_row, x as i32, y as i32);
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
    use crate::native_model::{file_action, FileAction};
    #[test]
    fn file_actions_create_rename_and_delete_without_overwriting_existing_entries() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let created = file_action(root, root, FileAction::NewFolder("sources".into())).unwrap();
        assert!(created.is_dir());
        let source = file_action(root, &created, FileAction::NewFile("Main.hy".into())).unwrap();
        assert!(source.is_file());
        assert!(file_action(root, &created, FileAction::NewFile("Main.hy".into())).is_err());
        let moved = file_action(root, &source, FileAction::Rename("Program.hy".into())).unwrap();
        assert!(moved.is_file());
        assert!(!source.exists());
        file_action(root, &moved, FileAction::Delete).unwrap();
        assert!(!moved.exists());
    }

    #[test]
    fn file_actions_reject_path_traversal_external_paths_and_root_removal() {
        let dir = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        assert!(file_action(
            dir.path(),
            dir.path(),
            FileAction::NewFile("../escape".into())
        )
        .is_err());
        assert!(file_action(dir.path(), dir.path(), FileAction::NewFolder("x/y".into())).is_err());
        assert!(file_action(dir.path(), dir.path(), FileAction::Delete).is_err());
        let outside = other.path().join("outside.hy");
        std::fs::write(&outside, "test").unwrap();
        assert!(file_action(dir.path(), &outside, FileAction::Delete).is_err());
        assert_eq!(std::fs::read_to_string(outside).unwrap(), "test");
    }

    /// GTK should never see the popover as a ListBox row. Otherwise naui
    /// Tree::rebuild can repeatedly emit "Tried to remove non-child".
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires a native GTK display; invoked explicitly under Xvfb"]
    fn context_menu_does_not_poison_listbox_row_rebuild() {
        use gtk4::prelude::*;
        gtk4::init().expect("GTK test requires a display");
        let list = gtk4::ListBox::new();
        let first = gtk4::ListBoxRow::new();
        let second = gtk4::ListBoxRow::new();
        list.append(&first);
        list.append(&second);
        let popover = gtk4::Popover::new();
        anchor_context_menu(&popover, &first, 4, 8);
        assert_eq!(popover.parent(), Some(first.clone().upcast()));
        assert!(list.first_child().unwrap().is::<gtk4::ListBoxRow>());
        list.remove(&first);
        assert!(popover.parent().is_none());
        anchor_context_menu(&popover, &second, 4, 8);
        assert_eq!(popover.parent(), Some(second.clone().upcast()));
        list.remove(&second);
        assert!(popover.parent().is_none());
        assert!(list.row_at_index(0).is_none());
    }

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
