mod command;
mod diagnostics;
mod editor;
mod hydrogen;
mod navigation;
mod plugins;
mod project;
mod storage;
mod symbols;
mod theme;
mod ui;
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
    #[serde(default = "default_visible")]
    terminal_visible: bool,
    automatic_completion: bool,
    include_generated: bool,
    index_limit: usize,
    command_timeout_seconds: u64,
    command_folder: Option<PathBuf>,
    compiler_verbs: Vec<String>,
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
            terminal_visible: false,
            automatic_completion: true,
            include_generated: false,
            index_limit: 50_000,
            command_timeout_seconds: 600,
            command_folder: None,
            compiler_verbs: vec!["build".into(), "check".into(), "run".into()],
        }
    }
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
    show_source_control: bool,
    explorer_view: ExplorerView,
    tree_revision: u64,
    tree_default_open: bool,
    plugins: Vec<plugins::Plugin>,
    plugin_warnings: Vec<String>,
    show_plugins: bool,
    completion: Option<CompletionPopup>,
    dismissed_completion: Option<(u64, usize, String)>,
    manual_completion: bool,
    focus_editor: bool,
    settings: Settings,
    docs: Vec<Document>,
    active: Option<usize>,
    output: String,
    command: String,
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
}
impl Quark {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply(&cc.egui_ctx);
        let (settings, settings_warning) = load_json::<Settings>(settings_file().as_deref());
        let (session, session_warning) = load_json::<Session>(session_file().as_deref());
        let mut app = Self::from_state(settings, session, settings_warning, session_warning);
        app.reload_plugins();
        app
    }
    fn from_state(
        settings: Settings,
        session: Session,
        settings_warning: Option<String>,
        session_warning: Option<String>,
    ) -> Self {
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
            show_source_control: false,
            explorer_view: ExplorerView::Files,
            tree_revision: 0,
            tree_default_open: false,
            plugins: Vec::new(),
            plugin_warnings: Vec::new(),
            show_plugins: false,
            completion: None,
            dismissed_completion: None,
            manual_completion: false,
            focus_editor: true,
            docs: session.docs,
            active: restored_active,
            output: "Quark IDE — native Rust edition\n".into(),
            command: String::new(),
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
        let result = settings_file()
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
                match session_file()
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
    fn can_run(&mut self, verb: &str) -> bool {
        if self.settings.compiler.trim().is_empty() {
            self.show_settings = true;
            self.status = "Set a Hydrogen compiler executable in Settings".into();
            return false;
        }
        if self.settings.compiler_verbs.iter().any(|v| v == verb) {
            true
        } else {
            self.show_settings = true;
            self.status = format!("Compiler profile does not support '{verb}'. Detect capabilities or choose a profile in Settings.");
            false
        }
    }
    fn add_folder(&mut self, path: PathBuf) {
        let path = path.canonicalize().unwrap_or(path);
        if !self.settings.folders.contains(&path) {
            self.settings.folders.push(path);
            self.workspace_requested = true;
            self.persist();
        }
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
        self.persist();
        self.workspace_requested = true;
        self.open(path);
        self.request_diagnostic_scan();
    }
    fn open(&mut self, path: PathBuf) {
        let path = path.canonicalize().unwrap_or(path);
        if path.extension().and_then(|e| e.to_str()) == Some("hyproj")
            && !self.settings.projects.contains(&path)
        {
            self.settings.projects.push(path.clone());
            self.workspace_requested = true;
            self.settings.selected_project = Some(path.clone());
            self.persist();
        }
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
                if path.extension().and_then(|s| s.to_str()) == Some("hyproj")
                    && !self.settings.projects.contains(&path)
                {
                    self.settings.projects.push(path.clone());
                    self.settings.selected_project = Some(path);
                    self.persist();
                }
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
            self.status = "Stop the current command before building".into();
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
                    self.status = format!("Cannot create build folder: {e}");
                    return;
                }
                self.launch(plan.program, plan.args, plan.cwd);
                if self.job_rx.is_some() {
                    self.status = plan.description;
                }
            }
            Err(e) => self.status = e,
        }
    }
    fn reload_plugins(&mut self) {
        if let Some(root) = plugins::directory(settings_file().as_deref()) {
            let (list, warnings) = plugins::load_all(&root);
            self.plugins = list;
            self.plugin_warnings = warnings;
        }
    }
    fn run_plugin(&mut self, id: &str, command_index: usize) {
        if self.job_rx.is_some() {
            self.status = "Stop the current command first".into();
            return;
        }
        if self
            .settings
            .disabled_plugins
            .iter()
            .any(|disabled| disabled == id)
        {
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
            Err(err) => self.status = err,
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
            self.status = "A command is already running".into();
            return;
        }
        self.settings.terminal_visible = true;
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
    fn shell(&mut self, command: String) {
        if command.trim().is_empty() {
            return;
        }
        #[cfg(windows)]
        let (program, args) = ("cmd".to_owned(), vec!["/C".to_owned(), command]);
        #[cfg(not(windows))]
        let (program, args) = ("/bin/sh".to_owned(), vec!["-c".to_owned(), command]);
        self.launch(program, args, self.cwd());
    }
    fn hydrogen(&mut self, verb: &str) {
        if self.job_rx.is_some() {
            self.status = "Stop the current command first".into();
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
            self.status = "Open a .hy or .hyproj first".into();
            return;
        };
        if !matches!(
            path.extension().and_then(|s| s.to_str()),
            Some("hy" | "hyproj")
        ) {
            self.status = "Open a Hydrogen file or project".into();
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
            self.show_source_control = false;
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
            if let Some(i) = self.active {
                self.save(i);
            }
        }
        if take_key(ctx, cmd, Key::F) {
            self.show_search = true;
            self.find_focus = true;
        }
        if take_key(ctx, cmd, Key::H) {
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
            && !self.show_plugins
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
            egui::CollapsingHeader::new(RichText::new(name).color(theme::MUTED))
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
            RichText::new(format!("{} [{}]", project.name, project.kind)).color(theme::ACCENT),
        )
        .id_salt((&project.path, self.tree_revision))
        .default_open(self.tree_default_open)
        .show(ui, |ui| {
            if ui.small_button("Open manifest").clicked() {
                self.open(project.path.clone());
            }
            for warning in &project.warnings {
                ui.colored_label(theme::WARM, format!("Warning: {warning}"));
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
                            .color(theme::MUTED),
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
                            .color(theme::MUTED),
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
                .color(theme::MUTED),
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
        if let Some(cancel) = &self.job_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        self.job_rx = None;
        if let Some(worker) = self.job_worker.take() {
            let _ = worker.join();
        }
        self.save_session();
    }
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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
            if self.docs.iter().any(|d| d.text != d.saved) || self.job_rx.is_some() {
                self.exit_requested = true;
            } else {
                self.save_session();
                self.allow_exit = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
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
            self.shortcuts(ctx);
            self.completion_keys(ctx);
        }
        self.diagnostic_tick(ctx);
        egui::TopBottomPanel::top("menu").show(ctx,|ui|{
            egui::menu::bar(ui,|ui|{
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(RichText::new("QUARK").strong().color(theme::ACCENT));
                ui.separator();
                ui.menu_button("File",|ui|{
                    if ui.button("New  Ctrl+N").clicked(){self.new_document();ui.close_menu();}
                    if ui.button("Open file  Ctrl+O").clicked(){if let Some(p)=rfd::FileDialog::new().pick_file(){self.open(p);}ui.close_menu();}
                    if ui.button("Open Folder…").clicked(){if let Some(p)=rfd::FileDialog::new().pick_folder(){self.add_folder(p);}ui.close_menu();}
                    if ui.button("Open .hyproj").clicked(){if let Some(p)=rfd::FileDialog::new().add_filter("Hydrogen project", &["hyproj"]).pick_file(){self.add_project(p);}ui.close_menu();}
                    if ui.button("Save  Ctrl+S").clicked(){if let Some(idx)=self.active {self.save(idx);}ui.close_menu();}
                    if ui.button("Save As…  Ctrl+Shift+S").clicked(){if let Some(i)=self.active{self.save_document(i,false,true);}ui.close_menu();}
                    if ui.button("Save all  Ctrl+Alt+S").clicked(){self.save_all();ui.close_menu();}
                    if ui.button("Close editor  Ctrl+W").clicked(){if let Some(i)=self.active{self.close(i);}ui.close_menu();}
                });
                ui.menu_button("Edit",|ui|{
                    if ui.button("Completion  Ctrl/Cmd+Space").clicked(){self.manual_completion=true;ui.close_menu();}
                    if ui.button("Find  Ctrl+F").clicked(){self.show_search=true;self.find_focus=true;ui.close_menu();}
                    if ui.button("Quick open  Ctrl+P").clicked(){self.show_quick_open=true;self.quick_focus=true;self.quick_selected=0;ui.close_menu();}
                    if ui.button("Find & replace · Ctrl+H").clicked(){self.show_search=true;self.show_replace=true;self.find_focus=true;ui.close_menu();}
                    if ui.button("Go to line · Ctrl+G").clicked(){self.show_go_to_line=true;self.go_focus=true;ui.close_menu();}
                    if ui.button("Duplicate line · Ctrl+Shift+D").clicked(){self.duplicate_active_line(ctx);ui.close_menu();}
                    if ui.button("Next tab · Ctrl+Tab").clicked(){self.next_tab(false);ui.close_menu();}
                    if ui.button("Command Palette…  F1").clicked(){self.show_palette=true;self.palette_focus=true;ui.close_menu();}
                });
                ui.menu_button("Hydrogen",|ui|{
                    if ui.button("Build active project  Ctrl+Shift+B").clicked(){self.build(BuildScope::Project);ui.close_menu();}
                    if ui.button("Compile current .hy file").clicked(){self.build(BuildScope::CurrentFile);ui.close_menu();}
                    ui.separator();
                    if ui.button("Check project").clicked(){self.hydrogen("check");ui.close_menu();}
                    if ui.button("Run (SDK)").clicked(){self.hydrogen("run");ui.close_menu();}
                    if ui.button("Settings…").clicked(){self.show_settings=true;ui.close_menu();}
                });
                ui.menu_button("Plugins",|ui|{
                    if ui.button("Manage plugins…").clicked(){self.show_plugins=true;ui.close_menu();}
                    if ui.button("Reload manifests").clicked(){self.reload_plugins();ui.close_menu();}
                    ui.separator();
                    let available:Vec<_>=self.plugins.iter().filter(|p|!self.settings.disabled_plugins.contains(&p.id)).cloned().collect();
                    if available.is_empty(){ui.label("No enabled plugins installed");}
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
                    changed |= ui.checkbox(&mut self.settings.sidebar_visible,"Explorer  Ctrl+B").changed();
                    changed |= ui.checkbox(&mut self.settings.terminal_visible,"Output  Ctrl+`").changed();
                    changed |= ui.checkbox(&mut self.settings.word_wrap,"Word wrap  Alt+Z").changed();
                    if changed{self.persist();}
                });
                ui.menu_button("Help",|ui|{
                    ui.label(format!("Quark IDE {}",env!("CARGO_PKG_VERSION")));
                    if ui.button("Welcome").clicked(){self.active=None;ui.close_menu();}
                    if ui.button("Command Palette…  F1").clicked(){self.show_palette=true;self.palette_focus=true;ui.close_menu();}
                    if ui.button("Settings").clicked(){self.show_settings=true;ui.close_menu();}
                });
                ui.separator();
                let build_button=ui.small_button(RichText::new("Build").color(theme::ACCENT))
                    .on_hover_text("Build selected .hyproj; hold Shift when clicking to compile the active .hy file");
                if build_button.clicked() {
                    let scope=if ui.input(|i|i.modifiers.shift){BuildScope::CurrentFile}else{BuildScope::Project};
                    self.build(scope);
                }
                if ui.small_button("Check").on_hover_text("Check Hydrogen project").clicked(){self.hydrogen("check");}
                if ui.small_button("Find").on_hover_text("Find in file · Ctrl+F").clicked(){self.show_search=true;self.find_focus=true;}
                if let Some(project)=self.selected_project() {
                    ui.separator();
                    ui.label(RichText::new(project.file_stem().unwrap_or_default().to_string_lossy()).small().color(theme::MUTED))
                        .on_hover_text(project.display().to_string());
                }

            });
        });
        egui::TopBottomPanel::top("command-center").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let width = ui.available_width().min(600.0);
                ui.add_space(((ui.available_width() - width) * 0.5).max(0.0));
                if ui
                    .add_sized(
                        [width, 27.0],
                        egui::Button::new("Search files  Ctrl+P     ·     Run a command  F1"),
                    )
                    .clicked()
                {
                    self.show_palette = true;
                    self.palette_focus = true;
                }
            });
        });
        egui::TopBottomPanel::bottom("status").frame(egui::Frame::new().fill(theme::BLUE).inner_margin(egui::Margin::symmetric(10,3))).show(ctx,|ui|{ui.horizontal(|ui|{

            ui.label(RichText::new(&self.status).small().color(egui::Color32::WHITE));
            let (errors, hints) = self.diagnostics.values().flatten().fold((0_usize,0_usize), |(e,h),d| {
                match d.severity {
                    diagnostics::Severity::Error => (e+1,h),
                    diagnostics::Severity::Hint => (e,h+1),
                    diagnostics::Severity::Warning => (e+1,h),
                }
            });
            {
                ui.separator();
                ui.label(RichText::new(format!("Quick checks: {errors} problems · {hints} hints{}",if self.diagnostic_rx.is_some(){" · scanning"}else if !self.analysis_complete{" · partial"}else{""})).small().color(egui::Color32::WHITE))
                    .on_hover_text(format!("Quick lexical checks and possible-unused hints only; compiler diagnostics are in Output. {} scanned files{}",
                        self.files_scanned, if self.analysis_complete {""} else {" (project analysis incomplete or no project selected)"}));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                if let Some((line,col))=self.caret_position {
                    ui.label(RichText::new(format!("Ln {line}, Col {col}")).small().color(theme::MUTED));
                }
                ui.label(RichText::new("UTF-8").small().color(theme::MUTED));
            });
        });});
        if self.settings.terminal_visible {
            egui::TopBottomPanel::bottom("terminal")
                .resizable(true)
                .min_height(140.0)
                .default_height(220.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("OUTPUT")
                                .strong()
                                .small()
                                .color(theme::ACCENT),
                        );
                        if self.job_rx.is_some() {
                            ui.spinner();
                            if ui.button("Stop").clicked(){if let Some(cancel)=&self.job_cancel{cancel.store(true,Ordering::Relaxed);}self.status="Stopping command…".into();}
                        }
                        ui.label(
                            RichText::new(self.job_cwd.clone().unwrap_or_else(||self.cwd()).display().to_string())
                                .small()
                                .color(theme::MUTED),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .small_button("×")
                                .on_hover_text("Hide Output · Ctrl+`")
                                .clicked()
                            {
                                self.settings.terminal_visible = false;
                                self.persist();
                            }
                            if ui.small_button("Clear").clicked() {
                                self.output.clear();
                            }
                        });
                    });
                    ui.label(RichText::new("One command at a time · directory is fixed above · interactive programs need an external terminal · scrollback: 256 KiB").small().color(theme::MUTED));
                    let height = (ui.available_height() - 32.0).max(35.0);
                    let lines:Vec<_>=self.output.lines().collect();
                    egui::ScrollArea::both().stick_to_bottom(true).max_height(height)
                        .show_rows(ui,17.0,lines.len(),|ui,rows|{for row in rows {ui.add(egui::Label::new(RichText::new(lines[row]).monospace()).selectable(true).extend());}});
                    ui.horizontal(|ui| {
                        let response = ui.add(
                            TextEdit::singleline(&mut self.command)
                                .desired_width(f32::INFINITY)
                                .hint_text("Run a command here · no interactive input; each command starts a fresh shell"),
                        );
                        if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) && self.job_rx.is_none() {
                            let c = std::mem::take(&mut self.command);
                            self.shell(c);
                            response.request_focus();
                        }
                    });
                });
        }
        egui::SidePanel::left("activity-bar")
            .exact_width(48.0)
            .resizable(false)
            .show(ctx, |ui| {
                ui.add_space(8.0);
                for (icon, tip, action) in [
                    (ui::Icon::Files, "Explorer · Ctrl+Shift+E", 0),
                    (ui::Icon::Search, "Find in file · Ctrl+F", 1),
                    (ui::Icon::Git, "Source Control", 2),
                    (ui::Icon::Extensions, "Plugins", 3),
                ] {
                    let selected =
                        (action == 0 && self.settings.sidebar_visible && !self.show_source_control)
                            || (action == 2
                                && self.settings.sidebar_visible
                                && self.show_source_control);
                    if ui::activity_button(ui, icon, tip, selected).clicked() {
                        match action {
                            0 => {
                                self.settings.sidebar_visible = true;
                                self.show_source_control = false;
                                self.persist();
                            }
                            1 => {
                                self.show_search = true;
                                self.find_focus = true;
                            }
                            2 => {
                                self.settings.sidebar_visible = true;
                                self.show_source_control = true;
                                self.persist();
                            }
                            _ => self.show_plugins = true,
                        }
                    }
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    if ui::activity_button(ui, ui::Icon::Settings, "Settings", false).clicked() {
                        self.show_settings = true;
                    }
                });
            });
        if self.settings.sidebar_visible {
            egui::SidePanel::left("explorer")
                .resizable(true)
                .default_width(245.0)
                .min_width(165.0)
                .show(ctx, |ui| {
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(if self.show_source_control {"SOURCE CONTROL"}else{"EXPLORER"})
                                .small()
                                .strong()
                                .color(theme::MUTED),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .small_button("×")
                                .on_hover_text("Hide Explorer · Ctrl+B")
                                .clicked()
                            {
                                self.settings.sidebar_visible = false;
                                self.persist();
                            }
                            if ui
                                .small_button("↻")
                                .on_hover_text("Refresh files, symbols and diagnostics")
                                .clicked()
                            {
                                self.workspace_requested = true;
                                self.request_diagnostic_scan();
                            }
                            if ui.small_button("+").on_hover_text("Expand all").clicked() {
                                self.tree_default_open = true;
                                self.tree_revision += 1;
                            }
                            if ui.small_button("-").on_hover_text("Collapse all").clicked() {
                                self.tree_default_open = false;
                                self.tree_revision += 1;
                            }
                        });
                    });
                    if self.show_source_control {
                        ui.label(RichText::new("Repository directory").small().color(theme::MUTED));
                        ui.label(self.cwd().display().to_string());
                        ui.separator();
                        ui.label("Git uses your system credentials. Results appear in Output.");
                        for (title,args) in [("Refresh status",vec!["status","--short","--branch"]),("View diff",vec!["diff"]),("Fetch",vec!["fetch","--all"]),("Pull",vec!["pull","--no-edit"]),("Push",vec!["push"])] {
                            if ui.add_enabled(self.job_rx.is_none(),egui::Button::new(title)).clicked(){self.git(&args);}
                        }
                        if ui.button("Choose command / Git folder…").clicked(){if let Some(p)=rfd::FileDialog::new().pick_folder(){self.settings.command_folder=Some(p);self.persist();}}
                        ui.label(RichText::new("Staging and commits currently use the Git CLI. Interactive authentication needs an external terminal.").small().color(theme::MUTED));
                    } else {
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut self.explorer_view, ExplorerView::Files, "Files");
                        ui.selectable_value(
                            &mut self.explorer_view,
                            ExplorerView::Outline,
                            "Symbols",
                        );
                    });

                    if !self.settings.projects.is_empty() {
                        let selected = self.selected_project();
                        egui::ComboBox::from_id_salt("active-project")
                            .selected_text(
                                selected
                                    .as_deref()
                                    .map(|p|format!("{} · {}",p.file_stem().unwrap_or_default().to_string_lossy(),p.parent().and_then(|p|p.file_name()).unwrap_or_default().to_string_lossy())).unwrap_or_default(),
                            )
                            .width(ui.available_width().max(120.0))
                            .show_ui(ui, |ui| {
                                for path in self.settings.projects.clone() {
                                    let label = path.display().to_string();
                                    if ui
                                        .selectable_label(selected.as_ref() == Some(&path), label)
                                        .clicked()
                                    {
                                        self.settings.selected_project = Some(path);
                                        self.persist();
                                        self.request_diagnostic_scan();
                                    }
                                }
                            });
                    }
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .id_salt("explorer-scroll")
                        .show(ui, |ui| match self.explorer_view {
                            ExplorerView::Files => {
                                if self.settings.folders.is_empty() {
                                    ui.label(
                                        RichText::new(
                                            "Use File > Open Folder to browse your workspace",
                                        )
                                        .color(theme::MUTED),
                                    );
                                }
                                for folder in self.settings.folders.clone() {
                                    egui::CollapsingHeader::new(format!(
                                        "{}",
                                        folder.file_name().unwrap_or_default().to_string_lossy()
                                    ))
                                    .id_salt((&folder, self.tree_revision))
                                    .default_open(self.tree_default_open || self.tree_revision == 0)
                                    .show(ui, |ui| self.tree_ui(ui, &folder, 0))
                                    .header_response.on_hover_text(folder.display().to_string()).context_menu(|ui|{
                                        if ui.button("Remove folder from workspace").clicked(){self.settings.folders.retain(|p|p!=&folder);if self.settings.command_folder.as_ref()==Some(&folder){self.settings.command_folder=None;}self.workspace_requested=true;self.persist();ui.close_menu();}
                                        if ui.button("Use for commands and Git").clicked(){self.settings.command_folder=Some(folder.clone());self.persist();ui.close_menu();}
                                    });
                                }
                            }
                            ExplorerView::Outline => {
                                if self.settings.projects.is_empty() {
                                    ui.label(
                                        RichText::new(
                                            "Open a .hyproj to see namespaces and classes",
                                        )
                                        .color(theme::MUTED),
                                    );
                                }
                                for project in self.workspace.projects.clone() { self.project_ui(ui,&project); }
                            }
                        });
                    if self.workspace_rx.is_some(){ui.horizontal(|ui|{ui.spinner();ui.label("Indexing workspace…");});}
                    ui.label(RichText::new(format!("{} indexed files · generated folders {}",self.workspace.files.len(),if self.settings.include_generated {"included"}else{"hidden"})).small().color(theme::MUTED));
                    if !self.workspace.notices.is_empty(){egui::CollapsingHeader::new(format!("Index notices ({})",self.workspace.notices.len())).show(ui,|ui|{for notice in &self.workspace.notices{ui.colored_label(theme::WARM,notice);}if ui.button("Increase index limit and rescan").clicked(){self.settings.index_limit=self.settings.index_limit.saturating_mul(2).min(1_000_000);self.workspace_requested=true;self.persist();}});}
                    if ui.small_button("Manage workspace…").clicked(){self.show_settings=true;}
                    }
                });
        }
        if !self.notices.is_empty() {
            egui::TopBottomPanel::top("notifications").show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(theme::WARM, &self.notices[0]);
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
            .frame(egui::Frame::new().fill(theme::EDITOR).inner_margin(8))
            .show(ctx, |ui| {
                self.caret_position = None;
                egui::ScrollArea::horizontal()
                    .id_salt("editor-tabs")
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 3.0;
                            let mut close = None;
                            if (self.active.is_none() || self.docs.is_empty())
                                && ui
                                    .selectable_label(self.active.is_none(), "Welcome")
                                    .clicked()
                            {
                                self.active = None;
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
                        .color(theme::MUTED),
                    );
                }
                if self.show_search {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Find").small().color(theme::MUTED));
                        let response = ui.add(
                            TextEdit::singleline(&mut self.search)
                                .desired_width(200.0)
                                .hint_text("Find text..."),
                        );
                        if self.find_focus {
                            response.request_focus();
                            self.find_focus = false;
                        }
                        if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                            self.next_match(ctx, false);
                        }
                        if ui
                            .small_button("Prev")
                            .on_hover_text("Previous · Shift+F3")
                            .clicked()
                        {
                            self.next_match(ctx, true);
                        }
                        if ui.small_button("Next").on_hover_text("Next · F3").clicked() {
                            self.next_match(ctx, false);
                        }
                        let count = if self.search.is_empty() {
                            0
                        } else {
                            self.active
                                .and_then(|i| self.docs.get(i))
                                .map_or(0, |d| d.text.matches(&self.search).count())
                        };
                        ui.label(
                            RichText::new(format!("{count} found"))
                                .small()
                                .color(theme::MUTED),
                        );
                        if ui.small_button("Replace").clicked() {
                            self.show_replace = !self.show_replace;
                        }
                        if ui.small_button("×").on_hover_text("Close find").clicked() {
                            self.show_search = false;
                        }
                    });
                    if self.show_replace {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("With").small().color(theme::MUTED));
                            ui.add(
                                TextEdit::singleline(&mut self.replace_with)
                                    .desired_width(200.0)
                                    .hint_text("Replacement text..."),
                            );
                            if ui
                                .small_button("One")
                                .on_hover_text("Replace next match")
                                .clicked()
                            {
                                self.replace_next(ctx);
                            }
                            if ui
                                .small_button("All")
                                .on_hover_text("Replace all matches")
                                .clicked()
                            {
                                self.replace_all(ctx);
                            }
                        });
                    }
                }
                ui.separator();
                if let Some(i) = self.active {
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
                                                diagnostics::Severity::Warning => theme::WARM,
                                                diagnostics::Severity::Hint => theme::MUTED,
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
                                painter.rect_filled(gutter_rect, 0.0, theme::SURFACE);
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
                                            theme::MUTED,
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
                                                    ("!", theme::WARM)
                                                }
                                                diagnostics::Severity::Hint => ("o", theme::MUTED),
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
                                    if (was_manual
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
        self.draw_completion(ctx);
        if self.show_plugins {
            let mut open = true;
            egui::Window::new("Quark plugins").open(&mut open).default_width(500.0).show(ctx,|ui|{
                ui.label("Plugins are user-installed external tools. Only enable plugins you trust.");
                if let Some(root)=plugins::directory(settings_file().as_deref()) {
                    ui.label(RichText::new(root.display().to_string()).monospace().small());
                    ui.horizontal(|ui|{
                        if ui.button("Create plugins directory").clicked(){
                            match fs::create_dir_all(&root){Ok(())=>self.status="Plugins directory ready".into(),Err(e)=>self.status=format!("Cannot create directory: {e}")}
                        }
                        if ui.button("Copy directory path").clicked(){ui.ctx().copy_text(root.to_string_lossy().into_owned());}
                        if ui.button("Reload").clicked(){self.reload_plugins();}
                    });
                }
                ui.separator();
                if self.plugins.is_empty(){ui.label("No plugins installed. Place a plugin.json under a plugin subdirectory.");}
                for plugin in self.plugins.clone(){
                    let mut enabled=!self.settings.disabled_plugins.contains(&plugin.id);
                    if ui.checkbox(&mut enabled,format!("{} ({})",plugin.name,plugin.id)).changed(){
                        self.settings.disabled_plugins.retain(|x|x!=&plugin.id);
                        if !enabled {self.settings.disabled_plugins.push(plugin.id.clone());}
                        self.persist();
                    }
                    if !plugin.description.is_empty(){ui.label(RichText::new(&plugin.description).small().color(theme::MUTED));}
                    ui.label(RichText::new(format!("{} command(s)",plugin.commands.len())).small().color(theme::MUTED));
                }
                for warning in &self.plugin_warnings {ui.colored_label(theme::WARM,warning);}
            });
            if !open {
                self.show_plugins = false;
            }
        }
        self.palette_ui(ctx);
        if self.show_settings {
            let mut open = true;
            egui::Window::new("Settings").open(&mut open).default_width(620.0).vscroll(true).show(ctx,|ui|{
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
                ui.label(RichText::new("Enabled commands · review detected capabilities for custom wrappers").small().color(theme::MUTED));
                ui.horizontal(|ui|{for verb in ["build","compile","check","run"]{let mut enabled=self.settings.compiler_verbs.iter().any(|v|v==verb);if ui.checkbox(&mut enabled,verb).changed(){self.settings.compiler_verbs.retain(|v|v!=verb);if enabled{self.settings.compiler_verbs.push(verb.into());}}}});
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
                    ui.separator(); ui.colored_label(theme::WARM,"Persistence paused to preserve an unreadable file.");
                    if ui.button("Back up unreadable files and resume persistence").clicked(){self.reset_persistence();}
                }
                ui.separator(); ui.label(RichText::new("Changes apply immediately. Save settings to retain them after restart.").small().color(theme::MUTED));
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
                        .color(theme::MUTED),
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
                        ui.colored_label(theme::WARM, notice);
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
        self.safety_dialogs(ctx);
        ctx.request_repaint_after(Duration::from_millis(
            if self.job_rx.is_some() || self.workspace_rx.is_some() {
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
            .with_inner_size([1280., 800.]),
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
        theme::apply(&ctx);
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
