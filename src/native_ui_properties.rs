//! Lossless attribute editing for Hydrogen's nested <Window> XML.
//! Paths identify element-child indices from the document root, so selecting
//! a deeply nested widget never flattens panels, tabs, event handlers or text.
use roxmltree::{Document, Node};

pub type ElementPath = Vec<usize>;

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
    fn rejects_stale_selection_without_touching_xml() {
        assert!(set_attribute("<Window/>", &[4], "text", "x").is_err());
    }
}
