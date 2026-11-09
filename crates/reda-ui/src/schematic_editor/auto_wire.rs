//! The Wire Tool's Auto Wire options, which Tab (`tool.edit_properties`) opens over the
//! sheet: Time Out After and the Avoid cutting wires slider from low to high. Every change
//! runs `tool.auto_wire.set`; the session keeps the options ([`WireTool`]).

use egui::{Rect, Ui};
use reda_engine::model::Uuid;
use reda_engine::schematic::Avoid;
use serde_json::json;

use super::Invocation;
use crate::shell::Shell;
use crate::tools::{Tool, WireTool};
use crate::tr;

/// Draws the options window over the canvas `rect` of `document` while the Wire Tool has
/// it open; returns the Commands its controls ask for.
pub fn options(ui: &Ui, shell: &Shell, document: Uuid, rect: Rect) -> Vec<Invocation> {
    let mut out = Vec::new();
    let on_sheet = shell
        .engine()
        .project()
        .is_some_and(|p| p.schematic_sheet(document).is_some());
    let Some(wire) = shell.engine().view().get::<WireTool>().copied() else {
        return out;
    };
    if !on_sheet || shell.tool() != Tool::Wire || !wire.options_open {
        return out;
    }
    egui::Window::new(tr!("auto-wire-title"))
        .id(egui::Id::NULL.with(("auto-wire-options", document)))
        .collapsible(false)
        .resizable(false)
        .fixed_pos(rect.left_top() + egui::vec2(12.0, 48.0))
        .show(ui.ctx(), |ui| {
            let mut seconds = wire.timeout_ms as f64 / 1000.0;
            let timeout = ui.add(
                egui::Slider::new(&mut seconds, 0.1..=60.0)
                    .logarithmic(true)
                    .suffix(" s")
                    .text(tr!("auto-wire-timeout")),
            );
            if timeout.changed() {
                let ms = (seconds * 1000.0).round().clamp(100.0, 60_000.0) as u32;
                out.push(("tool.auto_wire.set".to_owned(), json!({ "timeout_ms": ms })));
            }
            let levels = [Avoid::Low, Avoid::Medium, Avoid::High];
            let mut level = levels.iter().position(|a| *a == wire.avoid).unwrap_or(1) as f64;
            let avoid = ui.add(
                egui::Slider::new(&mut level, 0.0..=2.0)
                    .step_by(1.0)
                    .custom_formatter(|v, _| match v.round() as i64 {
                        0 => tr!("auto-wire-avoid-low"),
                        1 => tr!("auto-wire-avoid-medium"),
                        _ => tr!("auto-wire-avoid-high"),
                    })
                    .text(tr!("auto-wire-avoid")),
            );
            if avoid.changed() {
                let avoid = levels[(level.round() as usize).min(2)];
                out.push((
                    "tool.auto_wire.set".to_owned(),
                    json!({ "avoid_cutting_wires": avoid }),
                ));
            }
            if ui.button(tr!("auto-wire-close")).clicked() {
                out.push(("tool.edit_properties".to_owned(), json!({})));
            }
        });
    out
}
