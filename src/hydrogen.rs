//! Command planning for the Hydrogen SDK/self-hosted CLI.
//! `build` always consumes a .hyproj; `compile` consumes one .hy file.
use crate::project;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildScope {
    Project,
    CurrentFile,
}

#[derive(Debug, Clone)]
pub struct BuildPlan {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub output: PathBuf,
    pub output_is_directory: bool,
    pub description: String,
}

fn extension_for(kind: &str) -> &'static str {
    match kind {
        "windows" => "exe",
        "efi" => "efi",
        "kernel" => "bin",
        "exec" => "exec",
        "native" => "aun",
        _ => "",
    }
}

pub fn build_plan(
    compiler: &str,
    project_path: Option<&Path>,
    active_path: Option<&Path>,
    scope: BuildScope,
) -> Result<BuildPlan, String> {
    if compiler.trim().is_empty() {
        return Err("Set the Hydrogen compiler in Settings".into());
    }
    let (path, verb, kind) = match scope {
        BuildScope::Project => {
            let path =
                project_path.ok_or("Open or select a Hydrogen project (.hyproj) to build")?;
            if path.extension().and_then(|s| s.to_str()) != Some("hyproj") {
                return Err("Project build requires a .hyproj manifest".into());
            }
            let p = project::load(path);
            (path, "build", p.kind)
        }
        BuildScope::CurrentFile => {
            let path = active_path.ok_or("Open a .hy source file for Shift+Build")?;
            if path.extension().and_then(|s| s.to_str()) != Some("hy") {
                return Err(
                    "Shift+Build compiles one .hy source file; open a .hy tab first".into(),
                );
            }
            (path, "compile", "exe".into())
        }
    };
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let cwd = path
        .parent()
        .ok_or("Cannot resolve project directory")?
        .to_path_buf();
    let stem = path
        .file_stem()
        .ok_or("Invalid Hydrogen filename")?
        .to_string_lossy();
    let base = cwd.join(".quark").join("build").join(stem.as_ref());
    let ext = if scope == BuildScope::Project {
        extension_for(&kind)
    } else if cfg!(windows) {
        "exe"
    } else {
        ""
    };
    let output = if ext.is_empty() || kind == "os" {
        base
    } else {
        base.with_extension(ext)
    };
    Ok(BuildPlan {
        program: compiler.to_owned(),
        args: vec![
            verb.into(),
            path.to_string_lossy().into_owned(),
            "-o".into(),
            output.to_string_lossy().into_owned(),
        ],
        cwd,
        description: if scope == BuildScope::Project {
            format!("Building project {}", path.display())
        } else {
            format!("Compiling file {}", path.display())
        },
        output,
        output_is_directory: scope == BuildScope::Project && kind == "os",
    })
}

/// Help output is advisory; capabilities remain editable for custom wrappers.
pub fn detect_verbs(help: &str) -> Vec<String> {
    ["build", "compile", "check", "run"]
        .into_iter()
        .filter(|verb| {
            help.split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
                .any(|word| word == *verb)
        })
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn help_detection_matches_whole_command_names() {
        assert_eq!(
            detect_verbs("Commands: build | compile | check | run"),
            vec!["build", "compile", "check", "run"]
        );
        assert!(detect_verbs("building and runtime").is_empty());
    }
    #[test]
    fn project_build_passes_required_output() {
        let plan = build_plan(
            "hydrogen-stage1",
            Some(Path::new("project/A.hyproj")),
            Some(Path::new("project/X.hy")),
            BuildScope::Project,
        )
        .unwrap();
        assert_eq!(plan.args[0], "build");
        assert_eq!(plan.args[2], "-o");
        assert!(plan.args[1].ends_with("A.hyproj"));
        assert!(plan.args[3].contains(".quark"));
    }
    #[test]
    fn shift_build_compiles_only_active_file() {
        let plan = build_plan(
            "hy",
            Some(Path::new("A.hyproj")),
            Some(Path::new("src/MyFile.hy")),
            BuildScope::CurrentFile,
        )
        .unwrap();
        assert_eq!(plan.args[0], "compile");
        assert!(plan.args[1].ends_with("MyFile.hy"));
        assert_eq!(plan.args[2], "-o");
    }
    #[test]
    fn shift_build_rejects_a_manifest() {
        let err = build_plan(
            "hy",
            Some(Path::new("A.hyproj")),
            Some(Path::new("A.hyproj")),
            BuildScope::CurrentFile,
        )
        .unwrap_err();
        assert!(err.contains(".hy source"));
    }
    #[test]
    fn project_build_does_not_fall_back_to_a_source_file() {
        assert!(build_plan("hy", None, Some(Path::new("A.hy")), BuildScope::Project).is_err());
    }
    #[test]
    fn os_projects_get_a_directory_output() {
        // A malformed or missing manifest defaults to non-OS, so inspect suffix only.
        assert_eq!(extension_for("os"), "");
        assert_eq!(extension_for("kernel"), "bin");
    }
}
