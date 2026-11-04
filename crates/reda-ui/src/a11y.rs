//! Accessibility helpers: what screen readers hear for REDA's own widgets, and the
//! keyboard focus ring.
//!
//! Every icon-only button, tab and Dock icon is named by its Command or Panel title, with
//! the key that runs it as its description (docs/spec/v0.1/app.md#accessibility).

use egui::{Context, Id, Order, Response, Role, Stroke, StrokeKind, WidgetInfo};

/// Names `response` for AccessKit: `role`, `name`, whether it is enabled, and, for a
/// tab or a toggle, whether it is `selected`. Its description is `shortcut`, the key that
/// runs it, or none.
///
/// Call it after any tooltip, which would otherwise make the tooltip the description.
pub fn name(
    response: &Response,
    role: Role,
    enabled: bool,
    name: &str,
    selected: Option<bool>,
    shortcut: Option<&str>,
) {
    // Tabs and list items are selected; buttons are toggled on or off.
    let pickable = matches!(role, Role::Tab | Role::ListItem);
    response.widget_info(|| {
        let mut info = WidgetInfo::labeled(role, enabled, name);
        info.selected = selected.filter(|_| !pickable);
        info
    });
    response.ctx.accesskit_node_builder(response.id, |node| {
        node.set_label(name);
        if pickable && let Some(selected) = selected {
            node.set_selected(selected);
        }
        match shortcut {
            Some(key) => {
                node.set_description(key);
                node.set_keyboard_shortcut(key);
            }
            None => {
                node.clear_description();
                node.clear_keyboard_shortcut();
            }
        }
    });
}

/// Whether the keyboard moved focus last, rather than the pointer: the focus ring shows
/// only then, as the platforms do.
fn keyboard_mode(ctx: &Context) -> bool {
    let id = Id::NULL.with("reda-keyboard-focus-mode");
    let mut mode = ctx.data(|d| d.get_temp::<bool>(id).unwrap_or(false));
    ctx.input(|i| {
        // The raw events: the Keymap takes the keys it runs out of `events`.
        for event in &i.raw.events {
            match event {
                egui::Event::Key { pressed: true, .. } => mode = true,
                egui::Event::PointerButton { pressed: true, .. } => mode = false,
                _ => {}
            }
        }
    });
    ctx.data_mut(|d| d.insert_temp(id, mode));
    mode
}

/// Draws a ring around the widget with keyboard focus, over everything else, so focus is
/// always visible whatever the widget draws for itself. The window calls this once per
/// frame, after drawing.
pub fn focus_ring(ctx: &Context) {
    if !keyboard_mode(ctx) {
        return;
    }
    let Some(id) = ctx.memory(|m| m.focused()) else {
        return;
    };
    let Some(response) = ctx.read_response(id) else {
        return;
    };
    let accent = crate::theme::accent(ctx.theme());
    // An Area of its own, so the ring's layer is ordered over the window's.
    egui::Area::new(Id::NULL.with("focus-ring"))
        .order(Order::Foreground)
        .interactable(false)
        .fixed_pos(response.rect.min)
        .show(ctx, |ui| {
            let painter = ui.painter().with_clip_rect(ctx.content_rect());
            painter.rect_stroke(
                response.rect.expand(1.0),
                2.0,
                Stroke::new(2.0, accent),
                StrokeKind::Outside,
            );
        });
}
