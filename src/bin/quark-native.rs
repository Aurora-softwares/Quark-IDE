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
    Color, Length, ListItem, MenuSpec, Orientation, Point, PointerPhase, Rect, Settings, Sizing,
    Theme, ToolbarIcon, ToolbarItem, TreeItem, Ui,
};
use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
};

use native_model::{ControlKind, Document, Layout, LayoutDocument, Workspace};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Page {
    Start,
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
            page: Page::Start,
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
        self.workspace.remember(folder, false);
        self.workspace.selected = None;
        self.workspace.save(&workspace_file())?;
        self.project = None;
        self.document = None;
        self.layout = None;
        self.page = Page::Code;
        if let Some(path) = self.workspace.folders.last() {
            record_recent(path.clone());
        }
        Ok(())
    }

    fn clear_workspace(&mut self) {
        self.project = None;
        self.workspace.selected = None;
        self.document = None;
        self.layout = None;
        self.page = Page::Start;
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
            Page::Start => Err("Select a source file or UI layout first".into()),
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
    let root = ui.stack(Orientation::Vertical)?;
    root.set_sizing(Sizing::fill());

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
    root.append(&command_strip);

    let toolbox = ui.stack(Orientation::Vertical)?;
    toolbox.set_spacing(9.0);
    toolbox.append(&ui.label("TOOLBOX")?);
    toolbox.append(&ui.label("Common Controls")?);
    let mut toolbox_buttons = Vec::new();
    for kind in ControlKind::ALL {
        let button = ui.button(kind.title())?;
        toolbox.append(&button);
        toolbox_buttons.push((kind, button));
    }
    toolbox.append(&ui.label("Select a control, then click the designer surface to place it.")?);
    toolbox.set_sizing(Sizing::fill());

    // Visual Studio's compact auto-hide rail lives to the left of Toolbox.
    let left_rail = ui.stack(Orientation::Vertical)?;
    left_rail.set_spacing(5.0);
    left_rail.set_sizing(Sizing::new().width(Length::Fixed(85.0)));
    let server_tab = ui.button("Server")?;
    let toolbox_tab = ui.button("Toolbox")?;
    let data_tab = ui.button("Data")?;
    left_rail.append(&server_tab);
    left_rail.append(&toolbox_tab);
    left_rail.append(&data_tab);
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
    let start_button = ui.button("Start Window")?;
    let code_button = ui.button("Program.hy")?;
    let design_button = ui.button("Main.quarkui [Design]")?;
    document_tabs.append(&start_button);
    document_tabs.append(&code_button);
    document_tabs.append(&design_button);
    centre.append(&document_tabs);

    let pages = ui.stack(Orientation::Vertical)?;
    pages.set_sizing(Sizing::fill());

    let start = ui.stack(Orientation::Vertical)?;
    start.set_spacing(12.0);
    start.append(&ui.label("QUARK  |  START WINDOW")?);
    start.append(&ui.label("Get started with a Hydrogen project")?);
    let parent_field = ui.text_input("")?;
    parent_field.set_placeholder("Parent directory for a new project");
    parent_field.set_text(
        &std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .to_string_lossy(),
    );
    let name_field = ui.text_input("MyHydrogenApp")?;
    let template_box = ui.combo_box()?;
    template_box.set_items(&[
        "Linux Application",
        "Windows Application",
        "macOS Application",
        "Australis Executable",
        "Hydrogen Library",
    ]);
    template_box.set_selected(0);
    let create_button = ui.button("Create New Project")?;
    let open_project_button = ui.button("Open Existing Project…")?;
    let open_file_button = ui.button("Open Source File…")?;
    start.append(&ui.label("Project name")?);
    start.append(&name_field);
    start.append(&ui.label("Create in folder")?);
    start.append(&parent_field);
    start.append(&ui.label("Target platform")?);
    start.append(&template_box);
    start.append(&create_button);
    start.append(&open_project_button);
    start.append(&open_file_button);

    start.append(&ui.label("RECENT PROJECTS")?);
    let recent = ui.list()?;
    let initial_recent = state.borrow().workspace.recent.clone();
    recent.set_items(
        &initial_recent
            .iter()
            .map(|path| ListItem::new(path.display().to_string()))
            .collect::<Vec<_>>(),
    );
    recent.set_sizing(
        Sizing::new()
            .width(Length::Fill)
            .height(Length::Fixed(210.0)),
    );
    start.append(&recent);

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

    pages.append(&start);
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
    let delete_control = ui.button("Delete Selected Control")?;
    properties_pane.append(&delete_control);
    properties_pane.set_sizing(Sizing::fill());

    let inspector = ui.split_view(Orientation::Vertical)?;
    inspector.set_start(&solution_pane);
    inspector.set_end(&properties_pane);
    inspector.set_position(430.0);
    inspector.set_min_sizes(190.0, 160.0);
    inspector.set_sizing(Sizing::fill());

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

    // Native resizable pane splitters: NSSplitView on macOS, GtkPaned on
    // Linux and WinUI 3 Grid-based dividers on Windows.
    let edit_and_output = ui.split_view(Orientation::Vertical)?;
    edit_and_output.set_start(&centre);
    edit_and_output.set_end(&bottom);
    edit_and_output.set_position(640.0);
    edit_and_output.set_min_sizes(260.0, 130.0);
    edit_and_output.set_sizing(Sizing::fill());

    let project_and_editor = ui.split_view(Orientation::Horizontal)?;
    project_and_editor.set_start(&edit_and_output);
    project_and_editor.set_end(&inspector);
    project_and_editor.set_position(930.0);
    project_and_editor.set_min_sizes(350.0, 220.0);
    project_and_editor.set_sizing(Sizing::fill());

    let main_workspace = ui.split_view(Orientation::Horizontal)?;
    main_workspace.set_start(&left_side);
    main_workspace.set_end(&project_and_editor);
    main_workspace.set_position(210.0);
    main_workspace.set_min_sizes(160.0, 500.0);
    main_workspace.set_sizing(Sizing::fill());
    root.append(&main_workspace);

    let status_bar = ui.stack(Orientation::Horizontal)?;
    status_bar.append(&status);
    status_bar.append(&workspace_name);
    root.append(&status_bar);

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

    // Pane display is controlled separately from the persistent file model.
    let show_page: Rc<dyn Fn(Page)> = Rc::new({
        let state = state.clone();
        let pages = pages.clone();
        let start = start.clone();
        let code = code.clone();
        let designer = designer.clone();
        move |page| {
            state.borrow_mut().page = page;
            pages.clear();
            match page {
                Page::Start => pages.append(&start),
                Page::Code => pages.append(&code),
                Page::Design => pages.append(&designer),
            }
        }
    });
    for (button, page) in [
        (&start_button, Page::Start),
        (&code_button, Page::Code),
        (&design_button, Page::Design),
    ] {
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
        let recent = recent.clone();
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
            recent.set_items(
                &ide.workspace
                    .recent
                    .iter()
                    .map(|path| ListItem::new(path.display().to_string()))
                    .collect::<Vec<_>>(),
            );
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

    create_button.on_click({
        let state = state.clone();
        let open_project = open_project.clone();
        let status = status.clone();
        let parent_field = parent_field.clone();
        let name_field = name_field.clone();
        let template_box = template_box.clone();
        let canvas = canvas.clone();
        let show_page = show_page.clone();
        move || {
            let name = name_field.text();
            let parent = PathBuf::from(parent_field.text());
            let templates = startup::Template::ALL;
            let selected = template_box.selected().unwrap_or(0);
            let template = templates
                .get(selected)
                .copied()
                .unwrap_or(startup::Template::Linux);
            match startup::create_project(&parent, &name, template) {
                Ok(manifest) => {
                    let layout_path = manifest.parent().unwrap().join("Main.quarkui");
                    let data = serde_json::to_vec_pretty(&Layout::default()).unwrap_or_default();
                    if let Err(error) = fs::write(&layout_path, data) {
                        status.set_text(&format!("Created project, cannot create layout: {error}"));
                    }
                    open_project(manifest);
                    if layout_path.exists() {
                        if let Ok(doc) = LayoutDocument::open(&layout_path) {
                            state.borrow_mut().layout = Some(doc);
                            show_page(Page::Design);
                            canvas.redraw();
                        }
                    }
                }
                Err(error) => status.set_text(&format!("New project: {error}")),
            }
        }
    });

    open_project_button.on_click({
        let open_project = open_project.clone();
        move || {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Hydrogen project", &["hyproj"])
                .pick_file()
            {
                open_project(path);
            }
        }
    });
    open_file_button.on_click({
        let open_source = open_source.clone();
        move || {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Hydrogen source", &["hy"])
                .pick_file()
            {
                open_source(path);
            }
        }
    });

    recent.on_select({
        let state = state.clone();
        let open_project = open_project.clone();
        let status = status.clone();
        move |indices| {
            let Some(&index) = indices.first() else {
                return;
            };
            let path = state.borrow().workspace.recent.get(index).cloned();
            if let Some(path) = path {
                if path.extension().and_then(|s| s.to_str()) == Some("hyproj") {
                    open_project(path);
                } else {
                    status.set_text("Recent folder support is available through Open Project.");
                }
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
                PointerPhase::Up => ide.drag = None,
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
        let show_page = show_page.clone();
        let task_action = task_action.clone();
        let status = status.clone();
        let solution_search = solution_search.clone();
        move |index| match index {
            0 => show_page(Page::Start),
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
        let open_project = open_project.clone();
        let open_source = open_source.clone();
        let open_layout = open_layout.clone();
        let refresh_project = refresh_project.clone();
        let task_action = task_action.clone();
        let show_bottom = show_bottom.clone();
        let left_panel = left_panel.clone();
        let status = status.clone();
        let window = window.clone();
        move |menu, action| match (menu, action) {
            (0, 0) => show_page(Page::Start),
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
                            show_page(Page::Start);
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
                    show_page(Page::Start);
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
            (2, 0) => show_page(Page::Start),
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
            (11, 0) => status.set_text("Window panes can be resized by dragging their separators."),
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
        show_page(Page::Start);
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
