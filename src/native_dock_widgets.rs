//! Small consistent panel headers built through one native API.
//! The same layout model is used by GTK, AppKit and WinUI presenters.
use naui::{Color, Length, Orientation, Painter, Point, Rect, Sizing, Ui};

use crate::native_docking::DockId;

#[derive(Clone)]
pub struct DockHeader {
    pub row: naui::Stack,
    pub pin: naui::Button,
    pub collapse: naui::Button,
}

/// Compact vector icons use the same geometry on GTK, AppKit and WinUI.
/// Swap this renderer for SVG assets later without modifying dock headers.
fn render_dock_icon(p: &mut Painter, id: DockId) {
    let color = match id {
        DockId::Toolbox | DockId::Symbols => Color::rgb(0x36, 0xc2, 0xd6),
        DockId::Solution | DockId::Properties => Color::rgb(0x9d, 0x85, 0xeb),
        DockId::ErrorList | DockId::Debug => Color::rgb(0xe0, 0x9c, 0x6f),
        DockId::Output | DockId::TestResults => Color::rgb(0x70, 0xc3, 0x94),
        _ => Color::rgb(0x81, 0xae, 0xe6),
    };
    // Transparent icon slot: never paint a dark tile behind icons in light mode.
    // The host panel supplies the appropriate native background.
    match id {
        DockId::Toolbox | DockId::Data => {
            for y in [9.0, 16.0] {
                for x in [8.0, 15.0] {
                    p.fill_rect(Rect::new(x, y, 4.0, 4.0), color);
                }
            }
        }
        DockId::Solution | DockId::Server => {
            p.fill_rect(Rect::new(7.0, 11.0, 13.0, 10.0), color);
            p.fill_rect(Rect::new(7.0, 8.0, 7.0, 4.0), color);
            p.fill_rect(Rect::new(9.0, 15.0, 9.0, 4.0), Color::rgb(0x65, 0x51, 0xa0));
        }
        DockId::Properties => {
            for (y, x) in [(10.0, 12.0), (15.0, 17.0), (20.0, 9.0)] {
                p.fill_rect(Rect::new(7.0, y, 13.0, 1.0), color);
                p.fill_rect(Rect::new(x, y - 2.0, 3.0, 5.0), color);
            }
        }
        DockId::Output | DockId::Debug => {
            p.line(Point::new(8.0, 10.0), Point::new(13.0, 15.0), color, 2.0);
            p.line(Point::new(13.0, 15.0), Point::new(8.0, 20.0), color, 2.0);
            p.fill_rect(Rect::new(15.0, 19.0, 5.0, 2.0), color);
        }
        DockId::Search | DockId::FindResults => {
            p.line(Point::new(16.0, 18.0), Point::new(20.0, 22.0), color, 2.0);
            p.fill_rect(Rect::new(9.0, 10.0, 7.0, 2.0), color);
            p.fill_rect(Rect::new(8.0, 12.0, 2.0, 7.0), color);
            p.fill_rect(Rect::new(10.0, 18.0, 7.0, 2.0), color);
            p.fill_rect(Rect::new(16.0, 12.0, 2.0, 7.0), color);
        }
        DockId::Symbols => {
            p.line(Point::new(13.0, 8.0), Point::new(20.0, 15.0), color, 2.0);
            p.line(Point::new(20.0, 15.0), Point::new(13.0, 22.0), color, 2.0);
            p.line(Point::new(13.0, 22.0), Point::new(6.0, 15.0), color, 2.0);
            p.line(Point::new(6.0, 15.0), Point::new(13.0, 8.0), color, 2.0);
        }
        DockId::ErrorList => {
            p.fill_rect(Rect::new(12.0, 9.0, 3.0, 9.0), color);
            p.fill_rect(Rect::new(12.0, 20.0, 3.0, 3.0), color);
        }
        DockId::TestResults => {
            p.line(Point::new(7.0, 16.0), Point::new(11.0, 20.0), color, 2.0);
            p.line(Point::new(11.0, 20.0), Point::new(20.0, 10.0), color, 2.0);
        }
    }
}

/// One factory for a complete dock section: outer native widget, header,
/// grab affordance, and content container. Callers supply only the contents.
/// All sections therefore share the same border, spacing and title chrome.
pub fn section_panel(ui: &Ui, id: DockId) -> naui::Result<(naui::Stack, DockHeader)> {
    let panel = ui.stack(Orientation::Vertical)?;
    panel.set_spacing(5.0);
    panel.set_sizing(Sizing::fill());
    let header = section_header(ui, id)?;
    panel.append(&header.row);
    #[cfg(target_os = "linux")]
    {
        use gtk4::prelude::*;
        use naui::Widget as _;
        panel.native_widget().add_css_class("quark-dock-panel");
        header.row.native_widget().add_css_class("quark-dock-header");
        thread_local! {
            static INSTALLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        }
        INSTALLED.with(|installed| {
            if !installed.replace(true) {
                if let Some(display) = gtk4::gdk::Display::default() {
                    let css = gtk4::CssProvider::new();
                    css.load_from_data(
                        ".quark-dock-panel { border: 1px solid alpha(@theme_fg_color, 0.10); } 
                         .quark-dock-header { border-bottom: 1px solid alpha(@theme_fg_color, 0.10); }"
                    );
                    gtk4::style_context_add_provider_for_display(
                        &display, &css, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
                    );
                }
            }
        });
    }
    Ok((panel, header))
}

/// An internal drag payload is limited to Quark's known dock IDs; arbitrary
/// dropped text is never treated as a command or a path.
pub fn drag_token(id: DockId) -> String {
    format!("quark-dock:{id:?}")
}
pub fn parse_drag_token(value: &str) -> Option<DockId> {
    let tag = value.strip_prefix("quark-dock:")?;
    DockId::ALL.into_iter().find(|id| format!("{id:?}") == tag)
}

/// GTK adds a native pointer drag to the common header and passes a validated
/// source ID to the owning dock host. The host is responsible for moving
/// widgets and persisting the new layout, not this shared visual component.
#[cfg(target_os = "linux")]
pub fn enable_header_drag(
    header: &DockHeader,
    id: DockId,
    on_drop: impl Fn(DockId) -> bool + 'static,
) {
    use gtk4::prelude::*;
    use naui::Widget as _;
    let source = gtk4::DragSource::new();
    source.set_actions(gtk4::gdk::DragAction::MOVE);
    let token = drag_token(id);
    source.connect_prepare(move |_, _, _| {
        Some(gtk4::gdk::ContentProvider::for_value(&token.to_value()))
    });
    header.row.native_widget().add_controller(source);
    let target = gtk4::DropTarget::new(String::static_type(), gtk4::gdk::DragAction::MOVE);
    target.connect_drop(move |_, value, _, _| {
        value.get::<String>().ok()
            .and_then(|token| parse_drag_token(&token))
            .is_some_and(|source| on_drop(source))
    });
    header.row.native_widget().add_controller(target);
}

pub fn section_header(ui: &Ui, id: DockId) -> naui::Result<DockHeader> {
    let row = ui.stack(Orientation::Horizontal)?;
    row.set_spacing(5.0);
    row.set_sizing(
        Sizing::new()
            .width(Length::Fill)
            .height(Length::Fixed(34.0)),
    );

    // Consistent left-aligned 26px icon slot; icon colours are deliberately
    // data-driven, so they can later be replaced with SVG assets.
    let icon = ui.canvas()?;
    icon.set_sizing(
        Sizing::new()
            .width(Length::Fixed(27.0))
            .height(Length::Fixed(30.0)),
    );
    icon.on_draw(move |painter| render_dock_icon(painter, id));
    row.append(&icon);

    let title = ui.label(id.title())?;
    title.set_style(naui::TextStyle::Heading);
    row.append(&title);

    let pin = ui.button("♢")?;
    pin.set_sizing(Sizing::new().width(Length::Fixed(34.0)));
    row.append(&pin);

    let collapse = ui.button("⌄")?;
    collapse.set_sizing(Sizing::new().width(Length::Fixed(34.0)));
    row.append(&collapse);

    #[cfg(target_os = "linux")]
    {
        use gtk4::prelude::*;
        use naui::Widget as _;
        pin.native_widget().set_tooltip_text(Some(
            "Pin this tool window to keep it visible; unpinned windows collapse to their tab rails.",
        ));
        collapse
            .native_widget()
            .set_tooltip_text(Some("Collapse or expand the panel content."));
    }
    Ok(DockHeader { row, pin, collapse })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dock_drag_tokens_reject_unknown_and_external_text() {
        for id in DockId::ALL {
            assert_eq!(parse_drag_token(&drag_token(id)), Some(id));
        }
        assert_eq!(parse_drag_token("quark-dock:__unknown__"), None);
        assert_eq!(parse_drag_token("../src/main.rs"), None);
    }

    #[test]
    fn every_dock_has_vector_geometry_without_font_fallbacks() {
        for id in DockId::ALL {
            let mut painter = Painter::new(27.0, 30.0);
            render_dock_icon(&mut painter, id);
            assert!(
                painter.commands().len() >= 2,
                "{id:?} must draw an identifiable native tool icon"
            );
        }
    }
}
