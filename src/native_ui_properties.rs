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

/// Collapsible Inspector sections are design-session state, never
/// attributes written to the Hydrogen application XML.
#[derive(Debug, Default, Clone)]
pub struct InspectorSections {
    collapsed: std::collections::HashSet<String>,
}

impl InspectorSections {
    pub fn toggle(&mut self, section: &str) {
        if !self.collapsed.insert(section.to_owned()) {
            self.collapsed.remove(section);
        }
    }

    pub fn is_collapsed(&self, section: &str) -> bool {
        self.collapsed.contains(section)
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
        "visible"
            | "enabled"
            | "fill"
            | "checked"
            | "primary"
            | "indeterminate"
            | "running"
            | "open"
    ) {
        return ["false", "true"]
            .iter()
            .map(|value| (value.to_string(), value.to_string()))
            .collect();
    }
    let named_choices: &[&str] = match (kind, name) {
        ("Window", "theme") => &["system", "light", "dark"],
        ("Window", "material") => &["auto", "glass", "solid"],
        ("Label", "role") => &["title", "heading", "body", "caption"],
        ("Label", "tone") => &["normal", "muted", "accent"],
        ("FilePicker", "mode") => &["open", "save", "folder"],
        _ => &[],
    };
    if !named_choices.is_empty() {
        return named_choices
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

/// Convert user-facing Properties values to the Hydrogen runtime schema.
/// Tabs use human page numbers in Properties, while XML stores zero-based
/// indices. No live editor should write an invalid/partial page value.
pub fn runtime_property_value(kind: &str, name: &str, input: &str) -> Result<String, String> {
    if kind == "Tabs" && name == "value" {
        let page = input
            .trim()
            .parse::<usize>()
            .map_err(|_| "Startup page must be a positive whole number".to_owned())?;
        if page == 0 {
            return Err("Startup page must be 1 or greater".into());
        }
        Ok((page - 1).to_string())
    } else {
        Ok(input.to_owned())
    }
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
        "visible"
            | "enabled"
            | "fill"
            | "checked"
            | "primary"
            | "indeterminate"
            | "running"
            | "open"
    ) && value != "true"
        && value != "false"
    {
        return Err(format!("{name} must be true or false"));
    }
    if kind == "Window" && name == "version" && value != "1" {
        return Err("Hydrogen v1 requires Window version=1".into());
    }
    if kind == "Window" && name == "theme" && !matches!(value, "system" | "light" | "dark") {
        return Err("Window theme must be system, light or dark".into());
    }
    if kind == "Window" && name == "material" && !matches!(value, "auto" | "glass" | "solid") {
        return Err("Window material must be auto, glass or solid".into());
    }
    if kind == "Label" && name == "role"
        && !matches!(value, "title" | "heading" | "body" | "caption")
    {
        return Err("Label role must be title, heading, body or caption".into());
    }
    if kind == "Label" && name == "tone"
        && !matches!(value, "normal" | "muted" | "accent")
    {
        return Err("Label tone must be normal, muted or accent".into());
    }
    if kind == "FilePicker" && name == "mode"
        && !matches!(value, "open" | "save" | "folder")
    {
        return Err("FilePicker mode must be open, save or folder".into());
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
            | "minimum"
            | "maximum"
            | "interval"
    ) || (name == "columns" && kind == "GridPanel")
        || (name == "value" && matches!(kind, "Slider" | "NumberInput" | "ProgressBar"))
    {
        // HYUI v1 uses strictly nonnegative 0..32767 integer attributes.
        // A float can look valid in Quark's preview but fail when Hydrogen
        // parses the built application's layout, so never save it.
        let number = value
            .parse::<u32>()
            .map_err(|_| format!("{name} must be a nonnegative whole number"))?;
        if number > 32767 {
            return Err(format!("{name} must be no greater than 32767"));
        }
        let number = f64::from(number);
        if matches!(
            name,
            "width"
                | "height"
                | "padding"
                | "gap"
                | "breakpoint"
                | "minItemWidth"
                | "preferredWidth"
                | "interval"
        ) && number < 0.0
        {
            return Err(format!("{name} must not be negative"));
        }
        if name == "interval" && kind == "Timer" && number < 100.0 {
            return Err("Timer interval must be at least 100 ms".into());
        }
        if name == "columns" && kind == "GridPanel" && number == 0.0 {
            return Err("GridPanel must contain at least one column".into());
        }
        if matches!(name, "width" | "height" | "minItemWidth") && number == 0.0 {
            return Err(format!("{name} must be positive"));
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
            let choices = node
                .attribute(if kind == "Table" { "rows" } else { "items" })
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
            return Err(format!(
                "{kind} selection {index} is outside the available {count} choices"
            ));
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
    if !supported_fields(node.tag_name().name()).contains(&name)
        && !COMMON_FIELDS.contains(&name)
        && node.attribute(name).is_none()
    {
        return Err(format!(
            "Hydrogen does not support '{name}' on {}.              Position anchors, state colours and custom fonts require a Hydrogen runtime schema update.",
            node.tag_name().name()
        ));
    }
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

/// A native control can be inserted only where Hydrogen's document parser
/// allows visual children. Canvas coordinates do not exist in HYUI v1:
/// order and containment determine responsive placement.
fn visual_container(kind: &str) -> bool {
    matches!(
        kind,
        "Window" | "Panel" | "ColumnPanel" | "RowPanel" | "GridPanel" | "ScrollView" | "Tab"
    )
}

fn tag_for_toolbox(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "Label" => "Label",
        "Button" => "Button",
        "TextField" => "Input",
        "Checkbox" => "CheckBox",
        "TextArea" => "TextArea",
        "RadioButton" => "RadioGroup",
        "ComboBox" => "Select",
        "ListBox" => "ListBox",
        "Slider" => "Slider",
        "ProgressBar" => "ProgressBar",
        "Image" => "Image",
        "Panel" => "Panel",
        "TabControl" => "Tabs",
        "RowPanel" => "RowPanel",
        "ColumnPanel" => "ColumnPanel",
        "GridPanel" => "GridPanel",
        "ScrollView" => "ScrollView",
        _ => return None,
    })
}

fn toolbox_markup(tag: &str, id: &str) -> String {
    let prefix = format!(r#"<{tag} id="{id}""#);
    match tag {
        "Label" => format!(r#"{prefix} text="Label"/>"#),
        "Button" => format!(r#"{prefix} text="Button"/>"#),
        "Input" => format!(r#"{prefix} placeholder="Enter text"/>"#),
        "CheckBox" => format!(r#"{prefix} text="Check Box" checked="false"/>"#),
        "TextArea" => format!(r#"{prefix} height="100" placeholder="Enter text"/>"#),
        "RadioGroup" | "Select" | "ListBox" => {
            format!(r#"{prefix} items="Option 1|Option 2" value="0"/>"#)
        }
        "Slider" => format!(r#"{prefix} minimum="0" maximum="100" value="50"/>"#),
        "ProgressBar" => format!(r#"{prefix} minimum="0" maximum="100" value="0"/>"#),
        // A real asset path must be supplied before a newly added Image can
        // be used at runtime; leave that choice explicit rather than save
        // a non-existent asset and cause a build failure.
        "Image" => format!(r#"{prefix} source="assets/image.png" height="120"/>"#),
        "Panel" => format!(r#"{prefix} title="Panel" padding="20"></Panel>"#),
        "RowPanel" => format!(r#"{prefix} gap="12" breakpoint="700"></RowPanel>"#),
        "ColumnPanel" => format!(r#"{prefix} gap="12"></ColumnPanel>"#),
        "GridPanel" => format!(r#"{prefix} gap="12" columns="2" minItemWidth="240"></GridPanel>"#),
        "ScrollView" => format!(r#"{prefix} height="260"></ScrollView>"#),
        "Tabs" => format!(r#"{prefix} value="0"><Tab title="Page 1"/></Tabs>"#),
        _ => unreachable!("tag_for_toolbox controls the permitted set"),
    }
}

fn tag_end(source: &str, start: usize) -> Result<usize, String> {
    let mut quote = None;
    for (offset, ch) in source[start..].bytes().enumerate() {
        match (quote, ch) {
            (None, b'\'' | b'"') => quote = Some(ch),
            (Some(q), ch) if q == ch => quote = None,
            (None, b'>') => return Ok(start + offset),
            _ => {}
        }
    }
    Err("Unterminated Hydrogen UI start tag".into())
}

fn child_indent(source: &str, start: usize) -> String {
    let line = source[..start].rsplit('\n').next().unwrap_or("");
    let parent_indent = if line.bytes().all(|b| b == b' ' || b == b'\t') {
        line
    } else {
        ""
    };
    format!("{parent_indent}  ")
}

fn resolve_insert_parent(
    doc: &Document<'_>,
    selected: &[usize],
) -> Result<(Vec<usize>, Option<usize>), String> {
    let node = element_at(doc, selected)?;
    if node.tag_name().name() == "Tabs" {
        let first_tab = node
            .children()
            .filter(|child| child.is_element())
            .position(|child| child.has_tag_name("Tab"))
            .ok_or("Tabs requires a Tab page before adding controls")?;
        let mut parent = selected.to_vec();
        parent.push(first_tab);
        return Ok((parent, None));
    }
    if visual_container(node.tag_name().name()) {
        return Ok((selected.to_vec(), None));
    }
    if selected.is_empty() {
        return Err("Choose a Window or layout container to add a control".into());
    }
    let parent = selected[..selected.len() - 1].to_vec();
    if !visual_container(element_at(doc, &parent)?.tag_name().name()) {
        return Err("Select a visual layout container or one of its children".into());
    }
    Ok((parent, Some(*selected.last().unwrap())))
}

/// Insert a toolbox control into Hydrogen's real nested XML, returning the
/// modified document and its new selection. Dropping onto a leaf places the
/// widget after it; dropping onto a container appends inside it.
/// Existing source bytes, event handlers and sibling formatting are retained.
pub fn insert_toolbox_control(
    source: &str,
    selected: &[usize],
    kind: &str,
) -> Result<(String, Vec<usize>), String> {
    let tag = tag_for_toolbox(kind).ok_or("Unsupported native Hydrogen toolbox control")?;
    let doc = Document::parse(source).map_err(|e| e.to_string())?;
    if !doc.root_element().has_tag_name("Window") {
        return Err("Expected a Hydrogen <Window> document".into());
    }
    let (parent_path, after) = resolve_insert_parent(&doc, selected)?;
    let parent = element_at(&doc, &parent_path)?;
    if !visual_container(parent.tag_name().name()) {
        return Err("The selected element cannot contain native UI controls".into());
    }
    let taken = doc
        .descendants()
        .filter_map(|node| node.attribute("id"))
        .collect::<std::collections::HashSet<_>>();
    let id = (1..=10_000)
        .map(|index| format!("{}{}", tag.to_ascii_lowercase(), index))
        .find(|candidate| !taken.contains(candidate.as_str()))
        .ok_or("No unique control ID could be allocated")?;
    let markup = toolbox_markup(tag, &id);
    let open_end = tag_end(source, parent.range().start)?;
    let open = &source[parent.range().start..=open_end];
    let is_empty = open.trim_end_matches('>').trim_end().ends_with('/');
    let indentation = child_indent(source, parent.range().start);
    let mut result = source.to_owned();
    let mut new_path = parent_path;
    if let Some(index) = after {
        let existing = parent
            .children()
            .filter(|child| child.is_element())
            .nth(index)
            .ok_or("Selected sibling disappeared")?;
        let indent = child_indent(source, existing.range().start);
        result.insert_str(existing.range().end, &format!("\n{indent}{markup}"));
        new_path.push(index + 1);
    } else if is_empty {
        let prefix = open.trim_end_matches('>').trim_end_matches('/').trim_end();
        result.replace_range(
            parent.range().start..=open_end,
            &format!(
                "{prefix}>\n{indentation}{markup}\n</{}>",
                parent.tag_name().name()
            ),
        );
        new_path.push(0);
    } else {
        let children_count = parent.children().filter(|child| child.is_element()).count();
        let closing = source[parent.range()]
            .rfind("</")
            .ok_or("Cannot locate Hydrogen container closing tag")?
            + parent.range().start;
        result.insert_str(closing, &format!("\n{indentation}{markup}\n"));
        new_path.push(children_count);
    }
    Document::parse(&result).map_err(|e| e.to_string())?;
    Ok((result, new_path))
}

/// Rearrange controls within the current responsive layout without losing
/// their IDs or events. Reordering changes layout position, never pixel
/// coordinates that HYUI v1 does not support.
pub fn move_sibling(
    source: &str,
    selected: &[usize],
    up: bool,
) -> Result<(String, Vec<usize>), String> {
    let (&index, parent_path) = selected
        .split_last()
        .ok_or("The root Window cannot be moved")?;
    let doc = Document::parse(source).map_err(|e| e.to_string())?;
    let parent = element_at(&doc, parent_path)?;
    let children = parent
        .children()
        .filter(|child| child.is_element())
        .collect::<Vec<_>>();
    let target = if up {
        index
            .checked_sub(1)
            .ok_or("Already at the start of this container")?
    } else {
        index
            .checked_add(1)
            .filter(|i| *i < children.len())
            .ok_or("Already at the end of this container")?
    };
    if index >= children.len() {
        return Err("The selected control no longer exists".into());
    }
    let first = children[index.min(target)].range();
    let second = children[index.max(target)].range();
    let mut result = String::with_capacity(source.len());
    result.push_str(&source[..first.start]);
    result.push_str(&source[second.clone()]);
    result.push_str(&source[first.end..second.start]);
    result.push_str(&source[first]);
    result.push_str(&source[second.end..]);
    Document::parse(&result).map_err(|e| e.to_string())?;
    let mut path = parent_path.to_vec();
    path.push(target);
    Ok((result, path))
}

/// Move a visual control to a different responsive container or place it
/// after another control. The original XML subtree is copied verbatim so
/// event handlers, IDs, child controls, and existing style attributes survive.
#[cfg(any(target_os = "linux", test))]
#[cfg(any(target_os = "linux", test))]
pub fn move_into(
    source: &str,
    from: &[usize],
    target: &[usize],
) -> Result<(String, Vec<usize>), String> {
    if from.is_empty() {
        return Err("Cannot drag the root Hydrogen Window".into());
    }
    if target.starts_with(from) {
        return Err("Cannot move a UI element inside itself or its descendants".into());
    }
    let doc = Document::parse(source).map_err(|e| e.to_string())?;
    let moving = element_at(&doc, from)?;
    let moving_tag = moving.tag_name().name();
    if matches!(
        moving_tag,
        "Tab"
            | "Menu"
            | "MenuItem"
            | "MenuBar"
            | "Toolbar"
            | "ToolButton"
            | "ContextMenu"
            | "Popover"
            | "Clipboard"
            | "Dialog"
            | "Timer"
            | "State"
    ) {
        return Err(
            "Move this special Hydrogen element using the Inspector's sibling buttons".into(),
        );
    }
    let fragment = source[moving.range()].to_owned();
    let removed_parent = &from[..from.len() - 1];
    let removed_index = *from.last().unwrap();
    // Moving preserves IDs and external references, unlike deleting a
    // control. Do not apply remove_control's target-reference rejection.
    let mut removed = source.to_owned();
    removed.replace_range(moving.range(), "");
    let mut destination = target.to_vec();
    if destination.starts_with(removed_parent) && destination.len() > removed_parent.len() {
        let slot = &mut destination[removed_parent.len()];
        if *slot > removed_index {
            *slot -= 1;
        }
    }
    let next_doc = Document::parse(&removed).map_err(|e| e.to_string())?;
    let (parent_path, after) = resolve_insert_parent(&next_doc, &destination)?;
    let parent = element_at(&next_doc, &parent_path)?;
    if !visual_container(parent.tag_name().name()) {
        return Err("Drop the widget inside a responsive Hydrogen container".into());
    }
    let mut result = removed.clone();
    let mut new_path = parent_path;
    let parent_open_end = tag_end(&removed, parent.range().start)?;
    let opening = &removed[parent.range().start..=parent_open_end];
    if let Some(index) = after {
        let selected_sibling = parent
            .children()
            .filter(|child| child.is_element())
            .nth(index)
            .ok_or("Drop target is no longer available")?;
        let indentation = child_indent(&removed, selected_sibling.range().start);
        result.insert_str(
            selected_sibling.range().end,
            &format!("\n{indentation}{fragment}"),
        );
        new_path.push(index + 1);
    } else if opening.trim_end_matches('>').trim_end().ends_with('/') {
        let prefix = opening
            .trim_end_matches('>')
            .trim_end_matches('/')
            .trim_end();
        let indentation = child_indent(&removed, parent.range().start);
        result.replace_range(
            parent.range().start..=parent_open_end,
            &format!(
                "{prefix}>\n{indentation}{fragment}\n</{}>",
                parent.tag_name().name()
            ),
        );
        new_path.push(0);
    } else {
        let children = parent.children().filter(|child| child.is_element()).count();
        let closing = removed[parent.range()]
            .rfind("</")
            .ok_or("Cannot locate target container closing tag")?
            + parent.range().start;
        result.insert_str(
            closing,
            &format!(
                "\n{}{fragment}\n",
                child_indent(&removed, parent.range().start)
            ),
        );
        new_path.push(children);
    }
    Document::parse(&result).map_err(|e| e.to_string())?;
    Ok((result, new_path))
}

/// A structure edit is applied only after native drag/click callbacks finish;
/// this prevents GTK/AppKit/WinUI widgets being unparented during dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StructureEdit {
    Insert {
        selected: Vec<usize>,
        kind: String,
    },
    MoveEarlier {
        selected: Vec<usize>,
    },
    MoveLater {
        selected: Vec<usize>,
    },
    #[cfg(any(target_os = "linux", test))]
    MoveInto {
        selected: Vec<usize>,
        target: Vec<usize>,
    },
    Delete {
        selected: Vec<usize>,
    },
}

pub fn edit_structure(source: &str, edit: &StructureEdit) -> Result<(String, Vec<usize>), String> {
    match edit {
        StructureEdit::Insert { selected, kind } => insert_toolbox_control(source, selected, kind),
        StructureEdit::MoveEarlier { selected } => move_sibling(source, selected, true),
        StructureEdit::MoveLater { selected } => move_sibling(source, selected, false),
        #[cfg(any(target_os = "linux", test))]
        StructureEdit::MoveInto { selected, target } => move_into(source, selected, target),
        StructureEdit::Delete { selected } => {
            let (index, parent) = selected
                .split_last()
                .ok_or("Cannot delete the root Window")?;
            let updated = remove_control(source, selected)?;
            let mut new_path = parent.to_vec();
            if *index > 0 {
                new_path.push(index - 1);
            }
            Ok((updated, new_path))
        }
    }
}

/// Remove a visual node, guarding root, required tab pages and any
/// ContextMenu/Popover target references before the XML can be persisted.
pub fn remove_control(source: &str, selected: &[usize]) -> Result<String, String> {
    let (&index, parent_path) = selected
        .split_last()
        .ok_or("Cannot delete the root Hydrogen Window")?;
    let doc = Document::parse(source).map_err(|e| e.to_string())?;
    let parent = element_at(&doc, parent_path)?;
    let node = element_at(&doc, selected)?;
    let remaining = parent.children().filter(|child| child.is_element()).count();
    if remaining <= 1 && matches!(parent.tag_name().name(), "Tabs" | "Toolbar" | "ContextMenu") {
        return Err("Cannot remove the final required item from this container".into());
    }
    if parent
        .children()
        .filter(|child| child.is_element())
        .nth(index)
        .is_none()
    {
        return Err("Invalid selected control".into());
    }
    let removed_ids = node
        .descendants()
        .filter_map(|descendant| descendant.attribute("id"))
        .collect::<std::collections::HashSet<_>>();
    if doc.descendants().any(|other| {
        !node.range().contains(&other.range().start)
            && other
                .attribute("target")
                .is_some_and(|target| removed_ids.contains(target))
    }) {
        return Err("Cannot delete a control used by a ContextMenu or Popover".into());
    }
    let mut result = source.to_owned();
    result.replace_range(node.range(), "");
    Document::parse(&result).map_err(|e| e.to_string())?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_responsive_layout_containers_are_insertable_and_nested() {
        let xml = r#"<Window version="1" width="900" height="700"/>"#;
        let (row, row_path) = insert_toolbox_control(xml, &[], "RowPanel").unwrap();
        assert_eq!(row_path, [0]);
        assert!(row.contains("breakpoint=\"700\""));
        let (grid, grid_path) = insert_toolbox_control(&row, &row_path, "GridPanel").unwrap();
        assert_eq!(grid_path, [0, 0]);
        assert!(grid.contains("minItemWidth=\"240\""));
        let (body, body_path) = insert_toolbox_control(&grid, &grid_path, "ColumnPanel").unwrap();
        let (changed, child_path) = insert_toolbox_control(&body, &body_path, "Button").unwrap();
        assert_eq!(child_path, [0, 0, 0, 0]);
        assert!(changed.contains("<Button"));
        assert_eq!(properties(&changed, &child_path).unwrap().0, "Button");
    }

    #[test]
    fn responsive_layout_templates_keep_valid_hydrogen_source_after_multiple_inserts() {
        let mut xml = r#"<Window version="1"><Panel/></Window>"#.to_owned();
        for kind in ["RowPanel", "ColumnPanel", "GridPanel", "ScrollView"] {
            xml = insert_toolbox_control(&xml, &[0], kind).unwrap().0;
            Document::parse(&xml).unwrap();
        }
        assert_eq!(element_paths(&xml).unwrap().len(), 6);
        assert!(xml.contains("height=\"260\""));
    }

    #[test]
    fn insert_into_nested_panel_keeps_handlers_and_surrounding_xml() {
        let xml = r#"<Window version="1"><ColumnPanel><Panel title="Settings"><Button id="run" text="Run" onClick="Build"/></Panel></ColumnPanel></Window>"#;
        let (updated, path) = insert_toolbox_control(xml, &[0, 0], "TextField").unwrap();
        assert_eq!(path, [0, 0, 1]);
        assert!(updated.contains(r#"id="run" text="Run" onClick="Build""#));
        assert!(updated.contains(r#"<Input id="input1" placeholder="Enter text"/>"#));
        assert_eq!(properties(&updated, &path).unwrap().0, "Input");
        assert_eq!(element_paths(&updated).unwrap().len(), 5);
    }

    #[test]
    fn insert_after_leaf_and_into_self_closing_containers_preserves_paths() {
        let xml = r#"<Window version="1"><RowPanel gap="16"><Label text="A"/><Label text="B"/></RowPanel><Panel/></Window>"#;
        let (changed, inserted) = insert_toolbox_control(xml, &[0, 0], "Button").unwrap();
        assert_eq!(inserted, [0, 1]);
        assert_eq!(properties(&changed, &[0, 2]).unwrap().0, "Label");
        let (expanded, inner) = insert_toolbox_control(&changed, &[1], "Slider").unwrap();
        assert_eq!(inner, [1, 0]);
        assert_eq!(properties(&expanded, &inner).unwrap().0, "Slider");
        assert!(expanded.contains("<Panel>"));
        assert!(expanded.contains("</Panel>"));
    }

    #[test]
    fn special_controls_generate_valid_responsive_templates() {
        let xml = r#"<Window version="1"><ColumnPanel/></Window>"#;
        let (with_tabs, path) = insert_toolbox_control(xml, &[0], "TabControl").unwrap();
        assert_eq!(path, [0, 0]);
        assert!(with_tabs.contains(r#"<Tab title="Page 1"/>"#));
        let (with_check, nested) = insert_toolbox_control(&with_tabs, &path, "Checkbox").unwrap();
        assert_eq!(nested, [0, 0, 0, 0]);
        assert!(with_check.contains("checked=\"false\""));
        assert_eq!(properties(&with_check, &nested).unwrap().0, "CheckBox");
    }

    #[test]
    fn reorder_nodes_preserves_event_handlers_and_nested_children() {
        let xml = r#"<Window version="1"><ColumnPanel><Button id="a" onClick="Run"/><Panel><Input id="name" onChange="Edit"/></Panel><Label text="End"/></ColumnPanel></Window>"#;
        let (changed, path) = move_sibling(xml, &[0, 1], true).unwrap();
        assert_eq!(path, [0, 0]);
        assert_eq!(properties(&changed, &[0, 1]).unwrap().0, "Button");
        assert_eq!(properties(&changed, &[0, 0, 0]).unwrap().0, "Input");
        assert!(changed.contains("onChange=\"Edit\""));
        assert!(move_sibling(&changed, &[], true).is_err());
        assert!(move_sibling(&changed, &[0, 0], true).is_err());
    }

    #[test]
    fn removal_protects_last_tabs_and_linked_popover_targets() {
        let xml = r#"<Window version="1"><Button id="open"/><Popover id="tip" target="open" text="Tips"/><Tabs value="0"><Tab title="Home"/></Tabs></Window>"#;
        assert!(remove_control(xml, &[0]).is_err());
        assert!(remove_control(xml, &[2, 0]).is_err());
        let changed = remove_control(xml, &[1]).unwrap();
        let changed = remove_control(&changed, &[0]).unwrap();
        assert!(!changed.contains("id=\"open\""));
        assert!(changed.contains("<Tabs"));
        assert!(remove_control(xml, &[]).is_err());
    }

    #[test]
    fn moving_between_panels_preserves_handlers_and_nested_structure() {
        let source = r#"<Window version="1"><RowPanel><Panel title="A"><Button id="run" text="Run" onClick="Build"/></Panel><Panel title="B"/></RowPanel></Window>"#;
        let (updated, selected) = move_into(source, &[0, 0, 0], &[0, 1]).unwrap();
        assert_eq!(selected, [0, 1, 0]);
        assert_eq!(properties(&updated, &selected).unwrap().0, "Button");
        assert!(updated.contains(r#"onClick="Build""#));
        assert_eq!(properties(&updated, &[0, 0]).unwrap().0, "Panel");
        assert!(!updated.contains("title=\"A\"><Button"));
    }

    #[test]
    fn moving_popover_target_keeps_its_stable_id_and_reference() {
        let xml = r#"<Window version="1"><RowPanel><Panel><Button id="action" text="Go"/></Panel><Panel/></RowPanel><Popover id="tip" target="action" text="Details"/></Window>"#;
        let (moved, destination) = move_into(xml, &[0, 0, 0], &[0, 1]).unwrap();
        assert_eq!(destination, [0, 1, 0]);
        assert!(moved.contains("target=\"action\""));
        assert_eq!(properties(&moved, &destination).unwrap().0, "Button");
    }

    #[test]
    fn moving_control_rejects_self_descendants_and_special_hyui_nodes() {
        let source = r#"<Window version="1"><Panel><ColumnPanel><Button id="go" onClick="Go"/></ColumnPanel></Panel><Tabs><Tab title="One"/></Tabs></Window>"#;
        assert!(move_into(source, &[0], &[0, 0]).is_err());
        assert!(move_into(source, &[0, 0], &[0, 0, 0]).is_err());
        assert!(move_into(source, &[], &[0]).is_err());
        assert!(move_into(source, &[1, 0], &[0]).is_err());
        assert_eq!(properties(source, &[0, 0, 0]).unwrap().0, "Button");
    }

    #[test]
    fn structural_edits_preserve_selection_and_support_reordering_and_deletion() {
        let xml = r#"<Window version="1"><ColumnPanel><Button text="A"/><Button text="B"/></ColumnPanel></Window>"#;
        let (added, path) = edit_structure(
            xml,
            &StructureEdit::Insert {
                selected: vec![0],
                kind: "Slider".into(),
            },
        )
        .unwrap();
        assert_eq!(path, [0, 2]);
        let (earlier, path) =
            edit_structure(&added, &StructureEdit::MoveEarlier { selected: path }).unwrap();
        assert_eq!(path, [0, 1]);
        assert_eq!(properties(&earlier, &path).unwrap().0, "Slider");
        let (deleted, selected) =
            edit_structure(&earlier, &StructureEdit::Delete { selected: path }).unwrap();
        assert_eq!(selected, [0, 0]);
        assert!(!deleted.contains("<Slider"));
        assert!(deleted.contains("text=\"B\""));
    }

    #[test]
    fn invalid_structure_edits_return_errors_instead_of_crashing() {
        let xml = r#"<Window version="1"><ColumnPanel><Label text="Only"/></ColumnPanel></Window>"#;
        assert!(edit_structure(
            xml,
            &StructureEdit::MoveEarlier {
                selected: vec![0, 30]
            }
        )
        .is_err());
        assert!(edit_structure(
            xml,
            &StructureEdit::MoveLater {
                selected: vec![0, 30]
            }
        )
        .is_err());
        assert!(edit_structure(xml, &StructureEdit::Delete { selected: vec![] }).is_err());
        assert!(edit_structure(
            xml,
            &StructureEdit::Insert {
                selected: vec![],
                kind: "Unknown".into()
            }
        )
        .is_err());
    }

    #[test]
    fn invalid_drop_locations_and_duplicate_ids_cannot_corrupt_document() {
        let xml = r#"<Window version="1"><ColumnPanel><Button id="button1"/><Button id="button2"/></ColumnPanel></Window>"#;
        let (changed, path) = insert_toolbox_control(xml, &[0], "Button").unwrap();
        assert_eq!(path, [0, 2]);
        assert!(changed.contains("id=\"button3\""));
        assert!(insert_toolbox_control(xml, &[20], "Label").is_err());
        assert!(insert_toolbox_control(xml, &[0], "Window").is_err());
        assert!(move_sibling(xml, &[0, 7], false).is_err());
    }

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
    fn inspector_sections_toggle_independently_and_restore_expanded_state() {
        let mut inspector = InspectorSections::default();
        assert!(!inspector.is_collapsed("Appearance & layout"));
        inspector.toggle("Appearance & layout");
        assert!(inspector.is_collapsed("Appearance & layout"));
        assert!(!inspector.is_collapsed("Events & behaviour"));
        inspector.toggle("Events & behaviour");
        inspector.toggle("Appearance & layout");
        assert!(!inspector.is_collapsed("Appearance & layout"));
        assert!(inspector.is_collapsed("Events & behaviour"));
    }

    #[test]
    fn inspector_folding_never_touches_runtime_xml_or_property_groups() {
        let xml = r#"<Window version="1"><Button id="go" onClick="Go" text="Run" /></Window>"#;
        let (_, fields) = available_properties(xml, &[0]).unwrap();
        let mut inspector = InspectorSections::default();
        for section in fields.iter().map(|field| field.group) {
            inspector.toggle(section);
        }
        assert!(xml.contains("onClick=\"Go\""));
        assert!(fields
            .iter()
            .any(|field| field.group == "Events & behaviour"));
        assert_eq!(properties(xml, &[0]).unwrap().0, "Button");
    }

    #[test]
    fn inspector_exposes_valid_theme_font_role_and_tone_selectors() {
        let source = r#"<Window version="1"><Label role="heading" tone="accent"/></Window>"#;
        let (_, fields) = available_properties(source, &[0]).unwrap();
        assert_eq!(
            property_choices("Label", "role", &fields)
                .iter()
                .map(|(_, value)| value.as_str())
                .collect::<Vec<_>>(),
            ["title", "heading", "body", "caption"]
        );
        assert_eq!(property_choices("Label", "tone", &fields).len(), 3);
        assert!(set_attribute(source, &[0], "role", "caption").is_ok());
    }

    #[test]
    fn inspector_numeric_values_match_hydrogen_v1_integer_schema() {
        let xml = r#"<Window version="1" width="800"><GridPanel columns="2" minItemWidth="240"/><Timer interval="100" onTick="Tick"/></Window>"#;
        assert!(set_attribute(xml, &[], "width", "850").is_ok());
        assert!(set_attribute(xml, &[], "width", "850.5").is_err());
        assert!(set_attribute(xml, &[], "width", "99999").is_err());
        assert!(set_attribute(xml, &[0], "columns", "0").is_err());
        assert!(set_attribute(xml, &[0], "columns", "3").is_ok());
        assert!(set_attribute(xml, &[1], "interval", "99").is_err());
        assert!(set_attribute(xml, &[1], "interval", "250").is_ok());
        assert!(set_attribute(xml, &[], "version", "2").is_err());
    }

    #[test]
    fn inspector_appearance_enums_reject_hydrogen_runtime_invalid_values() {
        let xml = r#"<Window version="1"><Label role="heading" tone="accent"/><FilePicker mode="open" text="Choose" onPick="Pick"/></Window>"#;
        assert!(set_attribute(xml, &[], "material", "glass").is_ok());
        assert!(set_attribute(xml, &[], "material", "unknown").is_err());
        assert!(set_attribute(xml, &[0], "role", "italic").is_err());
        assert!(set_attribute(xml, &[0], "tone", "#FF00FF").is_err());
        assert!(set_attribute(xml, &[1], "mode", "download").is_err());
    }

    #[test]
    fn inspector_does_not_write_unsupported_font_anchor_or_hover_colours() {
        let original = r#"<Window version="1"><Button text="Run"/></Window>"#;
        for field in [
            "anchorLeft",
            "x",
            "y",
            "fontSize",
            "hoverColor",
            "activeBackground",
        ] {
            assert!(
                set_attribute(original, &[0], field, "12").is_err(),
                "{field}"
            );
        }
        // Unknown metadata imported from a future Hydrogen version stays
        // editable without changing unrelated existing attributes.
        let future = r#"<Window version="1"><Button text="Run" newRuntimeValue="old"/></Window>"#;
        assert!(set_attribute(future, &[0], "newRuntimeValue", "new")
            .unwrap()
            .contains("newRuntimeValue=\"new\""));
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
        let xml = r#"<Window title="Test"><Slider minimum="10" maximum="30" value="15"/><CheckBox checked="true"/></Window>"#;
        assert!(set_attribute(xml, &[0], "minimum", "31").is_err());
        assert!(set_attribute(xml, &[0], "maximum", "-11").is_err());
        assert!(set_attribute(xml, &[0], "value", "31").is_err());
        assert!(set_attribute(xml, &[0], "value", "NaN").is_err());
        assert!(set_attribute(xml, &[1], "checked", "maybe").is_err());
        assert!(set_attribute(xml, &[0], "value", "12.5").is_err());
        assert!(set_attribute(xml, &[0], "value", "12")
            .unwrap()
            .contains("value=\"12\""));
        assert!(set_attribute(xml, &[1], "checked", "false").is_ok());
    }

    #[test]
    fn properties_reject_invalid_pages_and_allow_valid_list_choices() {
        let xml = r#"<Window theme="system"><Tabs value="0"><Tab title="First"/><Tab title="Second"/></Tabs><Select items="Red|Green|Blue" value="1"/></Window>"#;
        assert!(set_attribute(xml, &[0], "value", "2").is_err());
        assert!(set_attribute(xml, &[0], "value", "-1").is_err());
        assert!(set_attribute(xml, &[1], "value", "3").is_err());
        assert!(set_attribute(xml, &[], "theme", "banana").is_err());
        assert!(set_attribute(xml, &[0], "value", "1")
            .unwrap()
            .contains("<Tabs value=\"1\""));
        assert!(set_attribute(xml, &[1], "value", "2")
            .unwrap()
            .contains("value=\"2\""));
        assert!(set_attribute(xml, &[], "theme", "dark").is_ok());
    }

    #[test]
    fn property_choice_editor_uses_platform_theme_and_boolean_values() {
        let xml = r#"<Window theme="system"><CheckBox checked="true"/></Window>"#;
        let (_, window) = available_properties(xml, &[]).unwrap();
        assert_eq!(
            property_choices("Window", "theme", &window),
            [
                ("system".into(), "system".into()),
                ("light".into(), "light".into()),
                ("dark".into(), "dark".into())
            ]
        );
        let (_, checkbox) = available_properties(xml, &[0]).unwrap();
        assert_eq!(
            property_choices("CheckBox", "checked", &checkbox),
            [
                ("false".into(), "false".into()),
                ("true".into(), "true".into())
            ]
        );
    }

    #[test]
    fn property_choice_editor_preserves_source_indices_not_option_labels() {
        let xml = r#"<Window><Select items="Apple|Banana|Cherry" value="1"/><Input text="hello"/></Window>"#;
        let (_, fields) = available_properties(xml, &[0]).unwrap();
        assert_eq!(
            property_choices("Select", "value", &fields),
            [
                ("0 · Apple".into(), "0".into()),
                ("1 · Banana".into(), "1".into()),
                ("2 · Cherry".into(), "2".into())
            ]
        );
        assert!(property_choices("Select", "items", &fields).is_empty());
        let (_, input) = available_properties(xml, &[1]).unwrap();
        assert!(property_choices("Input", "text", &input).is_empty());
    }

    #[test]
    fn live_tab_page_edit_is_one_based_but_round_trips_in_xml() {
        let xml =
            r#"<Window><Tabs value="0"><Tab title="Home"/><Tab title="Settings"/></Tabs></Window>"#;
        let runtime = runtime_property_value("Tabs", "value", "2").unwrap();
        assert_eq!(runtime, "1");
        let changed = set_attribute(xml, &[0], "value", &runtime).unwrap();
        assert!(changed.contains("<Tabs value=\"1\""));
        assert!(changed.contains("<Tab title=\"Settings\""));
        let display = available_properties(&changed, &[0])
            .unwrap()
            .1
            .into_iter()
            .find(|field| field.name == "value")
            .unwrap();
        assert_eq!(display.value, "1");
        assert_eq!(runtime_property_value("Tabs", "value", "1").unwrap(), "0");
    }

    #[test]
    fn live_edits_refuse_partial_pages_without_mutating_existing_document() {
        let original = r#"<Window><Tabs value="0"><Tab title="First"/></Tabs><Input text="Old" onChange="Handle"/></Window>"#;
        for invalid in ["", "0", "-1", "abc", "1.5"] {
            assert!(runtime_property_value("Tabs", "value", invalid).is_err());
            assert!(original.contains("value=\"0\""));
        }
        assert_eq!(
            runtime_property_value("Input", "text", "New & <value>").unwrap(),
            "New & <value>"
        );
        let changed = set_attribute(original, &[1], "text", "New & <value>").unwrap();
        assert!(changed.contains("New &amp; &lt;value&gt;"));
        assert!(changed.contains("onChange=\"Handle\""));
    }

    #[test]
    fn rejects_stale_selection_without_touching_xml() {
        assert!(set_attribute("<Window/>", &[4], "text", "x").is_err());
    }
}
