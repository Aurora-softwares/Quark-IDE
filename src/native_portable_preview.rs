//! Portable design-time rendering for Hydrogen <Window> files.
//! naui creates real AppKit / WinUI 3 / GTK controls from the same tree.
//! The preview never dispatches application handlers.
use naui::{Length, Orientation, Padding, Sizing, Ui};
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
    for (index, child) in node.children().filter(|child| child.is_element()).enumerate() {
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
        return Ok(());
    }
    let kind = node.tag_name().name();
    if matches!(kind, "Timer" | "Dialog") {
        // Non-visual nodes still appear as editable entries.
        let choose = ui.button(&format!("Edit {}", node_title(node)))
            .map_err(|e| e.to_string())?;
        let selected = on_select.clone();
        choose.on_click(move || selected(path.clone()));
        parent.append(&choose);
        return Ok(());
    }
    let row = ui.stack(Orientation::Vertical).map_err(|e| e.to_string())?;
    row.set_spacing(3.0);
    // Every element, including labels and layout containers, has a selection
    // target. Buttons additionally select when their native face is clicked.
    let choose = ui.button(&format!("◇ {}", node_title(node)))
        .map_err(|e| e.to_string())?;
    let selected = on_select.clone();
    let selected_path = path.clone();
    choose.on_click(move || selected(selected_path.clone()));
    row.append(&choose);

    match kind {
        "ColumnPanel" | "Panel" | "Tab" | "ScrollView" | "Window" => {
            let children = ui.stack(Orientation::Vertical).map_err(|e| e.to_string())?;
            children.set_spacing(node.attribute("gap").and_then(|v| v.parse::<f64>().ok()).unwrap_or(6.0).clamp(0.0, 48.0));
            render_children(ui, &children, node, &path, on_select)?;
            row.append(&children);
        }
        "RowPanel" | "GridPanel" | "MenuBar" | "Menu" => {
            let children = ui.stack(Orientation::Horizontal).map_err(|e| e.to_string())?;
            children.set_spacing(8.0);
            render_children(ui, &children, node, &path, on_select)?;
            row.append(&children);
        }
        "Tabs" => {
            let children = ui.stack(Orientation::Vertical).map_err(|e| e.to_string())?;
            let selected = node.attribute("value").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
            // Render the selected tab, but list the remaining tab headers so
            // their attributes can also be edited through Properties.
            for (i, tab) in node.children().filter(|n| n.is_element() && n.tag_name().name() == "Tab").enumerate() {
                let title = ui.button(tab.attribute("title").unwrap_or("Page")).map_err(|e| e.to_string())?;
                let mut tab_path = path.clone();
                let position = node.children().filter(|n| n.is_element()).position(|n| n == tab).unwrap_or(i);
                tab_path.push(position);
                let select = on_select.clone();
                title.on_click(move || select(tab_path.clone()));
                children.append(&title);
                if i == selected {
                    render_children(ui, &children, tab, &{
                        let mut p = path.clone();
                        p.push(position);
                        p
                    }, on_select)?;
                }
            }
            row.append(&children);
        }
        "Label" => row.append(&ui.label(node.attribute("text").unwrap_or("Label")).map_err(|e| e.to_string())?),
        "Button" | "MenuItem" => {
            let button = ui.button(node.attribute("text").unwrap_or("Button")).map_err(|e| e.to_string())?;
            let selected = on_select.clone();
            button.on_click(move || selected(path.clone()));
            row.append(&button);
        }
        "Input" | "NumberInput" | "SearchInput" => {
            let input = ui.text_input(node.attribute("text").unwrap_or("")).map_err(|e| e.to_string())?;
            input.set_placeholder(node.attribute("placeholder").unwrap_or(""));
            input.set_sizing(Sizing::new().width(Length::Fixed(190.0)));
            row.append(&input);
        }
        "TextArea" => {
            let input = ui.text_area(node.attribute("text").unwrap_or("")).map_err(|e| e.to_string())?;
            input.set_sizing(Sizing::new().width(Length::Fixed(240.0)).height(Length::Fixed(88.0)));
            row.append(&input);
        }
        "CheckBox" | "Switch" => {
            let input = ui.checkbox(node.attribute("text").unwrap_or("Check box")).map_err(|e| e.to_string())?;
            input.set_checked(node.attribute("checked") == Some("true"));
            row.append(&input);
        }
        "Select" | "ListBox" | "RadioGroup" => {
            let input = ui.combo_box().map_err(|e| e.to_string())?;
            let choices = node.attribute("items").unwrap_or("Choice");
            input.set_items(&choices.split('|').collect::<Vec<_>>());
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
    outer.set_spacing(8.0);
    outer.set_padding(Padding::all(14.0));
    let window = ui.stack(Orientation::Vertical).map_err(|e| e.to_string())?;
    window.set_spacing(8.0);
    window.set_padding(Padding::all(12.0));
    let width = root.attribute("width").and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(720.0).clamp(320.0, 1400.0);
    window.set_sizing(Sizing::new().width(Length::Fixed(width)));
    outer.append(&ui.label(root.attribute("title").unwrap_or("Hydrogen Window"))
        .map_err(|e| e.to_string())?);
    let choose_window = ui.button("Select window properties").map_err(|e| e.to_string())?;
    let selected = on_select.clone();
    choose_window.on_click(move || selected(Vec::new()));
    window.append(&choose_window);
    render_children(ui, &window, root, &[], &on_select)?;
    outer.append(&window);
    Ok(outer)
}
