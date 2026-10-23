//! Restrained Quark dark theme: graphite surfaces, teal primary accent,
//! warm warning and success colours. Syntax colours live in editor.rs.
use eframe::egui::{self, Color32, Stroke};

pub const ACCENT: Color32 = Color32::from_rgb(92, 196, 184);
pub const MUTED: Color32 = Color32::from_rgb(135, 148, 166);
pub const WARM: Color32 = Color32::from_rgb(232, 180, 105);
pub const SURFACE: Color32 = Color32::from_rgb(26, 31, 40);

pub fn apply(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.override_text_color = Some(Color32::from_rgb(218, 227, 237));
    visuals.panel_fill = Color32::from_rgb(23, 28, 36);
    visuals.window_fill = SURFACE;
    visuals.extreme_bg_color = Color32::from_rgb(16, 21, 28);
    visuals.faint_bg_color = Color32::from_rgb(34, 40, 51);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(40, 47, 59);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(51, 63, 74);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(239, 246, 248));
    visuals.widgets.active.bg_fill = Color32::from_rgb(45, 83, 83);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(51, 59, 71));
    visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(45, 113, 110, 130);
    visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.hyperlink_color = ACCENT;
    visuals.warn_fg_color = WARM;
    visuals.error_fg_color = Color32::from_rgb(227, 125, 130);
    ctx.set_visuals(visuals);
    ctx.style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(5.0, 4.0);
        style.spacing.button_padding = egui::vec2(6.0, 3.0);
        style.spacing.interact_size.y = 23.0;
    });
}
