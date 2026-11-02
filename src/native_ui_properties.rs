//! Lossless attribute editing for Hydrogen's nested <Window> XML.
//! Paths identify element-child indices from the document root, so selecting
//! a deeply nested widget never flattens panels, tabs, event handlers or text.
use roxmltree::{Document, Node};

fn element_at<'a, 'input>(
    document: &'a Document<'input>,
    path: &[usize],
) -> Result<Node<'a, 'input>, String> {
    let mut node = document.root_element();
    for index in path {
        node = node
            .children()
            .filter(|child| child.is_element())
            .nth(*index)
            .ok_or("The selected Hydrogen UI element no longer exists")?;
    }
    Ok(node)
}

pub fn properties(source: &str, path: &[usize]) -> Result<(String, Vec<(String, String)>), String> {
    let parsed = Document::parse(source).map_err(|e| e.to_string())?;
    if parsed.root_element().tag_name().name() != "Window" {
        return Err("Expected a Hydrogen <Window> document".into());
    }
    let node = element_at(&parsed, path)?;
    Ok((
        node.tag_name().name().to_owned(),
        node.attributes()
            .map(|attribute| (attribute.name().to_owned(), attribute.value().to_owned()))
            .collect(),
    ))
}

/// Property metadata mirrors the allowed attributes in Hydrogen's
/// tools/hyui/document.cpp. Values shown here are preview defaults only:
/// they are not inserted into the application's XML until the user applies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyField {
    pub name: String,
    pub value: String,
    pub default: String,
    pub explicitly_set: bool,
    pub group: &'static str,
}

fn supported_fields(kind: &str) -> &'static [&'static str] {
    match kind {
        "Window" => &["version","title","width","height","padding","theme","material","onLoad"],
        "ColumnPanel" => &["gap"],
        "RowPanel" => &["gap","breakpoint"],
        "GridPanel" => &["gap","columns","minItemWidth"],
        "ScrollView" => &["height","gap"],
        "Panel" => &["title","padding","gap"],
        "MenuBar" | "Separator" | "ContextMenu" => &[],
        "Menu" => &["text"],
        "MenuItem" => &["text","onClick","checked","shortcut"],
        "Tabs" => &["value","onChange"],
        "Tab" => &["title","gap"],
        "Label" => &["text","role","tone"],
        "Button" => &["text","onClick","primary"],
        "Input" | "SearchInput" => &["text","placeholder","onChange","onSubmit"],
        "PasswordInput" => &["placeholder","onChange","onSubmit"],
        "FilePicker" => &["mode","text","onPick","onCancel"],
        "TextArea" => &["text","placeholder","onChange","height"],
        "CheckBox" | "Switch" => &["text","checked","onChange"],
        "Select" | "RadioGroup" => &["items","value","onChange"],
        "ListBox" => &["items","value","height","onChange"],
        "Table" => &["columns","rows","value","height","onChange"],
        "Image" => &["source","height"],
        "Dialog" => &["title","text","buttons","open","value","onClose"],
        "Slider" | "NumberInput" => &["value","minimum","maximum","onChange"],
        "ProgressBar" => &["value","minimum","maximum","indeterminate"],
        "Spinner" => &["running"],
        "Spacer" => &["height"],
        "Clipboard" => &["onRead"],
        "State" => &["value"],
        "Timer" => &["interval","running","onTick"],
        _ => &[],
    }
}
const COMMON_FIELDS: &[&str] = &[
    "id","visible","enabled","fill","preferredWidth","accessibleName","tooltip",
];

fn default_value(kind: &str, name: &str) -> &'static str {
    match (kind, name) {
        ("Window", "version") => "1",
        ("Window", "title") => "Hydrogen",
        ("Window", "width") => "1100",
        ("Window", "height") => "820",
        ("Window", "padding") => "24",
        ("Window", "theme") => "system",
        ("Window", "material") => "auto",
        ("Tabs", "value") => "0", // First tab shown to user as page 1.
        ("GridPanel", "columns") => "3",
        ("GridPanel", "minItemWidth") => "240",
        ("RowPanel", "breakpoint") => "700",
        ("Panel", "padding") => "20",
        ("ScrollView", "height") => "200",
        ("TextArea", "height") => "100",
        ("ListBox" | "Table", "height") => "160",
        ("Image", "height") => "72",
        ("Spacer", "height") => "12",
        ("Slider" | "NumberInput" | "ProgressBar", "minimum") => "0",
        ("Slider" | "NumberInput" | "ProgressBar", "maximum") => "100",
        ("Dialog", "buttons") => "OK",
        ("Timer", "interval") => "1000",
        ("Spinner", "running") => "true",
        (_, "visible" | "enabled") => "true",
        (_, "fill" | "checked" | "primary" | "indeterminate") => "false",
        (_, "gap") => "12",
        (_, "value") => "0",
        _ => "",
    }
}

/// All supported properties, including absent attributes that have never
/// been set in the XML. Design-only page selection is deliberately absent:
/// the Tabs.value attribute always represents the built application's start.
pub fn available_properties(
    source: &str,
    path: &[usize],
) -> Result<(String, Vec<PropertyField>), String> {
    let parsed = Document::parse(source).map_err(|e| e.to_string())?;
    if parsed.root_element().tag_name().name() != "Window" {
        return Err("Expected a Hydrogen <Window> document".into());
    }
    let node = element_at(&parsed, path)?;
    let kind = node.tag_name().name();
    let mut fields = Vec::new();
    for (name, group) in supported_fields(kind).iter().map(|s| (*s, "Build / Design"))
        .chain(COMMON_FIELDS.iter().map(|s| (*s, "Common")))
    {
        let explicit = node.attribute(name);
        let default = default_value(kind, name);
        fields.push(PropertyField {
            name: name.to_owned(),
            value: explicit.unwrap_or(default).to_owned(),
            default: default.to_owned(),
            explicitly_set: explicit.is_some(),
            group,
        });
    }
    // Preserve unknown attributes imported from newer Hydrogen runtimes.
    for attr in node.attributes() {
        if !fields.iter().any(|p| p.name == attr.name()) {
            fields.push(PropertyField {
                name: attr.name().to_owned(),
                value: attr.value().to_owned(),
                default: String::new(),
                explicitly_set: true,
                group: "Other",
            });
        }
    }
    Ok((kind.to_owned(), fields))
}

/// An editor-only element navigator. Includes hidden and non-visual
/// elements so every supported property remains reachable without drawing
/// artificial selection buttons over the live application preview.
pub fn element_paths(source: &str) -> Result<Vec<(Vec<usize>, String)>, String> {
    let doc = Document::parse(source).map_err(|e| e.to_string())?;
    let root = doc.root_element();
    if root.tag_name().name() != "Window" {
        return Err("Expected a Hydrogen <Window> document".into());
    }
    fn visit(node: Node<'_, '_>, path: Vec<usize>, depth: usize, result: &mut Vec<(Vec<usize>, String)>) {
        let label = node.attribute("id")
            .or_else(|| node.attribute("title"))
            .or_else(|| node.attribute("text"))
            .unwrap_or("");
        let padding = "  ".repeat(depth.min(12));
        let suffix = if label.is_empty() { String::new() } else { format!(" · {label}") };
        result.push((path.clone(), format!("{padding}{}{suffix}", node.tag_name().name())));
        for (index, child) in node.children().filter(|n| n.is_element()).enumerate() {
            let mut child_path = path.clone();
            child_path.push(index);
            visit(child, child_path, depth + 1, result);
        }
    }
    let mut paths = Vec::new();
    visit(root, Vec::new(), 0, &mut paths);
    Ok(paths)
}

fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | ':' | '.'))
}

fn escaped(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Replace one attribute in its existing start tag, or add it if missing.
/// This edits only that one tag, preserving all other XML bytes verbatim.
pub fn set_attribute(
    source: &str,
    path: &[usize],
    name: &str,
    value: &str,
) -> Result<String, String> {
    if !valid_name(name) || name.starts_with("xmlns") {
        return Err("Invalid or reserved Hydrogen UI attribute name".into());
    }
    let parsed = Document::parse(source).map_err(|e| e.to_string())?;
    if parsed.root_element().tag_name().name() != "Window" {
        return Err("Expected a Hydrogen <Window> document".into());
    }
    let node = element_at(&parsed, path)?;
    let start = node.range().start;
    let bytes = source.as_bytes();
    let mut quote = None;
    let mut end = start;
    while end < bytes.len() {
        let ch = bytes[end];
        match (quote, ch) {
            (None, b'\'') | (None, b'"') => quote = Some(ch),
            (Some(q), c) if q == c => quote = None,
            (None, b'>') => break,
            _ => {}
        }
        end += 1;
    }
    if end >= bytes.len() {
        return Err("Malformed Hydrogen UI opening tag".into());
    }
    let opening = &source[start..=end];
    let escaped = escaped(value);
    let mut replacement = None;
    if let Some(attribute) = node.attributes().find(|a| a.name() == name) {
        let _ = attribute;
        // Search token boundaries instead of matching a suffix such as
        // 'title' within 'subtitle'. Respect both quote styles.
        let mut cursor = 1;
        while cursor < opening.len() {
            let remaining = &opening[cursor..];
            let Some(offset) = remaining.find(name) else {
                break;
            };
            let pos = cursor + offset;
            let before = opening.as_bytes()[pos.saturating_sub(1)];
            let after = opening.as_bytes().get(pos + name.len()).copied();
            if before.is_ascii_whitespace()
                && after.is_some_and(|c| c.is_ascii_whitespace() || c == b'=')
            {
                let mut value_start = pos + name.len();
                while opening
                    .as_bytes()
                    .get(value_start)
                    .is_some_and(|c| c.is_ascii_whitespace())
                {
                    value_start += 1;
                }
                if opening.as_bytes().get(value_start) == Some(&b'=') {
                    value_start += 1;
                    while opening
                        .as_bytes()
                        .get(value_start)
                        .is_some_and(|c| c.is_ascii_whitespace())
                    {
                        value_start += 1;
                    }
                    let delimiter = opening.as_bytes().get(value_start).copied();
                    if matches!(delimiter, Some(b'"' | b'\'')) {
                        let first = value_start + 1;
                        let last = opening[first..]
                            .find(delimiter.unwrap() as char)
                            .ok_or("Unterminated Hydrogen UI attribute")?
                            + first;
                        replacement = Some((first, last));
                        break;
                    }
                }
            }
            cursor = pos + name.len();
        }
    }
    let mut result = source.to_owned();
    if let Some((first, last)) = replacement {
        result.replace_range(start + first..start + last, &escaped);
    } else {
        let point = if end > start && bytes[end - 1] == b'/' {
            end - 1
        } else {
            end
        };
        result.insert_str(point, &format!(" {name}=\"{escaped}\""));
    }
    Document::parse(&result).map_err(|e| e.to_string())?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn updates_nested_control_without_rewriting_siblings_or_events() {
        let original = r#"<Window title="Studio" width="800"><ColumnPanel gap="6"><Button id="go" text="Old" onClick="Build"/><Input id="name" placeholder="Name"/></ColumnPanel></Window>"#;
        let changed = set_attribute(original, &[0, 0], "text", "New & <ready>").unwrap();
        assert!(changed.contains("text=\"New &amp; &lt;ready&gt;\""));
        assert!(changed.contains("onClick=\"Build\""));
        assert!(changed.contains(r#"<Input id="name" placeholder="Name"/>"#));
        assert_eq!(
            properties(&changed, &[0, 0])
                .unwrap()
                .1
                .iter()
                .find(|(key, _)| key == "text")
                .unwrap()
                .1,
            "New & <ready>"
        );
    }
    #[test]
    fn can_change_window_properties_and_add_missing_attributes() {
        let xml = "<Window title='Hello'><Label text='Hi'/></Window>";
        let revised = set_attribute(xml, &[], "width", "900").unwrap();
        let revised = set_attribute(&revised, &[], "title", "Mac's App").unwrap();
        assert!(revised.contains("title='Mac&apos;s App'"));
        assert!(revised.contains("width=\"900\""));
        assert!(set_attribute(xml, &[], "bad name", "x").is_err());
    }
    #[test]
    fn exposes_unset_runtime_and_design_attributes_without_changing_xml() {
        let xml = r#"<Window version="1" title="Studio"><Tabs id="pages"><Tab title="First"/><Tab title="Second"/></Tabs><TextArea id="notes"/></Window>"#;
        let (_, window) = available_properties(xml, &[]).unwrap();
        assert!(window.iter().any(|f| f.name == "theme" && f.value == "system" && !f.explicitly_set));
        let (_, tabs) = available_properties(xml, &[0]).unwrap();
        assert!(tabs.iter().any(|f| f.name == "value" && f.value == "0" && !f.explicitly_set));
        let (_, area) = available_properties(xml, &[1]).unwrap();
        assert!(area.iter().any(|f| f.name == "height" && f.value == "100" && !f.explicitly_set));
        assert!(!xml.contains("theme="));
    }
    #[test]
    fn hierarchy_includes_hidden_controls_and_all_tabs() {
        let xml = r#"<Window><Tabs><Tab title="First"><Button id="a"/></Tab><Tab title="Second"><Label id="hidden" visible="false"/></Tab></Tabs></Window>"#;
        let entries = element_paths(xml).unwrap();
        assert_eq!(entries.len(), 6);
        assert!(entries.iter().any(|(path, label)| path == &[0, 1, 0] && label.contains("hidden")));
        assert!(entries.iter().any(|(_, label)| label.contains("Second")));
    }

    #[test]
    fn explicit_attribute_and_unknown_new_runtime_field_are_retained() {
        let (_, fields) = available_properties("<Window title='App' futureSetting='on'/>", &[]).unwrap();
        assert!(fields.iter().any(|f| f.name == "title" && f.explicitly_set));
        assert!(fields.iter().any(|f| f.name == "futureSetting" && f.group == "Other"));
    }

    #[test]
    fn rejects_stale_selection_without_touching_xml() {
        assert!(set_attribute("<Window/>", &[4], "text", "x").is_err());
    }
}
