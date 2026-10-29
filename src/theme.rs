//! Familiar dark editor surfaces with Quark's blue accent.
use eframe::egui::{self, Color32, Stroke};
pub const ACCENT: Color32 = Color32::from_rgb(75, 169, 240);
pub const MUTED: Color32 = Color32::from_rgb(157, 157, 157);
pub const WARM: Color32 = Color32::from_rgb(229, 192, 123);
pub const SURFACE: Color32 = Color32::from_rgb(24, 24, 24);
pub const EDITOR: Color32 = Color32::from_rgb(31, 31, 31);
pub const BLUE: Color32 = Color32::from_rgb(0, 102, 184);
pub fn apply(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.override_text_color = Some(Color32::from_rgb(212, 212, 212));
    visuals.panel_fill = SURFACE;
    visuals.window_fill = Color32::from_rgb(37, 37, 38);
    visuals.extreme_bg_color = EDITOR;
    visuals.faint_bg_color = Color32::from_rgb(42, 42, 43);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(45, 45, 48);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(55, 55, 59);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    visuals.widgets.active.bg_fill = Color32::from_rgb(4, 57, 94);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(48, 48, 48));
    visuals.selection.bg_fill = Color32::from_rgb(4, 57, 94);
    visuals.selection.stroke = Stroke::new(1.0_f32, Color32::from_rgb(225, 237, 248));
    visuals.hyperlink_color = ACCENT;
    visuals.warn_fg_color = WARM;
    visuals.error_fg_color = Color32::from_rgb(244, 135, 113);
    ctx.set_visuals(visuals);
    ctx.style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(7.0, 6.0);
        style.spacing.button_padding = egui::vec2(9.0, 5.0);
        style.spacing.interact_size.y = 25.0;
    });
}
