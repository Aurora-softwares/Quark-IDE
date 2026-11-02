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
            Self::Toolbox | Self::Symbols | Self::Search | Self::Server | Self::Data => DockZone::Left,
            Self::Solution | Self::Properties => DockZone::Right,
            Self::Output | Self::ErrorList | Self::Debug
            | Self::TestResults | Self::FindResults => DockZone::Bottom,
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
}

impl Default for DockLayout {
    fn default() -> Self {
        let mut layout = Self { sections: Vec::new() };
        for id in DockId::ALL {
            // Default unpinned tool windows auto-hide after losing focus.
            // The user can pin any section to reserve its dock space.
            let pinned = matches!(id, DockId::Solution | DockId::Properties);
            layout.register(DockSection::new(id, id.default_zone()).pinned(pinned));
        }
        layout
    }
}

impl DockLayout {
    pub fn register(&mut self, mut section: DockSection) {
        if self.sections.iter().any(|existing| existing.id == section.id) {
            return;
        }
        section.order = self.sections.iter()
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
        let order = self.sections.iter()
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
        let mut sections: Vec<_> = self.sections.iter()
            .filter(|section| section.zone == zone)
            .collect();
        sections.sort_by_key(|section| section.order);
        sections
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
                let mut clean = Self { sections: Vec::new() };
                let mut seen = HashSet::new();
                for section in stored.sections {
                    if seen.insert(section.id) {
                        clean.sections.push(section);
                    }
                }
                for id in DockId::ALL {
                    if !seen.contains(&id) {
                        clean.register(DockSection::new(id, id.default_zone()));
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

/// Ephemeral interaction state. Unpinned sections overlay the editor and
/// dismiss when focus moves elsewhere; never persist the open popup.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DockInteraction {
    pub active_popup: Option<DockId>,
    pub dragging: Option<DockId>,
    pub target: Option<DockZone>,
}

impl DockInteraction {
    pub fn open(&mut self, layout: &DockLayout, id: DockId) {
        self.active_popup = layout.section(id)
            .filter(|s| !s.pinned)
            .map(|_| id);
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
        assert_eq!(layout.section(DockId::Toolbox).unwrap().zone, DockZone::Bottom);
        assert!(!layout.move_to(DockId::Toolbox, DockZone::Bottom));
        assert_eq!(layout.sections_in(DockZone::Bottom).last().unwrap().id, DockId::Toolbox);
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
        assert_eq!(layout.section(DockId::Toolbox).unwrap().zone, DockZone::Right);
        assert_eq!(gesture.dragging, None);
    }

    #[test]
    fn pin_and_collapse_survive_roundtrip() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("dock-layout.json");
        let mut layout = DockLayout::default();
        layout.pin(DockId::Output, true);
        layout.collapse(DockId::Output, true);
        layout.move_to(DockId::Output, DockZone::Left);
        layout.save(&path).unwrap();
        let restored = DockLayout::open(&path).unwrap();
        assert_eq!(layout, restored);
        assert!(restored.section(DockId::Output).unwrap().collapsed);
    }
}
