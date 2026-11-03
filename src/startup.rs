//! Safe project scaffolding for the start window.
//! Templates intentionally contain plain Hydrogen and a separate, design-time
//! UI document. Runtime UI generation requires a Hydrogen.UI backend.

use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Template {
    Linux,
    Windows,
    MacOS,
    Australis,
    Library,
}

impl Template {
    pub const ALL: [Self; 5] = [
        Self::Linux,
        Self::Windows,
        Self::MacOS,
        Self::Australis,
        Self::Library,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Linux => "Linux application",
            Self::Windows => "Windows application",
            Self::MacOS => "macOS application",
            Self::Australis => "Australis executable",
            Self::Library => "Hydrogen library",
        }
    }

    pub fn project_type(self) -> &'static str {
        match self {
            Self::Linux => "linux",
            Self::Windows => "windows",
            Self::MacOS => "macos",
            Self::Australis => "exec",
            Self::Library => "lib",
        }
    }
}

pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.as_bytes()[0].is_ascii_alphabetic()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

pub fn create_project(parent: &Path, name: &str, template: Template) -> Result<PathBuf, String> {
    if !valid_name(name) {
        return Err("Project name must start with a letter and contain only letters, numbers, underscores or hyphens (maximum 64 characters).".into());
    }
    if !parent.is_dir() {
        return Err(format!(
            "Choose an existing parent directory: {}",
            parent.display()
        ));
    }
    let folder = parent.join(name);
    // Never overwrite or reuse existing files or directories.
    fs::create_dir(&folder).map_err(|e| format!("Cannot create project folder: {e}"))?;
    let result = (|| -> Result<PathBuf, String> {
        let manifest = folder.join(format!("{name}.hyproj"));
        let source = if template == Template::Library {
            "public class Library {\n    public static int Add(int left, int right) {\n        return left + right;\n    }\n}\n".to_owned()
        } else {
            format!(
                "public class Program {{\n    public static void Main(string[] args) {{\n        System.Console.WriteLine(\"Hello from {name}!\");\n    }}\n}}\n"
            )
        };
        let metadata = format!(
            "format = 2\nname = \"{name}\"\nversion = \"0.1.0\"\ntype = \"{}\"\nsources = [\"Program.hy\"]\n",
            template.project_type()
        );
        fs::write(folder.join("Program.hy"), source).map_err(|e| e.to_string())?;
        fs::write(&manifest, metadata).map_err(|e| e.to_string())?;
        Ok(manifest)
    })();
    if result.is_err() {
        // Only remove the directory we successfully created above.
        let _ = fs::remove_dir_all(folder);
    }
    result
}

/// Create a genuinely runnable native Hydrogen UI app rather than a legacy
/// flat design document. The Hydrogen.UI library is vendored as Hydrogen
/// source inside the new project, so a fresh user's project can build
/// without a hard-coded path back into Hydrogen's repository.
#[cfg(any(feature = "native-ui", test))]
#[allow(dead_code)] // Called by native app wizard but not legacy UI.
pub fn create_native_ui_project(parent: &Path, name: &str) -> Result<PathBuf, String> {
    let manifest = create_project(parent, name, Template::Linux)?;
    let folder = manifest
        .parent()
        .ok_or("Missing project directory")?
        .to_path_buf();
    let result = (|| -> Result<(), String> {
        let library = folder.join("Hydrogen.UI");
        fs::create_dir(&library).map_err(|error| error.to_string())?;
        fs::write(
            library.join("Hydrogen.UI.hyproj"),
            "format = 2\nname = \"Hydrogen.UI\"\nversion = \"0.1.0\"\ntype = \"lib\"\nsources = [\"Application.hy\"]\n",
        )
        .map_err(|error| error.to_string())?;
        fs::write(
            library.join("Application.hy"),
            include_str!("../templates/hydrogen_ui_application.hy"),
        )
        .map_err(|error| error.to_string())?;

        let source = r#"using Hydrogen.UI;

public class Program {
    public static int Main(string[] args) {
        if (!Application.IsEvent(args)) {
            Application.Open("Main.hyui");
            return 0;
        }
        if (Application.Handler(args) == "ShowGreeting") {
            Application.SetText(
                "greeting",
                "Hello, " + Application.Value(args, "projectName") + "!"
            );
        }
        return 0;
    }
}
"#;
        fs::write(folder.join("Program.hy"), source).map_err(|error| error.to_string())?;
        let ui = format!(
            r#"<Window version="1" title="{name}" width="960" height="700" padding="24" theme="system">
  <ColumnPanel gap="20">
    <Label role="title" text="Welcome to {name}"/>
    <Label tone="muted" text="Build your app with native Hydrogen controls."/>
    <RowPanel gap="20" breakpoint="760">
      <Panel title="Getting started" padding="20">
        <ColumnPanel gap="12">
          <Label role="caption" text="PROJECT NAME"/>
          <Input id="projectName" text="{name}" placeholder="Project name"/>
          <Button id="greet" text="Say hello" primary="true" onClick="ShowGreeting"/>
        </ColumnPanel>
      </Panel>
      <Panel title="Preview" padding="20">
        <Label id="greeting" text="Your UI is ready to edit."/>
      </Panel>
    </RowPanel>
  </ColumnPanel>
</Window>
"#
        );
        fs::write(folder.join("Main.hyui"), ui).map_err(|error| error.to_string())?;
        fs::write(
            &manifest,
            format!(
                "format = 2\nname = \"{name}\"\nversion = \"0.1.0\"\ntype = \"exe\"\nui = \"Main.hyui\"\nsources = [\"Program.hy\"]\nproject_references = [\"Hydrogen.UI/Hydrogen.UI.hyproj\"]\n"
            ),
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    })();
    if let Err(error) = result {
        // create_project only succeeds for a newly created directory. Never
        // remove the user's pre-existing folders when scaffolding fails.
        let _ = fs::remove_dir_all(&folder);
        return Err(error);
    }
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_ui_project_creates_runnable_hydrogen_manifest_and_valid_hyui() {
        let directory = tempfile::tempdir().unwrap();
        let manifest = create_native_ui_project(directory.path(), "QuarkApp").unwrap();
        let folder = manifest.parent().unwrap();
        let project = fs::read_to_string(&manifest).unwrap();
        assert!(project.contains("ui = \"Main.hyui\""));
        assert!(project.contains("type = \"exe\""));
        assert!(project.contains("project_references = [\"Hydrogen.UI/Hydrogen.UI.hyproj\"]"));
        let ui = fs::read_to_string(folder.join("Main.hyui")).unwrap();
        let parsed = roxmltree::Document::parse(&ui).unwrap();
        assert!(parsed.root_element().has_tag_name("Window"));
        assert!(parsed
            .descendants()
            .any(|node| { node.attribute("onClick") == Some("ShowGreeting") }));
        assert!(folder.join("Hydrogen.UI/Application.hy").exists());
        assert!(folder.join("Program.hy").exists());
    }

    #[test]
    fn native_ui_project_has_functional_backend_handlers_and_is_not_overwritten() {
        let directory = tempfile::tempdir().unwrap();
        let manifest = create_native_ui_project(directory.path(), "MyEditor").unwrap();
        let root = manifest.parent().unwrap();
        let backend = fs::read_to_string(root.join("Program.hy")).unwrap();
        let library = fs::read_to_string(root.join("Hydrogen.UI/Application.hy")).unwrap();
        assert!(backend.contains("Application.Open(\"Main.hyui\")"));
        assert!(backend.contains("Application.Handler(args) == \"ShowGreeting\""));
        assert!(backend.contains("Application.SetText("));
        assert!(library.contains("public class Application"));
        assert!(create_native_ui_project(directory.path(), "MyEditor").is_err());
        assert!(root.join("Main.hyui").exists());
        assert!(create_native_ui_project(directory.path(), "../escape").is_err());
    }

    #[test]
    fn valid_and_invalid_project_names() {
        assert!(valid_name("Hello_World-2"));
        for name in ["", "../Oops", ".git", "9Start", "with space", "a/b", "a\\b"] {
            assert!(!valid_name(name), "{name}");
        }
    }

    #[test]
    fn creates_hydrogen_manifest_that_project_loader_accepts() {
        let dir = tempfile::tempdir().unwrap();
        for template in Template::ALL {
            let name = format!("Project_{}", template.project_type());
            let path = create_project(dir.path(), &name, template).unwrap();
            let project = crate::project::load(&path);
            assert_eq!(project.kind, template.project_type());
            assert_eq!(project.name, name);
            assert_eq!(project.sources.len(), 1);
            assert!(project.warnings.is_empty(), "{:?}", project.warnings);
            assert!(path.parent().unwrap().join("Program.hy").exists());
        }
    }

    #[test]
    fn does_not_overwrite_an_existing_project() {
        let dir = tempfile::tempdir().unwrap();
        let path = create_project(dir.path(), "Demo", Template::Linux).unwrap();
        let source = path.parent().unwrap().join("Program.hy");
        fs::write(&source, "keep my edits").unwrap();
        assert!(create_project(dir.path(), "Demo", Template::Windows).is_err());
        assert_eq!(fs::read_to_string(source).unwrap(), "keep my edits");
    }

    #[test]
    fn invalid_parent_or_name_does_not_create_any_project() {
        let dir = tempfile::tempdir().unwrap();
        assert!(create_project(dir.path(), "../escape", Template::Linux).is_err());
        assert!(!dir.path().join("escape").exists());
        assert!(create_project(&dir.path().join("missing"), "Demo", Template::Linux).is_err());
    }
}
