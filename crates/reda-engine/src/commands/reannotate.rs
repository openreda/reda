//! `schematic.reannotate` and its preview: renumbering Designators across every sheet, from
//! the schematic or from the board, with the last settings applied kept in the Project.

use reda_command::{Command, CommandError, Context, Kind, Length, Meta, Registry};
use reda_model::annotate::{
    Location, Mode, Order, ReannotateSettings, ScopeKind, SheetStart, Sides, Source,
};
use reda_model::{Object, Uuid};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::annotate::{self, Plan, Selection};
use crate::sync::{self, NO_PCB};

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(Reannotate);
    registry.add(ReannotatePreview);
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReannotateParams {
    /// Run the Project's saved settings (those of the last `schematic.reannotate`
    /// applied). No other setting may be given with it, except `selection` for a saved
    /// `selection` scope.
    #[serde(default)]
    pub saved: bool,
    /// `schematic` (the default): by where the Units sit on the sheets. `board`: by where
    /// the Footprints sit on the board; the new Designators write through to it.
    pub source: Option<Source>,
    /// `renumber_all` (the default), or `fix_duplicates_only`: only Components whose
    /// Designator another holds are renumbered, the first in processing order keeping it.
    pub mode: Option<Mode>,
    /// `project` (the default), `sheets` (the Components with a Unit on one of `sheets`)
    /// or `selection` (those in `selection`; from the board, Selected Only).
    pub scope: Option<ScopeKind>,
    /// The Schematic Sheets, for scope `sheets`.
    pub sheets: Option<Vec<Uuid>>,
    /// Component, Unit or Footprint instance UUIDs, for scope `selection`. A Unit or a
    /// Footprint stands for its Component.
    pub selection: Option<Vec<Uuid>>,
    /// From the schematic: `across_then_down` (the default), `across_then_up`,
    /// `down_then_across` or `up_then_across`; sheets always go in Projects panel order.
    /// From the board: `descending_y_then_ascending_x` (the default),
    /// `ascending_y_then_ascending_x`, `ascending_x_then_descending_y` or
    /// `ascending_x_then_ascending_y`.
    pub order: Option<Order>,
    /// From the schematic: sort by `part` (the body centre, the default) or `designator`
    /// (where its text sits, or the body centre when it is hidden).
    pub location: Option<Location>,
    /// A part this close to the first part of a row (or column) joins it. 100 mil by
    /// default.
    pub threshold: Option<Length>,
    /// From the schematic: where each listed sheet's numbering starts (1 for the others).
    /// A sheet that runs over takes the next free numbers.
    pub per_sheet: Option<Vec<SheetStart>>,
    /// From the board: `top_then_bottom` (the default), `top` or `bottom`.
    pub sides: Option<Sides>,
    /// From the board: where numbering starts, 1 by default.
    pub start_index: Option<u64>,
}

impl ReannotateParams {
    /// The settings to run with: the Project's saved ones, or those given over the
    /// defaults.
    fn settings(&self, cx: &Context<'_>) -> Result<ReannotateSettings, CommandError> {
        if self.saved {
            let given = [
                ("/source", self.source.is_some()),
                ("/mode", self.mode.is_some()),
                ("/scope", self.scope.is_some()),
                ("/sheets", self.sheets.is_some()),
                ("/order", self.order.is_some()),
                ("/location", self.location.is_some()),
                ("/threshold", self.threshold.is_some()),
                ("/per_sheet", self.per_sheet.is_some()),
                ("/sides", self.sides.is_some()),
                ("/start_index", self.start_index.is_some()),
            ];
            if let Some((pointer, _)) = given.iter().find(|(_, given)| *given) {
                return Err(CommandError::invalid_params(
                    pointer,
                    "`saved` runs the saved settings, so no other setting may be given",
                ));
            }
            return cx.project()?.props().reannotate.clone().ok_or_else(|| {
                CommandError::unavailable(
                    "this Project has no saved Re-annotate settings yet: apply a \
                     Re-annotate with settings first",
                )
            });
        }
        let source = self.source.unwrap_or_default();
        let mut s = ReannotateSettings::new(source);
        if let Some(order) = self.order {
            if order.source() != source {
                return Err(CommandError::invalid_params(
                    "/order",
                    format!("this order does not walk the {source:?} source").to_lowercase(),
                ));
            }
            s.order = order;
        }
        s.mode = self.mode.unwrap_or(s.mode);
        s.scope = self.scope.unwrap_or(s.scope);
        s.location = self.location.unwrap_or(s.location);
        s.threshold = self.threshold.map_or(s.threshold, Length::nm);
        if s.threshold.0 < 0 {
            return Err(CommandError::invalid_params(
                "/threshold",
                "the threshold cannot be negative",
            ));
        }
        s.sides = self.sides.unwrap_or(s.sides);
        s.start_index = self.start_index.unwrap_or(s.start_index);
        s.per_sheet = self.per_sheet.clone().unwrap_or_default();
        if s.scope == ScopeKind::Sheets {
            s.sheets = self.sheets.clone().ok_or_else(|| {
                CommandError::invalid_params("/sheets", "scope `sheets` needs `sheets`")
            })?;
        }
        Ok(s)
    }

    fn selection(&self, settings: &ReannotateSettings) -> Result<Selection, CommandError> {
        match settings.scope {
            ScopeKind::Project => Ok(Selection::Project),
            ScopeKind::Sheets => Ok(Selection::Sheets(settings.sheets.clone())),
            ScopeKind::Selection => {
                self.selection
                    .clone()
                    .map(Selection::Objects)
                    .ok_or_else(|| {
                        CommandError::invalid_params(
                            "/selection",
                            "scope `selection` needs a `selection`",
                        )
                    })
            }
        }
    }

    fn plan(&self, cx: &Context<'_>, settings: &ReannotateSettings) -> Result<Plan, CommandError> {
        if settings.source == Source::Board && cx.project()?.pcb().is_none() {
            return Err(CommandError::unavailable(format!(
                "Re-annotate from the board needs a board: {NO_PCB}"
            )));
        }
        let selection = self.selection(settings)?;
        annotate::plan(cx.project()?, settings, &selection)
    }
}

const DESCRIPTION: &str = "Renumbers Designators, from the schematic (by where each Unit's \
    body or Designator sits, sheets in Projects panel order) or from the board (by the \
    leading edge of each Footprint's Courtyard or Pads, the top side then the bottom seen \
    from below; the new Designators write through to the board, one undo entry per \
    Document). Rows and columns gather the parts within `threshold` of their first part. \
    Each Component in the scope that is not Designator-locked gets the smallest free \
    number for its prefix from its start index (with `fix_duplicates_only`, only \
    duplicates are renumbered); locked and out-of-scope Components keep theirs, so a \
    number an unselected Component holds is skipped. Every Unit of a Component gets the \
    same Designator. The settings applied are saved in the Project, and `{saved: true}` \
    runs them again.";

pub struct ReannotatePreview;

impl Command for ReannotatePreview {
    const META: Meta = Meta {
        id: "schematic.reannotate.preview",
        title: "Preview Re-annotate",
        description: "Shows old → new for what `schematic.reannotate` would do with the same \
                      params, and the Designators that locked or out-of-scope Components \
                      would still share, without changing anything.",
        category: "schematic",
        icon: "annotate",
        kind: Kind::Query,
    };
    type Params = ReannotateParams;
    type Output = Plan;

    fn run(&self, cx: &mut Context<'_>, p: ReannotateParams) -> Result<Plan, CommandError> {
        let settings = p.settings(cx)?;
        p.plan(cx, &settings)
    }
}

pub struct Reannotate;

impl Command for Reannotate {
    const META: Meta = Meta {
        id: "schematic.reannotate",
        title: "Re-annotate",
        description: DESCRIPTION,
        category: "schematic",
        icon: "annotate",
        kind: Kind::Edit,
    };
    type Params = ReannotateParams;
    type Output = Plan;

    fn run(&self, cx: &mut Context<'_>, p: ReannotateParams) -> Result<Plan, CommandError> {
        let settings = p.settings(cx)?;
        let board = settings.source == Source::Board;
        let run = |cx: &mut Context<'_>| {
            let plan = p.plan(cx, &settings)?;
            let tx = cx.tx()?;
            annotate::apply(tx, &plan.designators)?;
            if tx.project().props().reannotate.as_ref() != Some(&settings) {
                let mut props = tx.project().props().clone();
                props.reannotate = Some(settings.clone());
                tx.replace(Object::Project(props))?;
            }
            Ok(plan)
        };
        // Started from the board, it writes through like any PCB-side edit.
        match board {
            true => sync::write_through(cx, run),
            false => run(cx),
        }
    }
}
