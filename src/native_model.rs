//! GUI-independent state for the native WinUI 3 / AppKit / GTK 4 frontend.
//!
//! Keep workspaces, editor buffers and designer layouts independent of any
//! renderer so the three OS backends never diverge on file safety.

use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

const MAX_SOURCE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_LAYOUT_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub path: PathBuf,
    pub saved: String,
    pub text: String,
}

impl Document {
    pub fn open(path: &Path) -> Result<Self, String> {
        let path = path.canonicalize().map_err(|e| e.to_string())?;
        let metadata = fs::metadata(&path).map_err(|e| e.to_string())?;
        if !metadata.is_file() || metadata.len() > MAX_SOURCE_BYTES {
            return Err("Not a regular source file, or exceeds 16 MiB".into());
        }
        let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        if text.contains('\0') {
            return Err("Cannot open a binary file as text".into());
        }
        Ok(Self {
            path,
            saved: text.clone(),
            text,
        })
    }

    pub fn dirty(&self) -> bool {
        self.text != self.saved
    }

    pub fn save(&mut self) -> Result<(), String> {
        // Refuse external modifications/deletion instead of silently overwriting.
        crate::storage::save_checked(&self.path, &self.text, &self.saved).map_err(|error| {
            format!("File was modified externally or cannot be saved: {error:?}")
        })?;
        self.saved = self.text.clone();
        Ok(())
    }
}

/// Matches Quark's existing version-1 .quarkui schema exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlKind {
    Label,
    Button,
    TextField,
    Checkbox,
}

impl ControlKind {
    pub const ALL: [Self; 4] = [Self::Label, Self::Button, Self::TextField, Self::Checkbox];

    pub fn title(self) -> &'static str {
        match self {
            Self::Label => "Label",
            Self::Button => "Button",
            Self::TextField => "Text Box",
            Self::Checkbox => "Check Box",
        }
    }

    fn dimensions(self) -> (f32, f32) {
        match self {
            Self::Label => (160.0, 30.0),
            Self::Button => (130.0, 36.0),
            Self::TextField => (180.0, 36.0),
            Self::Checkbox => (160.0, 30.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Control {
    pub id: u64,
    pub kind: ControlKind,
    pub name: String,
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Editable geometry fields shown in Quark's native Properties window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeometryField {
    X,
    Y,
    Width,
    Height,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub version: u32,
    pub title: String,
    pub width: f32,
    pub height: f32,
    pub controls: Vec<Control>,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            version: 1,
            title: "Main Window".into(),
            width: 720.0,
            height: 480.0,
            controls: Vec::new(),
        }
    }
}

impl Layout {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("Unsupported Quark UI layout version".into());
        }
        if !self.width.is_finite()
            || !self.height.is_finite()
            || !(320.0..=4096.0).contains(&self.width)
            || !(240.0..=4096.0).contains(&self.height)
            || self.title.len() > 4096
            || self.controls.len() > 500
        {
            return Err("Invalid canvas size, title or control count".into());
        }
        let mut seen = std::collections::HashSet::new();
        for control in &self.controls {
            if !seen.insert(control.id)
                || !control.x.is_finite()
                || !control.y.is_finite()
                || !control.width.is_finite()
                || !control.height.is_finite()
                || control.width < 20.0
                || control.height < 20.0
                || control.x < 0.0
                || control.y < 0.0
                || control.x + control.width > self.width
                || control.y + control.height > self.height
                || control.name.len() > 128
                || control.text.len() > 4096
            {
                return Err("Invalid designer control geometry or metadata".into());
            }
        }
        Ok(())
    }

    pub fn add(&mut self, kind: ControlKind, x: f32, y: f32) -> Result<u64, String> {
        if !x.is_finite() || !y.is_finite() || self.controls.len() >= 500 {
            return Err("Invalid position or too many controls".into());
        }
        let id = self
            .controls
            .iter()
            .map(|control| control.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or("Control ID overflow")?;
        let (width, height) = kind.dimensions();
        self.controls.push(Control {
            id,
            kind,
            name: format!("{}{}", kind.title().replace(' ', ""), id),
            text: kind.title().into(),
            x: x.clamp(0.0, self.width - width),
            y: y.clamp(0.0, self.height - height),
            width,
            height,
        });
        Ok(id)
    }

    pub fn hit_test(&self, x: f32, y: f32) -> Option<u64> {
        self.controls
            .iter()
            .rev()
            .find(|control| {
                x >= control.x
                    && x <= control.x + control.width
                    && y >= control.y
                    && y <= control.y + control.height
            })
            .map(|control| control.id)
    }

    /// Apply a property-grid geometry change atomically. Invalid values
    /// never leave a .quarkui layout unsaveable.
    pub fn set_geometry(
        &mut self,
        id: u64,
        field: GeometryField,
        value: f32,
    ) -> Result<(), String> {
        if !value.is_finite() {
            return Err("Geometry values must be finite numbers".into());
        }
        let control = self
            .controls
            .iter_mut()
            .find(|control| control.id == id)
            .ok_or("Select a control first")?;
        let previous = control.clone();
        match field {
            GeometryField::X => control.x = value,
            GeometryField::Y => control.y = value,
            GeometryField::Width => control.width = value,
            GeometryField::Height => control.height = value,
        }
        if let Err(error) = self.validate() {
            if let Some(control) = self.controls.iter_mut().find(|control| control.id == id) {
                *control = previous;
            }
            return Err(error);
        }
        Ok(())
    }

    pub fn position(&mut self, id: u64, x: f32, y: f32) -> bool {
        if !x.is_finite() || !y.is_finite() {
            return false;
        }
        let (width, height) = (self.width, self.height);
        let Some(control) = self.controls.iter_mut().find(|item| item.id == id) else {
            return false;
        };
        let x = x.clamp(0.0, width - control.width);
        let y = y.clamp(0.0, height - control.height);
        let changed = (control.x, control.y) != (x, y);
        control.x = x;
        control.y = y;
        changed
    }
}

#[derive(Debug, Clone)]
pub struct LayoutDocument {
    pub path: PathBuf,
    pub layout: Layout,
    saved: String,
}

impl LayoutDocument {
    pub fn open(path: &Path) -> Result<Self, String> {
        let path = path.canonicalize().map_err(|e| e.to_string())?;
        let metadata = fs::metadata(&path).map_err(|e| e.to_string())?;
        if !metadata.is_file() || metadata.len() > MAX_LAYOUT_BYTES {
            return Err("Not a valid .quarkui file or exceeds 1 MiB".into());
        }
        let saved = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let layout: Layout = serde_json::from_str(&saved).map_err(|e| e.to_string())?;
        layout.validate()?;
        Ok(Self {
            path,
            layout,
            saved,
        })
    }

    pub fn dirty(&self) -> bool {
        serde_json::to_string_pretty(&self.layout).is_ok_and(|current| current != self.saved)
    }

    pub fn save(&mut self) -> Result<(), String> {
        self.layout.validate()?;
        let next = serde_json::to_string_pretty(&self.layout).map_err(|e| e.to_string())?;
        crate::storage::save_checked(&self.path, &next, &self.saved)
            .map_err(|e| format!("Layout was modified externally or cannot be saved: {e:?}"))?;
        self.saved = next;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Workspace {
    pub projects: Vec<PathBuf>,
    pub folders: Vec<PathBuf>,
    pub recent: Vec<PathBuf>,
    pub selected: Option<PathBuf>,
}

impl Workspace {
    pub fn remember(&mut self, path: PathBuf, project: bool) {
        let path = path.canonicalize().unwrap_or(path);
        let target = if project {
            &mut self.projects
        } else {
            &mut self.folders
        };
        if !target.contains(&path) {
            target.push(path.clone());
        }
        self.recent.retain(|p| p != &path);
        self.recent.insert(0, path.clone());
        self.recent.truncate(20);
        if project {
            self.selected = Some(path);
        }
    }

    pub fn open(path: &Path) -> Result<Self, String> {
        match fs::read_to_string(path) {
            Ok(content) => serde_json::from_str(&content).map_err(|e| e.to_string()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error.to_string()),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let text = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        crate::storage::atomic_write(path, &text).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_save_rejects_concurrent_modification_and_keeps_text() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.hy");
        fs::write(&path, "original").unwrap();
        let mut doc = Document::open(&path).unwrap();
        doc.text = "unsaved".into();
        fs::write(&path, "external").unwrap();
        assert!(doc.save().is_err());
        assert_eq!(doc.text, "unsaved");
        assert_eq!(fs::read_to_string(path).unwrap(), "external");
    }

    #[test]
    fn property_grid_edits_control_position_and_dimensions() {
        let mut layout = Layout::default();
        let id = layout.add(ControlKind::Button, 20.0, 30.0).unwrap();
        layout.set_geometry(id, GeometryField::X, 80.0).unwrap();
        layout.set_geometry(id, GeometryField::Y, 90.0).unwrap();
        layout
            .set_geometry(id, GeometryField::Width, 205.0)
            .unwrap();
        layout
            .set_geometry(id, GeometryField::Height, 48.0)
            .unwrap();
        let control = &layout.controls[0];
        assert_eq!(
            (control.x, control.y, control.width, control.height),
            (80.0, 90.0, 205.0, 48.0)
        );
        assert!(layout.validate().is_ok());
    }

    #[test]
    fn invalid_geometry_edits_preserve_the_original_control() {
        let mut layout = Layout::default();
        let id = layout.add(ControlKind::TextField, 30.0, 30.0).unwrap();
        let original = layout.controls[0].clone();
        for (field, value) in [
            (GeometryField::Width, 10.0),
            (GeometryField::Width, 10000.0),
            (GeometryField::X, -1.0),
            (GeometryField::Y, f32::NAN),
            (GeometryField::Height, f32::INFINITY),
        ] {
            assert!(layout.set_geometry(id, field, value).is_err());
            assert_eq!(layout.controls[0], original);
        }
    }

    #[test]
    fn source_round_trip_can_open_edit_and_save() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.hy");
        fs::write(&path, "original").unwrap();
        let mut doc = Document::open(&path).unwrap();
        doc.text = "new text".into();
        assert!(doc.dirty());
        doc.save().unwrap();
        assert!(!doc.dirty());
        assert_eq!(fs::read_to_string(path).unwrap(), "new text");
    }

    #[test]
    fn layout_is_compatible_with_existing_quarkui_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("Main.quarkui");
        let mut layout = Layout::default();
        let id = layout.add(ControlKind::Button, 120.0, 90.0).unwrap();
        assert_eq!(layout.hit_test(125.0, 96.0), Some(id));
        assert!(layout.position(id, 300.0, 120.0));
        fs::write(&path, serde_json::to_string_pretty(&layout).unwrap()).unwrap();
        let mut document = LayoutDocument::open(&path).unwrap();
        document.layout.controls[0].text = "Launch".into();
        document.save().unwrap();
        assert_eq!(
            LayoutDocument::open(&path).unwrap().layout.controls[0].text,
            "Launch"
        );
    }

    #[test]
    fn untrusted_layouts_are_bounded_and_invalid_geometry_is_rejected() {
        let mut layout = Layout::default();
        let id = layout.add(ControlKind::Checkbox, 2.0, 3.0).unwrap();
        layout.controls[0].x = f32::NAN;
        assert!(layout.validate().is_err());
        assert!(!layout.position(id, f32::NAN, 100.0));
        layout.controls[0].x = 10000.0;
        assert!(layout.validate().is_err());
    }

    #[test]
    fn native_workspace_recent_items_are_deduplicated_and_persisted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspace.json");
        let project = dir.path().join("app.hyproj");
        fs::write(&project, "type='linux'").unwrap();
        let mut workspace = Workspace::default();
        workspace.remember(project.clone(), true);
        workspace.remember(project.clone(), true);
        assert_eq!(workspace.projects.len(), 1);
        assert_eq!(workspace.recent.len(), 1);
        workspace.save(&path).unwrap();
        assert_eq!(
            Workspace::open(&path).unwrap().selected,
            Some(project.canonicalize().unwrap())
        );
    }

    #[test]
    fn layout_save_refuses_external_modification() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Main.quarkui");
        fs::write(
            &path,
            serde_json::to_string_pretty(&Layout::default()).unwrap(),
        )
        .unwrap();
        let mut document = LayoutDocument::open(&path).unwrap();
        document.layout.add(ControlKind::Label, 25.0, 35.0).unwrap();
        fs::write(&path, "external").unwrap();
        assert!(document.save().is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "external");
    }
}
