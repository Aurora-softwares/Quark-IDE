//! Welcome, command discovery, and explicit data-loss decisions.
use crate::*;
const ACTIONS: &[&str] = &[
    "File: New Hydrogen File",
    "File: Open File…",
    "File: Open Folder…",
    "File: Open Hydrogen Project…",
    "File: Save",
    "File: Save As…",
    "File: Save All",
    "File: Close Editor",
    "Go: Go to File…",
    "Hydrogen: Build Selected Project",
    "Hydrogen: Compile Current File",
    "Hydrogen: Check",
    "Hydrogen: Run",
    "View: Toggle Explorer",
    "View: Toggle Bottom Panel",
    "View: Welcome",
    "Preferences: Open Settings",
    "Extensions: Manage Extensions",
    "Workspace: Refresh Index",
    "Git: Status",
    "Git: Diff",
    "Command: Stop",
    "Edit: Find",
    "Edit: Replace",
    "Go: Go to Line",
    "Preferences: Themes",
    "View: Show Terminal",
    "View: Show Output",
    "File: Create New Project",
    "File: New UI Layout",
    "View: Start Window",
];
impl Quark {
    pub(crate) fn welcome(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add_space(28.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("QUARK")
                        .size(42.0)
                        .strong()
                        .color(theme::colors(ui.ctx()).accent.color()),
                );
                ui.label(RichText::new("Start Window").size(24.0).strong());
                ui.label(
                    RichText::new("Create, open, build and design Hydrogen applications")
                        .color(theme::colors(ui.ctx()).muted.color()),
                );
            });
            ui.add_space(24.0);
            ui.horizontal(|ui| {
                ui.add_space(20.0);
                if ui.available_width() > 650.0 {
                    ui.columns(2, |columns| {
                        self.welcome_start(&mut columns[0]);
                        self.welcome_guide(&mut columns[1]);
                    });
                } else {
                    ui.vertical(|ui| {
                        self.welcome_start(ui);
                        ui.add_space(18.0);
                        self.welcome_guide(ui);
                    });
                }
            });
        });
    }

    fn welcome_start(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new()
            .fill(theme::colors(ui.ctx()).surface.color())
            .corner_radius(8)
            .inner_margin(16)
            .show(ui, |ui| {
                ui.heading("Get started");
                ui.add_space(8.0);
                if ui.button("Create a new project…").clicked() {
                    self.start_project_dialog();
                }
                if ui.button("Open an existing project…").clicked() {
                    self.action(3, ui.ctx());
                }
                if ui.button("Open a folder…").clicked() {
                    self.action(2, ui.ctx());
                }
                if ui.button("New UI layout…").clicked() {
                    self.show_new_layout = true;
                }
                if ui.button("New File…").clicked() {
                    self.action(0, ui.ctx());
                }
            });
        ui.add_space(14.0);
        ui.heading("Recent projects and folders");
        ui.add_space(6.0);
        let recent = self.settings.recent_workspaces.clone().unwrap_or_default();
        if recent.is_empty() {
            ui.label(
                RichText::new("Your recently opened projects appear here.")
                    .color(theme::colors(ui.ctx()).muted.color()),
            );
        }
        for entry in recent.into_iter().take(12) {
            let path = &entry.path;
            ui.push_id((&entry.path, entry.project), |ui| {
                ui.horizontal(|ui| {
                    let title = if entry.project {
                        path.file_stem()
                    } else {
                        path.file_name()
                    }
                    .unwrap_or_default()
                    .to_string_lossy();
                    if ui
                        .link(title)
                        .on_hover_text(path.display().to_string())
                        .clicked()
                    {
                        if !path.exists() {
                            self.status = format!("Recent workspace not found: {}", path.display());
                        } else if entry.project {
                            self.add_project(path.clone());
                        } else {
                            self.add_folder(path.clone());
                            self.show_sidebar(SidebarView::Explorer);
                            self.tree_default_open = true;
                            self.tree_revision += 1;
                        }
                    }
                    if ui
                        .small_button("×")
                        .on_hover_text("Remove from recent projects")
                        .clicked()
                    {
                        self.remove_recent(&entry);
                    }
                });
                ui.label(
                    RichText::new(path.display().to_string())
                        .small()
                        .color(theme::colors(ui.ctx()).muted.color()),
                );
            });
        }
    }

    fn welcome_guide(&mut self, ui: &mut egui::Ui) {
        ui.heading("Development tools");
        ui.add_space(8.0);
        if (!self.settings.welcome_shortcuts_visible || !self.settings.welcome_build_visible)
            && ui.small_button("Restore hidden sections").clicked()
        {
            self.settings.welcome_shortcuts_visible = true;
            self.settings.welcome_build_visible = true;
            self.persist();
        }
        if self.settings.welcome_shortcuts_visible {
            egui::Frame::new()
                .fill(theme::colors(ui.ctx()).surface.color())
                .inner_margin(16)
                .corner_radius(5)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Coming from VS Code?").strong().size(16.0));
                        if ui
                            .small_button("×")
                            .on_hover_text("Hide shortcut guide")
                            .clicked()
                        {
                            self.settings.welcome_shortcuts_visible = false;
                            self.persist();
                        }
                    });
                    ui.add_space(8.0);
                    for (key, label) in [
                        ("Ctrl/Cmd+P", "Find a file"),
                        ("F1 / Ctrl/Cmd+Shift+P", "Run a command"),
                        ("Ctrl/Cmd+B", "Toggle Explorer"),
                        ("Ctrl+`", "Toggle Terminal / Output"),
                        ("Ctrl/Cmd+F", "Find in this file"),
                        ("Ctrl/Cmd+Space", "Show suggestions"),
                    ] {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new(key)
                                    .monospace()
                                    .color(theme::colors(ui.ctx()).accent.color()),
                            );
                            ui.label(label);
                        });
                    }
                });
        }
        if self.settings.welcome_build_visible {
            ui.add_space(14.0);
            egui::Frame::new()
                .fill(theme::colors(ui.ctx()).surface.color())
                .inner_margin(16)
                .corner_radius(5)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Your first build").strong().size(16.0));
                        if ui
                            .small_button("×")
                            .on_hover_text("Hide first-build guide")
                            .clicked()
                        {
                            self.settings.welcome_build_visible = false;
                            self.persist();
                        }
                    });
                    ui.label("1. Open a .hyproj and select it in Explorer.");
                    ui.label("2. Configure and test your Hydrogen compiler.");
                    ui.label("3. Choose Build, then follow progress in Output.");
                    if ui.button("Configure compiler…").clicked() {
                        self.show_settings = true;
                    }
                });
        }
    }
    pub(crate) fn action(&mut self, index: usize, ctx: &egui::Context) {
        match index {
            0 => self.new_document(),
            1 => {
                if let Some(p) = rfd::FileDialog::new().pick_file() {
                    self.open(p);
                }
            }
            2 => {
                if let Some(p) = rfd::FileDialog::new().pick_folder() {
                    self.add_folder(p);
                }
            }
            3 => {
                if let Some(p) = rfd::FileDialog::new()
                    .add_filter("Hydrogen project", &["hyproj"])
                    .pick_file()
                {
                    self.add_project(p);
                }
            }
            4 => {
                self.save_active();
            }
            5 => {
                if let Some(i) = self.active {
                    self.save_document(i, false, true);
                }
            }
            6 => {
                self.save_all();
            }
            7 => {
                if let Some(i) = self.active {
                    self.close(i);
                }
            }
            8 => {
                self.show_quick_open = true;
                self.quick_focus = true;
                self.quick_selected = 0;
            }
            9 => self.build(BuildScope::Project),
            10 => self.build(BuildScope::CurrentFile),
            11 => self.hydrogen("check"),
            12 => self.hydrogen("run"),
            13 => {
                self.settings.sidebar_visible = !self.settings.sidebar_visible;
                self.persist();
            }
            14 => {
                self.settings.terminal_visible = !self.settings.terminal_visible;
                self.persist();
            }
            15 => self.show_welcome(),
            16 => self.show_settings = true,
            17 => self.show_sidebar(SidebarView::Plugins),
            18 => {
                self.workspace_requested = true;
                self.request_diagnostic_scan();
            }
            19 => self.git(&["status", "--short", "--branch"]),
            20 => self.git(&["diff"]),
            21 => {
                if let Some(cancel) = &self.job_cancel {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
            22 => {
                self.show_sidebar(SidebarView::Search);
                self.show_search = true;
                self.find_focus = true;
            }
            23 => {
                self.show_sidebar(SidebarView::Search);
                self.show_search = true;
                self.show_replace = true;
                self.find_focus = true;
            }
            24 => {
                self.show_go_to_line = true;
                self.go_focus = true;
            }
            25 => self.show_sidebar(SidebarView::Themes),
            26 => {
                self.settings.terminal_visible = true;
                self.settings.bottom_tab = BottomTab::Terminal;
                self.persist();
            }
            27 => {
                self.settings.terminal_visible = true;
                self.settings.bottom_tab = BottomTab::Output;
                self.persist();
            }
            28 => self.start_project_dialog(),
            29 => self.show_new_layout = true,
            30 => self.show_welcome(),
            _ => {}
        }
        ctx.request_repaint();
    }
    pub(crate) fn palette_ui(&mut self, ctx: &egui::Context) {
        if !self.show_palette {
            return;
        }
        let mut selected = None;
        let id = egui::Id::new("command-palette");
        let response = egui::Modal::new(id)
            .area(
                egui::Modal::default_area(id)
                    .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 80.0)),
            )
            .show(ctx, |ui| {
                ui.set_width(580.0_f32.min(ctx.screen_rect().width() - 50.0));
                ui.heading("Command Palette");
                let response = ui.add(
                    TextEdit::singleline(&mut self.palette_query)
                        .hint_text("> Type a command…")
                        .desired_width(f32::INFINITY),
                );
                if self.palette_focus {
                    response.request_focus();
                    self.palette_focus = false;
                }
                let mut scroll_selection = response.changed();
                if response.changed() {
                    self.palette_selected = 0;
                }
                let query = self
                    .palette_query
                    .trim_start_matches('>')
                    .trim()
                    .to_lowercase();
                let matches: Vec<_> = ACTIONS
                    .iter()
                    .enumerate()
                    .filter(|(_, name)| name.to_lowercase().contains(&query))
                    .collect();
                self.palette_selected = self.palette_selected.min(matches.len().saturating_sub(1));
                if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowDown))
                    && !matches.is_empty()
                {
                    self.palette_selected = (self.palette_selected + 1) % matches.len();
                    scroll_selection = true;
                }
                if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowUp))
                    && !matches.is_empty()
                {
                    self.palette_selected =
                        (self.palette_selected + matches.len() - 1) % matches.len();
                    scroll_selection = true;
                }
                if ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) {
                    selected = matches.get(self.palette_selected).map(|(i, _)| *i);
                }
                egui::ScrollArea::vertical()
                    .max_height(340.0)
                    .show(ui, |ui| {
                        for (row, (index, name)) in matches.iter().enumerate() {
                            let r = ui.selectable_label(row == self.palette_selected, **name);
                            if row == self.palette_selected && scroll_selection {
                                r.scroll_to_me(Some(egui::Align::Center));
                            }
                            if r.clicked() {
                                selected = Some(*index);
                            }
                        }
                        if matches.is_empty() {
                            ui.label("No matching commands");
                        }
                    });
                ui.label(
                    RichText::new("Up / Down select · Enter run · Esc close")
                        .small()
                        .color(theme::colors(ui.ctx()).muted.color()),
                );
            });
        if response.should_close() {
            self.show_palette = false;
        }
        if let Some(index) = selected {
            self.show_palette = false;
            self.action(index, ctx);
        }
    }
    pub(crate) fn reset_persistence(&mut self) {
        for (path, blocked) in [
            (self.settings_path.clone(), self.settings_blocked),
            (self.session_path.clone(), self.session_blocked),
        ] {
            if !blocked {
                continue;
            }
            if let Some(path) = path {
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos();
                let backup = path.with_extension(format!("backup-{stamp}.json"));
                if let Err(e) = fs::copy(&path, &backup) {
                    self.notice(format!("Could not back up {}: {e}", path.display()));
                    return;
                }
                self.notice(format!("Original preserved at {}", backup.display()));
            }
        }
        self.settings_blocked = false;
        self.session_blocked = false;
        self.persist();
        self.save_session();
    }
    pub(crate) fn safety_dialogs(&mut self, ctx: &egui::Context) {
        if self.pending_designer_close {
            let title = self
                .designer
                .as_ref()
                .map(|d| d.path.display().to_string())
                .unwrap_or_default();
            let result =
                egui::Modal::new(egui::Id::new("confirm-designer-close")).show(ctx, |ui| {
                    ui.heading("Save the visual layout before closing?");
                    ui.label(title);
                    ui.horizontal(|ui| {
                        if ui.button("Save and close").clicked() && self.save_designer() {
                            self.designer = None;
                            self.designer_active = false;
                            self.pending_designer_close = false;
                        }
                        if ui.button("Discard changes").clicked() {
                            self.designer = None;
                            self.designer_active = false;
                            self.pending_designer_close = false;
                        }
                        if ui.button("Cancel").clicked() {
                            self.pending_designer_close = false;
                        }
                    });
                });
            if result.should_close() {
                self.pending_designer_close = false;
            }
            return;
        }

        if let Some(index) = self.pending_conflict {
            if index >= self.docs.len() {
                self.pending_conflict = None;
                return;
            }
            let name = self.docs[index]
                .path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            let response=egui::Modal::new(egui::Id::new("save-conflict")).show(ctx,|ui|{
                ui.set_max_width(540.0);ui.heading("This file changed on disk");ui.label(name);
                ui.label("Your edits are still in memory. Choose which version to keep, or save your edits to another file.");
                ui.horizontal_wrapped(|ui|{
                    if ui.button("Save As…").clicked() && self.save_document(index,false,true){self.pending_conflict=None;}
                    if ui.button("Overwrite disk with my edits").clicked() && self.save_document(index,true,false){self.pending_conflict=None;}
                    if ui.button("Reload disk · discard my edits").clicked(){
                        let result=self.docs[index].path.as_ref().and_then(|p|fs::read_to_string(p).ok());
                        if let Some(text)=result{self.docs[index].text=text.clone();self.docs[index].saved=text;self.pending_conflict=None;self.request_diagnostic_scan();self.workspace_requested=true;self.save_session();}
                        else{self.notice("Could not reload the file. Your edits have been kept.".into());}
                    }
                    if ui.button("Cancel").clicked(){self.pending_conflict=None;self.pending_close=None;self.exit_requested=false;}
                });
            });
            if response.should_close() {
                self.pending_conflict = None;
                self.pending_close = None;
                self.exit_requested = false;
            }
            return;
        }
        if self.exit_requested {
            let response = egui::Modal::new(egui::Id::new("confirm-exit")).show(ctx, |ui| {
                ui.set_max_width(480.0);
                ui.heading("Save your work before closing?");
                if let Some(designer) = &self.designer {
                    if designer.dirty {
                        ui.label(format!("UI layout: {}", designer.path.display()));
                    }
                }
                for doc in &self.docs {
                    if doc.text != doc.saved {
                        ui.label(
                            doc.path
                                .as_ref()
                                .map(|p| p.display().to_string())
                                .unwrap_or_else(|| format!("Untitled {}", doc.id)),
                        );
                    }
                }
                if self.job_rx.is_some()
                    || self.terminal.as_ref().is_some_and(|t| t.exited.is_none())
                {
                    ui.colored_label(
                        theme::colors(ui.ctx()).warning.color(),
                        "Closing Quark will end the terminal session and stop running tool jobs.",
                    );
                }
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Save all & close").clicked() && self.save_all() {
                        self.finish_exit(ctx, false);
                    }
                    if ui.button("Discard changes & close").clicked() {
                        self.finish_exit(ctx, true);
                    }
                    if ui.button("Cancel").clicked() {
                        self.exit_requested = false;
                    }
                });
            });
            if response.should_close() {
                self.exit_requested = false;
            }
        } else if let Some(index) = self.pending_close {
            if index >= self.docs.len() {
                self.pending_close = None;
                return;
            }
            let response = egui::Modal::new(egui::Id::new("confirm-tab-close")).show(ctx, |ui| {
                ui.set_max_width(480.0);
                ui.heading("Save changes before closing this file?");
                ui.label(
                    self.docs[index]
                        .path
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| format!("Untitled {}", self.docs[index].id)),
                );
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() && self.save(index) {
                        self.pending_close = None;
                        self.close(index);
                        self.save_session();
                    }
                    if ui.button("Discard").clicked() {
                        self.docs[index].text = self.docs[index].saved.clone();
                        self.pending_close = None;
                        self.close(index);
                        self.save_session();
                    }
                    if ui.button("Cancel").clicked() {
                        self.pending_close = None;
                    }
                });
            });
            if response.should_close() {
                self.pending_close = None;
            }
        }
    }
    fn finish_exit(&mut self, ctx: &egui::Context, discard: bool) {
        if discard {
            self.docs.retain(|d| d.path.is_some());
            for doc in &mut self.docs {
                doc.text = doc.saved.clone();
            }
            self.active = self.active.filter(|i| *i < self.docs.len());
        }
        if let Some(cancel) = &self.job_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        self.save_session();
        self.allow_exit = true;
        self.exit_requested = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Icon {
    Files,
    Search,
    Git,
    Extensions,
    Settings,
    Themes,
}
/// Draw icons as native vectors so missing Unicode glyphs never become boxes.
pub(crate) fn activity_button(
    ui: &mut egui::Ui,
    icon: Icon,
    tip: &str,
    selected: bool,
) -> egui::Response {
    let definition = theme::current(ui.ctx());
    let colors = definition.colors;
    let size = 28.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
    let p = ui.painter();
    if selected || response.hovered() {
        p.rect_filled(
            rect,
            2,
            if selected {
                colors.selection.color()
            } else {
                colors.surface.color()
            },
        );
    }
    if selected {
        p.line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            egui::Stroke::new(2.0_f32, theme::colors(ui.ctx()).accent.color()),
        );
    }
    let center = rect.center();
    let color = if selected || response.hovered() {
        colors.text.color()
    } else {
        theme::colors(ui.ctx()).muted.color()
    };
    let color = if !selected && !response.hovered() {
        colors.icon.color()
    } else {
        color
    };
    let stroke = egui::Stroke::new(
        if definition.icon_style == theme::IconStyle::Solid {
            2.5_f32
        } else {
            1.5_f32
        },
        color,
    );
    let rounding = if definition.icon_style == theme::IconStyle::Rounded {
        3
    } else {
        0
    };
    let pt =
        |x: f32, y: f32| center + egui::vec2(x, y) * (definition.icon_scale as f32 / 100.0) * 0.72;
    let line = |a: (f32, f32), b: (f32, f32)| {
        p.line_segment([pt(a.0, a.1), pt(b.0, b.1)], stroke);
    };
    match icon {
        Icon::Files => {
            p.rect_stroke(
                egui::Rect::from_min_max(pt(-6.0, -8.0), pt(8.0, 9.0)),
                rounding,
                stroke,
                egui::StrokeKind::Inside,
            );
            line((-9.0, 5.0), (-9.0, -11.0));
            line((-9.0, -11.0), (3.0, -11.0));
        }
        Icon::Search => {
            p.circle_stroke(pt(-2.0, -2.0), 7.0, stroke);
            line((3.0, 3.0), (10.0, 10.0));
        }
        Icon::Git => {
            for (x, y) in [(-5.0, -9.0), (-5.0, 9.0), (6.0, -5.0)] {
                p.circle_stroke(pt(x, y), 2.6, stroke);
            }
            line((-5.0, -6.0), (-5.0, 6.0));
            line((-5.0, 2.0), (6.0, -1.0));
            line((6.0, -1.0), (6.0, -2.4));
        }
        Icon::Extensions => {
            for (x, y) in [(-9.0, -7.0), (-9.0, 2.0), (0.0, 2.0), (2.0, -9.0)] {
                p.rect_stroke(
                    egui::Rect::from_min_max(pt(x, y), pt(x + 7.0, y + 7.0)),
                    rounding,
                    stroke,
                    egui::StrokeKind::Inside,
                );
            }
        }
        Icon::Themes => {
            p.circle_stroke(center, 8.0, stroke);
            for (x, y) in [(-3.5, -3.5), (3.5, -3.5), (-3.5, 3.5), (3.5, 3.5)] {
                p.circle_filled(
                    pt(x, y),
                    2.0,
                    if x < 0.0 {
                        colors.accent.color()
                    } else {
                        colors.warning.color()
                    },
                );
            }
        }
        Icon::Settings => {
            p.circle_stroke(center, 7.0, stroke);
            p.circle_stroke(center, 2.5, stroke);
            for n in 0..8 {
                let a = n as f32 * std::f32::consts::TAU / 8.0;
                line(
                    (a.cos() * 7.0, a.sin() * 7.0),
                    (a.cos() * 11.0, a.sin() * 11.0),
                );
            }
        }
    }
    response.on_hover_text(tip)
}

/// Two compact, right-aligned layout controls in the unified top bar.
pub(crate) fn layout_button(ui: &mut egui::Ui, left: bool, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(29.0, 24.0), egui::Sense::click());
    let colors = theme::colors(ui.ctx());
    if response.hovered() {
        ui.painter().rect_filled(rect, 3, colors.selection.color());
    }
    let r = rect.shrink2(egui::vec2(5.0, 5.0));
    let stroke = egui::Stroke::new(
        1.4_f32,
        if selected {
            colors.accent.color()
        } else {
            colors.icon.color()
        },
    );
    ui.painter()
        .rect_stroke(r, 1, stroke, egui::StrokeKind::Inside);
    let fill = if left {
        egui::Rect::from_min_max(r.min, r.min + egui::vec2(r.width() * 0.32, r.height()))
    } else {
        egui::Rect::from_min_max(r.min + egui::vec2(0.0, r.height() * 0.65), r.max)
    };
    if selected {
        ui.painter().rect_filled(fill, 0, stroke.color);
    } else {
        ui.painter().line_segment(
            if left {
                [fill.right_top(), fill.right_bottom()]
            } else {
                [fill.left_top(), fill.right_top()]
            },
            stroke,
        );
    }
    response.on_hover_text(if left {
        "Toggle left sidebar · Ctrl/Cmd+B"
    } else {
        "Toggle bottom panel · Ctrl+`"
    })
}

/// Compact underline tabs keep the Explorer toolbar familiar without a second activity rail.
pub(crate) fn view_tab(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let response = ui.add(
        egui::Button::new(RichText::new(label).small().color(if selected {
            theme::colors(ui.ctx()).text.color()
        } else {
            theme::colors(ui.ctx()).muted.color()
        }))
        .frame(false),
    );
    if selected {
        let r = response.rect;
        ui.painter().line_segment(
            [r.left_bottom(), r.right_bottom()],
            egui::Stroke::new(2.0_f32, theme::colors(ui.ctx()).accent.color()),
        );
    }
    response
}
