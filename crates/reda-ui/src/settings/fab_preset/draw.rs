//! Drawing a Fab Preset page into a settings window or the Preferences window.

use egui::{Role, Ui, WidgetInfo};
use reda_engine::model::{
    BomLayout, BottomPosition, BottomRotation, Column, ColumnSource, CplUnits, FileNaming,
    FileOrigin, LayerFileName, NotFittedBom, PasteExpansion, Side, SlotCommand,
};

use super::{Field, PresetEditor, PresetPage};
use crate::settings::{dropdown, field, layout, mono, strong};
use crate::theme::TokensExt as _;
use crate::tr;
use crate::widgets::{self, Column as TableColumn, Tooltip as _};

/// A typed value or a dropdown in a page's rows.
const FIELD_WIDTH: f32 = 320.0;
/// A file name rule's Layer.
const LAYER_WIDTH: f32 = 140.0;
/// A file name rule's name.
const FILE_NAME_WIDTH: f32 = 180.0;
/// A column's source.
const SOURCE_WIDTH: f32 = 130.0;
/// The Field a column takes its value from.
const SOURCE_FIELD_WIDTH: f32 = 120.0;
/// A column's header.
const HEADER_WIDTH: f32 = 160.0;
/// The designator separator.
const SEPARATOR_WIDTH: f32 = 60.0;
/// A mount or side value.
const VALUE_WIDTH: f32 = 100.0;

/// Draws `page` of the preset in `editor`.
pub fn page(ui: &mut Ui, editor: &mut PresetEditor, page: PresetPage) {
    match page {
        PresetPage::Rules => rules(ui, editor),
        PresetPage::Stackup => stackup(ui, editor),
        PresetPage::Gerber => gerber(ui, editor),
        PresetPage::Drill => drill(ui, editor),
        PresetPage::Bom => bom(ui, editor),
        PresetPage::Cpl => cpl(ui, editor),
        PresetPage::Placement => super::placement::grid(ui, editor),
        PresetPage::Packaging => packaging(ui, editor),
    }
}

/// A grid of label and value rows.
fn rows(ui: &mut Ui, id: &str, add: impl FnOnce(&mut Ui)) {
    widgets::form(id).show(ui, add);
}

/// A heading between a page's rows, in the name column.
fn subheading(ui: &mut Ui, text: String) {
    strong(ui, text);
    ui.end_row();
}

/// A typed field's row.
fn typed(ui: &mut Ui, editor: &mut PresetEditor, f: Field, label: String) {
    ui.add(widgets::form_name(&label));
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
    ui.add(widgets::form_name(&label));
    dropdown(ui, label.as_str(), &label, value, options, FIELD_WIDTH);
    ui.end_row();
}

/// A checkbox's row.
fn check(ui: &mut Ui, label: String, value: &mut bool) {
    ui.label("");
    ui.add(widgets::checkbox(value, label));
    ui.end_row();
}

/// A value shown read-only, where another page sets it.
fn read_only(ui: &mut Ui, label: String, value: String, set_on: String) {
    ui.add(widgets::form_name(&label));
    ui.horizontal(|ui| {
        strong(ui, value);
        layout::muted(ui, set_on);
    });
    ui.end_row();
}

fn note(ui: &mut Ui, text: String) {
    let t = ui.tokens();
    ui.add_space(t.space.stack);
    layout::muted(ui, text);
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
    layout::text(ui, tr!("preset-rules-intro"));
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
        subheading(ui, tr!("preset-preferred"));
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
        subheading(ui, tr!("preset-mask-paste"));
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
        subheading(ui, tr!("preset-seeds"));
        typed(
            ui,
            editor,
            Field::SeedThickness,
            tr!("preset-seed-thickness"),
        );
        typed(ui, editor, Field::SeedCopper, tr!("preset-seed-copper"));
        subheading(ui, tr!("preset-suggestions"));
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
    rows(ui, "preset-gerber", |ui| {
        let g = &mut editor.value_mut().gerber;
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
        layout::muted(ui, tr!("preset-file-origin-note"));
        ui.end_row();
        check(
            ui,
            tr!("preset-outline-every"),
            &mut g.outline_on_every_plot,
        );
        typed(ui, editor, Field::InnerCopper, tr!("preset-inner-copper"));
        ui.label("");
        layout::muted(ui, tr!("preset-inner-copper-tokens"));
        ui.end_row();
    });
    let t = ui.tokens();
    ui.add_space(t.space.section);
    strong(ui, tr!("preset-file-names"));
    file_names(ui, &mut editor.value_mut().gerber.file_names.layers);
    if editor.problem(Field::InnerCopper).is_none() {
        ui.add_space(t.space.section);
        strong(ui, tr!("preset-four-layer-preview"));
        let sides = [Some(Side::Top), None, None, Some(Side::Bottom)];
        for (i, side) in sides.into_iter().enumerate() {
            match reda_engine::output::copper_file_name(
                "Board",
                &editor.value().gerber,
                i + 1,
                side,
                None,
            ) {
                Ok(name) => {
                    mono(ui, name);
                }
                Err(why) => {
                    layout::error(ui, why);
                }
            }
        }
    }
}

fn file_names(ui: &mut Ui, names: &mut Vec<LayerFileName>) {
    let mut remove = None;
    widgets::table("preset-file-names")
        .columns([TableColumn::auto(); 3])
        .show(ui, |table| {
            table.header(|h| {
                h.text(tr!("preset-layer"));
                h.text(tr!("preset-file-name"));
                h.empty();
            });
            for (i, row) in names.iter_mut().enumerate() {
                let n = i + 1;
                table.row(|cells| {
                    cells.ui(|ui| {
                        let label = tr!("preset-layer-of", n = n);
                        field(ui, &label, &mut row.layer, LAYER_WIDTH, None)
                    });
                    cells.ui(|ui| {
                        let label = tr!("preset-file-name-of", n = n);
                        field(ui, &label, &mut row.name, FILE_NAME_WIDTH, None)
                    });
                    let clicked = cells.ui(|ui| {
                        ui.add(widgets::button(tr!("preset-remove")))
                            .tooltip(tr!("preset-remove-name", n = n))
                            .clicked()
                    });
                    if clicked {
                        remove = Some(i);
                    }
                });
            }
        });
    if let Some(i) = remove {
        names.remove(i);
    }
    if ui
        .add(widgets::button(tr!("preset-add-file-name")))
        .clicked()
    {
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
    // A table this wide gets its own horizontal scroll, so the page (and the note under
    // it) keeps its width.
    egui::ScrollArea::horizontal()
        .id_salt((id, "scroll"))
        .show(ui, |ui| {
            widgets::table(id)
                .columns([TableColumn::auto(); 5])
                .show(ui, |table| {
                    table.header(|h| {
                        h.text(tr!("preset-source"));
                        h.text(tr!("preset-header"));
                        h.empty();
                        h.empty();
                        h.empty();
                    });
                    let count = columns.len();
                    for (i, column) in columns.iter_mut().enumerate() {
                        let n = i + 1;
                        table.row(|cells| {
                            cells.ui(|ui| {
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
                                let combo = widgets::dropdown((id, "source", i), shown)
                                    .width(ui.tokens().size.of(SOURCE_WIDTH))
                                    .show(ui, |ui| {
                                        for (source, text) in &options {
                                            let source = source.clone();
                                            widgets::choice_value(
                                                ui,
                                                &mut column.source,
                                                source,
                                                text,
                                            );
                                        }
                                        let by_field =
                                            widgets::choice(is_field, tr!("preset-source-field"));
                                        if ui.add(by_field).clicked() && !is_field {
                                            column.source =
                                                ColumnSource::Field(column.header.clone());
                                        }
                                    });
                                combo.response.widget_info(|| {
                                    WidgetInfo::labeled(Role::ComboBox, true, label.clone())
                                });
                                if let ColumnSource::Field(name) = &mut column.source {
                                    let label = tr!("preset-field-of", n = n);
                                    field(ui, &label, name, SOURCE_FIELD_WIDTH, None);
                                }
                            });
                            cells.ui(|ui| {
                                let label = tr!("preset-header-of", n = n);
                                field(ui, &label, &mut column.header, HEADER_WIDTH, None)
                            });
                            let small = |ui: &mut Ui, on: bool, text: String, hint: String| {
                                ui.add_enabled(on, widgets::button(text))
                                    .tooltip(hint)
                                    .clicked()
                            };
                            let up = tr!("preset-up-of", n = n);
                            if cells.ui(|ui| small(ui, i > 0, tr!("preset-up"), up)) {
                                action = Some((i, -1));
                            }
                            let down = tr!("preset-down-of", n = n);
                            if cells.ui(|ui| small(ui, i + 1 < count, tr!("preset-down"), down)) {
                                action = Some((i, 1));
                            }
                            let remove = tr!("preset-remove-of", n = n);
                            if cells.ui(|ui| small(ui, true, tr!("preset-remove"), remove)) {
                                action = Some((i, 0));
                            }
                        });
                    }
                });
        });
    match action {
        Some((i, 0)) => {
            columns.remove(i);
        }
        Some((i, -1)) => columns.swap(i, i - 1),
        Some((i, _)) => columns.swap(i, i + 1),
        None => {}
    }
    if ui.add(widgets::button(tr!("preset-add-column"))).clicked() {
        columns.push(Column::new("", ColumnSource::Value));
    }
}

fn bom(ui: &mut Ui, editor: &mut PresetEditor) {
    let t = ui.tokens();
    strong(ui, tr!("preset-columns"));
    columns(
        ui,
        "preset-bom-columns",
        &mut editor.value_mut().bom.columns,
    );
    ui.add_space(t.space.section);
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
        ui.add(widgets::form_name(&label));
        field(
            ui,
            &label,
            &mut b.designator_separator,
            SEPARATOR_WIDTH,
            None,
        );
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
            ui.add(widgets::form_name(&label));
            field(ui, &label, value, VALUE_WIDTH, None);
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
    let t = ui.tokens();
    strong(ui, tr!("preset-columns"));
    columns(ui, "preset-cpl-columns", &mut c.columns);
    ui.add_space(t.space.section);
    rows(ui, "preset-cpl", |ui| {
        for (label, value) in [
            (tr!("preset-side-top"), &mut c.side_values.top),
            (tr!("preset-side-bottom"), &mut c.side_values.bottom),
        ] {
            ui.add(widgets::form_name(&label));
            field(ui, &label, value, VALUE_WIDTH, None);
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
