//! Shared Quark docking model. Native renderers on GTK, AppKit and WinUI
//! consume this state; panels never choose their own host location.
//!
//! A section is registered once with a default zone and may subsequently
//! move, stack, pin, collapse or auto-hide. The JSON representation is stable
//! across operating systems and does not contain native widget pointers.

use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fs, path::Path};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockZone {
    Left,
    Right,
    Bottom,
}

impl DockZone {
    pub const ALL: [Self; 3] = [Self::Left, Self::Right, Self::Bottom];

    pub fn title(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Right => "Right",
            Self::Bottom => "Bottom",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DockId {
    Toolbox,
    Symbols,
    Search,
    Server,
    Data,
    Solution,
    Properties,
    Output,
    ErrorList,
    Debug,
    TestResults,
    FindResults,
}

impl DockId {
    pub const ALL: [Self; 12] = [
        Self::Toolbox,
        Self::Symbols,
        Self::Search,
        Self::Server,
        Self::Data,
        Self::Solution,
        Self::Properties,
        Self::Output,
        Self::ErrorList,
        Self::Debug,
        Self::TestResults,
        Self::FindResults,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Toolbox => "Toolbox",
            Self::Symbols => "Symbols",
            Self::Search => "Search",
            Self::Server => "Server Explorer",
            Self::Data => "Data Sources",
            Self::Solution => "Solution Explorer",
            Self::Properties => "Properties",
            Self::Output => "Output",
            Self::ErrorList => "Error List",
            Self::Debug => "Debug",
            Self::TestResults => "Test Results",
            Self::FindResults => "Find Results",
        }
    }

    pub fn default_zone(self) -> DockZone {
        match self {
            Self::Toolbox | Self::Symbols | Self::Search | Self::Server | Self::Data => {
                DockZone::Left
            }
            Self::Solution | Self::Properties => DockZone::Right,
            Self::Output
            | Self::ErrorList
            | Self::Debug
            | Self::TestResults
            | Self::FindResults => DockZone::Bottom,
        }
    }

    /// Deliberately text-only symbols; platform presenters can replace these
    /// with themeable vector icons without changing persisted layout IDs.
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Toolbox => "▦",
            Self::Symbols => "◇",
            Self::Search => "⌕",
            Self::Server => "▤",
            Self::Data => "◫",
            Self::Solution => "▧",
            Self::Properties => "⚙",
            Self::Output => "▣",
            Self::ErrorList => "!",
            Self::Debug => "●",
            Self::TestResults => "✓",
            Self::FindResults => "⌕",
        }
    }
}

/// The sole section-registration API. Presenter implementations supply the
/// body widget separately, so application functionality is not part of this
/// serializable layout model.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DockSection {
    pub id: DockId,
    pub zone: DockZone,
    pub pinned: bool,
    pub collapsed: bool,
    pub order: u16,
}

impl DockSection {
    pub fn new(id: DockId, zone: DockZone) -> Self {
        Self {
            id,
            zone,
            pinned: false,
            collapsed: false,
            order: 0,
        }
    }

    pub fn pinned(mut self, value: bool) -> Self {
        self.pinned = value;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DockLayout {
    pub sections: Vec<DockSection>,
    /// Layout-format version; zero identifies arrangements saved before
    /// persistent Toolbox/Symbols and the single-row bottom tool strip.
    #[serde(default)]
    pub layout_version: u8,
}

impl Default for DockLayout {
    fn default() -> Self {
        let mut layout = Self {
            sections: Vec::new(),
            layout_version: 2,
        };
        for id in DockId::ALL {
            // Default unpinned tool windows auto-hide after losing focus.
            // The user can pin any section to reserve its dock space.
            let pinned = matches!(
                id,
                DockId::Toolbox
                    | DockId::Symbols
                    | DockId::Solution
                    | DockId::Properties
                    | DockId::Output
                    | DockId::ErrorList
                    | DockId::Debug
                    | DockId::TestResults
                    | DockId::FindResults
            );
            layout.register(DockSection::new(id, id.default_zone()).pinned(pinned));
        }
        layout
    }
}

impl DockLayout {
    pub fn register(&mut self, mut section: DockSection) {
        if self
            .sections
            .iter()
            .any(|existing| existing.id == section.id)
        {
            return;
        }
        section.order = self
            .sections
            .iter()
            .filter(|existing| existing.zone == section.zone)
            .map(|existing| existing.order)
            .max()
            .map_or(0, |last| last.saturating_add(1));
        self.sections.push(section);
    }

    pub fn section(&self, id: DockId) -> Option<&DockSection> {
        self.sections.iter().find(|entry| entry.id == id)
    }

    pub fn section_mut(&mut self, id: DockId) -> Option<&mut DockSection> {
        self.sections.iter_mut().find(|entry| entry.id == id)
    }

    pub fn move_to(&mut self, id: DockId, zone: DockZone) -> bool {
        let order = self
            .sections
            .iter()
            .filter(|entry| entry.zone == zone)
            .map(|entry| entry.order)
            .max()
            .map_or(0, |last| last.saturating_add(1));
        if let Some(section) = self.section_mut(id) {
            if section.zone == zone {
                return false;
            }
            section.zone = zone;
            section.order = order;
            true
        } else {
            false
        }
    }

    pub fn pin(&mut self, id: DockId, pinned: bool) {
        if let Some(section) = self.section_mut(id) {
            section.pinned = pinned;
        }
    }

    pub fn collapse(&mut self, id: DockId, collapsed: bool) {
        if let Some(section) = self.section_mut(id) {
            section.collapsed = collapsed;
        }
    }

    pub fn sections_in(&self, zone: DockZone) -> Vec<&DockSection> {
        let mut sections: Vec<_> = self
            .sections
            .iter()
            .filter(|section| section.zone == zone)
            .collect();
        sections.sort_by_key(|section| section.order);
        sections
    }

    /// An unpinned pane is visible only while explicitly activated by its
    /// rail or focused as a temporary tool window. The layout itself remains
    /// unchanged, so its preferred size survives auto-hide.
    pub fn visible(&self, id: DockId, active: Option<DockId>) -> bool {
        self.section(id)
            .is_some_and(|section| section.pinned || active == Some(id))
    }

    /// Move a section before a sibling within the requested dock zone.
    /// Reassign dense order values to prevent overflow after many drags.
    pub fn reorder_before(&mut self, id: DockId, sibling: DockId) -> bool {
        if id == sibling {
            return false;
        }
        let Some(zone) = self.section(sibling).map(|section| section.zone) else {
            return false;
        };
        if self.section(id).is_none() {
            return false;
        }
        let mut ordered: Vec<DockId> = self
            .sections_in(zone)
            .iter()
            .map(|section| section.id)
            .filter(|entry| *entry != id)
            .collect();
        let Some(index) = ordered.iter().position(|entry| *entry == sibling) else {
            return false;
        };
        ordered.insert(index, id);
        for (order, entry) in ordered.iter().enumerate() {
            if let Some(section) = self.section_mut(*entry) {
                section.zone = zone;
                section.order = order as u16;
            }
        }
        true
    }

    pub fn pinned_in(&self, zone: DockZone) -> Vec<&DockSection> {
        self.sections_in(zone)
            .into_iter()
            .filter(|entry| entry.pinned)
            .collect()
    }

    pub fn open(path: &Path) -> Result<Self, String> {
        match fs::read_to_string(path) {
            Ok(raw) => {
                let stored: Self = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
                let mut clean = Self {
                    sections: Vec::new(),
                    layout_version: 2,
                };
                let mut seen = HashSet::new();
                let legacy = stored.layout_version < 2;
                for mut section in stored.sections {
                    if legacy && matches!(section.id, DockId::Toolbox | DockId::Symbols) {
                        section.pinned = true;
                    }
                    if matches!(
                        section.id,
                        DockId::Output
                            | DockId::ErrorList
                            | DockId::Debug
                            | DockId::TestResults
                            | DockId::FindResults
                    ) {
                        // Bottom tabs have no fold or pin controls. Their
                        // display is governed by the selected bottom tab.
                        section.pinned = true;
                        section.collapsed = false;
                    }
                    if seen.insert(section.id) {
                        clean.sections.push(section);
                    }
                }
                for id in DockId::ALL {
                    if !seen.contains(&id) {
                        let defaults = Self::default();
                        if let Some(section) = defaults.section(id) {
                            clean.register(section.clone());
                        }
                    }
                }
                Ok(clean)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error.to_string()),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        crate::storage::atomic_write(path, &bytes).map_err(|e| e.to_string())
    }
}

/// The drag-preview layout is independent of any particular OS widget
/// coordinate system. Native frontends draw these three highlighted targets
/// above their content area while a heading is being dragged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DockRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl DockRect {
    pub fn contains(self, x: f64, y: f64) -> bool {
        x.is_finite()
            && y.is_finite()
            && x >= self.x
            && y >= self.y
            && x < self.x + self.width
            && y < self.y + self.height
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DockTargets {
    pub left: DockRect,
    pub right: DockRect,
    pub bottom: DockRect,
}

impl DockTargets {
    pub fn for_viewport(width: f64, height: f64) -> Option<Self> {
        if !width.is_finite() || !height.is_finite() || width < 360.0 || height < 240.0 {
            return None;
        }
        let target_width = (width * 0.22).clamp(80.0, 240.0);
        let target_height = (height * 0.20).clamp(55.0, 140.0);
        Some(Self {
            left: DockRect {
                x: 12.0,
                y: (height - target_height) / 2.0,
                width: target_width,
                height: target_height,
            },
            right: DockRect {
                x: width - target_width - 12.0,
                y: (height - target_height) / 2.0,
                width: target_width,
                height: target_height,
            },
            bottom: DockRect {
                x: (width - target_width) / 2.0,
                y: height - target_height - 12.0,
                width: target_width,
                height: target_height,
            },
        })
    }

    pub fn hit_test(self, x: f64, y: f64) -> Option<DockZone> {
        if self.left.contains(x, y) {
            Some(DockZone::Left)
        } else if self.right.contains(x, y) {
            Some(DockZone::Right)
        } else if self.bottom.contains(x, y) {
            Some(DockZone::Bottom)
        } else {
            None
        }
    }
}

/// Ephemeral interaction state. An unpinned section expands temporarily
/// from its tab rail and dismisses when focus moves elsewhere. The open
/// state is never saved alongside dock locations or preferred dimensions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DockInteraction {
    pub active_popup: Option<DockId>,
    pub dragging: Option<DockId>,
    pub target: Option<DockZone>,
}

impl DockInteraction {
    pub fn open(&mut self, layout: &DockLayout, id: DockId) {
        self.active_popup = layout.section(id).filter(|s| !s.pinned).map(|_| id);
    }

    pub fn blur(&mut self) {
        self.active_popup = None;
    }

    pub fn begin_drag(&mut self, id: DockId) {
        self.dragging = Some(id);
        self.target = None;
    }

    pub fn hover(&mut self, target: DockZone) {
        if self.dragging.is_some() {
            self.target = Some(target);
        }
    }

    pub fn drop(&mut self, layout: &mut DockLayout) -> bool {
        let changed = match (self.dragging, self.target) {
            (Some(id), Some(zone)) => layout.move_to(id, zone),
            _ => false,
        };
        self.dragging = None;
        self.target = None;
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_section_has_an_explicit_default_zone_and_unique_id() {
        let layout = DockLayout::default();
        assert_eq!(layout.sections.len(), DockId::ALL.len());
        assert_eq!(layout.sections_in(DockZone::Right).len(), 2);
        assert_eq!(layout.sections_in(DockZone::Bottom).len(), 5);
        assert_eq!(layout.pinned_in(DockZone::Right).len(), 2);
    }

    #[test]
    fn registrations_and_cross_zone_moves_are_stable() {
        let mut layout = DockLayout::default();
        layout.register(DockSection::new(DockId::Toolbox, DockZone::Right));
        assert_eq!(layout.sections.len(), DockId::ALL.len());
        assert!(layout.move_to(DockId::Toolbox, DockZone::Bottom));
        assert_eq!(
            layout.section(DockId::Toolbox).unwrap().zone,
            DockZone::Bottom
        );
        assert!(!layout.move_to(DockId::Toolbox, DockZone::Bottom));
        assert_eq!(
            layout.sections_in(DockZone::Bottom).last().unwrap().id,
            DockId::Toolbox
        );
    }

    #[test]
    fn auto_hide_only_applies_to_unpinned_sections_and_drops_are_validated() {
        let mut layout = DockLayout::default();
        let mut gesture = DockInteraction::default();
        gesture.open(&layout, DockId::Toolbox);
        assert_eq!(gesture.active_popup, Some(DockId::Toolbox));
        gesture.blur();
        assert_eq!(gesture.active_popup, None);
        layout.pin(DockId::Toolbox, true);
        gesture.open(&layout, DockId::Toolbox);
        assert_eq!(gesture.active_popup, None);
        gesture.begin_drag(DockId::Toolbox);
        gesture.hover(DockZone::Right);
        assert!(gesture.drop(&mut layout));
        assert_eq!(
            layout.section(DockId::Toolbox).unwrap().zone,
            DockZone::Right
        );
        assert_eq!(gesture.dragging, None);
    }

    #[test]
    fn drop_targets_are_distinct_and_the_editor_centre_is_not_a_dock() {
        let targets = DockTargets::for_viewport(1480.0, 820.0).unwrap();
        assert_eq!(
            targets.hit_test(targets.left.x + 3.0, targets.left.y + 3.0),
            Some(DockZone::Left)
        );
        assert_eq!(
            targets.hit_test(targets.right.x + 3.0, targets.right.y + 3.0),
            Some(DockZone::Right)
        );
        assert_eq!(
            targets.hit_test(targets.bottom.x + 3.0, targets.bottom.y + 3.0),
            Some(DockZone::Bottom)
        );
        assert_eq!(targets.hit_test(740.0, 380.0), None);
        assert_eq!(targets.hit_test(f64::NAN, 0.0), None);
        assert!(DockTargets::for_viewport(100.0, 100.0).is_none());
    }

    #[test]
    fn auto_hide_visibility_respects_pin_and_active_focus() {
        let mut layout = DockLayout::default();
        assert!(layout.visible(DockId::Toolbox, None));
        assert!(layout.visible(DockId::Symbols, None));
        assert!(layout.visible(DockId::Solution, None));
        assert!(layout.visible(DockId::Output, None));
        assert!(!layout.visible(DockId::Search, None));
        assert!(layout.visible(DockId::Search, Some(DockId::Search)));
        assert!(!layout.visible(DockId::Search, Some(DockId::Output)));
        layout.pin(DockId::Search, true);
        assert!(layout.visible(DockId::Search, None));
    }

    #[test]
    fn dragging_order_within_zone_preserves_unique_sections() {
        let mut layout = DockLayout::default();
        assert!(layout.reorder_before(DockId::Properties, DockId::Solution));
        assert_eq!(
            layout.sections_in(DockZone::Right)[0].id,
            DockId::Properties
        );
        assert!(!layout.reorder_before(DockId::Properties, DockId::Properties));
        assert!(layout.reorder_before(DockId::Output, DockId::Solution));
        assert_eq!(
            layout.section(DockId::Output).unwrap().zone,
            DockZone::Right
        );
        assert_eq!(
            layout.sections_in(DockZone::Right)[0].id,
            DockId::Properties
        );
        assert_eq!(layout.sections_in(DockZone::Right)[1].id, DockId::Output);
    }

    #[test]
    fn all_twelve_tools_move_to_each_native_zone_and_restore() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("movable-docks.json");
        for id in DockId::ALL {
            for zone in DockZone::ALL {
                let mut layout = DockLayout::default();
                assert_eq!(layout.section(id).unwrap().zone, id.default_zone());
                if zone != id.default_zone() {
                    assert!(layout.move_to(id, zone));
                }
                layout.pin(id, true);
                layout.save(&file).unwrap();
                let loaded = DockLayout::open(&file).unwrap();
                assert_eq!(loaded.sections.len(), DockId::ALL.len());
                assert_eq!(loaded.section(id).unwrap().zone, zone);
                assert!(loaded.visible(id, None));
                assert_eq!(
                    loaded
                        .sections
                        .iter()
                        .filter(|section| section.id == id)
                        .count(),
                    1,
                );
            }
        }
    }

    #[test]
    fn restoring_default_layout_removes_cross_zone_and_custom_pin_state() {
        let mut layout = DockLayout::default();
        for id in DockId::ALL {
            let zone = match id.default_zone() {
                DockZone::Left => DockZone::Right,
                DockZone::Right => DockZone::Bottom,
                DockZone::Bottom => DockZone::Left,
            };
            assert!(layout.move_to(id, zone));
            layout.pin(id, !layout.section(id).unwrap().pinned);
            layout.collapse(id, true);
        }
        layout = DockLayout::default();
        for id in DockId::ALL {
            assert_eq!(layout.section(id).unwrap().zone, id.default_zone());
            assert!(!layout.section(id).unwrap().collapsed);
        }
        assert!(layout.section(DockId::Solution).unwrap().pinned);
        assert!(layout.section(DockId::Toolbox).unwrap().pinned);
        assert!(layout.section(DockId::Symbols).unwrap().pinned);
    }

    #[test]
    fn old_saved_docks_gain_pinned_side_defaults_and_a_single_bottom_strip() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("old-layout.json");
        let mut layout = DockLayout::default();
        layout.layout_version = 0;
        layout.pin(DockId::Toolbox, false);
        layout.pin(DockId::Symbols, false);
        layout.pin(DockId::Output, false);
        layout.collapse(DockId::Output, true);
        layout.save(&path).unwrap();
        let restored = DockLayout::open(&path).unwrap();
        assert_eq!(restored.layout_version, 2);
        assert!(restored.section(DockId::Toolbox).unwrap().pinned);
        assert!(restored.section(DockId::Symbols).unwrap().pinned);
        assert!(restored.section(DockId::Output).unwrap().pinned);
        assert!(!restored.section(DockId::Output).unwrap().collapsed);
    }

    #[test]
    fn pin_and_collapse_survive_roundtrip() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("dock-layout.json");
        let mut layout = DockLayout::default();
        layout.pin(DockId::Search, true);
        layout.collapse(DockId::Search, true);
        layout.move_to(DockId::Search, DockZone::Bottom);
        layout.save(&path).unwrap();
        let restored = DockLayout::open(&path).unwrap();
        assert_eq!(layout, restored);
        assert!(restored.section(DockId::Search).unwrap().collapsed);
    }
}
