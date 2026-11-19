//! The Layers panel: for the board, each Layer's visibility, colour and whether it is the
//! active one; the Inactive-Layer mode; the view from the bottom; visibility per object
//! type; the Ratsnest, as a whole and per Net; units and snap grid. Drawing only: every
//! change is a Command (a view Command, or `app.preferences.set` for a colour), returned
//! for the window to run after the frame.

use egui::{Color32, Id, Role, ScrollArea, Ui, WidgetInfo};
use reda_engine::model::{Layer, LayerKind, ObjectType, PcbDocument, Side, Uuid};
use reda_engine::preferences::{InactiveLayerMode, Units};
use reda_engine::render::{BoardColor, ColorKey, Rgba};
use serde_json::{Value, json};

use crate::layout::PanelId;
use crate::pcb::layer_control::{chip_name, ordered, role_name, set_buttons};
use crate::pcb::view::{VIEW_TYPES, format_length, layer_key, to_hex};
use crate::shell::Shell;
use crate::theme::TokensExt as _;
use crate::widgets::{self, Tooltip as _};
use crate::{theme, tr};

/// The side of the pin mark on a pinned Layer's eye.
const PIN_MARK: f32 = 4.0;

/// A Command for the window to run.
pub type Invocation = (String, Value);

/// Draws the panel into `ui`.
pub fn show(ui: &mut Ui, shell: &Shell) -> Vec<Invocation> {
    let mut out = Vec::new();
    let Some(pcb) = shell.engine().project().and_then(|p| p.pcb()) else {
        ui.add_space(ui.tokens().space.gap);
        ui.vertical_centered(|ui| ui.weak(tr!("layers-no-pcb")));
        return out;
    };
    let view = shell.board_view();
    let layers = &view.layers;
    let active = shell.engine().project().and_then(|p| view.active_layer(p));
    let t = ui.tokens();
    let focused = shell.panel_focused(PanelId::Layers);
    ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.horizontal(|ui| {
            let mode = layers
                .inactive_mode
                .unwrap_or(shell.preferences().display.inactive_layer_mode);
            let mut chosen = mode;
            let label = tr!("layers-inactive");
            let dropdown = widgets::dropdown("layers-inactive", mode_title(mode));
            let shown = dropdown.show(ui, |ui| {
                for m in [
                    InactiveLayerMode::Normal,
                    InactiveLayerMode::Dimmed,
                    InactiveLayerMode::Hidden,
                ] {
                    widgets::choice_value(ui, &mut chosen, m, mode_title(m));
                }
            });
            shown
                .response
                .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label.clone()));
            ui.label(label.as_str());
            if chosen != mode {
                out.push(("pcb.view.inactive_layers".into(), json!({ "mode": chosen })));
            }
        });
        let mut bottom = layers.from_bottom;
        let from_bottom = widgets::checkbox(&mut bottom, tr!("layers-from-bottom"));
        if ui.add(from_bottom).changed() {
            out.push((
                "pcb.view.from_bottom".into(),
                json!({ "from_bottom": bottom }),
            ));
        }
        ui.label(tr!("layers-sets"));
        ui.horizontal_wrapped(|ui| set_buttons(ui, shell, &mut out));
        ui.separator();

        // A group eye per Layer kind the board has.
        ui.label(tr!("layers-kinds"));
        let mut kinds: Vec<LayerKind> = Vec::new();
        for layer in ordered(pcb) {
            if !kinds.contains(&layer.kind) {
                kinds.push(layer.kind);
            }
        }
        ui.horizontal_wrapped(|ui| {
            for kind in kinds {
                let of_kind = || pcb.layers().layers.iter().filter(move |l| l.kind == kind);
                let mut all = of_kind().all(|l| !layers.hidden.contains(&l.uuid));
                let title = kind_title(kind);
                let check = ui.add(widgets::checkbox(&mut all, title.as_str()));
                let label = tr!("layers-show-kind", kind = title.as_str());
                check.widget_info(|| WidgetInfo::labeled(Role::CheckBox, true, label.clone()));
                if check.changed() {
                    out.push((
                        "pcb.layer.show_kind".into(),
                        json!({"kind": kind, "visible": all}),
                    ));
                }
            }
        });
        ui.separator();

        let canvas_theme = theme::canvas_theme(ui.ctx().theme());
        let palette = shell.palette().palette(canvas_theme);
        let mut layer_row = |ui: &mut Ui, layer: &Layer| {
            ui.horizontal(|ui| {
                let mut visible = !layers.hidden.contains(&layer.uuid);
                let pinned = layers.pinned.contains(&layer.uuid);
                let name = chip_name(pcb, layer);
                let check = ui
                    .add(widgets::checkbox(&mut visible, ""))
                    .tooltip(tr!("layers-pin-hint"));
                let label = if pinned {
                    tr!("layers-show-pinned", layer = name.as_str())
                } else {
                    tr!("layers-show", layer = name.as_str())
                };
                check.widget_info(|| WidgetInfo::labeled(Role::CheckBox, true, label.clone()));
                if pinned {
                    // A pin mark beside the eye: a small filled square on the corner of
                    // the check box's mark.
                    let r = egui::Rect::from_center_size(
                        check.rect.center(),
                        egui::Vec2::splat(widgets::MARK),
                    );
                    let mark =
                        egui::Rect::from_center_size(r.right_top(), egui::Vec2::splat(PIN_MARK));
                    ui.painter().rect_filled(mark, 0.0, t.color.accent.text);
                }
                if check.changed() {
                    // Mod+click on the eye pins the Layer instead.
                    if ui.input(|i| i.modifiers.command) {
                        out.push(("pcb.layer.pin".into(), json!({ "layer": layer.uuid })));
                    } else {
                        out.push((
                            "pcb.layer.show".into(),
                            json!({"layer": layer.uuid, "visible": visible}),
                        ));
                    }
                }
                let key = ColorKey::Board(BoardColor::for_layer(pcb, layer));
                if let Some(color) =
                    colour_swatch(ui, pcb, layer, &name, palette.color(key), canvas_theme)
                {
                    out.push(("app.preferences.set".into(), color));
                }
                // The row is the active Layer's when it is selected.
                let row = widgets::list_row(name.as_str())
                    .selected(active == Some(layer.uuid))
                    .muted(!visible)
                    .panel_focused(focused);
                let row = if let Some(role) = role_name(layer) {
                    row.trailing(role)
                } else {
                    row
                };
                let row = ui.add(row);
                crate::pcb::layer_hover::chip(&row, layer.uuid);
                // Mod+click on a Layer's name highlights its content, as on a chip.
                if row.clicked() && ui.input(|i| i.modifiers.command) {
                    out.push(("pcb.layer.highlight".into(), json!({ "layer": layer.uuid })));
                } else if row.clicked() && (!visible || active != Some(layer.uuid)) {
                    out.push(("pcb.layer.activate".into(), json!({ "layer": layer.uuid })));
                }
            });
        };
        widgets::section(tr!("layers-copper-group"))
            .id_salt("copper-group")
            .default_open(true)
            .show(ui, |ui| {
                for layer in pcb.copper_layers() {
                    layer_row(ui, layer);
                }
            });
        for layer in pcb
            .layers()
            .layers
            .iter()
            .filter(|l| l.kind != LayerKind::Copper)
        {
            layer_row(ui, layer);
        }
        ui.separator();

        ui.label(tr!("layers-objects"));
        for kind in VIEW_TYPES {
            let mut visible = !layers.hidden_types.contains(&kind);
            let check = widgets::checkbox(&mut visible, type_title(kind));
            if ui.add(check).changed() {
                out.push((
                    "pcb.view.object_type.show".into(),
                    json!({"type": kind, "visible": visible}),
                ));
            }
        }
        ui.separator();

        let mut net_colors = layers.net_colors;
        let check = widgets::checkbox(&mut net_colors, tr!("layers-net-colors"));
        if ui.add(check).changed() {
            out.push(("pcb.view.net_colors".into(), json!({ "on": net_colors })));
        }
        let labels = layers.labels;
        for (key, value, param) in [
            (tr!("layers-pad-nets"), labels.pad_nets, "pad_nets"),
            (tr!("layers-via-nets"), labels.via_nets, "via_nets"),
            (
                tr!("layers-repeated-net-names"),
                labels.repeated_net_names,
                "repeated_net_names",
            ),
        ] {
            let mut on = value;
            if ui.add(widgets::checkbox(&mut on, key)).changed() {
                out.push(("pcb.view.labels".into(), json!({ param: on })));
            }
        }
        ui.separator();

        let mut ratsnest = !layers.ratsnest.hidden;
        let check = widgets::checkbox(&mut ratsnest, tr!("layers-ratsnest"));
        if ui.add(check).changed() {
            out.push(("pcb.ratsnest.show".into(), json!({ "visible": ratsnest })));
        }
        let mut nets: Vec<(&str, Uuid)> = pcb.nets().map(|n| (n.name.as_str(), n.uuid)).collect();
        nets.sort();
        widgets::section(tr!("layers-ratsnest-nets"))
            .id_salt("layers-ratsnest-nets")
            .show(ui, |ui| {
                for (name, net) in nets {
                    let mut shown = !layers.ratsnest.hidden_nets.contains(&net);
                    let check = ui.add_enabled(ratsnest, widgets::checkbox(&mut shown, name));
                    if check.changed() {
                        out.push((
                            "pcb.ratsnest.show".into(),
                            json!({"net": net, "visible": shown}),
                        ));
                    }
                }
            });
        ui.separator();

        ui.horizontal(|ui| {
            ui.label(tr!("layers-units"));
            let mut units = view.units;
            let options = [(Units::Mm, "mm"), (Units::Mil, "mil")];
            if ui.add(widgets::segmented(&mut units, &options)).changed() {
                out.push(("view.units.toggle".into(), json!({})));
            }
        });
        ui.horizontal(|ui| {
            let current = shell.grid(pcb.uuid);
            let mut grid = current;
            let label = tr!("layers-grid");
            let dropdown = widgets::dropdown("layers-grid", format_length(current, view.units));
            let shown = dropdown.show(ui, |ui| {
                let rows = shell.grids().rows(shell.engine().elements(), pcb.uuid);
                let tables = shell.grids().tables(crate::bars::DocumentType::Pcb).clone();
                let imperial = view.units == reda_engine::preferences::Units::Mil;
                for g in rows
                    .iter()
                    .map(|r| r.snap.0)
                    .filter(|g| tables.is_imperial(*g) == imperial)
                {
                    widgets::choice_value(ui, &mut grid, g, format_length(g, view.units));
                }
            });
            shown
                .response
                .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label.clone()));
            ui.label(label.as_str());
            if grid != current {
                out.push((
                    "view.grid.set".into(),
                    json!({ "document": pcb.uuid, "grid": grid }),
                ));
            }
        });
    });
    out
}

/// The colour button of `layer`: returns the `app.preferences.set` params once a new
/// colour has been picked (when the pointer lets go, so a drag writes once).
fn colour_swatch(
    ui: &mut Ui,
    pcb: &PcbDocument,
    layer: &Layer,
    name: &str,
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
        .unwrap_or_else(|| widgets::data_color(r, g, b));
    let label = tr!("layers-colour", layer = name);
    let response = ui.add(widgets::color_button(&mut color, label));
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
                layer_key(pcb.layers(), layer)
            );
            Some(json!({ "key": key, "value": hex }))
        }
        _ => None,
    }
}

/// "Top Copper", "Bottom Silkscreen", "Board Outline"... Inner copper is named by its copper
/// position and optional user name.
pub fn layer_name(pcb: &PcbDocument, layer: &Layer) -> String {
    if let Some(name) = pcb.inner_layer_label(layer.uuid) {
        return name;
    }
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

/// A Layer kind's name for its group eye: "Copper", "Silkscreen"...
fn kind_title(kind: LayerKind) -> String {
    match kind {
        LayerKind::Copper => tr!("layer-copper"),
        LayerKind::Silkscreen => tr!("layer-silkscreen"),
        LayerKind::SolderMask => tr!("layer-solder-mask"),
        LayerKind::Paste => tr!("layer-paste"),
        LayerKind::Courtyard => tr!("layer-courtyard"),
        LayerKind::Assembly => tr!("layer-assembly"),
        LayerKind::BoardOutline => tr!("layer-board-outline"),
        LayerKind::Mechanical => tr!("layer-mechanical"),
        LayerKind::Documentation => tr!("layer-documentation"),
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
        ObjectType::ZoneCutout => tr!("layers-type-zone-cutouts"),
        ObjectType::FreePad => tr!("layers-type-free-pads"),
        ObjectType::PcbText => tr!("layers-type-text"),
        ObjectType::PcbGraphic => tr!("layers-type-graphics"),
        _ => format!("{kind:?}"),
    }
}
