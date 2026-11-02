//! Portable design-time rendering for Hydrogen <Window> files.
//! naui creates real AppKit / WinUI 3 / GTK controls from the same tree.
//! The preview never dispatches application handlers.
use naui::{GridCell, Length, Orientation, Padding, Sizing, Track, Ui};
use roxmltree::Node;
use std::rc::Rc;

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
) -> Result<(), String> {
    for (index, child) in node
        .children()
        .filter(|child| child.is_element())
        .enumerate()
    {
        let mut next = path.to_vec();
        next.push(index);
        render_node(ui, parent, child, next, on_select)?;
    }
    Ok(())
}

fn render_node(
    ui: &Ui,
    parent: &naui::Stack,
    node: Node<'_, '_>,
    path: Vec<usize>,
    on_select: &SelectElement,
) -> Result<(), String> {
    if node.attribute("visible") == Some("false") {
        let hidden = ui
            .button(&format!("Edit hidden {}", node_title(node)))
            .map_err(|e| e.to_string())?;
        let select = on_select.clone();
        hidden.on_click(move || select(path.clone()));
        parent.append(&hidden);
        return Ok(());
    }
    let kind = node.tag_name().name();
    if matches!(kind, "Timer" | "Dialog") {
        // Non-visual nodes still appear as editable entries.
        let choose = ui
            .button(&format!("Edit {}", node_title(node)))
            .map_err(|e| e.to_string())?;
        let selected = on_select.clone();
        choose.on_click(move || selected(path.clone()));
        parent.append(&choose);
        return Ok(());
    }
    let is_container = matches!(
        kind,
        "ColumnPanel"
            | "RowPanel"
            | "GridPanel"
            | "Panel"
            | "Tab"
            | "Tabs"
            | "ScrollView"
            | "Window"
            | "MenuBar"
            | "Menu"
    );
    let row = ui
        .stack(if is_container {
            Orientation::Vertical
        } else {
            Orientation::Horizontal
        })
        .map_err(|e| e.to_string())?;
    row.set_spacing(if is_container { 5.0 } else { 7.0 });
    // Compact, unobtrusive selector rather than a second full-sized button
    // repeating the text of every native control.
    let select_row = ui
        .stack(Orientation::Horizontal)
        .map_err(|e| e.to_string())?;
    select_row.set_spacing(4.0);
    let choose = ui.button("◇").map_err(|e| e.to_string())?;
    choose.set_sizing(Sizing::new().width(Length::Fixed(28.0)));
    let selected = on_select.clone();
    let selected_path = path.clone();
    choose.on_click(move || selected(selected_path.clone()));
    select_row.append(&choose);
    if is_container {
        select_row.append(&ui.label(&node_title(node)).map_err(|e| e.to_string())?);
    }
    if is_container {
        row.append(&select_row);
    } else {
        row.append(&choose);
    }

    match kind {
        "ColumnPanel" | "Panel" | "Tab" | "ScrollView" | "Window" => {
            let children = ui.stack(Orientation::Vertical).map_err(|e| e.to_string())?;
            children.set_spacing(
                node.attribute("gap")
                    .and_then(|v| v.parse::<f64>().ok())
                    .unwrap_or(6.0)
                    .clamp(0.0, 48.0),
            );
            render_children(ui, &children, node, &path, on_select)?;
            row.append(&children);
        }
        "RowPanel" | "MenuBar" | "Menu" => {
            let children = ui
                .stack(Orientation::Horizontal)
                .map_err(|e| e.to_string())?;
            children.set_spacing(8.0);
            render_children(ui, &children, node, &path, on_select)?;
            row.append(&children);
        }
        "GridPanel" => {
            let grid = ui.grid().map_err(|e| e.to_string())?;
            let columns = node
                .attribute("columns")
                .and_then(|n| n.parse::<usize>().ok())
                .unwrap_or(2)
                .clamp(1, 12);
            grid.set_spacing(8.0, 8.0);
            for column in 0..columns {
                grid.set_column_track(column, Track::FILL);
            }
            for (index, child) in node.children().filter(|n| n.is_element()).enumerate() {
                let cell = ui.stack(Orientation::Vertical).map_err(|e| e.to_string())?;
                let mut child_path = path.clone();
                child_path.push(index);
                render_node(ui, &cell, child, child_path, on_select)?;
                grid.attach(&cell, GridCell::new(index % columns, index / columns));
            }
            row.append(&grid);
        }
        "Tabs" => {
            let children = ui.stack(Orientation::Vertical).map_err(|e| e.to_string())?;
            let selected = node
                .attribute("value")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(0);
            // Render the selected tab, but list the remaining tab headers so
            // their attributes can also be edited through Properties.
            for (i, tab) in node
                .children()
                .filter(|n| n.is_element() && n.tag_name().name() == "Tab")
                .enumerate()
            {
                let title = ui
                    .button(tab.attribute("title").unwrap_or("Page"))
                    .map_err(|e| e.to_string())?;
                let mut tab_path = path.clone();
                let position = node
                    .children()
                    .filter(|n| n.is_element())
                    .position(|n| n == tab)
                    .unwrap_or(i);
                tab_path.push(position);
                let select = on_select.clone();
                title.on_click(move || select(tab_path.clone()));
                children.append(&title);
                if i == selected {
                    render_children(
                        ui,
                        &children,
                        tab,
                        &{
                            let mut p = path.clone();
                            p.push(position);
                            p
                        },
                        on_select,
                    )?;
                }
            }
            row.append(&children);
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
        "Input" | "NumberInput" | "SearchInput" => {
            let input = ui
                .text_input(node.attribute("text").unwrap_or(""))
                .map_err(|e| e.to_string())?;
            input.set_placeholder(node.attribute("placeholder").unwrap_or(""));
            input.set_sizing(Sizing::new().width(Length::Fixed(190.0)));
            let selected = on_select.clone();
            input.on_change(move |_| selected(path.clone()));
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
        "CheckBox" | "Switch" => {
            let input = ui
                .checkbox(node.attribute("text").unwrap_or("Check box"))
                .map_err(|e| e.to_string())?;
            input.set_checked(node.attribute("checked") == Some("true"));
            let selected = on_select.clone();
            input.on_toggle(move |_| selected(path.clone()));
            row.append(&input);
        }
        "Select" | "ListBox" | "RadioGroup" => {
            let input = ui.combo_box().map_err(|e| e.to_string())?;
            let choices = node.attribute("items").unwrap_or("Choice");
            input.set_items(&choices.split('|').collect::<Vec<_>>());
            let selected = on_select.clone();
            input.on_select(move |_| selected(path.clone()));
            row.append(&input);
        }
        "Slider" | "ProgressBar" | "Image" | "Spinner" | "Separator" | "Table" => {
            row.append(&ui.label(&node_title(node)).map_err(|e| e.to_string())?);
        }
        _ => {
            let children = ui.stack(Orientation::Vertical).map_err(|e| e.to_string())?;
            render_children(ui, &children, node, &path, on_select)?;
            row.append(&children);
        }
    }
    parent.append(&row);
    Ok(())
}

pub fn render(ui: &Ui, xml: &str, on_select: SelectElement) -> Result<naui::Stack, String> {
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
    render_children(ui, &window, root, &[], &on_select)?;
    outer.append(&window);
    Ok(outer)
}
