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

#[allow(dead_code)] // Backwards-compatible API for other frontends and unit tests.
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
        "Window" => &[
            "version", "title", "width", "height", "padding", "theme", "material", "onLoad",
        ],
        "ColumnPanel" => &["gap"],
        "RowPanel" => &["gap", "breakpoint"],
        "GridPanel" => &["gap", "columns", "minItemWidth"],
        "ScrollView" => &["height", "gap"],
        "Panel" => &["title", "padding", "gap"],
        "MenuBar" | "Toolbar" | "Separator" => &[],
        "ContextMenu" => &["target"],
        "Popover" => &["target", "text", "open", "onClose"],
        "ToolButton" => &["text", "onClick"],
        "Menu" => &["text"],
        "MenuItem" => &["text", "onClick", "checked", "shortcut"],
        "Tabs" => &["value", "onChange"],
        "Tab" => &["title", "gap"],
        "Label" => &["text", "role", "tone"],
        "Button" => &["text", "onClick", "primary"],
        "Input" | "SearchInput" => &["text", "placeholder", "onChange", "onSubmit"],
        "PasswordInput" => &["placeholder", "onChange", "onSubmit"],
        "FilePicker" => &["mode", "text", "onPick", "onCancel"],
        "TextArea" => &["text", "placeholder", "onChange", "height"],
        "CheckBox" | "Switch" => &["text", "checked", "onChange"],
        "Select" | "RadioGroup" => &["items", "value", "onChange"],
        "ListBox" => &["items", "value", "height", "onChange"],
        "Table" => &["columns", "rows", "value", "height", "onChange"],
        "Image" => &["source", "height"],
        "Dialog" => &["title", "text", "buttons", "open", "value", "onClose"],
        "Slider" | "NumberInput" => &["value", "minimum", "maximum", "onChange"],
        "ProgressBar" => &["value", "minimum", "maximum", "indeterminate"],
        "Spinner" => &["running"],
        "Spacer" => &["height"],
        "Clipboard" => &["onRead"],
        "State" => &["value"],
        "Timer" => &["interval", "running", "onTick"],
        _ => &[],
    }
}
const COMMON_FIELDS: &[&str] = &[
    "id",
    "visible",
    "enabled",
    "fill",
    "preferredWidth",
    "accessibleName",
    "tooltip",
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
        ("Popover", "open") => "false",
        (_, "visible" | "enabled") => "true",
        (_, "fill" | "checked" | "primary" | "indeterminate") => "false",
        (_, "gap") => "12",
        (_, "value") => "0",
        _ => "",
    }
}

/// Keep design-time presentation separate from the application's runtime
/// defaults. Every supported XML attribute is editable even if it is absent.
fn property_group(kind: &str, name: &str) -> &'static str {
    if kind == "Tabs" && name == "value" {
        return "Application defaults";
    }
    if name.starts_with("on") {
        return "Events & behaviour";
    }
    if matches!(
        name,
        "text"
            | "title"
            | "placeholder"
            | "source"
            | "items"
            | "rows"
            | "columns"
            | "accessibleName"
    ) {
        return "Content & accessibility";
    }
    if matches!(
        name,
        "visible"
            | "enabled"
            | "checked"
            | "running"
            | "primary"
            | "indeterminate"
            | "open"
            | "value"
    ) {
        return "State & behaviour";
    }
    if matches!(
        name,
        "width"
            | "height"
            | "padding"
            | "gap"
            | "breakpoint"
            | "minItemWidth"
            | "preferredWidth"
            | "fill"
            | "theme"
            | "material"
            | "role"
            | "tone"
    ) {
        return "Appearance & layout";
    }
    "Other"
}

fn property_order(group: &str) -> usize {
    match group {
        "Application defaults" => 0,
        "Content & accessibility" => 1,
        "Appearance & layout" => 2,
        "State & behaviour" => 3,
        "Events & behaviour" => 4,
        _ => 5,
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
    for name in supported_fields(kind)
        .iter()
        .copied()
        .chain(COMMON_FIELDS.iter().copied())
    {
        let group = property_group(kind, name);
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
    fields.sort_by_key(|field| property_order(field.group));
    Ok((kind.to_owned(), fields))
}

/// Design-time page choices are independent of the Tabs.value attribute,
/// which always describes the application's persisted startup page.
pub fn design_tab_pages(source: &str, path: &[usize]) -> Result<Vec<String>, String> {
    let doc = Document::parse(source).map_err(|e| e.to_string())?;
    if doc.root_element().tag_name().name() != "Window" {
        return Err("Expected a Hydrogen <Window> document".into());
    }
    let tabs = element_at(&doc, path)?;
    if tabs.tag_name().name() != "Tabs" {
        return Err("Design page selection requires a <Tabs> element".into());
    }
    Ok(tabs
        .children()
        .filter(|child| child.is_element() && child.tag_name().name() == "Tab")
        .enumerate()
        .map(|(index, child)| {
            let title = child.attribute("title").unwrap_or("").trim();
            if title.is_empty() {
                format!("Page {}", index + 1)
            } else {
                format!("{} · {}", index + 1, title)
            }
        })
        .collect())
}

/// Suggested native Properties editors for finite-value fields.
/// Store the original XML value separately from its human-readable label.
pub fn property_choices(kind: &str, name: &str, fields: &[PropertyField]) -> Vec<(String, String)> {
    if matches!(
        name,
        "visible" | "enabled" | "fill" | "checked" | "primary" | "indeterminate"
            | "running" | "open"
    ) {
        return ["false", "true"]
            .iter()
            .map(|value| (value.to_string(), value.to_string()))
            .collect();
    }
    if kind == "Window" && name == "theme" {
        return ["system", "light", "dark"]
            .iter()
            .map(|value| (value.to_string(), value.to_string()))
            .collect();
    }
    if name == "value" && matches!(kind, "Select" | "RadioGroup" | "ListBox") {
        if let Some(items) = fields
            .iter()
            .find(|field| field.name == "items")
            .map(|field| field.value.as_str())
            .filter(|value| !value.is_empty())
        {
            return items
                .split('|')
                .enumerate()
                .map(|(index, label)| (format!("{index} · {label}"), index.to_string()))
                .collect();
        }
    }
    Vec::new()
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
    fn visit(
        node: Node<'_, '_>,
        path: Vec<usize>,
        depth: usize,
        result: &mut Vec<(Vec<usize>, String)>,
    ) {
        let label = node
            .attribute("id")
            .or_else(|| node.attribute("title"))
            .or_else(|| node.attribute("text"))
            .unwrap_or("");
        let padding = "  ".repeat(depth.min(12));
        let suffix = if label.is_empty() {
            String::new()
        } else {
            format!(" · {label}")
        };
        result.push((
            path.clone(),
            format!("{padding}{}{suffix}", node.tag_name().name()),
        ));
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

/// Reject invalid values before mutating the XML. Unknown future Hydrogen
/// attributes remain losslessly editable, but known native controls should
/// never accept impossible ranges, pages or bool strings.
fn validate_known_property(node: Node<'_, '_>, name: &str, value: &str) -> Result<(), String> {
    let kind = node.tag_name().name();
    if !supported_fields(kind).contains(&name) && !COMMON_FIELDS.contains(&name) {
        return Ok(());
    }
    if matches!(
        name,
        "visible" | "enabled" | "fill" | "checked" | "primary" | "indeterminate" | "running" | "open"
    ) && value != "true"
        && value != "false"
    {
        return Err(format!("{name} must be true or false"));
    }
    if name == "theme" && kind == "Window" && !matches!(value, "system" | "light" | "dark") {
        return Err("Window theme must be system, light or dark".into());
    }
    if matches!(
        name,
        "width" | "height" | "padding" | "gap" | "breakpoint" | "minItemWidth" | "preferredWidth"
            | "minimum" | "maximum" | "interval"
    ) || (name == "value" && matches!(kind, "Slider" | "NumberInput" | "ProgressBar"))
    {
        let number = value
            .parse::<f64>()
            .map_err(|_| format!("{name} must be a number"))?;
        if !number.is_finite() {
            return Err(format!("{name} must be a finite number"));
        }
        if matches!(name, "width" | "height" | "padding" | "gap" | "breakpoint"
            | "minItemWidth" | "preferredWidth" | "interval")
            && number < 0.0
        {
            return Err(format!("{name} must not be negative"));
        }
        if name == "width" && kind == "Window" && number < 200.0 {
            return Err("Window width must be at least 200".into());
        }
        if name == "height" && kind == "Window" && number < 200.0 {
            return Err("Window height must be at least 200".into());
        }
        if matches!(kind, "Slider" | "NumberInput" | "ProgressBar")
            && matches!(name, "minimum" | "maximum" | "value")
        {
            let min = if name == "minimum" {
                number
            } else {
                node.attribute("minimum")
                    .and_then(|n| n.parse::<f64>().ok())
                    .unwrap_or(0.0)
            };
            let max = if name == "maximum" {
                number
            } else {
                node.attribute("maximum")
                    .and_then(|n| n.parse::<f64>().ok())
                    .unwrap_or(100.0)
            };
            if !min.is_finite() || !max.is_finite() || max <= min {
                return Err("maximum must be greater than minimum".into());
            }
            if name == "value" && !(min..=max).contains(&number) {
                return Err(format!("value must be between {min} and {max}"));
            }
        }
    }
    if name == "value" && matches!(kind, "Tabs" | "Select" | "RadioGroup" | "ListBox" | "Table") {
        let index = value
            .parse::<usize>()
            .map_err(|_| format!("{kind} selection must be a zero-based, whole-number index"))?;
        let count = if kind == "Tabs" {
            node.children()
                .filter(|child| child.is_element() && child.tag_name().name() == "Tab")
                .count()
        } else {
            let choices = node.attribute(if kind == "Table" { "rows" } else { "items" })
                .unwrap_or("");
            if choices.is_empty() {
                0
            } else if kind == "Table" {
                choices.lines().count()
            } else {
                choices.split('|').count()
            }
        };
        if count > 0 && index >= count || count == 0 && index != 0 {
            return Err(format!("{kind} selection {index} is outside the available {count} choices"));
        }
    }
    Ok(())
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
    validate_known_property(node, name, value)?;
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
        assert!(window
            .iter()
            .any(|f| f.name == "theme" && f.value == "system" && !f.explicitly_set));
        let (_, tabs) = available_properties(xml, &[0]).unwrap();
        assert!(tabs
            .iter()
            .any(|f| f.name == "value" && f.value == "0" && !f.explicitly_set));
        let (_, area) = available_properties(xml, &[1]).unwrap();
        assert!(area
            .iter()
            .any(|f| f.name == "height" && f.value == "100" && !f.explicitly_set));
        assert!(!xml.contains("theme="));
    }
    #[test]
    fn runtime_popovers_toolbars_and_defaults_are_editable_before_being_set() {
        let xml = r#"<Window version="1"><Toolbar><ToolButton id="run"/></Toolbar><Popover target="run"/></Window>"#;
        let (_, button) = available_properties(xml, &[0, 0]).unwrap();
        assert!(button
            .iter()
            .any(|field| field.name == "text" && !field.explicitly_set));
        assert!(button
            .iter()
            .any(|field| field.name == "onClick" && field.group == "Events & behaviour"));
        let (_, popover) = available_properties(xml, &[1]).unwrap();
        assert!(popover
            .iter()
            .any(|field| field.name == "open" && field.value == "false"));
        assert!(popover.iter().any(|field| field.name == "onClose"));
    }

    #[test]
    fn hierarchy_includes_hidden_controls_and_all_tabs() {
        let xml = r#"<Window><Tabs><Tab title="First"><Button id="a"/></Tab><Tab title="Second"><Label id="hidden" visible="false"/></Tab></Tabs></Window>"#;
        let entries = element_paths(xml).unwrap();
        assert_eq!(entries.len(), 6);
        assert!(entries
            .iter()
            .any(|(path, label)| path == &[0, 1, 0] && label.contains("hidden")));
        assert!(entries.iter().any(|(_, label)| label.contains("Second")));
    }

    #[test]
    fn explicit_attribute_and_unknown_new_runtime_field_are_retained() {
        let (_, fields) =
            available_properties("<Window title='App' futureSetting='on'/>", &[]).unwrap();
        assert!(fields.iter().any(|f| f.name == "title" && f.explicitly_set));
        assert!(fields
            .iter()
            .any(|f| f.name == "futureSetting" && f.group == "Other"));
    }

    #[test]
    fn design_page_choices_follow_nested_tab_titles_without_modifying_startup_page() {
        let xml = r#"<Window><ColumnPanel><Tabs value="0"><Tab title="Home"/><Separator/><Tab title="Settings"/><Tab><Label text="Hi"/></Tab></Tabs></ColumnPanel></Window>"#;
        assert_eq!(
            design_tab_pages(xml, &[0, 0]).unwrap(),
            ["1 · Home", "2 · Settings", "Page 3"]
        );
        assert!(xml.contains("value=\"0\""));
    }

    #[test]
    fn design_page_selector_rejects_non_tab_and_stale_paths() {
        let xml = r#"<Window><Tabs><Tab title="First"/></Tabs><Button/></Window>"#;
        assert_eq!(design_tab_pages(xml, &[0]).unwrap(), ["1 · First"]);
        assert!(design_tab_pages(xml, &[]).is_err());
        assert!(design_tab_pages(xml, &[1]).is_err());
        assert!(design_tab_pages(xml, &[0, 4]).is_err());
    }

    #[test]
    fn properties_validate_native_control_ranges_and_booleans() {
        let xml = r#"<Window title="Test"><Slider minimum="-10" maximum="30" value="5"/><CheckBox checked="true"/></Window>"#;
        assert!(set_attribute(xml, &[0], "minimum", "31").is_err());
        assert!(set_attribute(xml, &[0], "maximum", "-11").is_err());
        assert!(set_attribute(xml, &[0], "value", "31").is_err());
        assert!(set_attribute(xml, &[0], "value", "NaN").is_err());
        assert!(set_attribute(xml, &[1], "checked", "maybe").is_err());
        assert!(set_attribute(xml, &[0], "value", "12.5").unwrap().contains("value=\"12.5\""));
        assert!(set_attribute(xml, &[1], "checked", "false").is_ok());
    }

    #[test]
    fn properties_reject_invalid_pages_and_allow_valid_list_choices() {
        let xml = r#"<Window theme="system"><Tabs value="0"><Tab title="First"/><Tab title="Second"/></Tabs><Select items="Red|Green|Blue" value="1"/></Window>"#;
        assert!(set_attribute(xml, &[0], "value", "2").is_err());
        assert!(set_attribute(xml, &[0], "value", "-1").is_err());
        assert!(set_attribute(xml, &[1], "value", "3").is_err());
        assert!(set_attribute(xml, &[], "theme", "banana").is_err());
        assert!(set_attribute(xml, &[0], "value", "1").unwrap().contains("<Tabs value=\"1\""));
        assert!(set_attribute(xml, &[1], "value", "2").unwrap().contains("value=\"2\""));
        assert!(set_attribute(xml, &[], "theme", "dark").is_ok());
    }

    #[test]
    fn property_choice_editor_uses_platform_theme_and_boolean_values() {
        let xml = r#"<Window theme="system"><CheckBox checked="true"/></Window>"#;
        let (_, window) = available_properties(xml, &[]).unwrap();
        assert_eq!(
            property_choices("Window", "theme", &window),
            [("system".into(), "system".into()), ("light".into(), "light".into()), ("dark".into(), "dark".into())]
        );
        let (_, checkbox) = available_properties(xml, &[0]).unwrap();
        assert_eq!(
            property_choices("CheckBox", "checked", &checkbox),
            [("false".into(), "false".into()), ("true".into(), "true".into())]
        );
    }

    #[test]
    fn property_choice_editor_preserves_source_indices_not_option_labels() {
        let xml = r#"<Window><Select items="Apple|Banana|Cherry" value="1"/><Input text="hello"/></Window>"#;
        let (_, fields) = available_properties(xml, &[0]).unwrap();
        assert_eq!(
            property_choices("Select", "value", &fields),
            [("0 · Apple".into(), "0".into()), ("1 · Banana".into(), "1".into()), ("2 · Cherry".into(), "2".into())]
        );
        assert!(property_choices("Select", "items", &fields).is_empty());
        let (_, input) = available_properties(xml, &[1]).unwrap();
        assert!(property_choices("Input", "text", &input).is_empty());
    }

    #[test]
    fn rejects_stale_selection_without_touching_xml() {
        assert!(set_attribute("<Window/>", &[4], "text", "x").is_err());
    }
}
