//! The Layers panel: for the board, each Layer's visibility, colour and whether it is the
//! active one; the Inactive-Layer mode; the view from the bottom; visibility per object
//! type; the Ratsnest, as a whole and per Net; units and snap grid. Drawing only: every
//! change is a Command (a view Command, or `app.preferences.set` for a colour), returned
//! for the window to run after the frame.

use egui::color_picker::{Alpha, color_edit_button_srgba};
use egui::{Color32, ComboBox, Id, RadioButton, Role, ScrollArea, Ui, WidgetInfo};
use reda_engine::model::{Layer, LayerKind, ObjectType, Side, Uuid};
use reda_engine::preferences::{InactiveLayerMode, Units};
use reda_engine::render::{BoardColor, ColorKey, Rgba};
use serde_json::{Value, json};

use crate::grid::pcb_presets;
use crate::pcb::view::{VIEW_TYPES, color_key, format_length, to_hex};
use crate::shell::Shell;
use crate::{theme, tr};

/// A Command for the window to run.
pub type Invocation = (String, Value);

/// Draws the panel into `ui`.
pub fn show(ui: &mut Ui, shell: &Shell) -> Vec<Invocation> {
    let mut out = Vec::new();
    let Some(pcb) = shell.engine().project().and_then(|p| p.pcb()) else {
        ui.add_space(8.0);
        ui.vertical_centered(|ui| ui.weak(tr!("layers-no-pcb")));
        return out;
    };
    let view = shell.board_view();
    let layers = &view.layers;
    let active = shell.engine().project().and_then(|p| view.active_layer(p));
    let mut run = |id: &str, params: Value| out.push((id.to_owned(), params));

    ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.horizontal(|ui| {
            let mode = layers
                .inactive_mode
                .unwrap_or(shell.preferences().display.inactive_layer_mode);
            let mut chosen = mode;
            ComboBox::from_label(tr!("layers-inactive"))
                .selected_text(mode_title(mode))
                .show_ui(ui, |ui| {
                    for m in [
                        InactiveLayerMode::Normal,
                        InactiveLayerMode::Dimmed,
                        InactiveLayerMode::Hidden,
                    ] {
                        ui.selectable_value(&mut chosen, m, mode_title(m));
                    }
                });
            if chosen != mode {
                run("pcb.view.inactive_layers", json!({ "mode": chosen }));
            }
        });
        let mut bottom = layers.from_bottom;
        if ui
            .checkbox(&mut bottom, tr!("layers-from-bottom"))
            .changed()
        {
            run("pcb.view.from_bottom", json!({ "from_bottom": bottom }));
        }
        ui.separator();

        let canvas_theme = theme::canvas_theme(ui.ctx().theme());
        let palette = shell.palette().palette(canvas_theme);
        for layer in &pcb.layers().layers {
            ui.horizontal(|ui| {
                let mut visible = !layers.hidden.contains(&layer.uuid);
                let name = layer_name(layer);
                let check = ui.checkbox(&mut visible, "");
                let label = tr!("layers-show", layer = name.as_str());
                check.widget_info(|| WidgetInfo::labeled(Role::CheckBox, true, label.clone()));
                if check.changed() {
                    run(
                        "pcb.layer.show",
                        json!({"layer": layer.uuid, "visible": visible}),
                    );
                }
                let key = ColorKey::Board(BoardColor::layer(layer.kind, layer.side));
                if let Some(color) = colour_swatch(ui, layer, palette.color(key), canvas_theme) {
                    run("app.preferences.set", color);
                }
                let radio = ui.add(RadioButton::new(active == Some(layer.uuid), name.as_str()));
                if radio.clicked() && active != Some(layer.uuid) {
                    run("pcb.layer.activate", json!({ "layer": layer.uuid }));
                }
            });
        }
        ui.separator();

        ui.label(tr!("layers-objects"));
        for kind in VIEW_TYPES {
            let mut visible = !layers.hidden_types.contains(&kind);
            if ui.checkbox(&mut visible, type_title(kind)).changed() {
                run(
                    "pcb.view.object_type.show",
                    json!({"type": kind, "visible": visible}),
                );
            }
        }
        ui.separator();

        let mut ratsnest = !layers.ratsnest.hidden;
        if ui.checkbox(&mut ratsnest, tr!("layers-ratsnest")).changed() {
            run("pcb.ratsnest.show", json!({ "visible": ratsnest }));
        }
        let mut nets: Vec<(&str, Uuid)> = pcb.nets().map(|n| (n.name.as_str(), n.uuid)).collect();
        nets.sort();
        egui::CollapsingHeader::new(tr!("layers-ratsnest-nets"))
            .id_salt("layers-ratsnest-nets")
            .show(ui, |ui| {
                for (name, net) in nets {
                    let mut shown = !layers.ratsnest.hidden_nets.contains(&net);
                    let check = ui.add_enabled(ratsnest, egui::Checkbox::new(&mut shown, name));
                    if check.changed() {
                        run("pcb.ratsnest.show", json!({"net": net, "visible": shown}));
                    }
                }
            });
        ui.separator();

        ui.horizontal(|ui| {
            ui.label(tr!("layers-units"));
            for (units, title) in [(Units::Mm, "mm"), (Units::Mil, "mil")] {
                if ui.selectable_label(view.units == units, title).clicked() && view.units != units
                {
                    run("view.units.toggle", json!({}));
                }
            }
        });
        ui.horizontal(|ui| {
            let current = shell.grid(pcb.uuid);
            let mut grid = current;
            ComboBox::from_label(tr!("layers-grid"))
                .selected_text(format_length(current, view.units))
                .show_ui(ui, |ui| {
                    for g in pcb_presets(view.units) {
                        ui.selectable_value(&mut grid, *g, format_length(*g, view.units));
                    }
                });
            if grid != current {
                run(
                    "view.grid.set",
                    json!({ "document": pcb.uuid, "grid": grid }),
                );
            }
        });
    });
    out
}

/// The colour button of `layer`: returns the `app.preferences.set` params once a new
/// colour has been picked (when the pointer lets go, so a drag writes once).
fn colour_swatch(
    ui: &mut Ui,
    layer: &Layer,
    current: Rgba,
    theme: reda_engine::render::Theme,
) -> Option<Value> {
    let theme_name = match theme {
        reda_engine::render::Theme::Dark => "dark",
        reda_engine::render::Theme::Light => "light",
    };
    let pending_id = Id::NULL.with(("layers-colour", layer.uuid));
    let [r, g, b, _] = current.0;
    let mut color: Color32 = ui
        .data(|d| d.get_temp(pending_id))
        .unwrap_or(Color32::from_rgb(r, g, b));
    let response = color_edit_button_srgba(ui, &mut color, Alpha::Opaque);
    let name = layer_name(layer);
    let label = tr!("layers-colour", layer = name.as_str());
    response.widget_info(|| WidgetInfo::labeled(Role::Button, true, label.clone()));
    if response.changed() {
        ui.data_mut(|d| d.insert_temp(pending_id, color));
    }
    let pending: Option<Color32> = ui.data(|d| d.get_temp(pending_id));
    let settled = !ui.input(|i| i.pointer.any_down());
    match pending {
        Some(c) if settled => {
            ui.data_mut(|d| d.remove::<Color32>(pending_id));
            let hex = to_hex(Rgba::rgb(c.r(), c.g(), c.b()));
            let key = format!(
                "display.layer_colors.{theme_name}.{}",
                color_key(layer.kind, layer.side)
            );
            Some(json!({ "key": key, "value": hex }))
        }
        _ => None,
    }
}

/// "Top Copper", "Bottom Silkscreen", "Board Outline"...
pub fn layer_name(layer: &Layer) -> String {
    let kind = match layer.kind {
        LayerKind::Copper => tr!("layer-copper"),
        LayerKind::Silkscreen => tr!("layer-silkscreen"),
        LayerKind::SolderMask => tr!("layer-solder-mask"),
        LayerKind::Paste => tr!("layer-paste"),
        LayerKind::Courtyard => tr!("layer-courtyard"),
        LayerKind::Assembly => tr!("layer-assembly"),
        LayerKind::BoardOutline => tr!("layer-board-outline"),
        LayerKind::Mechanical => tr!("layer-mechanical"),
        LayerKind::Documentation => tr!("layer-documentation"),
    };
    match layer.side {
        Some(Side::Top) => tr!("layer-top", kind = kind.as_str()),
        Some(Side::Bottom) => tr!("layer-bottom", kind = kind.as_str()),
        None => kind,
    }
}

fn mode_title(mode: InactiveLayerMode) -> String {
    match mode {
        InactiveLayerMode::Normal => tr!("layers-inactive-normal"),
        InactiveLayerMode::Dimmed => tr!("layers-inactive-dimmed"),
        InactiveLayerMode::Hidden => tr!("layers-inactive-hidden"),
    }
}

fn type_title(kind: ObjectType) -> String {
    match kind {
        ObjectType::FootprintInstance => tr!("layers-type-footprints"),
        ObjectType::Track => tr!("layers-type-tracks"),
        ObjectType::Via => tr!("layers-type-vias"),
        ObjectType::Zone => tr!("layers-type-zones"),
        ObjectType::Keepout => tr!("layers-type-keepouts"),
        ObjectType::FreePad => tr!("layers-type-free-pads"),
        ObjectType::PcbText => tr!("layers-type-text"),
        ObjectType::PcbGraphic => tr!("layers-type-graphics"),
        _ => format!("{kind:?}"),
    }
}
