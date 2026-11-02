//! Stable platform-independent Visual Studio-style shell specification.
//! The native menus are built from this table, so tests verify the actual
//! labels and command indices used by the WinUI3/AppKit/GTK presenters.

pub const MENUS: &[(&str, &[&str])] = &[
    (
        "File",
        &[
            "New Project…",
            "Open Project or Solution…",
            "Open Local Folder…",
            "Open File…",
            "Save",
            "Close Workspace",
            "Exit",
        ],
    ),
    ("Edit", &["Undo", "Redo", "Cut", "Copy", "Paste", "Find", "Format Document"]),
    (
        "View",
        &[
            "Empty Workspace",
            "Code Editor",
            "UI Designer",
            "Solution Explorer",
            "Toolbox",
            "Output",
            "Error List",
        ],
    ),
    ("Git", &["Clone Repository…", "Git Changes"]),
    ("Project", &["Add New UI Layout", "Project Properties"]),
    (
        "Build",
        &[
            "Build Solution",
            "Rebuild Solution",
            "Clean Solution",
            "Bundle / Publish Binaries",
        ],
    ),
    (
        "Debug",
        &[
            "Start Debugging (coming soon)",
            "Start Without Debugging",
            "Stop Debugging (coming soon)",
        ],
    ),
    ("Test", &["Test Explorer", "Run Tests (coming soon)"]),
    ("Analyze", &["Check Solution"]),
    ("Tools", &["Options"]),
    ("Extensions", &["Manage Extensions"]),
    ("Window", &["Reset Window Layout"]),
    ("Help", &["About Quark"]),
];

/// Comfortable native tool-rail labels with a separate selection indicator.
pub const SIDE_RAIL_WIDTH: f64 = 42.0;
pub const SIDE_TAB_HEIGHTS: [f64; 3] = [118.0, 106.0, 86.0];
pub const DESIGN_SIDE_TABS: [&str; 3] = ["TOOLBOX", "SERVER", "DATA"];
pub const CODE_SIDE_TABS: [&str; 2] = ["SYMBOLS", "SEARCH"];

/// The right-hand document controls occupy 122 logical pixels only while
/// the tabs genuinely overflow the available centre width.
pub const DOCUMENT_OVERFLOW_WIDTH: f64 = 122.0;

/// Match the actual tab sizes used by the native shell.
pub fn document_tab_width(filename: &str) -> f64 {
    (filename.chars().count() as f64 * 7.0 + 20.0).clamp(70.0, 144.0)
}

/// Determine how many file tabs fit at the current measured editor width.
/// Unconstrained widths (before initial layout) get a conservative fallback.
pub fn document_visible_capacity(width: f64, filenames: &[String]) -> usize {
    if filenames.is_empty() {
        return 0;
    }
    let width = if width.is_finite() && width >= 80.0 {
        width
    } else {
        620.0
    };
    let total = filenames
        .iter()
        .map(|name| document_tab_width(name))
        .sum::<f64>()
        + (filenames.len().saturating_sub(1) as f64 * 3.0);
    if total <= width {
        return filenames.len();
    }
    let budget = (width - DOCUMENT_OVERFLOW_WIDTH - 8.0).max(70.0);
    let mut used = 0.0;
    let mut count = 0;
    for name in filenames {
        let next = document_tab_width(name) + if count > 0 { 3.0 } else { 0.0 };
        if count > 0 && used + next > budget {
            break;
        }
        used += next;
        count += 1;
    }
    count.max(1)
}

/// The document switcher and scroll controls are hidden until needed.
pub fn document_overflow(open_count: usize, visible_capacity: usize) -> bool {
    visible_capacity > 0 && open_count > visible_capacity
}

/// Quark's palette: Hydrogen electron cyan, orbital blue, quark violet.
pub const HYDROGEN_CYAN: (u8, u8, u8) = (50, 212, 226);
pub const ORBIT_BLUE: (u8, u8, u8) = (51, 103, 214);
pub const QUARK_VIOLET: (u8, u8, u8) = (158, 113, 235);
pub const QUIET_SURFACE: (u8, u8, u8) = (28, 34, 48);

pub const TOOL_WINDOWS: &[&str] = &[
    "Toolbox",
    "Solution Explorer",
    "Properties",
    "Output",
    "Error List",
    "Debug",
    "Test Results",
    "Find Results",
];

pub const LAUNCH_ACTIONS: &[&str] = &[
    "Clone a repository",
    "Open a project or solution",
    "Open a local folder",
    "Create a new project",
    "Continue without code",
];

pub fn position(menu: &str, action: &str) -> Option<(usize, usize)> {
    MENUS.iter().enumerate().find_map(|(index, (name, items))| {
        if *name != menu {
            return None;
        }
        items
            .iter()
            .position(|item| *item == action)
            .map(|item| (index, item))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_overflow_controls_are_hidden_until_tabs_exceed_capacity() {
        assert!(!document_overflow(0, 3));
        assert!(!document_overflow(1, 3));
        assert!(!document_overflow(3, 3));
        assert!(document_overflow(4, 3));
        assert!(!document_overflow(4, 0));
    }

    #[test]
    fn document_tabs_use_measured_width_and_hide_overflow_when_everything_fits() {
        let names = vec![
            "Program.hy".into(),
            "Studio.hyui".into(),
            "README.md".into(),
        ];
        assert_eq!(document_visible_capacity(1100.0, &names), 3);
        assert!(document_visible_capacity(235.0, &names) < 3);
        assert_eq!(document_visible_capacity(90.0, &names), 1);
        assert_eq!(document_visible_capacity(700.0, &[]), 0);
        assert_eq!(document_tab_width("a.hy"), 70.0);
        assert_eq!(document_tab_width(&"x".repeat(50)), 144.0);
        assert_eq!(document_visible_capacity(f64::NAN, &names), 3);
    }

    #[test]
    fn side_rail_presets_are_compact_and_context_sensitive() {
        assert_eq!(SIDE_RAIL_WIDTH, 42.0);
        assert_eq!(SIDE_TAB_HEIGHTS, [118.0, 106.0, 86.0]);
        assert_eq!(DESIGN_SIDE_TABS, ["TOOLBOX", "SERVER", "DATA"]);
        assert_eq!(CODE_SIDE_TABS, ["SYMBOLS", "SEARCH"]);
        assert!(!CODE_SIDE_TABS.contains(&"TOOLBOX"));
        assert!(!DESIGN_SIDE_TABS.contains(&"SYMBOLS"));
    }

    #[test]
    fn hydrogen_quark_accents_are_distinct_and_start_tab_is_not_an_ide_document() {
        assert_ne!(HYDROGEN_CYAN, QUARK_VIOLET);
        assert_ne!(HYDROGEN_CYAN, ORBIT_BLUE);
        assert_ne!(ORBIT_BLUE, QUARK_VIOLET);
        assert_eq!(QUIET_SURFACE, (28, 34, 48));
        assert_eq!(position("View", "Empty Workspace"), Some((2, 0)));
        assert!(position("View", "Start Page").is_none());
    }

    #[test]
    fn professional_menus_include_debug_build_test_analyze_and_extensions() {
        let names: Vec<&str> = MENUS.iter().map(|(name, _)| *name).collect();
        assert_eq!(
            names,
            [
                "File",
                "Edit",
                "View",
                "Git",
                "Project",
                "Build",
                "Debug",
                "Test",
                "Analyze",
                "Tools",
                "Extensions",
                "Window",
                "Help",
            ]
        );
        assert_eq!(position("Build", "Build Solution"), Some((5, 0)));
        assert_eq!(position("Build", "Bundle / Publish Binaries"), Some((5, 3)));
        assert_eq!(position("Debug", "Start Without Debugging"), Some((6, 1)));
    }

    #[test]
    fn start_debugging_is_not_misrepresented_as_implemented() {
        assert_eq!(
            position("Debug", "Start Debugging (coming soon)"),
            Some((6, 0)),
        );
        assert!(position("Debug", "Start Debugging").is_none());
        assert_eq!(
            position("Debug", "Stop Debugging (coming soon)"),
            Some((6, 2)),
        );
    }

    #[test]
    fn full_ide_contains_visual_studio_tool_windows() {
        for required in [
            "Toolbox",
            "Solution Explorer",
            "Properties",
            "Output",
            "Error List",
            "Debug",
        ] {
            assert!(TOOL_WINDOWS.contains(&required));
        }
        assert!(!TOOL_WINDOWS.contains(&"Activity Bar"));
    }

    #[test]
    fn launcher_has_project_management_and_no_heavy_editor_actions() {
        assert_eq!(LAUNCH_ACTIONS.len(), 5);
        assert!(LAUNCH_ACTIONS.contains(&"Clone a repository"));
        assert!(LAUNCH_ACTIONS.contains(&"Create a new project"));
        assert!(LAUNCH_ACTIONS.contains(&"Continue without code"));
        assert!(!LAUNCH_ACTIONS.contains(&"Start Debugging"));
    }
}
