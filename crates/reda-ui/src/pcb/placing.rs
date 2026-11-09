//! Moving Footprints (docs/spec/v0.1/pcb.md#moving-footprints and #finding-and-queueing):
//! the placement modes, the Reposition Selected queue, Choose Component and the
//! confirmation Protect Locked Objects asks for when it is off.
//!
//! What a move has in flight lives in the window ([`SelectTool`](super::SelectTool)), so
//! these view Commands check and say what was asked, and the Shell applies it after they
//! run ([`after_run`]), as it does for the Track Tool's keys:
//!
//! - `pcb.place.mode {mode}` (`R`, `N`, the Placement Bar): steps `pcb.place.obstacle_mode`
//!   or `pcb.place.ratsnest_mode`, or toggles `pcb.place.alignment_guides`, through
//!   `app.preferences.set`, and redraws the move in flight.
//! - `pcb.place.reposition`: Reposition Selected, the selected Footprints one at a time
//!   in pick order (a selected Unit stands for its Footprint, so cross-select's order
//!   counts); `pcb.place.skip` (`Tab`) skips the one on the cursor.
//! - `pcb.place.choose` opens Choose Component (Move's click on empty space), and
//!   `pcb.place.pick_up {footprint, jump}` floats the chosen one on the cursor.
//! - `pcb.place.confirm {proceed}` answers the question a move or turn of locked objects
//!   asks with Protect Locked Objects off; yes runs it with `include_locked`.
//!
//! [`PlaceFlight`] is the view state the window and these Commands share.

use reda_engine::command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, NoParams, Preset, Registry,
    Unavailable,
};
use reda_engine::model::{Object, PcbDocument, Project, Uuid};
use reda_engine::pcb::place::{ObstacleMode, RatsnestMode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::selection::Selection;
use crate::shell::Shell;

/// Adds the placement Commands to `registry`. The Shell does this for its engine.
pub fn register_all(registry: &mut Registry) {
    registry.add(ModeCommand);
    registry.add(Reposition);
    registry.add(Skip);
    registry.add(Choose);
    registry.add(PickUp);
    registry.add(Confirm);
}

/// What the window and the placement Commands share, in the engine's view state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlaceFlight {
    /// A move carrying Footprints is in flight.
    pub moving: bool,
    /// Reposition Selected: the Footprint on the cursor (1-based) and how many are queued.
    pub queue: Option<(usize, usize)>,
    /// Choose Component is open, with its filter.
    pub choose: Option<String>,
    /// A move or turn of locked objects waiting for the user's yes (Protect Locked
    /// Objects off): the Commands, and how many locked objects they touch.
    pub confirm: Option<Pending>,
}

/// Commands held back for confirmation.
#[derive(Clone, Debug, PartialEq)]
pub struct Pending {
    pub commands: Vec<(String, Value)>,
    pub locked: usize,
    /// The question is Unroute's (`pcb.route.unroute` left locked copper): yes runs it
    /// again with `include_locked`.
    pub unroute: bool,
}

/// After `pcb.route.unroute` from the window left locked copper (`kept_locked`) without
/// being told about it, asks whether to take that too.
pub(crate) fn ask_about_locked_unroute(shell: &mut Shell, params: &Value, out: &Value) {
    let kept = out["result"]["kept_locked"]
        .as_array()
        .map_or(0, |a| a.len());
    if kept == 0 || params.get("include_locked").is_some() {
        return;
    }
    if let Some(f) = flight_mut(shell) {
        f.confirm = Some(Pending {
            commands: vec![("pcb.route.unroute".to_owned(), params.clone())],
            locked: kept,
            unroute: true,
        });
    }
}

fn needs_pcb(cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
    match cx.project {
        Some(p) if p.pcb().is_some() => Ok(()),
        Some(_) => Err(Unavailable {
            reason: "the Project has no PCB Document".into(),
        }),
        None => Err(Unavailable {
            reason: "no Project is open".into(),
        }),
    }
}

/// A placement mode the Placement Bar shows and switches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlaceMode {
    /// `R`: `ignore` and `avoid`, kept in `pcb.place.obstacle_mode`.
    ObstacleMode,
    /// `pcb.place.alignment_guides`.
    AlignmentGuides,
    /// `N`: Breaks, Hidden and Pad To Pad, kept in `pcb.place.ratsnest_mode`.
    RatsnestMode,
}

impl PlaceMode {
    pub const ALL: [PlaceMode; 3] = [
        PlaceMode::ObstacleMode,
        PlaceMode::AlignmentGuides,
        PlaceMode::RatsnestMode,
    ];

    pub fn name(self) -> &'static str {
        match self {
            PlaceMode::ObstacleMode => "obstacle_mode",
            PlaceMode::AlignmentGuides => "alignment_guides",
            PlaceMode::RatsnestMode => "ratsnest_mode",
        }
    }

    fn preset(self) -> (&'static str, &'static str) {
        match self {
            PlaceMode::ObstacleMode => ("Next Placement Obstacle Mode", "place_obstacle_ignore"),
            PlaceMode::AlignmentGuides => ("Toggle Alignment Guides", "place_guides"),
            PlaceMode::RatsnestMode => ("Next Ratsnest Mode in a Move", "place_ratsnest_breaks"),
        }
    }
}

/// The next obstacle mode: `ignore` and `avoid` in turn.
pub fn next_obstacle(mode: ObstacleMode) -> ObstacleMode {
    match mode {
        ObstacleMode::Ignore => ObstacleMode::Avoid,
        ObstacleMode::Avoid => ObstacleMode::Ignore,
    }
}

/// The next Ratsnest mode in a move: Breaks, Hidden, Pad To Pad, and round again.
pub fn next_ratsnest(mode: RatsnestMode) -> RatsnestMode {
    match mode {
        RatsnestMode::Breaks => RatsnestMode::Hidden,
        RatsnestMode::Hidden => RatsnestMode::PadToPad,
        RatsnestMode::PadToPad => RatsnestMode::Breaks,
    }
}

/// Whether the selection is only Footprints (and not empty).
pub fn only_footprints(project: &Project, selection: &Selection) -> bool {
    let Some(pcb) = project.pcb() else {
        return false;
    };
    !selection.objects.is_empty()
        && selection.pads.is_empty()
        && selection.texts.is_empty()
        && selection
            .objects
            .iter()
            .all(|o| pcb.footprint(*o).is_some())
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModeParams {
    pub mode: PlaceMode,
}

#[derive(Serialize, JsonSchema)]
pub struct ModeAsked {
    pub mode: PlaceMode,
}

struct ModeCommand;

impl Command for ModeCommand {
    const META: Meta = Meta {
        id: "pcb.place.mode",
        title: "Placement Mode",
        description: "Steps or toggles a mode of Footprint moves: the obstacle mode (`ignore` \
                      or `avoid`), the alignment guides, or the Ratsnest shown during a move \
                      (Breaks, Hidden, Pad To Pad). Kept in the `pcb.place.*` Preferences. \
                      Only with only Footprints selected or a Footprint move in flight.",
        category: "tool",
        icon: "move",
        kind: Kind::View,
    };
    type Params = ModeParams;
    type Output = ModeAsked;

    fn presets(&self) -> Vec<Preset> {
        PlaceMode::ALL
            .iter()
            .map(|m| {
                let (title, icon) = m.preset();
                Preset::new(m.name(), title, icon, json!({ "mode": m.name() }))
            })
            .collect()
    }

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: ModeParams) -> Result<ModeAsked, CommandError> {
        let flight = cx.view::<PlaceFlight>()?.clone();
        let selection = cx.view::<Selection>()?.clone();
        let in_flight = flight.moving || flight.queue.is_some();
        if !in_flight && !only_footprints(cx.project()?, &selection) {
            return Err(CommandError::rejected(
                "only for Footprints: select only Footprints, or move one",
            ));
        }
        Ok(ModeAsked { mode: p.mode })
    }
}

/// The Footprints Reposition Selected brings, in pick order: each selected Footprint, and
/// for a selected Unit its Component's Footprint, once each; locked ones stay where they
/// are (a lock blocks Reposition).
pub fn queue_of(project: &Project, selection: &Selection) -> Vec<Uuid> {
    let Some(pcb) = project.pcb() else {
        return Vec::new();
    };
    let mut out: Vec<Uuid> = Vec::new();
    for o in &selection.objects {
        let found: Vec<Uuid> = match project.object(*o) {
            Some(Object::FootprintInstance(f)) => vec![f.object.uuid],
            Some(Object::Unit(unit)) => pcb
                .footprints()
                .filter(|f| f.component.as_ref() == Some(&unit.object.component))
                .map(|f| f.uuid)
                .collect(),
            _ => Vec::new(),
        };
        for f in found {
            if !out.contains(&f) && pcb.footprint(f).is_some_and(|f| !f.locked) {
                out.push(f);
            }
        }
    }
    out
}

#[derive(Serialize, JsonSchema)]
pub struct Queued {
    /// The Footprints queued, in the order they come.
    pub footprints: Vec<Uuid>,
}

struct Reposition;

impl Command for Reposition {
    const META: Meta = Meta {
        id: "pcb.place.reposition",
        title: "Reposition Selected",
        description: "Brings the selected Footprints to the cursor one at a time, in pick \
                      order (a selected Unit brings its Footprint, so cross-select's order \
                      counts). A click places one and brings the next; Tab skips one; Esc \
                      stops and leaves the rest where they are. Locked Footprints stay put.",
        category: "pcb",
        icon: "move",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = Queued;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<Queued, CommandError> {
        let selection = cx.view::<Selection>()?.clone();
        let footprints = queue_of(cx.project()?, &selection);
        if footprints.is_empty() {
            return Err(CommandError::rejected(
                "no unlocked Footprint is selected: select the Footprints to reposition",
            ));
        }
        Ok(Queued { footprints })
    }
}

#[derive(Serialize, JsonSchema)]
pub struct Done {}

struct Skip;

impl Command for Skip {
    const META: Meta = Meta {
        id: "pcb.place.skip",
        title: "Skip Footprint",
        description: "During Reposition Selected: leaves the Footprint on the cursor where it \
                      was and brings the next.",
        category: "tool",
        icon: "place_queue",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = Done;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<Done, CommandError> {
        if cx.view::<PlaceFlight>()?.queue.is_none() {
            return Err(CommandError::rejected("only during Reposition Selected"));
        }
        Ok(Done {})
    }
}

struct Choose;

impl Command for Choose {
    const META: Meta = Meta {
        id: "pcb.place.choose",
        title: "Choose Component",
        description: "Opens Choose Component: the Components by Designator, with a filter \
                      taking `?` and `*`, to jump to one or move it to the cursor.",
        category: "pcb",
        icon: "move",
        kind: Kind::View,
    };
    type Params = NoParams;
    type Output = Done;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<Done, CommandError> {
        cx.view::<PlaceFlight>()?.choose = Some(String::new());
        Ok(Done {})
    }
}

/// The rows Choose Component lists: (Designator, Footprint), by Designator in natural
/// order, matching `filter` (`?` one character, `*` any run, case ignored; without a
/// wildcard it matches the start).
pub fn choices(pcb: &PcbDocument, filter: &str) -> Vec<(String, Uuid)> {
    let filter = filter.trim().to_uppercase();
    let pattern = if filter.contains(['*', '?']) {
        filter
    } else {
        format!("{filter}*")
    };
    let mut rows: Vec<(String, Uuid)> = pcb
        .footprints()
        .filter(|f| reda_engine::pcb::glob_match(&pattern, &f.designator.to_uppercase()))
        .map(|f| (f.designator.clone(), f.uuid))
        .collect();
    rows.sort_by(|a, b| reda_engine::pcb::natural_cmp(&a.0, &b.0));
    rows
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PickUpParams {
    pub footprint: Uuid,
    /// `true`: Jump to component (the view goes to it and it floats from where it is);
    /// `false`: Move component to cursor.
    #[serde(default)]
    pub jump: bool,
}

#[derive(Serialize, JsonSchema)]
pub struct PickedUp {
    pub footprint: Uuid,
    pub jump: bool,
}

struct PickUp;

impl Command for PickUp {
    const META: Meta = Meta {
        id: "pcb.place.pick_up",
        title: "Pick Up Component",
        description: "Choose Component's buttons: floats a Footprint on the cursor, after \
                      jumping the view to it (`jump`) or moved to the cursor.",
        category: "pcb",
        icon: "move",
        kind: Kind::View,
    };
    type Params = PickUpParams;
    type Output = PickedUp;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: PickUpParams) -> Result<PickedUp, CommandError> {
        let pcb = cx
            .project()?
            .pcb()
            .ok_or_else(|| CommandError::rejected("the Project has no PCB Document"))?;
        let Some(f) = pcb.footprint(p.footprint) else {
            return Err(CommandError::not_found(format!(
                "no Footprint {}",
                p.footprint
            )));
        };
        if f.locked {
            return Err(CommandError::rejected(format!(
                "{} is locked; unlock it to move it",
                f.designator
            )));
        }
        cx.view::<PlaceFlight>()?.choose = None;
        Ok(PickedUp {
            footprint: p.footprint,
            jump: p.jump,
        })
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfirmParams {
    /// Yes: move or turn the locked objects too.
    pub proceed: bool,
}

#[derive(Serialize, JsonSchema)]
pub struct Confirmed {
    pub proceed: bool,
}

struct Confirm;

impl Command for Confirm {
    const META: Meta = Meta {
        id: "pcb.place.confirm",
        title: "Confirm Moving Locked Objects",
        description: "Answers the question a move or turn of locked objects asks with Protect \
                      Locked Objects off: `proceed` runs it with `include_locked`.",
        category: "pcb",
        icon: "lock",
        kind: Kind::View,
    };
    type Params = ConfirmParams;
    type Output = Confirmed;

    fn run(&self, cx: &mut Context<'_>, p: ConfirmParams) -> Result<Confirmed, CommandError> {
        if cx.view::<PlaceFlight>()?.confirm.is_none() {
            return Err(CommandError::rejected(
                "nothing is waiting for confirmation",
            ));
        }
        Ok(Confirmed { proceed: p.proceed })
    }
}

/// The open view state.
pub fn flight(shell: &Shell) -> PlaceFlight {
    shell
        .engine()
        .view()
        .get::<PlaceFlight>()
        .cloned()
        .unwrap_or_default()
}

pub(crate) fn flight_mut(shell: &mut Shell) -> Option<&mut PlaceFlight> {
    shell.engine_mut().view_mut().get_mut::<PlaceFlight>()
}

/// Runs a move's or turn's Commands (`pcb.move`, `pcb.rotate`, `pcb.flip`), as one
/// `batch` when there are several. With Protect Locked Objects off and a locked object
/// among them it asks first: the Commands wait in [`PlaceFlight::confirm`]. Returns
/// whether they ran and succeeded.
pub(crate) fn commit(shell: &mut Shell, commands: Vec<(String, Value)>) -> bool {
    if commands.is_empty() {
        return false;
    }
    let locked = locked_in(shell, &commands);
    if locked > 0 && !shell.preferences().editing.protect_locked_objects {
        if let Some(f) = flight_mut(shell) {
            f.confirm = Some(Pending {
                commands,
                locked,
                unroute: false,
            });
        }
        return false;
    }
    run_all(shell, commands)
}

fn run_all(shell: &mut Shell, mut commands: Vec<(String, Value)>) -> bool {
    if commands.len() == 1 {
        let (id, params) = commands.remove(0);
        return shell.run_reporting(&id, params).is_ok();
    }
    let steps: Vec<Value> = commands
        .into_iter()
        .map(|(id, params)| json!({"id": id, "params": params}))
        .collect();
    shell
        .run_reporting("batch", json!({ "commands": steps }))
        .is_ok()
}

/// How many locked objects `commands` move or turn.
fn locked_in(shell: &Shell, commands: &[(String, Value)]) -> usize {
    let Some(pcb) = shell.engine().project().and_then(|p| p.pcb()) else {
        return 0;
    };
    let mut objects: Vec<Uuid> = commands
        .iter()
        .flat_map(|(_, p)| p["objects"].as_array().cloned().unwrap_or_default())
        .filter_map(|v| v.as_str().and_then(|s| s.parse().ok()))
        .collect();
    objects.sort();
    objects.dedup();
    objects
        .into_iter()
        .filter(|o| reda_engine::pcb::lock::is_locked(pcb, *o))
        .count()
}

/// Applies a placement Command after it ran: what the Shell's `after_edit` hands here.
pub(crate) fn after_run(shell: &mut Shell, id: &str, out: &Value) {
    match id {
        "pcb.place.mode" => {
            let Ok(mode) = serde_json::from_value::<PlaceMode>(out["mode"].clone()) else {
                return;
            };
            let prefs = &shell.preferences().pcb.place;
            let value = match mode {
                PlaceMode::ObstacleMode => json!(next_obstacle(prefs.obstacle_mode)),
                PlaceMode::AlignmentGuides => json!(!prefs.alignment_guides),
                PlaceMode::RatsnestMode => json!(next_ratsnest(prefs.ratsnest_mode)),
            };
            let key = format!("pcb.place.{}", mode.name());
            let _ = shell.run_reporting("app.preferences.set", json!({"key": key, "value": value}));
            shell.with_pcb_tool(|tool, shell| tool.refresh(shell));
        }
        "pcb.place.reposition" => {
            let footprints: Vec<Uuid> =
                serde_json::from_value(out["footprints"].clone()).unwrap_or_default();
            shell.with_pcb_tool(|tool, shell| tool.start_queue(shell, footprints));
        }
        "pcb.place.skip" => shell.with_pcb_tool(|tool, shell| tool.skip(shell)),
        "pcb.place.pick_up" => {
            let Ok(footprint) = serde_json::from_value::<Uuid>(out["footprint"].clone()) else {
                return;
            };
            let jump = out["jump"] == true;
            if jump {
                let _ = shell.run_reporting("view.zoom_to", json!({ "objects": [footprint] }));
            }
            shell.with_pcb_tool(|tool, shell| tool.pick_up(shell, footprint, jump));
        }
        "pcb.place.confirm" => {
            let pending = flight_mut(shell).and_then(|f| f.confirm.take());
            if let Some(pending) = pending
                && out["proceed"] == true
            {
                let commands = pending
                    .commands
                    .into_iter()
                    .map(|(id, mut params)| {
                        params["include_locked"] = json!(true);
                        (id, params)
                    })
                    .collect();
                run_all(shell, commands);
            } else if let Some(document) = shell
                .engine()
                .project()
                .and_then(|p| p.pcb())
                .map(|p| p.uuid)
            {
                shell.redraw(document);
            }
        }
        _ => {}
    }
}
