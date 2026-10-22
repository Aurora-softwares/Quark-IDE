mod project;

use eframe::egui::{self, Color32, Key, KeyboardShortcut, Modifiers, RichText, TextEdit};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc::{self, Receiver},
    thread,
};

#[derive(Clone)]
struct Document {
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
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Settings {
    folders: Vec<PathBuf>,
    projects: Vec<PathBuf>,
    compiler: String,
}

struct Quark {
    settings: Settings,
    docs: Vec<Document>,
    active: Option<usize>,
    output: String,
    command: String,
    search: String,
    show_search: bool,
    show_settings: bool,
    show_quick_open: bool,
    show_terminal: bool,
    show_clone: bool,
    clone_url: String,
    job_rx: Option<Receiver<String>>,
    status: String,
}
impl Quark {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let settings = settings_file()
            .and_then(|p| fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str::<Settings>(&s).ok())
            .unwrap_or_default();
        let mut q = Self {
            settings,
            docs: vec![],
            active: None,
            output: "Quark IDE — native Rust edition\n".into(),
            command: String::new(),
            search: String::new(),
            show_search: false,
            show_settings: false,
            show_quick_open: false,
            show_terminal: true,
            show_clone: false,
            clone_url: String::new(),
            job_rx: None,
            status: "Ready".into(),
        };
        if q.settings.compiler.is_empty() {
            q.settings.compiler = "hy".into();
        }
        q
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
        if let Some(dir) = path.parent() {
            self.add_folder(dir.to_path_buf());
        }
        self.persist();
        self.open(path);
    }
    fn open(&mut self, path: PathBuf) {
        let path = path.canonicalize().unwrap_or(path);
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
                    path: Some(path),
                    saved: text.clone(),
                    text,
                });
                self.active = Some(self.docs.len() - 1);
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
                self.status = "Saved".into();
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
        self.show_terminal = true;
        self.output
            .push_str(&format!("\n$ {} {}\n", program, args.join(" ")));
        let (tx, rx) = mpsc::channel();
        self.job_rx = Some(rx);
        thread::spawn(move || {
            let result = Command::new(&program).args(&args).current_dir(cwd).output();
            let message = match result {
                Ok(result) => format!(
                    "{}{}\n[Exit: {}]\n",
                    String::from_utf8_lossy(&result.stdout),
                    String::from_utf8_lossy(&result.stderr),
                    result.status
                ),
                Err(e) => format!("Cannot start {program}: {e}\n"),
            };
            let _ = tx.send(message);
        });
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
        let Some(path) = self.current_path() else {
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
            args.extend([
                "-o".into(),
                path.with_extension(if cfg!(windows) { "exe" } else { "out" })
                    .to_string_lossy()
                    .into_owned(),
            ]);
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
    fn shortcuts(&mut self, ctx: &egui::Context) {
        let cmd = Modifiers::COMMAND;
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::N))) {
            self.docs.push(Document {
                path: None,
                text: String::new(),
                saved: String::new(),
            });
            self.active = Some(self.docs.len() - 1);
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
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::F))) {
            self.show_search = true;
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::P))) {
            self.show_quick_open = true;
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::W))) {
            if let Some(i) = self.active {
                self.close(i);
            }
        }
        if ctx.input_mut(|i| {
            i.consume_shortcut(&KeyboardShortcut::new(
                Modifiers::CTRL | Modifiers::SHIFT,
                Key::B,
            ))
        }) {
            self.hydrogen("build");
        }
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::NONE, Key::F5))) {
            self.hydrogen("run");
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
        self.active = if self.docs.is_empty() {
            None
        } else {
            Some(idx.min(self.docs.len() - 1))
        };
    }
    fn project_ui(&mut self, ui: &mut egui::Ui, project: &project::Project) {
        egui::CollapsingHeader::new(format!("{} [{}]", project.name, project.kind))
            .default_open(true)
            .show(ui, |ui| {
                if ui.small_button("Open manifest").clicked() {
                    self.open(project.path.clone());
                }
                for warning in &project.warnings {
                    ui.colored_label(Color32::YELLOW, format!("⚠ {warning}"));
                }
                for src in &project.sources {
                    let name = src.file_name().unwrap_or_default().to_string_lossy();
                    if ui
                        .selectable_label(
                            false,
                            format!("{} {}", if src.exists() { "◦" } else { "⚠" }, name),
                        )
                        .on_hover_text(src.display().to_string())
                        .clicked()
                    {
                        self.open(src.clone());
                    }
                }
                for reference in &project.references {
                    self.project_ui(ui, reference);
                }
            });
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
                    Some(".git" | "target" | "build" | "node_modules" | ".venv" | "__pycache__")
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
                egui::CollapsingHeader::new(format!("▸ {name}"))
                    .id_salt(&path)
                    .show(ui, |ui| self.tree_ui(ui, &path, depth + 1));
            } else if ui.selectable_label(false, name).clicked() {
                self.open(path);
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
                Some(".git" | "target" | "build" | "node_modules" | ".venv")
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
            if let Ok(msg) = rx.try_recv() {
                self.output.push_str(&msg);
                self.job_rx = None;
            }
        }
        self.shortcuts(ctx);
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New  Ctrl+N").clicked() {
                        self.docs.push(Document {
                            path: None,
                            text: String::new(),
                            saved: String::new(),
                        });
                        self.active = Some(self.docs.len() - 1);
                        ui.close_menu();
                    }
                    if ui.button("Open file  Ctrl+O").clicked() {
                        if let Some(p) = rfd::FileDialog::new().pick_file() {
                            self.open(p);
                        }
                        ui.close_menu();
                    }
                    if ui.button("Add folder").clicked() {
                        if let Some(p) = rfd::FileDialog::new().pick_folder() {
                            self.add_folder(p);
                        }
                        ui.close_menu();
                    }
                    if ui.button("Open .hyproj").clicked() {
                        if let Some(p) = rfd::FileDialog::new()
                            .add_filter("Hydrogen project", &["hyproj"])
                            .pick_file()
                        {
                            self.add_project(p);
                        }
                        ui.close_menu();
                    }
                    if ui.button("Save  Ctrl+S").clicked() {
                        if let Some(idx) = self.active {
                            self.save(idx);
                        }
                        ui.close_menu();
                    }
                    if ui.button("Save all").clicked() {
                        self.save_all();
                        ui.close_menu();
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if ui.button("Find  Ctrl+F").clicked() {
                        self.show_search = true;
                        ui.close_menu();
                    }
                    if ui.button("Quick open  Ctrl+P").clicked() {
                        self.show_quick_open = true;
                        ui.close_menu();
                    }
                });
                ui.menu_button("Hydrogen", |ui| {
                    for (name, verb) in [
                        ("Check", "check"),
                        ("Build", "build"),
                        ("Compile single file", "compile"),
                        ("Run (SDK)", "run"),
                    ] {
                        if ui.button(name).clicked() {
                            self.hydrogen(verb);
                            ui.close_menu();
                        }
                    }
                    if ui.button("Compiler settings").clicked() {
                        self.show_settings = true;
                        ui.close_menu();
                    }
                });
                ui.menu_button("Git", |ui| {
                    for (name, args) in [
                        ("Status", vec!["status", "--short", "--branch"]),
                        ("Diff", vec!["diff"]),
                        ("Fetch", vec!["fetch", "--all"]),
                        ("Pull", vec!["pull"]),
                        ("Push", vec!["push"]),
                    ] {
                        if ui.button(name).clicked() {
                            self.git(&args);
                            ui.close_menu();
                        }
                    }
                    if ui.button("Clone repository").clicked() {
                        self.show_clone = true;
                        ui.close_menu();
                    }
                });
                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut self.show_terminal, "Command terminal");
                });
                ui.separator();
                if ui.button("▶ Build").clicked() {
                    self.hydrogen("build");
                }
                if ui.button("✓ Check").clicked() {
                    self.hydrogen("check");
                }
                if ui.button("Git status").clicked() {
                    self.git(&["status", "--short", "--branch"]);
                }
            });
        });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
                ui.separator();
                ui.label("Hydrogen • UTF-8 • Quark IDE 0.2.1");
            });
        });
        if self.show_terminal {
            egui::TopBottomPanel::bottom("terminal")
                .resizable(true)
                .default_height(190.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.strong("COMMAND TERMINAL");
                        if ui.button("Clear").clicked() {
                            self.output.clear();
                        }
                        if self.job_rx.is_some() {
                            ui.spinner();
                        }
                        ui.label(self.cwd().display().to_string());
                    });
                    egui::ScrollArea::vertical()
                        .stick_to_bottom(true)
                        .max_height(260.0)
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
        egui::SidePanel::left("explorer")
            .resizable(true)
            .default_width(260.0)
            .show(ctx, |ui| {
                ui.heading("EXPLORER");
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.label(RichText::new("WORKSPACE FOLDERS").strong());
                    for folder in self.settings.folders.clone() {
                        egui::CollapsingHeader::new(
                            folder.file_name().unwrap_or_default().to_string_lossy(),
                        )
                        .id_salt(&folder)
                        .show(ui, |ui| self.tree_ui(ui, &folder, 0));
                    }
                    ui.separator();
                    ui.label(RichText::new("HYDROGEN PROJECT GRAPH").strong());
                    for path in self.settings.projects.clone() {
                        self.project_ui(ui, &project::load(&path));
                    }
                });
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
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
            });
            ui.separator();
            if let Some(i) = self.active {
                if let Some(doc) = self.docs.get_mut(i) {
                    let syntax = doc
                        .path
                        .as_ref()
                        .and_then(|p| p.extension())
                        .and_then(|s| s.to_str())
                        .unwrap_or("hy");
                    let language = if syntax == "hyproj" { "toml" } else { "rs" };
                    let mut layouter = |ui: &egui::Ui, text: &str, wrap_width: f32| {
                        let mut job = egui::text::LayoutJob::simple(
                            text.to_owned(),
                            egui::FontId::monospace(14.0),
                            ui.visuals().text_color(),
                            wrap_width,
                        );
                        job.wrap.max_width = wrap_width;
                        ui.fonts(|f| f.layout_job(job))
                    };
                    let _ = language;
                    ui.add_sized(
                        ui.available_size(),
                        TextEdit::multiline(&mut doc.text)
                            .font(egui::TextStyle::Monospace)
                            .code_editor()
                            .desired_rows(25)
                            .lock_focus(true)
                            .layouter(&mut layouter),
                    );
                }
            } else {
                ui.centered_and_justified(|ui| {
                    ui.heading("Quark IDE\nOpen a Hydrogen file or project to begin");
                });
            }
        });
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
        if self.show_search {
            let mut open = true;
            egui::Window::new("Find in file")
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.text_edit_singleline(&mut self.search);
                    if let Some(i) = self.active {
                        if !self.search.is_empty() {
                            let count = self.docs[i].text.matches(&self.search).count();
                            ui.label(format!("{count} matches (case-sensitive)"));
                        }
                    }
                });
            if !open {
                self.show_search = false;
            }
        }
        if self.show_quick_open {
            let mut open = true;
            egui::Window::new("Quick open")
                .open(&mut open)
                .default_width(580.)
                .show(ctx, |ui| {
                    ui.text_edit_singleline(&mut self.search);
                    egui::ScrollArea::vertical()
                        .max_height(360.)
                        .show(ui, |ui| {
                            let search = self.search.to_lowercase();

                            let matching_paths: Vec<_> = self
                                .quick_paths()
                                .into_iter()
                                .filter(|p| p.to_string_lossy().to_lowercase().contains(&search))
                                .take(60)
                                .collect();

                            for p in matching_paths {
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
