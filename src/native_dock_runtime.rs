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

/// Equal-height initial partition for adjacent pinned tool panels. A GTK
/// splitter remains user-draggable after it has been created.
pub fn equal_stack_position(height: i32, upper: usize, lower: usize) -> i32 {
    let total = upper.saturating_add(lower);
    if total == 0 {
        return 0;
    }
    ((height.max(0) as f64) * upper as f64 / total as f64).round() as i32
}

fn collect_stacked_panels(widget: &gtk4::Widget, output: &mut Vec<gtk4::Widget>) {
    if let Ok(paned) = widget.clone().downcast::<gtk4::Paned>() {
        if let Some(child) = paned.start_child() {
            collect_stacked_panels(&child, output);
        }
        if let Some(child) = paned.end_child() {
            collect_stacked_panels(&child, output);
        }
    } else {
        output.push(widget.clone());
    }
}

fn build_stack(panels: &[gtk4::Widget]) -> gtk4::Widget {
    if panels.len() == 1 {
        return panels[0].clone();
    }
    let divider = gtk4::Paned::new(gtk4::Orientation::Vertical);
    divider.add_css_class("quark-resize-pane");
    divider.set_vexpand(true);
    divider.set_hexpand(true);
    divider.set_resize_start_child(true);
    divider.set_resize_end_child(true);
    divider.set_shrink_start_child(false);
    divider.set_shrink_end_child(false);
    divider.set_start_child(Some(&panels[0]));
    let below = build_stack(&panels[1..]);
    divider.set_end_child(Some(&below));
    let weak = divider.downgrade();
    let count = panels.len();
    gtk4::glib::idle_add_local_once(move || {
        if let Some(divider) = weak.upgrade() {
            let height = divider.allocated_height();
            if height > 0 {
                divider.set_position(equal_stack_position(height, 1, count - 1));
            }
        }
    });
    divider.upcast()
}

/// GTK's first_child()/next_sibling() can be traversed without mutating
/// the container. Never use a "while first_child { remove }" loop: if a
/// wrapper has reparented a widget behind the Box's back, gtk_box_remove()
/// rejects it without advancing the loop (and floods stderr forever).
///
/// Checking direct ownership and visiting each snapshot item at most once
/// also prevents removing a nested GtkPaned descendant as if it were a
/// direct GtkBox child.
fn remove_direct_box_children(host: &gtk4::Box) -> bool {
    let container: gtk4::Widget = host.clone().upcast();
    let mut children = Vec::new();
    let mut child = host.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if widget.parent().as_ref() != Some(&container) {
            eprintln!("Quark docking: attempted to clear a widget no longer owned by its GtkBox");
            return false;
        }
        children.push(widget);
        // Defensive bound if GTK's sibling links are temporarily inconsistent.
        if children.len() > 4096 {
            eprintln!("Quark docking: refusing to clear an unexpectedly large dock host");
            return false;
        }
    }
    for child in children {
        if child.parent().as_ref() != Some(&container) {
            eprintln!("Quark docking: dock host changed ownership during reflow");
            return false;
        }
        host.remove(&child);
    }
    true
}

/// Stack an arbitrary number of moved panels in a vertical native splitter
/// chain. Each live panel is reparented, never recreated, and single panels
/// fill the entire host without leaving a redundant divider. Call only after
/// a successful zone move, not on ordinary redraws, to preserve manual sizes.
pub fn rebalance_side_host(host: &gtk4::Box) {
    let mut panels = Vec::new();
    let mut child = host.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        collect_stacked_panels(&widget, &mut panels);
        if panels.len() > 256 {
            eprintln!("Quark docking: refusing an unexpectedly deep panel stack");
            return;
        }
    }
    if panels.is_empty() {
        let _ = remove_direct_box_children(host);
        return;
    }
    for widget in &panels {
        if !detach(widget) {
            eprintln!("Quark docking: unable to detach panel from its current owner");
            return;
        }
    }
    if !remove_direct_box_children(host) {
        // Never spin retrying gtk_box_remove() on the same non-child. Preserve
        // the actual panel widgets even if their old split wrapper is stale.
        for panel in &panels {
            if panel.parent().is_none() {
                host.append(panel);
            }
        }
        return;
    }
    let stacked = build_stack(&panels);
    stacked.set_vexpand(true);
    stacked.set_hexpand(true);
    host.append(&stacked);
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
        // Reparenting the source inside connect_drop() destroys its active
        // drag controller before GDK releases the pointer. Accept the drop
        // first; commit the move on the next GTK main-loop turn.
        let apply = drop.clone();
        gtk4::glib::idle_add_local_once(move || {
            apply(id, zone);
        });
        true
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
        // Keep the popup interactive across focus transfers from the rail.
        // The editor and other tabs explicitly dismiss it instead.
        popup.set_autohide(false);
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

#[cfg(test)]
mod stack_tests {
    use super::*;

    #[test]
    fn pinned_sections_default_to_equal_heights() {
        assert_eq!(equal_stack_position(600, 1, 1), 300);
        assert_eq!(equal_stack_position(600, 1, 2), 200);
        assert_eq!(equal_stack_position(600, 2, 1), 400);
        assert_eq!(equal_stack_position(600, 1, 0), 600);
    }
}
