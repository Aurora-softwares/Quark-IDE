//! Native IDE build, run, clean and binary publishing commands.
//! These are toolkit-independent: the UI dispatches work on a worker thread.

use crate::hydrogen::{self, BuildPlan, BuildScope};
use std::{
    fs, io,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Task {
    Build,
    Rebuild,
    Clean,
    RunWithoutDebugging,
    Publish,
}

impl Task {
    pub fn label(self) -> &'static str {
        match self {
            Self::Build => "Build Solution",
            Self::Rebuild => "Rebuild Solution",
            Self::Clean => "Clean Solution",
            Self::RunWithoutDebugging => "Start Without Debugging",
            Self::Publish => "Bundle / Publish Binaries",
        }
    }
}

#[derive(Debug)]
pub struct ResultLog {
    pub success: bool,
    pub text: String,
}

impl ResultLog {
    fn ok(message: impl Into<String>) -> Self {
        Self {
            success: true,
            text: message.into(),
        }
    }

    fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            text: message.into(),
        }
    }
}

fn output_path(plan: &BuildPlan) -> Result<&Path, String> {
    if plan.output_is_directory {
        return Err("Publishing directory-style OS images is not supported yet".into());
    }
    if !plan.output.is_file() {
        return Err(format!(
            "Build the application first. Binary not found at {}",
            plan.output.display()
        ));
    }
    let metadata = fs::symlink_metadata(&plan.output).map_err(|e| e.to_string())?;
    if metadata.file_type().is_symlink() {
        return Err("Refusing to package a symbolic link as a build output".into());
    }
    Ok(&plan.output)
}

pub fn publish_binary(plan: &BuildPlan) -> Result<PathBuf, String> {
    let binary = output_path(plan)?;
    let project_name = plan
        .cwd
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .replace(' ', "-");
    let output_name = binary
        .file_name()
        .ok_or("Build output has no file name")?
        .to_string_lossy()
        .to_string();
    let output_dir = plan.cwd.join(".quark").join("publish");
    fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;

    let archive = output_dir.join(format!("{project_name}-binaries.zip"));
    let temporary = output_dir.join(format!("{project_name}-binaries.zip.tmp"));
    let result = (|| -> Result<(), String> {
        let mut source = fs::File::open(binary).map_err(|e| e.to_string())?;
        let file = fs::File::create(&temporary).map_err(|e| e.to_string())?;
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zip.start_file(output_name, options)
            .map_err(|e| e.to_string())?;
        io::copy(&mut source, &mut zip).map_err(|e| e.to_string())?;
        zip.finish().map_err(|e| e.to_string())?;
        fs::rename(&temporary, &archive).map_err(|e| e.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map(|()| archive)
}

pub fn clean_generated(plan: &BuildPlan) -> Result<(), String> {
    let generated = plan.cwd.join(".quark").join("build");
    if !generated.exists() {
        return Ok(());
    }
    if fs::symlink_metadata(&generated)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("Refusing to clean a symbolic-link build directory".into());
    }
    let canonical_project = plan.cwd.canonicalize().map_err(|e| e.to_string())?;
    let canonical_build = generated.canonicalize().map_err(|e| e.to_string())?;
    if !canonical_build.starts_with(&canonical_project) {
        return Err("Refusing to clean a build directory outside the project".into());
    }
    fs::remove_dir_all(generated).map_err(|e| e.to_string())
}

pub fn run(task: Task, compiler: &str, project: Option<&Path>) -> ResultLog {
    let Some(project) = project else {
        return ResultLog::error("Open a Hydrogen project before building or running.");
    };
    let plan = match hydrogen::build_plan(compiler, Some(project), None, BuildScope::Project) {
        Ok(plan) => plan,
        Err(error) => return ResultLog::error(error),
    };
    let result = match task {
        Task::Clean => clean_generated(&plan).map(|()| "Generated build files removed".to_owned()),
        Task::Publish => {
            // Publishing must build the current sources, rather than silently
            // packaging a potentially stale executable from an earlier build.
            let output_parent = if plan.output_is_directory {
                &plan.output
            } else {
                plan.output.parent().unwrap_or_else(|| Path::new("."))
            };
            fs::create_dir_all(output_parent)
                .map_err(|e| format!("Cannot create build directory: {e}"))
                .and_then(|()| {
                    let built = Command::new(&plan.program)
                        .args(&plan.args)
                        .current_dir(&plan.cwd)
                        .output()
                        .map_err(|e| format!("Cannot start Hydrogen build: {e}"))?;
                    if !built.status.success() {
                        return Err(format!(
                            "Build failed; nothing was published.\n{}\n{}",
                            String::from_utf8_lossy(&built.stdout),
                            String::from_utf8_lossy(&built.stderr),
                        ));
                    }
                    publish_binary(&plan).map(|path| {
                        format!(
                            "Build succeeded. Binary archive created: {}",
                            path.display()
                        )
                    })
                })
        }
        Task::Build | Task::Rebuild | Task::RunWithoutDebugging => {
            if task == Task::Rebuild {
                if let Err(error) = clean_generated(&plan) {
                    return ResultLog::error(format!("Rebuild could not clean outputs: {error}"));
                }
            }
            let args = if task == Task::RunWithoutDebugging {
                vec!["run".into(), project.to_string_lossy().into_owned()]
            } else {
                plan.args.clone()
            };
            let output_parent = if plan.output_is_directory {
                &plan.output
            } else {
                plan.output.parent().unwrap_or_else(|| Path::new("."))
            };
            if task != Task::RunWithoutDebugging {
                if let Err(error) = fs::create_dir_all(output_parent) {
                    return ResultLog::error(format!("Cannot create build directory: {error}"));
                }
            }
            let output = Command::new(&plan.program)
                .args(&args)
                .current_dir(&plan.cwd)
                .output();
            match output {
                Ok(process) => {
                    let message = format!(
                        "Hydrogen {} {}\n{}\n{}\nProcess exit code: {}",
                        if task == Task::RunWithoutDebugging {
                            "run"
                        } else {
                            "build"
                        },
                        project.display(),
                        String::from_utf8_lossy(&process.stdout),
                        String::from_utf8_lossy(&process.stderr),
                        process.status.code().unwrap_or(-1),
                    );
                    if process.status.success() {
                        Ok(message)
                    } else {
                        Err(message)
                    }
                }
                Err(error) => Err(format!("Could not execute Hydrogen compiler: {error}")),
            }
        }
    };
    match result {
        Ok(message) => ResultLog::ok(message),
        Err(message) => ResultLog::error(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn sample_plan(dir: &Path) -> BuildPlan {
        BuildPlan {
            program: "hy".into(),
            args: vec![],
            cwd: dir.to_path_buf(),
            output: dir.join(".quark/build/Sample"),
            output_is_directory: false,
            description: "Sample build".into(),
        }
    }

    #[test]
    fn publish_requires_a_successful_fresh_build_not_a_stale_binary() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("Demo.hyproj");
        fs::write(&project, "type='linux'\nsources=['Main.hy']").unwrap();
        let plan = hydrogen::build_plan(
            "a-compiler-that-does-not-exist-quark",
            Some(&project),
            None,
            BuildScope::Project,
        )
        .unwrap();
        fs::create_dir_all(plan.output.parent().unwrap()).unwrap();
        fs::write(&plan.output, b"old stale binary").unwrap();
        let result = run(
            Task::Publish,
            "a-compiler-that-does-not-exist-quark",
            Some(&project),
        );
        assert!(!result.success);
        assert!(result.text.contains("Cannot start Hydrogen build"));
        assert!(!dir.path().join(".quark/publish").exists());
    }

    #[test]
    fn publish_without_project_does_not_create_any_artifacts() {
        let result = run(Task::Publish, "hy", None);
        assert!(!result.success);
        assert!(result.text.contains("Open a Hydrogen project"));
    }

    #[test]
    fn publishes_actual_binary_as_zip_with_original_filename() {
        let dir = tempfile::tempdir().unwrap();
        let plan = sample_plan(dir.path());
        fs::create_dir_all(plan.output.parent().unwrap()).unwrap();
        fs::write(&plan.output, b"binary bytes").unwrap();
        let archive = publish_binary(&plan).unwrap();
        let file = fs::File::open(archive).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();
        let mut output = Vec::new();
        zip.by_name("Sample")
            .unwrap()
            .read_to_end(&mut output)
            .unwrap();
        assert_eq!(output, b"binary bytes");
    }

    #[test]
    fn clean_removes_generated_files_not_source_documents() {
        let dir = tempfile::tempdir().unwrap();
        let plan = sample_plan(dir.path());
        fs::create_dir_all(plan.output.parent().unwrap()).unwrap();
        fs::write(&plan.output, b"generated").unwrap();
        fs::write(dir.path().join("Program.hy"), "keep").unwrap();
        clean_generated(&plan).unwrap();
        assert!(!plan.output.exists());
        assert_eq!(
            fs::read_to_string(dir.path().join("Program.hy")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn publish_refuses_missing_output_instead_of_claiming_success() {
        let dir = tempfile::tempdir().unwrap();
        let plan = sample_plan(dir.path());
        assert!(publish_binary(&plan)
            .unwrap_err()
            .contains("Build the application first"));
        assert!(!dir.path().join(".quark/publish").exists());
    }

    #[test]
    fn commands_refuse_to_run_with_no_project_or_compiler() {
        let no_project = run(Task::Build, "hy", None);
        assert!(!no_project.success);
        let dir = tempfile::tempdir().unwrap();
        let manifest = dir.path().join("App.hyproj");
        fs::write(&manifest, "type='linux'").unwrap();
        let no_compiler = run(Task::Build, "", Some(&manifest));
        assert!(!no_compiler.success);
    }

    #[cfg(unix)]
    #[test]
    fn clean_refuses_generated_symlink_to_external_directory() {
        let dir = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".quark")).unwrap();
        std::os::unix::fs::symlink(external.path(), dir.path().join(".quark/build")).unwrap();
        let plan = sample_plan(dir.path());
        assert!(clean_generated(&plan).is_err());
    }
}
