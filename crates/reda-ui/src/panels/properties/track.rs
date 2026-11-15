//! The Track region (Start and End XY, Layer, Net with a canvas pick) and the Net block
//! (Net, Net Class, colour, `loop_removal`, routed length, resistance and the current
//! estimate with its editable ΔT) of the Properties panel; see
//! [`crate::properties::track`].

use egui::{Role, Ui, WidgetInfo};
use reda_engine::model::Project;
use reda_engine::model::geom::Point;
use reda_engine::preferences::Units;
use reda_engine::render::Rgba;
use serde_json::json;

use super::{Out, parse_length, region, text_field, value_width};

/// The least width of a Track's Net dropdown, beside its pick button.
const MIN_NET_WIDTH: f32 = 60.0;
use crate::properties::track::{NetBlock, TrackEdit, TrackProperties, net_block};
use crate::properties::{MIXED, Shared};
use crate::shell::Shell;
use crate::theme::TokensExt as _;
use crate::tr;
use crate::widgets::{self, Tooltip as _};

/// Draws the Track region and the Net block for the selection, pushing what their edits
/// run onto `out`.
pub(super) fn show(
    ui: &mut Ui,
    shell: &Shell,
    project: &Project,
    units: Units,
    out: &mut Vec<Out>,
) {
    if let Some(track) = TrackProperties::of(project, shell.selection()) {
        region(ui, shell, "track", tr!("properties-region-track"), |ui| {
            track_rows(ui, shell, &track, units, out);
        });
    }
    let delta_t = shell.preferences().pcb.net_info.delta_t;
    if let Some(block) = net_block(project, shell.selection(), delta_t) {
        region(ui, shell, "net", tr!("properties-region-net"), |ui| {
            net_rows(ui, &block, units, out);
        });
    }
}

fn track_rows(
    ui: &mut Ui,
    shell: &Shell,
    track: &TrackProperties,
    units: Units,
    out: &mut Vec<Out>,
) {
    let mut edits = Vec::new();
    widgets::form("properties-track").show(ui, |ui| {
        if let Some((start, end)) = track.ends {
            for (key, label, at, is_start) in [
                ("start", tr!("properties-track-start"), start, true),
                ("end", tr!("properties-track-end"), end, false),
            ] {
                for (axis, value) in [("x", at.x), ("y", at.y)] {
                    let name = format!("{label} {}", axis.to_uppercase());
                    let shown = Shared::Same(crate::pcb::view::format_length(value, units));
                    let key = format!("track-{key}-{axis}");
                    if let Some(text) = text_field(ui, &key, &name, &shown, true)
                        && let Some(v) = parse_length(&text, units)
                    {
                        let p = if axis == "x" {
                            Point::new(v, at.y)
                        } else {
                            Point::new(at.x, v)
                        };
                        edits.push(if is_start {
                            TrackEdit::Start(p)
                        } else {
                            TrackEdit::End(p)
                        });
                    }
                    ui.end_row();
                }
            }
        }
        ui.add(widgets::form_name(tr!("properties-track-layer")));
        let selected = match &track.layer {
            Shared::Same(l) => TrackProperties::name_in(&track.layers, l),
            Shared::Mixed => MIXED.to_owned(),
        };
        let width = value_width(ui);
        let response = widgets::dropdown("properties-track-layer", selected)
            .width(width)
            .max_width(width)
            .show(ui, |ui| {
                for (uuid, name) in &track.layers {
                    let on = track.layer.same() == Some(uuid);
                    if ui.add(widgets::choice(on, name.as_str())).clicked() && !on {
                        edits.push(TrackEdit::Layer(*uuid));
                    }
                }
            })
            .response;
        let name = tr!("properties-track-layer");
        response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, name.clone()));
        ui.end_row();

        ui.add(widgets::form_name(tr!("properties-track-net")));
        ui.horizontal(|ui| {
            let selected = match &track.net {
                Shared::Same(Some(n)) => TrackProperties::name_in(&track.nets, n),
                Shared::Same(None) => tr!("properties-track-no-net"),
                Shared::Mixed => MIXED.to_owned(),
            };
            // The Net and its pick button share the value's width.
            let pick = widgets::button(tr!("properties-track-pick-net"));
            let gap = ui.tokens().space.gap;
            let width = (value_width(ui) - pick.width(ui) - gap).max(MIN_NET_WIDTH);
            let response = widgets::dropdown("properties-track-net", selected)
                .width(width)
                .max_width(width)
                .show(ui, |ui| {
                    let none = track.net.same() == Some(&None);
                    let no_net = widgets::choice(none, tr!("properties-track-no-net"));
                    if ui.add(no_net).clicked() && !none {
                        edits.push(TrackEdit::Net(None));
                    }
                    for (uuid, name) in &track.nets {
                        let on = track.net.same() == Some(&Some(*uuid));
                        if ui.add(widgets::choice(on, name.as_str())).clicked() && !on {
                            edits.push(TrackEdit::Net(Some(*uuid)));
                        }
                    }
                })
                .response;
            let name = tr!("properties-track-net");
            response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, name.clone()));
            let waiting = !shell.net_pick().is_empty();
            let pick = ui
                .add(pick.on(waiting))
                .tooltip(tr!("properties-track-pick-net-hint"));
            if pick.clicked() {
                let tracks = if waiting {
                    Vec::new()
                } else {
                    track.tracks.clone()
                };
                out.push(Out::Run(
                    "view.properties.pick_net",
                    json!({ "tracks": tracks }),
                ));
            }
        });
        ui.end_row();
    });
    for edit in edits {
        let (id, params) = track.command(&edit);
        out.push(Out::Run(id, params));
    }
}

fn net_rows(ui: &mut Ui, block: &NetBlock, units: Units, out: &mut Vec<Out>) {
    let info = &block.info;
    widgets::form("properties-net-block").show(ui, |ui| {
        ui.add(widgets::form_name(tr!("properties-net-name")));
        ui.horizontal(|ui| {
            let color = info.color.as_deref();
            if let Some(Rgba([r, g, b, _])) = color.and_then(crate::pcb::view::parse_hex) {
                let response = ui.add(widgets::swatch_rgb(r, g, b));
                let name = tr!("properties-net-color");
                response.widget_info(|| WidgetInfo::labeled(Role::Image, true, name.clone()));
            }
            ui.label(&info.name);
        });
        ui.end_row();

        ui.add(widgets::form_name(tr!("properties-net-class")));
        ui.label(if block.classes.is_empty() {
            tr!("properties-default-net-class")
        } else {
            block.classes.join(", ")
        });
        ui.end_row();

        ui.add(widgets::form_name(tr!("properties-net-loop-removal")));
        let mut on = info.loop_removal;
        let response = ui.add(widgets::checkbox(&mut on, ""));
        let name = tr!("properties-net-loop-removal");
        response.widget_info(|| WidgetInfo::selected(Role::CheckBox, true, on, name.clone()));
        if response.changed() {
            out.push(Out::Run(
                "pcb.net.set_routing",
                json!({"nets": [info.net], "loop_removal": on}),
            ));
        }
        ui.end_row();

        ui.add(widgets::form_name(tr!("properties-net-routed-length")));
        ui.label(crate::pcb::view::format_length(info.routed_length, units));
        ui.end_row();

        ui.add(widgets::form_name(tr!("properties-net-width")));
        ui.label(crate::pcb::view::format_length(info.width, units));
        ui.end_row();

        if let Some(r) = info.track_resistance {
            ui.add(widgets::form_name(tr!("properties-net-resistance")));
            ui.label(format!("{} mΩ", round3(r * 1000.0)));
            ui.end_row();
        }

        ui.add(widgets::form_name(tr!("properties-net-current")));
        ui.label(tr!(
            "properties-net-current-value",
            amps = round3(info.max_current_estimate).to_string()
        ))
        .tooltip(tr!("properties-net-current-hint"));
        ui.end_row();

        let label = tr!("properties-net-delta-t");
        let shown = Shared::Same(format!("{} °C", round3(info.delta_t)));
        if let Some(text) = text_field(ui, "net-delta-t", &label, &shown, true)
            && let Some(v) = parse_delta_t(&text)
        {
            out.push(Out::Run(
                "app.preferences.set",
                json!({"key": "pcb.net_info.delta_t", "value": v}),
            ));
        }
        ui.end_row();
    });
}

/// A typed temperature rise: a positive number, with or without °C.
fn parse_delta_t(text: &str) -> Option<f64> {
    let v: f64 = text
        .trim()
        .trim_end_matches("°C")
        .trim_end_matches('C')
        .trim()
        .parse()
        .ok()?;
    (v > 0.0 && v.is_finite()).then_some(v)
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}
