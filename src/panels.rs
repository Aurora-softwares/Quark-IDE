//! Independent sidebar views and a bottom panel confined to the editor column.
use crate::*;

#[derive(Default)]
pub(crate) struct SearchResults {
    pub generation: u64,
    pub query: String,
    pub hits: Vec<SearchHit>,
    pub notices: Vec<String>,
    pub scanned: usize,
}
pub(crate) struct SearchHit {
    pub path: PathBuf,
    pub line: usize,
    pub preview: String,
}
pub(crate) fn search_files(
    generation: u64,
    query: String,
    paths: Vec<PathBuf>,
    buffers: Vec<(PathBuf, String)>,
) -> SearchResults {
    let mut result = SearchResults {
        generation,
        query: query.clone(),
        ..Default::default()
    };
    if query.is_empty() {
        return result;
    }
    let mut paths = paths;
    paths.extend(buffers.iter().map(|(p, _)| p.clone()));
    paths.sort();
    paths.dedup();
    let mut skipped = 0;
    for path in paths {
        let source = if let Some((_, text)) = buffers.iter().find(|(p, _)| p == &path) {
            Some(text.clone())
        } else if fs::metadata(&path).is_ok_and(|m| m.len() <= 1024 * 1024) {
            fs::read_to_string(&path).ok()
        } else {
            None
        };
        let Some(source) = source else {
            skipped += 1;
            continue;
        };
        result.scanned += 1;
        for (i, line) in source.lines().enumerate() {
            if line.contains(&query) {
                result.hits.push(SearchHit {
                    path: path.clone(),
                    line: i + 1,
                    preview: line.trim().chars().take(160).collect(),
                });
                if result.hits.len() >= 500 {
                    result.notices.push(
                        "Showing the first 500 matching lines. Refine your search to see more."
                            .into(),
                    );
                    return result;
                }
            }
        }
    }
    if skipped > 0 {
        result.notices.push(format!(
            "Skipped {skipped} unreadable, binary, or over-1-MiB files."
        ));
    }
    result
}
impl Quark {
    pub(crate) fn show_sidebar(&mut self, view: SidebarView) {
        self.settings.sidebar_view = view;
        self.settings.sidebar_visible = true;
        if view == SidebarView::Search {
            self.find_focus = true;
        }
        self.persist();
    }
    pub(crate) fn poll_search(&mut self) {
        if let Some(result) = self.search_rx.as_ref().map(|rx| rx.try_recv()) {
            match result {
                Ok(result) => {
                    if result.generation == self.search_generation {
                        self.search_results = result;
                    }
                    self.search_rx = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.search_rx = None;
                    self.notice("Workspace search stopped unexpectedly".into());
                }
                Err(_) => {}
            }
        }
    }
    fn start_search(&mut self) {
        self.search_generation = self.search_generation.wrapping_add(1);
        let generation = self.search_generation;
        let query = self.search.clone();
        let paths = self.workspace.files.clone();
        let buffers = self
            .docs
            .iter()
            .filter_map(|d| d.path.clone().map(|p| (p, d.text.clone())))
            .collect();
        let (tx, rx) = mpsc::channel();
        self.search_rx = Some(rx);
        thread::spawn(move || {
            let _ = tx.send(search_files(generation, query, paths, buffers));
        });
    }
    pub(crate) fn sidebar_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for (icon, tip, view) in [
                (ui::Icon::Files, "Explorer", SidebarView::Explorer),
                (ui::Icon::Search, "Search", SidebarView::Search),
                (ui::Icon::Git, "Source Control", SidebarView::SourceControl),
                (ui::Icon::Extensions, "Extensions", SidebarView::Plugins),
                (ui::Icon::Themes, "Themes", SidebarView::Themes),
            ] {
                if ui::activity_button(ui, icon, tip, self.settings.sidebar_view == view).clicked()
                {
                    self.show_sidebar(view);
                }
            }
            if ui::activity_button(ui, ui::Icon::Settings, "Settings", false).clicked() {
                self.show_settings = true;
            }
        });
        ui.separator();
        let title = match self.settings.sidebar_view {
            SidebarView::Explorer => "SOLUTION EXPLORER",
            SidebarView::Search => "SEARCH",
            SidebarView::SourceControl => "SOURCE CONTROL",
            SidebarView::Plugins => "EXTENSIONS",
            SidebarView::Themes => "THEMES",
        };
        ui.label(
            RichText::new(title)
                .strong()
                .small()
                .color(theme::colors(ui.ctx()).muted.color()),
        );
        egui::ScrollArea::vertical()
            .id_salt(("sidebar-body", self.settings.sidebar_view as u8))
            .auto_shrink([false, false])
            .show(ui, |ui| match self.settings.sidebar_view {
                SidebarView::Explorer => self.explorer_ui(ui),
                SidebarView::Search => self.search_ui(ui),
                SidebarView::SourceControl => self.git_ui(ui),
                SidebarView::Plugins => self.extensions_catalog_ui(ui),
                SidebarView::Themes => self.themes_catalog_ui(ui),
            });
    }
    fn explorer_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui::view_tab(ui, "Solution", self.explorer_view == ExplorerView::Files).clicked() {
                self.explorer_view = ExplorerView::Files;
            }
            if ui::view_tab(
                ui,
                "Class View",
                self.explorer_view == ExplorerView::Outline,
            )
            .clicked()
            {
                self.explorer_view = ExplorerView::Outline;
            }
            if ui
                .small_button("New UI")
                .on_hover_text("Add a visual UI layout to the selected project")
                .clicked()
            {
                self.show_new_layout = true;
            }
            if ui.small_button("Refresh").clicked() {
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
        if !self.settings.projects.is_empty() {
            let selected = self.selected_project();
            egui::ComboBox::from_id_salt("active-project")
                .selected_text(
                    selected
                        .as_ref()
                        .and_then(|p| p.file_stem())
                        .unwrap_or_default()
                        .to_string_lossy(),
                )
                .width(ui.available_width() - 10.0)
                .show_ui(ui, |ui| {
                    for path in self.settings.projects.clone() {
                        if ui
                            .selectable_label(
                                selected.as_ref() == Some(&path),
                                path.display().to_string(),
                            )
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
        match self.explorer_view {
            ExplorerView::Files => {
                if self.settings.folders.is_empty() {
                    ui.label("Open a folder to browse your workspace.");
                    if ui.button("Open Folder…").clicked() {
                        if let Some(p) = rfd::FileDialog::new().pick_folder() {
                            self.add_folder(p);
                        }
                    }
                }
                for folder in self.settings.folders.clone() {
                    egui::CollapsingHeader::new(
                        folder.file_name().unwrap_or_default().to_string_lossy(),
                    )
                    .id_salt((&folder, self.tree_revision))
                    .default_open(self.tree_default_open || self.tree_revision == 0)
                    .show(ui, |ui| self.tree_ui(ui, &folder, 0))
                    .header_response
                    .on_hover_text(folder.display().to_string())
                    .context_menu(|ui| {
                        if ui.button("Remove folder from workspace").clicked() {
                            self.settings.folders.retain(|p| p != &folder);
                            if self.settings.command_folder.as_ref() == Some(&folder) {
                                self.settings.command_folder = None;
                            }
                            self.workspace_requested = true;
                            self.persist();
                            ui.close_menu();
                        }
                        if ui.button("Use for new terminals and Git").clicked() {
                            self.settings.command_folder = Some(folder.clone());
                            self.persist();
                            ui.close_menu();
                        }
                    });
                }
            }
            ExplorerView::Outline => {
                if self.settings.projects.is_empty() {
                    ui.label("Open a .hyproj to see namespaces and classes.");
                }
                for project in self.workspace.projects.clone() {
                    self.project_ui(ui, &project);
                }
            }
        }
        if self.workspace_rx.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Indexing workspace…");
            });
        }
        ui.add_space(10.0);
        ui.label(
            RichText::new(format!("{} indexed files", self.workspace.files.len()))
                .small()
                .color(theme::colors(ui.ctx()).muted.color()),
        );
        ui.label(
            RichText::new(if self.settings.include_generated {
                "Generated folders included"
            } else {
                "Generated folders hidden"
            })
            .small()
            .color(theme::colors(ui.ctx()).muted.color()),
        );
        if !self.workspace.notices.is_empty() {
            egui::CollapsingHeader::new(format!(
                "Index notices ({})",
                self.workspace.notices.len()
            ))
            .show(ui, |ui| {
                for notice in &self.workspace.notices {
                    ui.colored_label(theme::colors(ui.ctx()).warning.color(), notice);
                }
                if ui.button("Increase limit and rescan").clicked() {
                    self.settings.index_limit =
                        self.settings.index_limit.saturating_mul(2).min(1_000_000);
                    self.workspace_requested = true;
                    self.persist();
                }
            });
        }
        if ui.small_button("Manage workspace…").clicked() {
            self.show_settings = true;
        }
    }
    fn search_ui(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new(if self.workspace_search {
                "Find in files"
            } else {
                "Find in active editor"
            })
            .strong(),
        );
        let response = ui.add(
            TextEdit::singleline(&mut self.search)
                .desired_width(f32::INFINITY)
                .hint_text("Find text…"),
        );
        if self.find_focus {
            response.request_focus();
            self.find_focus = false;
        }
        // Selecting a match focuses the editor before it renders below. Consume
        // Enter here so it cannot replace that selection with a newline.
        let enter = response.lost_focus() && take_key(ui.ctx(), Modifiers::NONE, Key::Enter);
        ui.checkbox(&mut self.workspace_search, "Search workspace");
        if self.workspace_search {
            ui.label(
                RichText::new("Case-sensitive · Enter to search indexed text files")
                    .small()
                    .color(theme::colors(ui.ctx()).muted.color()),
            );
            if (ui.button("Search workspace").clicked() || enter) && !self.search.is_empty() {
                self.start_search();
            }
            if self.search_rx.is_some() {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Searching…");
                });
            }
            ui.label(format!(
                "{} matching lines · {} files",
                self.search_results.hits.len(),
                self.search_results.scanned
            ));
            if self.search_results.query != self.search && !self.search_results.hits.is_empty() {
                ui.label("Results are from the previous query. Press Enter to search again.");
            }
            let mut chosen = None;
            for hit in &self.search_results.hits {
                if ui
                    .selectable_label(
                        false,
                        format!(
                            "{}:{}",
                            hit.path.file_name().unwrap_or_default().to_string_lossy(),
                            hit.line
                        ),
                    )
                    .on_hover_text(hit.path.display().to_string())
                    .clicked()
                {
                    chosen = Some((hit.path.clone(), hit.line));
                }
                ui.label(
                    RichText::new(&hit.preview)
                        .small()
                        .color(theme::colors(ui.ctx()).muted.color()),
                );
            }
            if let Some((path, line)) = chosen {
                self.open(path.clone());
                self.pending_line = Some((path, line));
            }
            for notice in &self.search_results.notices {
                ui.colored_label(theme::colors(ui.ctx()).warning.color(), notice);
            }
        } else {
            let count = if self.search.is_empty() {
                0
            } else {
                self.active
                    .and_then(|i| self.docs.get(i))
                    .map_or(0, |d| d.text.matches(&self.search).count())
            };
            ui.label(format!("{count} matches in active file"));
            ui.horizontal(|ui| {
                if ui.button("Previous").clicked() {
                    self.next_match(ui.ctx(), true);
                }
                if ui.button("Next").clicked() || enter {
                    self.next_match(ui.ctx(), false);
                }
            });
            ui.checkbox(&mut self.show_replace, "Replace");
            if self.show_replace {
                ui.add(
                    TextEdit::singleline(&mut self.replace_with)
                        .desired_width(f32::INFINITY)
                        .hint_text("Replace with…"),
                );
                ui.horizontal(|ui| {
                    if ui.button("Replace one").clicked() {
                        self.replace_next(ui.ctx());
                    }
                    if ui.button("Replace all").clicked() {
                        self.replace_all(ui.ctx());
                    }
                });
            }
        }
    }
    fn git_ui(&mut self, ui: &mut egui::Ui) {
        let cwd = self.cwd();
        ui.label(RichText::new(cwd.file_name().unwrap_or_default().to_string_lossy()).strong());
        ui.label(
            RichText::new(cwd.display().to_string())
                .small()
                .color(theme::colors(ui.ctx()).muted.color()),
        );
        ui.add_space(5.0);
        ui.horizontal(|ui| {
            for (title, args) in [
                ("Status", vec!["status", "--short", "--branch"]),
                ("Diff", vec!["diff"]),
            ] {
                let width = (ui.available_width() / if title == "Status" { 2.0 } else { 1.0 }
                    - 3.0)
                    .max(40.0);
                if ui
                    .add_enabled(
                        self.job_rx.is_none(),
                        egui::Button::new(title).min_size(egui::vec2(width, 26.0)),
                    )
                    .clicked()
                {
                    self.git(&args);
                }
            }
        });
        ui.separator();
        ui.label(
            RichText::new("REMOTE")
                .small()
                .color(theme::colors(ui.ctx()).muted.color()),
        );
        ui.horizontal(|ui| {
            for (title, args) in [
                ("Fetch", vec!["fetch", "--all"]),
                ("Pull", vec!["pull", "--no-edit"]),
                ("Push", vec!["push"]),
            ] {
                if ui
                    .add_enabled(self.job_rx.is_none(), egui::Button::new(title))
                    .clicked()
                {
                    self.git(&args);
                }
            }
        });
        ui.add_space(6.0);
        ui.label("Git uses your OS credentials. Results appear in Output.");
        if ui.button("Choose Git / new terminal folder…").clicked() {
            if let Some(p) = rfd::FileDialog::new().pick_folder() {
                self.settings.command_folder = Some(p);
                self.persist();
            }
        }
        ui.label(
            RichText::new(
                "Use the Terminal tab for interactive Git commands, staging and commits.",
            )
            .small()
            .color(theme::colors(ui.ctx()).muted.color()),
        );
    }
    pub(crate) fn import_theme(&mut self, path: &Path, ctx: &egui::Context) {
        match fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|json| theme::parse(&json))
        {
            Ok(definition) => {
                let id = self.settings.catalog.install_theme(definition);
                self.enable_theme(&id, false);
                theme::apply(ctx, &self.effective_theme());
            }
            Err(error) => self.notice(format!("Theme import failed: {error}")),
        }
    }
    pub(crate) fn export_theme(&mut self, path: &Path, definition: &theme::Definition) {
        let result = serde_json::to_string_pretty(definition)
            .map_err(|e| e.to_string())
            .and_then(|json| {
                theme::parse(&json)?;
                storage::atomic_write(path, json.as_bytes()).map_err(|e| e.to_string())
            });
        if let Err(error) = result {
            self.notice(format!("Theme export failed: {error}"));
        }
    }
    pub(crate) fn bottom_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui|{
            for(tab,label)in[(BottomTab::Terminal,"Terminal"),(BottomTab::Output,"Output"),(BottomTab::Problems,"Problems")]{if ui.selectable_label(self.settings.bottom_tab==tab,label).clicked(){self.settings.bottom_tab=tab;self.persist();if tab==BottomTab::Terminal{if let Some(t)=&mut self.terminal{t.request_focus();}}}}
            if self.settings.bottom_tab==BottomTab::Output&&self.job_rx.is_some(){ui.spinner();if ui.small_button("Stop job").clicked(){if let Some(flag)=&self.job_cancel{flag.store(true,Ordering::Relaxed);}}}
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                if ui.small_button("×").on_hover_text("Hide bottom panel · Ctrl+`").clicked(){self.settings.terminal_visible=false;self.persist();}
                match self.settings.bottom_tab{
                    BottomTab::Problems=>{if ui.small_button("Clear problems").clicked(){self.problems.clear();self.problem_output.clear();}},
                    BottomTab::Output=>{if ui.small_button("Clear").clicked(){self.output.clear();}
if ui.small_button("Copy log").clicked(){ui.ctx().copy_text(self.output.clone());}},
                    BottomTab::Terminal=>{if ui.small_button("New session").on_hover_text("End this shell and start a new one in the selected workspace directory").clicked(){self.terminal.take();self.terminal_error=None;}
if ui.small_button("End session").clicked(){if let Some(t)=&mut self.terminal{t.stop();}}},
                }
            });
        });
        match self.settings.bottom_tab {
            BottomTab::Problems => {
                let errors = self
                    .problems
                    .iter()
                    .filter(|p| p.severity == problems::Severity::Error)
                    .count();
                let warnings = self.problems.len() - errors;
                ui.label(format!("{errors} errors · {warnings} warnings"));
                let mut selected = None;
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    if self.problems.is_empty() {
                        ui.label("No file/line compiler diagnostics found in the latest command. See Output for other messages.");
                    }
                    for problem in &self.problems {
                        let severity = match problem.severity {
                            problems::Severity::Error => "Error",
                            problems::Severity::Warning => "Warning",
                        };
                        let location = format!("{}:{}{}", problem.path.display(), problem.line,
                            problem.column.map(|col| format!(":{col}")).unwrap_or_default());
                        if ui.selectable_label(false, format!("{severity} · {location} · {}", problem.message))
                            .on_hover_text("Open diagnostic location in editor").clicked() {
                            selected = Some((problem.path.clone(), problem.line));
                        }
                    }
                });
                if let Some((path, line)) = selected {
                    self.open(path.clone());
                    self.pending_line = Some((path.canonicalize().unwrap_or(path), line));
                }
            }
            BottomTab::Output => {
                ui.label(
                    RichText::new(
                        self.job_cwd
                            .clone()
                            .unwrap_or_else(|| self.cwd())
                            .display()
                            .to_string(),
                    )
                    .small()
                    .color(theme::colors(ui.ctx()).muted.color()),
                );
                let lines: Vec<_> = self.output.lines().collect();
                egui::ScrollArea::both()
                    .stick_to_bottom(true)
                    .auto_shrink([false, false])
                    .show_rows(ui, 17.0, lines.len(), |ui, rows| {
                        for row in rows {
                            ui.add(
                                egui::Label::new(RichText::new(lines[row]).monospace())
                                    .selectable(true)
                                    .extend(),
                            );
                        }
                    });
            }
            BottomTab::Terminal => {
                if self.terminal.is_none() && self.terminal_error.is_none() {
                    match terminal::Terminal::spawn(&self.cwd(), ui.ctx().clone()) {
                        Ok(t) => self.terminal = Some(t),
                        Err(e) => self.terminal_error = Some(e),
                    }
                }
                if let Some(error) = &self.terminal_error {
                    ui.colored_label(
                        theme::colors(ui.ctx()).error.color(),
                        format!("Could not open OS terminal: {error}"),
                    );
                    ui.label("Check the OS shell and folder. New session retries; Output is still available.");
                }
                if let Some(t) = &mut self.terminal {
                    ui.label(
                        RichText::new(format!("{} · started in {}", t.shell, t.cwd.display()))
                            .small()
                            .color(theme::colors(ui.ctx()).muted.color()),
                    );
                    if let Some(status) = &t.exited {
                        ui.label(status);
                    }
                    if let Some(error) = &t.error {
                        ui.colored_label(theme::colors(ui.ctx()).error.color(), error);
                    }
                    t.ui(ui);
                }
            }
        }
    }
}
pub(crate) fn color_row(ui: &mut egui::Ui, name: &str, color: &mut theme::Rgb) {
    ui.horizontal(|ui| {
        ui.color_edit_button_srgb(&mut color.0);
        ui.label(name);
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn workspace_search_uses_open_buffers_and_line_numbers() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.hy");
        fs::write(&p, "old").unwrap();
        let r = search_files(
            4,
            "needle".into(),
            vec![p.clone()],
            vec![(p, "first\nneedle\n".into())],
        );
        assert_eq!(r.generation, 4);
        assert_eq!(r.hits[0].line, 2);
    }
    #[test]
    fn search_limits_are_visible() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.txt");
        fs::write(&p, "match\n".repeat(600)).unwrap();
        let r = search_files(1, "match".into(), vec![p], vec![]);
        assert_eq!(r.hits.len(), 500);
        assert!(!r.notices.is_empty());
    }

    #[test]
    fn workspace_search_is_case_sensitive_and_deduplicates_paths() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.hy");
        fs::write(&p, "Needle\nneedle\n").unwrap();
        let result = search_files(2, "needle".into(), vec![p.clone(), p.clone()], vec![]);
        assert_eq!(result.scanned, 1);
        assert_eq!(result.hits.len(), 1);
        assert_eq!(result.hits[0].line, 2);
        assert_eq!(result.generation, 2);
    }
    #[test]
    fn workspace_search_skips_binary_missing_and_oversized_files_with_notice() {
        let dir = tempfile::tempdir().unwrap();
        let binary = dir.path().join("binary");
        let large = dir.path().join("large");
        fs::write(&binary, [255]).unwrap();
        fs::write(&large, vec![b'x'; 1024 * 1024 + 1]).unwrap();
        let result = search_files(
            1,
            "x".into(),
            vec![binary, large, dir.path().join("missing")],
            vec![],
        );
        assert!(result.hits.is_empty());
        assert_eq!(result.scanned, 0);
        assert!(result.notices[0].contains("Skipped 3"));
    }
}
