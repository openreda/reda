//! The Stackup page's drafts: the target thickness, one row per copper Layer and
//! dielectric with its thickness as typed, the finish and colours, and the board's Fab
//! Notes text. Totals and the cross-section follow the drafts as you type.

use reda_engine::model::geom::Nm;
use reda_engine::model::{
    CopperEntry, DielectricEntry, PcbDocument, Project, Stackup, StackupEntry, StackupSuggestions,
    StackupTotals, Uuid,
};
use serde_json::{Map, Value, json};

use super::super::length::{mm, optional};
use crate::settings::{Draft, Problem, Step};
use crate::tr;

/// One Stackup entry as drafted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StackupRow {
    /// "Top Copper", "Dielectric 1".
    pub name: String,
    /// Copper (a Layer) or a dielectric.
    pub copper: bool,
    /// The copper Layer or the dielectric entry.
    pub uuid: Uuid,
    thickness: Draft<String>,
}

impl StackupRow {
    /// The thickness as typed; empty for a dielectric that takes the remainder.
    pub fn thickness(&self) -> &str {
        self.thickness.value()
    }
}

/// What a band of the cross-section is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Band {
    Silkscreen,
    Mask,
    Copper,
    Dielectric,
}

/// One band of the cross-section, top to bottom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrossBand {
    pub band: Band,
    pub name: String,
    /// Its thickness in mm (a blank dielectric's share of the remainder); empty for mask
    /// and silkscreen, which the Stackup does not hold.
    pub thickness: String,
}

/// The Stackup page's drafts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StackupDraft {
    target: Draft<String>,
    rows: Vec<StackupRow>,
    finish: Draft<String>,
    mask_color: Draft<String>,
    silk_color: Draft<String>,
    fab_notes: Draft<String>,
    suggestions: StackupSuggestions,
    /// The copper Layers, top to bottom, which a Stackup must name in order.
    copper_layers: Vec<Uuid>,
}

/// A copper thickness as typed: mm unless it names a unit (`35um`, `1oz`).
fn copper(text: &str) -> Result<Nm, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err(tr!("board-setup-required"));
    }
    let with_unit = if text.ends_with(|c: char| c.is_ascii_digit() || c == '.') {
        format!("{text}mm")
    } else {
        text.to_owned()
    };
    match Nm::parse_copper(&with_unit) {
        Ok(nm) if nm.0 > 0 => Ok(nm),
        Ok(_) => Err(tr!("board-setup-positive")),
        Err(_) => Err(tr!("board-setup-not-copper")),
    }
}

/// A row's thickness as typed: copper must be given, a dielectric may be blank.
fn row_value(row: &StackupRow, text: &str) -> Result<Option<Nm>, String> {
    if row.copper {
        return copper(text).map(Some);
    }
    match optional(text)? {
        Some(nm) if nm.0 == 0 => Err(tr!("board-setup-positive")),
        other => Ok(other),
    }
}

impl StackupDraft {
    pub(super) fn load(&mut self, project: &Project, pcb: &PcbDocument, reset: bool) {
        let stackup = pcb.stackup();
        let same = optional(self.target.value()).ok() == Some(Some(stackup.thickness));
        self.target.load(mm(stackup.thickness), reset || same);
        self.copper_layers = stackup.copper().map(|c| c.layer).collect();
        let mut dielectric = 0;
        let rows: Vec<StackupRow> = stackup
            .entries
            .iter()
            .map(|entry| {
                let (uuid, copper, fresh, name) = match entry {
                    StackupEntry::Copper(c) => {
                        let name = pcb
                            .layers()
                            .layer(c.layer)
                            .map(|l| crate::panels::layers::layer_name(pcb, l))
                            .unwrap_or_default();
                        (c.layer, true, mm(c.thickness), name)
                    }
                    StackupEntry::Dielectric(d) => {
                        dielectric += 1;
                        let name = tr!("board-setup-dielectric", number = dielectric);
                        (d.uuid, false, d.thickness.map(mm).unwrap_or_default(), name)
                    }
                };
                let mut thickness = self
                    .rows
                    .iter()
                    .find(|r| r.uuid == uuid)
                    .map_or_else(|| Draft::new(fresh.clone()), |r| r.thickness.clone());
                let row = StackupRow {
                    name: name.clone(),
                    copper,
                    uuid,
                    thickness: Draft::default(),
                };
                let same = row_value(&row, thickness.value()).ok() == row_value(&row, &fresh).ok();
                thickness.load(fresh, reset || same);
                StackupRow { thickness, ..row }
            })
            .collect();
        self.rows = rows;
        let text = |t: &Option<String>| t.clone().unwrap_or_default();
        self.finish.load(text(&stackup.finish), reset);
        self.mask_color.load(text(&stackup.mask_color), reset);
        self.silk_color.load(text(&stackup.silk_color), reset);
        self.fab_notes.load(text(&pcb.setup().fab_notes), reset);
        self.suggestions = project
            .output_jobs()
            .find(|j| j.pcb == pcb.uuid)
            .and_then(|j| project.fab_preset(j.fab_preset))
            .or_else(|| project.fab_presets().next())
            .map(|p| p.stackup_suggestions.clone())
            .unwrap_or_default();
    }

    /// The finished thickness to order, as typed.
    pub fn target(&self) -> &str {
        self.target.value()
    }

    pub fn target_mut(&mut self) -> &mut String {
        self.target.value_mut()
    }

    pub fn set_target(&mut self, text: impl Into<String>) {
        self.target.set(text.into());
    }

    /// The copper and dielectric rows, top to bottom.
    pub fn rows(&self) -> &[StackupRow] {
        &self.rows
    }

    pub fn set_row(&mut self, row: usize, text: impl Into<String>) {
        if let Some(r) = self.rows.get_mut(row) {
            r.thickness.set(text.into());
        }
    }

    pub fn row_mut(&mut self, row: usize) -> Option<&mut String> {
        self.rows.get_mut(row).map(|r| r.thickness.value_mut())
    }

    /// The Stackup as drafted, while every value reads.
    fn drafted(&self) -> Option<Stackup> {
        let thickness = optional(self.target.value()).ok()??;
        let entries = self
            .rows
            .iter()
            .map(|row| {
                let value = row_value(row, row.thickness.value()).ok()?;
                Some(if row.copper {
                    StackupEntry::Copper(CopperEntry {
                        layer: row.uuid,
                        thickness: value?,
                    })
                } else {
                    StackupEntry::Dielectric(DielectricEntry {
                        uuid: row.uuid,
                        thickness: value,
                    })
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Stackup {
            uuid: Uuid::NIL,
            thickness,
            entries,
            finish: None,
            mask_color: None,
            silk_color: None,
        })
    }

    /// The thicknesses added up as drafted; `None` while a value does not read.
    pub fn totals(&self) -> Option<StackupTotals> {
        Some(self.drafted()?.totals())
    }

    /// The warning when every dielectric is typed in and the total misses the target.
    pub fn warning(&self) -> Option<String> {
        let stackup = self.drafted()?;
        let totals = stackup.totals();
        (totals.remainder.is_none() && totals.total != stackup.thickness).then(|| {
            tr!(
                "board-setup-stackup-mismatch",
                total = mm(totals.total),
                target = mm(stackup.thickness)
            )
        })
    }

    /// "Total 1.6 mm, target 1.6 mm", as drafted.
    pub fn total_text(&self) -> Option<String> {
        let totals = self.totals()?;
        Some(tr!(
            "board-setup-stackup-total",
            copper = mm(totals.copper),
            dielectric = mm(totals.total - totals.copper),
            total = mm(totals.total),
            target = self.target.value().trim()
        ))
    }

    /// The cross-section's bands, top to bottom: silkscreen and mask outside the copper.
    pub fn cross_section(&self) -> Vec<CrossBand> {
        let totals = self.totals();
        let blank = self
            .rows
            .iter()
            .filter(|r| !r.copper && r.thickness.value().trim().is_empty())
            .count()
            .max(1) as i64;
        let outer = |band: Band| CrossBand {
            band,
            name: match band {
                Band::Silkscreen => tr!("layer-silkscreen"),
                _ => tr!("layer-solder-mask"),
            },
            thickness: String::new(),
        };
        let mut out = vec![outer(Band::Silkscreen), outer(Band::Mask)];
        for row in &self.rows {
            let typed = row_value(row, row.thickness.value()).ok().flatten();
            let thickness = match (typed, totals.and_then(|t| t.remainder)) {
                (Some(nm), _) => mm(nm),
                (None, Some(rest)) if !row.copper => mm(Nm(rest.0 / blank)),
                _ => String::new(),
            };
            out.push(CrossBand {
                band: if row.copper {
                    Band::Copper
                } else {
                    Band::Dielectric
                },
                name: row.name.clone(),
                thickness,
            });
        }
        out.push(outer(Band::Mask));
        out.push(outer(Band::Silkscreen));
        out
    }

    pub fn finish(&self) -> &str {
        self.finish.value()
    }

    pub fn set_finish(&mut self, text: impl Into<String>) {
        self.finish.set(text.into());
    }

    pub fn finish_mut(&mut self) -> &mut String {
        self.finish.value_mut()
    }

    pub fn mask_color(&self) -> &str {
        self.mask_color.value()
    }

    pub fn set_mask_color(&mut self, text: impl Into<String>) {
        self.mask_color.set(text.into());
    }

    pub fn mask_color_mut(&mut self) -> &mut String {
        self.mask_color.value_mut()
    }

    pub fn silk_color(&self) -> &str {
        self.silk_color.value()
    }

    pub fn set_silk_color(&mut self, text: impl Into<String>) {
        self.silk_color.set(text.into());
    }

    pub fn silk_color_mut(&mut self) -> &mut String {
        self.silk_color.value_mut()
    }

    /// The board's free text for the Fab Notes.
    pub fn fab_notes(&self) -> &str {
        self.fab_notes.value()
    }

    pub fn set_fab_notes(&mut self, text: impl Into<String>) {
        self.fab_notes.set(text.into());
    }

    pub fn fab_notes_mut(&mut self) -> &mut String {
        self.fab_notes.value_mut()
    }

    /// What the job's Fab Preset suggests for the thickness, copper, finish and colours.
    pub fn suggestions(&self) -> &StackupSuggestions {
        &self.suggestions
    }

    /// Every invalid value: rows by index (`stackup/row/<i>`), then the target
    /// (`stackup/thickness`), which also takes a negative remainder.
    pub fn problems(&self) -> Vec<Problem> {
        let mut out = Vec::new();
        for (i, row) in self.rows.iter().enumerate() {
            if let Err(message) = row_value(row, row.thickness.value()) {
                out.push(Problem::new(format!("stackup/row/{i}"), message));
            }
        }
        let target = match optional(self.target.value()) {
            Ok(Some(nm)) if nm.0 > 0 => Ok(()),
            Ok(_) => Err(tr!("board-setup-positive")),
            Err(message) => Err(message),
        };
        if let Err(message) = target {
            out.push(Problem::new("stackup/thickness", message));
        } else if self.overfull() {
            out.push(Problem::new(
                "stackup/thickness",
                tr!("board-setup-stackup-overfull"),
            ));
        }
        out
    }

    /// Whether the values that read already exceed the target, leaving a negative
    /// remainder for a blank dielectric.
    fn overfull(&self) -> bool {
        let Ok(Some(target)) = optional(self.target.value()) else {
            return false;
        };
        let mut typed = Nm::ZERO;
        let mut blank = false;
        for row in &self.rows {
            match row_value(row, row.thickness.value()) {
                Ok(Some(nm)) => typed = typed + nm,
                Ok(None) => blank = true,
                Err(_) => {}
            }
        }
        blank && typed > target
    }

    /// The `pcb.stackup.set` and `pcb.fab_notes.set` steps for what changed.
    pub fn steps(&self) -> Vec<Step> {
        let mut params = Map::new();
        if let (Ok(Some(now)), was) = (
            optional(self.target.value()),
            optional(self.target.base()).ok().flatten(),
        ) && Some(now) != was
        {
            params.insert("thickness".into(), json!(now.0));
        }
        let (mut copper_rows, mut dielectrics) = (Vec::new(), Vec::new());
        for row in &self.rows {
            let Ok(now) = row_value(row, row.thickness.value()) else {
                continue;
            };
            if Some(now) == row_value(row, row.thickness.base()).ok() {
                continue;
            }
            if row.copper {
                let nm = now.expect("copper is given").0;
                copper_rows.push(json!({"layer": row.uuid, "thickness": nm}));
            } else {
                let nm = now.map_or(Value::Null, |n| json!(n.0));
                dielectrics.push(json!({"uuid": row.uuid, "thickness": nm}));
            }
        }
        if !copper_rows.is_empty() {
            params.insert("copper".into(), Value::Array(copper_rows));
        }
        if !dielectrics.is_empty() {
            params.insert("dielectrics".into(), Value::Array(dielectrics));
        }
        for (key, draft) in [
            ("finish", &self.finish),
            ("mask_color", &self.mask_color),
            ("silk_color", &self.silk_color),
        ] {
            if draft.value().trim() != draft.base().trim() {
                let text = draft.value().trim();
                let value = if text.is_empty() {
                    Value::Null
                } else {
                    json!(text)
                };
                params.insert(key.into(), value);
            }
        }
        let mut steps = Vec::new();
        if !params.is_empty() {
            steps.push(Step::new("pcb.stackup.set", Value::Object(params)));
        }
        if self.fab_notes.value().trim() != self.fab_notes.base().trim() {
            let text = self.fab_notes.value().trim();
            let text = if text.is_empty() {
                Value::Null
            } else {
                json!(text)
            };
            steps.push(Step::new("pcb.fab_notes.set", json!({ "text": text })));
        }
        steps
    }
}
