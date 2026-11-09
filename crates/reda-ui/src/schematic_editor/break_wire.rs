//! The Break Wire Tool (docs/spec/v0.1/schematic.md#break-wire): a click cuts a gap out of
//! the Wire under the pointer with `schematic.wire.break`, as long as the Cutting Length
//! Preference says (Snap To Segment, Snap Grid Size Multiple or Fixed Length; Space
//! cycles it). Over a Wire the canvas previews the cut ([`break_gap`]): the Cutter Box and
//! Extremity Markers as the Display Preferences say (Never, Always, On Wire), or, with
//! both Never, the stretch to be cut highlighted. Off a Wire, Always draws them at the
//! pointer.
//!
//! Tab (`tool.edit_properties`) opens the Tool's options; every change there, and Space,
//! writes straight back to the Preference with `app.preferences.set`.

use egui::{Rect, Ui};
use reda_engine::model::Uuid;
use reda_engine::model::geom::{Nm, Point};
use reda_engine::preferences::{BreakWirePreferences, BreakWireShow, CuttingLength};
use reda_engine::schematic::break_gap;
use serde_json::{Value, json};

use super::{Cx, Interaction, Invocation, P, point};
use crate::canvas::interact::Pointer;
use crate::canvas::{Cutter, Overlay};
use crate::shell::Shell;
use crate::tools::{BreakWireTool, Tool};
use crate::tr;

/// The Break Wire Tool's pointer.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Breaking {
    /// Where the pointer is (nm).
    pub at: P,
    /// The Wire under it, if any.
    pub wire: Option<Uuid>,
    /// Half the Cutter Box's width (nm): the pick distance at the canvas's zoom.
    pub half_width: f64,
}

impl Cx<'_> {
    /// A pointer event in the Break Wire Tool: the preview follows the pointer, and a
    /// click on a Wire cuts it.
    pub(super) fn break_step(&mut self, mut b: Breaking, event: Pointer) -> Interaction {
        let at = super::event_at(event).unwrap_or(b.at);
        b.at = at;
        b.half_width = self.scale.pick;
        b.wire = self
            .hit(at)
            .filter(|o| self.sheet().is_some_and(|s| s.wire(*o).is_some()));
        if let (Pointer::Click { .. }, Some(wire)) = (event, b.wire) {
            let length = self.length();
            let mut params =
                json!({"wire": wire, "at": {"x": at.0.round() as i64, "y": at.1.round() as i64}});
            if let Some(length) = length {
                params["length"] = json!(length.0);
            }
            self.commands
                .push(("schematic.wire.break".to_owned(), params));
        }
        Interaction::Breaking(b)
    }

    fn length(&self) -> Option<Nm> {
        let grid = self.shell.grid(self.document);
        self.shell.preferences().schematic.break_wire.length(grid)
    }
}

/// What the canvas draws for `b`: the cut it would make, as the Display Preferences say.
pub(super) fn overlay(shell: &Shell, document: Uuid, b: &Breaking) -> Overlay {
    let prefs = &shell.preferences().schematic.break_wire;
    let grid = shell.grid(document);
    let length = prefs.length(grid);
    let wire = (shell.engine().project())
        .and_then(|p| p.schematic_sheet(document))
        .and_then(|s| s.wire(b.wire?))
        .cloned();
    let on_wire = wire.is_some();
    let shown = |show: BreakWireShow| match show {
        BreakWireShow::Never => false,
        BreakWireShow::Always => true,
        BreakWireShow::OnWire => on_wire,
    };
    let (gap, along) = match &wire {
        Some(wire) => {
            let gap = break_gap(wire, point(b.at), length);
            let (a, z) = (wire.vertices[gap.segment], wire.vertices[gap.segment + 1]);
            ((gap.from, gap.to), unit(a, z))
        }
        None => {
            let c = crate::snap::to_grid(shell, document, b.at);
            let half = length.unwrap_or(grid * 2).0 / 2;
            let d = Point::new(Nm(half), Nm(0));
            ((c - d, c + d), (1.0, 0.0))
        }
    };
    let cutter_box = shown(prefs.show_cutter_box);
    let markers = shown(prefs.show_extremity_markers);
    let highlight = on_wire
        && prefs.show_cutter_box == BreakWireShow::Never
        && prefs.show_extremity_markers == BreakWireShow::Never;
    let cutter = (cutter_box || markers || highlight).then(|| {
        let across = |p: Point, k: f64| {
            let (nx, ny) = (-along.1 * b.half_width * k, along.0 * b.half_width * k);
            let shift = Point::new(Nm(nx.round() as i64), Nm(ny.round() as i64));
            (p + shift, p - shift)
        };
        let (from, to) = (across(gap.0, 1.0), across(gap.1, 1.0));
        Cutter {
            gap,
            cutter_box: cutter_box.then_some([from.0, to.0, to.1, from.1]),
            extremities: markers.then(|| [across(gap.0, 1.5), across(gap.1, 1.5)]),
            highlight,
        }
    });
    Overlay {
        cutter,
        cursor: Some(crate::snap::to_grid(shell, document, b.at)),
        ..Overlay::default()
    }
}

/// The unit vector from `a` to `b`; along x when they are the same point.
fn unit(a: Point, b: Point) -> (f64, f64) {
    let (dx, dy) = ((b.x - a.x).0 as f64, (b.y - a.y).0 as f64);
    let length = dx.hypot(dy);
    if length == 0.0 {
        (1.0, 0.0)
    } else {
        (dx / length, dy / length)
    }
}

/// `app.preferences.set` for the Break Wire Preference `name`.
fn set(name: &str, value: Value) -> Invocation {
    (
        "app.preferences.set".to_owned(),
        json!({"key": format!("schematic.break_wire.{name}"), "value": value}),
    )
}

/// Space in the Tool: the next Cutting Length, written to the Preference.
pub fn cycle_cutting_length(shell: &Shell) -> Invocation {
    let next = shell
        .preferences()
        .schematic
        .break_wire
        .cutting_length
        .next();
    set("cutting_length", json!(next))
}

/// Draws the Tool's options over the canvas `rect` of `document` while Tab has them open;
/// returns the Commands its controls ask for, each writing one Preference.
pub fn options(ui: &Ui, shell: &Shell, document: Uuid, rect: Rect) -> Vec<Invocation> {
    let mut out = Vec::new();
    let on_sheet = shell
        .engine()
        .project()
        .is_some_and(|p| p.schematic_sheet(document).is_some());
    let open = (shell.engine().view().get::<BreakWireTool>()).is_some_and(|t| t.options_open);
    if !on_sheet || shell.tool() != Tool::BreakWire || !open {
        return out;
    }
    let prefs: BreakWirePreferences = shell.preferences().schematic.break_wire.clone();
    egui::Window::new(tr!("break-wire-title"))
        .id(egui::Id::NULL.with(("break-wire-options", document)))
        .collapsible(false)
        .resizable(false)
        .fixed_pos(rect.left_top() + egui::vec2(12.0, 48.0))
        .show(ui.ctx(), |ui| {
            ui.strong(tr!("pref-schematic-break-wire-cutting-length"));
            let lengths = [
                (CuttingLength::Segment, "segment"),
                (CuttingLength::GridMultiple, "grid-multiple"),
                (CuttingLength::Fixed, "fixed"),
            ];
            for (length, id) in lengths {
                let label = crate::i18n::translate(
                    &format!("pref-schematic-break-wire-cutting-length-{id}"),
                    &[],
                );
                if ui.radio(prefs.cutting_length == length, label).clicked() {
                    out.push(set("cutting_length", json!(length)));
                }
            }
            let mut multiple = prefs.grid_multiple;
            let range = BreakWirePreferences::GRID_MULTIPLES;
            let slider = egui::Slider::new(&mut multiple, *range.start()..=*range.end())
                .text(tr!("pref-schematic-break-wire-grid-multiple"));
            if ui.add(slider).changed() {
                out.push(set("grid_multiple", json!(multiple)));
            }
            let mut mils = prefs.fixed_length.0.0 as f64 / 25_400.0;
            let fixed = egui::Slider::new(&mut mils, 1.0..=1000.0)
                .logarithmic(true)
                .suffix(" mil")
                .text(tr!("pref-schematic-break-wire-fixed-length"));
            if ui.add(fixed).changed() {
                let nm = (mils * 25_400.0).round().max(25_400.0) as i64;
                out.push(set("fixed_length", json!(nm)));
            }
            ui.separator();
            let displays = [
                ("show_cutter_box", "show-cutter-box", prefs.show_cutter_box),
                (
                    "show_extremity_markers",
                    "show-extremity-markers",
                    prefs.show_extremity_markers,
                ),
            ];
            for (key, id, current) in displays {
                let title = crate::i18n::translate(&format!("pref-schematic-break-wire-{id}"), &[]);
                ui.strong(&title);
                ui.horizontal(|ui| {
                    let shows = [
                        (BreakWireShow::Never, "never"),
                        (BreakWireShow::Always, "always"),
                        (BreakWireShow::OnWire, "on-wire"),
                    ];
                    for (show, name) in shows {
                        let label = crate::i18n::translate(
                            &format!("pref-schematic-break-wire-{id}-{name}"),
                            &[],
                        );
                        let accessible = format!("{title}: {label}");
                        let radio = ui.radio(current == show, label);
                        radio.widget_info(|| {
                            egui::WidgetInfo::selected(
                                egui::Role::RadioButton,
                                true,
                                current == show,
                                accessible.as_str(),
                            )
                        });
                        if radio.clicked() {
                            out.push(set(key, json!(show)));
                        }
                    }
                });
            }
            if ui.button(tr!("break-wire-close")).clicked() {
                out.push(("tool.edit_properties".to_owned(), json!({})));
            }
        });
    out
}
