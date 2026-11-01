//! User-controlled sizes for Quark's native IDE tool windows.
//!
//! Grid columns/rows use Fixed tracks for the tool panes and Fill for the
//! editor/designer. Maximising the OS window therefore affects the editor,
//! rather than enlarging Solution Explorer, Toolbox, or Output.
//! The narrow native canvas grips update these values only from pointer drags.

use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

pub const HANDLE: f64 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Toolbox,
    Inspector,
    Output,
    Properties,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PaneSizes {
    pub toolbox: f64,
    pub inspector: f64,
    pub output: f64,
    pub properties: f64,
}

impl Default for PaneSizes {
    fn default() -> Self {
        Self {
            toolbox: 210.0,
            inspector: 325.0,
            output: 215.0,
            properties: 260.0,
        }
    }
}

impl PaneSizes {
    pub fn sanitize(mut self) -> Self {
        let defaults = Self::default();
        for (value, fallback, min, max) in [
            (&mut self.toolbox, defaults.toolbox, 155.0, 480.0),
            (&mut self.inspector, defaults.inspector, 220.0, 580.0),
            (&mut self.output, defaults.output, 100.0, 470.0),
            (&mut self.properties, defaults.properties, 150.0, 460.0),
        ] {
            *value = if value.is_finite() {
                value.clamp(min, max)
            } else {
                fallback
            };
        }
        self
    }

    pub fn resized(self, pane: Pane, delta: f64) -> Self {
        if !delta.is_finite() {
            return self;
        }
        let mut next = self;
        match pane {
            Pane::Toolbox => next.toolbox += delta,
            Pane::Inspector => next.inspector -= delta,
            Pane::Output => next.output -= delta,
            Pane::Properties => next.properties -= delta,
        }
        next.sanitize()
    }

    /// Model the useful centre area with the same fixed-track arithmetic as
    /// the native grid. No window resize should change the sizes of tool panes.
    /// This is a geometry contract used by the regression tests.
    #[cfg(test)]
    pub fn editor_area(self, window_width: f64, window_height: f64) -> (f64, f64) {
        (
            (window_width - self.toolbox - self.inspector - 2.0 * HANDLE).max(0.0),
            (window_height - self.output - HANDLE).max(0.0),
        )
    }

    pub fn open(path: &Path) -> Result<Self, String> {
        match fs::read_to_string(path) {
            Ok(data) => serde_json::from_str::<Self>(&data)
                .map(Self::sanitize)
                .map_err(|e| e.to_string()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error.to_string()),
        }
    }

    pub fn save(self, path: &Path) -> Result<(), String> {
        let data = serde_json::to_vec_pretty(&self.sanitize()).map_err(|e| e.to_string())?;
        crate::storage::atomic_write(path, &data).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn increasing_window_size_expands_only_the_editor() {
        let panes = PaneSizes::default();
        let (small_w, small_h) = panes.editor_area(1200.0, 750.0);
        let (large_w, large_h) = panes.editor_area(1900.0, 1120.0);
        assert_eq!(large_w - small_w, 700.0);
        assert_eq!(large_h - small_h, 370.0);
        assert_eq!(panes, PaneSizes::default());
    }

    #[test]
    fn dragging_left_and_right_changes_only_the_selected_pane() {
        let original = PaneSizes::default();
        let left = original.resized(Pane::Toolbox, 60.0);
        let right = original.resized(Pane::Inspector, -80.0);
        assert_eq!(left.toolbox, original.toolbox + 60.0);
        assert_eq!(left.inspector, original.inspector);
        assert_eq!(right.inspector, original.inspector + 80.0);
        assert_eq!(right.toolbox, original.toolbox);
    }

    #[test]
    fn dragging_bottom_and_properties_maintains_other_sizes() {
        let original = PaneSizes::default();
        let output = original.resized(Pane::Output, -30.0);
        assert_eq!(output.output, original.output + 30.0);
        assert_eq!(output.properties, original.properties);
        let props = original.resized(Pane::Properties, -75.0);
        assert_eq!(props.properties, original.properties + 75.0);
        assert_eq!(props.output, original.output);
    }

    #[test]
    fn layout_rejects_unbounded_nan_and_infinite_sizes() {
        let mut sizes = PaneSizes::default();
        sizes.inspector = f64::NAN;
        sizes.toolbox = f64::INFINITY;
        sizes.output = -40_000.0;
        sizes.properties = 200_000.0;
        let safe = sizes.sanitize();
        assert_eq!(safe.inspector, PaneSizes::default().inspector);
        assert_eq!(safe.toolbox, PaneSizes::default().toolbox);
        assert_eq!(safe.output, 100.0);
        assert_eq!(safe.properties, 460.0);
        assert_eq!(safe.resized(Pane::Toolbox, f64::NAN), safe);
    }

    #[test]
    fn user_panel_sizes_survive_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pane-layout.json");
        let updated = PaneSizes::default()
            .resized(Pane::Toolbox, 38.0)
            .resized(Pane::Inspector, -70.0)
            .resized(Pane::Output, -45.0);
        updated.save(&path).unwrap();
        assert_eq!(PaneSizes::open(&path).unwrap(), updated);
    }

    #[test]
    fn corrupt_layout_does_not_overwrite_existing_configuration() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pane-layout.json");
        fs::write(&path, b"{not-json").unwrap();
        assert!(PaneSizes::open(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"{not-json");
    }

    #[test]
    fn editor_respects_zero_available_space_without_negative_track_values() {
        let (w, h) = PaneSizes::default().editor_area(300.0, 100.0);
        assert_eq!((w, h), (0.0, 0.0));
    }
}
