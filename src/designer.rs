//! Quark's design-time Hydrogen UI layout editor.
//! Layouts are JSON documents (.quarkui). They are deliberately not represented
//! as runnable native widgets until Hydrogen.UI has a supported runtime binding.

use crate::storage;
use eframe::egui::{self, Align2, Color32, FontId, RichText, Sense, Stroke};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::PathBuf,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Label,
    Button,
    TextField,
    Checkbox,
}

impl Kind {
    pub const ALL: [Self; 4] = [Self::Label, Self::Button, Self::TextField, Self::Checkbox];
    pub fn label(self) -> &'static str {
        match self {
            Self::Label => "Label",
            Self::Button => "Button",
            Self::TextField => "Text box",
            Self::Checkbox => "Check box",
        }
    }
    fn size(self) -> [f32; 2] {
        match self {
            Self::Label => [160.0, 30.0],
            Self::Button => [130.0, 36.0],
            Self::TextField => [180.0, 36.0],
            Self::Checkbox => [160.0, 30.0],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Control {
    pub id: u64,
    pub kind: Kind,
    pub name: String,
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
            return Err("Unsupported layout version (expected version 1)".into());
        }
        if !(320.0..=4096.0).contains(&self.width) || !(240.0..=4096.0).contains(&self.height) {
            return Err("Canvas must be between 320x240 and 4096x4096".into());
        }
        if self.controls.len() > 500 {
            return Err("Layouts may contain at most 500 controls".into());
        }
        let mut ids = std::collections::HashSet::new();
        for control in &self.controls {
            if !ids.insert(control.id)
                || !control.x.is_finite()
                || !control.y.is_finite()
                || !control.width.is_finite()
                || !control.height.is_finite()
                || control.width < 20.0
                || control.height < 20.0
                || control.x < 0.0
                || control.y < 0.0
                || control.x + control.width > self.width + 0.01
                || control.y + control.height > self.height + 0.01
                || control.name.len() > 128
                || control.text.len() > 4096
            {
                return Err("Invalid control ID, bounds, name or text".into());
            }
        }
        Ok(())
    }

    pub fn add(&mut self, kind: Kind, x: f32, y: f32) -> Result<u64, String> {
        if self.controls.len() >= 500 {
            return Err("The layout already contains 500 controls".into());
        }
        let id = self
            .controls
            .iter()
            .map(|item| item.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or("Control IDs exhausted")?;
        let [width, height] = kind.size();
        let name = format!("{}{}", kind.label().replace(' ', ""), id);
        self.controls.push(Control {
            id,
            kind,
            name,
            text: kind.label().into(),
            x: x.clamp(0.0, self.width - width),
            y: y.clamp(0.0, self.height - height),
            width,
            height,
        });
        Ok(id)
    }

    pub fn move_control(&mut self, id: u64, dx: f32, dy: f32) -> bool {
        if !dx.is_finite() || !dy.is_finite() {
            return false;
        }
        let width = self.width;
        let height = self.height;
        if let Some(control) = self.controls.iter_mut().find(|item| item.id == id) {
            let x = (control.x + dx).clamp(0.0, width - control.width);
            let y = (control.y + dy).clamp(0.0, height - control.height);
            let changed = x != control.x || y != control.y;
            control.x = x;
            control.y = y;
            return changed;
        }
        false
    }
}

pub struct Designer {
    pub path: PathBuf,
    pub layout: Layout,
    pub dirty: bool,
    pub selected: Option<u64>,
    dragging: Option<Kind>,
    saved: String,
}

impl Designer {
    pub fn open(path: PathBuf) -> Result<Self, String> {
        let saved = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        if saved.len() > 1024 * 1024 {
            return Err("Layout exceeds the 1 MiB size limit".into());
        }
        let layout: Layout = serde_json::from_str(&saved).map_err(|e| e.to_string())?;
        layout.validate()?;
        Ok(Self {
            path,
            layout,
            dirty: false,
            selected: None,
            dragging: None,
            saved,
        })
    }

    pub fn create(path: PathBuf) -> Result<Self, String> {
        let layout = Layout::default();
        let saved = serde_json::to_string_pretty(&layout).map_err(|e| e.to_string())?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| format!("Cannot create layout: {e}"))?;
        file.write_all(saved.as_bytes())
            .map_err(|e| e.to_string())?;
        Self::open(path)
    }

    pub fn save(&mut self) -> Result<(), String> {
        self.layout.validate()?;
        let content = serde_json::to_string_pretty(&self.layout).map_err(|e| e.to_string())?;
        storage::save_checked(&self.path, &content, &self.saved).map_err(|error| {
            format!("Layout save failed: {error:?}. File may have changed on disk.")
        })?;
        self.saved = content;
        self.dirty = false;
        Ok(())
    }

    pub fn add(&mut self, kind: Kind, x: f32, y: f32) -> Result<(), String> {
        let id = self.layout.add(kind, x, y)?;
        self.selected = Some(id);
        self.dirty = true;
        Ok(())
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("UI Designer");
            ui.separator();
            ui.label(&self.layout.title);
            if self.dirty {
                ui.label(RichText::new("● Unsaved changes").color(Color32::YELLOW));
            }
            if ui.button("Save layout").clicked() {
                if let Err(error) = self.save() {
                    ui.ctx().debug_painter().text(
                        ui.ctx().screen_rect().center(),
                        Align2::CENTER_CENTER,
                        error,
                        FontId::proportional(16.0),
                        Color32::RED,
                    );
                }
            }
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.set_width(145.0);
                ui.heading("Toolbox");
                ui.label(RichText::new("Drag to the canvas").small());
                ui.add_space(6.0);
                for kind in Kind::ALL {
                    let response = ui.add(
                        egui::Label::new(format!("⊕  {}", kind.label()))
                            .sense(Sense::click_and_drag()),
                    );
                    if response.drag_started() {
                        self.dragging = Some(kind);
                    }
                    if response.clicked() {
                        let _ = self.add(kind, 26.0, 26.0);
                    }
                }
                ui.add_space(12.0);
                ui.label(RichText::new("Click to select; drag to move.").small());
            });
            ui.separator();
            ui.vertical(|ui| {
                ui.heading("Design surface");
                ui.label(
                    RichText::new("Visual layout preview · not yet generated Hydrogen code")
                        .small(),
                );
                egui::ScrollArea::both()
                    .id_salt("quark-designer-canvas-scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let (canvas, response) = ui.allocate_exact_size(
                            egui::vec2(self.layout.width, self.layout.height),
                            Sense::click(),
                        );
                        ui.painter()
                            .rect_filled(canvas, 4.0, Color32::from_rgb(244, 247, 251));
                        if response.clicked() {
                            self.selected = None;
                        }
                        let mut dragged = None;
                        for control in &self.layout.controls {
                            let rect = egui::Rect::from_min_size(
                                canvas.min + egui::vec2(control.x, control.y),
                                egui::vec2(control.width, control.height),
                            );
                            self.paint_control(ui.painter(), rect, control);
                            let hit = ui.interact(
                                rect,
                                ui.id().with(("designer-control", control.id)),
                                Sense::click_and_drag(),
                            );
                            if hit.clicked() || hit.drag_started() {
                                self.selected = Some(control.id);
                            }
                            if hit.dragged() {
                                dragged = Some(control.id);
                            }
                        }
                        if let Some(id) = dragged {
                            let delta = ui.input(|input| input.pointer.delta());
                            self.dirty |= self.layout.move_control(id, delta.x, delta.y);
                        }
                        if let Some(kind) = self.dragging {
                            let pointer = ui.input(|input| input.pointer.latest_pos());
                            if let Some(pointer) = pointer {
                                if ui.input(|input| input.pointer.any_released()) {
                                    if canvas.contains(pointer) {
                                        let local = pointer - canvas.min;
                                        let _ = self.add(kind, local.x, local.y);
                                    }
                                    self.dragging = None;
                                } else {
                                    ui.ctx().debug_painter().text(
                                        pointer,
                                        Align2::LEFT_BOTTOM,
                                        kind.label(),
                                        FontId::proportional(14.0),
                                        Color32::LIGHT_BLUE,
                                    );
                                }
                            }
                        }
                    });
            });
            ui.separator();
            ui.vertical(|ui| {
                ui.set_width(205.0);
                ui.heading("Properties");
                ui.separator();
                if let Some(id) = self.selected {
                    if let Some(control) = self.layout.controls.iter_mut().find(|c| c.id == id) {
                        let mut changed = false;
                        ui.label(format!("{} · {}", control.kind.label(), control.id));
                        ui.label("Name");
                        changed |= ui.text_edit_singleline(&mut control.name).changed();
                        ui.label("Text");
                        changed |= ui.text_edit_singleline(&mut control.text).changed();
                        let max_x = (self.layout.width - control.width).max(0.0);
                        let max_y = (self.layout.height - control.height).max(0.0);
                        let max_width = (self.layout.width - control.x).max(20.0);
                        let max_height = (self.layout.height - control.y).max(20.0);
                        for (label, value, min, max) in [
                            ("X", &mut control.x, 0.0_f32, max_x),
                            ("Y", &mut control.y, 0.0_f32, max_y),
                            ("Width", &mut control.width, 20.0_f32, max_width),
                            ("Height", &mut control.height, 20.0_f32, max_height),
                        ] {
                            ui.horizontal(|ui| {
                                ui.label(label);
                                changed |= ui
                                    .add(egui::DragValue::new(value).range(min..=max))
                                    .changed();
                            });
                        }
                        self.dirty |= changed;
                    }
                    if ui.button("Delete selected").clicked() {
                        self.layout.controls.retain(|c| c.id != id);
                        self.selected = None;
                        self.dirty = true;
                    }
                } else {
                    ui.label("Select a control to edit its properties.");
                }
                ui.separator();
                ui.heading("Window");
                ui.label("Title");
                self.dirty |= ui.text_edit_singleline(&mut self.layout.title).changed();
                ui.label(format!(
                    "{:.0} × {:.0}",
                    self.layout.width, self.layout.height
                ));
            });
        });
    }

    fn paint_control(&self, painter: &egui::Painter, rect: egui::Rect, control: &Control) {
        let foreground = Color32::from_rgb(35, 43, 54);
        match control.kind {
            Kind::Label => {
                painter.text(
                    rect.left_center(),
                    Align2::LEFT_CENTER,
                    &control.text,
                    FontId::proportional(15.0),
                    foreground,
                );
            }
            Kind::Button => {
                painter.rect_filled(rect, 5.0, Color32::from_rgb(212, 228, 247));
                painter.text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    &control.text,
                    FontId::proportional(15.0),
                    foreground,
                );
            }
            Kind::TextField => {
                painter.rect_filled(rect, 3.0, Color32::WHITE);
                painter.text(
                    rect.left_center() + egui::vec2(8.0, 0.0),
                    Align2::LEFT_CENTER,
                    &control.text,
                    FontId::proportional(15.0),
                    foreground,
                );
            }
            Kind::Checkbox => {
                let check = egui::Rect::from_min_size(
                    rect.left_center() + egui::vec2(4.0, -8.0),
                    egui::vec2(16.0, 16.0),
                );
                painter.rect_filled(check, 2.0, Color32::WHITE);
                painter.text(
                    check.right_center() + egui::vec2(8.0, 0.0),
                    Align2::LEFT_CENTER,
                    &control.text,
                    FontId::proportional(15.0),
                    foreground,
                );
            }
        }
        if self.selected == Some(control.id) {
            let outline = Stroke::new(2.0_f32, Color32::from_rgb(55, 120, 230));
            painter.line_segment([rect.left_top(), rect.right_top()], outline);
            painter.line_segment([rect.right_top(), rect.right_bottom()], outline);
            painter.line_segment([rect.right_bottom(), rect.left_bottom()], outline);
            painter.line_segment([rect.left_bottom(), rect.left_top()], outline);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_move_and_clamp_controls_to_canvas() {
        let mut layout = Layout::default();
        let id = layout.add(Kind::Button, 99999.0, 99999.0).unwrap();
        assert_eq!(layout.controls[0].x, 590.0);
        assert_eq!(layout.controls[0].y, 444.0);
        assert!(layout.move_control(id, -1000.0, -1000.0));
        assert_eq!((layout.controls[0].x, layout.controls[0].y), (0.0, 0.0));
        assert!(layout.validate().is_ok());
    }

    #[test]
    fn layout_roundtrip_create_edit_save_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Form.quarkui");
        let mut editor = Designer::create(path.clone()).unwrap();
        assert!(Designer::create(path.clone()).is_err());
        editor.add(Kind::Label, 50.0, 80.0).unwrap();
        editor.layout.controls[0].text = "Hello UI".into();
        editor.save().unwrap();
        assert!(!editor.dirty);
        let reopened = Designer::open(path).unwrap();
        assert_eq!(reopened.layout.controls[0].text, "Hello UI");
        assert_eq!(reopened.layout.controls[0].x, 50.0);
    }

    #[test]
    fn refuses_invalid_and_untrusted_layout_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Bad.quarkui");
        fs::write(
            &path,
            r#"{"version":999,"title":"bad","width":720,"height":480,"controls":[]}"#,
        )
        .unwrap();
        assert!(Designer::open(path.clone()).is_err());
        let mut layout = Layout::default();
        layout.add(Kind::Checkbox, 8.0, 8.0).unwrap();
        layout.controls[0].width = f32::NAN;
        assert!(layout.validate().is_err());
        assert!(Designer::open(path).is_err());
    }

    #[test]
    fn detects_external_modification_on_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Form.quarkui");
        let mut editor = Designer::create(path.clone()).unwrap();
        editor.add(Kind::TextField, 50.0, 60.0).unwrap();
        fs::write(&path, "external edit").unwrap();
        assert!(editor.save().is_err());
        assert!(editor.dirty);
        assert_eq!(fs::read_to_string(path).unwrap(), "external edit");
    }

    #[test]
    fn rejects_duplicate_ids_and_off_canvas_widgets() {
        let mut layout = Layout::default();
        layout.add(Kind::Label, 10.0, 10.0).unwrap();
        layout.add(Kind::Button, 20.0, 20.0).unwrap();
        layout.controls[1].id = layout.controls[0].id;
        assert!(layout.validate().is_err());
        layout.controls[1].id += 1;
        layout.controls[1].x = 700.0;
        assert!(layout.validate().is_err());
    }
}
