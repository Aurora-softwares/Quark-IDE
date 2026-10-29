//! Versioned themes cover surfaces, icons, syntax, and terminal colours.
use eframe::egui::{self, Color32, Stroke};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rgb(pub [u8; 3]);
impl Rgb {
    pub fn color(self) -> Color32 {
        Color32::from_rgb(self.0[0], self.0[1], self.0[2])
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IconStyle {
    Outline,
    Rounded,
    Solid,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SyntaxPalette {
    pub keyword: Rgb,
    pub modifier: Rgb,
    pub type_name: Rgb,
    pub literal: Rgb,
    pub comment: Rgb,
    pub string: Rgb,
    pub number: Rgb,
    pub operator: Rgb,
    pub punctuation: Rgb,
    pub declaration: Rgb,
    pub function: Rgb,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Palette {
    pub editor: Rgb,
    pub sidebar: Rgb,
    pub surface: Rgb,
    pub text: Rgb,
    pub muted: Rgb,
    pub accent: Rgb,
    pub border: Rgb,
    pub selection: Rgb,
    pub status: Rgb,
    pub status_text: Rgb,
    pub error: Rgb,
    pub warning: Rgb,
    pub icon: Rgb,
    pub terminal: Rgb,
    pub terminal_text: Rgb,
    pub ansi: [Rgb; 16],
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Definition {
    pub version: u32,
    pub name: String,
    pub dark: bool,
    pub icon_style: IconStyle,
    pub icon_scale: u8,
    pub colors: Palette,
    pub syntax: SyntaxPalette,
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Preset {
    #[default]
    Graphite,
    Aurora,
    Dawn,
    Contrast,
}
impl Preset {
    pub const ALL: [Self; 4] = [Self::Graphite, Self::Aurora, Self::Dawn, Self::Contrast];
    pub fn definition(self) -> &'static Definition {
        &presets()[self as usize]
    }
}
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub preset: Preset,
    pub custom: Option<Definition>,
}
impl Settings {
    pub fn definition(&self) -> &Definition {
        self.custom
            .as_ref()
            .unwrap_or_else(|| self.preset.definition())
    }
}
fn presets() -> &'static [Definition; 4] {
    static THEMES: OnceLock<[Definition; 4]> = OnceLock::new();
    THEMES.get_or_init(|| {
        [
            include_str!("../assets/themes/graphite.json"),
            include_str!("../assets/themes/aurora.json"),
            include_str!("../assets/themes/dawn.json"),
            include_str!("../assets/themes/contrast.json"),
        ]
        .map(|s| serde_json::from_str(s).expect("bundled theme must be valid"))
    })
}
pub fn parse(json: &str) -> Result<Definition, String> {
    let d: Definition = serde_json::from_str(json).map_err(|e| e.to_string())?;
    if d.version != 1 {
        return Err("Unsupported theme version (expected 1)".into());
    }
    if d.name.trim().is_empty() || !(75..=150).contains(&d.icon_scale) {
        return Err("Theme needs a name and icon scale between 75 and 150".into());
    }
    Ok(d)
}
pub fn current(ctx: &egui::Context) -> Definition {
    ctx.data(|d| d.get_temp::<Definition>(egui::Id::new("quark-theme")))
        .unwrap_or_else(|| Preset::Graphite.definition().clone())
}
pub fn colors(ctx: &egui::Context) -> Palette {
    ctx.data(|d| d.get_temp::<Palette>(egui::Id::new("quark-colors")))
        .unwrap_or_else(|| Preset::Graphite.definition().colors)
}
pub fn apply(ctx: &egui::Context, settings: &Settings) {
    let d = settings.definition();
    let c = d.colors;
    let mut v = if d.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.override_text_color = Some(c.text.color());
    v.panel_fill = c.sidebar.color();
    v.window_fill = c.surface.color();
    v.extreme_bg_color = c.editor.color();
    v.faint_bg_color = c.surface.color();
    v.widgets.inactive.bg_fill = c.surface.color();
    v.widgets.hovered.bg_fill = c.selection.color();
    v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, c.text.color());
    v.widgets.active.bg_fill = c.selection.color();
    v.widgets.active.fg_stroke = Stroke::new(1.0_f32, c.accent.color());
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, c.border.color());
    v.selection.bg_fill = c.selection.color();
    v.selection.stroke = Stroke::new(1.0_f32, c.text.color());
    v.hyperlink_color = c.accent.color();
    v.warn_fg_color = c.warning.color();
    v.error_fg_color = c.error.color();
    ctx.set_visuals(v);
    ctx.data_mut(|data| data.insert_temp(egui::Id::new("quark-colors"), c));
    ctx.data_mut(|data| data.insert_temp(egui::Id::new("quark-theme"), d.clone()));
    ctx.style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 5.0);
        s.spacing.button_padding = egui::vec2(8.0, 4.0);
        s.spacing.interact_size.y = 24.0;
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn four_complete_distinct_themes_roundtrip() {
        for p in Preset::ALL {
            let d = p.definition();
            assert_eq!(parse(&serde_json::to_string(d).unwrap()).unwrap(), *d);
        }
        assert!(!Preset::Dawn.definition().dark);
        assert_ne!(
            Preset::Graphite.definition().syntax,
            Preset::Aurora.definition().syntax
        );
        assert_ne!(
            Preset::Graphite.definition().icon_style,
            Preset::Contrast.definition().icon_style
        );
    }
    #[test]
    fn invalid_versions_and_scales_are_rejected() {
        let mut d = Preset::Graphite.definition().clone();
        d.version = 5;
        assert!(parse(&serde_json::to_string(&d).unwrap()).is_err());
        d.version = 1;
        d.icon_scale = 0;
        assert!(parse(&serde_json::to_string(&d).unwrap()).is_err());
    }
    #[test]
    fn old_preferences_default_to_graphite() {
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(s.preset, Preset::Graphite);
    }

    #[test]
    fn theme_import_rejects_missing_fields_and_blank_names() {
        assert!(parse("{}").is_err());
        let mut d = Preset::Dawn.definition().clone();
        d.name = "  ".into();
        assert!(parse(&serde_json::to_string(&d).unwrap()).is_err());
    }
    #[test]
    fn icon_scale_limits_and_all_styles_roundtrip() {
        for (scale, style) in [
            (75, IconStyle::Outline),
            (100, IconStyle::Rounded),
            (150, IconStyle::Solid),
        ] {
            let mut d = Preset::Graphite.definition().clone();
            d.icon_scale = scale;
            d.icon_style = style;
            let parsed = parse(&serde_json::to_string(&d).unwrap()).unwrap();
            assert_eq!(parsed.icon_style, style);
            assert_eq!(parsed.icon_scale, scale);
        }
    }
}
