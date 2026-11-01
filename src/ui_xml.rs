//! Hydrogen UI XML interchange for both Quark frontends.
//! .hyyui is the sole supported UI layout format; its contents are XML.
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
    let xml =
        quick_xml::se::to_string(&XmlLayout::from(shared)).map_err(|e| e.to_string())?;
    Ok(format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{xml}\n"))
}

pub fn decode<T: DeserializeOwned>(text: &str) -> Result<T, String> {
    let document = text.trim_start();
    let root = if document.starts_with("<?xml") {
        let end = document.find("?>").ok_or("Invalid XML declaration")?;
        document[end + 2..].trim_start()
    } else {
        document
    };
    if !root.starts_with("<HydrogenUI ") && !root.starts_with("<HydrogenUI>") {
        return Err("Expected a <HydrogenUI> XML root".into());
    }
    let xml: XmlLayout = quick_xml::de::from_str(text).map_err(|e| e.to_string())?;
    let shared = SharedLayout::from(xml);
    serde_json::from_value(serde_json::to_value(shared).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

pub fn load<T: DeserializeOwned>(path: &std::path::Path, text: &str) -> Result<T, String> {
    if path.extension().and_then(|e| e.to_str()) != Some("hyyui") {
        return Err("Expected a Hydrogen .hyyui UI layout".into());
    }
    decode(text)
}

pub fn save<T: Serialize>(path: &std::path::Path, layout: &T) -> Result<String, String> {
    if path.extension().and_then(|e| e.to_str()) != Some("hyyui") {
        return Err("Expected a Hydrogen .hyyui UI layout".into());
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
        assert!(load::<SharedLayout>(Path::new("Form.hyyui"), &xml).is_ok());
        assert!(load::<SharedLayout>(Path::new("Form.layout"), &xml).is_err());
        assert!(save(Path::new("Form.layout"), &layout).is_err());
    }

    #[test]
    fn wrong_xml_and_json_under_hyyui_are_rejected() {
        assert!(decode::<SharedLayout>(r#"{\"version\":1}"#).is_err());
        assert!(decode::<SharedLayout>("<Unexpected />").is_err());
    }
}
