//! Small consistent panel headers built through one native API.
//! The same layout model is used by GTK, AppKit and WinUI presenters.
use naui::{Color, Length, Orientation, Point, Rect, Sizing, Ui};

use crate::native_docking::DockId;

#[derive(Clone)]
pub struct DockHeader {
    pub row: naui::Stack,
    pub pin: naui::Button,
    pub collapse: naui::Button,
    pub drag: naui::Canvas,
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
    icon.on_draw(move |painter| {
        let (r, g, b) = match id {
            DockId::Toolbox | DockId::Symbols => (0x36, 0xc2, 0xd6),
            DockId::Solution | DockId::Properties => (0x9d, 0x85, 0xeb),
            DockId::ErrorList | DockId::Debug => (0xe0, 0x9c, 0x6f),
            DockId::Output | DockId::TestResults => (0x70, 0xc3, 0x94),
            _ => (0x81, 0xae, 0xe6),
        };
        painter.fill_rect(
            Rect::new(2.0, 3.0, 23.0, 24.0),
            Color::rgb(0x36, 0x39, 0x42),
        );
        painter.text(id.glyph(), Point::new(6.0, 8.0), 12.0, Color::rgb(r, g, b));
    });
    row.append(&icon);

    let title = ui.label(id.title())?;
    title.set_style(naui::TextStyle::Heading);
    row.append(&title);

    let drag = ui.canvas()?;
    drag.set_sizing(
        Sizing::new()
            .width(Length::Fixed(26.0))
            .height(Length::Fixed(30.0)),
    );
    drag.on_draw(|painter| {
        // Grabber handles are visually distinct from panel content buttons.
        for row in [10.0_f64, 15.0, 20.0] {
            for x in [8.0_f64, 13.0, 18.0] {
                painter.fill_rect(Rect::new(x, row, 2.0, 2.0), Color::rgb(0x8b, 0x8c, 0x96));
            }
        }
    });
    row.append(&drag);

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
        drag.native_widget().set_tooltip_text(Some(
            "Drag the header to move this panel between Left, Right and Bottom docks.",
        ));
        pin.native_widget().set_tooltip_text(Some(
            "Pin this panel open, or auto-hide it when focus moves elsewhere.",
        ));
        collapse
            .native_widget()
            .set_tooltip_text(Some("Collapse or expand the panel content."));
    }
    Ok(DockHeader {
        row,
        pin,
        collapse,
        drag,
    })
}
