//! Local theme/extension management. Remote discovery/update transports are intentionally absent.
use crate::*;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct InstalledTheme {
    pub id: String,
    pub definition: theme::Definition,
}
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Preferences {
    pub themes: Vec<InstalledTheme>,
    pub customisations: HashMap<String, theme::Definition>,
    pub disabled_themes: Vec<String>,
    pub workspace_theme: HashMap<PathBuf, String>,
    pub workspace_theme_enabled: HashMap<PathBuf, HashMap<String, bool>>,
    pub workspace_extensions: HashMap<PathBuf, HashMap<String, bool>>,
    pub theme_auto_update: Vec<String>,
    pub extension_auto_update: Vec<String>,
    next_theme: u64,
}
#[derive(Clone)]
pub(crate) struct ThemeItem {
    pub id: String,
    pub definition: theme::Definition,
    pub bundled: bool,
    pub preset: Option<theme::Preset>,
}
impl Preferences {
    pub fn themes(&self) -> Vec<ThemeItem> {
        let mut list: Vec<_> = theme::Preset::ALL
            .into_iter()
            .zip(["graphite", "aurora", "dawn", "contrast"])
            .map(|(preset, id)| ThemeItem {
                id: id.into(),
                definition: preset.definition().clone(),
                bundled: true,
                preset: Some(preset),
            })
            .collect();
        list.extend(
            self.themes
                .iter()
                .filter(|t| valid_id(&t.id))
                .map(|t| ThemeItem {
                    id: t.id.clone(),
                    definition: t.definition.clone(),
                    bundled: false,
                    preset: None,
                }),
        );
        for item in &mut list {
            if let Some(def) = self.customisations.get(&item.id) {
                item.definition = def.clone();
            }
        }
        list
    }
    pub fn install_theme(&mut self, definition: theme::Definition) -> String {
        // Reimporting an unchanged local theme does not create duplicate catalogue rows.
        if let Some(existing) = self.themes.iter().find(|t| t.definition == definition) {
            return existing.id.clone();
        }
        loop {
            self.next_theme = self.next_theme.saturating_add(1);
            let id = format!("local-{}", self.next_theme);
            if !self.themes.iter().any(|t| t.id == id) {
                self.themes.push(InstalledTheme {
                    id: id.clone(),
                    definition,
                });
                return id;
            }
        }
    }
    pub fn theme_enabled(&self, id: &str, workspace: Option<&Path>) -> bool {
        workspace
            .and_then(|w| self.workspace_theme_enabled.get(w))
            .and_then(|m| m.get(id))
            .copied()
            .unwrap_or_else(|| !self.disabled_themes.iter().any(|disabled| disabled == id))
    }
    pub fn extension_enabled(
        &self,
        id: &str,
        workspace: Option<&Path>,
        global_disabled: &[String],
    ) -> bool {
        workspace
            .and_then(|w| self.workspace_extensions.get(w))
            .and_then(|m| m.get(id))
            .copied()
            .unwrap_or_else(|| !global_disabled.iter().any(|disabled| disabled == id))
    }
}
pub(crate) fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
pub(crate) fn matches(query: &str, name: &str, description: &str) -> bool {
    let query = query.trim().to_lowercase();
    query
        .split_whitespace()
        .all(|word| name.to_lowercase().contains(word) || description.to_lowercase().contains(word))
}
impl Quark {
    pub(crate) fn workspace_root(&self) -> Option<PathBuf> {
        self.selected_project()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .or_else(|| self.settings.folders.first().cloned())
            .map(|p| p.canonicalize().unwrap_or(p))
    }
    pub(crate) fn effective_theme(&self) -> theme::Settings {
        let root = self.workspace_root();
        let catalog = &self.settings.catalog;
        let items = catalog.themes();
        if let Some(item) = root
            .as_ref()
            .and_then(|r| catalog.workspace_theme.get(r))
            .and_then(|id| items.iter().find(|t| &t.id == id))
            .filter(|t| catalog.theme_enabled(&t.id, root.as_deref()))
        {
            return item.settings();
        }
        let global = items
            .iter()
            .find(|t| t.definition == *self.settings.theme.definition());
        if global.is_none_or(|item| catalog.theme_enabled(&item.id, root.as_deref())) {
            return self.settings.theme.clone();
        }
        items
            .iter()
            .find(|t| catalog.theme_enabled(&t.id, root.as_deref()))
            .map(ThemeItem::settings)
            .unwrap_or_default()
    }
    pub(crate) fn enable_theme(&mut self, id: &str, workspace: bool) {
        let Some(item) = self
            .settings
            .catalog
            .themes()
            .into_iter()
            .find(|t| t.id == id)
        else {
            return;
        };
        if workspace {
            let Some(root) = self.workspace_root() else {
                self.notice("Open a workspace to enable a theme for it.".into());
                return;
            };
            self.settings
                .catalog
                .workspace_theme
                .insert(root.clone(), id.into());
            self.settings
                .catalog
                .workspace_theme_enabled
                .entry(root)
                .or_default()
                .insert(id.into(), true);
        } else {
            self.settings.catalog.disabled_themes.retain(|v| v != id);
            self.settings.theme = item.settings();
        }
        self.persist();
    }
    pub(crate) fn disable_theme(&mut self, id: &str, workspace: bool) {
        if workspace {
            let Some(root) = self.workspace_root() else {
                self.notice("Open a workspace to disable a theme for it.".into());
                return;
            };
            self.settings
                .catalog
                .workspace_theme_enabled
                .entry(root)
                .or_default()
                .insert(id.into(), false);
        } else if !self
            .settings
            .catalog
            .disabled_themes
            .iter()
            .any(|v| v == id)
        {
            self.settings.catalog.disabled_themes.push(id.into());
        }
        self.persist();
    }
    pub(crate) fn edit_theme(
        &mut self,
        id: &str,
        definition: theme::Definition,
    ) -> Result<(), String> {
        theme::parse(&serde_json::to_string(&definition).map_err(|e| e.to_string())?)?;
        let item = self
            .settings
            .catalog
            .themes()
            .into_iter()
            .find(|t| t.id == id)
            .ok_or("Theme no longer installed")?;
        if *self.settings.theme.definition() == item.definition {
            self.settings.theme = theme::Settings {
                preset: item.preset.unwrap_or_default(),
                custom: Some(definition.clone()),
            };
        }
        self.settings
            .catalog
            .customisations
            .insert(id.into(), definition);
        Ok(())
    }
    pub(crate) fn remove_theme(&mut self, id: &str) -> Result<(), String> {
        let item = self
            .settings
            .catalog
            .themes()
            .into_iter()
            .find(|t| t.id == id)
            .ok_or("Theme no longer installed")?;
        if item.bundled {
            return Err("Bundled themes can be disabled; they cannot be removed.".into());
        }
        if *self.settings.theme.definition() == item.definition {
            self.settings.theme = theme::Settings::default();
        }
        self.settings.catalog.themes.retain(|t| t.id != id);
        self.settings.catalog.customisations.remove(id);
        self.settings.catalog.workspace_theme.retain(|_, v| v != id);
        for entries in self.settings.catalog.workspace_theme_enabled.values_mut() {
            entries.remove(id);
        }
        self.settings.catalog.disabled_themes.retain(|v| v != id);
        self.settings.catalog.theme_auto_update.retain(|v| v != id);
        if self.editing_theme.as_deref() == Some(id) {
            self.editing_theme = None;
        }
        self.persist();
        Ok(())
    }
    pub(crate) fn extension_enabled(&self, id: &str) -> bool {
        self.settings.catalog.extension_enabled(
            id,
            self.workspace_root().as_deref(),
            &self.settings.disabled_plugins,
        )
    }
    pub(crate) fn set_extension_enabled(&mut self, id: &str, enabled: bool, workspace: bool) {
        if workspace {
            let Some(root) = self.workspace_root() else {
                self.notice("Open a workspace to change extension availability for it.".into());
                return;
            };
            self.settings
                .catalog
                .workspace_extensions
                .entry(root)
                .or_default()
                .insert(id.into(), enabled);
        } else {
            self.settings.disabled_plugins.retain(|v| v != id);
            if !enabled {
                self.settings.disabled_plugins.push(id.into());
            }
        }
        self.persist();
    }
    pub(crate) fn theme_folder(&mut self, id: &str) -> Result<PathBuf, String> {
        if !valid_id(id) {
            return Err("Invalid theme identifier".into());
        }
        let item = self
            .settings
            .catalog
            .themes()
            .into_iter()
            .find(|t| t.id == id)
            .ok_or("Theme no longer installed")?;
        let root = self
            .settings_path
            .as_ref()
            .and_then(|p| p.parent())
            .ok_or("Configuration folder unavailable")?
            .join("themes")
            .join(id);
        let json = serde_json::to_vec_pretty(&item.definition).map_err(|e| e.to_string())?;
        storage::atomic_write(&root.join("theme.json"), &json).map_err(|e| e.to_string())?;
        Ok(root)
    }
    pub(crate) fn remove_extension(&mut self, id: &str) -> Result<(), String> {
        if self.job_rx.is_some() {
            return Err("Stop the current tool job before removing an extension.".into());
        }
        let plugin = self
            .plugins
            .iter()
            .find(|p| p.id == id)
            .ok_or("Extension no longer installed")?;
        let root = plugins::directory(self.settings_path.as_deref())
            .ok_or("Configuration folder unavailable")?;
        let canonical_root = root.canonicalize().map_err(|e| e.to_string())?;
        let source = plugin.directory.canonicalize().map_err(|e| e.to_string())?;
        if source.parent() != Some(canonical_root.as_path()) || plugin.directory.is_symlink() {
            return Err("Extension is outside the managed extensions folder.".into());
        }
        let backup_root = root.parent().unwrap().join("extension-backups");
        fs::create_dir_all(&backup_root).map_err(|e| e.to_string())?;
        let backup = tempfile::Builder::new()
            .prefix("removed-")
            .tempdir_in(backup_root)
            .map_err(|e| e.to_string())?;
        fs::rename(
            &plugin.directory,
            backup.path().join(source.file_name().unwrap()),
        )
        .map_err(|e| e.to_string())?;
        let backup = backup.keep();
        self.plugins.retain(|p| p.id != id);
        self.settings.disabled_plugins.retain(|v| v != id);
        for overrides in self.settings.catalog.workspace_extensions.values_mut() {
            overrides.remove(id);
        }
        self.settings
            .catalog
            .extension_auto_update
            .retain(|v| v != id);
        self.editing_extension = None;
        self.notice(format!(
            "Extension removed. Backup kept at {}",
            backup.display()
        ));
        self.persist();
        Ok(())
    }
}
impl ThemeItem {
    pub fn settings(&self) -> theme::Settings {
        let preset = self.preset.unwrap_or_default();
        theme::Settings {
            preset,
            custom: if self.preset.is_some() && self.definition == *preset.definition() {
                None
            } else {
                Some(self.definition.clone())
            },
        }
    }
}

/// Store a distributable local ZIP, without following links or including external files.
pub(crate) fn export_zip(root: &Path, destination: &Path) -> Result<(), String> {
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    let temporary_path = temp.path().to_path_buf();
    let mut zip = zip::ZipWriter::new(temp);
    let mut pending = vec![root.to_path_buf()];
    let mut count = 0;
    let mut bytes = 0u64;
    while let Some(dir) = pending.pop() {
        count += 1;
        if count > 10000 {
            return Err("ZIP export exceeds 10,000 entries.".into());
        }
        let mut entries = fs::read_dir(&dir)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            if metadata.file_type().is_symlink() {
                return Err("ZIP export does not follow symbolic links.".into());
            }
            if metadata.is_dir() {
                pending.push(path);
                continue;
            }
            count += 1;
            bytes = bytes.saturating_add(metadata.len());
            if count > 10000 || bytes > 100 * 1024 * 1024 {
                return Err("ZIP export exceeds 10,000 files or 100 MiB.".into());
            }
            // Avoid packing an existing destination into itself when exporting beside sources.
            if path == destination || path == temporary_path {
                continue;
            }
            let name = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            zip.start_file(
                name,
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated),
            )
            .map_err(|e| e.to_string())?;
            let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
            let mut data = Vec::new();
            Read::by_ref(&mut file)
                .take(100 * 1024 * 1024 + 1)
                .read_to_end(&mut data)
                .map_err(|e| e.to_string())?;
            if data.len() > 100 * 1024 * 1024 {
                return Err("File grew beyond ZIP export limit.".into());
            }
            zip.write_all(&data).map_err(|e| e.to_string())?;
        }
    }
    let temp = zip.finish().map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist(destination).map_err(|e| e.to_string())?;
    Ok(())
}
pub(crate) fn open_folder(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    let mut command = {
        let mut c = std::process::Command::new("explorer.exe");
        c.arg(path);
        c
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut c = std::process::Command::new("open");
        c.arg(path);
        c
    };
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let mut command = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(path);
        c
    };
    command
        .spawn()
        .map(|mut child| {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        })
        .map_err(|e| e.to_string())
}
