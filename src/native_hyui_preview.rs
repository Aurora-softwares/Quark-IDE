//! Launch the real Hydrogen UI runtime in a separate OS-managed window.
//!
//! The embedded Quark design surface uses portable native widgets for
//! editing, but only the Hydrogen SDK's hyui runtime reproduces the final
//! GTK/libadwaita, AppKit or WinUI app window and native layout engine.
//! Never claim an embedded mock is pixel-identical to the built app.
use std::{path::Path, process::Command};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewKind {
    /// Exactly the saved XML's initial UI state, without backend events.
    Window,
    /// Build the active .hyproj and run it through Hydrogen's UI backend.
    Application,
}

pub fn command(
    runtime: &str,
    view: &Path,
    project: Option<&Path>,
    compiler: &str,
    kind: PreviewKind,
) -> Result<Command, String> {
    if runtime.trim().is_empty() {
        return Err("Set QUARK_HYUI_RUNTIME to the native Hydrogen UI executable".into());
    }
    if view.extension().and_then(|value| value.to_str()) != Some("hyui") {
        return Err("Native Hydrogen preview requires a .hyui layout".into());
    }
    let layout = view.canonicalize().map_err(|e| e.to_string())?;
    let mut process = Command::new(runtime);
    process
        .arg("--document")
        .arg(&layout)
        .current_dir(layout.parent().unwrap_or_else(|| Path::new(".")));
    if kind == PreviewKind::Application {
        let project =
            project.ok_or("Open a Hydrogen .hyproj project to run UI events with the backend")?;
        if project.extension().and_then(|value| value.to_str()) != Some("hyproj") {
            return Err("UI backend preview requires a Hydrogen .hyproj manifest".into());
        }
        if compiler.trim().is_empty() {
            return Err("Set QUARK_HYDROGEN_COMPILER to the Hydrogen compiler".into());
        }
        let project = project.canonicalize().map_err(|e| e.to_string())?;
        process
            .arg("--source")
            .arg(project)
            .arg("--compiler")
            .arg(compiler);
    }
    Ok(process)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_window_mode_never_silently_builds_a_backend() {
        let directory = tempfile::tempdir().unwrap();
        let document = directory.path().join("Studio.hyui");
        std::fs::write(&document, "<Window version=\"1\"/>").unwrap();
        let process = command("hyui", &document, None, "", PreviewKind::Window).unwrap();
        let args = process
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], "--document");
        assert_eq!(args[1], document.display().to_string());
    }

    #[test]
    fn application_preview_compiles_project_using_native_runtime_not_a_shell() {
        let directory = tempfile::tempdir().unwrap();
        let document = directory.path().join("Studio.hyui");
        let manifest = directory.path().join("Studio.hyproj");
        std::fs::write(&document, "<Window version=\"1\"/>").unwrap();
        std::fs::write(&manifest, "format = 2").unwrap();
        let process = command(
            "hyui",
            &document,
            Some(&manifest),
            "hydrogen-stage1",
            PreviewKind::Application,
        )
        .unwrap();
        let args = process
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(args[2], "--source");
        assert_eq!(args[4..], ["--compiler", "hydrogen-stage1"]);
        assert_eq!(process.get_current_dir(), Some(directory.path()));
    }

    #[test]
    fn invalid_layout_and_missing_backend_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let wrong = directory.path().join("Studio.hyyui");
        std::fs::write(&wrong, "<Window/>").unwrap();
        assert!(command("hyui", &wrong, None, "hy", PreviewKind::Window).is_err());
        let correct = directory.path().join("Studio.hyui");
        std::fs::write(&correct, "<Window/>").unwrap();
        assert!(command("hyui", &correct, None, "hy", PreviewKind::Application).is_err());
    }
}
