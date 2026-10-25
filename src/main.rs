mod command;
mod diagnostics;
mod editor;
mod hydrogen;
mod navigation;
mod plugins;
mod project;
mod symbols;
mod theme;

use editor::{completion_candidates, highlight_hydrogen, insert_completion, prefix_at};
use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, RichText, TextEdit};
use hydrogen::BuildScope;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    thread,
};

#[derive(Clone)]
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
            .unwrap_or_else(|| "Untitled".into());
        if self.text != self.saved {
            format!("● {name}")
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
            terminal_visible: true,
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

struct Quark {
    next_doc_id: u64,
    outline_cache: HashMap<PathBuf, symbols::SymbolNode>,
    explorer_view: ExplorerView,
    tree_revision: u64,
    tree_default_open: bool,
    plugins: Vec<plugins::Plugin>,
    plugin_warnings: Vec<String>,
    show_plugins: bool,
    completion: Option<CompletionPopup>,
    dismissed_completion: Option<(u64, usize, String)>,
    manual_completion: bool,
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
    job_rx: Option<Receiver<String>>,
    job_done: bool,
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
        let settings = settings_file()
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
            .unwrap_or_default();
        let (plugin_list, plugin_warnings) = plugins::directory(settings_file().as_deref())
            .map(|dir| plugins::load_all(&dir))
            .unwrap_or_default();
        let mut q = Self {
            settings,
            next_doc_id: 1,
            outline_cache: HashMap::new(),
            explorer_view: ExplorerView::Files,
            tree_revision: 0,
            tree_default_open: false,
            plugins: plugin_list,
            plugin_warnings,
            show_plugins: false,
            completion: None,
            dismissed_completion: None,
            manual_completion: false,
            docs: vec![],
            active: None,
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
            job_done: false,
            status: "Ready".into(),
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
                .filter_map(|d| d.path.as_ref().map(|p| (p.clone(), d.text.clone())))
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

    fn persist(&self) {
        if let Some(path) = settings_file() {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Ok(s) = serde_json::to_string_pretty(&self.settings) {
                let _ = fs::write(path, s);
            }
        }
    }
    fn add_folder(&mut self, path: PathBuf) {
        let path = path.canonicalize().unwrap_or(path);
        if !self.settings.folders.contains(&path) {
            self.settings.folders.push(path);
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
        self.outline_cache.clear();
        self.open(path);
        self.request_diagnostic_scan();
    }
    fn open(&mut self, path: PathBuf) {
        let path = path.canonicalize().unwrap_or(path);
        if path.extension().and_then(|e| e.to_str()) == Some("hyproj")
            && !self.settings.projects.contains(&path)
        {
            self.settings.projects.push(path.clone());
            self.settings.selected_project = Some(path.clone());
            self.persist();
        }
        if let Some(idx) = self
            .docs
            .iter()
            .position(|d| d.path.as_ref() == Some(&path))
        {
            self.active = Some(idx);
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
                self.request_diagnostic_scan();
            }
            Err(e) => self.status = format!("Open failed: {e}"),
        }
    }
    fn save(&mut self, idx: usize) -> bool {
        if idx >= self.docs.len() {
            return false;
        }
        let path = if let Some(path) = self.docs[idx].path.clone() {
            path
        } else if let Some(path) = rfd::FileDialog::new()
            .add_filter("Hydrogen", &["hy", "hyproj"])
            .save_file()
        {
            path
        } else {
            return false;
        };
        match fs::write(&path, &self.docs[idx].text) {
            Ok(_) => {
                self.docs[idx].path = Some(path);
                self.docs[idx].saved = self.docs[idx].text.clone();
                self.outline_cache.clear();
                self.status = "Saved".into();
                self.request_diagnostic_scan();
                true
            }
            Err(e) => {
                self.status = format!("Save failed: {e}");
                false
            }
        }
    }
    fn save_all(&mut self) -> bool {
        for i in 0..self.docs.len() {
            if self.docs[i].text != self.docs[i].saved && !self.save(i) {
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
            .or_else(|| {
                // Prefer an open manifest if no explicit project has been selected.
                self.active
                    .and_then(|i| self.docs.get(i))
                    .and_then(|d| d.path.as_ref())
                    .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("hyproj"))
                    .cloned()
            })
            .or_else(|| self.settings.projects.first().cloned())
    }
    fn build(&mut self, scope: BuildScope) {
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
        self.current_path()
            .and_then(|p| p.parent().map(Path::to_path_buf))
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
        self.output
            .push_str(&format!("\n$ {} {}\n", program, args.join(" ")));
        let (tx, rx) = mpsc::channel();
        self.job_rx = Some(rx);
        self.job_done = false;
        thread::spawn(move || command::run_streamed(&program, &args, &cwd, tx));
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
        self.launch(compiler, args, self.cwd());
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
    }
    fn shortcuts(&mut self, ctx: &egui::Context) {
        let cmd = Modifiers::COMMAND;
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::B))) {
            self.settings.sidebar_visible = !self.settings.sidebar_visible;
            self.persist();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F1)) {
            self.settings.terminal_visible = !self.settings.terminal_visible;
            self.persist();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::N))) {
            self.new_document();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::O))) {
            if let Some(p) = rfd::FileDialog::new().pick_file() {
                self.open(p);
            }
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::S))) {
            if let Some(i) = self.active {
                self.save(i);
            }
        }
        if ctx.input_mut(|i| {
            i.consume_shortcut(&KeyboardShortcut::new(cmd | Modifiers::SHIFT, Key::S))
        }) {
            self.save_all();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::F))) {
            self.show_search = true;
            self.find_focus = true;
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::H))) {
            self.show_search = true;
            self.show_replace = true;
            self.find_focus = true;
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::G))) {
            self.show_go_to_line = true;
            self.go_focus = true;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F3)) {
            self.next_match(ctx, false);
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::F3)) {
            self.next_match(ctx, true);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::P))) {
            self.show_quick_open = true;
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::Space))) {
            self.manual_completion = true;
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::ALT, Key::Z))) {
            self.settings.word_wrap = !self.settings.word_wrap;
            self.persist();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::W))) {
            if let Some(i) = self.active {
                self.close(i);
            }
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::Tab))) {
            self.next_tab(false);
        }
        if ctx.input_mut(|i| {
            i.consume_shortcut(&KeyboardShortcut::new(cmd | Modifiers::SHIFT, Key::Tab))
        }) {
            self.next_tab(true);
        }
        if ctx.input_mut(|i| {
            i.consume_shortcut(&KeyboardShortcut::new(cmd | Modifiers::SHIFT, Key::D))
        }) && self
            .editor_id()
            .is_some_and(|id| ctx.memory(|m| m.has_focus(id)))
        {
            self.duplicate_active_line(ctx);
        }
        if ctx.input_mut(|i| {
            i.consume_shortcut(&KeyboardShortcut::new(
                Modifiers::CTRL | Modifiers::SHIFT,
                Key::B,
            ))
        }) {
            self.build(BuildScope::Project);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::NONE, Key::F5))) {
            self.hydrogen("run");
        }
        if self.completion.is_none()
            && (self.show_search || self.show_go_to_line)
            && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
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
            self.status = "Unsaved changes: save before closing the tab".into();
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
        self.completion = None;
    }
    fn draw_symbol_node(&mut self, ui: &mut egui::Ui, node: &symbols::SymbolNode, salt: &str) {
        for (name, child) in &node.branches {
            egui::CollapsingHeader::new(RichText::new(format!("◇ {name}")).color(theme::MUTED))
                .id_salt((salt, name, self.tree_revision))
                .default_open(self.tree_default_open)
                .show(ui, |ui| {
                    self.draw_symbol_node(ui, child, &format!("{salt}/{name}"));
                });
        }
        for symbol in &node.items {
            let label = if symbol.kind == "file" {
                format!("▤ {}", symbol.name)
            } else if symbol.kind == "missing" {
                symbol.name.clone()
            } else {
                format!(
                    "◈ {} ({})",
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
            RichText::new(format!("◉ {} [{}]", project.name, project.kind)).color(theme::ACCENT),
        )
        .id_salt((&project.path, self.tree_revision))
        .default_open(self.tree_default_open)
        .show(ui, |ui| {
            if ui.small_button("Open manifest").clicked() {
                self.open(project.path.clone());
            }
            for warning in &project.warnings {
                ui.colored_label(theme::WARM, format!("⚠ {warning}"));
            }
            let outline = self
                .outline_cache
                .entry(project.path.clone())
                .or_insert_with(|| symbols::build_outline(&project.sources))
                .clone();
            self.draw_symbol_node(ui, &outline, &project.path.to_string_lossy());
            for reference in &project.references {
                self.project_ui(ui, reference);
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
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
                self.dismissed_completion =
                    Some((popup.doc_id, popup.cursor_char, popup.prefix.clone()));
                self.completion = None;
                return;
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowDown)) {
                popup.selected = (popup.selected + 1) % popup.candidates.len();
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowUp)) {
                popup.selected =
                    (popup.selected + popup.candidates.len() - 1) % popup.candidates.len();
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter))
                || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Tab))
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
                        RichText::new("↑ ↓ select  ·  Enter / Tab insert  ·  Esc dismiss")
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
        if depth > 14 {
            return;
        }
        let Ok(entries) = fs::read_dir(path) else {
            return;
        };
        let mut paths: Vec<_> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                !matches!(
                    p.file_name().and_then(|s| s.to_str()),
                    Some(
                        ".git"
                            | ".quark"
                            | "target"
                            | "build"
                            | "node_modules"
                            | ".venv"
                            | "__pycache__"
                    )
                )
            })
            .collect();
        paths.sort_by_key(|p| {
            (
                !p.is_dir(),
                p.file_name().unwrap_or_default().to_os_string(),
            )
        });
        for path in paths.into_iter().take(350) {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if path.is_dir() {
                egui::CollapsingHeader::new(format!("▤ {name}"))
                    .id_salt((&path, self.tree_revision))
                    .default_open(self.tree_default_open)
                    .show(ui, |ui| self.tree_ui(ui, &path, depth + 1));
            } else {
                let selected = self
                    .active
                    .and_then(|i| self.docs.get(i))
                    .is_some_and(|d| d.path.as_deref() == Some(path.as_path()));
                if ui
                    .selectable_label(selected, name)
                    .on_hover_text(path.display().to_string())
                    .clicked()
                {
                    self.open(path);
                }
            }
        }
    }
    fn quick_paths(&self) -> Vec<PathBuf> {
        let mut out = vec![];
        let mut visited = HashSet::new();
        for root in &self.settings.folders {
            collect_files(root, &mut out, &mut visited, 0);
        }
        for p in &self.settings.projects {
            let mut sources = vec![];
            project::collect_sources(&project::load(p), &mut sources);
            out.extend(sources);
        }
        out.sort();
        out.dedup();
        out
    }
}
fn collect_files(root: &Path, out: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>, depth: usize) {
    if depth > 12 || out.len() > 5000 {
        return;
    }
    let path = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if !seen.insert(path.clone()) {
        return;
    }
    if let Ok(entries) = fs::read_dir(&path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if matches!(
                p.file_name().and_then(|s| s.to_str()),
                Some(".git" | ".quark" | "target" | "build" | "node_modules" | ".venv")
            ) {
                continue;
            }
            if p.is_dir() {
                collect_files(&p, out, seen, depth + 1);
            } else if matches!(
                p.extension().and_then(|s| s.to_str()),
                Some("hy" | "hyproj" | "toml" | "md" | "rs" | "json" | "txt")
            ) {
                out.push(p);
            }
        }
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
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(rx) = &self.job_rx {
            while let Ok(msg) = rx.try_recv() {
                if let Some(code) = msg
                    .strip_prefix("[Exit code: ")
                    .and_then(|s| s.split(']').next())
                    .and_then(|s| s.parse::<i32>().ok())
                {
                    self.status = if code == 0 {
                        "Command finished successfully".into()
                    } else {
                        format!("Command failed (exit code {code})")
                    };
                }
                if msg.starts_with("Failed to start ") {
                    self.status = "Command failed to start".into();
                }
                self.output.push_str(&msg);
            }
            if matches!(rx.try_recv(), Err(mpsc::TryRecvError::Disconnected)) {
                self.job_done = true;
            }
        }
        if self.job_done {
            self.job_rx = None;
            self.job_done = false;
        }
        self.shortcuts(ctx);
        self.completion_keys(ctx);
        self.diagnostic_tick(ctx);
        egui::TopBottomPanel::top("menu").show(ctx,|ui|{
            egui::menu::bar(ui,|ui|{
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(RichText::new("◈ QUARK").strong().color(theme::ACCENT));
                ui.separator();
                if ui.small_button("☰").on_hover_text("Toggle Explorer · Ctrl+B").clicked(){
                    self.settings.sidebar_visible = !self.settings.sidebar_visible;
            self.persist();
                }
                if ui.small_button("▣").on_hover_text("Toggle Terminal · F1").clicked(){
                    self.settings.terminal_visible = !self.settings.terminal_visible;
            self.persist();
                }
                ui.separator();
                ui.menu_button("File",|ui|{
                    if ui.button("New  Ctrl+N").clicked(){self.new_document();ui.close_menu();}
                    if ui.button("Open file  Ctrl+O").clicked(){if let Some(p)=rfd::FileDialog::new().pick_file(){self.open(p);}ui.close_menu();}
                    if ui.button("Add folder").clicked(){if let Some(p)=rfd::FileDialog::new().pick_folder(){self.add_folder(p);}ui.close_menu();}
                    if ui.button("Open .hyproj").clicked(){if let Some(p)=rfd::FileDialog::new().add_filter("Hydrogen project", &["hyproj"]).pick_file(){self.add_project(p);}ui.close_menu();}
                    if ui.button("Save  Ctrl+S").clicked(){if let Some(idx)=self.active {self.save(idx);}ui.close_menu();}
                    if ui.button("Save all · Ctrl+Shift+S").clicked(){self.save_all();ui.close_menu();}
                });
                ui.menu_button("Edit",|ui|{
                    if ui.button("Completion  Ctrl/Cmd+Space").clicked(){self.manual_completion=true;ui.close_menu();}
                    if ui.button("Find  Ctrl+F").clicked(){self.show_search=true;self.find_focus=true;ui.close_menu();}
                    if ui.button("Quick open  Ctrl+P").clicked(){self.show_quick_open=true;ui.close_menu();}
                    if ui.button("Find & replace · Ctrl+H").clicked(){self.show_search=true;self.show_replace=true;self.find_focus=true;ui.close_menu();}
                    if ui.button("Go to line · Ctrl+G").clicked(){self.show_go_to_line=true;self.go_focus=true;ui.close_menu();}
                    if ui.button("Duplicate line · Ctrl+Shift+D").clicked(){self.duplicate_active_line(ctx);ui.close_menu();}
                    if ui.button("Next tab · Ctrl+Tab").clicked(){self.next_tab(false);ui.close_menu();}
                });
                ui.menu_button("Hydrogen",|ui|{
                    if ui.button("Build active project  Ctrl+Shift+B").clicked(){self.build(BuildScope::Project);ui.close_menu();}
                    if ui.button("Compile current .hy file").clicked(){self.build(BuildScope::CurrentFile);ui.close_menu();}
                    ui.separator();
                    if ui.button("Check project").clicked(){self.hydrogen("check");ui.close_menu();}
                    if ui.button("Run (SDK)").clicked(){self.hydrogen("run");ui.close_menu();}
                    if ui.button("Compiler settings").clicked(){self.show_settings=true;ui.close_menu();}
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
                    changed |= ui.checkbox(&mut self.settings.terminal_visible,"Terminal  F1").changed();
                    changed |= ui.checkbox(&mut self.settings.word_wrap,"Word wrap  Alt+Z").changed();
                    if changed{self.persist();}
                });
                ui.separator();
                let build_button=ui.small_button(RichText::new("▶ Build").color(theme::ACCENT))
                    .on_hover_text("Build selected .hyproj; hold Shift when clicking to compile the active .hy file");
                if build_button.clicked() {
                    let scope=if ui.input(|i|i.modifiers.shift){BuildScope::CurrentFile}else{BuildScope::Project};
                    self.build(scope);
                }
                if ui.small_button("✓").on_hover_text("Check Hydrogen project").clicked(){self.hydrogen("check");}
                if ui.small_button("⌕").on_hover_text("Find in file · Ctrl+F").clicked(){self.show_search=true;self.find_focus=true;}
                if let Some(project)=self.selected_project() {
                    ui.separator();
                    ui.label(RichText::new(project.file_stem().unwrap_or_default().to_string_lossy()).small().color(theme::MUTED))
                        .on_hover_text(project.display().to_string());
                }

            });
        });
        egui::TopBottomPanel::bottom("status").show(ctx,|ui|{ui.horizontal(|ui|{
            ui.label(RichText::new("●").small().color(theme::ACCENT));
            ui.label(RichText::new(&self.status).small().color(theme::MUTED));
            let (errors, hints) = self.diagnostics.values().flatten().fold((0_usize,0_usize), |(e,h),d| {
                match d.severity {
                    diagnostics::Severity::Error => (e+1,h),
                    diagnostics::Severity::Hint => (e,h+1),
                    diagnostics::Severity::Warning => (e+1,h),
                }
            });
            if errors > 0 || hints > 0 {
                ui.separator();
                ui.label(RichText::new(format!("! {errors}  ◌ {hints}")).small().color(theme::MUTED))
                    .on_hover_text(format!("{errors} problems · {hints} possible unused declarations · {} scanned files{}",
                        self.files_scanned, if self.analysis_complete {""} else {" (analysis incomplete)"}));
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
                .min_height(80.0)
                .default_height(160.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("TERMINAL")
                                .strong()
                                .small()
                                .color(theme::ACCENT),
                        );
                        if self.job_rx.is_some() {
                            ui.spinner();
                        }
                        ui.label(
                            RichText::new(self.cwd().display().to_string())
                                .small()
                                .color(theme::MUTED),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .small_button("×")
                                .on_hover_text("Hide Terminal · F1")
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
                    let height = (ui.available_height() - 28.0).max(35.0);
                    egui::ScrollArea::vertical()
                        .stick_to_bottom(true)
                        .max_height(height)
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(RichText::new(&self.output).monospace()).wrap(),
                            );
                        });
                    ui.horizontal(|ui| {
                        let response = ui.add(
                            TextEdit::singleline(&mut self.command)
                                .desired_width(f32::INFINITY)
                                .hint_text("Command (runs in workspace folder)..."),
                        );
                        if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                            let c = std::mem::take(&mut self.command);
                            self.shell(c);
                            response.request_focus();
                        }
                    });
                });
        }
        if self.settings.sidebar_visible {
            egui::SidePanel::left("explorer")
                .resizable(true)
                .default_width(245.0)
                .min_width(165.0)
                .show(ctx, |ui| {
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("EXPLORER")
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
                                .on_hover_text("Refresh symbols")
                                .clicked()
                            {
                                self.outline_cache.clear();
                            }
                            if ui.small_button("⊕").on_hover_text("Expand all").clicked() {
                                self.tree_default_open = true;
                                self.tree_revision += 1;
                            }
                            if ui.small_button("⊖").on_hover_text("Collapse all").clicked() {
                                self.tree_default_open = false;
                                self.tree_revision += 1;
                            }
                        });
                    });
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
                                    .and_then(|p| p.file_stem())
                                    .unwrap_or_default()
                                    .to_string_lossy(),
                            )
                            .width(ui.available_width().max(120.0))
                            .show_ui(ui, |ui| {
                                for path in self.settings.projects.clone() {
                                    let label =
                                        path.file_stem().unwrap_or_default().to_string_lossy();
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
                                            "Use File → Add folder to browse your workspace",
                                        )
                                        .color(theme::MUTED),
                                    );
                                }
                                for folder in self.settings.folders.clone() {
                                    egui::CollapsingHeader::new(format!(
                                        "▤ {}",
                                        folder.file_name().unwrap_or_default().to_string_lossy()
                                    ))
                                    .id_salt((&folder, self.tree_revision))
                                    .default_open(self.tree_default_open || self.tree_revision == 0)
                                    .show(ui, |ui| self.tree_ui(ui, &folder, 0));
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
                                for path in self.settings.projects.clone() {
                                    self.project_ui(ui, &project::load(&path));
                                }
                            }
                        });
                });
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            self.caret_position = None;
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                let mut close = None;
                for (i, doc) in self.docs.iter().enumerate() {
                    if ui
                        .selectable_label(self.active == Some(i), doc.label())
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
                    .small_button("⌕")
                    .on_hover_text("Find · Ctrl+F")
                    .clicked()
                {
                    self.show_search = true;
                    self.find_focus = true;
                }
            });
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
                        .small_button("↑")
                        .on_hover_text("Previous · Shift+F3")
                        .clicked()
                    {
                        self.next_match(ctx, true);
                    }
                    if ui.small_button("↓").on_hover_text("Next · F3").clicked() {
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
                    .and_then(|d| d.path.as_ref())
                    .and_then(|p| self.diagnostics.get(p))
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
                            highlight_hydrogen(text, ui.visuals().text_color())
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
                                    ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                                        ui.set_min_width(viewport_width);
                                        TextEdit::multiline(&mut doc.text)
                                            .id(edit_id)
                                            .font(egui::TextStyle::Monospace)
                                            .code_editor()
                                            .desired_rows(25)
                                            .desired_width(viewport_width)
                                            .lock_focus(true)
                                            .layouter(&mut layouter)
                                            .show(ui)
                                    })
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
                                        } else if on_line
                                            .iter()
                                            .any(|d| d.severity == diagnostics::Severity::Warning)
                                        {
                                            diagnostics::Severity::Warning
                                        } else {
                                            diagnostics::Severity::Hint
                                        };
                                        let (symbol, colour) = match severity {
                                            diagnostics::Severity::Error => {
                                                ("!", ui.visuals().error_fg_color)
                                            }
                                            diagnostics::Severity::Warning => ("!", theme::WARM),
                                            diagnostics::Severity::Hint => ("●", theme::MUTED),
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
                                        .on_hover_ui(
                                            |ui| {
                                                for d in on_line {
                                                    ui.label(&d.message);
                                                }
                                            },
                                        );
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
                    if output.response.changed() {
                        self.status = "Modified".into();
                        self.diagnostics.clear();
                        self.diagnostics_revision = self.diagnostics_revision.wrapping_add(1);
                        self.diagnostics_requested = true;
                        self.diagnostics_due = ctx.input(|i| i.time) + 0.5;
                    }
                    self.caret_position = output
                        .cursor_range
                        .map(|range| navigation::line_col(&doc.text, range.primary.ccursor.index));
                    let mut next_popup = None;
                    let was_manual = self.manual_completion;
                    self.manual_completion = false;
                    if is_hydrogen && (output.response.has_focus() || was_manual) {
                        if let Some(range) = output.cursor_range {
                            if range.primary.ccursor.index == range.secondary.ccursor.index {
                                let cursor_char = range.primary.ccursor.index;
                                let prefix = prefix_at(&doc.text, cursor_char);
                                if (was_manual || prefix.chars().count() >= 2)
                                    && self.dismissed_completion.as_ref()
                                        != Some(&(doc_id, cursor_char, prefix.clone()))
                                {
                                    let candidates: Vec<_> =
                                        completion_candidates(&doc.text, &prefix)
                                            .into_iter()
                                            .take(8)
                                            .collect();
                                    if !candidates.is_empty() {
                                        let caret = output.galley.pos_from_cursor(&range.primary);
                                        let screen = ctx.screen_rect();
                                        let expected_height =
                                            39.0 + 23.0 * candidates.len() as f32 + 25.0;
                                        let x = (output.galley_pos.x + caret.min.x).clamp(
                                            screen.left() + 4.0,
                                            (screen.right() - 280.0).max(screen.left() + 4.0),
                                        );
                                        let below = output.galley_pos.y + caret.max.y + 5.0;
                                        let y = if below + expected_height > screen.bottom() - 8.0 {
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
                                            .filter(|p| p.doc_id == doc_id && p.prefix == prefix)
                                            .map_or(0, |p| p.selected.min(candidates.len() - 1));
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
                ui.centered_and_justified(|ui| {
                    ui.heading(
                        RichText::new("◈ Quark IDE\nOpen a Hydrogen file or project to begin")
                            .color(theme::ACCENT),
                    );
                });
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
        if self.show_settings {
            let mut open = true;
            egui::Window::new("Hydrogen compiler")
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label("Command or absolute path (e.g. hy, hydrogen-stage1):");
                    ui.text_edit_singleline(&mut self.settings.compiler);
                    if ui.button("Save").clicked() {
                        self.persist();
                        self.show_settings = false;
                    }
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
            egui::Window::new("Quick open")
                .open(&mut open)
                .default_width(580.)
                .show(ctx, |ui| {
                    ui.text_edit_singleline(&mut self.quick_query);
                    egui::ScrollArea::vertical()
                        .max_height(360.)
                        .show(ui, |ui| {
                            let query = self.quick_query.to_lowercase();
                            let matches: Vec<_> = self
                                .quick_paths()
                                .into_iter()
                                .filter(|p| p.to_string_lossy().to_lowercase().contains(&query))
                                .take(60)
                                .collect();
                            for p in matches {
                                if ui.button(p.display().to_string()).clicked() {
                                    self.open(p);
                                    self.show_quick_open = false;
                                    break;
                                }
                            }
                        });
                });
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
                    if ui.button("Choose destination and clone").clicked() {
                        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                            self.launch(
                                "git".into(),
                                vec!["clone".into(), self.clone_url.clone()],
                                dir.clone(),
                            );
                            self.add_folder(dir);
                            self.show_clone = false;
                        }
                    }
                });
            if !open {
                self.show_clone = false;
            }
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
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
    fn defaults_leave_panels_visible() {
        let settings = Settings::default();
        assert!(settings.sidebar_visible && settings.terminal_visible);
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
