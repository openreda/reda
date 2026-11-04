//! `schematic.reannotate` and its preview: renumbering Designators across every sheet.

use reda_command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_model::Uuid;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::annotate::{self, Order, Rename, Selection};
use crate::sync::{self, NO_PCB};

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(Reannotate);
    registry.add(ReannotatePreview);
}

/// Which Components to renumber.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReannotateScope {
    /// Every Component in the Project.
    Project,
    /// Every Component with a Unit on the sheet `document`.
    Sheet,
    /// The Components or Units listed in `selection`.
    Selection,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReannotateParams {
    /// The order to number the Components in.
    pub order: Order,
    pub scope: ReannotateScope,
    /// The Schematic Sheet, for scope `sheet`.
    pub document: Option<Uuid>,
    /// Component or Unit UUIDs, for scope `selection`. A Unit stands for its Component.
    pub selection: Option<Vec<Uuid>>,
}

impl ReannotateParams {
    fn selection(&self) -> Result<Selection, CommandError> {
        match self.scope {
            ReannotateScope::Project => Ok(Selection::Project),
            ReannotateScope::Sheet => self.document.map(Selection::Sheet).ok_or_else(|| {
                CommandError::invalid_params("/document", "scope `sheet` needs a `document`")
            }),
            ReannotateScope::Selection => self
                .selection
                .clone()
                .map(Selection::Objects)
                .ok_or_else(|| {
                    CommandError::invalid_params(
                        "/selection",
                        "scope `selection` needs a `selection`",
                    )
                }),
        }
    }

    fn plan(&self, cx: &Context<'_>) -> Result<ReannotateResult, CommandError> {
        if self.order == Order::BoardPosition && cx.project()?.pcb().is_none() {
            return Err(CommandError::unavailable(format!(
                "Re-annotate by board position needs a board: {NO_PCB}"
            )));
        }
        let order = self.order.processing();
        let designators = annotate::plan(cx.project()?, &self.selection()?, order.as_ref())?;
        Ok(ReannotateResult { designators })
    }
}

/// Old → new for every Component renumbered, in processing order. Locked Components are
/// left out; a Component whose number comes out the same is listed with `old` = `new`.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ReannotateResult {
    pub designators: Vec<Rename>,
}

const DESCRIPTION: &str = "Renumbers Designators in a processing order (across then down, \
    across then up, down then across, up then across; sheets in Document order), or by \
    board position (where the Footprints sit: top side, then bottom; the new Designators \
    write through to the board, one undo entry per Document). Each \
    Component in the scope that is not Designator-locked gets the smallest free number for \
    its prefix; locked and out-of-scope Components keep theirs. Every Unit of a Component \
    gets the same Designator.";

pub struct ReannotatePreview;

impl Command for ReannotatePreview {
    const META: Meta = Meta {
        id: "schematic.reannotate.preview",
        title: "Preview Re-annotate",
        description: "Shows old → new for what `schematic.reannotate` would do with the same \
                      params, without changing anything.",
        category: "schematic",
        icon: "annotate",
        kind: Kind::Query,
    };
    type Params = ReannotateParams;
    type Output = ReannotateResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ReannotateParams,
    ) -> Result<ReannotateResult, CommandError> {
        p.plan(cx)
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
    type Output = ReannotateResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ReannotateParams,
    ) -> Result<ReannotateResult, CommandError> {
        if p.order == Order::BoardPosition {
            // Started from the board, so it writes through like any PCB-side edit.
            return sync::write_through(cx, |cx| {
                let result = p.plan(cx)?;
                annotate::apply(cx.tx()?, &result.designators)?;
                Ok(result)
            });
        }
        let result = p.plan(cx)?;
        annotate::apply(cx.tx()?, &result.designators)?;
        Ok(result)
    }
}
