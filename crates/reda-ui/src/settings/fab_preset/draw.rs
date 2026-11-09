//! Drawing a Fab Preset page into a settings window or the Preferences window.

use egui::{Button, ComboBox, Grid, Role, Ui, WidgetInfo};
use reda_engine::model::{
    BomLayout, BottomPosition, BottomRotation, Column, ColumnSource, CplUnits, FileNaming,
    FileOrigin, LayerFileName, NotFittedBom, PasteExpansion, SlotCommand,
};

use super::{Field, PresetEditor, PresetPage};
use crate::settings::field;
use crate::tr;

const FIELD_WIDTH: f32 = 220.0;

/// Draws `page` of the preset in `editor`.
pub fn page(ui: &mut Ui, editor: &mut PresetEditor, page: PresetPage) {
    match page {
        PresetPage::Rules => rules(ui, editor),
        PresetPage::Stackup => stackup(ui, editor),
        PresetPage::Gerber => gerber(ui, editor),
        PresetPage::Drill => drill(ui, editor),
        PresetPage::Bom => bom(ui, editor),
        PresetPage::Cpl => cpl(ui, editor),
        PresetPage::Packaging => packaging(ui, editor),
    }
}

/// A grid of label and value rows.
fn rows(ui: &mut Ui, id: &str, add: impl FnOnce(&mut Ui)) {
    Grid::new(id)
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, add);
}

/// A typed field's row.
fn typed(ui: &mut Ui, editor: &mut PresetEditor, f: Field, label: String) {
    ui.label(&label);
    let mut text = editor.text(f);
    let problem = editor.problem(f);
    let response = field(ui, &label, &mut text, FIELD_WIDTH, problem.as_ref());
    if response.changed() {
        editor.set_text(f, &text);
    }
    ui.end_row();
}

/// A drop-down's row.
fn choice<T: PartialEq + Copy>(ui: &mut Ui, label: String, value: &mut T, options: &[(T, String)]) {
    ui.label(&label);
    let shown = options
        .iter()
        .find(|(v, _)| v == value)
        .map(|(_, l)| l.clone())
        .unwrap_or_default();
    let combo = ComboBox::from_id_salt(&label)
        .selected_text(shown)
        .width(FIELD_WIDTH)
        .show_ui(ui, |ui| {
            for (option, text) in options {
                ui.selectable_value(value, *option, text);
            }
        });
    let announced = label.clone();
    combo
        .response
        .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, announced.clone()));
    ui.end_row();
}

/// A checkbox's row.
fn check(ui: &mut Ui, label: String, value: &mut bool) {
    ui.label("");
    ui.checkbox(value, label);
    ui.end_row();
}

/// A value shown read-only, where another page sets it.
fn read_only(ui: &mut Ui, label: String, value: String, set_on: String) {
    ui.label(&label);
    ui.horizontal(|ui| {
        ui.strong(value);
        ui.weak(set_on);
    });
    ui.end_row();
}

fn note(ui: &mut Ui, text: String) {
    ui.add_space(6.0);
    ui.weak(text);
}

fn origin_options() -> [(FileOrigin, String); 2] {
    [
        (FileOrigin::Absolute, tr!("preset-origin-absolute")),
        (FileOrigin::Relative, tr!("preset-origin-relative")),
    ]
}

fn origin_text(origin: FileOrigin) -> String {
    origin_options()
        .into_iter()
        .find(|(o, _)| *o == origin)
        .map(|(_, t)| t)
        .unwrap_or_default()
}

fn rules(ui: &mut Ui, editor: &mut PresetEditor) {
    ui.label(tr!("preset-rules-intro"));
    rows(ui, "preset-rules", |ui| {
        for (f, label) in [
            (Field::Clearance, tr!("preset-clearance")),
            (Field::TrackWidth, tr!("preset-track-width")),
            (Field::ViaDiameter, tr!("preset-via-diameter")),
            (Field::ViaHole, tr!("preset-via-hole")),
            (Field::HoleMin, tr!("preset-hole-min")),
            (Field::HoleMax, tr!("preset-hole-max")),
            (Field::AnnularRing, tr!("preset-annular-ring")),
            (Field::HoleToHole, tr!("preset-hole-to-hole")),
            (Field::EdgeClearance, tr!("preset-edge-clearance")),
        ] {
            typed(ui, editor, f, label);
        }
        ui.strong(tr!("preset-preferred"));
        ui.end_row();
        for (f, label) in [
            (Field::PreferredTrackWidth, tr!("preset-preferred-track")),
            (
                Field::PreferredViaDiameter,
                tr!("preset-preferred-via-diameter"),
            ),
            (Field::PreferredViaHole, tr!("preset-preferred-via-hole")),
        ] {
            typed(ui, editor, f, label);
        }
        ui.strong(tr!("preset-mask-paste"));
        ui.end_row();
        typed(ui, editor, Field::MaskTop, tr!("preset-mask-top"));
        typed(ui, editor, Field::MaskBottom, tr!("preset-mask-bottom"));
        let mask = &mut editor.value_mut().rules.mask_expansion;
        check(ui, tr!("preset-mask-from-hole"), &mut mask.from_hole_edge);
        check(ui, tr!("preset-tent-top"), &mut mask.tent_vias_top);
        check(ui, tr!("preset-tent-bottom"), &mut mask.tent_vias_bottom);
        let paste = &mut editor.value_mut().rules.paste_expansion;
        let mut percent = matches!(paste, PasteExpansion::Percent { .. });
        let was = percent;
        choice(
            ui,
            tr!("preset-paste-mode"),
            &mut percent,
            &[
                (false, tr!("preset-paste-length")),
                (true, tr!("preset-paste-percent")),
            ],
        );
        if percent != was {
            *paste = if percent {
                PasteExpansion::Percent { percent: 0 }
            } else {
                PasteExpansion::Expansion {
                    expansion: Default::default(),
                }
            };
            editor.set_text(Field::Paste, &Field::Paste.text(editor.value()));
        }
        typed(ui, editor, Field::Paste, tr!("preset-paste"));
    });
}

fn stackup(ui: &mut Ui, editor: &mut PresetEditor) {
    rows(ui, "preset-stackup", |ui| {
        ui.strong(tr!("preset-seeds"));
        ui.end_row();
        typed(
            ui,
            editor,
            Field::SeedThickness,
            tr!("preset-seed-thickness"),
        );
        typed(ui, editor, Field::SeedCopper, tr!("preset-seed-copper"));
        ui.strong(tr!("preset-suggestions"));
        ui.end_row();
        for (f, label) in [
            (Field::SuggestThickness, tr!("preset-suggest-thickness")),
            (Field::SuggestCopper, tr!("preset-suggest-copper")),
            (Field::SuggestFinish, tr!("preset-suggest-finish")),
            (Field::SuggestMaskColour, tr!("preset-suggest-mask")),
            (Field::SuggestSilkColour, tr!("preset-suggest-silk")),
        ] {
            typed(ui, editor, f, label);
        }
    });
    note(ui, tr!("preset-seeds-note"));
    note(ui, tr!("preset-suggestions-note"));
}

fn gerber(ui: &mut Ui, editor: &mut PresetEditor) {
    let g = &mut editor.value_mut().gerber;
    rows(ui, "preset-gerber", |ui| {
        choice(
            ui,
            tr!("preset-naming"),
            &mut g.naming,
            &[
                (FileNaming::Classic, tr!("preset-naming-classic")),
                (FileNaming::Descriptive, tr!("preset-naming-descriptive")),
            ],
        );
        check(ui, tr!("preset-x2"), &mut g.x2_attributes);
        choice(
            ui,
            tr!("preset-file-origin"),
            &mut g.file_origin,
            &origin_options(),
        );
        ui.label("");
        ui.weak(tr!("preset-file-origin-note"));
        ui.end_row();
        check(
            ui,
            tr!("preset-outline-every"),
            &mut g.outline_on_every_plot,
        );
    });
    ui.add_space(6.0);
    ui.strong(tr!("preset-file-names"));
    file_names(ui, &mut g.file_names);
}

fn file_names(ui: &mut Ui, names: &mut Vec<LayerFileName>) {
    let mut remove = None;
    Grid::new("preset-file-names")
        .num_columns(3)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            ui.weak(tr!("preset-layer"));
            ui.weak(tr!("preset-file-name"));
            ui.end_row();
            for (i, row) in names.iter_mut().enumerate() {
                let n = i + 1;
                field(
                    ui,
                    &tr!("preset-layer-of", n = n),
                    &mut row.layer,
                    140.0,
                    None,
                );
                field(
                    ui,
                    &tr!("preset-file-name-of", n = n),
                    &mut row.name,
                    180.0,
                    None,
                );
                if ui
                    .button(tr!("preset-remove"))
                    .on_hover_text(tr!("preset-remove-name", n = n))
                    .clicked()
                {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
    if let Some(i) = remove {
        names.remove(i);
    }
    if ui.button(tr!("preset-add-file-name")).clicked() {
        names.push(LayerFileName {
            layer: String::new(),
            name: String::new(),
        });
    }
}

fn drill(ui: &mut Ui, editor: &mut PresetEditor) {
    let g = &mut editor.value_mut().gerber;
    rows(ui, "preset-drill", |ui| {
        choice(
            ui,
            tr!("preset-slot-command"),
            &mut g.slot_command,
            &[
                (SlotCommand::Route, tr!("preset-slot-route")),
                (SlotCommand::G85, tr!("preset-slot-g85")),
            ],
        );
        check(ui, tr!("preset-merge-drill"), &mut g.merge_drill);
        read_only(
            ui,
            tr!("preset-file-origin"),
            origin_text(g.file_origin),
            tr!("preset-set-on-gerber"),
        );
    });
    let names = if g.merge_drill {
        tr!("preset-drill-merged")
    } else {
        tr!("preset-drill-split")
    };
    note(ui, names);
}

/// The sources a column can take, with their names.
fn sources() -> Vec<(ColumnSource, String)> {
    use ColumnSource::*;
    [
        Line, Designator, Quantity, Value, Footprint, Mount, CentreX, CentreY, OriginX, OriginY,
        Pad1X, Pad1Y, Fitted, Side, Rotation,
    ]
    .into_iter()
    .map(|s| {
        let key = format!(
            "preset-source-{}",
            super::source_token(&s).replace('_', "-")
        );
        let text = crate::i18n::translate(&key, &[]);
        (s, text)
    })
    .collect()
}

/// A column grid: each row Source → Header, with up, down, add and remove.
fn columns(ui: &mut Ui, id: &str, columns: &mut Vec<Column>) {
    let options = sources();
    let mut action: Option<(usize, i8)> = None;
    Grid::new(id)
        .num_columns(5)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            ui.weak(tr!("preset-source"));
            ui.weak(tr!("preset-header"));
            ui.end_row();
            let count = columns.len();
            for (i, column) in columns.iter_mut().enumerate() {
                let n = i + 1;
                ui.horizontal(|ui| {
                    let label = tr!("preset-source-of", n = n);
                    let is_field = matches!(column.source, ColumnSource::Field(_));
                    let shown = if is_field {
                        tr!("preset-source-field")
                    } else {
                        options
                            .iter()
                            .find(|(s, _)| *s == column.source)
                            .map(|(_, t)| t.clone())
                            .unwrap_or_default()
                    };
                    let combo = ComboBox::from_id_salt((id, "source", i))
                        .selected_text(shown)
                        .width(130.0)
                        .show_ui(ui, |ui| {
                            for (source, text) in &options {
                                ui.selectable_value(&mut column.source, source.clone(), text);
                            }
                            if ui
                                .selectable_label(is_field, tr!("preset-source-field"))
                                .clicked()
                                && !is_field
                            {
                                column.source = ColumnSource::Field(column.header.clone());
                            }
                        });
                    combo
                        .response
                        .widget_info(|| WidgetInfo::labeled(Role::ComboBox, true, label.clone()));
                    if let ColumnSource::Field(name) = &mut column.source {
                        field(ui, &tr!("preset-field-of", n = n), name, 90.0, None);
                    }
                });
                field(
                    ui,
                    &tr!("preset-header-of", n = n),
                    &mut column.header,
                    160.0,
                    None,
                );
                let small = |ui: &mut Ui, on: bool, text: String, hint: String| {
                    ui.add_enabled(on, Button::new(text))
                        .on_hover_text(hint)
                        .clicked()
                };
                if small(ui, i > 0, tr!("preset-up"), tr!("preset-up-of", n = n)) {
                    action = Some((i, -1));
                }
                if small(
                    ui,
                    i + 1 < count,
                    tr!("preset-down"),
                    tr!("preset-down-of", n = n),
                ) {
                    action = Some((i, 1));
                }
                if small(
                    ui,
                    true,
                    tr!("preset-remove"),
                    tr!("preset-remove-of", n = n),
                ) {
                    action = Some((i, 0));
                }
                ui.end_row();
            }
        });
    match action {
        Some((i, 0)) => {
            columns.remove(i);
        }
        Some((i, -1)) => columns.swap(i, i - 1),
        Some((i, _)) => columns.swap(i, i + 1),
        None => {}
    }
    if ui.button(tr!("preset-add-column")).clicked() {
        columns.push(Column::new("", ColumnSource::Value));
    }
}

fn bom(ui: &mut Ui, editor: &mut PresetEditor) {
    ui.strong(tr!("preset-columns"));
    columns(
        ui,
        "preset-bom-columns",
        &mut editor.value_mut().bom.columns,
    );
    ui.add_space(6.0);
    rows(ui, "preset-bom", |ui| {
        let b = &mut editor.value_mut().bom;
        choice(
            ui,
            tr!("preset-layout"),
            &mut b.layout,
            &[
                (BomLayout::Grouped, tr!("preset-layout-grouped")),
                (BomLayout::Flat, tr!("preset-layout-flat")),
            ],
        );
        typed(ui, editor, Field::GroupBy, tr!("preset-group-by"));
        typed(ui, editor, Field::Sort, tr!("preset-sort"));
        let b = &mut editor.value_mut().bom;
        check(ui, tr!("preset-line-numbers"), &mut b.line_numbers.enabled);
        typed(ui, editor, Field::LineStart, tr!("preset-line-start"));
        typed(ui, editor, Field::LineStep, tr!("preset-line-step"));
        let b = &mut editor.value_mut().bom;
        let label = tr!("preset-designator-separator");
        ui.label(&label);
        field(ui, &label, &mut b.designator_separator, 60.0, None);
        ui.end_row();
        choice(
            ui,
            tr!("preset-not-fitted"),
            &mut b.not_fitted,
            &[
                (NotFittedBom::Omit, tr!("preset-not-fitted-omit")),
                (NotFittedBom::List, tr!("preset-not-fitted-list")),
            ],
        );
        typed(
            ui,
            editor,
            Field::RequiredFields,
            tr!("preset-required-fields"),
        );
        let b = &mut editor.value_mut().bom;
        check(ui, tr!("preset-list-unplaced"), &mut b.list_unplaced);
        for (label, value) in [
            (tr!("preset-mount-smt"), &mut b.mount_values.smt),
            (tr!("preset-mount-tht"), &mut b.mount_values.tht),
        ] {
            ui.label(&label);
            field(ui, &label, value, 100.0, None);
            ui.end_row();
        }
    });
    note(ui, tr!("preset-tokens-note"));
}

fn cpl(ui: &mut Ui, editor: &mut PresetEditor) {
    let preset = editor.value_mut();
    let origin = preset.gerber.file_origin;
    let mount = preset.bom.mount_values.clone();
    let c = &mut preset.cpl;
    ui.strong(tr!("preset-columns"));
    columns(ui, "preset-cpl-columns", &mut c.columns);
    ui.add_space(6.0);
    rows(ui, "preset-cpl", |ui| {
        for (label, value) in [
            (tr!("preset-side-top"), &mut c.side_values.top),
            (tr!("preset-side-bottom"), &mut c.side_values.bottom),
        ] {
            ui.label(&label);
            field(ui, &label, value, 100.0, None);
            ui.end_row();
        }
        choice(
            ui,
            tr!("preset-units"),
            &mut c.units,
            &[
                (CplUnits::Mm, tr!("preset-units-mm")),
                (CplUnits::Mil, tr!("preset-units-mil")),
                (CplUnits::In, tr!("preset-units-in")),
            ],
        );
        check(ui, tr!("preset-unit-suffix"), &mut c.unit_suffix);
        choice(
            ui,
            tr!("preset-bottom-rotation"),
            &mut c.bottom_rotation,
            &[
                (BottomRotation::Stored, tr!("preset-rotation-stored")),
                (BottomRotation::Plus180, tr!("preset-rotation-plus-180")),
                (BottomRotation::Negate, tr!("preset-rotation-negate")),
            ],
        );
        choice(
            ui,
            tr!("preset-bottom-position"),
            &mut c.bottom_position,
            &[
                (BottomPosition::TopView, tr!("preset-position-top-view")),
                (BottomPosition::NegateX, tr!("preset-position-negate-x")),
                (BottomPosition::NegateY, tr!("preset-position-negate-y")),
            ],
        );
        check(ui, tr!("preset-exclude-tht"), &mut c.exclude_through_hole);
        read_only(
            ui,
            tr!("preset-file-origin"),
            origin_text(origin),
            tr!("preset-set-on-gerber"),
        );
        if c.columns
            .iter()
            .any(|col| col.source == ColumnSource::Mount)
        {
            read_only(
                ui,
                tr!("preset-mount-values"),
                format!("{} / {}", mount.smt, mount.tht),
                tr!("preset-set-on-bom"),
            );
        }
    });
}

fn packaging(ui: &mut Ui, editor: &mut PresetEditor) {
    rows(ui, "preset-packaging", |ui| {
        check(
            ui,
            tr!("preset-zip-assembly"),
            &mut editor.value_mut().zip_assembly,
        );
    });
    note(ui, tr!("preset-zip-note"));
}
