//! Safe project scaffolding for the start window.
//! Templates intentionally contain plain Hydrogen and a separate, design-time
//! UI document. Runtime UI generation requires a Hydrogen.UI backend.

use std::{fs, path::{Path, PathBuf}};

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
        Self::Linux, Self::Windows, Self::MacOS, Self::Australis, Self::Library,
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
        && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

pub fn create_project(parent: &Path, name: &str, template: Template) -> Result<PathBuf, String> {
    if !valid_name(name) {
        return Err("Project name must start with a letter and contain only letters, numbers, underscores or hyphens (maximum 64 characters).".into());
    }
    if !parent.is_dir() {
        return Err(format!("Choose an existing parent directory: {}", parent.display()));
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

#[cfg(test)]
mod tests {
    use super::*;
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
