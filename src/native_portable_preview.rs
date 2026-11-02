//! Portable design-time rendering for Hydrogen <Window> files.
//! naui creates real AppKit / WinUI 3 / GTK controls from the same tree.
//! The preview never dispatches application handlers.
use naui::{GridCell, Length, Orientation, Padding, Sizing, Track, Ui};
use roxmltree::Node;
use std::{cell::RefCell, collections::HashMap, rc::Rc};

/// Design-only active page per tab container. The source's Tabs.value stays
/// unchanged: a designer may explore page 2 without changing startup page 1.
pub type DesignTabs = Rc<RefCell<HashMap<Vec<usize>, usize>>>;

pub type SelectElement = Rc<dyn Fn(Vec<usize>)>;

fn node_title(node: Node<'_, '_>) -> String {
    let kind = node.tag_name().name();
    let label = node
        .attribute("id")
        .or_else(|| node.attribute("title"))
        .or_else(|| node.attribute("text"))
        .unwrap_or(kind);
    format!("{kind} · {label}")
}

fn render_children(
    ui: &Ui,
    parent: &naui::Stack,
    node: Node<'_, '_>,
    path: &[usize],
    on_select: &SelectElement,
    design_tabs: &DesignTabs,
) -> Result<(), String> {
    for (index, child) in node
        .children()
        .filter(|child| child.is_element())
        .enumerate()
    {
        let mut next = path.to_vec();
        next.push(index);
        render_node(ui, parent, child, next, on_select, design_tabs)?;
    }
    Ok(())
}

fn render_node(
    ui: &Ui,
    parent: &naui::Stack,
    node: Node<'_, '_>,
    path: Vec<usize>,
    on_select: &SelectElement,
    design_tabs: &DesignTabs,
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

    match kind {
        "ColumnPanel" | "Panel" | "Tab" | "ScrollView" | "Window" => {
            let children = ui.stack(Orientation::Vertical).map_err(|e| e.to_string())?;
            children.set_spacing(
                node.attribute("gap")
                    .and_then(|v| v.parse::<f64>().ok())
                    .unwrap_or(12.0)
                    .clamp(0.0, 48.0),
            );
            render_children(ui, &children, node, &path, on_select, design_tabs)?;
            row.append(&children);
        }
        "RowPanel" | "MenuBar" | "Menu" => {
            let children = ui
                .stack(Orientation::Horizontal)
                .map_err(|e| e.to_string())?;
            children.set_spacing(
                node.attribute("gap")
                    .and_then(|v| v.parse::<f64>().ok())
                    .unwrap_or(12.0)
                    .clamp(0.0, 48.0),
            );
            render_children(ui, &children, node, &path, on_select, design_tabs)?;
            row.append(&children);
        }
        "GridPanel" => {
            let grid = ui.grid().map_err(|e| e.to_string())?;
            let columns = node
                .attribute("columns")
                .and_then(|n| n.parse::<usize>().ok())
                .unwrap_or(3)
                .clamp(1, 12);
            let gap = node
                .attribute("gap")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(16.0)
                .clamp(0.0, 48.0);
            grid.set_spacing(gap, gap);
            for column in 0..columns {
                grid.set_column_track(column, Track::FILL);
            }
            for (index, child) in node.children().filter(|n| n.is_element()).enumerate() {
                let cell = ui.stack(Orientation::Vertical).map_err(|e| e.to_string())?;
                let mut child_path = path.clone();
                child_path.push(index);
                render_node(ui, &cell, child, child_path, on_select, design_tabs)?;
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
                render_children(ui, &page, tab, &tab_path, on_select, design_tabs)?;
                tabs.add_tab(tab.attribute("title").unwrap_or("Page"), &page);
                count += 1;
            }
            if count > 0 {
                // Hydrogen stores zero-based indices. Unset value=0 is page 1.
                let runtime_default = node
                    .attribute("value")
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(0);
                let selection = design_tabs
                    .borrow()
                    .get(&path)
                    .copied()
                    .unwrap_or(runtime_default)
                    .min(count - 1);
                tabs.set_selected(selection);
                let design_tabs = design_tabs.clone();
                let select = on_select.clone();
                tabs.on_select(move |index| {
                    design_tabs.borrow_mut().insert(path.clone(), index);
                    // Selecting another page for design never writes XML.
                    select(Vec::new());
                });
            }
            row.append(&tabs);
        }
        "Label" => row.append(
            &ui.label(node.attribute("text").unwrap_or("Label"))
                .map_err(|e| e.to_string())?,
        ),
        "Button" | "MenuItem" => {
            let button = ui
                .button(node.attribute("text").unwrap_or("Button"))
                .map_err(|e| e.to_string())?;
            let selected = on_select.clone();
            button.on_click(move || selected(path.clone()));
            row.append(&button);
        }
        "Input" => {
            let input = ui
                .text_input(node.attribute("text").unwrap_or(""))
                .map_err(|e| e.to_string())?;
            input.set_placeholder(node.attribute("placeholder").unwrap_or(""));
            input.set_sizing(Sizing::new().width(Length::Fixed(190.0)));
            let select = on_select.clone();
            input.on_change(move |_| select(path.clone()));
            row.append(&input);
        }
        "PasswordInput" => {
            let input = ui.password_input().map_err(|e| e.to_string())?;
            input.set_placeholder(node.attribute("placeholder").unwrap_or(""));
            let select = on_select.clone();
            input.on_change(move |_| select(path.clone()));
            row.append(&input);
        }
        "SearchInput" => {
            let input = ui.search_input().map_err(|e| e.to_string())?;
            input.set_text(node.attribute("text").unwrap_or(""));
            input.set_placeholder(node.attribute("placeholder").unwrap_or(""));
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
            input.set_sizing(
                Sizing::new()
                    .width(Length::Fixed(240.0))
                    .height(Length::Fixed(88.0)),
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
            let select = on_select.clone();
            input.on_toggle(move |_| select(path.clone()));
            row.append(&input);
        }
        "Switch" => {
            let input = ui
                .toggle(node.attribute("text").unwrap_or(""))
                .map_err(|e| e.to_string())?;
            input.set_on(node.attribute("checked") == Some("true"));
            let select = on_select.clone();
            input.on_toggle(move |_| select(path.clone()));
            row.append(&input);
        }
        "Select" => {
            let input = ui.combo_box().map_err(|e| e.to_string())?;
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
            let image = ui
                .image(node.attribute("source").unwrap_or(""))
                .map_err(|e| e.to_string())?;
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
        "ListBox" | "Table" | "Spinner" | "FilePicker" => {
            // These require a full presenter, not a misleading text stand-in.
            // The hierarchy inspector still exposes every attribute.
        }
        _ => {
            // Unknown future runtime controls are not faked with labels.
            let children = ui.stack(Orientation::Vertical).map_err(|e| e.to_string())?;
            render_children(ui, &children, node, &path, on_select, design_tabs)?;
            row.append(&children);
        }
    }
    parent.append(&row);
    Ok(())
}

pub fn render(
    ui: &Ui,
    xml: &str,
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
    outer.set_padding(Padding::all(18.0));
    let window = ui.stack(Orientation::Vertical).map_err(|e| e.to_string())?;
    window.set_spacing(8.0);
    window.set_padding(Padding::all(12.0));
    let width = root
        .attribute("width")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(720.0)
        .clamp(320.0, 1400.0);
    window.set_sizing(Sizing::new().width(Length::Fixed(width)));
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
        paint.text(
            &title,
            naui::Point::new(12.0, 9.0),
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
    render_children(ui, &window, root, &[], &on_select, &design_tabs)?;
    outer.append(&window);
    Ok(outer)
}
