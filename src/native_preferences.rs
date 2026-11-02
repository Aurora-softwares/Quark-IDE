//! Persistent preferences for the native GTK Quark frontend.
//! Keep application preferences separate from workspace and docking state.
use gtk4::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Preferences {
    pub appearance: Appearance,
    pub editor: Editor,
    pub debugging: Debugging,
    pub build: Build,
    pub hydrogen: Hydrogen,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Appearance {
    pub theme: ThemeChoice,
    pub compact_controls: bool,
}
impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::System,
            compact_controls: false,
        }
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Editor {
    pub tab_width: u8,
    pub show_line_numbers: bool,
    pub word_wrap: bool,
}
impl Default for Editor {
    fn default() -> Self {
        Self {
            tab_width: 4,
            show_line_numbers: true,
            word_wrap: false,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Debugging {
    pub stop_on_exception: bool,
    pub show_debug_output: bool,
}
impl Default for Debugging {
    fn default() -> Self {
        Self {
            stop_on_exception: true,
            show_debug_output: true,
        }
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Hydrogen {
    /// Absolute executable selected by the user; None enables automatic
    /// QUARK_HYDROGEN_COMPILER / PATH discovery.
    pub executable: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Build {
    pub save_before_build: bool,
    pub show_output: bool,
}
impl Default for Build {
    fn default() -> Self {
        Self {
            save_before_build: true,
            show_output: true,
        }
    }
}
pub fn load(path: &Path) -> Preferences {
    fs::read(path)
        .ok()
        .and_then(|data| serde_json::from_slice::<Preferences>(&data).ok())
        .unwrap_or_default()
}
fn save(path: &Path, preferences: &Preferences) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let bytes = serde_json::to_vec_pretty(preferences).map_err(|error| error.to_string())?;
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, bytes).map_err(|error| error.to_string())?;
    fs::rename(&temp, path).map_err(|error| error.to_string())
}
pub fn apply_theme(preferences: &Preferences) {
    if let Some(settings) = gtk4::Settings::default() {
        // System uses the platform's GTK theme; do not force dark/light.
        settings.set_gtk_application_prefer_dark_theme(matches!(
            preferences.appearance.theme,
            ThemeChoice::Dark
        ));
    }
}
fn section() -> gtk4::Box {
    let box_ = gtk4::Box::new(gtk4::Orientation::Vertical, 14);
    box_.set_margin_top(24);
    box_.set_margin_bottom(24);
    box_.set_margin_start(28);
    box_.set_margin_end(28);
    box_
}
fn heading(parent: &gtk4::Box, title: &str, description: &str) {
    let title_label = gtk4::Label::new(Some(title));
    title_label.add_css_class("title-2");
    title_label.set_xalign(0.0);
    parent.append(&title_label);
    let description_label = gtk4::Label::new(Some(description));
    description_label.add_css_class("dim-label");
    description_label.set_wrap(true);
    description_label.set_xalign(0.0);
    parent.append(&description_label);
}
fn checkbox(parent: &gtk4::Box, label: &str, active: bool, enabled: bool) -> gtk4::CheckButton {
    let check = gtk4::CheckButton::with_label(label);
    check.set_active(active);
    check.set_sensitive(enabled);
    parent.append(&check);
    check
}
pub fn show(path: PathBuf) {
    let preferences = load(&path);
    apply_theme(&preferences);
    let state = Rc::new(RefCell::new(preferences.clone()));
    let window = gtk4::Window::builder()
        .title("Quark Settings")
        .default_width(680)
        .default_height(530)
        .build();
    window.set_hide_on_close(true);
    let notebook = gtk4::Notebook::new();
    notebook.set_tab_pos(gtk4::PositionType::Left);
    notebook.set_hexpand(true);
    notebook.set_vexpand(true);

    let appearance = section();
    heading(
        &appearance,
        "Appearance",
        "Make Quark feel at home on your desktop.",
    );
    let label = gtk4::Label::new(Some("Colour scheme"));
    label.set_xalign(0.0);
    appearance.append(&label);
    // GTK 4.10+ replaces the deprecated ComboBoxText with DropDown.
    let themes = gtk4::DropDown::from_strings(&["Follow system", "Light", "Dark"]);
    let selected = match preferences.appearance.theme {
        ThemeChoice::System => 0,
        ThemeChoice::Light => 1,
        ThemeChoice::Dark => 2,
    };
    themes.set_selected(selected);
    appearance.append(&themes);
    {
        let state = state.clone();
        let path = path.clone();
        themes.connect_selected_notify(move |combo| {
            let mut prefs = state.borrow_mut();
            prefs.appearance.theme = match combo.selected() {
                1 => ThemeChoice::Light,
                2 => ThemeChoice::Dark,
                _ => ThemeChoice::System,
            };
            apply_theme(&prefs);
            if let Err(error) = save(&path, &prefs) {
                eprintln!("Quark: could not save preferences: {error}");
            }
        });
    }
    let compact = checkbox(
        &appearance,
        "Compact toolbar and controls (planned)",
        preferences.appearance.compact_controls,
        false,
    );
    compact.set_tooltip_text(Some("Reserved for the native layout density integration."));
    notebook.append_page(&appearance, Some(&gtk4::Label::new(Some("Appearance"))));

    let editor = section();
    heading(
        &editor,
        "Editor",
        "Editor behaviour will become configurable as native editing features are integrated.",
    );
    let line_numbers = checkbox(
        &editor,
        "Show line numbers (planned)",
        preferences.editor.show_line_numbers,
        false,
    );
    line_numbers.set_tooltip_text(Some("Not yet connected to the native code editor."));
    let wrap = checkbox(
        &editor,
        "Word wrap (planned)",
        preferences.editor.word_wrap,
        false,
    );
    wrap.set_tooltip_text(Some("Not yet connected to the native code editor."));
    notebook.append_page(&editor, Some(&gtk4::Label::new(Some("Editor"))));

    let debugging = section();
    heading(&debugging, "Debugging", "Source-level Hydrogen debugging is not yet implemented. These options will be enabled with the debugger adapter.");
    checkbox(
        &debugging,
        "Break on unhandled exceptions",
        preferences.debugging.stop_on_exception,
        false,
    );
    checkbox(
        &debugging,
        "Show debug session output",
        preferences.debugging.show_debug_output,
        false,
    );
    notebook.append_page(&debugging, Some(&gtk4::Label::new(Some("Debugging"))));

    let build = section();
    heading(
        &build,
        "Build & Run",
        "Build preferences will be enabled when they are connected to the build runner.",
    );
    checkbox(
        &build,
        "Save modified files before building",
        preferences.build.save_before_build,
        false,
    );
    checkbox(
        &build,
        "Show output after building",
        preferences.build.show_output,
        false,
    );
    notebook.append_page(&build, Some(&gtk4::Label::new(Some("Build & Run"))));

    let hydrogen = section();
    heading(
        &hydrogen,
        "Hydrogen Compiler",
        "Choose the Hydrogen executable used for Build, Run and native UI previews. The executable does not need to be on PATH.",
    );
    let path_label = gtk4::Label::new(Some("Compiler executable"));
    path_label.set_xalign(0.0);
    hydrogen.append(&path_label);
    let path_entry = gtk4::Entry::new();
    path_entry.set_hexpand(true);
    path_entry.set_placeholder_text(Some("Automatic: QUARK_HYDROGEN_COMPILER, then hy on PATH"));
    path_entry.set_text(preferences.hydrogen.executable.as_deref().unwrap_or(""));
    let path_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    path_row.append(&path_entry);
    let browse = gtk4::Button::with_label("Browse…");
    path_row.append(&browse);
    hydrogen.append(&path_row);
    let message = gtk4::Label::new(Some(
        "Leave blank for automatic discovery. Select the executable itself, not the project directory.",
    ));
    message.set_wrap(true);
    message.set_xalign(0.0);
    message.add_css_class("dim-label");
    hydrogen.append(&message);
    let controls = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let save_compiler = gtk4::Button::with_label("Save compiler path");
    let test_compiler = gtk4::Button::with_label("Test compiler");
    let automatic = gtk4::Button::with_label("Use automatic discovery");
    controls.append(&save_compiler);
    controls.append(&test_compiler);
    controls.append(&automatic);
    hydrogen.append(&controls);
    let feedback = gtk4::Label::new(None);
    feedback.set_wrap(true);
    feedback.set_selectable(true);
    feedback.set_xalign(0.0);
    hydrogen.append(&feedback);
    browse.connect_clicked({
        let entry = path_entry.clone();
        move |_| {
            if let Some(selected) = rfd::FileDialog::new()
                .set_title("Select Hydrogen compiler executable")
                .pick_file()
            {
                entry.set_text(&selected.to_string_lossy());
            }
        }
    });
    save_compiler.connect_clicked({
        let state = state.clone();
        let file = path.clone();
        let entry = path_entry.clone();
        let feedback = feedback.clone();
        move |_| {
            let raw = entry.text().trim().to_owned();
            let candidate = if raw.is_empty() { None } else { Some(raw) };
            if let Some(ref value) = candidate {
                if let Err(error) = crate::native_compiler::validate_explicit(value) {
                    feedback.set_text(&error);
                    return;
                }
            }
            let mut prefs = state.borrow_mut();
            prefs.hydrogen.executable = candidate;
            match save(&file, &prefs) {
                Ok(()) => feedback
                    .set_text("Compiler preference saved. New builds will use this executable."),
                Err(error) => {
                    feedback.set_text(&format!("Could not save compiler preference: {error}"))
                }
            }
        }
    });
    automatic.connect_clicked({
        let state = state.clone();
        let file = path.clone();
        let entry = path_entry.clone();
        let feedback = feedback.clone();
        move |_| {
            let mut prefs = state.borrow_mut();
            prefs.hydrogen.executable = None;
            match save(&file, &prefs) {
                Ok(()) => {
                    entry.set_text("");
                    feedback.set_text("Automatic compiler discovery enabled.");
                }
                Err(error) => {
                    feedback.set_text(&format!("Could not save compiler preference: {error}"))
                }
            }
        }
    });
    test_compiler.connect_clicked({
        let entry = path_entry.clone();
        let feedback = feedback.clone();
        let file = path.clone();
        move |_| {
            // Resolve the current entry, including unsaved edits.
            let raw = entry.text().trim().to_owned();
            let resolved = if raw.is_empty() {
                crate::native_compiler::resolve_at(&file)
            } else {
                crate::native_compiler::validate_explicit(&raw).map(|_| raw)
            };
            let executable = match resolved {
                Ok(value) => value,
                Err(error) => {
                    feedback.set_text(&error);
                    return;
                }
            };
            feedback.set_text("Checking Hydrogen executable…");
            let (sender, receiver) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let result = std::process::Command::new(&executable)
                    .arg("--version")
                    .output()
                    .map(|output| {
                        let detail = String::from_utf8_lossy(if !output.stdout.is_empty() {
                            &output.stdout
                        } else {
                            &output.stderr
                        })
                        .trim()
                        .to_owned();
                        if output.status.success() {
                            format!("Compiler started successfully: {detail}")
                        } else {
                            format!("Compiler exited with {}: {detail}", output.status)
                        }
                    })
                    .unwrap_or_else(|error| format!("Unable to execute Hydrogen: {error}"));
                let _ = sender.send(result);
            });
            let feedback = feedback.clone();
            gtk4::glib::timeout_add_local(std::time::Duration::from_millis(80), move || {
                match receiver.try_recv() {
                    Ok(result) => {
                        feedback.set_text(&result);
                        gtk4::glib::ControlFlow::Break
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => gtk4::glib::ControlFlow::Continue,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        feedback.set_text("Compiler check did not return a result.");
                        gtk4::glib::ControlFlow::Break
                    }
                }
            });
        }
    });
    notebook.append_page(&hydrogen, Some(&gtk4::Label::new(Some("Hydrogen"))));

    let general = section();
    heading(
        &general,
        "Application",
        "Settings are stored separately from project and workspace files.",
    );
    let path_label = gtk4::Label::new(Some(&format!("Preferences file:\n{}", path.display())));
    path_label.set_wrap(true);
    path_label.set_selectable(true);
    path_label.set_xalign(0.0);
    general.append(&path_label);
    let reset = gtk4::Button::with_label("Reset appearance to system default");
    {
        let state = state.clone();
        let path = path.clone();
        let themes = themes.clone();
        reset.connect_clicked(move |_| {
            let mut prefs = state.borrow_mut();
            prefs.appearance.theme = ThemeChoice::System;
            apply_theme(&prefs);
            themes.set_selected(0);
            if let Err(error) = save(&path, &prefs) {
                eprintln!("Quark: could not save preferences: {error}");
            }
        });
    }
    general.append(&reset);
    notebook.append_page(&general, Some(&gtk4::Label::new(Some("Application"))));
    window.set_child(Some(&notebook));
    window.present();
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preferences_roundtrip_and_old_files_use_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("prefs.json");
        let mut prefs = load(&file);
        assert_eq!(prefs.appearance.theme, ThemeChoice::System);
        prefs.appearance.theme = ThemeChoice::Light;
        save(&file, &prefs).unwrap();
        assert_eq!(load(&file).appearance.theme, ThemeChoice::Light);
        fs::write(&file, r#"{"appearance":{"theme":"dark"}}"#).unwrap();
        assert_eq!(load(&file).appearance.theme, ThemeChoice::Dark);
        assert!(load(&file).editor.show_line_numbers);
    }
}
