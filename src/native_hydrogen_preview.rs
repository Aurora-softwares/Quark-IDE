//! Read-only GTK4 design-time preview for real Hydrogen <Window> XML.
//! Uses actual native GTK widgets, rather than painting mock controls.
//! Runtime events are deliberately not dispatched in the designer.
use gtk4::prelude::*;
use gtk4::{self as gtk, Orientation};
use roxmltree::Node;

fn attr<'a, 'input>(node: Node<'a, 'input>, key: &str) -> &'a str {
    node.attribute(key).unwrap_or("")
}

fn gap(node: Node<'_, '_>, default: i32) -> i32 {
    attr(node, "gap").parse::<i32>().unwrap_or(default).clamp(0, 48)
}

fn build_children(node: Node<'_, '_>, direction: Orientation) -> gtk::Box {
    let container = gtk::Box::new(direction, gap(node, 8));
    container.set_hexpand(true);
    container.set_valign(gtk::Align::Start);
    for child in node.children().filter(|n| n.is_element()) {
        if let Some(widget) = build_widget(child) {
            container.append(&widget);
        }
    }
    container
}

fn build_widget(node: Node<'_, '_>) -> Option<gtk::Widget> {
    if attr(node, "visible") == "false" {
        return None;
    }
    let kind = node.tag_name().name();
    let w: gtk::Widget = match kind {
        "ColumnPanel" | "Tab" => build_children(node, Orientation::Vertical).upcast(),
        "RowPanel" => {
            // FlowBox keeps controls wrapped as the design-time viewport changes.
            let flow = gtk::FlowBox::new();
            flow.set_column_spacing(gap(node, 8) as u32);
            flow.set_row_spacing(gap(node, 8) as u32);
            flow.set_selection_mode(gtk::SelectionMode::None);
            for child in node.children().filter(|n| n.is_element()) {
                if let Some(widget) = build_widget(child) {
                    flow.insert(&widget, -1);
                }
            }
            flow.upcast()
        }
        "GridPanel" => {
            let grid = gtk::Grid::new();
            let columns = attr(node, "columns").parse::<i32>().unwrap_or(2).clamp(1, 12);
            grid.set_column_spacing(gap(node, 8) as u32);
            grid.set_row_spacing(gap(node, 8) as u32);
            grid.set_column_homogeneous(true);
            for (index, child) in node.children().filter(|n| n.is_element()).enumerate() {
                if let Some(widget) = build_widget(child) {
                    let index = index as i32;
                    grid.attach(&widget, index % columns, index / columns, 1, 1);
                }
            }
            grid.upcast()
        }
        "Panel" => {
            let frame = gtk::Frame::new(node.attribute("title"));
            let content = build_children(node, Orientation::Vertical);
            let padding = attr(node, "padding").parse::<i32>().unwrap_or(12).clamp(0, 40);
            content.set_margin_start(padding);
            content.set_margin_end(padding);
            content.set_margin_top(padding);
            content.set_margin_bottom(padding);
            frame.set_child(Some(&content));
            frame.set_hexpand(true);
            frame.upcast()
        }
        "Tabs" => {
            let tabs = gtk::Notebook::new();
            for tab in node.children().filter(|n| n.is_element() && n.tag_name().name() == "Tab") {
                let child = build_children(tab, Orientation::Vertical);
                child.set_margin_start(10);
                child.set_margin_end(10);
                child.set_margin_top(10);
                child.set_margin_bottom(10);
                let title = gtk::Label::new(Some(tab.attribute("title").unwrap_or("Page")));
                tabs.append_page(&child, Some(&title));
            }
            let selected = attr(node, "value").parse::<u32>().unwrap_or(0);
            tabs.set_current_page(Some(selected));
            tabs.upcast()
        }
        "ScrollView" => {
            let scroll = gtk::ScrolledWindow::new();
            scroll.set_min_content_height(attr(node, "height").parse().unwrap_or(140));
            scroll.set_child(Some(&build_children(node, Orientation::Vertical)));
            scroll.upcast()
        }
        "Label" => {
            let label = gtk::Label::new(Some(attr(node, "text")));
            label.set_xalign(0.0);
            label.set_wrap(true);
            label.upcast()
        }
        "Button" => gtk::Button::with_label(attr(node, "text")).upcast(),
        "Input" | "NumberInput" => {
            let field = gtk::Entry::new();
            field.set_text(attr(node, "text"));
            field.set_placeholder_text(node.attribute("placeholder"));
            field.upcast()
        }
        "SearchInput" => {
            let field = gtk::SearchEntry::new();
            field.set_text(attr(node, "text"));
            field.set_placeholder_text(node.attribute("placeholder"));
            field.upcast()
        }
        "TextArea" => {
            let field = gtk::TextView::new();
            field.buffer().set_text(attr(node, "text"));
            field.set_wrap_mode(gtk::WrapMode::WordChar);
            field.set_height_request(attr(node, "height").parse().unwrap_or(90));
            field.upcast()
        }
        "CheckBox" => {
            let check = gtk::CheckButton::with_label(attr(node, "text"));
            check.set_active(attr(node, "checked") == "true");
            check.upcast()
        }
        "Switch" => {
            let box_widget = gtk::Box::new(Orientation::Horizontal, 8);
            box_widget.append(&gtk::Label::new(Some(attr(node, "text"))));
            let toggle = gtk::Switch::new();
            toggle.set_active(attr(node, "checked") == "true");
            box_widget.append(&toggle);
            box_widget.upcast()
        }
        "Select" | "ListBox" | "RadioGroup" => {
            let list = gtk::ComboBoxText::new();
            for value in attr(node, "items").split('|') {
                list.append_text(value);
            }
            list.set_active(Some(attr(node, "value").parse().unwrap_or(0)));
            list.upcast()
        }
        "Slider" => {
            let slider = gtk::Scale::with_range(Orientation::Horizontal, 0.0, 100.0, 1.0);
            slider.set_value(attr(node, "value").parse().unwrap_or(0.0));
            slider.upcast()
        }
        "ProgressBar" => {
            let bar = gtk::ProgressBar::new();
            let percent = attr(node, "value").parse::<f64>().unwrap_or(0.0);
            bar.set_fraction((percent / 100.0).clamp(0.0, 1.0));
            bar.upcast()
        }
        "Separator" => gtk::Separator::new(Orientation::Horizontal).upcast(),
        "Spinner" => {
            let spinner = gtk::Spinner::new();
            if attr(node, "running") == "true" { spinner.start(); }
            spinner.upcast()
        }
        "Image" => {
            let label = gtk::Label::new(Some("Image"));
            label.add_css_class("dim-label");
            label.upcast()
        }
        "MenuBar" => {
            let menu = gtk::Box::new(Orientation::Horizontal, 12);
            for child in node.children().filter(|n| n.is_element()) {
                menu.append(&gtk::Label::new(child.attribute("text")));
            }
            menu.upcast()
        }
        "Dialog" | "Timer" => return None,
        _ => {
            let frame = gtk::Frame::new(Some(kind));
            frame.set_child(Some(&build_children(node, Orientation::Vertical)));
            frame.upcast()
        }
    };
    if let Ok(width) = attr(node, "preferredWidth").parse::<i32>() {
        w.set_size_request(width.clamp(20, 1200), -1);
    }
    if let Ok(height) = attr(node, "height").parse::<i32>() {
        w.set_size_request(-1, height.clamp(20, 1200));
    }
    w.set_hexpand(true);
    Some(w)
}

pub fn build_preview(xml: &str) -> Result<gtk::Widget, String> {
    let parsed = roxmltree::Document::parse(xml).map_err(|e| e.to_string())?;
    let root = parsed.root_element();
    if root.tag_name().name() != "Window" {
        return Err("Expected a Hydrogen <Window> document".into());
    }
    let frame = gtk::Frame::new(None);
    let content = gtk::Box::new(Orientation::Vertical, 0);
    content.set_size_request(
        attr(root, "width").parse::<i32>().unwrap_or(720).clamp(320, 1400),
        attr(root, "height").parse::<i32>().unwrap_or(480).clamp(240, 1600),
    );
    let title = gtk::HeaderBar::new();
    title.set_title_widget(Some(&gtk::Label::new(Some(
        root.attribute("title").unwrap_or("Hydrogen Window"),
    ))));
    content.append(&title);
    let body = build_children(root, Orientation::Vertical);
    body.set_margin_start(12);
    body.set_margin_end(12);
    body.set_margin_top(12);
    body.set_margin_bottom(12);
    content.append(&body);
    frame.set_child(Some(&content));
    frame.add_css_class("view");
    Ok(frame.upcast())
}
