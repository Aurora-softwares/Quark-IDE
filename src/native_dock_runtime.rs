//! GTK docking host operations for live Quark tool widgets.
//! Native widgets are moved, never cloned or reconstructed: selection, scroll
//! positions, event subscriptions and open documents survive the relocation.
//! The serializable DockLayout remains the only persisted authority.
#![cfg(target_os = "linux")]
use crate::native_dock_widgets::parse_drag_token;
use crate::native_docking::{DockId, DockZone};
use gtk4::prelude::*;
use gtk4::{self, gdk};
use std::rc::Rc;

/// GTK widgets can only belong to one parent. Explicitly remove them from
/// supported native host containers before moving to another docking area.
pub fn detach(widget: &gtk4::Widget) -> bool {
    let Some(parent) = widget.parent() else {
        return true;
    };
    if let Ok(box_) = parent.clone().downcast::<gtk4::Box>() {
        box_.remove(widget);
        return true;
    }
    if let Ok(paned) = parent.clone().downcast::<gtk4::Paned>() {
        if paned.start_child().as_ref() == Some(widget) {
            paned.set_start_child(None::<&gtk4::Widget>);
            return true;
        }
        if paned.end_child().as_ref() == Some(widget) {
            paned.set_end_child(None::<&gtk4::Widget>);
            return true;
        }
    }
    if let Ok(popover) = parent.clone().downcast::<gtk4::Popover>() {
        if popover.child().as_ref() == Some(widget) {
            popover.set_child(None::<&gtk4::Widget>);
            return true;
        }
    }
    if let Ok(window) = parent.downcast::<gtk4::Window>() {
        if window.child().as_ref() == Some(widget) {
            window.set_child(None::<&gtk4::Widget>);
            return true;
        }
    }
    false
}

pub fn attach_box(widget: &gtk4::Widget, host: &gtk4::Box) -> bool {
    if widget.parent().as_ref() == Some(&host.clone().upcast::<gtk4::Widget>()) {
        return true;
    }
    if !detach(widget) {
        return false;
    }
    host.append(widget);
    true
}

pub fn attach_paned(widget: &gtk4::Widget, host: &gtk4::Paned, start: bool) -> bool {
    let occupied = if start {
        host.start_child()
    } else {
        host.end_child()
    };
    if occupied.as_ref() == Some(widget) {
        return true;
    }
    if occupied.is_some() {
        // The user may have swapped Solution and Properties. Redocking one
        // must use the free sibling slot rather than silently rejecting it.
        let alternate = if start {
            host.end_child()
        } else {
            host.start_child()
        };
        if alternate.is_none() {
            return attach_paned(widget, host, !start);
        }
        return false;
    }
    if !detach(widget) {
        return false;
    }
    if start {
        host.set_start_child(Some(widget));
    } else {
        host.set_end_child(Some(widget));
    }
    true
}

/// A zone's drop target accepts only the internal, validated panel IDs.
/// Drag motion is highlighted with a GTK CSS class; unrelated files and text
/// are refused. The presenter commits the layout only after reparent succeeds.
pub fn install_zone_target(
    host: &gtk4::Widget,
    zone: DockZone,
    drop: Rc<dyn Fn(DockId, DockZone) -> bool>,
) {
    let target = gtk4::DropTarget::new(String::static_type(), gdk::DragAction::MOVE);
    let highlighted = host.clone();
    target.connect_enter(move |_, _, _| {
        highlighted.add_css_class("quark-dock-drop-preview");
        gdk::DragAction::MOVE
    });
    let highlighted = host.clone();
    target.connect_leave(move |_| highlighted.remove_css_class("quark-dock-drop-preview"));
    let highlighted = host.clone();
    target.connect_drop(move |_, value, _, _| {
        highlighted.remove_css_class("quark-dock-drop-preview");
        let Some(id) = value
            .get::<String>()
            .ok()
            .and_then(|v| parse_drag_token(&v))
        else {
            return false;
        };
        drop(id, zone)
    });
    host.add_controller(target);
}

/// Install zone preview visuals exactly once on the display. This uses native
/// theme colours and never draws a fake window over the real GTK content.
pub fn install_preview_css() {
    thread_local! {
        static INSTALLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    INSTALLED.with(|installed| {
        if installed.replace(true) {
            return;
        }
        if let Some(display) = gdk::Display::default() {
            let provider = gtk4::CssProvider::new();
            provider.load_from_data(
                ".quark-dock-drop-preview { outline: 3px solid @accent_color; \
                 outline-offset: -4px; background: alpha(@accent_color, 0.08); }",
            );
            gtk4::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    });
}

/// An unpinned section opens over the workspace as a native, auto-dismissing
/// GTK popover. No editor resize is necessary. Keep the popover for reuse:
/// anchors own their children and a second temporary window is unnecessary.
pub fn open_auto_hide(
    anchor: &gtk4::Widget,
    panel: &gtk4::Widget,
    existing: Option<&gtk4::Popover>,
    position: gtk4::PositionType,
    on_closed: Rc<dyn Fn()>,
) -> Option<gtk4::Popover> {
    let popover = if let Some(existing) = existing {
        existing.clone()
    } else {
        let popup = gtk4::Popover::new();
        popup.set_autohide(true);
        popup.set_has_arrow(false);
        popup.set_position(position);
        popup.set_size_request(
            if position == gtk4::PositionType::Top {
                460
            } else {
                340
            },
            360,
        );
        popup.set_parent(anchor);
        popup.connect_closed(move |_| on_closed());
        popup
    };
    if !detach(panel) {
        return None;
    }
    popover.set_child(Some(panel));
    popover.popup();
    // The side rail uses a native drawing area rather than a MenuButton.
    // Explicitly present the anchored popover after changing its contents.
    popover.present();
    Some(popover)
}

/// A detached panel owns its original, live GTK widget. Closing the floating
/// native window docks it back, instead of destroying or rebuilding it.
pub fn float_panel(
    id: DockId,
    widget: &gtk4::Widget,
    redock: Rc<dyn Fn(DockId)>,
) -> Option<gtk4::Window> {
    if !detach(widget) {
        return None;
    }
    let floating = gtk4::Window::builder()
        .title(format!("Quark — {}", id.title()))
        .default_width(420)
        .default_height(340)
        .build();
    floating.set_child(Some(widget));
    let returning = widget.clone();
    floating.connect_close_request(move |window| {
        window.set_child(None::<&gtk4::Widget>);
        redock(id);
        // Reattach happens in the host callback; keep the widget alive.
        let _ = &returning;
        gtk4::glib::Propagation::Proceed
    });
    floating.present();
    Some(floating)
}
