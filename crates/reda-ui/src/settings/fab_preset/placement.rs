//! The Placement grid and its match labels.
use crate::settings::{dropdown, field, layout};
use crate::{tr, widgets};
use egui::Ui;
use reda_engine::model::geom::{Millideg, Nm, Point, exact_decimal};
use reda_engine::model::{PlacementCorrection, PlacementMatch};

pub(crate) fn match_name(m: &PlacementMatch) -> String {
    match m {
        PlacementMatch::Footprint { footprint } => footprint.clone(),
        PlacementMatch::Field { field, value } => format!("{field} = {value}"),
    }
}

pub(crate) fn angle(a: Millideg) -> String {
    format!("{}°", exact_decimal(a.0, 1_000))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CorrectionForm {
    original: Option<PlacementMatch>,
    before: Option<PlacementMatch>,
    by_field: bool,
    name: String,
    value: String,
    rotation: String,
    x: String,
    y: String,
}

pub(crate) enum FormAction {
    Pending,
    Saved,
    Cancelled,
}

impl CorrectionForm {
    /// A new specific row must take priority over the broader match the preview showed.
    pub(crate) fn for_preview(
        rows: &[PlacementCorrection],
        p: &reda_engine::output::CplPlacement,
    ) -> Self {
        let mut form = Self::open(rows, p.suggested_match.clone());
        form.before = p
            .matched
            .and_then(|i| rows.get(i))
            .map(PlacementCorrection::matcher);
        form
    }

    pub(crate) fn open(rows: &[PlacementCorrection], suggested: PlacementMatch) -> Self {
        let existing = rows.iter().find(|r| r.matcher().same_match(&suggested));
        let (by_field, name, value) = match &existing
            .map(PlacementCorrection::matcher)
            .unwrap_or(suggested)
        {
            PlacementMatch::Field { field, value } => (true, field.clone(), value.clone()),
            PlacementMatch::Footprint { footprint } => (false, footprint.clone(), String::new()),
        };
        let offset = existing.and_then(|r| r.adjustment().offset);
        Self {
            original: existing.map(PlacementCorrection::matcher),
            before: None,
            by_field,
            name,
            value,
            rotation: exact_decimal(existing.map_or(0, |r| r.adjustment().rotation.0), 1_000),
            x: offset.map(|p| p.x.mm_exact()).unwrap_or_default(),
            y: offset.map(|p| p.y.mm_exact()).unwrap_or_default(),
        }
    }

    fn row(&self) -> Option<PlacementCorrection> {
        if self.name.trim().is_empty() {
            return None;
        }
        let matcher = if self.by_field {
            PlacementMatch::Field {
                field: self.name.trim().to_owned(),
                value: self.value.clone(),
            }
        } else {
            PlacementMatch::Footprint {
                footprint: self.name.clone(),
            }
        };
        let text = self.rotation.trim();
        let rotation = Millideg::parse(&format!("{text}deg"))
            .or_else(|_| Millideg::parse(text))
            .ok()?;
        let length = |s: &str| {
            if s.trim().is_empty() {
                Some(Nm::ZERO)
            } else {
                Nm::parse(&format!("{}mm", s.trim()))
                    .or_else(|_| Nm::parse(s.trim()))
                    .ok()
            }
        };
        let offset = if self.x.trim().is_empty() && self.y.trim().is_empty() {
            None
        } else {
            Some(Point::new(length(&self.x)?, length(&self.y)?))
        };
        Some(PlacementCorrection::new(matcher, rotation, offset))
    }

    /// Apply to the current rows so changes to other rows while the form is open survive.
    pub(crate) fn show(&mut self, ui: &mut Ui, rows: &mut Vec<PlacementCorrection>) -> FormAction {
        widgets::form("placement-correction-form").show(ui, |ui| {
            let label = tr!("placement-kind");
            ui.add(widgets::form_name(&label));
            dropdown(
                ui,
                "placement-kind",
                &label,
                &mut self.by_field,
                &[
                    (false, tr!("preset-source-footprint")),
                    (true, tr!("preset-source-field")),
                ],
                155.0,
            );
            ui.end_row();
            let input = |ui: &mut Ui, label: String, text: &mut String| {
                ui.add(widgets::form_name(&label));
                field(ui, &label, text, 180., None);
                ui.end_row();
            };
            input(
                ui,
                if self.by_field {
                    tr!("placement-field-name")
                } else {
                    tr!("placement-pattern")
                },
                &mut self.name,
            );
            if self.by_field {
                input(ui, tr!("placement-field-value"), &mut self.value);
            }
            input(ui, tr!("placement-rotation"), &mut self.rotation);
            input(ui, tr!("placement-offset-x"), &mut self.x);
            input(ui, tr!("placement-offset-y"), &mut self.y);
        });
        let row = self.row();
        let index = self
            .original
            .as_ref()
            .and_then(|m| rows.iter().position(|r| r.matcher().same_match(m)));
        let duplicate = row.as_ref().and_then(|r| {
            rows.iter()
                .position(|other| other.matcher().same_match(&r.matcher()))
        });
        let available = duplicate.is_none() || duplicate == index;
        if !available {
            layout::error(ui, tr!("placement-duplicate"));
        }
        let mut action = FormAction::Pending;
        widgets::button_row(ui, |ui| {
            if ui
                .add_enabled(
                    row.is_some() && available,
                    widgets::button(tr!("placement-save")).primary(),
                )
                .clicked()
                && let Some(row) = row
            {
                if let Some(index) = index {
                    rows[index] = row;
                } else {
                    let before = self
                        .before
                        .as_ref()
                        .and_then(|m| rows.iter().position(|r| r.matcher().same_match(m)));
                    rows.insert(before.unwrap_or(rows.len()), row);
                }
                action = FormAction::Saved;
            }
            if ui.add(widgets::button(tr!("dialog-cancel"))).clicked() {
                action = FormAction::Cancelled;
            }
        });
        action
    }
}

pub(super) fn grid(ui: &mut Ui, editor: &mut super::PresetEditor) {
    let rows = &mut editor.value.placement_corrections;
    let mut move_to = None;
    let mut remove = None;
    egui::ScrollArea::horizontal()
        .id_salt("placement-scroll")
        .show(ui, |ui| {
            widgets::table("placement-rows")
                .columns([widgets::Column::auto(); 7])
                .show(ui, |table| {
                    table.header(|h| {
                        for label in [
                            tr!("placement-kind"),
                            tr!("placement-match-value"),
                            tr!("placement-rotation"),
                            tr!("placement-offset-x"),
                            tr!("placement-offset-y"),
                        ] {
                            h.text(label);
                        }
                        h.empty();
                        h.empty();
                    });
                    for (index, row) in rows.iter().enumerate() {
                        table.row(|cells| {
                            cells.text(match row {
                                PlacementCorrection::Footprint { .. } => {
                                    tr!("preset-source-footprint")
                                }
                                PlacementCorrection::Field { .. } => tr!("preset-source-field"),
                            });
                            cells.text(match_name(&row.matcher()));
                            cells.text(angle(row.adjustment().rotation));
                            cells.text(
                                row.adjustment()
                                    .offset
                                    .map(|o| o.x.mm_exact())
                                    .unwrap_or_default(),
                            );
                            cells.text(
                                row.adjustment()
                                    .offset
                                    .map(|o| o.y.mm_exact())
                                    .unwrap_or_default(),
                            );
                            if cells
                                .ui(|ui| ui.add(widgets::button(tr!("placement-edit"))).clicked())
                            {
                                editor.correction = Some(CorrectionForm::open(rows, row.matcher()));
                            }
                            cells.ui(|ui| {
                                ui.horizontal(|ui| {
                                    if ui
                                        .add_enabled(index > 0, widgets::button(tr!("preset-up")))
                                        .clicked()
                                    {
                                        move_to = Some((index, index - 1));
                                    }
                                    if ui
                                        .add_enabled(
                                            index + 1 < rows.len(),
                                            widgets::button(tr!("preset-down")),
                                        )
                                        .clicked()
                                    {
                                        move_to = Some((index, index + 1));
                                    }
                                    if ui.add(widgets::button(tr!("preset-remove"))).clicked() {
                                        remove = Some(index);
                                    }
                                })
                            });
                        });
                    }
                });
        });
    if let Some((from, to)) = move_to {
        rows.swap(from, to);
    }
    if let Some(index) = remove {
        rows.remove(index);
        editor.correction = None;
    }
    if ui.add(widgets::button(tr!("placement-add"))).clicked() {
        editor.correction = Some(CorrectionForm::open(
            rows,
            PlacementMatch::Footprint {
                footprint: String::new(),
            },
        ));
    }
    if let Some(form) = &mut editor.correction
        && !matches!(form.show(ui, rows), FormAction::Pending)
    {
        editor.correction = None;
    }
}
