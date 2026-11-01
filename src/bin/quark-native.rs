//! Native Quark frontend preview: WinUI 3 on Windows, AppKit on macOS,
//! GTK 4/libadwaita on Linux. This does not draw egui controls.
//!
//! The existing full-featured egui IDE remains available during the migration.
//! This native frontend deliberately shares .hyproj, .hy and .quarkui files
//! instead of silently converting or rewriting them.

#[path = "../hydrogen.rs"]
mod hydrogen;
#[path = "../native_model.rs"]
mod native_model;
#[path = "../project.rs"]
mod project;
#[path = "../startup.rs"]
mod startup;
#[path = "../storage.rs"]
mod storage;

use naui::{
    Color, GridCell, Length, ListItem, MenuItem, MenuSpec, Orientation, Point,
    PointerPhase, Rect, Settings, Sizing, Theme, ToolbarIcon, ToolbarItem, Track, TreeItem, Ui,
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
        if manifest.extension().and_then(|s| s.to_str()) != Some("hyproj")
            || !manifest.is_file()
        {
            return Err("Choose an existing Hydrogen .hyproj file".into());
        }
        self.workspace.remember(manifest, true);
        self.workspace.save(&workspace_file())?;
        self.project = self.workspace.selected.as_deref().map(project::load);
        self.page = Page::Code;
        Ok(())
    }

    fn open_source(&mut self, file: &Path) -> Result<(), String> {
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
        return (vec![TreeItem::new("Open or create a Hydrogen project")], vec![]);
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

fn run_build(state: Rc<RefCell<Ide>>, output: naui::Label, status: naui::Label, ui: Ui) {
    let plan = {
        let ide = state.borrow();
        let compiler = std::env::var("QUARK_HYDROGEN_COMPILER").unwrap_or_else(|_| "hy".into());
        hydrogen::build_plan(
            &compiler,
            ide.workspace.selected.as_deref(),
            ide.document.as_ref().map(|doc| doc.path.as_path()),
            hydrogen::BuildScope::Project,
        )
    };
    let Ok(plan) = plan else {
        status.set_text(&plan.unwrap_err());
        return;
    };
    let directory = if plan.output_is_directory {
        plan.output.clone()
    } else {
        plan.output
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf()
    };
    if let Err(error) = fs::create_dir_all(directory) {
        status.set_text(&format!("Build folder error: {error}"));
        return;
    }
    status.set_text("Building Hydrogen project…");
    output.set_text(&format!("{}\n", plan.description));
    let sender = ui.tasks().channel(move |message: String| {
        output.set_text(&message);
        status.set_text(if message.contains("[Exit code: 0]") {
            "Build succeeded"
        } else {
            "Build completed — review Output"
        });
    });
    std::thread::spawn(move || {
        let result = std::process::Command::new(&plan.program)
            .args(&plan.args)
            .current_dir(&plan.cwd)
            .output();
        let message = match result {
            Ok(completed) => format!(
                "{}\n{}\n{}\n[Exit code: {}]",
                plan.description,
                String::from_utf8_lossy(&completed.stdout),
                String::from_utf8_lossy(&completed.stderr),
                completed.status.code().unwrap_or(-1)
            ),
            Err(error) => format!("Cannot start Hydrogen compiler: {error}"),
        };
        let _ = sender.send(message);
    });
}

fn build(ui: &Ui) -> naui::Result<()> {
    let state = Rc::new(RefCell::new(Ide::load()));
    let window = ui.window("Quark IDE — Hydrogen Development Environment", 1480.0, 940.0)?;

    // A proper native system title bar, native application menus and a native
    // toolbar. No frameless window, drawn caption buttons or simulated OS controls.
    let menu = ui.menu_bar()?;
    menu.set_menus(&[
        MenuSpec::new("File", [
            MenuItem::new("New Project"),
            MenuItem::new("Open Project"),
            MenuItem::new("Open File"),
            MenuItem::new("Save"),
            MenuItem::new("Exit"),
        ]),
        MenuSpec::new("Edit", ["Undo", "Redo", "Find"]),
        MenuSpec::new("View", ["Start Window", "Code Editor", "UI Designer"]),
        MenuSpec::new("Project", ["New UI Layout", "Solution Explorer"]),
        MenuSpec::new("Build", ["Build Solution"]),
        MenuSpec::new("Debug", ["Start Without Debugging", "Start Debugging (not yet implemented)"]),
        MenuSpec::new("Tools", ["Options"]),
        MenuSpec::new("Window", ["Reset Window Layout"]),
        MenuSpec::new("Help", ["About Quark"]),
    ]);
    window.set_menu_bar(&menu);

    let toolbar = ui.toolbar()?;
    toolbar.set_items(&[
        ToolbarItem::new(ToolbarIcon::New, "New Project"),
        ToolbarItem::new(ToolbarIcon::Open, "Open Project"),
        ToolbarItem::separator(),
        ToolbarItem::new(ToolbarIcon::Save, "Save"),
    ]);
    window.set_toolbar(&toolbar);

    // Professional IDE layout, deliberately different from the VS Code-style
    // activity bar. The centre owns documents/designers; tools and properties
    // live in persistent surrounding panes.
    let root = ui.grid()?;
    root.set_column_track(0, Track::Fixed(200.0));
    root.set_column_track(1, Track::FILL);
    root.set_column_track(2, Track::Fixed(320.0));
    root.set_row_track(0, Track::FILL);
    root.set_row_track(1, Track::Fixed(220.0));
    root.set_row_track(2, Track::Fixed(28.0));

    let toolbox = ui.stack(Orientation::Vertical)?;
    toolbox.set_spacing(9.0);
    toolbox.append(&ui.label("TOOLBOX")?);
    toolbox.append(&ui.label("Common Controls")?);
    for kind in ControlKind::ALL {
        let button = ui.button(kind.title())?;
        let state = state.clone();
        button.on_click(move || {
            let mut ide = state.borrow_mut();
            ide.armed = Some(kind);
            ide.page = Page::Design;
        });
        toolbox.append(&button);
    }
    toolbox.append(&ui.label("Select a control, then click the designer surface to place it.")?);
    root.attach(&toolbox, GridCell::new(0, 0));

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
    recent.set_sizing(Sizing::new().width(Length::Fill).height(Length::Fixed(210.0)));
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
    root.attach(&centre, GridCell::new(1, 0));

    let inspector = ui.stack(Orientation::Vertical)?;
    inspector.set_spacing(9.0);
    inspector.append(&ui.label("SOLUTION EXPLORER")?);
    let tree = ui.tree()?;
    tree.set_sizing(Sizing::new().width(Length::Fill).height(Length::Fixed(310.0)));
    let (initial_tree, _) = file_items(&state.borrow());
    tree.set_items(&initial_tree);
    inspector.append(&tree);
    inspector.append(&ui.label("PROPERTIES")?);
    let selected_name = ui.text_input("")?;
    selected_name.set_placeholder("Name");
    let selected_text = ui.text_input("")?;
    selected_text.set_placeholder("Text");
    inspector.append(&ui.label("Control name")?);
    inspector.append(&selected_name);
    inspector.append(&ui.label("Text")?);
    inspector.append(&selected_text);
    let delete_control = ui.button("Delete Selected Control")?;
    inspector.append(&delete_control);
    root.attach(&inspector, GridCell::new(2, 0));

    let bottom = ui.stack(Orientation::Vertical)?;
    bottom.append(&ui.label("OUTPUT  |  ERROR LIST  |  BUILD")?);
    let output = ui.label("Ready — choose a project from the Start Window.")?;
    let output_scroll = ui.scroll()?;
    output_scroll.set_sizing(Sizing::fill());
    output_scroll.set_child(&output);
    bottom.append(&output_scroll);
    root.attach(&bottom, GridCell::new(1, 1));

    let status = ui.label(&format!(
        "Ready  |  {}  |  Hydrogen  |  UTF-8",
        backend()
    ))?;
    root.attach(&status, GridCell::new(0, 2));
    let workspace_name = ui.label(&display_project_name(&state.borrow()))?;
    root.attach(&workspace_name, GridCell::new(1, 2));

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

    editor.on_change({
        let state = state.clone();
        move |text| {
            if let Some(document) = state.borrow_mut().document.as_mut() {
                document.text = text;
            }
        }
    });

    let refresh_project: Rc<dyn Fn()> = Rc::new({
        let state = state.clone();
        let tree = tree.clone();
        let recent = recent.clone();
        let workspace_name = workspace_name.clone();
        move || {
            let ide = state.borrow();
            let (items, _) = file_items(&ide);
            tree.set_items(&items);
            workspace_name.set_text(&display_project_name(&ide));
            recent.set_items(
                &ide.workspace
                    .recent
                    .iter()
                    .map(|path| ListItem::new(path.display().to_string()))
                    .collect::<Vec<_>>(),
            );
        }
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
            let template = templates.get(selected).copied().unwrap_or(startup::Template::Linux);
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
            if let Some(path) = rfd::FileDialog::new().add_filter("Hydrogen source", &["hy"]).pick_file() {
                open_source(path);
            }
        }
    });

    recent.on_select({
        let state = state.clone();
        let open_project = open_project.clone();
        let status = status.clone();
        move |indices| {
            let Some(&index) = indices.first() else { return };
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
        let state = state.clone();
        let open_source = open_source.clone();
        let open_layout = open_layout.clone();
        move |position| {
            if position.len() < 2 || position[0] != 0 {
                return;
            }
            let files = file_items(&state.borrow()).1;
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
                    Rect::new(control.x, control.y, control.width, control.height),
                    colour,
                );
                painter.text(
                    &control.text,
                    Point::new(control.x + 8.0, control.y + 8.0),
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
                    let (x, y) = (event.point.x, event.point.y);
                    if let Some(kind) = ide.armed.take() {
                        if let Some(layout) = ide.layout.as_mut() {
                            if let Ok(id) = layout.layout.add(kind, x, y) {
                                ide.selected_id = Some(id);
                                status.set_text(&format!("Placed {} in the designer", kind.title()));
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
                            layout.layout.position(id, event.point.x - dx, event.point.y - dy);
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
                control.name = text;
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
                control.text = text;
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

    toolbar.on_activate({
        let save_action = save_action.clone();
        let open_project = open_project.clone();
        let show_page = show_page.clone();
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
            3 => save_action(),
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
        let status = status.clone();
        let tree = tree.clone();
        let output = output.clone();
        let ui = ui.clone();
        move |menu, action| match (menu, action) {
            (0, 0) => show_page(Page::Start),
            (0, 1) => {
                if let Some(path) = rfd::FileDialog::new().add_filter("Hydrogen", &["hyproj"]).pick_file() {
                    open_project(path);
                }
            }
            (0, 2) => {
                if let Some(path) = rfd::FileDialog::new().pick_file() {
                    if path.extension().and_then(|ext| ext.to_str()) == Some("quarkui") {
                        open_layout(path);
                    } else {
                        open_source(path);
                    }
                }
            }
            (0, 3) => save_action(),
            (2, 0) => show_page(Page::Start),
            (2, 1) => show_page(Page::Code),
            (2, 2) => show_page(Page::Design),
            (3, 0) => {
                let path = state.borrow().workspace.selected.as_ref().and_then(|p| p.parent().map(|p| p.join("NewWindow.quarkui")));
                if let Some(path) = path {
                    if !path.exists() {
                        let data = serde_json::to_vec_pretty(&Layout::default()).unwrap_or_default();
                        if let Err(err) = fs::write(&path, data) {
                            status.set_text(&format!("Unable to create design: {err}"));
                            return;
                        }
                    }
                    open_layout(path);
                    let (items, _) = file_items(&state.borrow());
                    tree.set_items(&items);
                }
            }
            (4, 0) => run_build(state.clone(), output.clone(), status.clone(), ui.clone()),
            (5, 0) => status.set_text("Run without debugging uses Hydrogen CLI; select Build Solution first."),
            (5, 1) => status.set_text("Source-level debugger not integrated in this native frontend yet."),
            (8, 0) => status.set_text("Quark native frontend: WinUI 3, AppKit and GTK 4 controls."),
            _ => {}
        }
    });

    // The native shell provides the actual OS frame and follows the system's
    // appearance; unlike the legacy renderer, none of these widgets is egui.
    window.set_child(&root);
    window.show();
    Ok(())
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
        let mut ide = Ide::load();
        ide.remember_project(manifest.clone()).unwrap();
        assert!(ide.project.is_some());
        let (_, files) = file_items(&ide);
        assert!(files.contains(&manifest.canonicalize().unwrap()));
        assert!(files.iter().any(|path| path.file_name().unwrap() == "Main.quarkui"));
    }

    #[test]
    fn native_shell_preserves_dirty_source_during_failed_save() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("App.hy");
        fs::write(&source, "original").unwrap();
        let mut ide = Ide::load();
        ide.open_source(&source).unwrap();
        ide.document.as_mut().unwrap().text = "modified".into();
        assert!(ide.has_unsaved());
        fs::write(&source, "another process").unwrap();
        assert!(ide.save_active().is_err());
        assert!(ide.has_unsaved());
    }
}
