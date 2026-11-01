//! A separate, lightweight native Start Window.
//! Quark's editor, debugger and designer are not created until a project,
//! folder or "Continue without code" is selected.

use crate::{
    native_launcher::{self, AgeGroup, History, Row},
    native_model::Layout,
    startup,
};
use naui::{Length, ListItem, Orientation, Padding, Sizing, TextColor, TextStyle, Ui, Window};
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Project(PathBuf),
    Folder(PathBuf),
    Empty,
}

pub(crate) type Launch = Rc<dyn Fn(Action) -> Result<(), String>>;

pub fn show(
    ui: &Ui,
    window: Window,
    history_path: PathBuf,
    previously_opened: &[PathBuf],
    launch: Launch,
) -> naui::Result<()> {
    let history = Rc::new(RefCell::new(
        History::open(&history_path).unwrap_or_default(),
    ));
    history
        .borrow_mut()
        .migrate_paths(previously_opened, native_launcher::now_epoch());
    let _ = history.borrow().save(&history_path);

    let selected: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));
    let filter = Rc::new(RefCell::new(String::new()));
    let rows = Rc::new(RefCell::new(Vec::<Row>::new()));
    let collapsed = Rc::new(RefCell::new(Vec::<AgeGroup>::new()));

    let root = ui.stack(Orientation::Vertical)?;
    root.set_padding(Padding::all(24.0));
    root.set_spacing(12.0);
    let title = ui.label("Quark IDE")?;
    title.set_style(TextStyle::LargeTitle);
    root.append(&title);
    let subtitle = ui.label("Hydrogen Development Environment")?;
    subtitle.set_color(TextColor::Secondary);
    root.append(&subtitle);
    // Navigation errors must remain visible even when the lower section of
    // the Start Window is clipped by a smaller macOS display.
    let status = ui.label("Choose an existing workspace or create something new.")?;
    status.set_color(TextColor::Secondary);
    root.append(&status);

    let left = ui.stack(Orientation::Vertical)?;
    left.set_padding(Padding::all(12.0));
    left.set_spacing(10.0);
    let left_title = ui.label("Open recent")?;
    left_title.set_style(TextStyle::Title);
    left.append(&left_title);

    let search = ui.search_input()?;
    search.set_placeholder("Search recent projects and folders");
    left.append(&search);

    let recent = ui.list()?;
    recent.set_sizing(
        Sizing::new()
            .width(Length::Fill)
            .height(Length::Fixed(245.0)),
    );
    left.append(&recent);

    let recent_actions = ui.stack(Orientation::Horizontal)?;
    recent_actions.set_spacing(8.0);
    let open_recent = ui.button("Open selected")?;
    let pin_recent = ui.button("Pin / unpin")?;
    let remove_recent = ui.button("Remove from recent")?;
    recent_actions.append(&open_recent);
    recent_actions.append(&pin_recent);
    recent_actions.append(&remove_recent);
    left.append(&recent_actions);
    let note = ui.label("Removing an entry does not remove its project or files.")?;
    note.set_style(TextStyle::Caption);
    note.set_color(TextColor::Secondary);
    left.append(&note);

    let right = ui.stack(Orientation::Vertical)?;
    right.set_padding(Padding::all(14.0));
    right.set_spacing(9.0);
    let right_title = ui.label("Get started")?;
    right_title.set_style(TextStyle::Title);
    right.append(&right_title);

    let clone_button = start_card(
        ui,
        &right,
        "⇩   Clone a repository",
        "Get code from a GitHub repository",
    )?;
    let project_button = start_card(
        ui,
        &right,
        "▣   Open a project or solution",
        "Open an existing Hydrogen .hyproj",
    )?;
    let folder_button = start_card(
        ui,
        &right,
        "▤   Open a local folder",
        "Explore and edit local files",
    )?;
    let create_button = start_card(
        ui,
        &right,
        "✧   Create a new project",
        "Choose a target and generate starter code",
    )?;
    let continue_button = ui.button("Continue without code →")?;
    right.append(&continue_button);

    let columns = ui.split_view(Orientation::Horizontal)?;
    columns.set_start(&left);
    columns.set_end(&right);
    columns.set_position(550.0);
    columns.set_min_sizes(330.0, 340.0);
    columns.set_sizing(Sizing::fill());
    root.append(&columns);
    // Never replace the AppKit NSWindow content while its NSButton handler
    // is still on the stack. Defer every launch action to the next UI tick.
    // A single pending flag also prevents double-clicks opening twice.
    let pending = Rc::new(std::cell::Cell::new(false));
    let sender = ui.tasks().channel({
        let launch = launch.clone();
        let status = status.clone();
        let pending = pending.clone();
        move |action: Action| {
            if let Err(error) = launch(action) {
                pending.set(false);
                status.set_text(&format!("Cannot open workspace: {error}"));
            }
        }
    });
    let launch: Launch = Rc::new({
        let pending = pending.clone();
        let status = status.clone();
        move |action| {
            if pending.replace(true) {
                return Ok(());
            }
            status.set_text("Opening workspace…");
            if let Err(error) = sender.send(action) {
                pending.set(false);
                return Err(error.to_string());
            }
            Ok(())
        }
    });

    let refresh: Rc<dyn Fn()> = Rc::new({
        let history = history.clone();
        let filter = filter.clone();
        let rows = rows.clone();
        let recent = recent.clone();
        let collapsed = collapsed.clone();
        move || {
            let rendered = history.borrow().rows_collapsed(
                &filter.borrow(),
                native_launcher::now_epoch(),
                &collapsed.borrow(),
            );
            recent.set_items(
                &rendered
                    .iter()
                    .map(|entry| ListItem::new(entry.title.clone()))
                    .collect::<Vec<_>>(),
            );
            *rows.borrow_mut() = rendered;
        }
    });
    refresh();

    search.on_change({
        let filter = filter.clone();
        let refresh = refresh.clone();
        let selected = selected.clone();
        move |value| {
            *selected.borrow_mut() = None;
            *filter.borrow_mut() = value.to_owned();
            refresh();
        }
    });
    recent.on_select({
        let selected = selected.clone();
        let rows = rows.clone();
        let collapsed = collapsed.clone();
        let refresh = refresh.clone();
        move |indices| {
            let entry = indices
                .first()
                .and_then(|&index| rows.borrow().get(index).cloned());
            let Some(row) = entry else {
                *selected.borrow_mut() = None;
                return;
            };
            if let Some(path) = row.path {
                *selected.borrow_mut() = Some(path);
                return;
            }
            *selected.borrow_mut() = None;
            if let Some(group) = AgeGroup::ALL
                .iter()
                .copied()
                .find(|group| row.title.ends_with(group.label()))
            {
                let mut folded = collapsed.borrow_mut();
                if let Some(index) = folded.iter().position(|entry| *entry == group) {
                    folded.remove(index);
                } else {
                    folded.push(group);
                }
                drop(folded);
                refresh();
            }
        }
    });

    open_recent.on_click({
        let selected = selected.clone();
        let launch = launch.clone();
        let window = window.clone();
        let status = status.clone();
        move || {
            let Some(path) = selected.borrow().clone() else {
                status.set_text("Choose a project or folder in the recent list.");
                return;
            };
            let action = if path.is_dir() {
                Action::Folder(path)
            } else {
                Action::Project(path)
            };
            transition(action, &launch, &window, &status);
        }
    });
    pin_recent.on_click({
        let selected = selected.clone();
        let history = history.clone();
        let history_path = history_path.clone();
        let refresh = refresh.clone();
        let status = status.clone();
        move || {
            if let Some(path) = selected.borrow().as_deref() {
                history.borrow_mut().toggle_pin(path);
                if let Err(error) = history.borrow().save(&history_path) {
                    status.set_text(&format!("Cannot save recent list: {error}"));
                }
                refresh();
            }
        }
    });
    remove_recent.on_click({
        let selected = selected.clone();
        let history = history.clone();
        let history_path = history_path.clone();
        let refresh = refresh.clone();
        let status = status.clone();
        move || {
            if let Some(path) = selected.borrow_mut().take() {
                history.borrow_mut().remove(&path);
                if let Err(error) = history.borrow().save(&history_path) {
                    status.set_text(&format!("Cannot save recent list: {error}"));
                }
                refresh();
            }
        }
    });

    project_button.on_click({
        let launch = launch.clone();
        let window = window.clone();
        let status = status.clone();
        move || {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Hydrogen project", &["hyproj"])
                .pick_file()
            {
                transition(Action::Project(path), &launch, &window, &status);
            }
        }
    });
    folder_button.on_click({
        let launch = launch.clone();
        let window = window.clone();
        let status = status.clone();
        move || {
            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                transition(Action::Folder(path), &launch, &window, &status);
            }
        }
    });
    continue_button.on_click({
        let launch = launch.clone();
        let window = window.clone();
        let status = status.clone();
        move || transition(Action::Empty, &launch, &window, &status)
    });
    create_button.on_click({
        let ui = ui.clone();
        let window = window.clone();
        let launch = launch.clone();
        let status = status.clone();
        move || {
            if let Err(error) = create_project_wizard(&ui, window.clone(), launch.clone(), true) {
                status.set_text(&format!("Cannot open project wizard: {error}"));
            }
        }
    });
    clone_button.on_click({
        let ui = ui.clone();
        let window = window.clone();
        let launch = launch.clone();
        let status = status.clone();
        move || {
            if let Err(error) = clone_wizard(&ui, window.clone(), launch.clone()) {
                status.set_text(&format!("Cannot open repository wizard: {error}"));
            }
        }
    });

    window.set_child(&root);
    window.show();
    Ok(())
}

fn start_card(
    ui: &Ui,
    column: &naui::Stack,
    label: &str,
    description: &str,
) -> naui::Result<naui::Button> {
    let button = ui.button(label)?;
    button.set_sizing(Sizing::new().width(Length::Fill));
    column.append(&button);
    let subtitle = ui.label(description)?;
    subtitle.set_style(TextStyle::Caption);
    subtitle.set_color(TextColor::Secondary);
    column.append(&subtitle);
    Ok(button)
}

fn transition(action: Action, launch: &Launch, _launcher: &Window, status: &naui::Label) {
    // The launcher and IDE are the same native window. Its contents are
    // replaced by the launch callback; closing here would close the IDE too.
    if let Err(error) = launch(action) {
        status.set_text(&error);
    }
}

pub(crate) fn create_project_wizard(
    ui: &Ui,
    _launcher: Window,
    launch: Launch,
    _close_parent: bool,
) -> naui::Result<()> {
    let wizard = ui.window("Create a Hydrogen project", 640.0, 450.0)?;
    let root = ui.stack(Orientation::Vertical)?;
    root.set_padding(Padding::all(22.0));
    root.set_spacing(10.0);

    let heading = ui.label("Create a new project")?;
    heading.set_style(TextStyle::Title);
    root.append(&heading);
    let name = ui.text_input("MyHydrogenApp")?;
    let parent = ui.text_input(
        &std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .display()
            .to_string(),
    )?;
    let browse = ui.button("Browse for a folder…")?;
    let template = ui.combo_box()?;
    template.set_items(&[
        "Linux application",
        "Windows application",
        "macOS application",
        "Australis executable",
        "Hydrogen library",
    ]);
    template.set_selected(0);

    root.append(&ui.label("Project name")?);
    root.append(&name);
    root.append(&ui.label("Location")?);
    root.append(&parent);
    root.append(&browse);
    root.append(&ui.label("Project template")?);
    root.append(&template);
    let actions = ui.stack(Orientation::Horizontal)?;
    let submit = ui.button("Create project")?;
    let cancel = ui.button("Cancel")?;
    actions.append(&submit);
    actions.append(&cancel);
    root.append(&actions);
    let status = ui.label("Creates a Hydrogen project and a Main.hyui design.")?;
    status.set_color(TextColor::Secondary);
    root.append(&status);

    browse.on_click({
        let parent = parent.clone();
        move || {
            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                parent.set_text(&path.display().to_string());
            }
        }
    });
    submit.on_click({
        let name = name.clone();
        let parent = parent.clone();
        let template = template.clone();
        let status = status.clone();
        let wizard = wizard.clone();
        move || {
            let selected = template.selected().unwrap_or(0);
            let kind = startup::Template::ALL
                .get(selected)
                .copied()
                .unwrap_or(startup::Template::Linux);
            match startup::create_project(Path::new(&parent.text()), &name.text(), kind) {
                Ok(manifest) => {
                    let design = manifest.parent().unwrap().join("Main.hyui");
                    if let Ok(bytes) = crate::ui_xml::encode(&Layout::default()).map(String::into_bytes) {
                        if let Err(error) = crate::storage::atomic_write(&design, &bytes) {
                            status.set_text(&format!(
                                "Project created, but the designer file could not be written: {error}"
                            ));
                        }
                    }
                    match launch(Action::Project(manifest)) {
                        Ok(()) => {
                            // The parent is now the IDE window; never close it.
                            wizard.close();
                        }
                        Err(error) => status.set_text(&error),
                    }
                }
                Err(error) => status.set_text(&error),
            }
        }
    });
    cancel.on_click({
        let wizard = wizard.clone();
        move || wizard.close()
    });
    wizard.set_child(&root);
    wizard.show();
    Ok(())
}

fn clone_wizard(ui: &Ui, _launcher: Window, launch: Launch) -> naui::Result<()> {
    let wizard = ui.window("Clone a repository", 650.0, 420.0)?;
    let root = ui.stack(Orientation::Vertical)?;
    root.set_padding(Padding::all(22.0));
    root.set_spacing(10.0);
    let heading = ui.label("Clone a GitHub repository")?;
    heading.set_style(TextStyle::Title);
    root.append(&heading);

    let url = ui.text_input("")?;
    url.set_placeholder("https://github.com/organisation/project.git");
    let parent = ui.text_input(
        &std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .display()
            .to_string(),
    )?;
    let name = ui.text_input("MyRepository")?;
    let browse = ui.button("Browse destination…")?;
    root.append(&ui.label("Repository URL")?);
    root.append(&url);
    root.append(&ui.label("Clone into folder")?);
    root.append(&parent);
    root.append(&browse);
    root.append(&ui.label("Local directory name")?);
    root.append(&name);

    let actions = ui.stack(Orientation::Horizontal)?;
    let submit = ui.button("Clone and open")?;
    let cancel = ui.button("Cancel")?;
    actions.append(&submit);
    actions.append(&cancel);
    root.append(&actions);
    let status = ui.label("Requires Git. The clone runs without blocking the launcher.")?;
    status.set_color(TextColor::Secondary);
    root.append(&status);

    browse.on_click({
        let parent = parent.clone();
        move || {
            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                parent.set_text(&path.display().to_string());
            }
        }
    });
    submit.on_click({
        let url = url.clone();
        let parent = parent.clone();
        let name = name.clone();
        let status = status.clone();
        let wizard = wizard.clone();
        let ui = ui.clone();
        move || {
            let origin = url.text();
            let folder = PathBuf::from(parent.text());
            let destination = match native_launcher::clone_request(&origin, &folder, &name.text()) {
                Ok(value) => value,
                Err(error) => {
                    status.set_text(&error);
                    return;
                }
            };
            status.set_text("Cloning repository…");
            let launch = launch.clone();
            let wizard = wizard.clone();
            let status = status.clone();
            let destination_for_worker = destination.clone();
            let sender = ui
                .tasks()
                .channel(move |result: Result<(), String>| match result {
                    Ok(()) => {
                        let action = native_launcher::first_project(&destination)
                            .map(Action::Project)
                            .unwrap_or_else(|| Action::Folder(destination.clone()));
                        match launch(action) {
                            Ok(()) => wizard.close(),
                            Err(error) => status.set_text(&error),
                        }
                    }
                    Err(error) => status.set_text(&error),
                });
            std::thread::spawn(move || {
                let output = std::process::Command::new("git")
                    .arg("clone")
                    .arg("--")
                    .arg(&origin)
                    .arg(&destination_for_worker)
                    .output();
                let result = match output {
                    Ok(out) if out.status.success() => Ok(()),
                    Ok(out) => Err(String::from_utf8_lossy(&out.stderr).to_string()),
                    Err(error) => Err(format!("Unable to run Git: {error}")),
                };
                let _ = sender.send(result);
            });
        }
    });
    cancel.on_click({
        let wizard = wizard.clone();
        move || wizard.close()
    });
    wizard.set_child(&root);
    wizard.show();
    Ok(())
}
