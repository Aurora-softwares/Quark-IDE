//! VS Code-style installed-item lists. Editing is deliberately behind each item's options menu.
use crate::*;
impl Quark {
    pub(crate) fn themes_catalog_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Colour themes").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button("+ Add")
                    .on_hover_text("Add a local theme or view catalogue setup")
                    .clicked()
                {
                    self.catalog_add = Some(true);
                }
            });
        });
        ui.add(
            TextEdit::singleline(&mut self.theme_query)
                .hint_text("Search themes…")
                .desired_width(f32::INFINITY),
        );
        let root = self.workspace_root();
        let active = self.effective_theme();
        let items: Vec<_> = self
            .settings
            .catalog
            .themes()
            .into_iter()
            .filter(|t| {
                catalog::matches(
                    &self.theme_query,
                    &t.definition.name,
                    if t.bundled {
                        "bundled colour theme"
                    } else {
                        "local colour theme"
                    },
                )
            })
            .collect();
        ui.label(
            RichText::new(format!("INSTALLED  {}", items.len()))
                .small()
                .color(theme::colors(ui.ctx()).muted.color()),
        );
        if items.is_empty() {
            ui.label("No themes match your search.");
        }
        for item in items {
            ui.push_id(&item.id, |ui| {
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(2, 10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (rect, _) =
                                ui.allocate_exact_size(egui::vec2(34., 38.), egui::Sense::hover());
                            ui.painter().rect_filled(
                                rect,
                                3,
                                item.definition.colors.editor.color(),
                            );
                            for (index, color) in [
                                item.definition.colors.accent,
                                item.definition.syntax.keyword,
                                item.definition.syntax.string,
                            ]
                            .into_iter()
                            .enumerate()
                            {
                                let y = rect.top() + 8. + index as f32 * 8.;
                                ui.painter().line_segment(
                                    [
                                        egui::pos2(rect.left() + 6., y),
                                        egui::pos2(rect.right() - 6. - index as f32 * 3., y),
                                    ],
                                    egui::Stroke::new(3.0_f32, color.color()),
                                );
                            }
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&item.definition.name).strong());
                                ui.label(
                                    RichText::new(if item.bundled {
                                        "Bundled · Quark"
                                    } else {
                                        "Local theme"
                                    })
                                    .small()
                                    .color(theme::colors(ui.ctx()).muted.color()),
                                );
                                let enabled = self
                                    .settings
                                    .catalog
                                    .theme_enabled(&item.id, root.as_deref());
                                let status = if item.definition == *active.definition() {
                                    "Active"
                                } else if enabled {
                                    "Installed"
                                } else {
                                    "Disabled"
                                };
                                ui.label(RichText::new(status).small().color(
                                    if status == "Active" {
                                        theme::colors(ui.ctx()).accent.color()
                                    } else {
                                        theme::colors(ui.ctx()).muted.color()
                                    },
                                ));
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.menu_button("...", |ui| self.theme_menu(ui, &item))
                                        .response
                                        .on_hover_text(format!(
                                            "Options for {}",
                                            item.definition.name
                                        ));
                                },
                            );
                        });
                    });
                ui.separator();
            });
        }
    }
    fn theme_menu(&mut self, ui: &mut egui::Ui, item: &catalog::ThemeItem) {
        let workspace = self.workspace_root().is_some();
        if ui.button("Enable").clicked() {
            self.enable_theme(&item.id, false);
            ui.close_menu();
        }
        if ui
            .add_enabled(workspace, egui::Button::new("Enable (Workspace)"))
            .on_disabled_hover_text("Open a workspace first")
            .clicked()
        {
            self.enable_theme(&item.id, true);
            ui.close_menu();
        }
        if ui.button("Disable").clicked() {
            self.disable_theme(&item.id, false);
            ui.close_menu();
        }
        if ui
            .add_enabled(workspace, egui::Button::new("Disable (Workspace)"))
            .on_disabled_hover_text("Open a workspace first")
            .clicked()
        {
            self.disable_theme(&item.id, true);
            ui.close_menu();
        }
        ui.separator();
        let mut update = self.settings.catalog.theme_auto_update.contains(&item.id);
        if ui.checkbox(&mut update,"Auto update").on_hover_text("Preference only: remote updates will be available when the theme repository is configured.").changed(){self.settings.catalog.theme_auto_update.retain(|id|id!=&item.id);if update{self.settings.catalog.theme_auto_update.push(item.id.clone());}self.persist();}
        if ui.button("Settings").clicked() {
            self.editing_theme = Some(item.id.clone());
            ui.close_menu();
        }
        if ui.button("Open Containing Folder").clicked() {
            if let Err(e) = self
                .theme_folder(&item.id)
                .and_then(|p| catalog::open_folder(&p))
            {
                self.notice(format!("Could not open theme folder: {e}"));
            }
            ui.close_menu();
        }
        if ui
            .button("Download ZIP")
            .on_hover_text("Export this installed theme as a ZIP file")
            .clicked()
        {
            if let Some(path) = rfd::FileDialog::new()
                .set_file_name(format!("{}.zip", item.id))
                .add_filter("ZIP archive", &["zip"])
                .save_file()
            {
                if let Err(e) = self
                    .theme_folder(&item.id)
                    .and_then(|root| catalog::export_zip(&root, &path))
                {
                    self.notice(format!("Theme ZIP export failed: {e}"));
                }
            }
            ui.close_menu();
        }
        ui.separator();
        if ui
            .add_enabled(!item.bundled, egui::Button::new("Remove"))
            .on_disabled_hover_text("Bundled themes can be disabled, but cannot be removed.")
            .clicked()
        {
            if let Err(e) = self.remove_theme(&item.id) {
                self.notice(e);
            }
            ui.close_menu();
        }
    }
    pub(crate) fn extensions_catalog_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Extensions").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("+ Add").clicked() {
                    self.catalog_add = Some(false);
                }
                if ui
                    .small_button("↻")
                    .on_hover_text("Reload extensions")
                    .clicked()
                {
                    self.reload_plugins();
                }
            });
        });
        ui.add(
            TextEdit::singleline(&mut self.extension_query)
                .hint_text("Search extensions…")
                .desired_width(f32::INFINITY),
        );
        let list: Vec<_> = self
            .plugins
            .clone()
            .into_iter()
            .filter(|p| {
                catalog::matches(
                    &self.extension_query,
                    &p.name,
                    &format!("{} {}", p.id, p.description),
                )
            })
            .collect();
        ui.label(
            RichText::new(format!("INSTALLED  {}", list.len()))
                .small()
                .color(theme::colors(ui.ctx()).muted.color()),
        );
        if self.plugins.is_empty() {
            ui.label("No extensions installed.");
            ui.label("Add a local extension to get started.");
        } else if list.is_empty() {
            ui.label("No extensions match your search.");
        }
        for plugin in list {
            ui.push_id(&plugin.id, |ui| {
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(2, 10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (rect, _) =
                                ui.allocate_exact_size(egui::vec2(34., 38.), egui::Sense::hover());
                            let colors = theme::colors(ui.ctx());
                            ui.painter().rect_filled(rect, 3, colors.surface.color());
                            ui.painter().text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                plugin
                                    .name
                                    .chars()
                                    .next()
                                    .unwrap_or('E')
                                    .to_uppercase()
                                    .to_string(),
                                egui::FontId::proportional(23.),
                                colors.accent.color(),
                            );
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&plugin.name).strong());
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&plugin.description)
                                            .small()
                                            .color(colors.muted.color()),
                                    )
                                    .truncate(),
                                );
                                ui.label(
                                    RichText::new(if self.extension_enabled(&plugin.id) {
                                        "Enabled · Local extension"
                                    } else {
                                        "Disabled · Local extension"
                                    })
                                    .small()
                                    .color(colors.muted.color()),
                                );
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.menu_button("...", |ui| self.extension_menu(ui, &plugin))
                                        .response
                                        .on_hover_text(format!("Options for {}", plugin.name));
                                },
                            );
                        });
                    });
                ui.separator();
            });
        }
        for warning in &self.plugin_warnings {
            ui.colored_label(theme::colors(ui.ctx()).warning.color(), warning);
        }
    }
    fn extension_menu(&mut self, ui: &mut egui::Ui, plugin: &plugins::Plugin) {
        let workspace = self.workspace_root().is_some();
        if ui.button("Enable").clicked() {
            self.set_extension_enabled(&plugin.id, true, false);
            ui.close_menu();
        }
        if ui
            .add_enabled(workspace, egui::Button::new("Enable (Workspace)"))
            .on_disabled_hover_text("Open a workspace first")
            .clicked()
        {
            self.set_extension_enabled(&plugin.id, true, true);
            ui.close_menu();
        }
        if ui.button("Disable").clicked() {
            self.set_extension_enabled(&plugin.id, false, false);
            ui.close_menu();
        }
        if ui
            .add_enabled(workspace, egui::Button::new("Disable (Workspace)"))
            .on_disabled_hover_text("Open a workspace first")
            .clicked()
        {
            self.set_extension_enabled(&plugin.id, false, true);
            ui.close_menu();
        }
        ui.separator();
        let mut update = self
            .settings
            .catalog
            .extension_auto_update
            .contains(&plugin.id);
        if ui
            .checkbox(&mut update, "Auto update")
            .on_hover_text("Preference only: remote updates are not connected yet.")
            .changed()
        {
            self.settings
                .catalog
                .extension_auto_update
                .retain(|id| id != &plugin.id);
            if update {
                self.settings
                    .catalog
                    .extension_auto_update
                    .push(plugin.id.clone());
            }
            self.persist();
        }
        if ui.button("Settings").clicked() {
            self.editing_extension = Some(plugin.id.clone());
            ui.close_menu();
        }
        if ui.button("Open Containing Folder").clicked() {
            if let Err(e) = catalog::open_folder(&plugin.directory) {
                self.notice(format!("Could not open extension folder: {e}"));
            }
            ui.close_menu();
        }
        if ui
            .button("Download ZIP")
            .on_hover_text("Export this installed extension as a ZIP file")
            .clicked()
        {
            if let Some(path) = rfd::FileDialog::new()
                .set_file_name("quark-extension.zip")
                .add_filter("ZIP archive", &["zip"])
                .save_file()
            {
                if let Err(e) = catalog::export_zip(&plugin.directory, &path) {
                    self.notice(format!("Extension ZIP export failed: {e}"));
                }
            }
            ui.close_menu();
        }
        ui.separator();
        if ui
            .add_enabled(self.job_rx.is_none(), egui::Button::new("Remove"))
            .on_disabled_hover_text("Stop the running tool job first")
            .clicked()
        {
            self.pending_extension_remove = Some(plugin.id.clone());
            ui.close_menu();
        }
    }
    pub(crate) fn catalog_windows(&mut self, ctx: &egui::Context) {
        if let Some(themes) = self.catalog_add {
            let mut open = true;
            egui::Window::new(if themes{"Add themes"}else{"Add extensions"}).open(&mut open).default_width(460.).show(ctx,|ui|{
                ui.heading(if themes{"Make Quark your own"}else{"Extend your workspace"});
                ui.label("The online catalogue is not connected yet.");ui.label("Repository browsing, downloads and automatic updates will appear here once a repository is configured.");
                ui.separator();if themes{
                    if ui.button("Import local theme…").clicked(){if let Some(path)=rfd::FileDialog::new().add_filter("Quark theme",&["json"]).pick_file(){self.import_theme(&path,ctx);}}
                    ui.label("Version 1 Quark theme JSON files are supported.");
                }else if let Some(root)=plugins::directory(self.settings_path.as_deref()){
                    ui.label("Place an extension subfolder containing plugin.json here, then reload:");ui.label(root.display().to_string());
                    if ui.button("Open extensions folder").clicked(){if let Err(e)=fs::create_dir_all(&root).map_err(|e|e.to_string()).and_then(|_|catalog::open_folder(&root)){self.notice(e);}}
                    if ui.button("Reload extensions").clicked(){self.reload_plugins();}
                }
            });
            if !open {
                self.catalog_add = None;
            }
        }
        if let Some(id) = self.editing_theme.clone() {
            if let Some(item) = self
                .settings
                .catalog
                .themes()
                .into_iter()
                .find(|t| t.id == id)
            {
                let mut open = true;
                let mut definition = item.definition.clone();
                let mut reset = false;
                egui::Window::new(format!("Theme settings · {}",item.definition.name)).id(egui::Id::new("theme-settings")).open(&mut open).default_width(620.).vscroll(true).show(ctx,|ui|{
                    ui.label("Customisations are saved in your Quark preferences. Bundled theme files stay intact.");
                    ui.label("Theme name");ui.add(TextEdit::singleline(&mut definition.name).id(egui::Id::new("theme-option-name")));
                    egui::ComboBox::from_id_salt("theme-icon-style").selected_text(format!("{:?} icons",definition.icon_style)).show_ui(ui,|ui|{for style in[theme::IconStyle::Outline,theme::IconStyle::Rounded,theme::IconStyle::Solid]{ui.selectable_value(&mut definition.icon_style,style,format!("{style:?}"));}});
                    ui.add(egui::Slider::new(&mut definition.icon_scale,75..=150).text("Icon size %"));
                    theme_colours(ui,&mut definition);
                    if ui.button("Export theme JSON…").clicked(){if let Some(path)=rfd::FileDialog::new().set_file_name("quark-theme.json").save_file(){self.export_theme(&path,&definition);}}
                    if ui.button("Reset customisations").clicked(){reset=true;let old=item.definition.clone();self.settings.catalog.customisations.remove(&id);let original=self.settings.catalog.themes().into_iter().find(|t|t.id==id).unwrap();if *self.settings.theme.definition()==old{self.settings.theme=original.settings();}definition=original.definition;self.persist();}
                });
                if !reset && definition != item.definition {
                    match self.edit_theme(&id, definition) {
                        Ok(()) => self.theme_persist_due = Some(ctx.input(|i| i.time) + 0.5),
                        Err(e) => {
                            self.status = e;
                        }
                    }
                }
                if !open {
                    self.editing_theme = None;
                }
            } else {
                self.editing_theme = None;
            }
        }
        if let Some(id) = self.editing_extension.clone() {
            if let Some(plugin) = self.plugins.iter().find(|p| p.id == id).cloned() {
                let mut open = true;
                egui::Window::new(format!("Extension settings · {}", plugin.name))
                    .id(egui::Id::new("extension-settings"))
                    .open(&mut open)
                    .default_width(560.)
                    .vscroll(true)
                    .show(ctx, |ui| {
                        ui.label(&plugin.description);
                        ui.label(plugin.directory.display().to_string());
                        ui.label(if self.extension_enabled(&id) {
                            "Enabled in this workspace"
                        } else {
                            "Disabled in this workspace"
                        });
                        if ui.button("Edit extension manifest…").clicked() {
                            self.open(plugin.directory.join("plugin.json"));
                        }
                        ui.label("Save manifest edits, then Reload extensions to apply them.");
                        if ui.button("Reload extensions").clicked() {
                            self.reload_plugins();
                        }
                        ui.separator();
                        ui.heading("Actions");
                        for (index, command) in plugin.commands.iter().enumerate() {
                            if ui
                                .add_enabled(
                                    self.extension_enabled(&id) && self.job_rx.is_none(),
                                    egui::Button::new(&command.title),
                                )
                                .clicked()
                            {
                                self.run_plugin(&id, index);
                            }
                        }
                    });
                if !open {
                    self.editing_extension = None;
                }
            } else {
                self.editing_extension = None;
            }
        }
        if let Some(id) = self.pending_extension_remove.clone() {
            egui::Modal::new(egui::Id::new("remove-extension")).show(ctx,|ui|{
            ui.heading("Remove extension?");ui.label("The extension folder will be moved to extension-backups beside your configuration. Workspace and project files are kept.");
            ui.horizontal(|ui|{if ui.button("Remove extension").clicked(){if let Err(e)=self.remove_extension(&id){self.notice(e);}self.pending_extension_remove=None;}
if ui.button("Cancel").clicked(){self.pending_extension_remove=None;}});
        });
        }
    }
}
fn theme_colours(ui: &mut egui::Ui, d: &mut theme::Definition) {
    egui::CollapsingHeader::new("Interface colours").show(ui, |ui| {
        for (name, color) in [
            ("Editor", &mut d.colors.editor),
            ("Sidebar", &mut d.colors.sidebar),
            ("Surface", &mut d.colors.surface),
            ("Text", &mut d.colors.text),
            ("Muted text", &mut d.colors.muted),
            ("Accent", &mut d.colors.accent),
            ("Borders", &mut d.colors.border),
            ("Selection", &mut d.colors.selection),
            ("Status bar", &mut d.colors.status),
            ("Status text", &mut d.colors.status_text),
            ("Errors", &mut d.colors.error),
            ("Warnings", &mut d.colors.warning),
            ("Icons", &mut d.colors.icon),
        ] {
            panels::color_row(ui, name, color);
        }
    });
    egui::CollapsingHeader::new("Syntax colours").show(ui, |ui| {
        for (name, color) in [
            ("Keywords", &mut d.syntax.keyword),
            ("Modifiers", &mut d.syntax.modifier),
            ("Types", &mut d.syntax.type_name),
            ("Literals", &mut d.syntax.literal),
            ("Comments", &mut d.syntax.comment),
            ("Strings", &mut d.syntax.string),
            ("Numbers", &mut d.syntax.number),
            ("Operators", &mut d.syntax.operator),
            ("Punctuation", &mut d.syntax.punctuation),
            ("Declarations", &mut d.syntax.declaration),
            ("Functions", &mut d.syntax.function),
        ] {
            panels::color_row(ui, name, color);
        }
    });
    egui::CollapsingHeader::new("Terminal colours").show(ui, |ui| {
        panels::color_row(ui, "Background", &mut d.colors.terminal);
        panels::color_row(ui, "Text", &mut d.colors.terminal_text);
        for (index, color) in d.colors.ansi.iter_mut().enumerate() {
            panels::color_row(ui, &format!("ANSI {index}"), color);
        }
    });
}
