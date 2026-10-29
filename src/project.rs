use serde::Deserialize;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Default, Deserialize)]
struct Manifest {
    name: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    sources: Vec<String>,
    #[serde(default)]
    project_references: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Project {
    pub path: PathBuf,
    pub name: String,
    pub kind: String,
    pub sources: Vec<PathBuf>,
    pub references: Vec<Project>,
    pub warnings: Vec<String>,
}

pub fn load(path: &Path) -> Project {
    load_inner(path, &mut HashSet::new(), 0)
}

fn load_inner(path: &Path, seen: &mut HashSet<PathBuf>, depth: usize) -> Project {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let mut result = Project {
        name: canonical
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        kind: "unknown".into(),
        path: canonical.clone(),
        sources: vec![],
        references: vec![],
        warnings: vec![],
    };
    if depth > 64 || seen.len() >= 4096 {
        result
            .warnings
            .push("Incomplete project graph: reference depth/count limit reached".into());
        return result;
    }
    if !seen.insert(canonical.clone()) {
        result
            .warnings
            .push("Already visited (cycle/shared reference)".into());
        return result;
    }
    let content = match fs::read_to_string(&canonical) {
        Ok(text) => text,
        Err(e) => {
            result.warnings.push(format!("Cannot read: {e}"));
            return result;
        }
    };
    let manifest: Manifest = match toml::from_str(&content) {
        Ok(m) => m,
        Err(e) => {
            result.warnings.push(format!("Invalid .hyproj: {e}"));
            return result;
        }
    };
    result.name = manifest.name.unwrap_or(result.name);
    result.kind = manifest.kind.unwrap_or(result.kind);
    let base = canonical.parent().unwrap_or(Path::new("."));
    for source in manifest.sources {
        let src = base.join(source);
        if !src.exists() {
            result
                .warnings
                .push(format!("Missing source: {}", src.display()));
        }
        result.sources.push(src.canonicalize().unwrap_or(src));
    }
    for reference in manifest.project_references {
        result
            .references
            .push(load_inner(&base.join(reference), seen, depth + 1));
    }
    result
}

pub fn collect_sources(project: &Project, out: &mut Vec<PathBuf>) {
    out.extend(project.sources.iter().cloned());
    for child in &project.references {
        collect_sources(child, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_recursive_references_and_cycles() {
        let base = std::env::temp_dir().join(format!("quark-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("app")).unwrap();
        fs::create_dir_all(base.join("lib")).unwrap();
        fs::write(base.join("app/a.hyproj"), "format=2\nname='App'\ntype='exe'\nsources=['main.hy']\nproject_references=['../lib/b.hyproj']\n").unwrap();
        fs::write(
            base.join("lib/b.hyproj"),
            "name='Lib'\ntype='lib'\nproject_references=['../app/a.hyproj']\n",
        )
        .unwrap();
        fs::write(base.join("app/main.hy"), "").unwrap();
        let p = load(&base.join("app/a.hyproj"));
        assert_eq!(p.name, "App");
        assert_eq!(p.references[0].name, "Lib");
        assert!(!p.references[0].references[0].warnings.is_empty());
        let mut files = vec![];
        collect_sources(&p, &mut files);
        assert_eq!(files.len(), 1);
        fs::remove_dir_all(&base).unwrap();
    }
}

#[cfg(test)]
mod extra_tests {
    use super::*;
    fn sandbox(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "quark-{}-{}-{:?}",
            name,
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }
    #[test]
    fn detects_missing_source() {
        let root = sandbox("missing");
        fs::write(
            root.join("p.hyproj"),
            "name='missing'\nsources=['lost.hy']\n",
        )
        .unwrap();
        let p = load(&root.join("p.hyproj"));
        assert!(p.warnings.iter().any(|w| w.contains("Missing source")));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn malformed_manifest_becomes_warning() {
        let root = sandbox("malformed");
        fs::write(root.join("p.hyproj"), "sources = [not valid").unwrap();
        let p = load(&root.join("p.hyproj"));
        assert!(p.warnings.iter().any(|w| w.contains("Invalid .hyproj")));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn external_reference_loads_sources() {
        let root = sandbox("external");
        fs::create_dir_all(root.join("outer")).unwrap();
        fs::create_dir_all(root.join("other")).unwrap();
        fs::write(
            root.join("outer/root.hyproj"),
            "name='Root'\nproject_references=['../other/lib.hyproj']\n",
        )
        .unwrap();
        fs::write(
            root.join("other/lib.hyproj"),
            "name='Lib'\nsources=['lib.hy']\n",
        )
        .unwrap();
        fs::write(root.join("other/lib.hy"), "").unwrap();
        let p = load(&root.join("outer/root.hyproj"));
        let mut sources = vec![];
        collect_sources(&p, &mut sources);
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].file_name().unwrap(), "lib.hy");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn missing_reference_is_reported() {
        let root = sandbox("missing-ref");
        fs::write(
            root.join("root.hyproj"),
            "project_references=['notfound.hyproj']\n",
        )
        .unwrap();
        let p = load(&root.join("root.hyproj"));
        assert_eq!(p.references.len(), 1);
        assert!(p.references[0]
            .warnings
            .iter()
            .any(|w| w.contains("Cannot read")));
        fs::remove_dir_all(root).unwrap();
    }
}
