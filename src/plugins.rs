//! Minimal v1 native plugin protocol: explicitly invoked external commands.
//! Plugins run as child processes, never in the Quark process. This is not a
//! sandbox: users must install only plugins they trust.
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Deserialize)]
pub struct PluginCommand {
    pub title: String,
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub needs_project: bool,
    #[serde(default)]
    pub needs_file: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct Manifest {
    api_version: u32,
    id: String,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    commands: Vec<PluginCommand>,
}

#[derive(Debug, Clone)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub description: String,
    pub directory: PathBuf,
    pub commands: Vec<PluginCommand>,
}

pub fn directory(settings_path: Option<&Path>) -> Option<PathBuf> {
    settings_path?.parent().map(|root| root.join("plugins"))
}

pub fn load_all(root: &Path) -> (Vec<Plugin>, Vec<String>) {
    let mut plugins = vec![];
    let mut warnings = vec![];
    if !root.exists() {
        return (plugins, warnings);
    }
    let Ok(entries) = fs::read_dir(root) else {
        return (plugins, vec![format!("Cannot read {}", root.display())]);
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let path = dir.join("plugin.json");
        if !path.exists() {
            continue;
        }
        let manifest = fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|s| serde_json::from_str::<Manifest>(&s).map_err(|e| e.to_string()));
        match manifest {
            Ok(m)
                if m.api_version == 1
                    && !m.id.trim().is_empty()
                    && !m.commands.is_empty()
                    && m.commands
                        .iter()
                        .all(|c| !c.title.trim().is_empty() && !c.program.trim().is_empty()) =>
            {
                if plugins.iter().any(|p: &Plugin| p.id == m.id) {
                    warnings.push(format!("Duplicate plugin ID: {}", m.id));
                    continue;
                }
                plugins.push(Plugin {
                    id: m.id,
                    name: m.name,
                    description: m.description,
                    directory: dir,
                    commands: m.commands,
                });
            }
            Ok(_) => warnings.push(format!("Invalid v1 plugin manifest: {}", path.display())),
            Err(e) => warnings.push(format!("{}: {e}", path.display())),
        }
    }
    plugins.sort_by(|a, b| a.name.cmp(&b.name));
    (plugins, warnings)
}

#[derive(Debug)]
pub struct Invocation {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
}

fn expand(
    template: &str,
    plugin: &Plugin,
    project: Option<&Path>,
    file: Option<&Path>,
    workspace: &Path,
) -> Result<String, String> {
    let mut result = template
        .replace("{plugin_dir}", &plugin.directory.to_string_lossy())
        .replace("{workspace}", &workspace.to_string_lossy());
    if result.contains("{project}") {
        let p = project.ok_or("Plugin requires a selected Hydrogen project")?;
        result = result.replace("{project}", &p.to_string_lossy());
    }
    if result.contains("{file}") {
        let p = file.ok_or("Plugin requires an active saved file")?;
        result = result.replace("{file}", &p.to_string_lossy());
    }
    Ok(result)
}

pub fn invoke(
    plugin: &Plugin,
    command: &PluginCommand,
    project: Option<&Path>,
    file: Option<&Path>,
    workspace: &Path,
) -> Result<Invocation, String> {
    if command.needs_project && project.is_none() {
        return Err("Select a Hydrogen project for this plugin".into());
    }
    if command.needs_file && file.is_none() {
        return Err("Open and save a source file for this plugin".into());
    }
    let raw_program = expand(&command.program, plugin, project, file, workspace)?;
    // Executables starting with ./ are resolved relative to the plugin's own directory.
    let program = if raw_program.starts_with("./") || raw_program.starts_with(".\\") {
        plugin
            .directory
            .join(&raw_program[2..])
            .to_string_lossy()
            .into_owned()
    } else {
        raw_program
    };
    let args = command
        .args
        .iter()
        .map(|s| expand(s, plugin, project, file, workspace))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Invocation {
        program,
        args,
        cwd: workspace.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Plugin {
        Plugin {
            id: "test".into(),
            name: "Test".into(),
            description: "".into(),
            directory: PathBuf::from("/tmp/quark-plugins/test"),
            commands: vec![],
        }
    }
    #[test]
    fn explicit_args_preserve_spaces_without_shell_quoting() {
        let c = PluginCommand {
            title: "Inspect".into(),
            program: "git".into(),
            args: vec!["status".into(), "--".into(), "{file}".into()],
            needs_file: true,
            needs_project: false,
        };
        let inv = invoke(
            &fixture(),
            &c,
            None,
            Some(Path::new("/tmp/my source.hy")),
            Path::new("/tmp"),
        )
        .unwrap();
        assert_eq!(inv.args[2], "/tmp/my source.hy");
        assert_eq!(inv.program, "git");
    }
    #[test]
    fn requires_explicit_context() {
        let c = PluginCommand {
            title: "Build".into(),
            program: "hy".into(),
            args: vec!["{project}".into()],
            needs_file: false,
            needs_project: true,
        };
        assert!(invoke(&fixture(), &c, None, None, Path::new(".")).is_err());
    }
    #[test]
    fn relative_program_resolves_to_plugin_folder() {
        let c = PluginCommand {
            title: "Run".into(),
            program: "./tool".into(),
            args: vec![],
            needs_file: false,
            needs_project: false,
        };
        let inv = invoke(&fixture(), &c, None, None, Path::new(".")).unwrap();
        assert_eq!(Path::new(&inv.program).file_name().unwrap(), "tool");
    }
}

#[cfg(test)]
mod loading_tests {
    use super::*;
    #[test]
    fn loads_v1_manifest_and_reports_invalid_versions() {
        let root = std::env::temp_dir().join(format!(
            "quark-plugin-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("good")).unwrap();
        fs::create_dir_all(root.join("wrong")).unwrap();
        fs::write(root.join("good/plugin.json"),r#"{"api_version":1,"id":"good","name":"Valid","commands":[{"title":"Status","program":"git","args":["status"]}]}"#).unwrap();
        fs::write(root.join("wrong/plugin.json"),r#"{"api_version":99,"id":"bad","name":"Bad","commands":[{"title":"Run","program":"git"}]}"#).unwrap();
        let (plugins, warnings) = load_all(&root);
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].id, "good");
        assert_eq!(warnings.len(), 1);
        fs::remove_dir_all(root).unwrap();
    }
}
