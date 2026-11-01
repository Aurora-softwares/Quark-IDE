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
    ("Edit", &["Undo", "Redo", "Cut", "Copy", "Paste", "Find"]),
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
    ("Analyze", &["Run Code Analysis (coming soon)"]),
    ("Tools", &["Options"]),
    ("Extensions", &["Manage Extensions"]),
    ("Window", &["Reset Window Layout"]),
    ("Help", &["About Quark"]),
];

/// Compact, native-button side rail. naui 0.8 does not expose rotated
/// text for buttons, so captions use vertical writing while retaining
/// keyboard focus and native platform styling.
pub const SIDE_RAIL_WIDTH: f64 = 29.0;
pub const SIDE_TABS: &[(&str, &str, f64)] = &[
    ("Toolbox", "T\nO\nO\nL\nB\nO\nX", 122.0),
    ("Server Explorer", "S\nE\nR\nV\nE\nR", 110.0),
    ("Data Sources", "D\nA\nT\nA", 80.0),
];

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
    fn professional_side_rail_is_compact_and_uses_native_focusable_tabs() {
        assert_eq!(SIDE_RAIL_WIDTH, 29.0);
        assert_eq!(SIDE_TABS.len(), 3);
        assert_eq!(SIDE_TABS[0].0, "Toolbox");
        for &(name, caption, height) in SIDE_TABS {
            assert!(!name.is_empty());
            assert!((80.0..=122.0).contains(&height));
            assert!(caption.contains('\n'));
            assert!(caption.lines().all(|line| line.chars().count() == 1));
        }
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
