//! Launch the real Hydrogen UI runtime in a separate OS-managed window.
//!
//! The embedded Quark design surface uses portable native widgets for
//! editing, but only the Hydrogen SDK's hyui runtime reproduces the final
//! GTK/libadwaita, AppKit or WinUI app window and native layout engine.
//! Never claim an embedded mock is pixel-identical to the built app.
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewKind {
    /// Visual inspection only: no Hydrogen backend is built or attached.
    Window,
    /// Build the active .hyproj and run it through Hydrogen's UI backend.
    Application,
}

/// The default OS Window action must launch a working application. A
/// separate, explicitly named layout-only action keeps no-backend previews
/// available for visual comparisons without presenting them as built apps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewButton {
    OsWindow,
    LayoutOnly,
}

pub fn kind_for_button(button: PreviewButton) -> PreviewKind {
    match button {
        PreviewButton::OsWindow => PreviewKind::Application,
        PreviewButton::LayoutOnly => PreviewKind::Window,
    }
}

/// Hydrogen's native runtime changes its working directory to the project
/// before it runs the compiler. Passing only `hy` through its --compiler
/// argument is unsafe: the runtime converts that bare name to a path under
/// the project instead of searching PATH. Resolve it here to an absolute
/// executable path, without executing a shell.
fn compiler_executable(raw: &str, path: Option<&OsStr>) -> Result<PathBuf, String> {
    let value = raw.trim();
    if value.is_empty() {
        return Err("Set the Hydrogen compiler executable in Quark Settings".into());
    }
    let input = Path::new(value);
    if input.is_absolute() || input.components().count() > 1 {
        return fs::canonicalize(input)
            .map_err(|error| format!("Cannot find Hydrogen compiler {value}: {error}"));
    }
    let candidates = if cfg!(windows) && input.extension().is_none() {
        vec![value.to_owned(), format!("{value}.exe")]
    } else {
        vec![value.to_owned()]
    };
    for directory in path.into_iter().flat_map(std::env::split_paths) {
        for candidate in &candidates {
            let full = directory.join(candidate);
            if full.is_file() {
                return fs::canonicalize(&full).map_err(|error| error.to_string());
            }
        }
    }
    Err(format!(
        "Hydrogen compiler '{value}' not found in PATH. Configure its absolute path in Quark Settings."
    ))
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
        let path_env = std::env::var_os("PATH");
        let executable = compiler_executable(compiler, path_env.as_deref())?;
        process
            .arg("--source")
            .arg(project)
            .arg("--compiler")
            .arg(executable);
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
        assert_eq!(
            std::path::PathBuf::from(&args[1]),
            document.canonicalize().unwrap(),
        );
    }

    #[test]
    fn application_preview_compiles_project_using_native_runtime_not_a_shell() {
        let directory = tempfile::tempdir().unwrap();
        let document = directory.path().join("Studio.hyui");
        let manifest = directory.path().join("Studio.hyproj");
        std::fs::write(&document, "<Window version=\"1\"/>").unwrap();
        std::fs::write(&manifest, "format = 2").unwrap();
        let compiler = directory.path().join(if cfg!(windows) {
            "hydrogen-stage1.exe"
        } else {
            "hydrogen-stage1"
        });
        std::fs::write(&compiler, "test compiler").unwrap();
        let process = command(
            "hyui",
            &document,
            Some(&manifest),
            compiler.to_str().unwrap(),
            PreviewKind::Application,
        )
        .unwrap();
        let args = process
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(args[2], "--source");
        assert_eq!(args[4], "--compiler");
        assert_eq!(PathBuf::from(&args[5]), compiler.canonicalize().unwrap());
        let canonical_directory = directory.path().canonicalize().unwrap();
        assert_eq!(
            process.get_current_dir(),
            Some(canonical_directory.as_path())
        );
    }

    #[test]
    fn os_window_is_interactive_and_layout_preview_is_explicitly_read_only() {
        assert_eq!(
            kind_for_button(PreviewButton::OsWindow),
            PreviewKind::Application
        );
        assert_eq!(
            kind_for_button(PreviewButton::LayoutOnly),
            PreviewKind::Window
        );
    }

    #[test]
    fn compiler_name_on_path_resolves_to_absolute_path_before_runtime_changes_cwd() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory
            .path()
            .join(if cfg!(windows) { "hy.exe" } else { "hy" });
        std::fs::write(&executable, "test compiler").unwrap();
        let resolved = compiler_executable("hy", Some(directory.path().as_os_str())).unwrap();
        assert_eq!(resolved, executable.canonicalize().unwrap());
        assert!(resolved.is_absolute());
    }

    #[test]
    fn missing_compiler_is_an_error_not_an_unresponsive_window() {
        let directory = tempfile::tempdir().unwrap();
        let result = compiler_executable(
            "hydrogen-not-installed-test-only",
            Some(directory.path().as_os_str()),
        );
        assert!(result.unwrap_err().contains("not found in PATH"));
    }

    #[test]
    fn native_preview_passes_paths_with_spaces_as_single_arguments() {
        let directory = tempfile::tempdir().unwrap();
        let view = directory.path().join("My UI Layout.hyui");
        std::fs::write(&view, "<Window version=\"1\"/>").unwrap();
        let process = command("hyui", &view, None, "hy", PreviewKind::Window).unwrap();
        let args = process
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(args.len(), 2);
        assert_eq!(
            std::path::PathBuf::from(&args[1]),
            view.canonicalize().unwrap(),
        );
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
