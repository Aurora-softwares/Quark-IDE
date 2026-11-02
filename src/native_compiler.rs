//! Cross-platform Hydrogen executable resolution for Quark's native IDE.
//! Settings are stored alongside native preferences, without requiring GTK
//! on macOS or Windows. All command paths share the same precedence.
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn preferences_file() -> PathBuf {
    crate::workspace_file().with_file_name("native-preferences.json")
}

/// Validate a manually configured executable before saving or invoking it.
pub fn validate_explicit(raw: &str) -> Result<(), String> {
    let path = Path::new(raw);
    if !path.is_absolute() {
        return Err("Choose an absolute path to the Hydrogen executable.".into());
    }
    let metadata = fs::metadata(path).map_err(|error| {
        format!(
            "Hydrogen executable not found at {}: {error}",
            path.display()
        )
    })?;
    if !metadata.is_file() {
        return Err(format!(
            "Hydrogen executable must be a file: {}",
            path.display()
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(format!(
                "Hydrogen executable is not marked executable: {}",
                path.display()
            ));
        }
    }
    Ok(())
}

pub fn resolve_at(preferences: &Path) -> Result<String, String> {
    // Explicit selection overrides the environment, even when invalid.
    // Falling back silently could execute the wrong installed compiler.
    if let Ok(bytes) = fs::read(preferences) {
        if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes) {
            if let Some(path) = json
                .get("hydrogen")
                .and_then(|value| value.get("executable"))
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                validate_explicit(path)?;
                return Ok(path.to_owned());
            }
        }
    }
    if let Ok(value) = std::env::var("QUARK_HYDROGEN_COMPILER") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            if Path::new(trimmed).is_absolute() {
                validate_explicit(trimmed)?;
            }
            return Ok(trimmed.to_owned());
        }
    }
    Ok("hy".to_owned())
}

pub fn resolve() -> Result<String, String> {
    resolve_at(&preferences_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_configuration_falls_back_to_hy_or_environment() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("preferences.json");
        let expected = std::env::var("QUARK_HYDROGEN_COMPILER")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "hy".into());
        assert_eq!(resolve_at(&path).unwrap(), expected);
    }
    #[test]
    fn explicit_missing_path_fails_instead_of_falling_back() {
        let temp = tempfile::tempdir().unwrap();
        let config = temp.path().join("preferences.json");
        let missing = temp.path().join("does-not-exist-hy");
        fs::write(
            &config,
            serde_json::json!({
                "hydrogen": {"executable": missing.to_string_lossy()}
            })
            .to_string(),
        )
        .unwrap();
        assert!(resolve_at(&config).unwrap_err().contains("not found"));
    }
    #[cfg(unix)]
    #[test]
    fn manual_executable_validates_and_overrides_default() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let binary = temp.path().join("hydrogen-stage1");
        fs::write(&binary, "#!/bin/sh\necho hydrogen\n").unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
        let config = temp.path().join("preferences.json");
        fs::write(
            &config,
            serde_json::json!({
                "hydrogen": {"executable": binary.to_string_lossy()}
            })
            .to_string(),
        )
        .unwrap();
        assert_eq!(resolve_at(&config).unwrap(), binary.to_string_lossy());
    }
}
