//! GTK platform settings editor backed by the cross-platform preferences schema.
use gtk4::prelude::*;
use std::{cell::RefCell, path::PathBuf, rc::Rc};
use crate::native_settings::{load, Preferences, ThemeChoice};
fn save(path: &std::path::Path, preferences: &Preferences) -> Result<(), String> {
    crate::native_settings::save(path, preferences)
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
        "Editor preferences apply to newly opened native source editors.",
    );
    let line_numbers = checkbox(
        &editor,
        "Show line numbers",
        preferences.editor.show_line_numbers,
        true,
    );
    {
        let state = state.clone();
        let path = path.clone();
        line_numbers.connect_toggled(move |button| {
            state.borrow_mut().editor.show_line_numbers = button.is_active();
            if let Err(error) = save(&path, &state.borrow()) {
                eprintln!("Quark: could not save editor preference: {error}");
            }
        });
    }
    let wrap = checkbox(&editor, "Word wrap", preferences.editor.word_wrap, true);
    {
        let state = state.clone();
        let path = path.clone();
        wrap.connect_toggled(move |button| {
            state.borrow_mut().editor.word_wrap = button.is_active();
            if let Err(error) = save(&path, &state.borrow()) {
                eprintln!("Quark: could not save editor preference: {error}");
            }
        });
    }
    let tab_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    tab_row.append(&gtk4::Label::new(Some("Tab width (1–16)")));
    let tab_width = gtk4::SpinButton::with_range(1.0, 16.0, 1.0);
    tab_width.set_value(f64::from(preferences.editor.tab_width));
    {
        let state = state.clone();
        let path = path.clone();
        tab_width.connect_value_changed(move |spin| {
            state.borrow_mut().editor.tab_width = spin.value_as_int() as u8;
            if let Err(error) = save(&path, &state.borrow()) {
                eprintln!("Quark: could not save tab width: {error}");
            }
        });
    }
    tab_row.append(&tab_width);
    editor.append(&tab_row);
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
        "Changes take effect on the next build or run.",
    );
    let save_before = checkbox(
        &build,
        "Save modified files before building",
        preferences.build.save_before_build,
        true,
    );
    {
        let state = state.clone();
        let path = path.clone();
        save_before.connect_toggled(move |button| {
            state.borrow_mut().build.save_before_build = button.is_active();
            if let Err(error) = save(&path, &state.borrow()) {
                eprintln!("Quark: could not save build preference: {error}");
            }
        });
    }
    let show_output = checkbox(
        &build,
        "Show output after building",
        preferences.build.show_output,
        true,
    );
    {
        let state = state.clone();
        let path = path.clone();
        show_output.connect_toggled(move |button| {
            state.borrow_mut().build.show_output = button.is_active();
            if let Err(error) = save(&path, &state.borrow()) {
                eprintln!("Quark: could not save build preference: {error}");
            }
        });
    }
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
    use std::fs;
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
