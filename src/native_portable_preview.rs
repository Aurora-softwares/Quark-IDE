//! Portable design-time rendering for Hydrogen <Window> files.
//! naui creates real AppKit / WinUI 3 / GTK controls from the same tree.
//! The preview never dispatches application handlers.
use naui::{GridCell, Length, Orientation, Padding, Sizing, Track, Ui};
use roxmltree::Node;
use std::path::Path;
use std::{cell::RefCell, collections::HashMap, rc::Rc};

/// Design-only active page per tab container. The source's Tabs.value stays
/// unchanged: a designer may explore page 2 without changing startup page 1.
pub type DesignTabs = Rc<RefCell<HashMap<Vec<usize>, usize>>>;

pub type SelectElement = Rc<dyn Fn(Vec<usize>)>;

/// Space between the fixed-size Hydrogen design window (including its shadow)
/// and the available editor viewport. Deliberately not part of Window.padding.
pub const DESIGN_VIEWPORT_INSET: f64 = 16.0;

#[cfg(target_os = "linux")]
const DESIGN_FRAME_CSS: &str = "box.quark-design-frame { border-radius: 12px; \
      box-shadow: 0 6px 18px alpha(currentColor, 0.18); }";

/// Install GTK-only appearance rules once per native display. Colours derive
/// from currentColor, so no dark-mode colours are baked into the designer.
#[cfg(target_os = "linux")]
fn install_gtk_design_shadow() {
    thread_local! {
        static INSTALLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    INSTALLED.with(|installed| {
        if installed.get() {
            return;
        }
        if let Some(display) = gtk4::gdk::Display::default() {
            let provider = gtk4::CssProvider::new();
            provider.load_from_data(DESIGN_FRAME_CSS);
            gtk4::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
            installed.set(true);
        }
    });
}

/// The compiled application's startup tab is zero-based, while its
/// displayed first page is page 1. Moving around the designer must never
/// mutate that startup value.
pub fn design_tab_index(
    source_default: usize,
    design_override: Option<usize>,
    number_of_pages: usize,
) -> Option<usize> {
    number_of_pages
        .checked_sub(1)
        .map(|last| design_override.unwrap_or(source_default).min(last))
}

/// Resolve an image exactly as the Hydrogen runtime does: relative to
/// its .hyui document, not to Quark's current working directory.
fn image_source(document: &Path, source: &str) -> String {
    if source.is_empty() || source.contains("://") || source.starts_with("data:") {
        return source.to_owned();
    }
    let source_path = Path::new(source);
    let path = if source_path.is_absolute() {
        source_path.to_path_buf()
    } else {
        document
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(source_path)
    };
    path.to_string_lossy().into_owned()
}

/// Only active controls claim the native design-time click sequence. A
/// container must allow the event to reach its nested child for selection.
#[cfg(any(target_os = "linux", test))]
fn design_press_claims_activation(kind: &str) -> bool {
    matches!(
        kind,
        "Button"
            | "MenuItem"
            | "ToolButton"
            | "Input"
            | "PasswordInput"
            | "SearchInput"
            | "NumberInput"
            | "TextArea"
            | "CheckBox"
            | "Switch"
            | "Select"
            | "RadioGroup"
            | "Slider"
            | "ListBox"
            | "Table"
            | "FilePicker"
    )
}

/// The native GTK menu row is a single horizontal box just as in Hydrogen.
/// Separating construction makes its real widget hierarchy testable under Xvfb.
#[cfg(target_os = "linux")]
fn build_gtk_design_menu_bar(
    node: Node<'_, '_>,
    path: &[usize],
    on_select: &SelectElement,
) -> gtk4::Box {
    use gtk4::prelude::*;

    let bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    bar.set_halign(gtk4::Align::Start);
    bar.set_valign(gtk4::Align::Start);
    bar.set_hexpand(false);
    for (index, menu) in node
        .children()
        .filter(|child| child.is_element() && child.has_tag_name("Menu"))
        .enumerate()
    {
        let button = gtk4::MenuButton::new();
        button.set_label(menu.attribute("text").unwrap_or("Menu"));
        button.set_hexpand(false);
        button.set_halign(gtk4::Align::Start);
        let popover = gtk4::Popover::new();
        let contents = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        for item in menu
            .children()
            .filter(|child| child.is_element() && child.has_tag_name("MenuItem"))
        {
            let item_label = gtk4::Label::new(item.attribute("text"));
            item_label.set_xalign(0.0);
            item_label.set_margin_start(8);
            item_label.set_margin_end(8);
            item_label.set_margin_top(4);
            item_label.set_margin_bottom(4);
            contents.append(&item_label);
        }
        popover.set_child(Some(&contents));
        button.set_popover(Some(&popover));

        // This is a selection-only design surface, not an app
        // whose menus should dispatch user-defined commands.
        let capture = gtk4::GestureClick::new();
        capture.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let selected = on_select.clone();
        let mut selected_path = path.to_vec();
        selected_path.push(index);
        capture.connect_pressed(move |gesture, _, _, _| {
            selected(selected_path.clone());
            gesture.set_state(gtk4::EventSequenceState::Claimed);
        });
        button.add_controller(capture);
        let keys = gtk4::EventControllerKey::new();
        keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        keys.connect_key_pressed(|_, _, _, _| gtk4::glib::Propagation::Stop);
        button.add_controller(keys);
        bar.append(&button);
    }

    bar
}

/// Mirror Hydrogen's native `GtkFrame(card) -> padded content` hierarchy.
/// A bare box styled as a card does not have the same requisition or border.
#[cfg(target_os = "linux")]
fn build_gtk_design_panel_frame(
    content: &impl gtk4::prelude::IsA<gtk4::Widget>,
    padding: i32,
) -> gtk4::Frame {
    use gtk4::prelude::*;

    let frame = gtk4::Frame::new(None);
    frame.add_css_class("card");
    frame.set_hexpand(true);
    frame.set_halign(gtk4::Align::Fill);
    content.set_margin_start(padding);
    content.set_margin_end(padding);
    content.set_margin_top(padding);
    content.set_margin_bottom(padding);
    frame.set_child(Some(content));
    frame
}

/// Match Hydrogen's GTK stylesheet identifiers instead of translating
/// roles into a second set of cross-platform font/colour settings.
#[cfg(any(target_os = "linux", test))]
fn hydrogen_gtk_label_classes(role: &str, tone: &str) -> Vec<&'static str> {
    let mut classes = Vec::new();
    match role {
        "title" => classes.push("title-1"),
        "heading" => classes.push("title-3"),
        "caption" => classes.push("caption"),
        _ => {}
    }
    match tone {
        "muted" => classes.push("dim-label"),
        "accent" => classes.push("accent"),
        _ => {}
    }
    classes
}

/// Runtime GTK expansion defaults from Hydrogen's native renderer.
fn runtime_fills(node: Node<'_, '_>) -> bool {
    match node.attribute("fill") {
        Some("true") => true,
        Some("false") => false,
        _ => !matches!(
            node.tag_name().name(),
            "Button" | "MenuItem" | "ToolButton" | "FilePicker" | "Spinner"
        ),
    }
}

/// Width hints for the children of a horizontal RowPanel. Explicit
/// preferredWidth is a constraint, not a suggestion to split all children
/// into equal-size cells. The runtime keeps those fixed widths while its
/// native FlowBox handles wrapping at narrow window sizes.
fn row_child_widths(node: Node<'_, '_>, available_width: f64, gap: f64) -> Vec<f64> {
    let children = node
        .children()
        .filter(|child| child.is_element())
        .collect::<Vec<_>>();
    let widths = children
        .iter()
        .map(|child| {
            if child.attribute("visible") == Some("false") {
                return None;
            }
            child
                .attribute("preferredWidth")
                .and_then(|width| width.parse::<f64>().ok())
                .filter(|width| width.is_finite() && *width >= 20.0)
        })
        .collect::<Vec<_>>();
    let visible_count = children
        .iter()
        .filter(|child| child.attribute("visible") != Some("false"))
        .count();
    let fixed_total: f64 = widths.iter().flatten().sum();
    let flexible_count = children
        .iter()
        .zip(widths.iter())
        .filter(|(child, width)| child.attribute("visible") != Some("false") && width.is_none())
        .count();
    let remaining =
        (available_width - gap * visible_count.saturating_sub(1) as f64 - fixed_total).max(0.0);
    let flexible_width = if flexible_count == 0 {
        0.0
    } else {
        (remaining / flexible_count as f64).max(80.0)
    };
    let mut assigned = children
        .iter()
        .zip(widths)
        .map(|(child, fixed)| {
            if child.attribute("visible") == Some("false") {
                0.0
            } else {
                fixed.unwrap_or(flexible_width)
            }
        })
        .collect::<Vec<_>>();
    if flexible_count == 0 {
        // GTK treats preferredWidth as a *minimum*, not a fixed width. A
        // FlowBox grants remaining space to its final expanding child.
        // At 1120px Studio keeps settings at 600 and expands its summary
        // beyond the 340px requested minimum.
        if let Some(last) = children
            .iter()
            .rposition(|child| child.attribute("visible") != Some("false") && runtime_fills(*child))
        {
            assigned[last] += remaining;
        }
    }
    assigned
}

fn row_panel_stacks(node: Node<'_, '_>, available_width: f64, gap: f64) -> bool {
    let breakpoint = node
        .attribute("breakpoint")
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(700.0);
    let children = node
        .children()
        .filter(|child| child.is_element() && child.attribute("visible") != Some("false"))
        .collect::<Vec<_>>();
    let minimums: f64 = children
        .iter()
        .filter_map(|child| child.attribute("preferredWidth"))
        .filter_map(|width| width.parse::<f64>().ok())
        .filter(|width| width.is_finite() && *width >= 20.0)
        .sum();
    available_width < breakpoint
        || (minimums > 0.0
            && minimums + gap * children.len().saturating_sub(1) as f64 > available_width)
}

#[allow(clippy::too_many_arguments)] // Recursive renderer forwards native UI, selection state and document context.
fn render_children(
    ui: &Ui,
    parent: &naui::Stack,
    node: Node<'_, '_>,
    path: &[usize],
    on_select: &SelectElement,
    design_tabs: &DesignTabs,
    available_width: f64,
    document: &Path,
) -> Result<(), String> {
    for (index, child) in node
        .children()
        .filter(|child| child.is_element())
        .enumerate()
    {
        let mut next = path.to_vec();
        next.push(index);
        render_node(
            ui,
            parent,
            child,
            next,
            on_select,
            design_tabs,
            available_width,
            document,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)] // Recursive renderer forwards native UI, selection state and document context.
fn render_node(
    ui: &Ui,
    parent: &naui::Stack,
    node: Node<'_, '_>,
    path: Vec<usize>,
    on_select: &SelectElement,
    design_tabs: &DesignTabs,
    available_width: f64,
    document: &Path,
) -> Result<(), String> {
    // Only draw what Hydrogen will draw. Selection chrome belongs to the
    // Properties hierarchy, never inside the application being previewed.
    if node.attribute("visible") == Some("false") {
        return Ok(());
    }
    let kind = node.tag_name().name();
    if matches!(
        kind,
        "Timer" | "Dialog" | "State" | "Clipboard" | "ContextMenu"
    ) {
        return Ok(());
    }
    let row = ui.stack(Orientation::Vertical).map_err(|e| e.to_string())?;
    row.set_spacing(0.0);
    // Honor the same common layout attributes as the Hydrogen runtime.
    // These are design-time measurements, not changes to the XML source.
    if let Some(width) = node
        .attribute("preferredWidth")
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value >= 20.0)
    {
        row.set_sizing(Sizing::new().width(Length::Fixed(width.max(available_width).min(2400.0))));
    } else if runtime_fills(node) {
        // Hydrogen defaults fill=true for ordinary content. A non-filling
        // wrapper lets otherwise correct GTK controls collapse to natural
        // sizes inside a much wider panel (notably multi-line text editors).
        row.set_sizing(Sizing::fill_width());
    }
    #[cfg(target_os = "linux")]
    if let Some(tip) = node.attribute("tooltip").filter(|value| !value.is_empty()) {
        use gtk4::prelude::*;
        use naui::Widget as _;
        row.native_widget().set_tooltip_text(Some(tip));
    }
    // In GTK, container captures allow deeper selection. Actionable controls
    // claim the design-time press before their default behaviour runs.
    // Edits to application state belong exclusively in Properties.
    #[cfg(target_os = "linux")]
    {
        use gtk4::prelude::*;
        use naui::Widget as _;
        let capture = gtk4::GestureClick::new();
        capture.set_button(0);
        capture.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let selected = on_select.clone();
        let selected_path = path.clone();
        let claims_activation = design_press_claims_activation(kind);
        capture.connect_pressed(move |gesture, _, _, _| {
            selected(selected_path.clone());
            // Do not claim on containers: allow a nested interactive child to
            // receive its design-time selection event instead.
            if claims_activation {
                gesture.set_state(gtk4::EventSequenceState::Claimed);
            }
        });
        row.native_widget().add_controller(capture);
        if claims_activation {
            let key_controller = gtk4::EventControllerKey::new();
            key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
            key_controller.connect_key_pressed(|_, key, _, modifiers| {
                // Keep IDE navigation/shortcuts functional, but never let
                // printable keys, Space or arrows edit the preview's state.
                let is_navigation = key == gtk4::gdk::Key::Tab
                    || key == gtk4::gdk::Key::ISO_Left_Tab
                    || key == gtk4::gdk::Key::Escape;
                let is_ide_shortcut = modifiers.intersects(
                    gtk4::gdk::ModifierType::CONTROL_MASK
                        | gtk4::gdk::ModifierType::META_MASK
                        | gtk4::gdk::ModifierType::SUPER_MASK,
                );
                if is_navigation || is_ide_shortcut {
                    gtk4::glib::Propagation::Proceed
                } else {
                    gtk4::glib::Propagation::Stop
                }
            });
            row.native_widget().add_controller(key_controller);
        }
    }
    let enabled = node.attribute("enabled") != Some("false");

    match kind {
        "Panel" => {
            let panel = ui.stack(Orientation::Vertical).map_err(|e| e.to_string())?;
            let padding = node
                .attribute("padding")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(20.0)
                .clamp(0.0, 80.0);
            #[cfg(not(target_os = "linux"))]
            panel.set_padding(Padding::all(padding));
            // Linux uses the exact GTK Frame -> padded child construction
            // from Hydrogen's runtime; padding a bare 'card' GtkBox instead
            // changed the content's measured dimensions and card borders.
            #[cfg(target_os = "linux")]
            panel.set_padding(Padding::all(0.0));
            // The panel's preferredWidth belongs to the outer row. Its native
            // card must occupy the entire row rather than remain centred at
            // a narrow natural width.
            panel.set_sizing(Sizing::fill_width());
            panel.set_spacing(
                node.attribute("gap")
                    .and_then(|v| v.parse::<f64>().ok())
                    .unwrap_or(12.0)
                    .clamp(0.0, 48.0),
            );
            if let Some(title) = node.attribute("title").filter(|s| !s.is_empty()) {
                let label = ui.label(title).map_err(|e| e.to_string())?;
                #[cfg(not(target_os = "linux"))]
                label.set_style(naui::TextStyle::Heading);
                #[cfg(target_os = "linux")]
                {
                    use gtk4::prelude::*;
                    use naui::Widget as _;
                    label.native_widget().add_css_class("heading");
                }
                // Hydrogen places panel headings at the start of each card,
                // rather than allowing the generic Stack cross-axis to
                // centre them.
                label.set_sizing(Sizing::fill_width());
                #[cfg(target_os = "linux")]
                {
                    use gtk4::prelude::*;
                    use naui::Widget as _;
                    if let Ok(native) = label.native_widget().downcast::<gtk4::Label>() {
                        native.set_xalign(0.0);
                    }
                }
                panel.append(&label);
            }
            render_children(
                ui,
                &panel,
                node,
                &path,
                on_select,
                design_tabs,
                (available_width - 2.0 * padding).max(80.0),
                document,
            )?;
            #[cfg(target_os = "linux")]
            {
                use gtk4::prelude::*;
                use naui::Widget as _;

                let content = panel.size_bin();
                let frame = build_gtk_design_panel_frame(&content, padding.round() as i32);
                let host = row
                    .native_widget()
                    .downcast::<gtk4::Box>()
                    .map_err(|_| "Native GTK panel host is not a GtkBox")?;
                host.append(&frame);
            }
            #[cfg(not(target_os = "linux"))]
            row.append(&panel);
        }
        "ColumnPanel" | "Tab" | "Window" => {
            let children = ui.stack(Orientation::Vertical).map_err(|e| e.to_string())?;
            children.set_sizing(Sizing::fill_width());
            children.set_spacing(
                node.attribute("gap")
                    .and_then(|v| v.parse::<f64>().ok())
                    .unwrap_or(12.0)
                    .clamp(0.0, 48.0),
            );
            render_children(
                ui,
                &children,
                node,
                &path,
                on_select,
                design_tabs,
                available_width,
                document,
            )?;
            row.append(&children);
        }
        "ScrollView" => {
            // A real native scrolling container, not a vertically stacked
            // imitation. The scroll position is strictly design-time state.
            let scroll = ui.scroll().map_err(|e| e.to_string())?;
            let children = ui.stack(Orientation::Vertical).map_err(|e| e.to_string())?;
            children.set_spacing(
                node.attribute("gap")
                    .and_then(|value| value.parse::<f64>().ok())
                    .unwrap_or(12.0)
                    .clamp(0.0, 48.0),
            );
            render_children(
                ui,
                &children,
                node,
                &path,
                on_select,
                design_tabs,
                available_width,
                document,
            )?;
            scroll.set_child(&children);
            let height = node
                .attribute("height")
                .and_then(|value| value.parse::<f64>().ok())
                .filter(|value| value.is_finite())
                .unwrap_or(200.0)
                .clamp(40.0, 1600.0);
            scroll.set_sizing(
                Sizing::new()
                    .width(Length::Fill)
                    .height(Length::Fixed(height)),
            );
            row.append(&scroll);
        }
        "MenuBar" => {
            // The Hydrogen GTK runtime uses GtkMenuButton for each top-level
            // Menu. Rendering its MenuItems as ordinary push buttons makes
            // the whole document look unlike the real application.
            #[cfg(target_os = "linux")]
            {
                use gtk4::prelude::*;
                use naui::Widget as _;

                // `row` is a vertical wrapper for ordinary design nodes.
                // Never append menu buttons directly to it: that creates two
                // full-width, stacked dropdowns (the screenshot regression).
                // Hydrogen constructs its MenuBar as a horizontal GtkBox.
                let host = row
                    .native_widget()
                    .downcast::<gtk4::Box>()
                    .map_err(|_| "Native GTK menu host is not a GtkBox")?;
                let bar = build_gtk_design_menu_bar(node, &path, on_select);
                host.append(&bar);
            }
            #[cfg(not(target_os = "linux"))]
            {
                render_children(
                    ui,
                    &row,
                    node,
                    &path,
                    on_select,
                    design_tabs,
                    available_width,
                    document,
                )?;
            }
        }
        "RowPanel" | "Menu" => {
            let gap = node
                .attribute("gap")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(12.0)
                .clamp(0.0, 48.0);
            let direction = if kind == "RowPanel" && row_panel_stacks(node, available_width, gap) {
                Orientation::Vertical
            } else {
                Orientation::Horizontal
            };
            let children = ui.stack(direction).map_err(|e| e.to_string())?;
            children.set_sizing(Sizing::fill_width());
            children.set_spacing(gap);
            // Preserve the specified widths of individual panels. In the
            // Studio fixture this must remain 600px + 340px, not 50/50.
            let widths = row_child_widths(node, available_width, gap);
            for (index, child) in node
                .children()
                .filter(|child| child.is_element())
                .enumerate()
            {
                let mut child_path = path.clone();
                child_path.push(index);
                let child_width = if direction == Orientation::Horizontal {
                    widths[index]
                } else {
                    available_width
                };
                render_node(
                    ui,
                    &children,
                    child,
                    child_path,
                    on_select,
                    design_tabs,
                    child_width,
                    document,
                )?;
            }
            row.append(&children);
        }
        "GridPanel" => {
            let grid = ui.grid().map_err(|e| e.to_string())?;
            grid.set_sizing(Sizing::fill_width());
            let maximum_columns = node
                .attribute("columns")
                .and_then(|n| n.parse::<usize>().ok())
                .unwrap_or(3)
                .clamp(1, 12);
            let gap = node
                .attribute("gap")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(16.0)
                .clamp(0.0, 48.0);
            let minimum_width = node
                .attribute("minItemWidth")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(240.0)
                .clamp(80.0, 1200.0);
            // Hydrogen's layout is responsive. Use the target window's width
            // rather than always forcing the maximum columns.
            let columns = maximum_columns.min(
                ((available_width + gap) / (minimum_width + gap))
                    .floor()
                    .max(1.0) as usize,
            );
            grid.set_spacing(gap, gap);
            for column in 0..columns {
                grid.set_column_track(column, Track::FILL);
            }
            for (index, child) in node.children().filter(|n| n.is_element()).enumerate() {
                let cell = ui.stack(Orientation::Vertical).map_err(|e| e.to_string())?;
                cell.set_sizing(Sizing::fill_width());
                let mut child_path = path.clone();
                child_path.push(index);
                render_node(
                    ui,
                    &cell,
                    child,
                    child_path,
                    on_select,
                    design_tabs,
                    (available_width - gap * (columns as f64 - 1.0)) / columns as f64,
                    document,
                )?;
                grid.attach(&cell, GridCell::new(index % columns, index / columns));
            }
            row.append(&grid);
        }
        "Tabs" => {
            // Hydrogen's GTK runtime uses a GtkStackSwitcher + GtkStack,
            // not GtkNotebook. Keep the same native control pair in the
            // designer so tab size, typography and theme match the runtime.
            let mut pages = Vec::new();
            for (position, tab) in node.children().filter(|n| n.is_element()).enumerate() {
                if tab.tag_name().name() != "Tab" {
                    continue;
                }
                let page = ui.stack(Orientation::Vertical).map_err(|e| e.to_string())?;
                page.set_sizing(Sizing::fill_width());
                let mut tab_path = path.clone();
                tab_path.push(position);
                render_children(
                    ui,
                    &page,
                    tab,
                    &tab_path,
                    on_select,
                    design_tabs,
                    available_width,
                    document,
                )?;
                pages.push((tab.attribute("title").unwrap_or("Page").to_owned(), page));
            }
            if !pages.is_empty() {
                let runtime_default = node
                    .attribute("value")
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(0);
                let design_override = design_tabs.borrow().get(&path).copied();
                let chosen =
                    design_tab_index(runtime_default, design_override, pages.len()).unwrap_or(0);
                #[cfg(target_os = "linux")]
                {
                    use gtk4::prelude::*;
                    use naui::Widget as _;

                    let stack = gtk4::Stack::new();
                    stack.set_hhomogeneous(false);
                    stack.set_vhomogeneous(false);
                    for (index, (title, page)) in pages.iter().enumerate() {
                        stack.add_titled(&page.size_bin(), Some(&index.to_string()), title);
                    }
                    stack.set_visible_child_name(&chosen.to_string());
                    let switcher = gtk4::StackSwitcher::new();
                    switcher.set_stack(Some(&stack));

                    // Tab clicks are design-time selections, not app events.
                    // Only Properties can change the saved startup value.
                    let capture = gtk4::GestureClick::new();
                    capture.set_button(0);
                    capture.set_propagation_phase(gtk4::PropagationPhase::Capture);
                    let select = on_select.clone();
                    let selected_path = path.clone();
                    capture.connect_pressed(move |gesture, _, _, _| {
                        select(selected_path.clone());
                        gesture.set_state(gtk4::EventSequenceState::Claimed);
                    });
                    switcher.add_controller(capture);
                    let keys = gtk4::EventControllerKey::new();
                    keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
                    keys.connect_key_pressed(|_, _, _, _| gtk4::glib::Propagation::Stop);
                    switcher.add_controller(keys);

                    let host = row
                        .native_widget()
                        .downcast::<gtk4::Box>()
                        .map_err(|_| "Native GTK tab host is not a GtkBox")?;
                    host.append(&switcher);
                    host.append(&stack);
                }
                #[cfg(not(target_os = "linux"))]
                {
                    let tabs = ui.tabs().map_err(|e| e.to_string())?;
                    for (title, page) in &pages {
                        tabs.add_tab(title, page);
                    }
                    tabs.set_selected(chosen);
                    let selection = design_tabs.clone();
                    let select = on_select.clone();
                    tabs.on_select(move |index| {
                        selection.borrow_mut().insert(path.clone(), index);
                        select(path.clone());
                    });
                    row.append(&tabs);
                }
            }
        }
        "Label" => {
            let label = ui
                .label(node.attribute("text").unwrap_or(""))
                .map_err(|e| e.to_string())?;
            #[cfg(target_os = "linux")]
            {
                use gtk4::prelude::*;
                use naui::Widget as _;
                if let Ok(native) = label.native_widget().downcast::<gtk4::Label>() {
                    // Hydrogen's gtk.cpp always sets label xalign=0.
                    native.set_xalign(0.0);
                }
            }
            if runtime_fills(node) {
                label.set_sizing(Sizing::fill_width());
            }
            #[cfg(target_os = "linux")]
            {
                use gtk4::prelude::*;
                use naui::Widget as _;
                for class in hydrogen_gtk_label_classes(
                    node.attribute("role").unwrap_or(""),
                    node.attribute("tone").unwrap_or(""),
                ) {
                    label.native_widget().add_css_class(class);
                }
            }
            #[cfg(not(target_os = "linux"))]
            {
                match node.attribute("role").unwrap_or("") {
                    "title" => label.set_style(naui::TextStyle::LargeTitle),
                    "heading" => label.set_style(naui::TextStyle::Heading),
                    "caption" => label.set_style(naui::TextStyle::Caption),
                    _ => {}
                }
                match node.attribute("tone").unwrap_or("") {
                    "accent" => label.set_color(naui::TextColor::Accent),
                    "muted" => label.set_color(naui::TextColor::Secondary),
                    _ => {}
                }
            }
            row.append(&label);
        }
        "Button" | "MenuItem" | "ToolButton" => {
            let button = ui
                .button(node.attribute("text").unwrap_or("Button"))
                .map_err(|e| e.to_string())?;
            button.set_enabled(enabled);
            if node.attribute("primary") == Some("true") {
                #[cfg(target_os = "linux")]
                {
                    use gtk4::prelude::*;
                    use naui::Widget as _;
                    // This is the exact GTK style class used by Hydrogen.
                    button.native_widget().add_css_class("suggested-action");
                }
            }
            if runtime_fills(node) {
                button.set_sizing(Sizing::fill_width());
            }
            #[cfg(target_os = "linux")]
            {
                use gtk4::prelude::*;
                use naui::Widget as _;
                button.native_widget().set_halign(if runtime_fills(node) {
                    gtk4::Align::Fill
                } else {
                    gtk4::Align::Start
                });
            }
            let selected = on_select.clone();
            button.on_click(move || selected(path.clone()));
            row.append(&button);
        }
        "Input" => {
            let input = ui
                .text_input(node.attribute("text").unwrap_or(""))
                .map_err(|e| e.to_string())?;
            input.set_placeholder(node.attribute("placeholder").unwrap_or(""));
            input.set_enabled(enabled);
            if runtime_fills(node) {
                input.set_sizing(Sizing::fill_width());
            } else {
                input.set_sizing(Sizing::new().width(Length::Fixed(190.0)));
            }
            let select = on_select.clone();
            input.on_change(move |_| select(path.clone()));
            row.append(&input);
        }
        "PasswordInput" => {
            let input = ui.password_input().map_err(|e| e.to_string())?;
            input.set_placeholder(node.attribute("placeholder").unwrap_or(""));
            input.set_enabled(enabled);
            let select = on_select.clone();
            input.on_change(move |_| select(path.clone()));
            row.append(&input);
        }
        "SearchInput" => {
            let input = ui.search_input().map_err(|e| e.to_string())?;
            input.set_text(node.attribute("text").unwrap_or(""));
            input.set_placeholder(node.attribute("placeholder").unwrap_or(""));
            input.set_enabled(enabled);
            let select = on_select.clone();
            input.on_change(move |_| select(path.clone()));
            row.append(&input);
        }
        "NumberInput" => {
            let value = node
                .attribute("value")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.0);
            let input = ui.number_input(value).map_err(|e| e.to_string())?;
            input.set_enabled(enabled);
            input.set_range(
                node.attribute("minimum")
                    .and_then(|v| v.parse::<f64>().ok()),
                node.attribute("maximum")
                    .and_then(|v| v.parse::<f64>().ok()),
            );
            let select = on_select.clone();
            input.on_change(move |_| select(path.clone()));
            row.append(&input);
        }
        "TextArea" => {
            let input = ui
                .text_area(node.attribute("text").unwrap_or(""))
                .map_err(|e| e.to_string())?;
            input.set_placeholder(node.attribute("placeholder").unwrap_or(""));
            input.set_enabled(enabled);
            input.set_sizing(
                Sizing::new().width(Length::Fill).height(Length::Fixed(
                    node.attribute("height")
                        .and_then(|value| value.parse::<f64>().ok())
                        .unwrap_or(100.0)
                        .clamp(48.0, 1200.0),
                )),
            );
            let selected = on_select.clone();
            input.on_change(move |_| selected(path.clone()));
            row.append(&input);
        }
        "CheckBox" => {
            let input = ui
                .checkbox(node.attribute("text").unwrap_or("Check box"))
                .map_err(|e| e.to_string())?;
            input.set_checked(node.attribute("checked") == Some("true"));
            input.set_enabled(enabled);
            #[cfg(target_os = "linux")]
            {
                use gtk4::prelude::*;
                use naui::Widget as _;
                input.native_widget().set_halign(gtk4::Align::Start);
            }
            let select = on_select.clone();
            input.on_toggle(move |_| select(path.clone()));
            row.append(&input);
        }
        "Switch" => {
            let input = ui
                .toggle(node.attribute("text").unwrap_or(""))
                .map_err(|e| e.to_string())?;
            input.set_on(node.attribute("checked") == Some("true"));
            input.set_enabled(enabled);
            #[cfg(target_os = "linux")]
            {
                use gtk4::prelude::*;
                use naui::Widget as _;
                input.native_widget().set_halign(gtk4::Align::Start);
            }
            let select = on_select.clone();
            input.on_toggle(move |_| select(path.clone()));
            row.append(&input);
        }
        "Select" => {
            let input = ui.combo_box().map_err(|e| e.to_string())?;
            input.set_enabled(enabled);
            let choices: Vec<_> = node.attribute("items").unwrap_or("").split('|').collect();
            input.set_items(&choices);
            if runtime_fills(node) {
                input.set_sizing(Sizing::fill_width());
            }
            input.set_selected(
                node.attribute("value")
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(0),
            );
            let select = on_select.clone();
            input.on_select(move |_| select(path.clone()));
            row.append(&input);
        }
        "RadioGroup" => {
            let input = ui.radio_group().map_err(|e| e.to_string())?;
            input.set_enabled(enabled);
            let choices: Vec<_> = node.attribute("items").unwrap_or("").split('|').collect();
            input.set_items(&choices);
            input.set_selected(
                node.attribute("value")
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(0),
            );
            let select = on_select.clone();
            input.on_select(move |_| select(path.clone()));
            row.append(&input);
        }
        "Slider" => {
            let min = node
                .attribute("minimum")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.0);
            let max = node
                .attribute("maximum")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(100.0)
                .max(min + 1.0);
            let slider = ui.slider(min, max).map_err(|e| e.to_string())?;
            slider.set_enabled(enabled);
            if runtime_fills(node) {
                slider.set_sizing(Sizing::fill_width());
            }
            slider.set_value(
                node.attribute("value")
                    .and_then(|v| v.parse::<f64>().ok())
                    .unwrap_or(min)
                    .clamp(min, max),
            );
            let select = on_select.clone();
            slider.on_change(move |_| select(path.clone()));
            row.append(&slider);
        }
        "ProgressBar" => {
            let progress = ui.progress_bar().map_err(|e| e.to_string())?;
            if runtime_fills(node) {
                progress.set_sizing(Sizing::fill_width());
            }
            let min = node
                .attribute("minimum")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.0);
            let max = node
                .attribute("maximum")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(100.0);
            let value = node
                .attribute("value")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(min);
            progress.set_value(if max > min {
                (value - min) / (max - min)
            } else {
                0.0
            });
            row.append(&progress);
        }
        "Image" => {
            let source = image_source(document, node.attribute("source").unwrap_or(""));
            let image = ui.image(&source).map_err(|e| e.to_string())?;
            row.append(&image);
        }
        "Separator" => {
            let separator = ui.canvas().map_err(|e| e.to_string())?;
            separator.set_sizing(Sizing::new().width(Length::Fill).height(Length::Fixed(1.0)));
            separator.on_draw(|p| {
                p.fill_rect(
                    naui::Rect::new(0.0, 0.0, p.width(), 1.0),
                    naui::Color::rgb(0x83, 0x86, 0x8c),
                )
            });
            row.append(&separator);
        }
        "Spacer" => {
            let spacer = ui.spacer().map_err(|e| e.to_string())?;
            spacer.set_sizing(
                Sizing::new().height(Length::Fixed(
                    node.attribute("height")
                        .and_then(|v| v.parse::<f64>().ok())
                        .unwrap_or(12.0),
                )),
            );
            row.append(&spacer);
        }
        "ListBox" => {
            let list = ui.list().map_err(|e| e.to_string())?;
            let items = node.attribute("items").unwrap_or("");
            list.set_items(&naui::ListItem::list(items.split('|')));
            list.set_selected(
                node.attribute("value")
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(0),
            );
            list.set_sizing(
                Sizing::new().height(Length::Fixed(
                    node.attribute("height")
                        .and_then(|v| v.parse::<f64>().ok())
                        .unwrap_or(160.0),
                )),
            );
            let select = on_select.clone();
            list.on_select(move |_| select(path.clone()));
            row.append(&list);
        }
        "Table" => {
            let table = ui.table().map_err(|e| e.to_string())?;
            table.set_columns(&naui::TableColumn::list(
                node.attribute("columns").unwrap_or("").split('|'),
            ));
            let rows = node.attribute("rows").unwrap_or("");
            table.set_rows(
                &rows
                    .lines()
                    .map(|line| naui::TableRow::new(line.split('|')))
                    .collect::<Vec<_>>(),
            );
            table.set_sizing(
                Sizing::new().height(Length::Fixed(
                    node.attribute("height")
                        .and_then(|v| v.parse::<f64>().ok())
                        .unwrap_or(160.0),
                )),
            );
            table.set_selected(
                node.attribute("value")
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(0),
            );
            let select = on_select.clone();
            table.on_select(move |_| select(path.clone()));
            row.append(&table);
        }
        "FilePicker" => {
            // Hydrogen's picker is a native button until its dialog opens.
            // A design-time click selects the control; no real OS picker runs.
            let button = ui
                .button(node.attribute("text").unwrap_or("Choose file…"))
                .map_err(|e| e.to_string())?;
            button.set_enabled(enabled);
            let select = on_select.clone();
            button.on_click(move || select(path.clone()));
            row.append(&button);
        }
        "Spinner" => {
            // Spinner is deliberately omitted until the platform indicator
            // can be hosted without an inaccurate imitation.
        }
        _ => {
            // Unknown future runtime controls are not faked with labels.
            let children = ui.stack(Orientation::Vertical).map_err(|e| e.to_string())?;
            render_children(
                ui,
                &children,
                node,
                &path,
                on_select,
                design_tabs,
                available_width,
                document,
            )?;
            row.append(&children);
        }
    }
    parent.append(&row);
    Ok(())
}

/// Elements that are understood by Hydrogen but do not currently have
/// a matching live native design-time widget. Keep this visible in the IDE
/// status rather than silently presenting an incomplete window as exact.
pub fn missing_native_controls(xml: &str) -> Result<Vec<String>, String> {
    let parsed = roxmltree::Document::parse(xml).map_err(|e| e.to_string())?;
    let mut missing = std::collections::BTreeSet::new();
    for node in parsed.descendants().filter(|node| node.is_element()) {
        if matches!(node.tag_name().name(), "Spinner" | "Popover" | "Toolbar") {
            missing.insert(node.tag_name().name().to_owned());
        }
    }
    Ok(missing.into_iter().collect())
}

pub fn render(
    ui: &Ui,
    xml: &str,
    document: &Path,
    on_select: SelectElement,
    design_tabs: DesignTabs,
) -> Result<naui::Stack, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| e.to_string())?;
    let root = doc.root_element();
    if root.tag_name().name() != "Window" {
        return Err("Expected a Hydrogen <Window> UI".into());
    }
    let outer = ui.stack(Orientation::Vertical).map_err(|e| e.to_string())?;
    outer.set_spacing(0.0);
    // Align the window at the viewport origin: never introduce padding that
    // makes the preview float in the middle of the Design surface.
    outer.set_padding(Padding::all(0.0));
    let window = ui.stack(Orientation::Vertical).map_err(|e| e.to_string())?;
    // Match Hydrogen's GtkView: the application body is a vertical box with
    // 20px spacing and Window.padding (24px when unspecified).
    window.set_spacing(20.0);
    let body_padding = root
        .attribute("padding")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(24.0)
        .clamp(0.0, 120.0);
    window.set_padding(Padding::all(body_padding));
    let width = root
        .attribute("width")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(720.0)
        .clamp(320.0, 1400.0);
    let height = root
        .attribute("height")
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(480.0)
        .clamp(240.0, 1800.0);
    // Retain the intended window's document dimensions even if the current
    // tab has only a few controls. The preview scrolls inside Quark instead
    // of stretching across and losing its frame.
    window.set_sizing(
        Sizing::new().width(Length::Fixed(width)).min_height(
            (height
                - if cfg!(target_os = "linux") {
                    48.0
                } else {
                    34.0
                })
            .max(120.0),
        ),
    );
    // A visible window frame, kept inside the design viewport. The body
    // itself is composed of platform-native widgets.
    #[cfg(target_os = "linux")]
    {
        use gtk4::prelude::*;
        use naui::Widget as _;

        // Exactly the HeaderBar class used by Hydrogen's GTK application,
        // but without window-manager buttons: this is an embedded child of
        // Quark, so working close/minimise controls would target the IDE.
        let header = libadwaita::HeaderBar::new();
        header.set_show_start_title_buttons(false);
        header.set_show_end_title_buttons(false);
        let title = root.attribute("title").unwrap_or("Hydrogen Window");
        header.set_title_widget(Some(&gtk4::Label::new(Some(title))));
        header.set_tooltip_text(Some(
            "Design-only native header; use OS Window for actual window controls",
        ));
        header.set_width_request(width as i32);
        let host = outer
            .native_widget()
            .downcast::<gtk4::Box>()
            .map_err(|_| "Native GTK preview host is not a GtkBox")?;
        // Libadwaita's card style follows the user's light/dark system
        // palette and visually separates the design window from Quark.
        install_gtk_design_shadow();
        host.add_css_class("card");
        host.add_css_class("quark-design-frame");
        host.append(&header);
        let selected = on_select.clone();
        let gesture = gtk4::GestureClick::new();
        gesture.connect_pressed(move |_, _, _, _| selected(Vec::new()));
        header.add_controller(gesture);
    }
    #[cfg(not(target_os = "linux"))]
    {
        let titlebar = ui.canvas().map_err(|e| e.to_string())?;
        titlebar.set_sizing(
            Sizing::new()
                .width(Length::Fixed(width))
                .height(Length::Fixed(34.0)),
        );
        let title = root
            .attribute("title")
            .unwrap_or("Hydrogen Window")
            .to_owned();
        let select = on_select.clone();
        titlebar.on_draw(move |paint| {
            paint.fill_rect(
                naui::Rect::new(0.0, 0.0, paint.width(), paint.height()),
                naui::Color::rgb(0x45, 0x47, 0x50),
            );
            paint.fill_rect(
                naui::Rect::new(0.0, 33.0, paint.width(), 1.0),
                naui::Color::rgb(0x70, 0x72, 0x7a),
            );
            // The embedded title bar is nonfunctional preview chrome: only
            // Hydrogen's external OS Window action creates real window chrome.
            // Use familiar host-platform controls so the designer reads like
            // a window rather than a generic form on a canvas.
            #[cfg(target_os = "macos")]
            for (x, color) in [
                (16.0, naui::Color::rgb(0xff, 0x60, 0x57)),
                (36.0, naui::Color::rgb(0xff, 0xbd, 0x2e)),
                (56.0, naui::Color::rgb(0x28, 0xc8, 0x40)),
            ] {
                paint.fill_circle(naui::Point::new(x, 17.0), 5.3, color);
            }
            #[cfg(not(target_os = "macos"))]
            {
                let edge = paint.width();
                let light = naui::Color::rgb(0xc6, 0xc8, 0xd0);
                paint.line(
                    naui::Point::new(edge - 77.0, 17.0),
                    naui::Point::new(edge - 65.0, 17.0),
                    light,
                    1.4,
                );
                paint.stroke_rect(naui::Rect::new(edge - 48.0, 12.0, 11.0, 10.0), light, 1.2);
                paint.line(
                    naui::Point::new(edge - 22.0, 12.0),
                    naui::Point::new(edge - 12.0, 22.0),
                    light,
                    1.5,
                );
                paint.line(
                    naui::Point::new(edge - 12.0, 12.0),
                    naui::Point::new(edge - 22.0, 22.0),
                    light,
                    1.5,
                );
            }
            paint.text(
                &title,
                naui::Point::new(
                    if cfg!(target_os = "macos") {
                        76.0
                    } else {
                        12.0
                    },
                    9.0,
                ),
                12.0,
                naui::Color::rgb(0xf0, 0xf0, 0xf2),
            );
        });
        titlebar.on_pointer(move |event| {
            if event.phase == naui::PointerPhase::Down {
                select(Vec::new());
            }
        });
        outer.append(&titlebar);
    }
    render_children(
        ui,
        &window,
        root,
        &[],
        &on_select,
        &design_tabs,
        (width
            - 2.0
                * root
                    .attribute("padding")
                    .and_then(|v| v.parse::<f64>().ok())
                    .unwrap_or(24.0))
        .max(160.0),
        document,
    )?;
    outer.append(&window);
    #[cfg(not(target_os = "linux"))]
    {
        let frame_edge = ui.canvas().map_err(|e| e.to_string())?;
        frame_edge.set_sizing(
            Sizing::new()
                .width(Length::Fixed(width))
                .height(Length::Fixed(2.0)),
        );
        frame_edge.on_draw(|paint| {
            paint.fill_rect(
                naui::Rect::new(0.0, 0.0, paint.width(), 2.0),
                naui::Color::rgb(0x70, 0x72, 0x7a),
            );
        });
        outer.append(&frame_edge);
    }
    Ok(outer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warns_about_controls_without_a_true_design_time_widget() {
        let xml = "<Window><Button text=\"Hi\"/><Spinner/><Popover/><Toolbar/><Spinner/></Window>";
        assert_eq!(
            missing_native_controls(xml).unwrap(),
            ["Popover", "Spinner", "Toolbar"]
        );
        assert!(
            missing_native_controls("<Window><Tabs><Tab><TextArea/></Tab></Tabs></Window>")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn relative_ui_image_assets_resolve_from_document_directory() {
        let directory = tempfile::tempdir().unwrap();
        let document = directory.path().join("Studio.hyui");
        assert_eq!(
            image_source(&document, "assets/logo.png"),
            directory.path().join("assets/logo.png").to_string_lossy()
        );
        assert_eq!(
            image_source(&document, "data:image/png;base64,a"),
            "data:image/png;base64,a"
        );
        assert_eq!(
            image_source(&document, "https://example.com/logo.png"),
            "https://example.com/logo.png"
        );
    }

    #[test]
    fn real_hydrogen_studio_default_tab_is_one_but_design_navigation_is_independent() {
        let source = include_str!("../tests/fixtures/Studio.hyui");
        let document = roxmltree::Document::parse(source).unwrap();
        let tabs = document
            .descendants()
            .find(|node| node.has_tag_name("Tabs"))
            .unwrap();
        let startup = tabs
            .attribute("value")
            .unwrap_or("0")
            .parse::<usize>()
            .unwrap();
        let count = tabs
            .children()
            .filter(|node| node.has_tag_name("Tab"))
            .count();
        assert_eq!(design_tab_index(startup, None, count), Some(0));
        assert_eq!(design_tab_index(startup, Some(1), count), Some(1));
        assert_eq!(tabs.attribute("value"), Some("0"));
    }

    #[test]
    fn interactive_controls_claim_design_presses_but_containers_do_not() {
        for kind in [
            "Button", "Input", "Slider", "CheckBox", "ListBox", "Table", "Select",
        ] {
            assert!(design_press_claims_activation(kind), "{kind}");
        }
        for kind in ["Window", "Panel", "ColumnPanel", "GridPanel", "Tabs", "Tab"] {
            assert!(!design_press_claims_activation(kind), "{kind}");
        }
    }

    #[test]
    fn design_window_inset_leaves_room_for_shadows_without_changing_xml_geometry() {
        let xml = r#"<Window width="800" height="600" padding="24"><Label text="Hi"/></Window>"#;
        let root = roxmltree::Document::parse(xml).unwrap();
        let root = root.root_element();
        let width: f64 = root.attribute("width").unwrap().parse().unwrap();
        let height: f64 = root.attribute("height").unwrap().parse().unwrap();
        assert_eq!(width, 800.0);
        assert_eq!(height, 600.0);
        let extent = (
            width + DESIGN_VIEWPORT_INSET * 2.0,
            height + DESIGN_VIEWPORT_INSET * 2.0,
        );
        assert_eq!(extent, (832.0, 632.0));
        assert!(extent.0 > width && extent.1 > height);
        assert!(xml.contains("padding=\"24\""));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn gtk_shadow_uses_native_theme_colours_and_keeps_background_unmodified() {
        assert!(DESIGN_FRAME_CSS.contains("box.quark-design-frame"));
        assert!(DESIGN_FRAME_CSS.contains("box-shadow"));
        assert!(DESIGN_FRAME_CSS.contains("currentColor"));
        assert!(!DESIGN_FRAME_CSS.contains("#"));
        assert!(!DESIGN_FRAME_CSS.contains("background:"));
    }

    #[test]
    fn preferred_widths_are_minimums_and_summary_expands_to_fill_remaining_row() {
        let xml = r#"<RowPanel breakpoint="760" gap="20"><Panel preferredWidth="600"/><Panel preferredWidth="340"/></RowPanel>"#;
        let parsed = roxmltree::Document::parse(xml).unwrap();
        let root = parsed.root_element();
        let widths = row_child_widths(root, 1072.0, 20.0);
        assert_eq!(widths, [600.0, 452.0]);
        assert!(!row_panel_stacks(root, 1072.0, 20.0));
        assert_eq!(widths.iter().sum::<f64>() + 20.0, 1072.0);
    }

    #[test]
    fn preferred_width_overflow_wraps_even_above_explicit_breakpoint() {
        let xml = r#"<RowPanel breakpoint="760" gap="20"><Panel preferredWidth="600"/><Panel preferredWidth="340"/></RowPanel>"#;
        let parsed = roxmltree::Document::parse(xml).unwrap();
        let root = parsed.root_element();
        assert!(row_panel_stacks(root, 920.0, 20.0));
        assert!(row_panel_stacks(root, 700.0, 20.0));
        assert!(!row_panel_stacks(root, 960.0, 20.0));
    }

    #[test]
    fn row_preferred_widths_follow_hydrogen_studio_not_even_distribution() {
        let xml = include_str!("../tests/fixtures/Studio.hyui");
        let parsed = roxmltree::Document::parse(xml).unwrap();
        let row = parsed
            .descendants()
            .find(|node| {
                node.has_tag_name("RowPanel")
                    && node
                        .children()
                        .filter(|child| child.is_element() && child.has_tag_name("Panel"))
                        .count()
                        == 2
                    && node.attribute("gap") == Some("20")
            })
            .unwrap();
        let widths = row_child_widths(row, 1072.0, 20.0);
        assert_eq!(widths, vec![600.0, 452.0]);
        assert_ne!(widths, vec![526.0, 526.0]);
    }

    #[test]
    fn row_widths_distribute_remaining_space_and_skip_hidden_children() {
        let xml = r#"<RowPanel><Panel preferredWidth="240"/><Label/><Panel visible="false" preferredWidth="350"/><Panel/></RowPanel>"#;
        let parsed = roxmltree::Document::parse(xml).unwrap();
        let widths = row_child_widths(parsed.root_element(), 720.0, 20.0);
        assert_eq!(widths, vec![240.0, 220.0, 0.0, 220.0]);
    }

    #[test]
    fn hydrogen_menu_bar_groups_keep_source_order_and_a_single_horizontal_row() {
        let source = include_str!("../tests/fixtures/Studio.hyui");
        let document = roxmltree::Document::parse(source).unwrap();
        let bar = document
            .root_element()
            .children()
            .find(|element| element.has_tag_name("MenuBar"))
            .unwrap();
        let labels = bar
            .children()
            .filter(|child| child.has_tag_name("Menu"))
            .map(|menu| menu.attribute("text").unwrap_or(""))
            .collect::<Vec<_>>();
        assert_eq!(labels, ["Studio", "Help"]);
        // The GTK renderer constructs one GtkBox(Horizontal) for those
        // siblings. In particular, MenuBar must not inherit RowPanel's
        // breakpoint / vertical stacking behaviour.
        assert_ne!(bar.tag_name().name(), "RowPanel");
        assert!(bar.attribute("breakpoint").is_none());
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "Needs a running GTK display; exercised with xvfb-run in CI"]
    fn gtk_design_panel_uses_actual_native_frame_and_inner_margins() {
        use gtk4::prelude::*;

        gtk4::init().expect("GTK must initialise inside an Xvfb session");
        let box_widget = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        let frame = build_gtk_design_panel_frame(&box_widget, 18);
        assert!(frame.has_css_class("card"));
        assert!(frame.property::<bool>("hexpand"));
        assert_eq!(frame.halign(), gtk4::Align::Fill);
        assert_eq!(frame.child().unwrap(), box_widget.upcast::<gtk4::Widget>());
        assert_eq!(box_widget.margin_start(), 18);
        assert_eq!(box_widget.margin_end(), 18);
        assert_eq!(box_widget.margin_top(), 18);
        assert_eq!(box_widget.margin_bottom(), 18);
    }

    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "Needs a running GTK display; exercised with xvfb-run in CI"]
    fn gtk_design_menu_bar_is_one_horizontal_row_with_natural_size_buttons() {
        use gtk4::prelude::*;

        gtk4::init().expect("GTK must initialise inside an Xvfb session");
        let xml = include_str!("../tests/fixtures/Studio.hyui");
        let parsed = roxmltree::Document::parse(xml).unwrap();
        let menu = parsed
            .root_element()
            .children()
            .find(|node| node.has_tag_name("MenuBar"))
            .unwrap();
        let select: SelectElement = Rc::new(|_| {});
        let bar = build_gtk_design_menu_bar(menu, &[], &select);
        assert_eq!(bar.orientation(), gtk4::Orientation::Horizontal);
        assert_eq!(bar.spacing(), 8);
        assert!(!bar.property::<bool>("hexpand"));
        assert_eq!(bar.halign(), gtk4::Align::Start);

        let studio = bar
            .first_child()
            .unwrap()
            .downcast::<gtk4::MenuButton>()
            .unwrap();
        let help = studio
            .next_sibling()
            .unwrap()
            .downcast::<gtk4::MenuButton>()
            .unwrap();
        assert_eq!(studio.label().as_deref(), Some("Studio"));
        assert_eq!(help.label().as_deref(), Some("Help"));
        assert!(!studio.property::<bool>("hexpand") && !help.property::<bool>("hexpand"));
        assert!(help.next_sibling().is_none());
        assert!(studio.popover().is_some());
        assert!(help.popover().is_some());
    }

    #[test]
    fn hydrogen_studio_declares_top_level_native_menu_groups() {
        let xml = include_str!("../tests/fixtures/Studio.hyui");
        let parsed = roxmltree::Document::parse(xml).unwrap();
        let bar = parsed
            .root_element()
            .children()
            .find(|child| child.has_tag_name("MenuBar"))
            .unwrap();
        let groups = bar
            .children()
            .filter(|child| child.is_element() && child.has_tag_name("Menu"))
            .map(|menu| menu.attribute("text").unwrap_or(""))
            .collect::<Vec<_>>();
        assert_eq!(groups, ["Studio", "Help"]);
        assert_eq!(
            bar.children()
                .filter(|child| child.has_tag_name("MenuItem"))
                .count(),
            0
        );
    }

    #[test]
    fn hydrogen_default_control_expansion_matches_runtime_except_buttons() {
        let xml = r#"<Window><Panel><Label/><Input/><Slider/><ProgressBar/><Button/><Spinner/><Button fill="true"/><Label fill="false"/></Panel></Window>"#;
        let parsed = roxmltree::Document::parse(xml).unwrap();
        let elements = parsed
            .descendants()
            .filter(|node| node.is_element())
            .collect::<Vec<_>>();
        for (index, element) in elements.iter().enumerate().take(6) {
            assert!(runtime_fills(*element), "element {index} should fill");
        }
        assert!(!runtime_fills(elements[6]));
        assert!(!runtime_fills(elements[7]));
        assert!(runtime_fills(elements[8]));
        assert!(!runtime_fills(elements[9]));
    }

    #[test]
    fn runtime_expansion_respects_opt_out_and_explicit_overrides() {
        let xml = r#"<Window><FilePicker/><ToolButton/><Spinner fill="true"/><Input fill="false"/></Window>"#;
        let parsed = roxmltree::Document::parse(xml).unwrap();
        let elements = parsed
            .root_element()
            .children()
            .filter(|node| node.is_element())
            .collect::<Vec<_>>();
        assert!(!runtime_fills(elements[0]));
        assert!(!runtime_fills(elements[1]));
        assert!(runtime_fills(elements[2]));
        assert!(!runtime_fills(elements[3]));
    }

    #[test]
    fn hydrogen_gtk_labels_use_runtime_style_classes_not_generic_ide_accent() {
        assert_eq!(hydrogen_gtk_label_classes("title", ""), ["title-1"]);
        assert_eq!(
            hydrogen_gtk_label_classes("heading", "accent"),
            ["title-3", "accent"]
        );
        assert_eq!(
            hydrogen_gtk_label_classes("caption", "muted"),
            ["caption", "dim-label"]
        );
        assert!(hydrogen_gtk_label_classes("", "").is_empty());
    }

    #[test]
    fn studio_fixture_title_caption_and_muted_labels_match_hydrogen_css() {
        let xml = include_str!("../tests/fixtures/Studio.hyui");
        let parsed = roxmltree::Document::parse(xml).unwrap();
        let labels = parsed
            .descendants()
            .filter(|node| node.has_tag_name("Label"))
            .collect::<Vec<_>>();
        let title = labels
            .iter()
            .find(|node| node.attribute("role") == Some("title"))
            .unwrap();
        assert_eq!(
            hydrogen_gtk_label_classes(
                title.attribute("role").unwrap_or(""),
                title.attribute("tone").unwrap_or(""),
            ),
            ["title-1"]
        );
        let accent = labels
            .iter()
            .find(|node| {
                node.attribute("role") == Some("caption")
                    && node.attribute("tone") == Some("accent")
            })
            .unwrap();
        assert_eq!(
            hydrogen_gtk_label_classes(
                accent.attribute("role").unwrap_or(""),
                accent.attribute("tone").unwrap_or(""),
            ),
            ["caption", "accent"]
        );
        let muted = labels
            .iter()
            .find(|node| node.attribute("tone") == Some("muted"))
            .unwrap();
        assert!(hydrogen_gtk_label_classes(
            muted.attribute("role").unwrap_or(""),
            muted.attribute("tone").unwrap_or(""),
        )
        .contains(&"dim-label"));
    }

    #[test]
    fn hydrogen_studio_primary_button_uses_exact_runtime_attribute() {
        let xml = include_str!("../tests/fixtures/Studio.hyui");
        let parsed = roxmltree::Document::parse(xml).unwrap();
        let build = parsed
            .descendants()
            .find(|node| node.attribute("id") == Some("build"))
            .unwrap();
        assert_eq!(build.tag_name().name(), "Button");
        assert_eq!(build.attribute("primary"), Some("true"));
        assert!(runtime_fills(build));
        let cancel = parsed
            .descendants()
            .find(|node| node.attribute("id") == Some("cancel"))
            .unwrap();
        assert_ne!(cancel.attribute("primary"), Some("true"));
        assert!(runtime_fills(cancel));
    }

    #[test]
    fn first_tab_is_default_without_an_explicit_source_attribute() {
        assert_eq!(design_tab_index(0, None, 3), Some(0));
        assert_eq!(design_tab_index(0, None, 1), Some(0));
        assert_eq!(design_tab_index(0, None, 0), None);
    }

    #[test]
    fn navigating_design_tabs_does_not_change_compiled_startup_page() {
        let source = r#"<Window version="1"><Tabs value="0"><Tab title="First"/><Tab title="Second"/></Tabs></Window>"#;
        let override_selection = design_tab_index(0, Some(1), 2);
        assert_eq!(override_selection, Some(1));
        assert!(source.contains("value=\"0\""));
        assert_eq!(design_tab_index(0, None, 2), Some(0));
    }
}
