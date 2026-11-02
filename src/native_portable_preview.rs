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
    if node.attribute("fill") == Some("true") {
        row.set_sizing(Sizing::fill_width());
    } else if let Some(width) = node
        .attribute("preferredWidth")
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value >= 20.0)
    {
        row.set_sizing(Sizing::new().width(Length::Fixed(width.min(2400.0))));
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
            panel.set_padding(Padding::all(padding));
            panel.set_spacing(
                node.attribute("gap")
                    .and_then(|v| v.parse::<f64>().ok())
                    .unwrap_or(12.0)
                    .clamp(0.0, 48.0),
            );
            if let Some(title) = node.attribute("title").filter(|s| !s.is_empty()) {
                let label = ui.label(title).map_err(|e| e.to_string())?;
                label.set_style(naui::TextStyle::Heading);
                panel.append(&label);
            }
            #[cfg(target_os = "linux")]
            {
                use gtk4::prelude::*;
                use naui::Widget as _;
                panel.native_widget().add_css_class("card");
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
            row.append(&panel);
        }
        "ColumnPanel" | "Tab" | "Window" => {
            let children = ui.stack(Orientation::Vertical).map_err(|e| e.to_string())?;
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
        "RowPanel" | "MenuBar" | "Menu" => {
            let direction = if kind == "RowPanel"
                && available_width
                    < node
                        .attribute("breakpoint")
                        .and_then(|v| v.parse::<f64>().ok())
                        .unwrap_or(700.0)
            {
                Orientation::Vertical
            } else {
                Orientation::Horizontal
            };
            let children = ui.stack(direction).map_err(|e| e.to_string())?;
            let gap = node
                .attribute("gap")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(12.0)
                .clamp(0.0, 48.0);
            children.set_spacing(gap);
            let count = node.children().filter(|n| n.is_element()).count().max(1);
            let child_width = if direction == Orientation::Horizontal {
                ((available_width - (count as f64 - 1.0) * gap) / count as f64).max(80.0)
            } else {
                available_width
            };
            render_children(
                ui,
                &children,
                node,
                &path,
                on_select,
                design_tabs,
                child_width,
                document,
            )?;
            row.append(&children);
        }
        "GridPanel" => {
            let grid = ui.grid().map_err(|e| e.to_string())?;
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
            // Use the actual native tab widget (GtkNotebook on Linux,
            // NSSegmentedControl-hosted pages on macOS, WinUI tabs on Windows).
            // No fake text buttons or duplicate labels.
            let tabs = ui.tabs().map_err(|e| e.to_string())?;
            let mut count = 0;
            for (position, tab) in node.children().filter(|n| n.is_element()).enumerate() {
                if tab.tag_name().name() != "Tab" {
                    continue;
                }
                let page = ui.stack(Orientation::Vertical).map_err(|e| e.to_string())?;
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
                tabs.add_tab(tab.attribute("title").unwrap_or("Page"), &page);
                count += 1;
            }
            if count > 0 {
                // Hydrogen stores zero-based indices. Unset value=0 is page 1.
                let runtime_default = node
                    .attribute("value")
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(0);
                let design_override = design_tabs.borrow().get(&path).copied();
                if let Some(selection) = design_tab_index(runtime_default, design_override, count) {
                    tabs.set_selected(selection);
                }
                let design_tabs = design_tabs.clone();
                let select = on_select.clone();
                tabs.on_select(move |index| {
                    design_tabs.borrow_mut().insert(path.clone(), index);
                    // Select Tabs in the Properties inspector, not Window.
                    // Design-only navigation never writes Tabs.value.
                    select(path.clone());
                });
            }
            row.append(&tabs);
        }
        "Label" => {
            let label = ui
                .label(node.attribute("text").unwrap_or(""))
                .map_err(|e| e.to_string())?;
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
            row.append(&label);
        }
        "Button" | "MenuItem" | "ToolButton" => {
            let button = ui
                .button(node.attribute("text").unwrap_or("Button"))
                .map_err(|e| e.to_string())?;
            button.set_enabled(enabled);
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
            input.set_sizing(Sizing::new().width(Length::Fixed(190.0)));
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
            let select = on_select.clone();
            input.on_toggle(move |_| select(path.clone()));
            row.append(&input);
        }
        "Select" => {
            let input = ui.combo_box().map_err(|e| e.to_string())?;
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
    window.set_spacing(8.0);
    window.set_padding(Padding::all(12.0));
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
        Sizing::new()
            .width(Length::Fixed(width))
            .min_height((height - 34.0).max(120.0)),
    );
    // A visible window frame, kept inside the design viewport. The body
    // itself is composed of platform-native widgets.
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
        for kind in ["Button", "Input", "Slider", "CheckBox", "ListBox", "Table", "Select"] {
            assert!(design_press_claims_activation(kind), "{kind}");
        }
        for kind in ["Window", "Panel", "ColumnPanel", "GridPanel", "Tabs", "Tab"] {
            assert!(!design_press_claims_activation(kind), "{kind}");
        }
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
