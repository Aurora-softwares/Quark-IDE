//! Hydrogen UI XML interchange for both Quark frontends.
//! .hyui is the sole supported UI layout format; its contents are XML.
use serde::{de::DeserializeOwned, Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct SharedControl {
    id: u64,
    kind: String,
    name: String,
    text: String,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

#[derive(Debug, Serialize, Deserialize)]
struct SharedLayout {
    version: u32,
    title: String,
    width: f32,
    height: f32,
    controls: Vec<SharedControl>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename = "HydrogenUI")]
struct XmlLayout {
    #[serde(rename = "@version")]
    version: u32,
    #[serde(rename = "@title")]
    title: String,
    #[serde(rename = "@width")]
    width: f32,
    #[serde(rename = "@height")]
    height: f32,
    #[serde(rename = "Control", default)]
    controls: Vec<XmlControl>,
}

#[derive(Debug, Serialize, Deserialize)]
struct XmlControl {
    #[serde(rename = "@id")]
    id: u64,
    #[serde(rename = "@kind")]
    kind: String,
    #[serde(rename = "@name")]
    name: String,
    #[serde(rename = "@text")]
    text: String,
    #[serde(rename = "@x")]
    x: f32,
    #[serde(rename = "@y")]
    y: f32,
    #[serde(rename = "@width")]
    width: f32,
    #[serde(rename = "@height")]
    height: f32,
}

impl From<SharedControl> for XmlControl {
    fn from(c: SharedControl) -> Self {
        Self {
            id: c.id,
            kind: c.kind,
            name: c.name,
            text: c.text,
            x: c.x,
            y: c.y,
            width: c.width,
            height: c.height,
        }
    }
}
impl From<XmlControl> for SharedControl {
    fn from(c: XmlControl) -> Self {
        Self {
            id: c.id,
            kind: c.kind,
            name: c.name,
            text: c.text,
            x: c.x,
            y: c.y,
            width: c.width,
            height: c.height,
        }
    }
}
impl From<SharedLayout> for XmlLayout {
    fn from(layout: SharedLayout) -> Self {
        Self {
            version: layout.version,
            title: layout.title,
            width: layout.width,
            height: layout.height,
            controls: layout.controls.into_iter().map(Into::into).collect(),
        }
    }
}
impl From<XmlLayout> for SharedLayout {
    fn from(layout: XmlLayout) -> Self {
        Self {
            version: layout.version,
            title: layout.title,
            width: layout.width,
            height: layout.height,
            controls: layout.controls.into_iter().map(Into::into).collect(),
        }
    }
}

pub fn encode<T: Serialize>(layout: &T) -> Result<String, String> {
    let shared: SharedLayout =
        serde_json::from_value(serde_json::to_value(layout).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let xml = quick_xml::se::to_string(&XmlLayout::from(shared)).map_err(|e| e.to_string())?;
    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{xml}\n"
    ))
}

/// Hydrogen's real runtime UI format is a nested <Window> tree, not the
/// former flat Quark designer schema. Read it without removing any nodes,
/// events, attributes or layout containers from the source document.
/// This converts only a *preview projection* into editable Layout metadata.
pub fn is_native_window(text: &str) -> bool {
    roxmltree::Document::parse(text)
        .ok()
        .is_some_and(|doc| doc.root_element().tag_name().name() == "Window")
}

fn hydrogen_window_projection(text: &str) -> Result<SharedLayout, String> {
    let document = roxmltree::Document::parse(text).map_err(|e| e.to_string())?;
    let window = document.root_element();
    if window.tag_name().name() != "Window" {
        return Err("Expected Hydrogen <Window> UI XML".into());
    }
    let attr_float = |key: &str, fallback: f32| -> f32 {
        window
            .attribute(key)
            .and_then(|s| s.parse::<f32>().ok())
            .filter(|n| n.is_finite())
            .unwrap_or(fallback)
    };
    let width = attr_float("width", 720.0);
    let height = attr_float("height", 480.0);
    if !(320.0..=4096.0).contains(&width) || !(240.0..=4096.0).contains(&height) {
        return Err("Hydrogen window dimensions are outside designer limits".into());
    }

    let mut controls = Vec::new();
    let kinds: &[(&str, &str)] = &[
        ("Label", "label"),
        ("Button", "button"),
        ("Input", "text_field"),
        ("SearchInput", "text_field"),
        ("NumberInput", "text_field"),
        ("CheckBox", "checkbox"),
        ("Switch", "checkbox"),
        ("TextArea", "text_area"),
        ("RadioGroup", "radio_button"),
        ("Select", "combo_box"),
        ("ListBox", "list_box"),
        ("Table", "list_box"),
        ("Slider", "slider"),
        ("ProgressBar", "progress_bar"),
        ("Image", "image"),
        ("Tabs", "tab_control"),
        ("Panel", "panel"),
    ];

    for node in window.descendants().filter(|n| n.is_element()) {
        if node == window || controls.len() >= 500 {
            continue;
        }
        // A tab control has multiple pages in the document but only one is
        // visible at runtime. The preview defaults to its selected page.
        let hidden_tab = node
            .ancestors()
            .find(|ancestor| ancestor.is_element() && ancestor.tag_name().name() == "Tab")
            .and_then(|tab| {
                let tabs = tab.parent_element()?;
                if tabs.tag_name().name() != "Tabs" {
                    return None;
                }
                let selected = tabs
                    .attribute("value")
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(0);
                let index = tabs
                    .children()
                    .filter(|n| n.is_element() && n.tag_name().name() == "Tab")
                    .position(|candidate| candidate == tab)?;
                Some(index != selected)
            })
            .unwrap_or(false);
        if hidden_tab || node.attribute("visible") == Some("false") {
            continue;
        }
        let Some((_, kind)) = kinds.iter().find(|(tag, _)| *tag == node.tag_name().name()) else {
            continue;
        };
        let number = controls.len();
        let cols = 3_usize;
        let col_width = ((width - 36.0) / cols as f32).max(80.0);
        let x = 12.0 + (number % cols) as f32 * col_width;
        let y = 12.0 + (number / cols) as f32 * 43.0;
        let available_width = (width - x - 10.0).max(20.0);
        let available_height = (height - y - 5.0).max(20.0);
        // Long responsive forms may extend beyond the first viewport.
        // Do not corrupt the source; preview rows wrap in the designer.
        let y = y.min((height - 40.0).max(0.0));
        let suggested_width: f32 = match *kind {
            "label" | "checkbox" | "radio_button" => 170.0,
            "slider" | "progress_bar" | "text_field" => 210.0,
            _ => 150.0,
        };
        let w = suggested_width
            .min(col_width - 8.0)
            .min(available_width)
            .max(20.0);
        let h = if *kind == "progress_bar" {
            24.0
        } else {
            32.0_f32.min(available_height)
        };
        let name = node.attribute("id").unwrap_or(node.tag_name().name());
        let text = node
            .attribute("text")
            .or_else(|| node.attribute("title"))
            .or_else(|| node.attribute("placeholder"))
            .unwrap_or(name);
        controls.push(SharedControl {
            id: (number + 1) as u64,
            kind: (*kind).to_owned(),
            name: name.chars().take(128).collect(),
            text: text.chars().take(4096).collect(),
            x,
            y,
            width: w,
            height: h.max(20.0),
        });
    }
    Ok(SharedLayout {
        version: window
            .attribute("version")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1),
        title: window
            .attribute("title")
            .unwrap_or("Hydrogen Window")
            .to_owned(),
        width,
        height,
        controls,
    })
}

pub fn decode<T: DeserializeOwned>(text: &str) -> Result<T, String> {
    let document = text.trim_start();
    let root = if document.starts_with("<?xml") {
        let end = document.find("?>").ok_or("Invalid XML declaration")?;
        document[end + 2..].trim_start()
    } else {
        document
    };
    if root.starts_with("<Window ") || root.starts_with("<Window>") {
        let projected = hydrogen_window_projection(text)?;
        return serde_json::from_value(serde_json::to_value(projected).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string());
    }
    if !root.starts_with("<HydrogenUI ") && !root.starts_with("<HydrogenUI>") {
        return Err("Expected Hydrogen <Window> or Quark <HydrogenUI> XML root".into());
    }
    let xml: XmlLayout = quick_xml::de::from_str(text).map_err(|e| e.to_string())?;
    let shared = SharedLayout::from(xml);
    serde_json::from_value(serde_json::to_value(shared).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

pub fn load<T: DeserializeOwned>(path: &std::path::Path, text: &str) -> Result<T, String> {
    if !matches!(path.extension().and_then(|e| e.to_str()), Some("hyui")) {
        return Err("Expected a Hydrogen .hyui UI layout".into());
    }
    decode(text)
}

pub fn save<T: Serialize>(path: &std::path::Path, layout: &T) -> Result<String, String> {
    if !matches!(path.extension().and_then(|e| e.to_str()), Some("hyui")) {
        return Err("Expected a Hydrogen .hyui UI layout".into());
    }
    encode(layout)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn xml_round_trip_preserves_controls_and_escaping() {
        let input = SharedLayout {
            version: 1,
            title: "Hello <Hydrogen> & Quark".into(),
            width: 720.0,
            height: 480.0,
            controls: vec![SharedControl {
                id: 1,
                kind: "button".into(),
                name: "launch".into(),
                text: "Save & close \"now\"".into(),
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 30.0,
            }],
        };
        let xml = encode(&input).unwrap();
        assert!(xml.contains("<HydrogenUI"));
        assert!(xml.contains("&amp;"));
        let decoded: SharedLayout = decode(&xml).unwrap();
        assert_eq!(decoded.title, input.title);
        assert_eq!(decoded.controls[0].text, input.controls[0].text);
    }
    #[test]
    fn only_hydrogen_ui_extension_is_accepted() {
        use std::path::Path;
        let layout = SharedLayout {
            version: 1,
            title: "Test".into(),
            width: 720.0,
            height: 480.0,
            controls: Vec::new(),
        };
        let xml = encode(&layout).unwrap();
        assert!(load::<SharedLayout>(Path::new("Form.hyui"), &xml).is_ok());
        assert!(load::<SharedLayout>(Path::new("Form.hyyui"), &xml).is_err());
        assert!(save(Path::new("Form.hyyui"), &layout).is_err());
        assert!(load::<SharedLayout>(Path::new("Form.layout"), &xml).is_err());
        assert!(save(Path::new("Form.layout"), &layout).is_err());
    }

    #[test]
    fn native_hydrogen_window_with_nested_tabs_events_and_responsive_panels_opens() {
        let source = r#"<?xml version="1.0" encoding="UTF-8"?>
<Window version="1" id="studio" title="Hydrogen Studio" width="1120" height="900" theme="system">
    <MenuBar><Menu text="Studio"><MenuItem onClick="ShowWorkspace" /></Menu></MenuBar>
    <Tabs id="pages" value="0" onChange="PageChanged">
      <Tab id="workspace" title="Workspace">
        <ColumnPanel gap="6">
          <Label text="Build something native." />
          <Panel title="Settings"><Input id="name" text="Aurora &amp; Hydrogen" onChange="Preview" />
          <Button id="build" text="Build" onClick="Build" /></Panel>
        </ColumnPanel>
      </Tab>
      <Tab title="Gallery"><Button id="hidden" text="Second page" /></Tab>
    </Tabs>
    <Dialog id="confirm" text="Continue?" onClose="Confirmed"/>
</Window>"#;
        assert!(is_native_window(source));
        let parsed: SharedLayout = decode(source).unwrap();
        assert_eq!(parsed.title, "Hydrogen Studio");
        assert_eq!(parsed.width, 1120.0);
        assert!(parsed.controls.iter().any(|c| c.name == "build"));
        assert!(parsed
            .controls
            .iter()
            .any(|c| c.text == "Aurora & Hydrogen"));
        assert!(!parsed.controls.iter().any(|c| c.name == "hidden"));
    }

    #[test]
    fn actual_hydrogen_studio_sample_loads_without_converting_its_xml() {
        let xml = include_str!("../tests/fixtures/Studio.hyui");
        assert!(is_native_window(xml));
        let layout: SharedLayout = load(std::path::Path::new("Studio.hyui"), xml).unwrap();
        assert_eq!(layout.title, "Hydrogen Studio");
        assert_eq!((layout.width, layout.height), (1120.0, 900.0));
        assert!(layout.controls.iter().any(|c| c.name == "build"));
        assert!(layout.controls.iter().any(|c| c.name == "projectName"));
        assert!(!layout.controls.iter().any(|c| c.name == "showDialog"));
        // The source, including onClick/onChange handlers and nested Tabs,
        // is retained by LayoutDocument instead of flattened on save.
        assert!(xml.contains("onClick=\"Build\""));
        assert!(xml.contains("<GridPanel"));
    }

    #[test]
    fn wrong_xml_and_json_under_hyui_are_rejected() {
        assert!(decode::<SharedLayout>(r#"{\"version\":1}"#).is_err());
        assert!(decode::<SharedLayout>("<Unexpected />").is_err());
    }
}
