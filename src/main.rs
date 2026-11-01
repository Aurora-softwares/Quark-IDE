mod catalog;
mod catalog_ui;
mod command;
mod designer;
mod diagnostics;
mod editor;
#[cfg(test)]
mod feature_tests;
mod hydrogen;
#[cfg(test)]
#[allow(dead_code)]
mod native_commands;
#[cfg(test)]
#[allow(dead_code)]
mod native_launcher;
#[cfg(test)]
#[allow(dead_code)]
mod native_layout;
#[cfg(test)]
#[allow(dead_code)] // The native frontend uses the rest of this API.
mod native_model;
#[cfg(test)]
#[allow(dead_code)]
mod native_shell;
mod navigation;
mod panels;
mod plugins;
mod problems;
mod project;
mod startup;
mod storage;
mod symbols;
mod terminal;
mod theme;
mod ui;
mod window;
mod workspace;

use editor::{completion_candidates, insert_completion, prefix_at};
use eframe::egui::{self, Key, Modifiers, RichText, TextEdit};
use hydrogen::BuildScope;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
    thread,
    time::Duration,
};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Document {
    id: u64,
    path: Option<PathBuf>,
    text: String,
    saved: String,
}
impl Document {
    fn label(&self) -> String {
        let name = self
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("Untitled-{}", self.id));
        if self.text != self.saved {
            format!("* {name}")
        } else {
            name
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct RecentWorkspace {
    path: PathBuf,
    project: bool,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct Settings {
    folders: Vec<PathBuf>,
    projects: Vec<PathBuf>,
    compiler: String,
    word_wrap: bool,
    selected_project: Option<PathBuf>,
    disabled_plugins: Vec<String>,
    #[serde(default = "default_visible")]
    sidebar_visible: bool,
    integrated_title_bar: bool,
    #[serde(default = "default_visible")]
    terminal_visible: bool,
    automatic_completion: bool,
    include_generated: bool,
    index_limit: usize,
    command_timeout_seconds: u64,
    command_folder: Option<PathBuf>,
    compiler_verbs: Vec<String>,
    theme: theme::Settings,
    catalog: catalog::Preferences,
    sidebar_view: SidebarView,
    bottom_tab: BottomTab,
    recent_workspaces: Option<Vec<RecentWorkspace>>,
    welcome_shortcuts_visible: bool,
    welcome_build_visible: bool,
}
fn default_visible() -> bool {
    true
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            folders: vec![],
            projects: vec![],
            compiler: String::new(),
            word_wrap: false,
            selected_project: None,
            disabled_plugins: vec![],
            sidebar_visible: true,
            integrated_title_bar: true,
            terminal_visible: false,
            automatic_completion: true,
            include_generated: false,
            index_limit: 50_000,
            command_timeout_seconds: 600,
            command_folder: None,
            compiler_verbs: vec!["build".into(), "check".into(), "run".into()],
            theme: theme::Settings::default(),
            catalog: catalog::Preferences::default(),
            sidebar_view: SidebarView::Explorer,
            bottom_tab: BottomTab::Terminal,
            recent_workspaces: None,
            welcome_shortcuts_visible: true,
            welcome_build_visible: true,
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq, Clone, Copy, serde::Serialize, serde::Deserialize)]
enum SidebarView {
    #[default]
    Explorer,
    Search,
    SourceControl,
    Plugins,
    Themes,
}
#[derive(Debug, Default, PartialEq, Eq, Clone, Copy, serde::Serialize, serde::Deserialize)]
enum BottomTab {
    #[default]
    Terminal,
    Output,
    Problems,
}
#[derive(PartialEq, Clone, Copy)]
enum ExplorerView {
    Files,
    Outline,
}

#[derive(Clone)]
struct CompletionPopup {
    doc_id: u64,
    edit_id: egui::Id,
    cursor_char: usize,
    prefix: String,
    candidates: Vec<String>,
    anchor: egui::Pos2,
    selected: usize,
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Session {
    docs: Vec<Document>,
    active: Option<usize>,
}

struct Quark {
    next_doc_id: u64,
    workspace: workspace::Snapshot,
    workspace_rx: Option<Receiver<workspace::Snapshot>>,
    workspace_requested: bool,
    tree_limits: HashMap<PathBuf, usize>,
    notices: Vec<String>,
    session_due: f64,
    session_last: String,
    settings_path: Option<PathBuf>,
    session_path: Option<PathBuf>,
    settings_blocked: bool,
    session_blocked: bool,
    exit_requested: bool,
    allow_exit: bool,
    pending_close: Option<usize>,
    pending_conflict: Option<usize>,
    quick_focus: bool,
    quick_selected: usize,
    quick_filtered: Vec<PathBuf>,
    quick_filter: String,
    quick_revision: u64,
    workspace_revision: u64,
    show_palette: bool,
    palette_query: String,
    palette_focus: bool,
    palette_selected: usize,
    job_cancel: Option<Arc<AtomicBool>>,
    job_worker: Option<thread::JoinHandle<()>>,
    job_cwd: Option<PathBuf>,
    pending_clone: Option<PathBuf>,
    probe_running: bool,
    probe_output: String,
    applied_theme: Option<theme::Settings>,
    applied_window_chrome: Option<bool>,
    theme_persist_due: Option<f64>,
    terminal: Option<terminal::Terminal>,
    terminal_error: Option<String>,
    theme_query: String,
    extension_query: String,
    editing_theme: Option<String>,
    editing_extension: Option<String>,
    catalog_add: Option<bool>,
    pending_extension_remove: Option<String>,
    workspace_search: bool,
    search_rx: Option<Receiver<panels::SearchResults>>,
    search_results: panels::SearchResults,
    search_generation: u64,
    pending_line: Option<(PathBuf, usize)>,
    sidebar_rect: egui::Rect,
    bottom_rect: egui::Rect,
    top_rect: egui::Rect,
    explorer_view: ExplorerView,
    tree_revision: u64,
    tree_default_open: bool,
    plugins: Vec<plugins::Plugin>,
    plugin_warnings: Vec<String>,
    completion: Option<CompletionPopup>,
    dismissed_completion: Option<(u64, usize, String)>,
    manual_completion: bool,
    focus_editor: bool,
    settings: Settings,
    docs: Vec<Document>,
    active: Option<usize>,
    output: String,
    problems: Vec<problems::Problem>,
    problem_output: String,
    problem_cwd: Option<PathBuf>,
    search: String,
    quick_query: String,
    show_search: bool,
    find_focus: bool,
    show_replace: bool,
    replace_with: String,
    show_go_to_line: bool,
    go_line_input: String,
    go_focus: bool,
    caret_position: Option<(usize, usize)>,
    show_settings: bool,
    show_quick_open: bool,
    show_clone: bool,
    clone_url: String,
    job_rx: Option<Receiver<command::Event>>,
    status: String,
    diagnostics: diagnostics::ByFile,
    diagnostics_revision: u64,
    diagnostics_requested: bool,
    diagnostics_due: f64,
    diagnostic_rx: Option<Receiver<diagnostics::ScanResult>>,
    analysis_complete: bool,
    files_scanned: usize,
    designer: Option<designer::Designer>,
    designer_active: bool,
    pending_designer_close: bool,
    show_new_project: bool,
    new_project_name: String,
    new_project_parent: String,
    new_project_template: startup::Template,
    show_new_layout: bool,
    new_layout_name: String,
}
impl Quark {
    fn show_welcome(&mut self) {
        self.active = None;
        self.designer_active = false;
    }

    fn save_designer(&mut self) -> bool {
        let result = self.designer.as_mut().map(|d| d.save());
        match result {
            Some(Ok(())) => {
                self.status = "UI layout saved".into();
                self.workspace_requested = true;
                true
            }
            Some(Err(e)) => {
                self.status = e.clone();
                self.notice(e);
                false
            }
            None => true,
        }
    }

    fn save_active(&mut self) -> bool {
        if self.designer_active {
            self.save_designer()
        } else if let Some(index) = self.active {
            self.save(index)
        } else {
            true
        }
    }

    fn open_designer(&mut self, path: PathBuf) {
        if self
            .designer
            .as_ref()
            .is_some_and(|d| d.dirty && d.path != path)
        {
            self.status = "Save the current UI layout before opening another layout".into();
            return;
        }
        let path = path.canonicalize().unwrap_or(path);
        if self.designer.as_ref().is_some_and(|d| d.path == path) {
            self.designer_active = true;
            self.active = None;
            return;
        }
        match designer::Designer::open(path) {
            Ok(document) => {
                self.designer = Some(document);
                self.designer_active = true;
                self.active = None;
            }
            Err(error) => self.status = format!("Cannot open layout: {error}"),
        }
    }

    fn close_designer(&mut self) {
        if self.designer.as_ref().is_some_and(|d| d.dirty) {
            self.pending_designer_close = true;
        } else {
            self.designer = None;
            self.designer_active = false;
        }
    }

    fn start_project_dialog(&mut self) {
        self.show_new_project = true;
    }

    fn new_project_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_new_project {
            return;
        }
        let mut open = true;
        let mut create = false;
        egui::Window::new("Create a Hydrogen project")
            .open(&mut open)
            .default_width(480.0)
            .resizable(false)
            .show(ctx, |ui| {
                ui.heading("New project");
                ui.label("Project name");
                ui.text_edit_singleline(&mut self.new_project_name);
                ui.label("Parent folder");
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.new_project_parent);
                    if ui.button("Browse…").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_folder() {
                            self.new_project_parent = path.display().to_string();
                        }
                    }
                });
                ui.label("Template");
                egui::ComboBox::from_id_salt("quark-project-template")
                    .selected_text(self.new_project_template.label())
                    .show_ui(ui, |ui| {
                        for template in startup::Template::ALL {
                            ui.selectable_value(&mut self.new_project_template, template, template.label());
                        }
                    });
                ui.label("Quark creates a Hydrogen .hyproj and Program.hy.");
                ui.label("The visual designer saves .quarkui layouts separately from executable source code.");
                create = ui.button("Create project").clicked();
            });
        if create {
            let parent = PathBuf::from(self.new_project_parent.trim());
            let name = self.new_project_name.trim();
            match startup::create_project(&parent, name, self.new_project_template) {
                Ok(path) => {
                    self.show_new_project = false;
                    self.add_project(path.clone());
                    self.status = format!("Created project {}", path.display());
                    let layout_path = path.parent().unwrap_or(Path::new(".")).join("Main.quarkui");
                    match designer::Designer::create(layout_path) {
                        Ok(document) => {
                            self.designer = Some(document);
                            self.designer_active = true;
                            self.active = None;
                            self.workspace_requested = true;
                        }
                        Err(error) => self.notice(format!(
                            "Project created, but could not create the starting UI layout: {error}"
                        )),
                    }
                }
                Err(error) => self.status = error,
            }
        }
        if !open {
            self.show_new_project = false;
        }
    }

    fn new_layout_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_new_layout {
            return;
        }
        let mut open = true;
        let mut create = false;
        let parent = self
            .selected_project()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .or_else(|| self.settings.folders.first().cloned());
        egui::Window::new("New UI layout")
            .open(&mut open)
            .default_width(430.0)
            .show(ctx, |ui| {
                ui.heading("New visual layout");
                ui.label("Layout name");
                ui.text_edit_singleline(&mut self.new_layout_name);
                if let Some(parent) = &parent {
                    ui.label(format!("Project folder: {}", parent.display()));
                } else {
                    ui.label("Open a project or folder to create a layout.");
                }
                create = ui
                    .add_enabled(parent.is_some(), egui::Button::new("Create layout"))
                    .clicked();
            });
        if create {
            if !startup::valid_name(&self.new_layout_name) {
                self.status = "Use a layout name starting with a letter, containing only letters, numbers, hyphens or underscores.".into();
            } else if self.designer.as_ref().is_some_and(|d| d.dirty) {
                self.status = "Save the current UI layout before creating another".into();
            } else if let Some(parent) = parent {
                let path = parent.join(format!("{}.quarkui", self.new_layout_name));
                match designer::Designer::create(path) {
                    Ok(document) => {
                        self.designer = Some(document);
                        self.designer_active = true;
                        self.active = None;
                        self.workspace_requested = true;
                        self.show_new_layout = false;
                    }
                    Err(error) => self.status = error,
                }
            }
        }
        if !open {
            self.show_new_layout = false;
        }
    }

    fn request_exit(&mut self, ctx: &egui::Context) {
        if self.docs.iter().any(|d| d.text != d.saved)
            || self.designer.as_ref().is_some_and(|d| d.dirty)
            || self.job_rx.is_some()
            || self.terminal.as_ref().is_some_and(|t| t.exited.is_none())
        {
            self.exit_requested = true;
        } else {
            self.save_session();
            self.allow_exit = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (settings, settings_warning) = load_json::<Settings>(settings_file().as_deref());
        let (session, session_warning) = load_json::<Session>(session_file().as_deref());
        let mut app = Self::from_state(settings, session, settings_warning, session_warning);
        app.reload_plugins();
        theme::apply(&cc.egui_ctx, &app.effective_theme());
        app.applied_theme = Some(app.effective_theme());
        app
    }
    fn from_state(
        mut settings: Settings,
        session: Session,
        settings_warning: Option<String>,
        session_warning: Option<String>,
    ) -> Self {
        if let Some(custom) = settings.theme.custom.as_ref() {
            if !settings
                .catalog
                .themes()
                .iter()
                .any(|item| &item.definition == custom)
            {
                settings.catalog.install_theme(custom.clone());
            }
        }
        if settings.recent_workspaces.is_none() {
            settings.recent_workspaces = Some(
                settings
                    .projects
                    .iter()
                    .map(|path| RecentWorkspace {
                        path: path.clone(),
                        project: true,
                    })
                    .chain(settings.folders.iter().map(|path| RecentWorkspace {
                        path: path.clone(),
                        project: false,
                    }))
                    .take(20)
                    .collect(),
            );
        }
        let settings_blocked = settings_warning.is_some();
        let session_blocked = session_warning.is_some();
        let mut notices: Vec<String> = settings_warning
            .into_iter()
            .chain(session_warning)
            .collect();
        if settings_file().is_none() {
            notices.push(
                "Configuration folder unavailable: settings and recovery cannot be saved.".into(),
            );
        }
        let next_doc_id = session
            .docs
            .iter()
            .map(|d| d.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let restored_active = session.active.filter(|i| *i < session.docs.len()).or({
            if session.docs.is_empty() {
                None
            } else {
                Some(0)
            }
        });
        let mut q = Self {
            settings,
            next_doc_id,
            workspace: workspace::Snapshot::default(),
            workspace_rx: None,
            workspace_requested: true,
            tree_limits: HashMap::new(),
            notices,
            session_due: 0.0,
            session_last: String::new(),
            settings_path: settings_file(),
            session_path: session_file(),
            settings_blocked,
            session_blocked,
            exit_requested: false,
            allow_exit: false,
            pending_close: None,
            pending_conflict: None,
            quick_focus: false,
            quick_selected: 0,
            quick_filtered: Vec::new(),
            quick_filter: String::new(),
            quick_revision: u64::MAX,
            workspace_revision: 0,
            show_palette: false,
            palette_query: String::new(),
            palette_focus: false,
            palette_selected: 0,
            job_cancel: None,
            job_worker: None,
            job_cwd: None,
            pending_clone: None,
            probe_running: false,
            probe_output: String::new(),
            applied_theme: None,
            applied_window_chrome: None,
            theme_persist_due: None,
            terminal: None,
            terminal_error: None,
            theme_query: String::new(),
            extension_query: String::new(),
            editing_theme: None,
            editing_extension: None,
            catalog_add: None,
            pending_extension_remove: None,
            workspace_search: false,
            search_rx: None,
            search_results: panels::SearchResults::default(),
            search_generation: 0,
            pending_line: None,
            sidebar_rect: egui::Rect::NOTHING,
            bottom_rect: egui::Rect::NOTHING,
            top_rect: egui::Rect::NOTHING,
            explorer_view: ExplorerView::Files,
            tree_revision: 0,
            tree_default_open: false,
            plugins: Vec::new(),
            plugin_warnings: Vec::new(),
            completion: None,
            dismissed_completion: None,
            manual_completion: false,
            focus_editor: true,
            docs: session.docs,
            active: restored_active,
            output: "Quark IDE — native Rust edition\n".into(),
            problems: Vec::new(),
            problem_output: String::new(),
            problem_cwd: None,
            search: String::new(),
            quick_query: String::new(),
            show_search: false,
            find_focus: false,
            show_replace: false,
            replace_with: String::new(),
            show_go_to_line: false,
            go_line_input: String::new(),
            go_focus: false,
            caret_position: None,
            show_settings: false,
            show_quick_open: false,
            show_clone: false,
            clone_url: String::new(),
            job_rx: None,
            status: "Welcome to Quark".into(),
            diagnostics: HashMap::new(),
            diagnostics_revision: 0,
            diagnostics_requested: true,
            diagnostics_due: 0.0,
            diagnostic_rx: None,
            analysis_complete: false,
            files_scanned: 0,
            designer: None,
            designer_active: false,
            pending_designer_close: false,
            show_new_project: false,
            new_project_name: "MyHydrogenApp".into(),
            new_project_parent: std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .to_string_lossy()
                .into_owned(),
            new_project_template: startup::Template::Linux,
            show_new_layout: false,
            new_layout_name: "Main".into(),
        };
        if q.settings.compiler.is_empty() {
            q.settings.compiler = "hy".into();
        }
        // Refresh clean restored tabs from disk; dirty buffers retain their
        // saved baseline so an external edit still triggers conflict protection.
        for doc in &mut q.docs {
            if doc.text == doc.saved {
                if let Some(text) = doc.path.as_ref().and_then(|p| fs::read_to_string(p).ok()) {
                    doc.text = text.clone();
                    doc.saved = text;
                }
            }
        }
        if q.docs.iter().any(|d| d.text != d.saved) {
            q.notice("Restored unsaved work from the last recovery snapshot. Review your files, then save when ready.".into());
        }
        q
    }
    fn request_diagnostic_scan(&mut self) {
        self.diagnostics.clear();
        self.diagnostics_revision = self.diagnostics_revision.wrapping_add(1);
        self.diagnostics_requested = true;
        self.diagnostics_due = 0.0;
    }

    fn diagnostic_tick(&mut self, ctx: &egui::Context) {
        if let Some(result) = self.diagnostic_rx.as_ref().map(|rx| rx.try_recv()) {
            match result {
                Ok(result) => {
                    if result.revision == self.diagnostics_revision {
                        self.diagnostics = result.by_file;
                        self.files_scanned = result.files_scanned;
                        self.analysis_complete = result.complete;
                    }
                    self.diagnostic_rx = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.diagnostic_rx = None;
                    self.diagnostics_requested = true;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        let now = ctx.input(|i| i.time);
        if self.diagnostics_requested && self.diagnostic_rx.is_none() && now >= self.diagnostics_due
        {
            self.diagnostics_requested = false;
            let revision = self.diagnostics_revision;
            let project = self.selected_project();
            let docs: Vec<_> = self
                .docs
                .iter()
                .map(|d| (diagnostic_path(d), d.text.clone()))
                .collect();
            let (send, rx) = mpsc::channel();
            self.diagnostic_rx = Some(rx);
            thread::spawn(move || {
                let result = diagnostics::scan(revision, project.as_deref(), &docs);
                let _ = send.send(result);
            });
        }
        if self.diagnostic_rx.is_some() || self.diagnostics_requested {
            ctx.request_repaint_after(std::time::Duration::from_millis(160));
        }
    }

    fn persist(&mut self) {
        if self.settings_blocked {
            return;
        }
        let result = self
            .settings_path
            .clone()
            .ok_or_else(|| "No configuration directory".to_owned())
            .and_then(|p| {
                serde_json::to_vec_pretty(&self.settings)
                    .map_err(|e| e.to_string())
                    .and_then(|s| storage::atomic_write(&p, &s).map_err(|e| e.to_string()))
            });
        if let Err(e) = result {
            self.notice(format!("Settings not saved: {e}"));
        }
    }
    fn notice(&mut self, message: String) {
        self.status = message.clone();
        if !self.notices.contains(&message) {
            self.notices.push(message);
        }
    }
    fn save_session(&mut self) {
        if self.session_blocked {
            return;
        }
        let session = Session {
            docs: self.docs.clone(),
            active: self.active,
        };
        if let Ok(json) = serde_json::to_string(&session) {
            if json != self.session_last {
                match self
                    .session_path
                    .clone()
                    .ok_or_else(|| "No recovery directory".into())
                    .and_then(|p| {
                        storage::atomic_write(&p, json.as_bytes()).map_err(|e| e.to_string())
                    }) {
                    Ok(()) => self.session_last = json,
                    Err(e) => self.notice(format!("Recovery snapshot not saved: {e}")),
                }
            }
        }
    }
    fn workspace_tick(&mut self) {
        if let Some(result) = self.workspace_rx.as_ref().map(|rx| rx.try_recv()) {
            match result {
                Ok(snapshot) => {
                    self.workspace = snapshot;
                    self.workspace_revision = self.workspace_revision.wrapping_add(1);
                    self.workspace_rx = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.workspace_rx = None;
                    self.notice("Workspace scan failed; click Refresh to retry.".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.workspace_requested && self.workspace_rx.is_none() {
            self.workspace_requested = false;
            let roots = self.settings.folders.clone();
            let projects = self.settings.projects.clone();
            let buffers: Vec<_> = self
                .docs
                .iter()
                .filter_map(|d| d.path.clone().map(|p| (p, d.text.clone())))
                .collect();
            let generated = self.settings.include_generated;
            let limit = self.settings.index_limit;
            let (tx, rx) = mpsc::channel();
            self.workspace_rx = Some(rx);
            thread::spawn(move || {
                let _ = tx.send(workspace::scan(
                    &roots, &projects, &buffers, generated, limit,
                ));
            });
        }
    }
    fn tool_error(&mut self, message: String) {
        self.status = message.clone();
        self.settings.terminal_visible = true;
        self.settings.bottom_tab = BottomTab::Output;
        command::append_bounded(&mut self.output, &format!("\n[{}]\n", message));
        self.persist();
    }
    fn can_run(&mut self, verb: &str) -> bool {
        if self.settings.compiler.trim().is_empty() {
            self.show_settings = true;
            self.tool_error("Set a Hydrogen compiler executable in Settings".into());
            return false;
        }
        if self.settings.compiler_verbs.iter().any(|v| v == verb) {
            true
        } else {
            self.show_settings = true;
            self.tool_error(format!("Compiler profile does not support '{verb}'. Detect capabilities or choose a profile in Settings."));
            false
        }
    }
    fn remember_workspace(&mut self, path: PathBuf, project: bool) {
        let entries = self.settings.recent_workspaces.get_or_insert_with(Vec::new);
        entries.retain(|entry| entry.path != path || entry.project != project);
        entries.insert(0, RecentWorkspace { path, project });
        entries.truncate(20);
    }
    fn remove_recent(&mut self, entry: &RecentWorkspace) {
        if let Some(entries) = &mut self.settings.recent_workspaces {
            entries.retain(|e| e != entry);
        }
        self.persist();
    }
    fn add_folder(&mut self, path: PathBuf) {
        let path = path.canonicalize().unwrap_or(path);
        self.remember_workspace(path.clone(), false);
        if !self.settings.folders.contains(&path) {
            self.settings.folders.push(path);
            self.workspace_requested = true;
        }
        self.persist();
    }
    fn add_project(&mut self, path: PathBuf) {
        let path = path.canonicalize().unwrap_or(path);
        if !self.settings.projects.contains(&path) {
            self.settings.projects.push(path.clone());
        }
        self.settings.selected_project = Some(path.clone());
        if let Some(dir) = path.parent() {
            self.add_folder(dir.to_path_buf());
        }
        self.remember_workspace(path.clone(), true);
        self.persist();
        self.workspace_requested = true;
        self.open(path);
        self.request_diagnostic_scan();
    }
    fn open(&mut self, path: PathBuf) {
        if path.extension().and_then(|ext| ext.to_str()) == Some("quarkui") {
            self.open_designer(path);
            return;
        }
        self.designer_active = false;
        let path = path.canonicalize().unwrap_or(path);
        // Editing a manifest is a file operation. Workspace roots are added only
        // by an explicit Open Project or Select as build project action.
        if let Some(idx) = self
            .docs
            .iter()
            .position(|d| d.path.as_ref() == Some(&path))
        {
            self.active = Some(idx);
            self.focus_editor = true;
            return;
        }
        match fs::read_to_string(&path) {
            Ok(text) => {
                self.docs.push(Document {
                    id: self.next_doc_id,
                    path: Some(path),
                    saved: text.clone(),
                    text,
                });
                self.next_doc_id += 1;
                self.active = Some(self.docs.len() - 1);
                self.focus_editor = true;
                self.request_diagnostic_scan();
            }
            Err(e) => self.status = format!("Open failed: {e}"),
        }
    }
    fn save(&mut self, idx: usize) -> bool {
        self.save_document(idx, false, false)
    }
    fn save_document(&mut self, idx: usize, overwrite: bool, save_as: bool) -> bool {
        let Some(doc) = self.docs.get(idx) else {
            return false;
        };
        let existing = !save_as && doc.path.is_some();
        let path = if existing {
            doc.path.clone().unwrap()
        } else if let Some(path) = rfd::FileDialog::new()
            .set_file_name(
                doc.path
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .unwrap_or_default()
                    .to_string_lossy(),
            )
            .add_filter("Hydrogen", &["hy", "hyproj"])
            .save_file()
        {
            path
        } else {
            return false;
        };
        self.save_document_to(idx, path, overwrite, existing)
    }
    // The file chooser only selects a destination; all save decisions share this path.
    fn save_document_to(
        &mut self,
        idx: usize,
        path: PathBuf,
        overwrite: bool,
        existing: bool,
    ) -> bool {
        if idx >= self.docs.len() {
            return false;
        }
        let path = path.canonicalize().unwrap_or(path);
        if self
            .docs
            .iter()
            .enumerate()
            .any(|(i, d)| i != idx && d.path.as_ref() == Some(&path))
        {
            self.notice(
                "That file is already open in another tab. Switch to that tab to save it.".into(),
            );
            return false;
        }
        let result = if existing && !overwrite {
            storage::save_checked(&path, &self.docs[idx].text, &self.docs[idx].saved)
        } else {
            storage::atomic_write(&path, self.docs[idx].text.as_bytes())
                .map_err(storage::SaveError::Io)
        };
        match result {
            Ok(()) => {
                self.docs[idx].path = Some(path.clone());
                self.docs[idx].saved = self.docs[idx].text.clone();
                self.workspace_requested = true;
                self.status = "Saved".into();
                self.request_diagnostic_scan();
                self.save_session();
                true
            }
            Err(storage::SaveError::Conflict) => {
                self.pending_conflict = Some(idx);
                self.status = "File changed on disk: choose how to resolve the conflict".into();
                false
            }
            Err(storage::SaveError::Io(e)) => {
                self.notice(format!("Save failed: {e}"));
                false
            }
        }
    }
    fn save_all(&mut self) -> bool {
        if self.designer.as_ref().is_some_and(|d| d.dirty) && !self.save_designer() {
            return false;
        }
        for i in 0..self.docs.len() {
            if (self.docs[i].text != self.docs[i].saved || self.docs[i].path.is_none())
                && !self.save(i)
            {
                return false;
            }
        }
        true
    }
    fn selected_project(&self) -> Option<PathBuf> {
        self.settings
            .selected_project
            .clone()
            .filter(|p| self.settings.projects.contains(p))
            .or_else(|| self.settings.projects.first().cloned())
    }
    fn build(&mut self, scope: BuildScope) {
        if self.job_rx.is_some() {
            self.tool_error("Stop the current tool job before building".into());
            return;
        }
        if !self.can_run(if scope == BuildScope::Project {
            "build"
        } else {
            "compile"
        }) {
            return;
        }
        if !self.save_all() {
            return;
        }
        let active_path = self
            .active
            .and_then(|i| self.docs.get(i))
            .and_then(|d| d.path.as_deref());
        let project_path = self.selected_project();
        match hydrogen::build_plan(
            &self.settings.compiler,
            project_path.as_deref(),
            active_path,
            scope,
        ) {
            Ok(plan) => {
                // Always make output parents first; the compiler chooses the actual artifacts.
                let directory = if plan.output_is_directory {
                    plan.output.as_path()
                } else {
                    plan.output.parent().unwrap_or(Path::new("."))
                };
                if let Err(e) = fs::create_dir_all(directory) {
                    self.tool_error(format!("Cannot create build folder: {e}"));
                    return;
                }
                self.launch(plan.program, plan.args, plan.cwd);
                if self.job_rx.is_some() {
                    self.status = plan.description;
                }
            }
            Err(e) => self.tool_error(e),
        }
    }
    fn reload_plugins(&mut self) {
        if let Some(root) = plugins::directory(self.settings_path.as_deref()) {
            let (list, warnings) = plugins::load_all(&root);
            self.plugins = list;
            self.plugin_warnings = warnings;
        }
    }
    fn run_plugin(&mut self, id: &str, command_index: usize) {
        if self.job_rx.is_some() {
            self.tool_error("Stop the current tool job first".into());
            return;
        }
        if !self.extension_enabled(id) {
            self.status = "Enable the plugin in Plugins settings first".into();
            return;
        }
        let Some(plugin) = self.plugins.iter().find(|p| p.id == id).cloned() else {
            return;
        };
        let Some(command) = plugin.commands.get(command_index) else {
            return;
        };
        if !self.save_all() {
            return;
        }
        let cwd = self.cwd();
        let project = self.selected_project();
        let file = self
            .active
            .and_then(|i| self.docs.get(i))
            .and_then(|d| d.path.as_deref());
        match plugins::invoke(&plugin, command, project.as_deref(), file, &cwd) {
            Ok(invocation) => self.launch(invocation.program, invocation.args, invocation.cwd),
            Err(err) => self.tool_error(err),
        }
    }
    fn current_path(&self) -> Option<PathBuf> {
        self.active
            .and_then(|idx| self.docs.get(idx))
            .and_then(|d| d.path.clone())
            .or_else(|| self.settings.projects.first().cloned())
    }
    fn cwd(&self) -> PathBuf {
        self.settings
            .command_folder
            .clone()
            .or_else(|| {
                self.selected_project()
                    .and_then(|p| p.parent().map(Path::to_path_buf))
            })
            .or_else(|| self.settings.folders.first().cloned())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    }
    fn launch(&mut self, program: String, args: Vec<String>, cwd: PathBuf) {
        if self.job_rx.is_some() {
            self.tool_error("A tool job is already running".into());
            return;
        }
        self.settings.terminal_visible = true;
        self.settings.bottom_tab = BottomTab::Output;
        self.problem_cwd = Some(cwd.clone());
        self.problem_output.clear();
        self.problems.clear();
        self.persist();
        self.status = "Running command…".into();
        command::append_bounded(
            &mut self.output,
            &format!(
                "\n[{}]\n$ {} {}\n",
                cwd.display(),
                program,
                args.iter()
                    .map(|a| format!("{a:?}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
        );
        let (tx, rx) = mpsc::sync_channel(128);
        self.job_rx = Some(rx);
        self.job_cwd = Some(cwd.clone());
        let cancel = Arc::new(AtomicBool::new(false));
        self.job_cancel = Some(cancel.clone());
        let timeout = Duration::from_secs(self.settings.command_timeout_seconds);
        self.job_worker = Some(thread::spawn(move || {
            command::run_streamed(&program, &args, &cwd, tx, cancel, timeout)
        }));
    }
    fn hydrogen(&mut self, verb: &str) {
        if self.job_rx.is_some() {
            self.tool_error("Stop the current tool job first".into());
            return;
        }
        if !self.can_run(verb) {
            return;
        }
        let Some(path) = (if verb == "check" || verb == "run" {
            self.selected_project()
        } else {
            None
        })
        .or_else(|| self.current_path()) else {
            self.tool_error("Open a .hy or .hyproj first".into());
            return;
        };
        if !matches!(
            path.extension().and_then(|s| s.to_str()),
            Some("hy" | "hyproj")
        ) {
            self.tool_error("Open a Hydrogen file or project".into());
            return;
        }
        if !self.save_all() {
            return;
        }
        let compiler = self.settings.compiler.clone();
        let mut args = vec![verb.to_string(), path.to_string_lossy().into_owned()];
        if verb == "compile" {
            let output = path
                .parent()
                .unwrap_or(Path::new("."))
                .join(".quark/build")
                .join(path.file_stem().unwrap_or_default());
            let _ = fs::create_dir_all(output.parent().unwrap_or(Path::new(".")));
            args.extend(["-o".into(), output.to_string_lossy().into_owned()]);
        }
        let cwd = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        self.launch(compiler, args, cwd);
    }
    fn git(&mut self, args: &[&str]) {
        self.launch(
            "git".into(),
            args.iter().map(|s| (*s).into()).collect(),
            self.cwd(),
        );
    }
    fn editor_id(&self) -> Option<egui::Id> {
        self.active
            .and_then(|i| self.docs.get(i))
            .map(|doc| egui::Id::new(("quark-document-editor", doc.id)))
    }
    fn caret(&self, ctx: &egui::Context) -> usize {
        self.editor_id()
            .and_then(|id| TextEdit::load_state(ctx, id))
            .and_then(|state| state.cursor.char_range())
            .map_or(0, |range| range.primary.index)
    }
    fn set_selection(&self, ctx: &egui::Context, start: usize, end: usize) {
        if let Some(id) = self.editor_id() {
            if let Some(mut state) = TextEdit::load_state(ctx, id) {
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::two(
                        egui::text::CCursor::new(end),
                        egui::text::CCursor::new(start),
                    )));
                state.store(ctx, id);
                ctx.memory_mut(|m| m.request_focus(id));
            }
        }
    }
    fn next_match(&mut self, ctx: &egui::Context, backwards: bool) {
        let cursor = self
            .editor_id()
            .and_then(|id| TextEdit::load_state(ctx, id))
            .and_then(|state| state.cursor.char_range())
            .map_or(0, |range| {
                if backwards {
                    range.primary.index.min(range.secondary.index)
                } else {
                    range.primary.index.max(range.secondary.index)
                }
            });
        let target = self
            .active
            .and_then(|i| self.docs.get(i))
            .and_then(|doc| navigation::find(&doc.text, &self.search, cursor, backwards));
        if let Some((start, end)) = target {
            self.set_selection(ctx, start, end);
            self.status = "Match selected".into();
        } else {
            self.status = "No matching text".into();
        }
    }
    fn replace_next(&mut self, ctx: &egui::Context) {
        if self.search.is_empty() {
            return;
        }
        let cursor = self.caret(ctx);
        let selected = self
            .editor_id()
            .and_then(|id| TextEdit::load_state(ctx, id))
            .and_then(|state| state.cursor.char_range())
            .map(|range| {
                (
                    range.primary.index.min(range.secondary.index),
                    range.primary.index.max(range.secondary.index),
                )
            });
        let target = self.active.and_then(|i| self.docs.get(i)).and_then(|doc| {
            let current = selected.filter(|(start, end)| {
                let selected_text: String =
                    doc.text.chars().skip(*start).take(end - start).collect();
                selected_text == self.search
            });
            current.or_else(|| navigation::find(&doc.text, &self.search, cursor, false))
        });
        if let Some((start, end)) = target {
            if let Some(doc) = self.active.and_then(|i| self.docs.get_mut(i)) {
                let updated =
                    navigation::replace_at(&mut doc.text, (start, end), &self.replace_with);
                self.set_selection(ctx, updated, updated);
                self.status = "Replaced occurrence".into();
                self.request_diagnostic_scan();
            }
        } else {
            self.status = "No matching text".into();
        }
    }
    fn replace_all(&mut self, ctx: &egui::Context) {
        if self.search.is_empty() {
            return;
        }
        if let Some(doc) = self.active.and_then(|i| self.docs.get_mut(i)) {
            let count = doc.text.matches(&self.search).count();
            if count > 0 {
                doc.text = doc.text.replace(&self.search, &self.replace_with);
            }
            self.status = format!("Replaced {count} occurrence(s)");
            self.set_selection(ctx, 0, 0);
            if count > 0 {
                self.request_diagnostic_scan();
            }
        }
    }
    fn go_to_line(&mut self, ctx: &egui::Context) {
        let Ok(line) = self.go_line_input.trim().parse::<usize>() else {
            self.status = "Enter a numeric line".into();
            return;
        };
        let target = self
            .active
            .and_then(|i| self.docs.get(i))
            .and_then(|doc| navigation::line_start(&doc.text, line));
        if let Some(index) = target {
            self.set_selection(ctx, index, index);
            self.show_go_to_line = false;
            self.status = format!("Line {line}");
        } else {
            self.status = "Line outside file".into();
        }
    }
    fn duplicate_active_line(&mut self, ctx: &egui::Context) {
        if self.editor_id().is_none() {
            return;
        }
        let cursor = self.caret(ctx);
        if let Some(doc) = self.active.and_then(|i| self.docs.get_mut(i)) {
            let index = navigation::duplicate_line(&mut doc.text, cursor);
            self.set_selection(ctx, index, index);
            self.completion = None;
            self.status = "Line duplicated".into();
            self.request_diagnostic_scan();
        }
    }
    fn next_tab(&mut self, backwards: bool) {
        if self.docs.is_empty() {
            return;
        }
        let i = self.active.unwrap_or(0);
        self.active = Some(if backwards {
            (i + self.docs.len() - 1) % self.docs.len()
        } else {
            (i + 1) % self.docs.len()
        });
        self.completion = None;
        self.focus_editor = true;
    }
    fn shortcuts(&mut self, ctx: &egui::Context) {
        let cmd = Modifiers::COMMAND;
        if take_key(ctx, cmd, Key::B) {
            self.settings.sidebar_visible = !self.settings.sidebar_visible;
            self.persist();
        }
        if take_key(ctx, Modifiers::NONE, Key::F1) || take_key(ctx, cmd | Modifiers::SHIFT, Key::P)
        {
            self.show_palette = true;
            self.palette_focus = true;
            self.palette_query.clear();
            self.palette_selected = 0;
        }
        if take_key(ctx, Modifiers::CTRL, Key::Backtick) {
            self.settings.terminal_visible = !self.settings.terminal_visible;
            self.persist();
        }
        if take_key(ctx, cmd | Modifiers::SHIFT, Key::E) {
            self.settings.sidebar_visible = true;
            self.settings.sidebar_view = SidebarView::Explorer;
            self.persist();
        }
        if take_key(ctx, cmd, Key::N) {
            self.new_document();
        }
        if take_key(ctx, cmd, Key::O) {
            if let Some(p) = rfd::FileDialog::new().pick_file() {
                self.open(p);
            }
        }
        if take_key(ctx, cmd | Modifiers::SHIFT, Key::S) {
            if let Some(i) = self.active {
                self.save_document(i, false, true);
            }
        }
        if take_key(ctx, cmd | Modifiers::ALT, Key::S) {
            self.save_all();
        }
        if take_key(ctx, cmd, Key::S) {
            self.save_active();
        }
        if take_key(ctx, cmd | Modifiers::SHIFT, Key::F) {
            self.show_sidebar(SidebarView::Search);
            self.workspace_search = true;
        }
        if take_key(ctx, cmd, Key::F) {
            self.show_sidebar(SidebarView::Search);
            self.show_search = true;
            self.find_focus = true;
        }
        if take_key(ctx, cmd, Key::H) {
            self.show_sidebar(SidebarView::Search);
            self.show_search = true;
            self.show_replace = true;
            self.find_focus = true;
        }
        if take_key(ctx, cmd, Key::G) {
            self.show_go_to_line = true;
            self.go_focus = true;
        }
        if take_key(ctx, Modifiers::SHIFT, Key::F3) {
            self.next_match(ctx, true);
        }
        if take_key(ctx, Modifiers::NONE, Key::F3) {
            self.next_match(ctx, false);
        }
        if take_key(ctx, cmd, Key::P) {
            self.show_quick_open = true;
            self.quick_focus = true;
            self.quick_selected = 0;
        }
        if take_key(ctx, cmd, Key::Space) {
            self.manual_completion = true;
        }
        if take_key(ctx, Modifiers::ALT, Key::Z) {
            self.settings.word_wrap = !self.settings.word_wrap;
            self.persist();
        }
        if take_key(ctx, cmd, Key::W) {
            if let Some(i) = self.active {
                self.close(i);
            }
        }
        if take_key(ctx, cmd | Modifiers::SHIFT, Key::Tab) {
            self.next_tab(true);
        }
        if take_key(ctx, cmd, Key::Tab) {
            self.next_tab(false);
        }
        if take_key(ctx, cmd | Modifiers::SHIFT, Key::D)
            && self
                .editor_id()
                .is_some_and(|id| ctx.memory(|m| m.has_focus(id)))
        {
            self.duplicate_active_line(ctx);
        }
        if take_key(ctx, Modifiers::CTRL | Modifiers::SHIFT, Key::B) {
            self.build(BuildScope::Project);
        }
        if take_key(ctx, Modifiers::NONE, Key::F5) {
            self.hydrogen("run");
        }
        if self.completion.is_none()
            && !self.show_quick_open
            && !self.show_settings
            && !self.show_clone
            && take_key(ctx, Modifiers::NONE, Key::Escape)
        {
            self.show_search = false;
            self.show_go_to_line = false;
        }
    }
    fn close(&mut self, idx: usize) {
        if idx >= self.docs.len() {
            return;
        }
        if self.docs[idx].text != self.docs[idx].saved {
            self.pending_close = Some(idx);
            return;
        }
        self.docs.remove(idx);
        self.request_diagnostic_scan();
        self.completion = None;
        self.active = match self.active {
            None => None,
            Some(_) if self.docs.is_empty() => None,
            Some(active) if active > idx => Some(active - 1),
            Some(active) if active == idx => Some(idx.min(self.docs.len() - 1)),
            Some(active) => Some(active),
        };
    }
    fn new_document(&mut self) {
        self.designer_active = false;
        self.docs.push(Document {
            id: self.next_doc_id,
            path: None,
            text: String::new(),
            saved: String::new(),
        });
        self.next_doc_id += 1;
        self.active = Some(self.docs.len() - 1);
        self.focus_editor = true;
        self.request_diagnostic_scan();
        self.completion = None;
    }
    fn draw_symbol_node(&mut self, ui: &mut egui::Ui, node: &symbols::SymbolNode, salt: &str) {
        for (name, child) in &node.branches {
            egui::CollapsingHeader::new(
                RichText::new(name).color(theme::colors(ui.ctx()).muted.color()),
            )
            .id_salt((salt, name, self.tree_revision))
            .default_open(self.tree_default_open)
            .show(ui, |ui| {
                self.draw_symbol_node(ui, child, &format!("{salt}/{name}"));
            });
        }
        for symbol in &node.items {
            let label = if matches!(symbol.kind, "file" | "missing") {
                symbol.name.clone()
            } else {
                format!(
                    "{} ({})",
                    symbol.name,
                    symbol
                        .path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                )
            };
            if ui
                .selectable_label(false, label)
                .on_hover_text(format!("{} · {}", symbol.kind, symbol.path.display()))
                .clicked()
            {
                self.open(symbol.path.clone());
            }
        }
    }
    fn project_ui(&mut self, ui: &mut egui::Ui, project: &project::Project) {
        egui::CollapsingHeader::new(
            RichText::new(format!("{} [{}]", project.name, project.kind))
                .color(theme::colors(ui.ctx()).accent.color()),
        )
        .id_salt((&project.path, self.tree_revision))
        .default_open(self.tree_default_open)
        .show(ui, |ui| {
            if ui.small_button("Open manifest").clicked() {
                self.open(project.path.clone());
            }
            for warning in &project.warnings {
                ui.colored_label(
                    theme::colors(ui.ctx()).warning.color(),
                    format!("Warning: {warning}"),
                );
            }
            let outline = self
                .workspace
                .outlines
                .get(&project.path)
                .cloned()
                .unwrap_or_default();
            self.draw_symbol_node(ui, &outline, &project.path.to_string_lossy());
            for reference in &project.references {
                self.project_ui(ui, reference);
            }
        })
        .header_response
        .on_hover_text(project.path.display().to_string())
        .context_menu(|ui| {
            if ui.button("Select as build project").clicked() {
                self.settings.selected_project = Some(project.path.clone());
                if !self.settings.projects.contains(&project.path) {
                    self.settings.projects.push(project.path.clone());
                }
                self.persist();
                self.request_diagnostic_scan();
                ui.close_menu();
            }
            if ui.button("Remove project from workspace").clicked() {
                self.settings.projects.retain(|p| p != &project.path);
                if self.settings.selected_project.as_ref() == Some(&project.path) {
                    self.settings.selected_project = None;
                }
                self.workspace_requested = true;
                self.request_diagnostic_scan();
                self.persist();
                ui.close_menu();
            }
        });
    }
    fn accept_completion(&mut self, candidate: &str, ctx: &egui::Context) {
        if let Some(popup) = self.completion.take() {
            if let Some(doc) = self.docs.iter_mut().find(|d| d.id == popup.doc_id) {
                let new_index =
                    insert_completion(&mut doc.text, popup.cursor_char, &popup.prefix, candidate);
                if let Some(mut state) = egui::TextEdit::load_state(ctx, popup.edit_id) {
                    state
                        .cursor
                        .set_char_range(Some(egui::text::CCursorRange::one(
                            egui::text::CCursor::new(new_index),
                        )));
                    state.store(ctx, popup.edit_id);
                }
                ctx.memory_mut(|m| m.request_focus(popup.edit_id));
                self.dismissed_completion = Some((doc.id, new_index, candidate.to_owned()));
                self.status = "Modified".into();
                self.request_diagnostic_scan();
            }
        }
    }
    fn completion_keys(&mut self, ctx: &egui::Context) {
        let mut accept = None;
        if let Some(popup) = self.completion.as_mut() {
            if !ctx.memory(|m| m.has_focus(popup.edit_id)) {
                return;
            }
            if take_key(ctx, Modifiers::NONE, Key::Escape) {
                self.dismissed_completion =
                    Some((popup.doc_id, popup.cursor_char, popup.prefix.clone()));
                self.completion = None;
                return;
            }
            if take_key(ctx, Modifiers::NONE, Key::ArrowDown) {
                popup.selected = (popup.selected + 1) % popup.candidates.len();
            }
            if take_key(ctx, Modifiers::NONE, Key::ArrowUp) {
                popup.selected =
                    (popup.selected + popup.candidates.len() - 1) % popup.candidates.len();
            }
            if take_key(ctx, Modifiers::NONE, Key::Enter)
                || take_key(ctx, Modifiers::NONE, Key::Tab)
            {
                accept = popup.candidates.get(popup.selected).cloned();
            }
        }
        if let Some(candidate) = accept {
            self.accept_completion(&candidate, ctx);
        }
    }
    fn draw_completion(&mut self, ctx: &egui::Context) {
        let Some(popup) = self.completion.as_ref().cloned() else {
            return;
        };
        let mut accept = None;
        egui::Area::new(egui::Id::new("completion-menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(popup.anchor)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(250.0);
                    ui.label(
                        RichText::new("HYDROGEN · SUGGESTIONS")
                            .small()
                            .color(theme::colors(ui.ctx()).muted.color()),
                    );
                    for (index, candidate) in popup.candidates.iter().enumerate() {
                        if ui
                            .selectable_label(popup.selected == index, candidate)
                            .clicked()
                        {
                            accept = Some(candidate.clone());
                        }
                    }
                    ui.label(
                        RichText::new("Up / Down select · Enter / Tab insert · Esc dismiss")
                            .small()
                            .color(theme::colors(ui.ctx()).muted.color()),
                    );
                });
            });
        if let Some(candidate) = accept {
            self.accept_completion(&candidate, ctx);
        }
    }
    fn tree_ui(&mut self, ui: &mut egui::Ui, path: &Path, depth: usize) {
        if depth > 64 {
            ui.label("Folder depth limit reached");
            return;
        }
        let Some(entries) = self.workspace.directories.get(path).cloned() else {
            ui.label(
                RichText::new(if self.workspace_rx.is_some() {
                    "Loading…"
                } else {
                    "Folder unavailable or skipped; see index notices below"
                })
                .small()
                .color(theme::colors(ui.ctx()).muted.color()),
            );
            return;
        };
        let limit = *self.tree_limits.get(path).unwrap_or(&100);
        for entry in entries.iter().take(limit) {
            let name = entry.path.file_name().unwrap_or_default().to_string_lossy();
            if entry.directory {
                egui::CollapsingHeader::new(format!("{name}"))
                    .id_salt((&entry.path, self.tree_revision))
                    .default_open(self.tree_default_open)
                    .show(ui, |ui| self.tree_ui(ui, &entry.path, depth + 1));
            } else {
                let selected = self
                    .active
                    .and_then(|i| self.docs.get(i))
                    .is_some_and(|d| d.path.as_ref() == Some(&entry.path));
                if ui
                    .selectable_label(selected, name)
                    .on_hover_text(entry.path.display().to_string())
                    .clicked()
                {
                    self.open(entry.path.clone());
                }
            }
        }
        if entries.len() > limit
            && ui
                .button(format!("Show more ({} remaining)", entries.len() - limit))
                .clicked()
        {
            self.tree_limits.insert(path.to_path_buf(), limit + 100);
        }
    }
}
/// egui's consume_shortcut ignores extra Shift/Alt. Editor commands need
/// exact modifiers so Save cannot swallow Save As or Build can toggle Explorer.
fn take_key(ctx: &egui::Context, modifiers: Modifiers, key: Key) -> bool {
    ctx.input_mut(|input|{
        let mut found=false;
        input.events.retain(|event|{
            let matches=matches!(event,egui::Event::Key{key:event_key,pressed:true,modifiers:event_modifiers,..} if *event_key==key && event_modifiers.matches_exact(modifiers));
            found|=matches;!matches
        });
        found
    })
}
fn diagnostic_path(doc: &Document) -> PathBuf {
    doc.path
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!("quark-untitled-{}.hy", doc.id)))
}
fn session_file() -> Option<PathBuf> {
    settings_file().map(|p| p.with_file_name("session.json"))
}
fn load_json<T: serde::de::DeserializeOwned + Default>(path: Option<&Path>) -> (T, Option<String>) {
    let Some(path) = path else {
        return (T::default(), None);
    };
    match fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str(&text) {
            Ok(value) => (value,None),
            Err(e) => (T::default(),Some(format!("Cannot read {}: {e}. Original preserved; persistence paused. Repair it or back it up and reset from Settings.",path.display()))),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (T::default(),None),
        Err(e) => (T::default(),Some(format!("Cannot read {}: {e}. Persistence paused; repair access and restart.",path.display()))),
    }
}

fn settings_file() -> Option<PathBuf> {
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let base =
        std::env::var_os("HOME").map(|p| PathBuf::from(p).join("Library/Application Support"));
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")));
    base.map(|p| p.join("QuarkIDE/settings.json"))
}
impl eframe::App for Quark {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.theme_persist_due.take().is_some() {
            self.persist();
        }
        if let Some(cancel) = &self.job_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        self.terminal.take();
        self.job_rx = None;
        if let Some(worker) = self.job_worker.take() {
            let _ = worker.join();
        }
        self.save_session();
    }
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.applied_window_chrome != Some(self.settings.integrated_title_bar) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(
                !self.settings.integrated_title_bar,
            ));
            self.applied_window_chrome = Some(self.settings.integrated_title_bar);
        }
        let effective_theme = self.effective_theme();
        if self.applied_theme.as_ref() != Some(&effective_theme) {
            theme::apply(ctx, &effective_theme);
            self.applied_theme = Some(effective_theme);
        }
        if let Some(terminal) = &mut self.terminal {
            terminal.poll();
        }
        if self
            .theme_persist_due
            .is_some_and(|due| ctx.input(|i| i.time) >= due)
        {
            self.theme_persist_due = None;
            self.persist();
        }
        self.poll_search();
        let mut finished = None;
        if let Some(rx) = &self.job_rx {
            // Keep a noisy process from monopolising a frame.
            for _ in 0..64 {
                match rx.try_recv() {
                    Ok(command::Event::Output(text)) => {
                        if self.probe_running {
                            command::append_bounded(&mut self.probe_output, &text);
                        }
                        command::append_bounded(&mut self.output, &text);
                        command::append_bounded(&mut self.problem_output, &text);
                        if let Some(cwd) = self.problem_cwd.as_deref() {
                            self.problems = problems::parse(&self.problem_output, cwd);
                        }
                    }
                    Ok(command::Event::Finished(result)) => {
                        finished = Some(result);
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        finished = Some(Err("Command worker disconnected".into()));
                        break;
                    }
                }
            }
        }
        if let Some(result) = finished {
            let succeeded = matches!(result, Ok(0));
            self.status = match &result {
                Ok(0) => "Command finished successfully".into(),
                Ok(code) => format!("Command failed (exit code {code})"),
                Err(e) => e.clone(),
            };
            command::append_bounded(
                &mut self.output,
                &format!(
                    "\n[{}]\n",
                    match result {
                        Ok(code) => format!("Exit code: {code}"),
                        Err(e) => e,
                    }
                ),
            );
            self.job_rx = None;
            if let Some(worker) = self.job_worker.take() {
                let _ = worker.join();
            }
            self.job_cancel = None;
            self.job_cwd = None;
            if let Some(folder) = self.pending_clone.take() {
                if succeeded {
                    self.add_folder(folder);
                }
            }
            if self.probe_running {
                self.probe_running = false;
                let verbs = hydrogen::detect_verbs(&self.probe_output);
                if verbs.is_empty() {
                    self.notice("No supported commands found in compiler help. Choose a profile or set capabilities manually.".into());
                } else {
                    self.settings.compiler_verbs = verbs;
                    self.persist();
                    self.status = "Compiler capabilities detected; review them in Settings".into();
                }
            }
            self.workspace_requested = true;
            self.request_diagnostic_scan();
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_exit {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.request_exit(ctx);
        }
        let now = ctx.input(|i| i.time);
        if now >= self.session_due {
            self.save_session();
            self.session_due = now + 2.0;
        }
        self.workspace_tick();
        if !self.exit_requested
            && self.pending_close.is_none()
            && self.pending_conflict.is_none()
            && !self.show_palette
        {
            if self.terminal.as_ref().is_some_and(|t| t.focused(ctx))
                && self.settings.terminal_visible
                && self.settings.bottom_tab == BottomTab::Terminal
            {
                if take_key(ctx, Modifiers::COMMAND | Modifiers::SHIFT, Key::P) {
                    self.show_palette = true;
                    self.palette_focus = true;
                }
                if take_key(ctx, Modifiers::CTRL | Modifiers::SHIFT, Key::B) {
                    self.build(BuildScope::Project);
                }
                if take_key(ctx, Modifiers::COMMAND | Modifiers::SHIFT, Key::E) {
                    self.show_sidebar(SidebarView::Explorer);
                }
                if take_key(ctx, Modifiers::COMMAND | Modifiers::SHIFT, Key::F) {
                    self.show_sidebar(SidebarView::Search);
                    self.workspace_search = true;
                }
                if take_key(ctx, Modifiers::CTRL, Key::Backtick) {
                    self.settings.terminal_visible = false;
                    self.persist();
                }
            } else {
                self.shortcuts(ctx);
                self.completion_keys(ctx);
            }
        }
        self.diagnostic_tick(ctx);
        let top=egui::TopBottomPanel::top("menu").show(ctx,|ui|{
            egui::menu::bar(ui,|ui|{
                ui.spacing_mut().item_spacing.x = 4.0;
                window::caption(ui,self.settings.integrated_title_bar);
                ui.separator();
                ui.menu_button("File",|ui|{
                    if ui.button("New Hydrogen project…").clicked(){self.start_project_dialog();ui.close_menu();}
                    if ui.button("New UI layout…").clicked(){self.show_new_layout=true;ui.close_menu();}
                    if ui.button("New file  Ctrl+N").clicked(){self.new_document();ui.close_menu();}
                    if ui.button("Open file  Ctrl+O").clicked(){if let Some(p)=rfd::FileDialog::new().pick_file(){self.open(p);}ui.close_menu();}
                    if ui.button("Open Folder…").clicked(){if let Some(p)=rfd::FileDialog::new().pick_folder(){self.add_folder(p);}ui.close_menu();}
                    if ui.button("Open .hyproj").clicked(){if let Some(p)=rfd::FileDialog::new().add_filter("Hydrogen project", &["hyproj"]).pick_file(){self.add_project(p);}ui.close_menu();}
                    if ui.button("Save  Ctrl+S").clicked(){self.save_active();ui.close_menu();}
                    if ui.button("Save As…  Ctrl+Shift+S").clicked(){if let Some(i)=self.active{self.save_document(i,false,true);}ui.close_menu();}
                    if ui.button("Save all  Ctrl+Alt+S").clicked(){self.save_all();ui.close_menu();}
                    if ui.button("Close editor  Ctrl+W").clicked(){if self.designer_active{self.close_designer();}else if let Some(i)=self.active{self.close(i);}ui.close_menu();}
                });
                ui.menu_button("Edit",|ui|{
                    if ui.button("Completion  Ctrl/Cmd+Space").clicked(){self.manual_completion=true;ui.close_menu();}
                    if ui.button("Find  Ctrl+F").clicked(){self.show_sidebar(SidebarView::Search);self.show_search=true;self.find_focus=true;ui.close_menu();}
                    if ui.button("Quick open  Ctrl+P").clicked(){self.show_quick_open=true;self.quick_focus=true;self.quick_selected=0;ui.close_menu();}
                    if ui.button("Find & replace · Ctrl+H").clicked(){self.show_sidebar(SidebarView::Search);self.show_search=true;self.show_replace=true;self.find_focus=true;ui.close_menu();}
                    if ui.button("Go to line · Ctrl+G").clicked(){self.show_go_to_line=true;self.go_focus=true;ui.close_menu();}
                    if ui.button("Duplicate line · Ctrl+Shift+D").clicked(){self.duplicate_active_line(ctx);ui.close_menu();}
                    if ui.button("Next tab · Ctrl+Tab").clicked(){self.next_tab(false);ui.close_menu();}
                    if ui.button("Command Palette…  F1").clicked(){self.show_palette=true;self.palette_focus=true;ui.close_menu();}
                });
                ui.menu_button("Project",|ui|{
                    if ui.button("Create new project…").clicked(){self.start_project_dialog();ui.close_menu();}
                    if ui.button("New UI layout…").clicked(){self.show_new_layout=true;ui.close_menu();}
                    if ui.button("Open Hydrogen project…").clicked(){if let Some(p)=rfd::FileDialog::new().add_filter("Hydrogen project",&["hyproj"]).pick_file(){self.add_project(p);}ui.close_menu();}
                    if ui.button("Solution Explorer").clicked(){self.show_sidebar(SidebarView::Explorer);ui.close_menu();}
                });
                ui.menu_button("Build",|ui|{
                    if ui.button("Build selected project  Ctrl+Shift+B").clicked(){self.build(BuildScope::Project);ui.close_menu();}
                    if ui.button("Compile current file").clicked(){self.build(BuildScope::CurrentFile);ui.close_menu();}
                    if ui.button("Check project").clicked(){self.hydrogen("check");ui.close_menu();}
                });
                ui.menu_button("Debug",|ui|{
                    if ui.button("Start without debugging  Ctrl+F5").clicked(){self.hydrogen("run");ui.close_menu();}
                    ui.add_enabled(false,egui::Button::new("Start debugging (requires Hydrogen debug symbols)"));
                });
                ui.menu_button("Hydrogen",|ui|{
                    if ui.button("Build active project  Ctrl+Shift+B").clicked(){self.build(BuildScope::Project);ui.close_menu();}
                    if ui.button("Compile current .hy file").clicked(){self.build(BuildScope::CurrentFile);ui.close_menu();}
                    ui.separator();
                    if ui.button("Check project").clicked(){self.hydrogen("check");ui.close_menu();}
                    if ui.button("Run (SDK)").clicked(){self.hydrogen("run");ui.close_menu();}
                    if ui.button("Settings…").clicked(){self.show_settings=true;ui.close_menu();}
                });
                ui.menu_button("Extensions",|ui|{
                    if ui.button("Manage extensions…").clicked(){self.show_sidebar(SidebarView::Plugins);ui.close_menu();}
                    if ui.button("Reload manifests").clicked(){self.reload_plugins();ui.close_menu();}
                    ui.separator();
                    let available:Vec<_>=self.plugins.iter().filter(|p|self.extension_enabled(&p.id)).cloned().collect();
                    if available.is_empty(){ui.label("No enabled extensions installed");}
                    for plugin in available {
                        ui.menu_button(plugin.name.clone(), |ui| {
                            for (index, command) in plugin.commands.iter().enumerate() {
                                if ui.button(&command.title).clicked(){self.run_plugin(&plugin.id,index);ui.close_menu();}
                            }
                        });
                    }
                });
                ui.menu_button("Git",|ui|{
                    for (name,args) in [("Status",vec!["status","--short","--branch"]),("Diff",vec!["diff"]),("Fetch",vec!["fetch","--all"]),("Pull",vec!["pull"]),("Push",vec!["push"]) ]{
                        if ui.button(name).clicked(){self.git(&args);ui.close_menu();}
                    }
                    if ui.button("Clone repository").clicked(){self.show_clone=true;ui.close_menu();}
                });
                ui.menu_button("View",|ui|{
                    let mut changed=false;
                    changed |= ui.checkbox(&mut self.settings.sidebar_visible,"Sidebar  Ctrl+B").changed();
                    changed |= ui.checkbox(&mut self.settings.terminal_visible,"Bottom panel  Ctrl+`").changed();
                    changed |= ui.checkbox(&mut self.settings.word_wrap,"Word wrap  Alt+Z").changed();
                    if ui.button("Themes…").clicked(){self.show_sidebar(SidebarView::Themes);ui.close_menu();}
                    if changed{self.persist();}
                });
                ui.menu_button("Help",|ui|{
                    ui.label(format!("Quark IDE {}",env!("CARGO_PKG_VERSION")));
                    if ui.button("Start Window").clicked(){self.show_welcome();ui.close_menu();}
                    if ui.button("Command Palette…  F1").clicked(){self.show_palette=true;self.palette_focus=true;ui.close_menu();}
                    if ui.button("Settings").clicked(){self.show_settings=true;ui.close_menu();}
                });
                ui.separator();
                let build_button=ui.small_button(RichText::new("Build").color(theme::colors(ui.ctx()).accent.color()))
                    .on_hover_text("Build selected .hyproj; hold Shift when clicking to compile the active .hy file");
                if build_button.clicked() {
                    let scope=if ui.input(|i|i.modifiers.shift){BuildScope::CurrentFile}else{BuildScope::Project};
                    self.build(scope);
                }
                if ui.small_button("▶ Run").on_hover_text("Start selected Hydrogen project without debugger").clicked(){self.hydrogen("run");}
                if ui.small_button("Check").on_hover_text("Check Hydrogen project").clicked(){self.hydrogen("check");}
                if ui.small_button("Find").on_hover_text("Find in file · Ctrl+F").clicked(){self.show_sidebar(SidebarView::Search);self.show_search=true;self.find_focus=true;}
                let reserved=if self.settings.integrated_title_bar {175.0} else {76.0};
                let command_width=(ui.available_width()-reserved).max(40.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                    if self.settings.integrated_title_bar {
                        if window::button(ui,window::Action::Close).clicked(){self.request_exit(ctx);}
                        if window::button(ui,window::Action::Maximise).clicked(){window::toggle_maximise(ctx);}
                        if window::button(ui,window::Action::Minimise).clicked(){ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));}
                    }
                    if ui::layout_button(ui,false,self.settings.terminal_visible).clicked(){self.settings.terminal_visible= !self.settings.terminal_visible;self.persist();}
                    if ui::layout_button(ui,true,self.settings.sidebar_visible).clicked(){self.settings.sidebar_visible= !self.settings.sidebar_visible;self.persist();}
                    if ui.add_sized([command_width,24.0],egui::Button::new(if command_width>230.0 {"Search files / commands…  F1"} else {"Search… F1"}).truncate().frame(true).fill(theme::colors(ui.ctx()).surface.color()).stroke(egui::Stroke::new(1.0_f32,theme::colors(ui.ctx()).border.color()))).on_hover_text("Command Palette · F1 / Ctrl+Shift+P; Go to File · Ctrl+P").clicked(){self.show_palette=true;self.palette_focus=true;}
                });
            });
        });
        self.top_rect = top.response.rect;
        egui::TopBottomPanel::bottom("status").frame(egui::Frame::new().fill(theme::colors(ctx).status.color()).inner_margin(egui::Margin::symmetric(10,3))).show(ctx,|ui|{ui.horizontal(|ui|{

            ui.label(RichText::new(&self.status).small().color(theme::colors(ui.ctx()).status_text.color()));
            let (errors, hints) = self.diagnostics.values().flatten().fold((0_usize,0_usize), |(e,h),d| {
                match d.severity {
                    diagnostics::Severity::Error => (e+1,h),
                    diagnostics::Severity::Hint => (e,h+1),
                    diagnostics::Severity::Warning => (e+1,h),
                }
            });
            {
                ui.separator();
                ui.label(RichText::new(format!("Quick checks: {errors} problems · {hints} hints{}",if self.diagnostic_rx.is_some(){" · scanning"}else if !self.analysis_complete{" · partial"}else{""})).small().color(theme::colors(ui.ctx()).status_text.color()))
                    .on_hover_text(format!("Quick lexical checks and possible-unused hints only; compiler diagnostics are in Output. {} scanned files{}",
                        self.files_scanned, if self.analysis_complete {""} else {" (project analysis incomplete or no project selected)"}));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                if let Some((line,col))=self.caret_position {
                    ui.label(RichText::new(format!("Ln {line}, Col {col}")).small().color(theme::colors(ui.ctx()).status_text.color()));
                }
                ui.label(RichText::new("UTF-8").small().color(theme::colors(ui.ctx()).status_text.color()));
            });
        });});
        self.sidebar_rect = egui::Rect::NOTHING;
        if self.settings.sidebar_visible {
            let sidebar = egui::SidePanel::left("left-sidebar")
                .resizable(true)
                .default_width(290.0)
                .min_width(240.0)
                .show(ctx, |ui| self.sidebar_ui(ui));
            self.sidebar_rect = sidebar.response.rect;
        }
        self.bottom_rect = egui::Rect::NOTHING;
        if self.settings.terminal_visible {
            let bottom = egui::TopBottomPanel::bottom("bottom-panel")
                .resizable(true)
                .min_height(150.0)
                .default_height(240.0)
                .show(ctx, |ui| self.bottom_ui(ui));
            self.bottom_rect = bottom.response.rect;
        }
        if !self.notices.is_empty() {
            egui::TopBottomPanel::top("notifications").show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(theme::colors(ui.ctx()).warning.color(), &self.notices[0]);
                    if ui.small_button("Settings").clicked() {
                        self.show_settings = true;
                    }
                    if ui.small_button("Dismiss").clicked() {
                        self.notices.remove(0);
                    }
                });
            });
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::colors(ctx).editor.color())
                    .inner_margin(8),
            )
            .show(ctx, |ui| {
                self.caret_position = None;
                egui::ScrollArea::horizontal()
                    .id_salt("editor-tabs")
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 3.0;
                            let mut close = None;
                            if ui
                                .selectable_label(
                                    self.active.is_none() && !self.designer_active,
                                    "Start Window",
                                )
                                .clicked()
                            {
                                self.show_welcome();
                            }
                            if let Some(design) = &self.designer {
                                let caption = if design.dirty {
                                    format!(
                                        "● {} [Design]",
                                        design
                                            .path
                                            .file_stem()
                                            .unwrap_or_default()
                                            .to_string_lossy()
                                    )
                                } else {
                                    format!(
                                        "{} [Design]",
                                        design
                                            .path
                                            .file_stem()
                                            .unwrap_or_default()
                                            .to_string_lossy()
                                    )
                                };
                                if ui.selectable_label(self.designer_active, caption).clicked() {
                                    self.designer_active = true;
                                    self.active = None;
                                }
                                if ui
                                    .small_button("×")
                                    .on_hover_text("Close visual designer")
                                    .clicked()
                                {
                                    self.close_designer();
                                }
                            }
                            for (i, doc) in self.docs.iter().enumerate() {
                                if ui
                                    .add(
                                        egui::Button::new(doc.label())
                                            .selected(self.active == Some(i))
                                            .min_size(egui::vec2(90.0, 30.0)),
                                    )
                                    .on_hover_text(
                                        doc.path
                                            .as_ref()
                                            .map(|p| p.display().to_string())
                                            .unwrap_or_else(|| format!("Untitled {}", doc.id)),
                                    )
                                    .clicked()
                                {
                                    self.active = Some(i);
                                    self.designer_active = false;
                                }
                                if ui.small_button("×").clicked() {
                                    close = Some(i);
                                }
                            }
                            if let Some(i) = close {
                                self.close(i);
                            }
                            if ui
                                .small_button("+")
                                .on_hover_text("New file · Ctrl+N")
                                .clicked()
                            {
                                self.new_document();
                            }
                            if ui
                                .small_button("Find")
                                .on_hover_text("Find · Ctrl+F")
                                .clicked()
                            {
                                self.show_sidebar(SidebarView::Search);
                                self.show_search = true;
                                self.find_focus = true;
                            }
                        });
                    });
                if let Some(doc) = self.active.and_then(|i| self.docs.get(i)) {
                    ui.label(
                        RichText::new(
                            doc.path
                                .as_ref()
                                .map(|p| p.display().to_string())
                                .unwrap_or_else(|| format!("Untitled {} · Hydrogen", doc.id)),
                        )
                        .small()
                        .color(theme::colors(ui.ctx()).muted.color()),
                    );
                }

                ui.separator();
                if self.designer_active {
                    if let Some(document) = self.designer.as_mut() {
                        document.show(ui);
                    }
                } else if let Some(i) = self.active {
                    let active_diags = self
                        .docs
                        .get(i)
                        .map(diagnostic_path)
                        .and_then(|p| self.diagnostics.get(&p))
                        .cloned()
                        .unwrap_or_default();
                    if let Some(doc) = self.docs.get_mut(i) {
                        let is_hydrogen = doc
                            .path
                            .as_ref()
                            .and_then(|p| p.extension())
                            .and_then(|s| s.to_str())
                            .is_some_and(|ext| ext == "hy")
                            || doc.path.is_none();
                        let wrap = self.settings.word_wrap;
                        let doc_id = doc.id;
                        let edit_id = egui::Id::new(("quark-document-editor", doc_id));
                        let viewport_height = ui.available_height().max(120.0);
                        let line_count =
                            doc.text.as_bytes().iter().filter(|b| **b == b'\n').count() + 1;
                        let digits = line_count.to_string().len() as f32;
                        let gutter_width = 28.0 + (digits.max(2.0) * 8.5);
                        let viewport_width = (ui.available_width() - gutter_width).max(150.0);
                        let decoration_diags = active_diags.clone();
                        let mut layouter = move |ui: &egui::Ui, text: &str, wrap_width: f32| {
                            let width = if wrap { wrap_width } else { f32::INFINITY };
                            let mut job = if is_hydrogen {
                                editor::cached_highlight(ui.ctx(), text, ui.visuals().text_color())
                            } else {
                                egui::text::LayoutJob::simple(
                                    text.to_owned(),
                                    egui::FontId::monospace(14.0),
                                    ui.visuals().text_color(),
                                    width,
                                )
                            };
                            if is_hydrogen {
                                // Both sections and diagnostics are source-order sorted:
                                // sweep once instead of checking every token against
                                // every warning in large Hydrogen files.
                                let mut diagnostic_index = 0_usize;
                                for section in &mut job.sections {
                                    while diagnostic_index < decoration_diags.len()
                                        && decoration_diags[diagnostic_index].end
                                            <= section.byte_range.start
                                    {
                                        diagnostic_index += 1;
                                    }
                                    if let Some(diag) = decoration_diags.get(diagnostic_index) {
                                        if section.byte_range.start < diag.end
                                            && diag.start < section.byte_range.end
                                        {
                                            let colour = match diag.severity {
                                                diagnostics::Severity::Error => {
                                                    ui.visuals().error_fg_color
                                                }
                                                diagnostics::Severity::Warning => {
                                                    theme::colors(ui.ctx()).warning.color()
                                                }
                                                diagnostics::Severity::Hint => {
                                                    theme::colors(ui.ctx()).muted.color()
                                                }
                                            };
                                            section.format.underline =
                                                egui::Stroke::new(1.0_f32, colour);
                                        }
                                    }
                                }
                            }
                            job.wrap.max_width = width;
                            ui.fonts(|fonts| fonts.layout_job(job))
                        };
                        // The diagnostic gutter is pinned outside the independently
                        // scrollable editor viewport. Its Y positions come from the
                        // *same* galley, so soft-wrapped rows remain aligned.
                        let output = ui
                            .horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 0.0;
                                let (gutter_rect, _) = ui.allocate_exact_size(
                                    egui::vec2(gutter_width, viewport_height),
                                    egui::Sense::hover(),
                                );
                                let output = egui::ScrollArea::new([!wrap, true])
                                    .id_salt(("quark-scroll", doc_id))
                                    .auto_shrink([false, false])
                                    .max_width(viewport_width)
                                    .max_height(viewport_height)
                                    .show(ui, |ui| {
                                        ui.with_layout(
                                            egui::Layout::top_down(egui::Align::Min),
                                            |ui| {
                                                ui.set_min_width(viewport_width);
                                                TextEdit::multiline(&mut doc.text)
                                                    .id(edit_id)
                                                    .font(egui::TextStyle::Monospace)
                                                    .code_editor()
                                                    .frame(false)
                                                    .min_size(egui::vec2(
                                                        viewport_width,
                                                        viewport_height - 8.0,
                                                    ))
                                                    .desired_rows(1)
                                                    .desired_width(viewport_width)
                                                    .lock_focus(true)
                                                    .layouter(&mut layouter)
                                                    .show(ui)
                                            },
                                        )
                                        .inner
                                    })
                                    .inner;
                                let painter = ui.painter_at(gutter_rect);
                                painter.rect_filled(
                                    gutter_rect,
                                    0.0,
                                    theme::colors(ui.ctx()).sidebar.color(),
                                );
                                painter.line_segment(
                                    [
                                        egui::pos2(gutter_rect.right() - 1.0, gutter_rect.top()),
                                        egui::pos2(gutter_rect.right() - 1.0, gutter_rect.bottom()),
                                    ],
                                    egui::Stroke::new(
                                        1.0_f32,
                                        ui.visuals().widgets.noninteractive.bg_stroke.color,
                                    ),
                                );
                                let mut markers = std::collections::BTreeMap::<
                                    usize,
                                    Vec<&diagnostics::Diagnostic>,
                                >::new();
                                for diagnostic in &active_diags {
                                    markers.entry(diagnostic.line).or_default().push(diagnostic);
                                }
                                let mut logical_line = 1_usize;
                                let mut first_visual_row = true;
                                for row in &output.galley.rows {
                                    let y = output.galley_pos.y + row.rect.top();
                                    let visible = y + row.rect.height() >= gutter_rect.top()
                                        && y <= gutter_rect.bottom();
                                    if first_visual_row && visible {
                                        painter.text(
                                            egui::pos2(gutter_rect.right() - 6.0, y),
                                            egui::Align2::RIGHT_TOP,
                                            logical_line.to_string(),
                                            egui::FontId::monospace(12.0),
                                            theme::colors(ui.ctx()).muted.color(),
                                        );
                                        if let Some(on_line) = markers.get(&logical_line) {
                                            let severity = if on_line
                                                .iter()
                                                .any(|d| d.severity == diagnostics::Severity::Error)
                                            {
                                                diagnostics::Severity::Error
                                            } else if on_line.iter().any(|d| {
                                                d.severity == diagnostics::Severity::Warning
                                            }) {
                                                diagnostics::Severity::Warning
                                            } else {
                                                diagnostics::Severity::Hint
                                            };
                                            let (symbol, colour) = match severity {
                                                diagnostics::Severity::Error => {
                                                    ("!", ui.visuals().error_fg_color)
                                                }
                                                diagnostics::Severity::Warning => {
                                                    ("!", theme::colors(ui.ctx()).warning.color())
                                                }
                                                diagnostics::Severity::Hint => {
                                                    ("o", theme::colors(ui.ctx()).muted.color())
                                                }
                                            };
                                            let marker = egui::Rect::from_min_size(
                                                egui::pos2(gutter_rect.left() + 3.0, y),
                                                egui::vec2(16.0, 18.0),
                                            );
                                            painter.text(
                                                marker.center(),
                                                egui::Align2::CENTER_CENTER,
                                                symbol,
                                                egui::FontId::proportional(13.0),
                                                colour,
                                            );
                                            ui.interact(
                                                marker,
                                                ui.id().with(("diag-marker", doc_id, logical_line)),
                                                egui::Sense::hover(),
                                            )
                                            .on_hover_ui(|ui| {
                                                for d in on_line {
                                                    ui.label(&d.message);
                                                }
                                            });
                                        }
                                    }
                                    if row.ends_with_newline {
                                        logical_line += 1;
                                        first_visual_row = true;
                                    } else {
                                        first_visual_row = false;
                                    }
                                }
                                output
                            })
                            .inner;
                        if self.focus_editor {
                            output.response.request_focus();
                            self.focus_editor = false;
                        }
                        if output.response.changed() {
                            self.status = "Modified".into();
                            self.diagnostics.clear();
                            self.diagnostics_revision = self.diagnostics_revision.wrapping_add(1);
                            self.diagnostics_requested = true;
                            self.diagnostics_due = ctx.input(|i| i.time) + 0.5;
                        }
                        self.caret_position = output.cursor_range.map(|range| {
                            navigation::line_col(&doc.text, range.primary.ccursor.index)
                        });
                        let mut next_popup = None;
                        let was_manual = self.manual_completion;
                        self.manual_completion = false;
                        if is_hydrogen && (output.response.has_focus() || was_manual) {
                            if let Some(range) = output.cursor_range {
                                if range.primary.ccursor.index == range.secondary.ccursor.index {
                                    let cursor_char = range.primary.ccursor.index;
                                    let prefix = prefix_at(&doc.text, cursor_char);
                                    let keep_open = self.completion.as_ref().is_some_and(|popup| {
                                        popup.doc_id == doc_id
                                            && popup.cursor_char == cursor_char
                                            && popup.prefix == prefix
                                    });
                                    if (was_manual
                                        || keep_open
                                        || self.settings.automatic_completion
                                            && prefix.chars().count() >= 2)
                                        && editor::completion_context(&doc.text, cursor_char)
                                        && self.dismissed_completion.as_ref()
                                            != Some(&(doc_id, cursor_char, prefix.clone()))
                                    {
                                        let candidates: Vec<_> =
                                            completion_candidates(&doc.text, &prefix)
                                                .into_iter()
                                                .take(8)
                                                .collect();
                                        if !candidates.is_empty() {
                                            let caret =
                                                output.galley.pos_from_cursor(&range.primary);
                                            let screen = ctx.screen_rect();
                                            let expected_height =
                                                39.0 + 23.0 * candidates.len() as f32 + 25.0;
                                            let x = (output.galley_pos.x + caret.min.x).clamp(
                                                screen.left() + 4.0,
                                                (screen.right() - 280.0).max(screen.left() + 4.0),
                                            );
                                            let below = output.galley_pos.y + caret.max.y + 5.0;
                                            let y = if below + expected_height
                                                > screen.bottom() - 8.0
                                            {
                                                (output.galley_pos.y + caret.min.y
                                                    - expected_height
                                                    - 5.0)
                                                    .max(screen.top() + 4.0)
                                            } else {
                                                below
                                            };
                                            let selected = self
                                                .completion
                                                .as_ref()
                                                .filter(|p| {
                                                    p.doc_id == doc_id && p.prefix == prefix
                                                })
                                                .map_or(0, |p| {
                                                    p.selected.min(candidates.len() - 1)
                                                });
                                            next_popup = Some(CompletionPopup {
                                                doc_id,
                                                edit_id,
                                                cursor_char,
                                                prefix,
                                                candidates,
                                                anchor: egui::pos2(x, y),
                                                selected,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                        // Allow a pending pointer click on the completion popup to
                        // reach its buttons even after the text field loses focus.
                        let popup_click = ctx.input(|input| input.pointer.any_pressed())
                            && self.completion.as_ref().is_some_and(|p| p.doc_id == doc_id);
                        if next_popup.is_some() {
                            self.completion = next_popup;
                        } else if !popup_click {
                            self.completion = None;
                        }
                    }
                } else {
                    self.welcome(ui);
                    self.completion = None;
                }
            });
        if let Some((path, line)) = self.pending_line.clone() {
            if self
                .active
                .and_then(|i| self.docs.get(i))
                .is_some_and(|d| d.path.as_ref() == Some(&path))
                && self
                    .editor_id()
                    .and_then(|id| TextEdit::load_state(ctx, id))
                    .is_some()
            {
                if let Some(index) = self
                    .active
                    .and_then(|i| navigation::line_start(&self.docs[i].text, line))
                {
                    self.set_selection(ctx, index, index);
                }
                self.pending_line = None;
            }
        }
        self.draw_completion(ctx);

        self.palette_ui(ctx);
        if self.show_settings {
            let mut open = true;
            egui::Window::new("Settings").open(&mut open).default_width(620.0).vscroll(true).show(ctx,|ui|{
                if ui.checkbox(&mut self.settings.integrated_title_bar,"Integrated window controls").on_hover_text("Minimise, maximise/restore and close in Quark’s top bar. Disable to use your system title bar.").changed(){self.persist();}
                ui.label("Drag QUARK to move the window; double-click it to maximise or restore.");
                if ui.button("Colours, icons and syntax themes…").clicked(){self.show_sidebar(SidebarView::Themes);self.show_settings=false;}
                ui.separator();
                ui.heading("Hydrogen toolchain");
                ui.label("Executable name or absolute path (arguments are configured by Quark)");
                if ui.add_enabled(!self.probe_running,TextEdit::singleline(&mut self.settings.compiler)).changed(){self.settings.compiler_verbs.clear();}
                ui.horizontal_wrapped(|ui|{
                    if ui.button("SDK profile").on_hover_text("hy: build / check / run").clicked(){self.settings.compiler_verbs=vec!["build".into(),"check".into(),"run".into()];}
                    if ui.button("Single-file compiler profile").on_hover_text("Conservative profile: compile only. Detect or enable other commands your compiler supports.").clicked(){self.settings.compiler_verbs=vec!["compile".into()];}
                    if ui.add_enabled(self.job_rx.is_none(),egui::Button::new("Detect / test executable")).clicked(){
                        self.probe_output.clear();self.launch(self.settings.compiler.clone(),vec!["--help".into()],self.cwd());self.probe_running=self.job_rx.is_some();
                    }
                });
                ui.label(RichText::new("Enabled commands · review detected capabilities for custom wrappers").small().color(theme::colors(ui.ctx()).muted.color()));
                ui.horizontal(|ui|{for verb in ["build","compile","check","run"]{let mut enabled=self.settings.compiler_verbs.iter().any(|v|v==verb);if ui.checkbox(&mut enabled,verb).changed(){self.settings.compiler_verbs.retain(|v|v!=verb);if enabled{self.settings.compiler_verbs.push(verb.into());}}}});
                ui.separator();ui.heading("Welcome page");
                let welcome_changed = ui.checkbox(&mut self.settings.welcome_shortcuts_visible,"Show VS Code shortcut guide").changed()
                    | ui.checkbox(&mut self.settings.welcome_build_visible,"Show first-build guide").changed();
                if welcome_changed {self.persist();}
                ui.separator(); ui.heading("Editor & commands");
                ui.checkbox(&mut self.settings.automatic_completion,"Show suggestions automatically (Ctrl/Cmd+Space always available)");
                ui.checkbox(&mut self.settings.word_wrap,"Word wrap");
                ui.checkbox(&mut self.settings.include_generated,"Include generated folders in workspace index (.git stays excluded)");
                ui.horizontal(|ui|{ui.label("Index entry limit");ui.add(egui::DragValue::new(&mut self.settings.index_limit).range(100..=1_000_000));});
                ui.horizontal(|ui|{ui.label("Command timeout in seconds (0 = unlimited)");ui.add(egui::DragValue::new(&mut self.settings.command_timeout_seconds).range(0..=86400));});
                ui.label(format!("Command / Git folder: {}",self.cwd().display()));
                ui.horizontal(|ui|{
                    if ui.button("Choose folder…").clicked(){if let Some(p)=rfd::FileDialog::new().pick_folder(){self.settings.command_folder=Some(p);}}
                    if ui.button("Follow selected project / first workspace folder").clicked(){self.settings.command_folder=None;}
                });
                ui.separator();ui.heading("Workspace");
                for folder in self.settings.folders.clone(){ui.horizontal(|ui|{if ui.small_button("Remove").clicked(){self.settings.folders.retain(|p|p!=&folder);if self.settings.command_folder.as_ref()==Some(&folder){self.settings.command_folder=None;}self.workspace_requested=true;}ui.label(folder.display().to_string());});}
                for project in self.settings.projects.clone(){ui.horizontal(|ui|{if ui.small_button("Remove project").clicked(){self.settings.projects.retain(|p|p!=&project);if self.settings.selected_project.as_ref()==Some(&project){self.settings.selected_project=None;}self.workspace_requested=true;self.request_diagnostic_scan();}ui.label(project.display().to_string());});}
                if self.settings_blocked || self.session_blocked {
                    ui.separator(); ui.colored_label(theme::colors(ui.ctx()).warning.color(),"Persistence paused to preserve an unreadable file.");
                    if ui.button("Back up unreadable files and resume persistence").clicked(){self.reset_persistence();}
                }
                ui.separator(); ui.label(RichText::new("Changes apply immediately. Save settings to retain them after restart.").small().color(theme::colors(ui.ctx()).muted.color()));
                if ui.button("Save settings").clicked(){self.persist();self.workspace_requested=true;if !self.settings_blocked{self.show_settings=false;}}
            });
            if !open {
                self.show_settings = false;
            }
        }
        if self.show_go_to_line {
            let mut open = true;
            egui::Window::new("Go to line")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(260.0)
                .show(ctx, |ui| {
                    ui.label("Line number");
                    let response =
                        ui.add(TextEdit::singleline(&mut self.go_line_input).hint_text("e.g. 128"));
                    if self.go_focus {
                        response.request_focus();
                        self.go_focus = false;
                    }
                    let pressed = response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                    if ui.small_button("Go").clicked() || pressed {
                        self.go_to_line(ctx);
                    }
                });
            if !open {
                self.show_go_to_line = false;
            }
        }
        if self.show_quick_open {
            let mut open = true;
            let mut chosen = None;
            egui::Window::new("Go to File")
                .open(&mut open)
                .collapsible(false)
                .default_width(680.0)
                .show(ctx, |ui| {
                    let response = ui.add(
                        TextEdit::singleline(&mut self.quick_query)
                            .hint_text("Search files by name or path…")
                            .desired_width(f32::INFINITY),
                    );
                    if self.quick_focus {
                        response.request_focus();
                        self.quick_focus = false;
                    }
                    if response.changed() {
                        self.quick_selected = 0;
                    }
                    let query = self.quick_query.to_lowercase();
                    if query != self.quick_filter || self.quick_revision != self.workspace_revision
                    {
                        self.quick_filtered = self
                            .workspace
                            .files
                            .iter()
                            .filter(|p| p.to_string_lossy().to_lowercase().contains(&query))
                            .cloned()
                            .collect();
                        self.quick_filter = query;
                        self.quick_revision = self.workspace_revision;
                    }
                    let matches = &self.quick_filtered;
                    self.quick_selected = self.quick_selected.min(matches.len().saturating_sub(1));
                    let mut move_selection = false;
                    if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowDown))
                        && !matches.is_empty()
                    {
                        self.quick_selected = (self.quick_selected + 1) % matches.len();
                        move_selection = true;
                    }
                    if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowUp))
                        && !matches.is_empty()
                    {
                        self.quick_selected =
                            (self.quick_selected + matches.len() - 1) % matches.len();
                        move_selection = true;
                    }
                    if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) {
                        chosen = matches.get(self.quick_selected).cloned();
                    }
                    ui.label(
                        RichText::new(format!(
                            "{} matches · Up / Down to select · Enter to open",
                            matches.len()
                        ))
                        .small()
                        .color(theme::colors(ui.ctx()).muted.color()),
                    );
                    let mut scroll = egui::ScrollArea::vertical().max_height(340.0);
                    if move_selection {
                        scroll = scroll.vertical_scroll_offset(self.quick_selected as f32 * 27.0);
                    }
                    scroll.show_rows(ui, 27.0, matches.len(), |ui, rows| {
                        for i in rows {
                            let p = &matches[i];
                            let r = ui.selectable_label(
                                i == self.quick_selected,
                                p.display().to_string(),
                            );
                            if r.clicked() {
                                chosen = Some(p.clone());
                            }
                        }
                    });
                    if self.workspace_rx.is_some() {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Indexing…");
                        });
                    }
                    if matches.is_empty() {
                        ui.label(
                            "No files found. Add a folder, refresh the index, or use Open File.",
                        );
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Open File…").clicked() {
                            chosen = rfd::FileDialog::new().pick_file();
                        }
                        if ui.button("Index settings…").clicked() {
                            self.show_settings = true;
                        }
                    });
                    for notice in self.workspace.notices.iter().take(3) {
                        ui.colored_label(theme::colors(ui.ctx()).warning.color(), notice);
                    }
                });
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
                open = false;
            }
            if let Some(p) = chosen {
                self.open(p);
                self.show_quick_open = false;
            }
            if !open {
                self.show_quick_open = false;
            }
        }
        if self.show_clone {
            let mut open = true;
            egui::Window::new("Clone Git repository")
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label("HTTPS or SSH repository URL:");
                    ui.text_edit_singleline(&mut self.clone_url);
                    ui.label("Choose an empty folder. The repository will be cloned into that folder and added only after success.");
                    if ui.add_enabled(self.job_rx.is_none() && !self.clone_url.trim().is_empty(),egui::Button::new("Choose empty destination and clone")).clicked() {
                        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                            self.launch(
                                "git".into(),
                                vec!["clone".into(), "--".into(), self.clone_url.trim().into(), ".".into()],
                                dir.clone(),
                            );
                            if self.job_rx.is_some(){self.pending_clone=Some(dir);}
                            self.show_clone = false;
                        }
                    }
                });
            if !open {
                self.show_clone = false;
            }
        }
        self.new_project_dialog(ctx);
        self.new_layout_dialog(ctx);
        self.catalog_windows(ctx);
        self.safety_dialogs(ctx);
        if self.settings.integrated_title_bar && !self.exit_requested {
            window::resize(ctx);
        }
        ctx.request_repaint_after(Duration::from_millis(
            if self.job_rx.is_some()
                || self.workspace_rx.is_some()
                || self.search_rx.is_some()
                || self.terminal.is_some()
            {
                50
            } else {
                250
            },
        ));
    }
}
fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Quark IDE")
            .with_inner_size([1280., 800.])
            .with_min_inner_size([800., 500.]),
        ..Default::default()
    };
    eframe::run_native(
        "Quark IDE",
        options,
        Box::new(|cc| Ok(Box::new(Quark::new(cc)))),
    )
}

#[cfg(test)]
mod settings_tests {
    use super::*;
    #[test]
    fn defaults_show_explorer_and_keep_welcome_uncluttered() {
        let settings = Settings::default();
        assert!(settings.sidebar_visible && !settings.terminal_visible);
    }
    #[test]
    fn old_settings_and_new_preferences_remain_compatible() {
        let old: Settings = serde_json::from_str(r#"{"word_wrap":true}"#).unwrap();
        assert!(old.sidebar_visible && old.terminal_visible && old.word_wrap);
        let mut new = old;
        new.sidebar_visible = false;
        new.terminal_visible = false;
        let reloaded: Settings =
            serde_json::from_str(&serde_json::to_string(&new).unwrap()).unwrap();
        assert!(!reloaded.sidebar_visible && !reloaded.terminal_visible);
    }
}

#[cfg(test)]
mod reliability_tests {
    use super::*;
    use eframe::App;
    fn app() -> Quark {
        Quark::from_state(
            Settings::default(),
            Session::default(),
            Some("test settings persistence disabled".into()),
            Some("test session persistence disabled".into()),
        )
    }
    fn frame(app: &mut Quark, ctx: &egui::Context, input: egui::RawInput) -> egui::FullOutput {
        let mut native = eframe::Frame::_new_kittest();
        ctx.run(input, |ctx| app.update(ctx, &mut native))
    }
    fn input() -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            ..Default::default()
        }
    }
    fn click_text(q: &mut Quark, ctx: &egui::Context, label: &str) {
        let mut pos = None;
        for _ in 0..2 {
            let output = frame(q, ctx, input());
            for shape in output.shapes {
                if let egui::epaint::Shape::Text(t) = shape.shape {
                    if t.galley.text() == label {
                        pos = Some(t.pos + t.galley.size() * 0.5);
                    }
                }
            }
        }
        let pos = pos.unwrap_or_else(|| panic!("Missing button {label}"));
        for pressed in [true, false] {
            let mut i = input();
            i.events = vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ];
            frame(q, ctx, i);
        }
    }
    #[test]
    fn discard_and_cancel_buttons_preserve_their_expected_versions() {
        let ctx = egui::Context::default();
        let mut q = app();
        q.new_document();
        q.docs[0].text = "unsaved".into();
        q.close(0);
        click_text(&mut q, &ctx, "Cancel");
        assert_eq!(q.docs[0].text, "unsaved");
        assert!(q.pending_close.is_none());
        q.close(0);
        click_text(&mut q, &ctx, "Discard");
        assert!(q.docs.is_empty());
    }
    #[test]
    fn conflict_reload_reads_external_content_without_overwriting_disk() {
        let ctx = egui::Context::default();
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("test.hy");
        fs::write(&p, "external").unwrap();
        let mut q = app();
        q.docs.push(Document {
            id: 1,
            path: Some(p.clone()),
            text: "my edits".into(),
            saved: "old".into(),
        });
        q.active = Some(0);
        assert!(!q.save(0));
        click_text(&mut q, &ctx, "Reload disk · discard my edits");
        assert_eq!(q.docs[0].text, "external");
        assert_eq!(q.docs[0].saved, "external");
        assert_eq!(fs::read_to_string(p).unwrap(), "external");
    }
    #[test]
    fn modified_shortcuts_never_trigger_the_unmodified_action() {
        let ctx = egui::Context::default();
        let mut q = app();
        let mut i = input();
        i.events.push(egui::Event::Key {
            key: Key::B,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::CTRL | Modifiers::COMMAND | Modifiers::SHIFT,
        });
        frame(&mut q, &ctx, i);
        assert!(q.settings.sidebar_visible);
        assert!(q.status.contains("project"));
        for _ in 0..3 {
            q.new_document();
        }
        q.active = Some(0);
        let mut i = input();
        i.events.push(egui::Event::Key {
            key: Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::CTRL | Modifiers::COMMAND | Modifiers::SHIFT,
        });
        frame(&mut q, &ctx, i);
        assert_eq!(q.active, Some(2));
        let mut i = input();
        i.events.push(egui::Event::Key {
            key: Key::P,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::CTRL | Modifiers::COMMAND | Modifiers::SHIFT,
        });
        frame(&mut q, &ctx, i);
        assert!(q.show_palette);
        assert!(!q.show_quick_open);
    }
    #[test]
    fn escape_closes_go_to_file_without_losing_documents() {
        let ctx = egui::Context::default();
        let mut q = app();
        q.new_document();
        q.docs[0].text = "keep".into();
        q.show_quick_open = true;
        q.quick_focus = true;
        frame(&mut q, &ctx, input());
        let mut i = input();
        i.events.push(egui::Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
        frame(&mut q, &ctx, i);
        assert!(!q.show_quick_open);
        assert_eq!(q.docs[0].text, "keep");
    }
    #[test]
    fn editing_manifest_ids_never_adds_workspace_roots_or_changes_build_target() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("app.hyproj");
        let dependency = dir.path().join("library.hyproj");
        fs::write(
            &root,
            "name='App'\nid='original-app'\nproject_references=['library.hyproj']\n",
        )
        .unwrap();
        fs::write(
            &dependency,
            "name='Library'\nid='original-library'\ntype='lib'\n",
        )
        .unwrap();
        let root = root.canonicalize().unwrap();
        let dependency = dependency.canonicalize().unwrap();
        let mut q = app();
        q.add_project(root.clone());
        let folders = q.settings.folders.clone();
        q.open(dependency.clone());
        assert_eq!(q.settings.projects, vec![root.clone()]);
        assert_eq!(q.selected_project(), Some(root.clone()));
        for path in [&dependency, &root] {
            q.open(path.clone());
            let index = q.active.unwrap();
            q.docs[index].text = q.docs[index].text.replace("original-", "updated-");
            assert!(q.save(index));
            assert!(q.save(index));
            assert!(fs::read_to_string(path).unwrap().contains("updated-"));
            assert_eq!(q.settings.projects, vec![root.clone()]);
            assert_eq!(q.selected_project(), Some(root.clone()));
            assert_eq!(q.settings.folders, folders);
        }
        assert_eq!(project::load(&root).references.len(), 1);
        let other = dir.path().join("standalone.hyproj");
        fs::write(&other, "id='standalone'\n").unwrap();
        let mut folder_only = app();
        folder_only.add_folder(dir.path().to_path_buf());
        folder_only.open(other.clone());
        folder_only.docs[0].text = "id='edited'\n".into();
        assert!(folder_only.save(0));
        assert!(folder_only.settings.projects.is_empty());
        assert!(folder_only.settings.selected_project.is_none());
        // Explicitly opening a project still registers it, once.
        folder_only.add_project(other.clone());
        folder_only.add_project(other.clone());
        assert_eq!(
            folder_only.settings.projects,
            vec![other.canonicalize().unwrap()]
        );
    }
    #[test]
    fn integrated_window_controls_emit_native_commands_and_guard_dirty_close() {
        fn click(q: &mut Quark, ctx: &egui::Context, x: f32) -> egui::FullOutput {
            let pos = egui::pos2(x, 14.0);
            let mut result = None;
            for pressed in [true, false] {
                let mut i = input();
                i.events = vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ];
                result = Some(frame(q, ctx, i));
            }
            result.unwrap()
        }
        fn has(output: &egui::FullOutput, command: egui::ViewportCommand) -> bool {
            output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .contains(&command)
        }
        let ctx = egui::Context::default();
        let mut q = app();
        let first = frame(&mut q, &ctx, input());
        assert!(has(&first, egui::ViewportCommand::Decorations(false)));
        frame(&mut q, &ctx, input());
        assert!(has(
            &click(&mut q, &ctx, 1192.0),
            egui::ViewportCommand::Minimized(true)
        ));
        assert!(has(
            &click(&mut q, &ctx, 1225.0),
            egui::ViewportCommand::Maximized(true)
        ));
        let mut maximised = input();
        maximised
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .maximized = Some(true);
        let output = ctx.run(maximised, window::toggle_maximise);
        assert!(has(&output, egui::ViewportCommand::Maximized(false)));
        q.new_document();
        q.docs[0].text = "unsaved".into();
        frame(&mut q, &ctx, input());
        let close = click(&mut q, &ctx, 1258.0);
        assert!(q.exit_requested && !q.allow_exit);
        assert!(!has(&close, egui::ViewportCommand::Close));
        assert_eq!(q.docs[0].text, "unsaved");
        q.exit_requested = false;
        q.docs[0].saved = q.docs[0].text.clone();
        frame(&mut q, &ctx, input());
        assert!(has(
            &click(&mut q, &ctx, 1258.0),
            egui::ViewportCommand::Close
        ));
        q.settings.integrated_title_bar = false;
        assert!(has(
            &frame(&mut q, &ctx, input()),
            egui::ViewportCommand::Decorations(true)
        ));
    }
    #[test]
    fn sidebar_owns_lower_left_and_header_is_a_single_row() {
        let ctx = egui::Context::default();
        let mut q = app();
        q.settings.terminal_visible = true;
        q.settings.bottom_tab = BottomTab::Output;
        for _ in 0..3 {
            frame(&mut q, &ctx, input());
        }
        assert!(q.sidebar_rect.right() <= q.bottom_rect.left() + 1.0);
        assert!((q.sidebar_rect.bottom() - q.bottom_rect.bottom()).abs() < 2.0);
        assert!(q.top_rect.height() < 45.0, "{:?}", q.top_rect);
        assert!(q.sidebar_rect.width() >= 240.0);
        assert!(q.terminal.is_none());
    }
    #[test]
    fn search_plugins_and_themes_are_distinct_sidebar_views() {
        let ctx = egui::Context::default();
        let mut q = app();
        for view in [
            SidebarView::Search,
            SidebarView::Plugins,
            SidebarView::Themes,
        ] {
            q.show_sidebar(view);
            frame(&mut q, &ctx, input());
            assert_eq!(q.settings.sidebar_view, view);
            assert!(q.sidebar_rect.is_positive());
        }
    }
    #[test]
    fn tool_jobs_select_read_only_output_without_starting_a_shell() {
        let ctx = egui::Context::default();
        let mut q = app();
        q.launch(
            "quark-missing-program-test".into(),
            vec![],
            PathBuf::from("."),
        );
        assert_eq!(q.settings.bottom_tab, BottomTab::Output);
        assert!(q.settings.terminal_visible);
        assert!(q.terminal.is_none());
        let mut i = input();
        i.events.push(egui::Event::Text("do not run this".into()));
        frame(&mut q, &ctx, i);
        assert!(!q.output.contains("do not run this"));
        q.on_exit(None);
    }
    #[test]
    fn theme_choices_and_custom_colours_roundtrip_in_preferences() {
        let ctx = egui::Context::default();
        let mut q = app();
        for preset in theme::Preset::ALL {
            q.settings.theme = theme::Settings {
                preset,
                custom: None,
            };
            frame(&mut q, &ctx, input());
            assert_eq!(theme::colors(&ctx), preset.definition().colors);
        }
        let mut d = q.settings.theme.definition().clone();
        d.colors.icon = theme::Rgb([12, 34, 56]);
        q.settings.theme.custom = Some(d.clone());
        let settings: Settings =
            serde_json::from_str(&serde_json::to_string(&q.settings).unwrap()).unwrap();
        assert_eq!(settings.theme.definition(), &d);
    }
    #[test]
    fn invalid_tool_actions_report_to_the_output_tab() {
        let mut q = app();
        q.build(BuildScope::Project);
        assert_eq!(q.settings.bottom_tab, BottomTab::Output);
        assert!(q.output.contains(".hyproj"));
        assert!(q.job_rx.is_none());
    }
    #[test]
    fn enter_in_search_selects_a_match_without_editing_the_document() {
        let ctx = egui::Context::default();
        let mut q = app();
        q.new_document();
        q.docs[0].text = "needle needle".into();
        frame(&mut q, &ctx, input());
        q.show_sidebar(SidebarView::Search);
        q.search = "needle".into();
        q.find_focus = true;
        frame(&mut q, &ctx, input());
        let mut i = input();
        i.events.push(egui::Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
        frame(&mut q, &ctx, i);
        assert_eq!(q.docs[0].text, "needle needle");
        assert_eq!(q.status, "Match selected");
    }
    #[test]
    fn command_directory_does_not_change_when_tabs_change() {
        let mut q = app();
        q.settings.folders.push(PathBuf::from("/workspace"));
        q.docs.push(Document {
            id: 1,
            path: Some(PathBuf::from("/other/repository/file.hy")),
            text: String::new(),
            saved: String::new(),
        });
        q.active = Some(0);
        assert_eq!(q.cwd(), PathBuf::from("/workspace"));
        q.settings.command_folder = Some(PathBuf::from("/explicit"));
        q.settings.projects.push(PathBuf::from("/project/a.hyproj"));
        assert_eq!(q.cwd(), PathBuf::from("/explicit"));
    }
    #[test]
    fn dirty_close_requests_a_decision_and_keeps_text() {
        let mut q = app();
        q.new_document();
        q.docs[0].text = "work".into();
        q.close(0);
        assert_eq!(q.docs[0].text, "work");
        assert_eq!(q.pending_close, Some(0));
        q.docs[0].saved = "work".into();
        q.pending_close = None;
        q.close(0);
        assert!(q.docs.is_empty());
    }
    #[test]
    fn application_exit_is_cancelled_while_dirty() {
        let ctx = egui::Context::default();
        let mut q = app();
        q.new_document();
        q.docs[0].text = "keep me".into();
        let mut i = input();
        i.viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .events
            .push(egui::ViewportEvent::Close);
        let out = frame(&mut q, &ctx, i);
        assert!(q.exit_requested && !q.allow_exit);
        assert!(out.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::CancelClose));
        assert_eq!(q.docs[0].text, "keep me");
    }
    #[test]
    fn corrupt_settings_are_preserved_with_a_visible_warning() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, "broken").unwrap();
        let (_, warning) = load_json::<Settings>(Some(&path));
        assert!(warning.unwrap().contains("preserved"));
        assert_eq!(fs::read_to_string(path).unwrap(), "broken");
    }
    #[test]
    fn recovery_retains_unsaved_baseline_for_conflict_detection() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("test.hy");
        fs::write(&p, "external").unwrap();
        let session = Session {
            docs: vec![Document {
                id: 7,
                path: Some(p.clone()),
                text: "my edits".into(),
                saved: "old disk".into(),
            }],
            active: Some(99),
        };
        let serialized = serde_json::to_string(&session).unwrap();
        let mut q = Quark::from_state(
            Settings::default(),
            serde_json::from_str(&serialized).unwrap(),
            Some("test".into()),
            Some("test".into()),
        );
        assert_eq!(q.active, Some(0));
        assert_eq!(q.next_doc_id, 8);
        assert!(!q.save(0));
        assert_eq!(q.pending_conflict, Some(0));
        assert_eq!(fs::read_to_string(p).unwrap(), "external");
        assert_eq!(q.docs[0].text, "my edits");
    }
    #[test]
    fn welcome_palette_and_editor_render_with_keyboard_navigation() {
        let ctx = egui::Context::default();
        theme::apply(&ctx, &theme::Settings::default());
        let mut q = app();
        assert!(!frame(&mut q, &ctx, input()).shapes.is_empty());
        let mut i = input();
        i.events.push(egui::Event::Key {
            key: Key::F1,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
        frame(&mut q, &ctx, i);
        assert!(q.show_palette);
        q.palette_query = "New Hydrogen File".into();
        let mut i = input();
        i.events.push(egui::Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        });
        frame(&mut q, &ctx, i);
        assert!(!q.show_palette);
        assert_eq!(q.docs.len(), 1);
        q.docs[0].text = "class Demo {}".into();
        assert!(!frame(&mut q, &ctx, input()).shapes.is_empty());
    }
    #[test]
    fn unavailable_compiler_action_opens_settings_without_running() {
        let mut q = app();
        q.build(BuildScope::CurrentFile);
        assert!(q.show_settings);
        assert!(q.job_rx.is_none());
        assert!(q.status.contains("compile"));
    }
}
