//! Deleting on the board from the window (docs/spec/v0.1/pcb.md#dimensions). `Del` runs
//! `pcb.copper.delete` on the selection; here the Shell splits a selection that holds
//! Dimensions, free graphics, text or Free Pads into each kind's delete, in one `batch`.
//! Deleting an object a Dimension follows asks first whether to delete the Dimension
//! too (`pcb.delete.confirm`); a Command run by an agent or script keeps it, without
//! its `ref`.

use egui::Id;
use reda_engine::command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_engine::model::{PcbDocument, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::shell::Shell;
use crate::tr;

/// The delete waiting for the user's answer, in the engine's view state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeletePrompt {
    pub pending: Option<Pending>,
    /// The split deletes are running: the copper one goes through as it is.
    running: bool,
}

/// A delete held back while the window asks about Dimensions.
#[derive(Clone, Debug, PartialEq)]
pub struct Pending {
    /// The deletes as asked for.
    pub commands: Vec<(String, Value)>,
    /// The Dimensions that follow a deleted object and weren't picked themselves.
    pub dimensions: Vec<Uuid>,
}

pub fn register_all(registry: &mut Registry) {
    registry.add(Confirm);
}

/// What the window's answer does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    /// Deletes the Dimensions with their references.
    DeleteDimensions,
    /// Deletes the objects only; the Dimensions stay, without a `ref` there.
    KeepDimensions,
    /// Deletes nothing.
    Cancel,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfirmParams {
    pub answer: Answer,
}

#[derive(Serialize, JsonSchema)]
pub struct Confirmed {
    pub answer: Answer,
}

struct Confirm;

impl Command for Confirm {
    const META: Meta = Meta {
        id: "pcb.delete.confirm",
        title: "Answer Delete Dimensions",
        description: "Answers the question the window asks when a delete takes an object a \
                      Dimension follows: delete the Dimensions too, keep them (without that \
                      `ref`), or cancel the delete.",
        category: "pcb",
        icon: "delete",
        kind: Kind::View,
    };
    type Params = ConfirmParams;
    type Output = Confirmed;

    fn run(&self, cx: &mut Context<'_>, p: ConfirmParams) -> Result<Confirmed, CommandError> {
        if cx.view::<DeletePrompt>()?.pending.is_none() {
            return Err(CommandError::rejected("no delete is waiting for an answer"));
        }
        Ok(Confirmed { answer: p.answer })
    }
}

/// The delete waiting for an answer.
pub fn pending(shell: &Shell) -> Option<Pending> {
    shell.engine().view().get::<DeletePrompt>()?.pending.clone()
}

/// `Del` (`pcb.copper.delete` with `params`) from the window: `None` when it is plain
/// copper that no Dimension follows, which runs as it is; otherwise the split deletes ran
/// (or wait for an answer), with what they returned.
pub(crate) fn intercept(shell: &mut Shell, params: &Value) -> Option<Result<Value, CommandError>> {
    let prompt = shell.engine().view().get::<DeletePrompt>()?;
    if prompt.running {
        return None;
    }
    let pcb = shell.engine().project()?.pcb()?;
    let objects: Vec<Uuid> = serde_json::from_value(params["objects"].clone()).ok()?;
    let (commands, deleted) = split(pcb, &objects, params);
    let dimensions = followers(pcb, &deleted);
    if dimensions.is_empty() && commands.len() == 1 && commands[0].0 == "pcb.copper.delete" {
        return None;
    }
    if !dimensions.is_empty() {
        if let Some(prompt) = shell.engine_mut().view_mut().get_mut::<DeletePrompt>() {
            prompt.pending = Some(Pending {
                commands,
                dimensions: dimensions.clone(),
            });
        }
        return Some(Ok(json!({"asked": dimensions})));
    }
    Some(run_all(shell, commands))
}

/// After `pcb.delete.confirm` answered `out`: runs the held delete as answered.
pub(crate) fn after_confirm(shell: &mut Shell, out: &Value) {
    let Some(Pending {
        mut commands,
        dimensions,
    }) = shell
        .engine_mut()
        .view_mut()
        .get_mut::<DeletePrompt>()
        .and_then(|p| p.pending.take())
    else {
        return;
    };
    let answer: Answer = serde_json::from_value(out["answer"].clone()).unwrap_or(Answer::Cancel);
    match answer {
        Answer::Cancel => return,
        Answer::KeepDimensions => {}
        Answer::DeleteDimensions => commands.extend(
            dimensions
                .iter()
                .map(|d| ("pcb.dimension.delete".to_owned(), json!({ "uuid": d }))),
        ),
    }
    if let Err(e) = run_all(shell, commands) {
        shell.report(tr!("error-command", error = e.message));
    }
}

fn run_all(shell: &mut Shell, mut commands: Vec<(String, Value)>) -> Result<Value, CommandError> {
    let running = |shell: &mut Shell, on: bool| {
        if let Some(p) = shell.engine_mut().view_mut().get_mut::<DeletePrompt>() {
            p.running = on;
        }
    };
    running(shell, true);
    let out = if commands.len() == 1 {
        let (id, params) = commands.remove(0);
        shell.run(&id, params)
    } else {
        let steps: Vec<Value> = commands
            .into_iter()
            .map(|(id, params)| json!({"id": id, "params": params}))
            .collect();
        shell.run("batch", json!({ "commands": steps }))
    };
    running(shell, false);
    out
}

/// Each kind's delete for `objects`, and the objects they take. Anything that is not a
/// Dimension, graphic, text or Free Pad stays with the copper delete, which says so.
fn split(pcb: &PcbDocument, objects: &[Uuid], params: &Value) -> (Vec<(String, Value)>, Vec<Uuid>) {
    let mut copper = Vec::new();
    let mut pads = Vec::new();
    let mut commands = Vec::new();
    let locked = params.get("include_locked").cloned();
    let with_locked = |mut p: Value| {
        if let Some(l) = &locked {
            p["include_locked"] = l.clone();
        }
        p
    };
    for uuid in objects {
        let id = if pcb.dimension(*uuid).is_some() {
            "pcb.dimension.delete"
        } else if pcb.graphic(*uuid).is_some() {
            "pcb.graphic.delete"
        } else if pcb.text(*uuid).is_some() {
            "pcb.text.delete"
        } else if pcb.free_pad(*uuid).is_some() {
            pads.push(*uuid);
            continue;
        } else {
            copper.push(*uuid);
            continue;
        };
        commands.push((id.to_owned(), with_locked(json!({ "uuid": uuid }))));
    }
    if !pads.is_empty() {
        commands.insert(
            0,
            (
                "pcb.free_pad.delete".to_owned(),
                with_locked(json!({ "pads": pads })),
            ),
        );
    }
    if !copper.is_empty() {
        let mut p = params.clone();
        p["objects"] = json!(copper);
        commands.insert(0, ("pcb.copper.delete".to_owned(), p));
    }
    (commands, objects.to_vec())
}

/// The Dimensions that follow one of `deleted` and aren't among them.
fn followers(pcb: &PcbDocument, deleted: &[Uuid]) -> Vec<Uuid> {
    pcb.dimensions()
        .filter(|d| !deleted.contains(&d.uuid))
        .filter(|d| d.references().iter().any(|r| deleted.contains(r)))
        .map(|d| d.uuid)
        .collect()
}

/// The question, while a delete waits for it. Returns the answer's Command.
pub fn dialog(ctx: &egui::Context, shell: &Shell) -> Option<(String, Value)> {
    let pending = pending(shell)?;
    let answer = |a: Answer| Some(("pcb.delete.confirm".to_owned(), json!({ "answer": a })));
    let mut out = None;
    let response = crate::modal::Dialog::new(
        Id::NULL.with("delete-dimensions"),
        tr!("delete-dimensions-title"),
    )
    .width(340.0)
    .show(ctx, |ui| {
        ui.label(tr!(
            "delete-dimensions-body",
            count = pending.dimensions.len()
        ));
        ui.horizontal(|ui| {
            if ui.button(tr!("delete-dimensions-too")).clicked() {
                out = answer(Answer::DeleteDimensions);
            }
            if ui.button(tr!("delete-dimensions-keep")).clicked() {
                out = answer(Answer::KeepDimensions);
            }
            if ui.button(tr!("delete-dimensions-cancel")).clicked() {
                out = answer(Answer::Cancel);
            }
        });
    });
    if out.is_none() && response.should_close() {
        out = answer(Answer::Cancel);
    }
    out
}
