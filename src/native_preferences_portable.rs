//! AppKit/WinUI Settings window built from naui's cross-platform native
//! controls. Shares the GTK native-preferences.json file and Hydrogen resolver.
#![cfg(not(target_os = "linux"))]

use naui::{Orientation, Theme, Ui};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
};

fn read(path: &Path) -> Value {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}))
}

fn write(path: &Path, value: &Value) -> Result<(), String> {
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder).map_err(|err| err.to_string())?;
    }
    let bytes = serde_json::to_vec_pretty(value).map_err(|err| err.to_string())?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, bytes).map_err(|err| err.to_string())?;
    fs::rename(&temporary, path).map_err(|err| err.to_string())
}

fn update(path: &Path, section: &str, field: &str, value: Value) -> Result<(), String> {
    let mut prefs = read(path);
    if !prefs.get(section).is_some_and(Value::is_object) {
        prefs[section] = json!({});
    }
    prefs[section][field] = value;
    write(path, &prefs)
}

/// Apply a saved theme before the editor window opens. The native backend
/// retains platform ownership of colours, focus, fonts and window chrome.
pub fn apply_saved_theme(ui: &Ui, path: &Path) {
    let saved = read(path);
    let theme = saved
        .get("appearance")
        .and_then(|v| v.get("theme"))
        .and_then(Value::as_str)
        .unwrap_or("system");
    let _ = ui.set_theme(match theme {
        "light" => Theme::Light,
        "dark" => Theme::Dark,
        _ => Theme::System,
    });
}

/// Create an OS-native settings window; keep it independent of the active
/// project so manually chosen compiler paths persist across all projects.
pub fn show(ui: &Ui, preferences_file: PathBuf) -> naui::Result<()> {
    let window = ui.window("Quark Settings", 680.0, 465.0)?;
    let root = ui.stack(Orientation::Vertical)?;
    root.set_spacing(12.0);
    root.append(&ui.label("Quark IDE · Settings")?);
    root.append(&ui.label("Appearance")?);
    let themes = ui.stack(Orientation::Horizontal)?;
    themes.set_spacing(8.0);
    for (name, theme, persisted) in [
        ("System", Theme::System, "system"),
        ("Light", Theme::Light, "light"),
        ("Dark", Theme::Dark, "dark"),
    ] {
        let button = ui.button(name)?;
        let ui = ui.clone();
        let file = preferences_file.clone();
        button.on_click(move || {
            if let Err(error) = ui.set_theme(theme) {
                eprintln!("Quark: could not apply theme: {error}");
            }
            if let Err(error) = update(&file, "appearance", "theme", json!(persisted)) {
                eprintln!("Quark: cannot save theme: {error}");
            }
        });
        themes.append(&button);
    }
    root.append(&themes);
    root.append(&ui.label("Hydrogen Compiler")?);
    root.append(&ui.label(
        "Enter the absolute path of the Hydrogen executable. Leave blank for automatic discovery.",
    )?);
    let selection = ui.text_input("")?;
    let saved = read(&preferences_file)
        .get("hydrogen")
        .and_then(|v| v.get("executable"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    selection.set_text(&saved);
    selection.set_placeholder("Path to hydrogen-stage1 or hy.exe");
    root.append(&selection);
    let current = Rc::new(RefCell::new(saved));
    selection.on_change({
        let current = current.clone();
        move |text| *current.borrow_mut() = text.to_owned()
    });
    let result = ui.label("")?;
    let actions = ui.stack(Orientation::Horizontal)?;
    actions.set_spacing(8.0);
    let save_button = ui.button("Save compiler path")?;
    save_button.on_click({
        let input = current.clone();
        let file = preferences_file.clone();
        let status = result.clone();
        move || {
            let input = input.borrow().trim().to_owned();
            if !input.is_empty() {
                if let Err(error) = crate::native_compiler::validate_explicit(&input) {
                    status.set_text(&error);
                    return;
                }
            }
            let value = if input.is_empty() {
                Value::Null
            } else {
                json!(input)
            };
            match update(&file, "hydrogen", "executable", value) {
                Ok(()) => status.set_text("Compiler path saved. Builds use it immediately."),
                Err(error) => status.set_text(&format!("Cannot save compiler: {error}")),
            }
        }
    });
    actions.append(&save_button);
    let probe = ui.button("Test executable")?;
    probe.on_click({
        let input = current.clone();
        let file = preferences_file.clone();
        let status = result.clone();
        let ui = ui.clone();
        move || {
            let path = input.borrow().trim().to_owned();
            let compiler = if path.is_empty() {
                crate::native_compiler::resolve_at(&file)
            } else {
                crate::native_compiler::validate_explicit(&path).map(|()| path)
            };
            let compiler = match compiler {
                Ok(v) => v,
                Err(error) => {
                    status.set_text(&error);
                    return;
                }
            };
            status.set_text("Checking Hydrogen executable…");
            let sender = ui.tasks().channel({
                let status = status.clone();
                move |message: String| status.set_text(&message)
            });
            std::thread::spawn(move || {
                let message = match std::process::Command::new(&compiler)
                    .arg("--version")
                    .output()
                {
                    Ok(output) => {
                        let text = String::from_utf8_lossy(if !output.stdout.is_empty() {
                            &output.stdout
                        } else {
                            &output.stderr
                        })
                        .trim()
                        .to_owned();
                        if output.status.success() {
                            format!("Hydrogen compiler is working: {text}")
                        } else {
                            format!("Hydrogen exited with {}: {text}", output.status)
                        }
                    }
                    Err(error) => format!("Cannot execute {}: {error}", compiler),
                };
                let _ = sender.send(message);
            });
        }
    });
    actions.append(&probe);
    root.append(&actions);
    root.append(&result);
    root.append(&ui.label("Build, Run, Format and Check use the selected executable. Native debugging still requires a debugger adapter.")?);
    window.set_child(&root);
    window.show();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn updating_compiler_preserves_other_preferences() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("native-preferences.json");
        write(
            &file,
            &json!({"appearance":{"theme":"dark"},"editor":{"tab_width":4}}),
        )
        .unwrap();
        update(
            &file,
            "hydrogen",
            "executable",
            json!("/tmp/hydrogen-stage1"),
        )
        .unwrap();
        let loaded = read(&file);
        assert_eq!(loaded["appearance"]["theme"], "dark");
        assert_eq!(loaded["editor"]["tab_width"], 4);
        assert_eq!(loaded["hydrogen"]["executable"], "/tmp/hydrogen-stage1");
    }
}
