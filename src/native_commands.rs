//! Native IDE build, run, clean and binary publishing commands.
//! These are toolkit-independent: the UI dispatches work on a worker thread.

use crate::{
    hydrogen::{self, BuildPlan, BuildScope},
    project,
};
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Task {
    Build,
    Test,
    Check,
    Rebuild,
    Clean,
    RunWithoutDebugging,
    Publish,
}

impl Task {
    pub fn label(self) -> &'static str {
        match self {
            Self::Build => "Build Solution",
            Self::Check => "Check Solution",
            Self::Test => "Run Hydrogen Tests",
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

fn run_target(plan: &BuildPlan, manifest: &Path) -> Result<(PathBuf, bool), String> {
    let selected = project::load(manifest);
    let supported = match selected.kind.as_str() {
        "exe" => true,
        "windows" => cfg!(windows),
        "macos" => cfg!(target_os = "macos"),
        _ => false,
    };
    if !supported {
        return Err(format!(
            "Cannot run Hydrogen target '{}' on this host; select a host desktop executable project",
            selected.kind
        ));
    }
    let artifact = if selected.has_ui && cfg!(windows) {
        append_output_suffix(&plan.output, "exe")
    } else if selected.has_ui && cfg!(target_os = "macos") {
        append_output_suffix(&plan.output, "app")
    } else {
        plan.output.clone()
    };
    Ok((artifact, selected.has_ui))
}

fn append_output_suffix(path: &Path, suffix: &str) -> PathBuf {
    if path
        .extension()
        .is_some_and(|extension| extension == suffix)
    {
        return path.to_path_buf();
    }
    let mut name = path.as_os_str().to_os_string();
    name.push(".");
    name.push(suffix);
    PathBuf::from(name)
}

fn remove_previous_run_artifact(plan: &BuildPlan, artifact: &Path, ui: bool) -> Result<(), String> {
    let build_root = plan.cwd.join(".quark").join("build");
    if !artifact.starts_with(&build_root) || artifact == build_root {
        return Err(
            "Refusing to replace a build artifact outside the generated build directory".into(),
        );
    }
    for directory in [plan.cwd.join(".quark"), build_root] {
        if fs::symlink_metadata(&directory).is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            return Err("Refusing to build through a linked generated directory".into());
        }
    }
    remove_generated_path(artifact)?;
    if ui && cfg!(windows) {
        // WinUI exports a launcher plus a sibling runtime directory. Both
        // must be removed together before packaging a fresh application.
        let mut runtime_name = artifact.as_os_str().to_os_string();
        runtime_name.push(".runtime");
        let runtime = PathBuf::from(runtime_name);
        remove_generated_path(&runtime)?;
    }
    Ok(())
}

fn remove_generated_path(path: &Path) -> Result<(), String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    if metadata.file_type().is_symlink() {
        return Err("Refusing to replace a linked build artifact".into());
    }
    if metadata.is_dir() {
        fs::remove_dir_all(path).map_err(|error| error.to_string())
    } else {
        fs::remove_file(path).map_err(|error| error.to_string())
    }
}

fn launch_built(path: &Path, cwd: &Path, ui: bool) -> Result<String, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        format!(
            "Built application is missing at {}: {error}",
            path.display()
        )
    })?;
    if metadata.file_type().is_symlink() {
        return Err("Refusing to launch a linked build artifact".into());
    }
    if ui && cfg!(target_os = "macos") {
        if !metadata.is_dir() || !path.join("Contents/MacOS/hyui-launcher").is_file() {
            return Err("Native macOS UI bundle is incomplete".into());
        }
        let result = Command::new("open")
            .arg(path)
            .current_dir(cwd)
            .output()
            .map_err(|error| format!("Cannot open native UI application: {error}"))?;
        if !result.status.success() {
            return Err(format!(
                "Native UI launch failed: {}",
                String::from_utf8_lossy(&result.stderr)
            ));
        }
        return Ok(format!(
            "Launched native UI application: {}",
            path.display()
        ));
    }
    if !metadata.is_file() {
        return Err(format!(
            "Build output is not an executable file: {}",
            path.display()
        ));
    }
    if ui {
        let mut child = Command::new(path)
            .current_dir(cwd)
            .spawn()
            .map_err(|error| format!("Cannot start native UI application: {error}"))?;
        let pid = child.id();
        let _ = thread::spawn(move || {
            let _ = child.wait();
        });
        return Ok(format!(
            "Launched native UI application: {} (process {pid})",
            path.display()
        ));
    }
    let result = Command::new(path)
        .current_dir(cwd)
        .output()
        .map_err(|error| format!("Cannot start built application: {error}"))?;
    let log = format!(
        "{}{}Process exit code: {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr),
        result.status.code().unwrap_or(-1)
    );
    if result.status.success() {
        Ok(log)
    } else {
        Err(log)
    }
}

/// Format a saved Hydrogen source file with the configured compiler.
/// The caller must ensure the editor has no unsaved changes before invoking.
pub fn format_document(compiler: &str, source: &Path) -> ResultLog {
    if source.extension().and_then(|extension| extension.to_str()) != Some("hy") {
        return ResultLog::error("Format Document currently supports .hy files.");
    }
    if !source.is_file() {
        return ResultLog::error(format!("Source file does not exist: {}", source.display()));
    }
    let output = Command::new(compiler)
        .env("CLICOLOR_FORCE", "1")
        .env("FORCE_COLOR", "1")
        .env("TERM", "xterm-256color")
        .arg("fmt")
        .arg(source)
        .current_dir(source.parent().unwrap_or_else(|| Path::new(".")))
        .output();
    match output {
        Ok(process) if process.status.success() => ResultLog::ok(format!(
            "Hydrogen formatting succeeded: {}\n{}{}",
            source.display(),
            String::from_utf8_lossy(&process.stdout),
            String::from_utf8_lossy(&process.stderr)
        )),
        Ok(process) => ResultLog::error(format!(
            "Hydrogen formatting failed ({}):\n{}{}",
            process.status,
            String::from_utf8_lossy(&process.stdout),
            String::from_utf8_lossy(&process.stderr)
        )),
        Err(error) => ResultLog::error(format!(
            "Cannot run Hydrogen formatter '{}': {error}",
            compiler
        )),
    }
}

/// Explain a failed process launch in terms of the exact executable,
/// arguments and working directory rather than attributing ENOENT to the
/// Hydrogen project. ENOENT also occurs when a binary's dynamic loader is
/// missing, even if the binary itself exists.
fn launch_diagnostics(program: &str, cwd: &Path, args: &[String], error: &io::Error) -> String {
    let candidate = Path::new(program);
    let executable = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else if candidate.components().count() > 1 {
        cwd.join(candidate)
    } else {
        PathBuf::from(program)
    };
    let cwd_exists = cwd.is_dir();
    let explicit = candidate.is_absolute() || candidate.components().count() > 1;
    let executable_exists = if explicit {
        Some(executable.is_file())
    } else {
        None
    };
    let explanation = if !cwd_exists {
        "The project working directory does not exist or is not a directory."
    } else if executable_exists == Some(false) {
        "The configured Hydrogen executable cannot be found at this path."
    } else if error.kind() == io::ErrorKind::NotFound && executable_exists == Some(true) {
        "The executable exists, but its interpreter or dynamic loader may be missing (check its shebang, file type and shared-library dependencies)."
    } else if error.kind() == io::ErrorKind::NotFound {
        "The compiler command could not be found on PATH. Select the full executable path in Tools → Options → Hydrogen."
    } else {
        "The operating system refused to start the compiler."
    };
    format!(
        "Could not start Hydrogen compiler.\nExecutable: {}\nWorking directory: {}\nArguments: {:?}\nExecutable exists: {}\nWorking directory exists: {}\nSystem error: {}\n{}",
        executable.display(),
        cwd.display(),
        args,
        executable_exists.map(|v| v.to_string()).unwrap_or_else(|| "not checked (PATH lookup)".into()),
        cwd_exists,
        error,
        explanation
    )
}

fn invoke_hydrogen(
    program: &str,
    cwd: &Path,
    args: &[String],
) -> Result<std::process::Output, String> {
    if !cwd.is_dir() {
        return Err(format!(
            "Hydrogen build working directory does not exist: {}",
            cwd.display()
        ));
    }
    let path = Path::new(program);
    if path.is_absolute() && !path.is_file() {
        return Err(format!(
            "Configured Hydrogen executable is missing: {}\nChoose it again in Tools → Options → Hydrogen.",
            path.display()
        ));
    }
    Command::new(program)
        .args(args)
        .current_dir(cwd)
        // Honour ANSI colour output even when Quark captures the pipes.
        // Compilers that do not support these conventional variables can
        // safely ignore them.
        .env("CLICOLOR_FORCE", "1")
        .env("FORCE_COLOR", "1")
        .env("TERM", "xterm-256color")
        .output()
        .map_err(|error| launch_diagnostics(program, cwd, args, &error))
}

/// Stream the compiler's actual stdout/stderr while it is executing.
/// Separate reader threads avoid deadlocks if either pipe fills. Chunks keep
/// CR, ANSI and partial UTF-8 intact for the VT100 screen in the UI.
fn invoke_hydrogen_streaming(
    program: &str,
    cwd: &Path,
    args: &[String],
    emit: &mut dyn FnMut(&[u8]),
) -> Result<std::process::Output, String> {
    if !cwd.is_dir() {
        return Err(format!(
            "Hydrogen build working directory does not exist: {}",
            cwd.display()
        ));
    }
    let executable = Path::new(program);
    if executable.is_absolute() && !executable.is_file() {
        return Err(format!(
            "Configured Hydrogen executable is missing: {}\nChoose it again in Tools → Options → Hydrogen.",
            executable.display()
        ));
    }
    let mut child = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .env("CLICOLOR_FORCE", "1")
        .env("FORCE_COLOR", "1")
        .env("TERM", "xterm-256color")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| launch_diagnostics(program, cwd, args, &error))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Hydrogen output pipe is unavailable".to_owned())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Hydrogen error pipe is unavailable".to_owned())?;
    let (sender, receiver) = mpsc::channel::<(bool, Vec<u8>)>();
    let read_stream = |mut source: Box<dyn Read + Send>,
                       error_stream: bool,
                       sender: mpsc::Sender<(bool, Vec<u8>)>| {
        thread::spawn(move || {
            let mut buf = [0_u8; 8192];
            loop {
                match source.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if sender.send((error_stream, buf[..n].to_vec())).is_err() {
                            break;
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
        })
    };
    let stdout_thread = read_stream(Box::new(stdout), false, sender.clone());
    let stderr_thread = read_stream(Box::new(stderr), true, sender.clone());
    drop(sender);
    let mut captured_stdout = Vec::new();
    let mut captured_stderr = Vec::new();
    for (is_error, chunk) in receiver {
        emit(&chunk);
        if is_error {
            captured_stderr.extend_from_slice(&chunk);
        } else {
            captured_stdout.extend_from_slice(&chunk);
        }
    }
    let status = child
        .wait()
        .map_err(|error| format!("Cannot wait for Hydrogen compiler: {error}"))?;
    let _ = stdout_thread.join();
    let _ = stderr_thread.join();
    Ok(std::process::Output {
        status,
        stdout: captured_stdout,
        stderr: captured_stderr,
    })
}

/// Existing one-shot callers retain their exact result contract; native UI
/// callers can subscribe to raw output chunks without waiting for exit.
fn invoke_hydrogen_maybe_streaming(
    program: &str,
    cwd: &Path,
    args: &[String],
    emit: &mut Option<&mut dyn FnMut(&[u8])>,
) -> Result<std::process::Output, String> {
    if let Some(emit) = emit.as_deref_mut() {
        invoke_hydrogen_streaming(program, cwd, args, emit)
    } else {
        invoke_hydrogen(program, cwd, args)
    }
}

pub fn run_streaming(
    task: Task,
    compiler: &str,
    project: Option<&Path>,
    mut emit: impl FnMut(&[u8]),
) -> ResultLog {
    run_with_output(task, compiler, project, Some(&mut emit))
}

pub fn run(task: Task, compiler: &str, project: Option<&Path>) -> ResultLog {
    run_with_output(task, compiler, project, None)
}

fn run_with_output(
    task: Task,
    compiler: &str,
    project: Option<&Path>,
    mut emit: Option<&mut dyn FnMut(&[u8])>,
) -> ResultLog {
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
                    let built = invoke_hydrogen_maybe_streaming(
                        &plan.program,
                        &plan.cwd,
                        &plan.args,
                        &mut emit,
                    )?;
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
        Task::Build | Task::Check | Task::Test | Task::Rebuild | Task::RunWithoutDebugging => {
            let target = if task == Task::RunWithoutDebugging {
                match run_target(&plan, project) {
                    Ok(target) => Some(target),
                    Err(error) => return ResultLog::error(error),
                }
            } else {
                None
            };
            if task == Task::Rebuild {
                if let Err(error) = clean_generated(&plan) {
                    return ResultLog::error(format!("Rebuild could not clean outputs: {error}"));
                }
            }
            let args = match task {
                Task::Check => vec!["check".into(), project.to_string_lossy().into_owned()],
                Task::Test => vec!["test".into(), project.to_string_lossy().into_owned()],
                _ => plan.args.clone(),
            };
            let output_parent = if plan.output_is_directory {
                &plan.output
            } else {
                plan.output.parent().unwrap_or_else(|| Path::new("."))
            };
            if !matches!(task, Task::Check | Task::Test) {
                if let Err(error) = fs::create_dir_all(output_parent) {
                    return ResultLog::error(format!("Cannot create build directory: {error}"));
                }
            }
            if let Some((artifact, ui)) = &target {
                if let Err(error) = remove_previous_run_artifact(&plan, artifact, *ui) {
                    return ResultLog::error(format!(
                        "Cannot prepare a fresh run artifact: {error}"
                    ));
                }
            }
            let output =
                invoke_hydrogen_maybe_streaming(&plan.program, &plan.cwd, &args, &mut emit);
            match output {
                Ok(process) => {
                    let message = format!(
                        "Hydrogen {} {}\n{}\n{}\nProcess exit code: {}",
                        match task {
                            Task::RunWithoutDebugging => "build",
                            Task::Check => "check",
                            Task::Test => "test",
                            _ => "build",
                        },
                        project.display(),
                        String::from_utf8_lossy(&process.stdout),
                        String::from_utf8_lossy(&process.stderr),
                        process.status.code().unwrap_or(-1),
                    );
                    if process.status.success() {
                        if let Some((artifact, ui)) = target {
                            launch_built(&artifact, &plan.cwd, ui)
                                .map(|launch| format!("{message}\n{launch}"))
                                .map_err(|error| format!("{message}\n{error}"))
                        } else {
                            Ok(message)
                        }
                    } else {
                        Err(message)
                    }
                }
                Err(error) => Err(error),
            }
        }
    };
    let context = format!(
        "Hydrogen compiler: {}\nProject: {}\nWorking directory: {}\n",
        plan.program,
        project.display(),
        plan.cwd.display(),
    );
    match result {
        Ok(message) => ResultLog::ok(format!("{context}{message}")),
        Err(message) => ResultLog::error(format!("{context}{message}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[cfg(unix)]
    #[test]
    fn streaming_hydrogen_preserves_carriage_returns_and_chunk_order() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let compiler = dir.path().join("fake-hy");
        std::fs::write(
            &compiler,
            "#!/bin/sh\nprintf 'compiling\\r'\nsleep 0.05\nprintf 'done\\033[2K\\n'\nprintf 'warning: sample\\n' >&2\n",
        )
        .unwrap();
        std::fs::set_permissions(&compiler, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut chunks = Vec::new();
        let result = invoke_hydrogen_streaming(
            &compiler.to_string_lossy(),
            dir.path(),
            &["build".to_owned()],
            &mut |bytes| chunks.push(bytes.to_vec()),
        )
        .unwrap();
        assert!(result.status.success());
        assert!(!chunks.is_empty());
        let merged = chunks.concat();
        assert!(merged
            .windows(b"compiling\r".len())
            .any(|part| part == b"compiling\r"));
        assert!(merged
            .windows(b"done\x1b[2K".len())
            .any(|part| part == b"done\x1b[2K"));
        assert!(String::from_utf8_lossy(&result.stderr).contains("warning: sample"));
    }

    #[cfg(unix)]
    #[test]
    fn streaming_hydrogen_reports_real_nonzero_exit_status() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let compiler = dir.path().join("failing-hy");
        std::fs::write(&compiler, "#!/bin/sh\nprintf 'broken\\r' >&2\nexit 7\n").unwrap();
        std::fs::set_permissions(&compiler, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut displayed = Vec::new();
        let result =
            invoke_hydrogen_streaming(&compiler.to_string_lossy(), dir.path(), &[], &mut |bytes| {
                displayed.extend_from_slice(bytes)
            })
            .unwrap();
        assert_eq!(result.status.code(), Some(7));
        assert_eq!(displayed, b"broken\r");
    }

    #[test]
    fn compiler_launch_error_identifies_saved_executable_and_project_directory() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("does-not-exist").join("hydrogen-stage1");
        let args = vec!["build".into(), "src/australis.hyproj".into()];
        let result = invoke_hydrogen(&missing.to_string_lossy(), temp.path(), &args).unwrap_err();
        assert!(result.contains("Configured Hydrogen executable is missing"));
        assert!(result.contains(&missing.to_string_lossy().to_string()));

        let cwd_missing = temp.path().join("no-project-directory");
        let result = invoke_hydrogen("hy", &cwd_missing, &args).unwrap_err();
        assert!(result.contains("working directory does not exist"));
    }

    #[cfg(unix)]
    #[test]
    fn an_existing_executable_can_report_missing_loader() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let executable = temp.path().join("hydrogen-script");
        std::fs::write(&executable, "#!/path/to/nonexistent/quark-interpreter\n").unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        let result = invoke_hydrogen(
            &executable.to_string_lossy(),
            temp.path(),
            &["--version".into()],
        )
        .unwrap_err();
        assert!(result.contains("interpreter or dynamic loader"));
        assert!(result.contains(&executable.to_string_lossy().to_string()));
    }

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

    #[cfg(unix)]
    #[test]
    fn hydrogen_test_task_uses_configured_compiler_and_captures_results() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("QuarkTests.hyproj");
        fs::write(&project, "type='linux'\nsources=['Main.hy']").unwrap();
        let compiler = dir.path().join("hydrogen-test-wrapper");
        fs::write(&compiler, "#!/bin/sh\nprintf 'PASS: Quark PTY test\\n'\n").unwrap();
        fs::set_permissions(&compiler, fs::Permissions::from_mode(0o755)).unwrap();
        let result = run(Task::Test, &compiler.to_string_lossy(), Some(&project));
        assert!(result.success, "{}", result.text);
        assert!(result.text.contains("PASS: Quark PTY test"));
        assert!(result.text.contains("Hydrogen test"));
        assert!(!dir.path().join(".quark/build").exists());
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
        // The launcher now reports the selected executable and working
        // directory rather than the previous generic build-start message.
        assert!(
            result.text.contains("Could not start Hydrogen compiler"),
            "{}",
            result.text
        );
        assert!(
            result.text.contains("a-compiler-that-does-not-exist-quark"),
            "{}",
            result.text
        );
        assert!(
            result.text.contains("Working directory:"),
            "{}",
            result.text
        );
        assert!(!dir.path().join(".quark/publish").exists());
        assert_eq!(fs::read(&plan.output).unwrap(), b"old stale binary");
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

    #[test]
    fn run_selects_host_artifact_for_console_and_ui_manifests() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("Sample.hyproj");
        let plan = sample_plan(dir.path());
        fs::write(&project, "type='exe'\nsources=['Main.hy']").unwrap();
        assert_eq!(
            run_target(&plan, &project).unwrap(),
            (plan.output.clone(), false)
        );

        fs::write(
            &project,
            "type='exe'\nui='Studio.hyui'\nsources=['Main.hy']",
        )
        .unwrap();
        let (artifact, is_ui) = run_target(&plan, &project).unwrap();
        assert!(is_ui);
        if cfg!(windows) {
            assert_eq!(artifact, append_output_suffix(&plan.output, "exe"));
        } else if cfg!(target_os = "macos") {
            assert_eq!(artifact, append_output_suffix(&plan.output, "app"));
        } else {
            assert_eq!(artifact, plan.output);
        }
        assert_eq!(
            append_output_suffix(&dir.path().join("My.App"), "exe"),
            dir.path().join("My.App.exe")
        );
    }

    #[test]
    fn run_rejects_non_host_executable_targets_before_build() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("Firmware.hyproj");
        fs::write(&project, "type='efi'\nsources=['Main.hy']").unwrap();
        let result = run(Task::RunWithoutDebugging, "hy", Some(&project));
        assert!(!result.success);
        assert!(result.text.contains("Cannot run Hydrogen target 'efi'"));
        assert!(!dir.path().join(".quark").exists());
    }

    #[test]
    fn run_replaces_only_generated_artifacts_and_windows_runtime_sidecar() {
        let dir = tempfile::tempdir().unwrap();
        let plan = sample_plan(dir.path());
        let artifact = if cfg!(windows) {
            append_output_suffix(&plan.output, "exe")
        } else if cfg!(target_os = "macos") {
            append_output_suffix(&plan.output, "app")
        } else {
            plan.output.clone()
        };
        fs::create_dir_all(artifact.parent().unwrap()).unwrap();
        if cfg!(target_os = "macos") {
            fs::create_dir_all(&artifact).unwrap();
        } else {
            fs::write(&artifact, b"previous application").unwrap();
        }
        if cfg!(windows) {
            let mut runtime = artifact.as_os_str().to_os_string();
            runtime.push(".runtime");
            fs::create_dir_all(PathBuf::from(runtime)).unwrap();
        }
        remove_previous_run_artifact(&plan, &artifact, true).unwrap();
        assert!(!artifact.exists());
        if cfg!(windows) {
            let mut runtime = artifact.as_os_str().to_os_string();
            runtime.push(".runtime");
            assert!(!PathBuf::from(runtime).exists());
        }
        assert!(dir.path().exists());
        assert!(remove_previous_run_artifact(&plan, dir.path(), true).is_err());
    }

    #[cfg(unix)]
    fn executable_script(path: &Path, content: &str) {
        use std::os::unix::fs::PermissionsExt;
        fs::write(path, content).unwrap();
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(path, permissions).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn run_builds_and_executes_a_fresh_console_application() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("Console.hyproj");
        fs::write(&project, "type='exe'\nsources=['Main.hy']").unwrap();
        let compiler = dir.path().join("fake-hy");
        executable_script(
            &compiler,
            "#!/bin/sh\n[ \"$1\" = build ] || exit 10\n[ \"$3\" = -o ] || exit 11\nprintf '#!/bin/sh\\nprintf \"fresh-console\\n\"\\n' > \"$4\"\nchmod +x \"$4\"\n",
        );
        let result = run(
            Task::RunWithoutDebugging,
            compiler.to_str().unwrap(),
            Some(&project),
        );
        assert!(result.success, "{}", result.text);
        assert!(result.text.contains("fresh-console"));
        assert!(result.text.contains("Process exit code: 0"));
    }

    #[cfg(unix)]
    #[test]
    fn run_does_not_launch_stale_output_after_a_no_op_build() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("Console.hyproj");
        fs::write(&project, "type='exe'").unwrap();
        let plan = hydrogen::build_plan("hy", Some(&project), None, BuildScope::Project).unwrap();
        fs::create_dir_all(plan.output.parent().unwrap()).unwrap();
        let marker = dir.path().join("stale-launched");
        executable_script(
            &plan.output,
            &format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
        );
        let compiler = dir.path().join("fake-hy");
        executable_script(&compiler, "#!/bin/sh\nexit 0\n");
        let result = run(
            Task::RunWithoutDebugging,
            compiler.to_str().unwrap(),
            Some(&project),
        );
        assert!(!result.success);
        assert!(result.text.contains("Built application is missing"));
        assert!(!marker.exists());
        assert!(!plan.output.exists());
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
