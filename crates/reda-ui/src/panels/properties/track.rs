//! The Track region (Start and End XY, Layer, Net with a canvas pick) and the Net block
//! (Net, Net Class, colour, `loop_removal`, routed length, resistance and the current
//! estimate with its editable ΔT) of the Properties panel; see
//! [`crate::properties::track`].

use egui::{ComboBox, Grid, Role, Ui, WidgetInfo};
use reda_engine::model::Project;
use reda_engine::model::geom::Point;
use reda_engine::preferences::Units;
use serde_json::json;

use super::{Out, parse_length, region, text_field};
use crate::properties::track::{NetBlock, TrackEdit, TrackProperties, net_block};
use crate::properties::{MIXED, Shared};
use crate::shell::Shell;
use crate::tr;

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
    Grid::new("properties-track").num_columns(2).show(ui, |ui| {
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
        ui.label(tr!("properties-track-layer"));
        let selected = match &track.layer {
            Shared::Same(l) => TrackProperties::name_in(&track.layers, l),
            Shared::Mixed => MIXED.to_owned(),
        };
        let response = ComboBox::from_id_salt("properties-track-layer")
            .selected_text(selected)
            .show_ui(ui, |ui| {
                for (uuid, name) in &track.layers {
                    let on = track.layer.same() == Some(uuid);
                    if ui.selectable_label(on, name).clicked() && !on {
                        edits.push(TrackEdit::Layer(*uuid));
                    }
                }
            })
            .response;
        let name = tr!("properties-track-layer");
        response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, name.clone()));
        ui.end_row();

        ui.label(tr!("properties-track-net"));
        ui.horizontal(|ui| {
            let selected = match &track.net {
                Shared::Same(Some(n)) => TrackProperties::name_in(&track.nets, n),
                Shared::Same(None) => tr!("properties-track-no-net"),
                Shared::Mixed => MIXED.to_owned(),
            };
            let response = ComboBox::from_id_salt("properties-track-net")
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    let none = track.net.same() == Some(&None);
                    if ui
                        .selectable_label(none, tr!("properties-track-no-net"))
                        .clicked()
                        && !none
                    {
                        edits.push(TrackEdit::Net(None));
                    }
                    for (uuid, name) in &track.nets {
                        let on = track.net.same() == Some(&Some(*uuid));
                        if ui.selectable_label(on, name).clicked() && !on {
                            edits.push(TrackEdit::Net(Some(*uuid)));
                        }
                    }
                })
                .response;
            let name = tr!("properties-track-net");
            response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, name.clone()));
            let waiting = !shell.net_pick().is_empty();
            let pick = ui
                .selectable_label(waiting, tr!("properties-track-pick-net"))
                .on_hover_text(tr!("properties-track-pick-net-hint"));
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
    Grid::new("properties-net-block")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label(tr!("properties-net-name"));
            ui.horizontal(|ui| {
                if let Some(color) = info.color.as_deref().and_then(parse_color) {
                    let (rect, response) =
                        ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 2.0, color);
                    let name = tr!("properties-net-color");
                    response.widget_info(|| WidgetInfo::labeled(Role::Image, true, name.clone()));
                }
                ui.label(&info.name);
            });
            ui.end_row();

            ui.label(tr!("properties-net-class"));
            ui.label(if block.classes.is_empty() {
                tr!("properties-default-net-class")
            } else {
                block.classes.join(", ")
            });
            ui.end_row();

            ui.label(tr!("properties-net-loop-removal"));
            let mut on = info.loop_removal;
            let response = ui.checkbox(&mut on, "");
            let name = tr!("properties-net-loop-removal");
            response.widget_info(|| WidgetInfo::selected(Role::CheckBox, true, on, name.clone()));
            if response.changed() {
                out.push(Out::Run(
                    "pcb.net.set_routing",
                    json!({"nets": [info.net], "loop_removal": on}),
                ));
            }
            ui.end_row();

            ui.label(tr!("properties-net-routed-length"));
            ui.label(crate::pcb::view::format_length(info.routed_length, units));
            ui.end_row();

            ui.label(tr!("properties-net-width"));
            ui.label(crate::pcb::view::format_length(info.width, units));
            ui.end_row();

            if let Some(r) = info.track_resistance {
                ui.label(tr!("properties-net-resistance"));
                ui.label(format!("{} mΩ", round3(r * 1000.0)));
                ui.end_row();
            }

            ui.label(tr!("properties-net-current"));
            ui.label(tr!(
                "properties-net-current-value",
                amps = round3(info.max_current_estimate).to_string()
            ))
            .on_hover_text(tr!("properties-net-current-hint"));
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

/// `#rrggbb` as a colour.
fn parse_color(text: &str) -> Option<egui::Color32> {
    let hex = text.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(hex, 16).ok()?;
    Some(egui::Color32::from_rgb(
        (v >> 16) as u8,
        (v >> 8) as u8,
        v as u8,
    ))
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
