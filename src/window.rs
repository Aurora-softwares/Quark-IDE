//! Native window controls for the integrated title bar.
use crate::theme;
use eframe::egui::{self, CursorIcon, ResizeDirection, ViewportCommand};

#[derive(Clone, Copy)]
pub(crate) enum Action {
    Minimise,
    Maximise,
    Close,
}

pub(crate) fn toggle_maximise(ctx: &egui::Context) {
    let maximised = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
    ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximised));
}

pub(crate) fn caption(ui: &mut egui::Ui, integrated: bool) {
    let label = egui::Label::new(
        egui::RichText::new("QUARK")
            .strong()
            .color(theme::colors(ui.ctx()).accent.color()),
    );
    if !integrated {
        ui.add(label);
        return;
    }
    let response = ui
        .add(label.sense(egui::Sense::click_and_drag()))
        .on_hover_text("Drag to move · Double-click to maximise or restore");
    if response.double_clicked() {
        toggle_maximise(ui.ctx());
    } else if response.drag_started_by(egui::PointerButton::Primary) {
        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
    }
}

/// Returns clicks to the caller so Close can use the document/session guard.
pub(crate) fn button(ui: &mut egui::Ui, action: Action) -> egui::Response {
    let maximised = ui.input(|i| i.viewport().maximized.unwrap_or(false));
    let title = match action {
        Action::Minimise => "Minimise window",
        Action::Maximise if maximised => "Restore window",
        Action::Maximise => "Maximise window",
        Action::Close => "Close window",
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(29.0, 24.0), egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, title));
    let colors = theme::colors(ui.ctx());
    let close_hover = matches!(action, Action::Close) && response.hovered();
    if response.hovered() {
        ui.painter().rect_filled(
            rect,
            3,
            if close_hover {
                colors.error.color()
            } else {
                colors.selection.color()
            },
        );
    }
    let stroke = egui::Stroke::new(
        1.3_f32,
        if close_hover {
            colors.editor.color()
        } else {
            colors.icon.color()
        },
    );
    let r = egui::Rect::from_center_size(rect.center(), egui::vec2(10.0, 10.0));
    match action {
        Action::Minimise => {
            ui.painter()
                .line_segment([r.left_center(), r.right_center()], stroke);
        }
        Action::Close => {
            ui.painter()
                .line_segment([r.left_top(), r.right_bottom()], stroke);
            ui.painter()
                .line_segment([r.right_top(), r.left_bottom()], stroke);
        }
        Action::Maximise if maximised => {
            let front = r.translate(egui::vec2(-1.5, 1.5)).shrink(1.0);
            let back = r.translate(egui::vec2(1.5, -1.5)).shrink(1.0);
            ui.painter()
                .line_segment([back.left_top(), back.right_top()], stroke);
            ui.painter()
                .line_segment([back.right_top(), back.right_bottom()], stroke);
            ui.painter()
                .rect_stroke(front, 0, stroke, egui::StrokeKind::Inside);
        }
        Action::Maximise => {
            ui.painter()
                .rect_stroke(r, 0, stroke, egui::StrokeKind::Inside);
        }
    }
    response.on_hover_text(title)
}

fn resize_edge(rect: egui::Rect, pos: egui::Pos2) -> Option<(ResizeDirection, CursorIcon)> {
    if !rect.contains(pos) {
        return None;
    }
    let left = pos.x < rect.left() + 4.0;
    let right = pos.x > rect.right() - 4.0;
    let top = pos.y < rect.top() + 4.0;
    let bottom = pos.y > rect.bottom() - 4.0;
    // Larger corner targets make diagonal resizing easier without covering toolbar controls.
    let corner_left = pos.x < rect.left() + 12.0;
    let corner_right = pos.x > rect.right() - 12.0;
    let corner_top = pos.y < rect.top() + 12.0;
    let corner_bottom = pos.y > rect.bottom() - 12.0;
    match (left, right, top, bottom) {
        _ if (left && corner_top) || (top && corner_left) => {
            Some((ResizeDirection::NorthWest, CursorIcon::ResizeNorthWest))
        }
        _ if (right && corner_top) || (top && corner_right) => {
            Some((ResizeDirection::NorthEast, CursorIcon::ResizeNorthEast))
        }
        _ if (left && corner_bottom) || (bottom && corner_left) => {
            Some((ResizeDirection::SouthWest, CursorIcon::ResizeSouthWest))
        }
        _ if (right && corner_bottom) || (bottom && corner_right) => {
            Some((ResizeDirection::SouthEast, CursorIcon::ResizeSouthEast))
        }
        (true, _, _, _) => Some((ResizeDirection::West, CursorIcon::ResizeWest)),
        (_, true, _, _) => Some((ResizeDirection::East, CursorIcon::ResizeEast)),
        (_, _, true, _) => Some((ResizeDirection::North, CursorIcon::ResizeNorth)),
        (_, _, _, true) => Some((ResizeDirection::South, CursorIcon::ResizeSouth)),
        _ => None,
    }
}

/// Frameless windows need to hand edge drags to the OS window manager too.
pub(crate) fn resize(ctx: &egui::Context) {
    let edge = ctx.input(|i| {
        if i.viewport().maximized == Some(true) || i.viewport().fullscreen == Some(true) {
            return None;
        }
        resize_edge(i.screen_rect(), i.pointer.hover_pos()?)
    });
    if let Some((direction, cursor)) = edge {
        ctx.set_cursor_icon(cursor);
        if ctx.input(|i| i.pointer.primary_pressed()) {
            ctx.send_viewport_cmd(ViewportCommand::BeginResize(direction));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edge_targets_cover_eight_directions_without_covering_controls() {
        let r = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0));
        for (pos, direction) in [
            ([1., 1.], ResizeDirection::NorthWest),
            ([1279., 1.], ResizeDirection::NorthEast),
            ([1., 799.], ResizeDirection::SouthWest),
            ([1279., 799.], ResizeDirection::SouthEast),
            ([1., 400.], ResizeDirection::West),
            ([1279., 400.], ResizeDirection::East),
            ([640., 1.], ResizeDirection::North),
            ([640., 799.], ResizeDirection::South),
        ] {
            assert_eq!(resize_edge(r, pos.into()).unwrap().0, direction);
        }
        assert!(resize_edge(r, egui::pos2(1260., 14.)).is_none());
        assert!(resize_edge(r, egui::pos2(640., 400.)).is_none());
        assert!(resize_edge(r, egui::pos2(-1., 400.)).is_none());
    }
}

#[cfg(test)]
mod feature_coverage {
    use super::*;
    fn input(events: Vec<egui::Event>, maximised: bool, fullscreen: bool) -> egui::RawInput {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800., 500.),
            )),
            events,
            ..Default::default()
        };
        let v = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
        v.maximized = Some(maximised);
        v.fullscreen = Some(fullscreen);
        input
    }
    fn commands(output: &egui::FullOutput) -> &[ViewportCommand] {
        &output.viewport_output[&egui::ViewportId::ROOT].commands
    }
    #[test]
    fn maximise_command_becomes_restore_when_already_maximised() {
        let ctx = egui::Context::default();
        let out = ctx.run(input(vec![], true, false), toggle_maximise);
        assert!(commands(&out).contains(&ViewportCommand::Maximized(false)));
    }
    #[test]
    fn maximise_command_works_when_backend_has_no_previous_state() {
        let ctx = egui::Context::default();
        let out = ctx.run(egui::RawInput::default(), toggle_maximise);
        assert!(commands(&out).contains(&ViewportCommand::Maximized(true)));
    }
    #[test]
    fn edge_press_emits_native_resize_and_appropriate_cursor() {
        let ctx = egui::Context::default();
        let pos = egui::pos2(1., 250.);
        let out = ctx.run(
            input(
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                false,
                false,
            ),
            resize,
        );
        assert!(commands(&out).contains(&ViewportCommand::BeginResize(ResizeDirection::West)));
        assert_eq!(out.platform_output.cursor_icon, CursorIcon::ResizeWest);
    }
    #[test]
    fn resize_is_disabled_when_maximised_fullscreen_or_pointer_in_body() {
        for (max, full, pos) in [
            (true, false, egui::pos2(1., 250.)),
            (false, true, egui::pos2(1., 250.)),
            (false, false, egui::pos2(400., 250.)),
        ] {
            let ctx = egui::Context::default();
            let out = ctx.run(
                input(
                    vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    max,
                    full,
                ),
                resize,
            );
            assert!(!commands(&out)
                .iter()
                .any(|c| matches!(c, ViewportCommand::BeginResize(_))));
        }
    }
    #[test]
    fn integrated_caption_drag_hands_movement_to_os() {
        let ctx = egui::Context::default();
        let frame = |events| {
            ctx.run(input(events, false, false), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| caption(ui, true));
            })
        };
        frame(vec![]);
        let p = egui::pos2(25., 15.);
        frame(vec![
            egui::Event::PointerMoved(p),
            egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        let out = frame(vec![egui::Event::PointerMoved(egui::pos2(45., 20.))]);
        assert!(commands(&out).contains(&ViewportCommand::StartDrag));
    }
    #[test]
    fn system_title_bar_caption_does_not_duplicate_window_drag_commands() {
        let ctx = egui::Context::default();
        let frame = |events| {
            ctx.run(input(events, false, false), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| caption(ui, false));
            })
        };
        frame(vec![]);
        let p = egui::pos2(25., 15.);
        frame(vec![
            egui::Event::PointerMoved(p),
            egui::Event::PointerButton {
                pos: p,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        let out = frame(vec![egui::Event::PointerMoved(egui::pos2(45., 20.))]);
        assert!(!commands(&out).contains(&ViewportCommand::StartDrag));
    }

    fn double_click(maximised: bool) -> egui::FullOutput {
        let ctx = egui::Context::default();
        let mut tick = 0.;
        let mut frame = |events| {
            tick += 0.05;
            let mut i = input(events, maximised, false);
            i.time = Some(tick);
            ctx.run(i, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| caption(ui, true));
            })
        };
        frame(vec![]);
        frame(vec![]);
        let pos = egui::pos2(25., 15.);
        let event = |pressed| {
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        frame(event(true));
        frame(event(false));
        frame(event(true));
        frame(event(false))
    }
    #[test]
    fn caption_double_click_maximises_a_restored_window() {
        let out = double_click(false);
        assert!(commands(&out).contains(&ViewportCommand::Maximized(true)));
    }
    #[test]
    fn caption_double_click_restores_a_maximised_window() {
        let out = double_click(true);
        assert!(commands(&out).contains(&ViewportCommand::Maximized(false)));
    }
}
