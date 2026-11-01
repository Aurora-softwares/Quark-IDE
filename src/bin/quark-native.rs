//! Native Quark frontend preview: WinUI 3 on Windows, AppKit on macOS,
//! GTK 4/libadwaita on Linux. This does not draw egui controls.
//!
//! The existing full-featured egui IDE remains available during the migration.
//! This native frontend deliberately shares .hyproj, .hy and .quarkui files
//! instead of silently converting or rewriting them.

#[allow(dead_code)] // Shared legacy APIs are migrated incrementally.
#[path = "../hydrogen.rs"]
mod hydrogen;
#[path = "../native_commands.rs"]
mod native_commands;
#[path = "../native_launcher.rs"]
mod native_launcher;
#[path = "../native_layout.rs"]
mod native_layout;
#[allow(dead_code)] // Shared legacy APIs are migrated incrementally.
#[path = "../native_model.rs"]
mod native_model;
#[allow(dead_code)] // Menu metadata also used by UI contract tests.
#[path = "../native_shell.rs"]
mod native_shell;
#[path = "../native_start_window.rs"]
mod native_start_window;
#[path = "../problems.rs"]
mod problems;
#[allow(dead_code)] // Shared legacy APIs are migrated incrementally.
#[path = "../project.rs"]
mod project;
#[allow(dead_code)] // Shared legacy APIs are migrated incrementally.
#[path = "../startup.rs"]
mod startup;
#[allow(dead_code)] // Shared legacy APIs are migrated incrementally.
#[path = "../storage.rs"]
mod storage;

use naui::{
    Color, GridCell, Length, ListItem, MenuSpec, Orientation, Point, PointerPhase, Rect, Settings,
    Sizing, Theme, ToolbarIcon, ToolbarItem, Track, TreeItem, Ui,
};
use std::{
    cell::{Cell, RefCell},
    fs,
    path::{Path, PathBuf},
    rc::Rc,
};

use native_model::{ControlKind, Document, GeometryField, Layout, LayoutDocument, Workspace};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Page {
    Empty,
    Code,
    Design,
}

struct Ide {
    workspace: Workspace,
    project: Option<project::Project>,
    document: Option<Document>,
    layout: Option<LayoutDocument>,
    page: Page,
    armed: Option<ControlKind>,
    selected_id: Option<u64>,
    drag: Option<(u64, f32, f32)>,
}

impl Ide {
    fn load() -> Self {
        let workspace = Workspace::open(&workspace_file()).unwrap_or_default();
        Self::from_workspace(workspace)
    }

    fn from_workspace(workspace: Workspace) -> Self {
        let project = workspace.selected.as_deref().map(project::load);
        Self {
            workspace,
            project,
            document: None,
            layout: None,
            page: Page::Empty,
            armed: None,
            selected_id: None,
            drag: None,
        }
    }

    fn remember_project(&mut self, manifest: PathBuf) -> Result<(), String> {
        if self.has_unsaved() {
            return Err("Save open source and UI design changes before changing projects".into());
        }
        if manifest.extension().and_then(|s| s.to_str()) != Some("hyproj") || !manifest.is_file() {
            return Err("Choose an existing Hydrogen .hyproj file".into());
        }
        self.workspace.remember(manifest, true);
        self.workspace.save(&workspace_file())?;
        self.project = self.workspace.selected.as_deref().map(project::load);
        self.document = None;
        self.layout = None;
        self.page = Page::Code;
        if let Some(path) = &self.workspace.selected {
            record_recent(path.clone());
        }
        Ok(())
    }

    fn remember_folder(&mut self, folder: PathBuf) -> Result<(), String> {
        if self.has_unsaved() {
            return Err("Save open source and UI design changes before changing folders".into());
        }
        if !folder.is_dir() {
            return Err("Choose an existing folder".into());
        }
        let canonical = folder.canonicalize().unwrap_or(folder);
        self.workspace.remember(canonical.clone(), false);
        self.workspace.selected = None;
        self.workspace.save(&workspace_file())?;
        self.project = None;
        self.document = None;
        self.layout = None;
        self.page = Page::Code;
        record_recent(canonical);
        Ok(())
    }

    fn clear_workspace(&mut self) {
        self.project = None;
        self.workspace.selected = None;
        self.workspace.projects.clear();
        self.workspace.folders.clear();
        self.document = None;
        self.layout = None;
        self.page = Page::Empty;
    }

    fn open_source(&mut self, file: &Path) -> Result<(), String> {
        if self.has_unsaved() {
            return Err(
                "Save open source and UI layout changes before replacing a document".into(),
            );
        }
        self.document = Some(Document::open(file)?);
        self.page = Page::Code;
        Ok(())
    }

    fn open_design(&mut self, file: &Path) -> Result<(), String> {
        if self.layout.as_ref().is_some_and(LayoutDocument::dirty) {
            return Err("Save your current UI layout before opening another".into());
        }
        self.layout = Some(LayoutDocument::open(file)?);
        self.page = Page::Design;
        self.selected_id = None;
        Ok(())
    }

    fn display_page(&self, requested: Page) -> Page {
        match requested {
            Page::Code if self.document.is_none() => Page::Empty,
            Page::Design if self.layout.is_none() => Page::Empty,
            _ => requested,
        }
    }

    fn save_active(&mut self) -> Result<(), String> {
        match self.page {
            Page::Code => self
                .document
                .as_mut()
                .ok_or_else(|| "No source document is open".to_owned())?
                .save(),
            Page::Design => self
                .layout
                .as_mut()
                .ok_or_else(|| "No UI layout is open".to_owned())?
                .save(),
            Page::Empty => Err("No document is open".into()),
        }
    }

    fn has_unsaved(&self) -> bool {
        self.document.as_ref().is_some_and(Document::dirty)
            || self.layout.as_ref().is_some_and(LayoutDocument::dirty)
    }
}

fn workspace_file() -> PathBuf {
    #[cfg(target_os = "windows")]
    let root = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let root = std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join("Library/Application Support"));
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let root = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    root.unwrap_or_else(std::env::temp_dir)
        .join("QuarkIDE/native-workspace.json")
}

fn record_recent(path: PathBuf) {
    let file = workspace_file().with_file_name("native-launcher.json");
    // A damaged history file must not be overwritten automatically.
    if let Ok(mut history) = native_launcher::History::open(&file) {
        history.remember(path, native_launcher::now_epoch());
        let _ = history.save(&file);
    }
}

fn backend() -> &'static str {
    if cfg!(target_os = "windows") {
        "WinUI 3 / Fluent"
    } else if cfg!(target_os = "macos") {
        "AppKit"
    } else {
        "GTK 4 / libadwaita"
    }
}

fn display_project_name(ide: &Ide) -> String {
    ide.project
        .as_ref()
        .map(|project| project.name.clone())
        .unwrap_or_else(|| "No solution loaded".into())
}

fn file_items(ide: &Ide) -> (Vec<TreeItem>, Vec<PathBuf>) {
    let Some(project) = &ide.project else {
        let Some(folder) = ide.workspace.folders.last() else {
            return (vec![TreeItem::new("No solution loaded")], vec![]);
        };
        let mut files = fs::read_dir(folder)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file()
                    && matches!(
                        path.extension().and_then(|ext| ext.to_str()),
                        Some("hy" | "hyproj" | "quarkui")
                    )
            })
            .collect::<Vec<_>>();
        files.sort();
        let children: Vec<TreeItem> = files
            .iter()
            .map(|path| {
                TreeItem::new(
                    path.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                )
            })
            .collect();
        return (
            vec![TreeItem::new(format!("Folder '{}'", folder.display()))
                .expanded(true)
                .children(children)],
            files,
        );
    };
    let mut paths = Vec::new();
    paths.push(project.path.clone());
    paths.extend(project.sources.iter().cloned());
    if let Some(layout) = &ide.layout {
        paths.push(layout.path.clone());
    }
    if let Some(folder) = project.path.parent() {
        if let Ok(entries) = fs::read_dir(folder) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|ext| ext.to_str()) == Some("quarkui") {
                    paths.push(path);
                }
            }
        }
    }
    paths.sort();
    paths.dedup();
    let children = paths
        .iter()
        .map(|path| {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            TreeItem::new(name)
        })
        .collect::<Vec<_>>();
    (
        vec![TreeItem::new(format!("Solution '{}'", project.name))
            .expanded(true)
            .children(children)],
        paths,
    )
}

fn filtered_file_items(ide: &Ide, search: &str) -> (Vec<TreeItem>, Vec<PathBuf>) {
    let (original, files) = file_items(ide);
    let pattern = search.trim().to_lowercase();
    if pattern.is_empty() {
        return (original, files);
    }
    let matching: Vec<_> = files
        .into_iter()
        .filter(|path| path.to_string_lossy().to_lowercase().contains(&pattern))
        .collect();
    let children: Vec<TreeItem> = matching
        .iter()
        .map(|path| {
            TreeItem::new(
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
            )
        })
        .collect();
    let label = ide
        .project
        .as_ref()
        .map(|p| format!("Solution '{}'", p.name))
        .unwrap_or_else(|| "Folder".to_owned());
    (
        vec![TreeItem::new(label).expanded(true).children(children)],
        matching,
    )
}

#[allow(clippy::too_many_arguments)] // UI control handles are intentionally passed together.
fn run_task(
    task: native_commands::Task,
    state: Rc<RefCell<Ide>>,
    output: naui::Label,
    status: naui::Label,
    error_list: naui::List,
    error_summary: naui::Label,
    current_errors: Rc<RefCell<Vec<problems::Problem>>>,
    ui: Ui,
) {
    let project = state.borrow().workspace.selected.clone();
    let compiler = std::env::var("QUARK_HYDROGEN_COMPILER").unwrap_or_else(|_| "hy".into());
    let project_dir = project
        .as_ref()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    status.set_text(&format!("{}…", task.label()));
    output.set_text(&format!("Running {}…", task.label()));
    let sender = ui
        .tasks()
        .channel(move |result: native_commands::ResultLog| {
            let parsed = problems::parse(&result.text, &project_dir);
            let errors = parsed
                .iter()
                .filter(|item| item.severity == problems::Severity::Error)
                .count();
            let warnings = parsed.len() - errors;
            error_summary.set_text(&format!("{errors} errors, {warnings} warnings"));
            error_list.set_items(
                &parsed
                    .iter()
                    .map(|item| {
                        ListItem::new(format!(
                            "{}: {}:{} — {}",
                            if item.severity == problems::Severity::Error {
                                "Error"
                            } else {
                                "Warning"
                            },
                            item.path.display(),
                            item.line,
                            item.message
                        ))
                    })
                    .collect::<Vec<_>>(),
            );
            *current_errors.borrow_mut() = parsed;
            output.set_text(&result.text);
            status.set_text(if result.success {
                "Operation completed successfully"
            } else {
                "Operation failed — review Output and Error List"
            });
        });
    std::thread::spawn(move || {
        let result = native_commands::run(task, &compiler, project.as_deref());
        let _ = sender.send(result);
    });
}

/// User-initiated tool-pane resizing. OS window resizes never update
/// these stored widths/heights: the centre's Fill tracks absorb the change.
fn attach_pane_grip(
    grip: &naui::Canvas,
    pane: native_layout::Pane,
    prefs: Rc<Cell<native_layout::PaneSizes>>,
    save_path: PathBuf,
    apply: Rc<dyn Fn(native_layout::PaneSizes)>,
) {
    let vertical_divider = matches!(
        pane,
        native_layout::Pane::Toolbox | native_layout::Pane::Inspector
    );
    grip.on_draw(move |painter| {
        let (width, height) = (painter.width(), painter.height());
        // Full-width hit target with an unmistakable Hydrogen / Quark grip.
        painter.fill_rect(
            Rect::new(0.0, 0.0, width, height),
            Color::rgb(0x23, 0x2c, 0x40),
        );
        let (r, g, b) = native_shell::HYDROGEN_CYAN;
        let line = if vertical_divider {
            Rect::new((width - 2.0) / 2.0, 0.0, 2.0, height)
        } else {
            Rect::new(0.0, (height - 2.0) / 2.0, width, 2.0)
        };
        painter.fill_rect(line, Color::rgb(r, g, b));
        let (r, g, b) = native_shell::QUARK_VIOLET;
        let marker = if vertical_divider {
            Rect::new((width - 4.0) / 2.0, (height - 38.0) / 2.0, 4.0, 38.0)
        } else {
            Rect::new((width - 38.0) / 2.0, (height - 4.0) / 2.0, 38.0, 4.0)
        };
        painter.fill_rect(marker, Color::rgb(r, g, b));
    });
    // Captured pointers are relative to the MOVING grip itself. Using the
    // original pane size caused the divider to snap back after every reflow.
    let dragging = Rc::new(Cell::new(None::<f64>));
    let modified = Rc::new(Cell::new(false));
    grip.on_pointer({
        let dragging = dragging.clone();
        let modified = modified.clone();
        move |event| {
            let coordinate = if vertical_divider {
                event.point.x
            } else {
                event.point.y
            };
            match event.phase {
                PointerPhase::Down => {
                    dragging.set(Some(coordinate));
                    modified.set(false);
                }
                PointerPhase::Move => {
                    if let Some(begin) = dragging.get() {
                        let original = prefs.get();
                        let next = original.drag(pane, begin, coordinate);
                        if next != original {
                            prefs.set(next);
                            modified.set(true);
                            apply(next);
                        }
                    }
                }
                PointerPhase::Up => {
                    if dragging.replace(None).is_some() && modified.replace(false) {
                        let _ = prefs.get().save(&save_path);
                    }
                }
            }
        }
    });
}

fn build_ide(ui: &Ui, state: Rc<RefCell<Ide>>) -> naui::Result<()> {
    let window = ui.window(
        "Quark IDE — Hydrogen Development Environment",
        1480.0,
        940.0,
    )?;
    // A proper native system title bar, native application menus and a native
    // toolbar. No frameless window, drawn caption buttons or simulated OS controls.
    let menu = ui.menu_bar()?;
    menu.set_menus(
        &native_shell::MENUS
            .iter()
            .map(|(name, items)| MenuSpec::new(*name, items.iter().copied()))
            .collect::<Vec<_>>(),
    );
    window.set_menu_bar(&menu);

    let toolbar = ui.toolbar()?;
    toolbar.set_items(&[
        ToolbarItem::new(ToolbarIcon::New, "New Project"),
        ToolbarItem::new(ToolbarIcon::Open, "Open Project"),
        ToolbarItem::new(ToolbarIcon::Save, "Save"),
        ToolbarItem::separator(),
        ToolbarItem::new(ToolbarIcon::Undo, "Undo"),
        ToolbarItem::new(ToolbarIcon::Redo, "Redo"),
        ToolbarItem::separator(),
        ToolbarItem::new(ToolbarIcon::Refresh, "Build Solution"),
        ToolbarItem::new(ToolbarIcon::Forward, "Start Without Debugging"),
        ToolbarItem::new(ToolbarIcon::Share, "Publish Binaries"),
        ToolbarItem::new(ToolbarIcon::Search, "Search Solution"),
    ]);
    window.set_toolbar(&toolbar);

    // Professional IDE layout, deliberately different from the VS Code-style
    // activity bar. The centre owns documents/designers; tools and properties
    // live in persistent surrounding panes.
    let root = ui.grid()?;
    root.set_sizing(Sizing::fill());
    root.set_column_track(0, Track::FILL);
    root.set_row_track(0, Track::Fixed(45.0));
    root.set_row_track(1, Track::FILL);
    root.set_row_track(2, Track::Fixed(26.0));
    let pane_path = workspace_file().with_file_name("native-pane-layout.json");
    let preferred = native_layout::PaneSizes::open(&pane_path).unwrap_or_default();
    let pane_sizes = Rc::new(Cell::new(preferred));

    // The familiar Visual Studio command strip lives below the native menu.
    // Configuration and architecture are visible now; native compiler flag
    // support is a separate milestone, so no selection is silently invented.
    let command_strip = ui.stack(Orientation::Horizontal)?;
    command_strip.set_spacing(8.0);
    let configuration = ui.combo_box()?;
    configuration.set_items(&["Debug", "Release"]);
    configuration.set_selected(0);
    let architecture = ui.combo_box()?;
    architecture.set_items(&["Any CPU", "x64", "arm64"]);
    architecture.set_selected(0);
    let startup_project = ui.combo_box()?;
    startup_project.set_items(&[&display_project_name(&state.borrow())]);
    startup_project.set_selected(0);
    let build_button = ui.button("Build")?;
    let run_button = ui.button("▶ Run without debugging")?;
    let debug_button = ui.button("▶ Start debugging")?;
    debug_button.set_enabled(false); // Requires a verified native debugger adapter.
    let publish_button = ui.button("Publish binaries")?;
    command_strip.append(&ui.label("Configuration:")?);
    command_strip.append(&configuration);
    command_strip.append(&architecture);
    command_strip.append(&startup_project);
    command_strip.append(&build_button);
    command_strip.append(&run_button);
    command_strip.append(&debug_button);
    command_strip.append(&publish_button);
    root.attach(&command_strip, GridCell::new(0, 0));

    let toolbox = ui.stack(Orientation::Vertical)?;
    toolbox.set_spacing(7.0);
    toolbox.append(&ui.label("TOOLBOX")?);
    let toolbox_search = ui.search_input()?;
    toolbox_search.set_placeholder("Search controls");
    toolbox.append(&toolbox_search);
    let toolbox_items = ui.stack(Orientation::Vertical)?;
    toolbox_items.set_spacing(4.0);
    let mut toolbox_buttons = Vec::new();
    let mut toolbox_headers = Vec::new();
    for category in ["Common Controls", "Containers", "Display"] {
        let heading = ui.label(category)?;
        toolbox_items.append(&heading);
        toolbox_headers.push((category, heading));
        for kind in ControlKind::ALL {
            if kind.category() == category {
                let button = ui.button(kind.title())?;
                button.set_sizing(Sizing::fill_width());
                toolbox_items.append(&button);
                toolbox_buttons.push((kind, button));
            }
        }
    }
    let toolbox_scroll = ui.scroll()?;
    toolbox_scroll.set_child(&toolbox_items);
    toolbox_scroll.set_sizing(Sizing::fill());
    toolbox.append(&toolbox_scroll);
    toolbox.append(&ui.label("Select a control, then click the design surface.")?);
    toolbox.set_sizing(Sizing::fill());
    toolbox_search.on_change({
        let entries = toolbox_items.clone();
        let controls = toolbox_buttons.clone();
        move |search| {
            let query = search.trim().to_lowercase();
            entries.clear();
            for (category, header) in &toolbox_headers {
                let visible = controls
                    .iter()
                    .filter(|(kind, _)| {
                        kind.category() == *category
                            && (query.is_empty() || kind.title().to_lowercase().contains(&query))
                    })
                    .collect::<Vec<_>>();
                if !visible.is_empty() {
                    entries.append(header);
                    for (_, button) in visible {
                        entries.append(button);
                    }
                }
            }
        }
    });

    // Narrow native tool-window tabs, with vertically written captions.
    // They are keyboard-focusable platform buttons, not a VS Code activity bar.
    let left_rail = ui.stack(Orientation::Vertical)?;
    left_rail.set_spacing(2.0);
    left_rail.set_sizing(
        Sizing::new().width(Length::Fixed(native_shell::SIDE_RAIL_WIDTH)),
    );
    let rail_accent = ui.canvas()?;
    rail_accent.set_sizing(
        Sizing::new()
            .width(Length::Fill)
            .height(Length::Fixed(4.0)),
    );
    rail_accent.on_draw(|painter| {
        let (r, g, b) = native_shell::QUARK_VIOLET;
        painter.fill_rect(
            Rect::new(0.0, 0.0, painter.width(), painter.height()),
            Color::rgb(r, g, b),
        );
    });
    left_rail.append(&rail_accent);
    let toolbox_tab = ui.button(native_shell::SIDE_TABS[0].1)?;
    let server_tab = ui.button(native_shell::SIDE_TABS[1].1)?;
    let data_tab = ui.button(native_shell::SIDE_TABS[2].1)?;
    for (button, tab) in [
        (&toolbox_tab, native_shell::SIDE_TABS[0]),
        (&server_tab, native_shell::SIDE_TABS[1]),
        (&data_tab, native_shell::SIDE_TABS[2]),
    ] {
        button.set_sizing(
            Sizing::new()
                .width(Length::Fixed(native_shell::SIDE_RAIL_WIDTH - 4.0))
                .height(Length::Fixed(tab.2)),
        );
        left_rail.append(button);
    }
    let server_view = ui.stack(Orientation::Vertical)?;
    server_view.append(&ui.label("SERVER EXPLORER")?);
    server_view.append(&ui.label("Server connections can be added here in a future update.")?);
    let data_view = ui.stack(Orientation::Vertical)?;
    data_view.append(&ui.label("DATA SOURCES")?);
    data_view.append(&ui.label("Data source integration is planned.")?);
    let left_content = ui.stack(Orientation::Vertical)?;
    left_content.append(&toolbox);
    left_content.set_sizing(Sizing::fill());
    let left_side = ui.stack(Orientation::Horizontal)?;
    left_side.append(&left_rail);
    left_side.append(&left_content);
    left_side.set_sizing(Sizing::fill());

    let centre = ui.stack(Orientation::Vertical)?;
    centre.set_spacing(6.0);
    let document_tabs = ui.stack(Orientation::Horizontal)?;
    document_tabs.set_spacing(8.0);
    let code_button = ui.button("Program.hy")?;
    let design_button = ui.button("Main.quarkui [Design]")?;

    let pages = ui.stack(Orientation::Vertical)?;
    pages.set_sizing(Sizing::fill());

    // The full IDE has no Start document. Without an open source or layout,
    // the editing region is a quiet canvas. Project management stays in the
    // separate lightweight launcher.
    let empty_workspace = ui.canvas()?;
    empty_workspace.set_sizing(Sizing::fill());
    empty_workspace.on_draw(|painter| {
        let (r, g, b) = native_shell::QUIET_SURFACE;
        painter.fill_rect(
            Rect::new(0.0, 0.0, painter.width(), painter.height()),
            Color::rgb(r, g, b),
        );
    });

    let code = ui.stack(Orientation::Vertical)?;
    code.append(&ui.label("CODE EDITOR  |  Native multiline text control")?);
    let editor = ui.text_area("")?;
    editor.set_sizing(Sizing::fill());
    code.append(&editor);

    let canvas = ui.canvas()?;
    canvas.set_sizing(Sizing::fill());
    let designer = ui.stack(Orientation::Vertical)?;
    designer.append(&ui.label("WINDOW DESIGNER  |  Native canvas with movable controls")?);
    designer.append(&canvas);

    pages.append(&empty_workspace);
    centre.append(&pages);
    centre.set_sizing(Sizing::fill());

    let solution_pane = ui.stack(Orientation::Vertical)?;
    solution_pane.set_spacing(6.0);
    let solution_heading = ui.stack(Orientation::Horizontal)?;
    let heading = ui.label("SOLUTION EXPLORER")?;
    heading.set_style(naui::TextStyle::Heading);
    solution_heading.append(&heading);
    let refresh_solution = ui.button("⟳")?;
    let collapse_solution = ui.button("−")?;
    solution_heading.append(&refresh_solution);
    solution_heading.append(&collapse_solution);
    solution_pane.append(&solution_heading);
    let solution_search = ui.search_input()?;
    solution_search.set_placeholder("Search Solution Explorer (Ctrl+;)");
    solution_pane.append(&solution_search);
    let tree = ui.tree()?;
    tree.set_sizing(Sizing::fill());
    let (initial_tree, initial_paths) = file_items(&state.borrow());
    let displayed_paths = Rc::new(RefCell::new(initial_paths));
    let solution_query = Rc::new(RefCell::new(String::new()));
    tree.set_items(&initial_tree);
    solution_pane.append(&tree);
    solution_pane.set_sizing(Sizing::fill());

    let properties_pane = ui.stack(Orientation::Vertical)?;
    properties_pane.set_spacing(7.0);
    let properties_heading = ui.label("PROPERTIES")?;
    properties_heading.set_style(naui::TextStyle::Heading);
    properties_pane.append(&properties_heading);
    let selected_name = ui.text_input("")?;
    selected_name.set_placeholder("Name");
    let selected_text = ui.text_input("")?;
    selected_text.set_placeholder("Text");
    properties_pane.append(&ui.label("Control name")?);
    properties_pane.append(&selected_name);
    properties_pane.append(&ui.label("Text")?);
    properties_pane.append(&selected_text);
    let selected_x = ui.text_input("")?;
    let selected_y = ui.text_input("")?;
    let selected_width = ui.text_input("")?;
    let selected_height = ui.text_input("")?;
    for (title, field) in [
        ("X position", &selected_x),
        ("Y position", &selected_y),
        ("Width", &selected_width),
        ("Height", &selected_height),
    ] {
        properties_pane.append(&ui.label(title)?);
        properties_pane.append(field);
    }
    let delete_control = ui.button("Delete Selected Control")?;
    properties_pane.append(&delete_control);
    properties_pane.set_sizing(Sizing::fill());

    let inspector = ui.grid()?;
    inspector.set_sizing(Sizing::fill());
    inspector.set_column_track(0, Track::FILL);
    inspector.set_row_track(0, Track::FILL);
    inspector.set_row_track(1, Track::Fixed(native_layout::HANDLE));
    inspector.set_row_track(2, Track::Fixed(preferred.properties));
    let properties_grip = ui.canvas()?;
    properties_grip.set_sizing(
        Sizing::new()
            .width(Length::Fill)
            .height(Length::Fixed(native_layout::HANDLE)),
    );
    inspector.attach(&solution_pane, GridCell::new(0, 0));
    inspector.attach(&properties_grip, GridCell::new(0, 1));
    inspector.attach(&properties_pane, GridCell::new(0, 2));

    let bottom = ui.stack(Orientation::Vertical)?;
    let bottom_tabs = ui.stack(Orientation::Horizontal)?;
    bottom_tabs.set_spacing(8.0);
    let output_tab = ui.button("Output")?;
    let errors_tab = ui.button("Error List")?;
    let debug_tab = ui.button("Debug")?;
    let test_tab = ui.button("Test Results")?;
    let find_tab = ui.button("Find Results")?;
    bottom_tabs.append(&output_tab);
    bottom_tabs.append(&errors_tab);
    bottom_tabs.append(&debug_tab);
    bottom_tabs.append(&test_tab);
    bottom_tabs.append(&find_tab);
    bottom.append(&bottom_tabs);

    let bottom_pages = ui.stack(Orientation::Vertical)?;
    bottom_pages.set_sizing(Sizing::fill());
    let output = ui.label("Ready. Open a Hydrogen project to build, run or publish.")?;
    let output_scroll = ui.scroll()?;
    output_scroll.set_sizing(Sizing::fill());
    output_scroll.set_child(&output);
    let errors_view = ui.stack(Orientation::Vertical)?;
    let error_summary = ui.label("0 errors, 0 warnings")?;
    let error_list = ui.list()?;
    error_list.set_sizing(Sizing::fill());
    let current_errors = Rc::new(RefCell::new(Vec::<problems::Problem>::new()));
    errors_view.append(&error_summary);
    errors_view.append(&error_list);
    let debug_view =
        ui.label("Debugger not yet attached. Source-level Hydrogen debug support is planned.")?;
    let tests_view =
        ui.label("Test Explorer integration is planned. Use the Hydrogen CLI in the meantime.")?;
    let find_view =
        ui.label("Search results appear here after the solution search service is connected.")?;
    bottom_pages.append(&output_scroll);
    bottom.append(&bottom_pages);
    bottom.set_sizing(Sizing::fill());

    let status = ui.label(&format!("Ready  |  {}  |  Hydrogen  |  UTF-8", backend()))?;
    let workspace_name = ui.label(&display_project_name(&state.borrow()))?;

    // Fixed tool-pane tracks with a Fill editor preserve Visual Studio-style
    // proportions when the OS window is resized. Users adjust the pane sizes
    // using the narrow canvas grips; those preferences are persisted.
    let document_workspace = ui.grid()?;
    document_workspace.set_sizing(Sizing::fill());
    document_workspace.set_column_track(0, Track::FILL);
    document_workspace.set_row_track(0, Track::FILL);
    document_workspace.set_row_track(1, Track::Fixed(native_layout::HANDLE));
    document_workspace.set_row_track(2, Track::Fixed(preferred.output));
    let output_grip = ui.canvas()?;
    output_grip.set_sizing(
        Sizing::new()
            .width(Length::Fill)
            .height(Length::Fixed(native_layout::HANDLE)),
    );
    document_workspace.attach(&centre, GridCell::new(0, 0));
    document_workspace.attach(&output_grip, GridCell::new(0, 1));
    document_workspace.attach(&bottom, GridCell::new(0, 2));

    let main_workspace = ui.grid()?;
    main_workspace.set_sizing(Sizing::fill());
    main_workspace.set_row_track(0, Track::FILL);
    main_workspace.set_column_track(0, Track::Fixed(preferred.toolbox));
    main_workspace.set_column_track(1, Track::Fixed(native_layout::HANDLE));
    main_workspace.set_column_track(2, Track::FILL);
    main_workspace.set_column_track(3, Track::Fixed(native_layout::HANDLE));
    main_workspace.set_column_track(4, Track::Fixed(preferred.inspector));
    let toolbox_grip = ui.canvas()?;
    toolbox_grip.set_sizing(
        Sizing::new()
            .width(Length::Fixed(native_layout::HANDLE))
            .height(Length::Fill),
    );
    let inspector_grip = ui.canvas()?;
    inspector_grip.set_sizing(
        Sizing::new()
            .width(Length::Fixed(native_layout::HANDLE))
            .height(Length::Fill),
    );
    main_workspace.attach(&left_side, GridCell::new(0, 0));
    main_workspace.attach(&toolbox_grip, GridCell::new(1, 0));
    main_workspace.attach(&document_workspace, GridCell::new(2, 0));
    main_workspace.attach(&inspector_grip, GridCell::new(3, 0));
    main_workspace.attach(&inspector, GridCell::new(4, 0));
    root.attach(&main_workspace, GridCell::new(0, 1));

    let refresh_tracks: Rc<dyn Fn(native_layout::PaneSizes)> = Rc::new({
        let main_workspace = main_workspace.clone();
        let document_workspace = document_workspace.clone();
        let inspector = inspector.clone();
        move |sizes| {
            main_workspace.set_column_track(0, Track::Fixed(sizes.toolbox));
            main_workspace.set_column_track(4, Track::Fixed(sizes.inspector));
            document_workspace.set_row_track(2, Track::Fixed(sizes.output));
            inspector.set_row_track(2, Track::Fixed(sizes.properties));
        }
    });
    for (grip, pane) in [
        (&toolbox_grip, native_layout::Pane::Toolbox),
        (&inspector_grip, native_layout::Pane::Inspector),
        (&output_grip, native_layout::Pane::Output),
        (&properties_grip, native_layout::Pane::Properties),
    ] {
        attach_pane_grip(
            grip,
            pane,
            pane_sizes.clone(),
            pane_path.clone(),
            refresh_tracks.clone(),
        );
    }

    let status_bar = ui.stack(Orientation::Horizontal)?;
    status_bar.append(&status);
    status_bar.append(&workspace_name);
    root.attach(&status_bar, GridCell::new(0, 2));

    let reset_window_layout: Rc<dyn Fn()> = Rc::new({
        let pane_sizes = pane_sizes.clone();
        let pane_path = pane_path.clone();
        let refresh_tracks = refresh_tracks.clone();
        let status = status.clone();
        move || {
            let defaults = native_layout::PaneSizes::default();
            pane_sizes.set(defaults);
            refresh_tracks(defaults);
            match defaults.save(&pane_path) {
                Ok(()) => status.set_text("Window layout reset to defaults."),
                Err(error) => status.set_text(&format!("Could not save window layout: {error}")),
            }
        }
    });

    // Native docking/tab selection is independent of the active document.
    let show_bottom: Rc<dyn Fn(usize)> = Rc::new({
        let bottom_pages = bottom_pages.clone();
        let output_scroll = output_scroll.clone();
        let errors_view = errors_view.clone();
        let debug_view = debug_view.clone();
        let tests_view = tests_view.clone();
        let find_view = find_view.clone();
        move |page| {
            bottom_pages.clear();
            match page {
                1 => bottom_pages.append(&errors_view),
                2 => bottom_pages.append(&debug_view),
                3 => bottom_pages.append(&tests_view),
                4 => bottom_pages.append(&find_view),
                _ => bottom_pages.append(&output_scroll),
            }
        }
    });
    for (tab, page) in [
        (&output_tab, 0),
        (&errors_tab, 1),
        (&debug_tab, 2),
        (&test_tab, 3),
        (&find_tab, 4),
    ] {
        let show_bottom = show_bottom.clone();
        tab.on_click(move || show_bottom(page));
    }

    let left_panel: Rc<dyn Fn(usize)> = Rc::new({
        let left_content = left_content.clone();
        let toolbox = toolbox.clone();
        let server_view = server_view.clone();
        let data_view = data_view.clone();
        move |index| {
            left_content.clear();
            match index {
                1 => left_content.append(&server_view),
                2 => left_content.append(&data_view),
                _ => left_content.append(&toolbox),
            }
        }
    });
    for (button, index) in [(&toolbox_tab, 0), (&server_tab, 1), (&data_tab, 2)] {
        let left_panel = left_panel.clone();
        button.on_click(move || left_panel(index));
    }

    // Only opened documents appear as tabs. An empty workspace contains no
    // fake editor or designer tabs and never replaces the launcher.
    let show_page: Rc<dyn Fn(Page)> = Rc::new({
        let state = state.clone();
        let centre = centre.clone();
        let document_tabs = document_tabs.clone();
        let pages = pages.clone();
        let empty_workspace = empty_workspace.clone();
        let code_button = code_button.clone();
        let design_button = design_button.clone();
        let code = code.clone();
        let designer = designer.clone();
        move |requested| {
            let (page, has_code, has_design) = {
                let mut ide = state.borrow_mut();
                let page = ide.display_page(requested);
                ide.page = page;
                (page, ide.document.is_some(), ide.layout.is_some())
            };
            centre.clear();
            document_tabs.clear();
            if has_code {
                document_tabs.append(&code_button);
            }
            if has_design {
                document_tabs.append(&design_button);
            }
            if has_code || has_design {
                centre.append(&document_tabs);
            }
            pages.clear();
            match page {
                Page::Empty => pages.append(&empty_workspace),
                Page::Code => pages.append(&code),
                Page::Design => pages.append(&designer),
            }
            centre.append(&pages);
        }
    });
    for (button, page) in [(&code_button, Page::Code), (&design_button, Page::Design)] {
        let show_page = show_page.clone();
        button.on_click(move || show_page(page));
    }
    for (kind, button) in toolbox_buttons {
        let show_page = show_page.clone();
        let state = state.clone();
        button.on_click(move || {
            state.borrow_mut().armed = Some(kind);
            show_page(Page::Design);
        });
    }
    collapse_solution.on_click({
        let solution_pane = solution_pane.clone();
        let solution_heading = solution_heading.clone();
        let tree = tree.clone();
        let solution_search = solution_search.clone();
        let collapsed = Rc::new(std::cell::Cell::new(false));
        move || {
            let hidden = !collapsed.get();
            collapsed.set(hidden);
            solution_pane.clear();
            solution_pane.append(&solution_heading);
            if !hidden {
                solution_pane.append(&solution_search);
                solution_pane.append(&tree);
            }
        }
    });

    editor.on_change({
        let state = state.clone();
        move |text| {
            if let Some(document) = state.borrow_mut().document.as_mut() {
                document.text = text.to_owned();
            }
        }
    });

    let refresh_project: Rc<dyn Fn()> = Rc::new({
        let state = state.clone();
        let tree = tree.clone();
        let workspace_name = workspace_name.clone();
        let startup_project = startup_project.clone();
        let solution_query = solution_query.clone();
        let displayed_paths = displayed_paths.clone();
        move || {
            let ide = state.borrow();
            let (items, paths) = filtered_file_items(&ide, &solution_query.borrow());
            tree.set_items(&items);
            *displayed_paths.borrow_mut() = paths;
            let project_name = display_project_name(&ide);
            workspace_name.set_text(&project_name);
            startup_project.set_items(&[&project_name]);
            startup_project.set_selected(0);
        }
    });
    solution_search.on_change({
        let solution_query = solution_query.clone();
        let refresh_project = refresh_project.clone();
        move |value| {
            *solution_query.borrow_mut() = value.to_owned();
            refresh_project();
        }
    });
    refresh_solution.on_click({
        let refresh_project = refresh_project.clone();
        move || refresh_project()
    });

    let open_source: Rc<dyn Fn(PathBuf)> = Rc::new({
        let state = state.clone();
        let editor = editor.clone();
        let show_page = show_page.clone();
        let status = status.clone();
        move |path| {
            let result = state.borrow_mut().open_source(&path);
            match result {
                Ok(()) => {
                    let text = state.borrow().document.as_ref().unwrap().text.clone();
                    editor.set_text(&text);
                    show_page(Page::Code);
                    status.set_text(&format!("Editing {}", path.display()));
                }
                Err(error) => status.set_text(&format!("Open failed: {error}")),
            }
        }
    });

    let open_layout: Rc<dyn Fn(PathBuf)> = Rc::new({
        let state = state.clone();
        let show_page = show_page.clone();
        let status = status.clone();
        let canvas = canvas.clone();
        move |path| {
            let result = state.borrow_mut().open_design(&path);
            match result {
                Ok(()) => {
                    show_page(Page::Design);
                    canvas.redraw();
                    status.set_text(&format!("Designing {}", path.display()));
                }
                Err(error) => status.set_text(&format!("Cannot open layout: {error}")),
            }
        }
    });

    let open_project: Rc<dyn Fn(PathBuf)> = Rc::new({
        let state = state.clone();
        let refresh_project = refresh_project.clone();
        let open_source = open_source.clone();
        let status = status.clone();
        move |path| {
            let result = state.borrow_mut().remember_project(path);
            match result {
                Ok(()) => {
                    refresh_project();
                    let first_source = state
                        .borrow()
                        .project
                        .as_ref()
                        .and_then(|project| project.sources.first().cloned());
                    if let Some(file) = first_source {
                        open_source(file);
                    }
                    status.set_text("Project loaded");
                }
                Err(error) => status.set_text(&format!("Cannot open project: {error}")),
            }
        }
    });

    // File > New Project reuses the native launcher wizard as a separate
    // window, without reopening the IDE's removed Start document or closing
    // the IDE when creation completes.
    let show_new_project: Rc<dyn Fn()> = Rc::new({
        let ui = ui.clone();
        let window = window.clone();
        let open_project = open_project.clone();
        let status = status.clone();
        move || {
            let launch: native_start_window::Launch = Rc::new({
                let open_project = open_project.clone();
                move |action| match action {
                    native_start_window::Action::Project(manifest) => {
                        open_project(manifest);
                        Ok(())
                    }
                    _ => Err("Choose a new Hydrogen project".into()),
                }
            });
            if let Err(error) =
                native_start_window::create_project_wizard(&ui, window.clone(), launch, false)
            {
                status.set_text(&format!("Cannot open project wizard: {error}"));
            }
        }
    });

    tree.on_select({
        let displayed_paths = displayed_paths.clone();
        let open_source = open_source.clone();
        let open_layout = open_layout.clone();
        move |position| {
            if position.len() < 2 || position[0] != 0 {
                return;
            }
            let files = displayed_paths.borrow();
            if let Some(path) = files.get(position[1]).cloned() {
                if path.extension().and_then(|ext| ext.to_str()) == Some("quarkui") {
                    open_layout(path);
                } else {
                    open_source(path);
                }
            }
        }
    });

    canvas.on_draw({
        let state = state.clone();
        move |painter| {
            painter.fill_rect(
                Rect::new(0.0, 0.0, painter.width(), painter.height()),
                Color::rgb(0xf2, 0xf5, 0xfa),
            );
            let ide = state.borrow();
            let Some(layout) = ide.layout.as_ref().map(|doc| &doc.layout) else {
                painter.text(
                    "Open or create Main.quarkui to use the designer",
                    Point::new(24.0, 36.0),
                    15.0,
                    Color::BLACK,
                );
                return;
            };
            for control in &layout.controls {
                let colour = if ide.selected_id == Some(control.id) {
                    Color::rgb(0xa4, 0xcf, 0xf6)
                } else {
                    Color::rgb(0xdf, 0xe6, 0xf0)
                };
                painter.fill_rect(
                    Rect::new(
                        f64::from(control.x),
                        f64::from(control.y),
                        f64::from(control.width),
                        f64::from(control.height),
                    ),
                    colour,
                );
                painter.text(
                    &control.text,
                    Point::new(f64::from(control.x) + 8.0, f64::from(control.y) + 8.0),
                    14.0,
                    Color::BLACK,
                );
            }
        }
    });

    canvas.on_pointer({
        let state = state.clone();
        let canvas = canvas.clone();
        let selected_name = selected_name.clone();
        let selected_text = selected_text.clone();
        let selected_x = selected_x.clone();
        let selected_y = selected_y.clone();
        let selected_width = selected_width.clone();
        let selected_height = selected_height.clone();
        let status = status.clone();
        move |event| {
            let mut ide = state.borrow_mut();
            match event.phase {
                PointerPhase::Down => {
                    let (x, y) = (event.point.x as f32, event.point.y as f32);
                    if let Some(kind) = ide.armed.take() {
                        if let Some(layout) = ide.layout.as_mut() {
                            if let Ok(id) = layout.layout.add(kind, x, y) {
                                ide.selected_id = Some(id);
                                status
                                    .set_text(&format!("Placed {} in the designer", kind.title()));
                            }
                        }
                    } else {
                        let hit = ide
                            .layout
                            .as_ref()
                            .and_then(|doc| doc.layout.hit_test(x, y));
                        ide.selected_id = hit;
                        if let Some(id) = hit {
                            let offset = ide
                                .layout
                                .as_ref()
                                .and_then(|doc| doc.layout.controls.iter().find(|c| c.id == id))
                                .map(|c| (x - c.x, y - c.y));
                            if let Some((dx, dy)) = offset {
                                ide.drag = Some((id, dx, dy));
                            }
                        }
                    }
                    if let (Some(doc), Some(id)) = (&ide.layout, ide.selected_id) {
                        if let Some(control) = doc.layout.controls.iter().find(|c| c.id == id) {
                            selected_name.set_text(&control.name);
                            selected_text.set_text(&control.text);
                            selected_x.set_text(&control.x.to_string());
                            selected_y.set_text(&control.y.to_string());
                            selected_width.set_text(&control.width.to_string());
                            selected_height.set_text(&control.height.to_string());
                        }
                    }
                }
                PointerPhase::Move => {
                    if let Some((id, dx, dy)) = ide.drag {
                        if let Some(layout) = ide.layout.as_mut() {
                            layout.layout.position(
                                id,
                                event.point.x as f32 - dx,
                                event.point.y as f32 - dy,
                            );
                        }
                    }
                }
                PointerPhase::Up => {
                    ide.drag = None;
                    if let (Some(doc), Some(id)) = (&ide.layout, ide.selected_id) {
                        if let Some(control) = doc.layout.controls.iter().find(|c| c.id == id) {
                            selected_x.set_text(&control.x.to_string());
                            selected_y.set_text(&control.y.to_string());
                        }
                    }
                }
            }
            drop(ide);
            canvas.redraw();
        }
    });

    selected_name.on_change({
        let state = state.clone();
        move |text| {
            let mut ide = state.borrow_mut();
            let id = ide.selected_id;
            if let Some(control) = ide
                .layout
                .as_mut()
                .and_then(|d| d.layout.controls.iter_mut().find(|c| Some(c.id) == id))
            {
                control.name = text.to_owned();
            }
        }
    });
    selected_text.on_change({
        let state = state.clone();
        let canvas = canvas.clone();
        move |text| {
            let mut ide = state.borrow_mut();
            let id = ide.selected_id;
            if let Some(control) = ide
                .layout
                .as_mut()
                .and_then(|d| d.layout.controls.iter_mut().find(|c| Some(c.id) == id))
            {
                control.text = text.to_owned();
            }
            drop(ide);
            canvas.redraw();
        }
    });
    // Property-grid edits update the same validated design model as dragging.
    // Ignore incomplete numeric input while typing; invalid geometry is
    // rejected without corrupting the layout on disk.
    for (field, widget) in [
        (GeometryField::X, &selected_x),
        (GeometryField::Y, &selected_y),
        (GeometryField::Width, &selected_width),
        (GeometryField::Height, &selected_height),
    ] {
        let state = state.clone();
        let canvas = canvas.clone();
        let status = status.clone();
        widget.on_change(move |text| {
            let Ok(value) = text.trim().parse::<f32>() else {
                return;
            };
            let mut ide = state.borrow_mut();
            let selected_id = ide.selected_id;
            let Some((layout, id)) = ide.layout.as_mut().zip(selected_id) else {
                return;
            };
            let result = layout.layout.set_geometry(id, field, value);
            drop(ide);
            if let Err(error) = result {
                status.set_text(&format!("Invalid control geometry: {error}"));
            }
            canvas.redraw();
        });
    }
    delete_control.on_click({
        let state = state.clone();
        let canvas = canvas.clone();
        move || {
            let mut ide = state.borrow_mut();
            if let Some(id) = ide.selected_id.take() {
                if let Some(doc) = ide.layout.as_mut() {
                    doc.layout.controls.retain(|c| c.id != id);
                }
            }
            drop(ide);
            canvas.redraw();
        }
    });

    let save_action: Rc<dyn Fn()> = Rc::new({
        let state = state.clone();
        let status = status.clone();
        move || {
            let result = state.borrow_mut().save_active();
            status.set_text(match result {
                Ok(()) => "Saved successfully",
                Err(ref error) => error,
            });
        }
    });

    // Build/Run/Publish execute off the main thread and keep the IDE responsive.
    let task_action: Rc<dyn Fn(native_commands::Task)> = Rc::new({
        let state = state.clone();
        let output = output.clone();
        let status = status.clone();
        let error_list = error_list.clone();
        let error_summary = error_summary.clone();
        let current_errors = current_errors.clone();
        let ui = ui.clone();
        let show_bottom = show_bottom.clone();
        move |task| {
            show_bottom(0);
            run_task(
                task,
                state.clone(),
                output.clone(),
                status.clone(),
                error_list.clone(),
                error_summary.clone(),
                current_errors.clone(),
                ui.clone(),
            );
        }
    });
    for (button, task) in [
        (&build_button, native_commands::Task::Build),
        (&run_button, native_commands::Task::RunWithoutDebugging),
        (&publish_button, native_commands::Task::Publish),
    ] {
        let task_action = task_action.clone();
        button.on_click(move || task_action(task));
    }

    error_list.on_select({
        let errors = current_errors.clone();
        let open_source = open_source.clone();
        let status = status.clone();
        move |indices| {
            let problem = indices
                .first()
                .and_then(|&index| errors.borrow().get(index).cloned());
            if let Some(problem) = problem {
                open_source(problem.path.clone());
                status.set_text(&format!(
                    "{}:{} — {}",
                    problem.path.display(),
                    problem.line,
                    problem.message
                ));
            }
        }
    });

    toolbar.on_activate({
        let save_action = save_action.clone();
        let open_project = open_project.clone();
        let show_new_project = show_new_project.clone();
        let task_action = task_action.clone();
        let status = status.clone();
        let solution_search = solution_search.clone();
        move |index| match index {
            0 => show_new_project(),
            1 => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Hydrogen project", &["hyproj"])
                    .pick_file()
                {
                    open_project(path);
                }
            }
            2 => save_action(),
            4 | 5 => status.set_text("Undo/Redo integration is pending for the native editor."),
            7 => task_action(native_commands::Task::Build),
            8 => task_action(native_commands::Task::RunWithoutDebugging),
            9 => task_action(native_commands::Task::Publish),
            10 => {
                solution_search.set_text("");
                status.set_text("Use the Solution Explorer search field to find project files.");
            }
            _ => {}
        }
    });

    menu.on_activate({
        let state = state.clone();
        let save_action = save_action.clone();
        let show_page = show_page.clone();
        let show_new_project = show_new_project.clone();
        let open_project = open_project.clone();
        let open_source = open_source.clone();
        let open_layout = open_layout.clone();
        let refresh_project = refresh_project.clone();
        let task_action = task_action.clone();
        let show_bottom = show_bottom.clone();
        let left_panel = left_panel.clone();
        let status = status.clone();
        let window = window.clone();
        let reset_window_layout = reset_window_layout.clone();
        move |menu, action| match (menu, action) {
            (0, 0) => show_new_project(),
            (0, 1) => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Hydrogen", &["hyproj"])
                    .pick_file()
                {
                    open_project(path);
                }
            }
            (0, 2) => {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    match state.borrow_mut().remember_folder(path) {
                        Ok(()) => {
                            refresh_project();
                            show_page(Page::Empty);
                        }
                        Err(error) => status.set_text(&error),
                    }
                }
            }
            (0, 3) => {
                if let Some(path) = rfd::FileDialog::new().pick_file() {
                    if path.extension().and_then(|ext| ext.to_str()) == Some("quarkui") {
                        open_layout(path);
                    } else {
                        open_source(path);
                    }
                }
            }
            (0, 4) => save_action(),
            (0, 5) => {
                if state.borrow().has_unsaved() {
                    status.set_text(
                        "Save modified documents and layouts before closing the workspace.",
                    );
                } else {
                    state.borrow_mut().clear_workspace();
                    refresh_project();
                    show_page(Page::Empty);
                }
            }
            (0, 6) => {
                if state.borrow().has_unsaved() {
                    status.set_text("Save modified documents and layouts before quitting.");
                } else {
                    window.close();
                }
            }
            (1, _) => status.set_text("Native editor editing commands are being integrated."),
            (2, 0) => show_page(Page::Empty),
            (2, 1) => show_page(Page::Code),
            (2, 2) => show_page(Page::Design),
            (2, 3) => refresh_project(),
            (2, 4) => left_panel(0),
            (2, 5) => show_bottom(0),
            (2, 6) => show_bottom(1),
            (3, 0) => status.set_text("Clone a repository from the Quark Start Window."),
            (3, 1) => status.set_text("Native Git Changes pane is not yet integrated."),
            (4, 0) => {
                let base = state
                    .borrow()
                    .workspace
                    .selected
                    .as_ref()
                    .and_then(|p| p.parent().map(Path::to_path_buf))
                    .or_else(|| state.borrow().workspace.folders.last().cloned());
                if let Some(folder) = base {
                    let mut number = 1_u32;
                    let target = loop {
                        let path = folder.join(format!("Window{number}.quarkui"));
                        if !path.exists() {
                            break path;
                        }
                        number += 1;
                        if number > 200 {
                            status.set_text("Cannot create more UI layouts in this folder.");
                            return;
                        }
                    };
                    match serde_json::to_vec_pretty(&Layout::default()) {
                        Ok(contents) => match crate::storage::atomic_write(&target, &contents) {
                            Ok(()) => {
                                open_layout(target);
                                refresh_project();
                            }
                            Err(error) => {
                                status.set_text(&format!("Cannot create design: {error}"))
                            }
                        },
                        Err(error) => status.set_text(&error.to_string()),
                    }
                } else {
                    status.set_text("Open a project or folder to add a UI layout.");
                }
            }
            (4, 1) => status.set_text("Project Properties integration is planned."),
            (5, 0) => task_action(native_commands::Task::Build),
            (5, 1) => task_action(native_commands::Task::Rebuild),
            (5, 2) => task_action(native_commands::Task::Clean),
            (5, 3) => task_action(native_commands::Task::Publish),
            (6, 0) => {
                show_bottom(2);
                status.set_text("Native source-level Hydrogen debugging is not implemented yet.");
            }
            (6, 1) => task_action(native_commands::Task::RunWithoutDebugging),
            (6, 2) => status.set_text("No native debug session is running."),
            (7, 0) => show_bottom(3),
            (7, 1) => status.set_text("Test execution integration is planned."),
            (8, 0) => {
                show_bottom(1);
                status.set_text(
                    "Static analysis is available through Hydrogen check in the legacy IDE.",
                );
            }
            (9, 0) => status.set_text("Native Options panel is planned."),
            (10, 0) => status.set_text("Native extension management is planned."),
            (11, 0) => reset_window_layout(),
            (12, 0) => status.set_text("Quark IDE — native WinUI 3 / AppKit / GTK 4 preview."),
            _ => {}
        }
    });

    // Projects selected in the small launcher open directly in an editor tab.
    // "Continue without code" enters the full IDE with no selected project.
    let initial_document = {
        let ide = state.borrow();
        (
            ide.document.as_ref().map(|doc| doc.text.clone()),
            ide.layout.is_some(),
        )
    };
    if let Some(text) = initial_document.0 {
        editor.set_text(&text);
        show_page(Page::Code);
    } else if initial_document.1 {
        show_page(Page::Design);
    } else {
        show_page(Page::Empty);
    }

    // The native shell provides the actual OS frame and follows the system's
    // appearance; unlike the legacy renderer, none of these widgets is egui.
    window.set_child(&root);
    window.show();
    Ok(())
}

fn build(ui: &Ui) -> naui::Result<()> {
    let state = Rc::new(RefCell::new(Ide::load()));
    let existing = state.borrow().workspace.recent.clone();
    let handle = ui.clone();
    let launch_state = state.clone();
    let launch: Rc<dyn Fn(native_start_window::Action) -> Result<(), String>> =
        Rc::new(move |action| {
            {
                let mut ide = launch_state.borrow_mut();
                match action {
                    native_start_window::Action::Project(path) => {
                        ide.remember_project(path)?;
                        let source = ide
                            .project
                            .as_ref()
                            .and_then(|project| project.sources.first().cloned());
                        if let Some(source) = source {
                            ide.open_source(&source)?;
                        }
                    }
                    native_start_window::Action::Folder(path) => {
                        ide.remember_folder(path)?;
                    }
                    native_start_window::Action::Empty => {
                        ide.clear_workspace();
                    }
                }
            }
            build_ide(&handle, launch_state.clone()).map_err(|error| error.to_string())
        });
    native_start_window::show(
        ui,
        workspace_file().with_file_name("native-launcher.json"),
        &existing,
        launch,
    )
}

fn main() -> naui::Result<()> {
    naui::run(Settings::new("quark-ide").theme(Theme::System), build)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn platform_name_matches_native_target() {
        #[cfg(target_os = "windows")]
        assert!(backend().contains("WinUI"));
        #[cfg(target_os = "macos")]
        assert_eq!(backend(), "AppKit");
        #[cfg(target_os = "linux")]
        assert!(backend().contains("GTK"));
    }

    #[test]
    fn solution_explorer_paths_include_project_and_ui_layouts() {
        let dir = tempfile::tempdir().unwrap();
        let manifest = dir.path().join("Sample.hyproj");
        fs::write(&manifest, "name = 'Sample'\nsources = ['Program.hy']").unwrap();
        fs::write(dir.path().join("Program.hy"), "class Program {}").unwrap();
        let layout = dir.path().join("Main.quarkui");
        fs::write(&layout, serde_json::to_vec(&Layout::default()).unwrap()).unwrap();
        let mut workspace = Workspace::default();
        workspace.selected = Some(manifest.clone());
        workspace.projects.push(manifest.clone());
        let ide = Ide::from_workspace(workspace);
        assert!(ide.project.is_some());
        let (_, files) = file_items(&ide);
        assert!(files.contains(&manifest.canonicalize().unwrap()));
        assert!(files
            .iter()
            .any(|path| path.file_name().unwrap() == "Main.quarkui"));
    }

    #[test]
    fn continue_without_code_does_not_reopen_an_old_workspace() {
        let mut workspace = Workspace::default();
        workspace.folders.push(PathBuf::from("/unused/old-project"));
        workspace.projects.push(PathBuf::from("/unused/app.hyproj"));
        let mut ide = Ide::from_workspace(workspace);
        ide.clear_workspace();
        assert!(ide.project.is_none());
        assert!(ide.workspace.selected.is_none());
        assert!(ide.workspace.folders.is_empty());
        assert!(ide.workspace.projects.is_empty());
    }

    #[test]
    fn native_shell_preserves_dirty_source_during_failed_save() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("App.hy");
        fs::write(&source, "original").unwrap();
        let mut ide = Ide::from_workspace(Workspace::default());
        ide.open_source(&source).unwrap();
        ide.document.as_mut().unwrap().text = "modified".into();
        assert!(ide.has_unsaved());
        fs::write(&source, "another process").unwrap();
        assert!(ide.save_active().is_err());
        assert!(ide.has_unsaved());
    }
}
