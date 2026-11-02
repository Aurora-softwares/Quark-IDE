//! One versioned preferences contract for GTK, AppKit and WinUI.
//! Settings are independent of the user's workspace and dock layouts.
use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path};

pub const SCHEMA_VERSION: u32 = 1;

fn version() -> u32 {
    SCHEMA_VERSION
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Preferences {
    #[serde(default = "version")]
    pub schema_version: u32,
    pub appearance: Appearance,
    pub editor: Editor,
    pub debugging: Debugging,
    pub build: Build,
    pub hydrogen: Hydrogen,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            appearance: Appearance::default(),
            editor: Editor::default(),
            debugging: Debugging::default(),
            build: Build::default(),
            hydrogen: Hydrogen::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Appearance {
    pub theme: ThemeChoice,
    pub compact_controls: bool,
}
impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: ThemeChoice::System,
            compact_controls: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Editor {
    pub tab_width: u8,
    pub show_line_numbers: bool,
    pub word_wrap: bool,
}
impl Default for Editor {
    fn default() -> Self {
        Self {
            tab_width: 4,
            show_line_numbers: true,
            word_wrap: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Debugging {
    pub stop_on_exception: bool,
    pub show_debug_output: bool,
}
impl Default for Debugging {
    fn default() -> Self {
        Self {
            stop_on_exception: true,
            show_debug_output: true,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Hydrogen {
    pub executable: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Build {
    pub save_before_build: bool,
    pub show_output: bool,
}
impl Default for Build {
    fn default() -> Self {
        Self {
            save_before_build: true,
            show_output: true,
        }
    }
}

impl Preferences {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(format!(
                "Unsupported Quark settings version {} (expected {})",
                self.schema_version, SCHEMA_VERSION
            ));
        }
        if !(1..=16).contains(&self.editor.tab_width) {
            return Err("Editor tab width must be between 1 and 16".into());
        }
        if let Some(executable) = &self.hydrogen.executable {
            if executable.contains('\0') {
                return Err("Compiler path contains a NUL byte".into());
            }
        }
        Ok(())
    }
}

/// Old settings without a schema version automatically migrate to version 1.
/// Invalid settings are never overwritten by an unrelated preference change.
pub fn try_load(path: &Path) -> Result<Preferences, String> {
    let data = match fs::read(path) {
        Ok(data) => data,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(Preferences::default());
        }
        Err(error) => return Err(format!("Cannot read settings: {error}")),
    };
    let parsed: serde_json::Value =
        serde_json::from_slice(&data).map_err(|error| format!("Invalid settings JSON: {error}"))?;
    let Some(object) = parsed.as_object() else {
        return Err("Settings file must contain a JSON object".into());
    };
    if object
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .is_some_and(|value| value > u64::from(SCHEMA_VERSION))
    {
        return Err("Settings were created by a newer version of Quark".into());
    }
    let preferences: Preferences =
        serde_json::from_value(parsed).map_err(|error| format!("Invalid settings: {error}"))?;
    preferences.validate()?;
    Ok(preferences)
}

/// Tolerant read for initial window appearance only; writes always check the
/// file with try_load so corrupt or future settings cannot be destroyed.
pub fn load(path: &Path) -> Preferences {
    try_load(path).unwrap_or_default()
}

pub fn save(path: &Path, preferences: &Preferences) -> Result<(), String> {
    preferences.validate()?;
    // Do not silently replace an existing corrupt or future-version file.
    try_load(path)?;
    let data = serde_json::to_vec_pretty(preferences).map_err(|error| error.to_string())?;
    crate::storage::atomic_write(path, &data).map_err(|error| error.to_string())
}

pub fn update(
    path: &Path,
    change: impl FnOnce(&mut Preferences),
) -> Result<Preferences, String> {
    let mut preferences = try_load(path)?;
    change(&mut preferences);
    save(path, &preferences)?;
    Ok(preferences)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_settings_upgrade_to_versioned_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("native-preferences.json");
        fs::write(&path, r#"{"appearance":{"theme":"dark"},"editor":{"tab_width":8}}"#)
            .unwrap();
        let migrated = try_load(&path).unwrap();
        assert_eq!(migrated.schema_version, SCHEMA_VERSION);
        assert_eq!(migrated.appearance.theme, ThemeChoice::Dark);
        assert_eq!(migrated.editor.tab_width, 8);
        assert!(migrated.build.save_before_build);
        save(&path, &migrated).unwrap();
        assert!(fs::read_to_string(&path).unwrap().contains("schema_version"));
    }

    #[test]
    fn corrupt_and_newer_settings_survive_updates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("native-preferences.json");
        for original in ["{invalid", r#"{"schema_version":900}"#, "[]"] {
            fs::write(&path, original).unwrap();
            assert!(update(&path, |prefs| prefs.build.show_output = false).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), original);
        }
    }

    #[test]
    fn preferences_validate_editor_bounds_and_preserve_other_sections() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("native-preferences.json");
        let changed = update(&path, |prefs| {
            prefs.editor.tab_width = 16;
            prefs.build.show_output = false;
        })
        .unwrap();
        assert_eq!(changed.editor.tab_width, 16);
        assert!(!try_load(&path).unwrap().build.show_output);
        assert!(try_load(&path).unwrap().editor.show_line_numbers);
        assert!(update(&path, |prefs| prefs.editor.tab_width = 0).is_err());
        assert_eq!(try_load(&path).unwrap().editor.tab_width, 16);
    }
}
