//! Stack ordering and cycling shared by all canvas editors.

use std::collections::BTreeMap;
use std::time::Instant;

use reda_engine::command::{Command, CommandError, Context, Kind, Meta};
use reda_engine::model::Uuid;
use reda_engine::render::{Child, Item, Scene, SceneLayer};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{Selection, SelectionSetResult, rules};
use crate::canvas::interact::{Combine, click_selection, hit_stack_on};
use crate::shell::Shell;

#[derive(Clone, Debug)]
pub struct Stack {
    pub at: (f64, f64),
    pub objects: Vec<Uuid>,
    pub index: usize,
    pub time: f64,
    pub targets: BTreeMap<Uuid, Selection>,
}

#[derive(Debug)]
pub struct Picking {
    pub stacks: BTreeMap<Uuid, Stack>,
    epoch: Instant,
    pub time: Option<f64>,
    moving: Option<MovingText>,
    pub list: Option<PickList>,
    pub idle: bool,
}

impl Default for Picking {
    fn default() -> Self {
        Self {
            stacks: BTreeMap::new(),
            epoch: Instant::now(),
            time: None,
            moving: None,
            list: None,
            idle: true,
        }
    }
}

impl Picking {
    fn now(&self) -> f64 {
        self.time
            .unwrap_or_else(|| self.epoch.elapsed().as_secs_f64())
    }
}

/// The board's explicit tier order; a sheet has only the size ordering.
fn rank(shell: &Shell, document: Uuid, item: &Item) -> Option<u8> {
    rank_with_locks(shell, document, item, true)
}

fn rank_with_locks(shell: &Shell, document: Uuid, item: &Item, protect: bool) -> Option<u8> {
    if protect
        && shell.preferences().editing.protect_locked_objects
        && rules::locked(shell, document, item.object)
    {
        return None;
    }
    if item.layer == SceneLayer::Ratsnest || !rules::allowed(shell, document, item.object) {
        return None;
    }
    let Some(project) = shell
        .engine()
        .project()
        .filter(|p| p.pcb_document(document).is_some())
    else {
        return Some(0);
    };
    let kind = rules::kind(shell, document, item.object)?;
    Some(match kind.as_str() {
        "footprint_instance" => 2,
        "zone" | "keepout" => 3,
        "board_outline" | "board_cutout" => 4,
        _ if item.layer
            == shell
                .board_view()
                .active_layer(project)
                .map(SceneLayer::Board)
                .unwrap_or(SceneLayer::Drill) =>
        {
            0
        }
        _ => 1,
    })
}

pub fn stack(shell: &Shell, document: Uuid, at: (f64, f64), tolerance: f64) -> Vec<Uuid> {
    let Some(tab) = shell.document_tab(document) else {
        return Vec::new();
    };
    hit_stack_on(
        &pick_scene(&tab.scene.scene),
        tab.scene.layers.as_deref(),
        at,
        tolerance,
        |i| rank(shell, document, i),
    )
}

pub fn hit(shell: &Shell, document: Uuid, at: (f64, f64), tolerance: f64) -> Option<Uuid> {
    stack(shell, document, at, tolerance).first().copied()
}

/// Chooses a hit, cycles a repeated click, and returns the normal public selection Command.
pub fn click(
    shell: &mut Shell,
    document: Uuid,
    at: (f64, f64),
    tolerance: f64,
    modifiers: egui::Modifiers,
) -> Option<(String, Value)> {
    let objects = stack(shell, document, at, tolerance);
    let how = Combine::of(modifiers);
    if objects.is_empty()
        && (how != Combine::Replace || !shell.preferences().editing.click_clears_selection)
    {
        return None;
    }
    let current = shell.selection().canvas_ids();
    let targets: BTreeMap<_, _> = objects
        .iter()
        .map(|id| (*id, selected(shell, document, &[*id])))
        .collect();
    let picker = shell.selection_picking_mut();
    let now = picker.now();
    let index = picker
        .stacks
        .get(&document)
        .filter(|s| {
            how == Combine::Replace
                && objects.len() > 1
                && objects == s.objects
                && (at.0 - s.at.0).hypot(at.1 - s.at.1) <= tolerance / 5.0 * 3.0
                && now >= s.time
                && now - s.time <= 0.6
        })
        .map_or(0, |s| (s.index + 1) % objects.len());
    let picked: Vec<_> = objects.get(index).copied().into_iter().collect();
    picker.stacks.insert(
        document,
        Stack {
            at,
            objects,
            index,
            time: now,
            targets,
        },
    );
    let selected = click_selection(&current, &picked, how);
    Some((
        "selection.set".into(),
        serde_json::to_value(self::selected(shell, document, &selected))
            .expect("selection serializes"),
    ))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CycleParams {
    document: Uuid,
}

pub struct Cycle;
impl Command for Cycle {
    const META: Meta = Meta {
        id: "selection.cycle",
        title: "Cycle Selection",
        description: "Selects the next object in the stack under the last idle pointer position.",
        category: "selection",
        icon: "select",
        kind: Kind::View,
    };
    type Params = CycleParams;
    type Output = SelectionSetResult;
    fn run(&self, cx: &mut Context<'_>, p: CycleParams) -> Result<Self::Output, CommandError> {
        let picker = cx.view::<Picking>()?;
        if !picker.idle {
            return Err(CommandError::unavailable(
                "finish the canvas operation before cycling",
            ));
        }
        let Some(stack) = picker
            .stacks
            .get_mut(&p.document)
            .filter(|s| !s.objects.is_empty())
        else {
            return Ok(SelectionSetResult { count: 0 });
        };
        stack.index = (stack.index + 1) % stack.objects.len();
        let selection = stack
            .targets
            .get(&stack.objects[stack.index])
            .cloned()
            .unwrap_or_default();
        *cx.view::<Selection>()? = selection;
        Ok(SelectionSetResult { count: 1 })
    }
}

/// A picking view of the Scene, preserving parent tags in the rendering Scene.
pub fn pick_scene(scene: &Scene) -> Scene {
    let mut scene = scene.clone();
    for item in &mut scene.items {
        if let Some(child) = &item.child {
            item.object = child.id();
        }
    }
    scene
}

pub fn selected(shell: &Shell, document: Uuid, ids: &[Uuid]) -> Selection {
    let mut selected = Selection::default();
    for id in ids {
        let child = shell.document_tab(document).and_then(|t| {
            t.scene
                .scene
                .items
                .iter()
                .filter_map(|i| i.child.as_ref())
                .find(|c| c.id() == *id)
        });
        match child {
            Some(child) if ids.contains(&child.owner()) => {}
            Some(Child::Field { unit, field }) => selected.fields.push(super::FieldRef {
                unit: *unit,
                field: field.clone(),
            }),
            Some(Child::Pad { footprint, pad }) => selected.pads.push(super::PadRef {
                footprint: *footprint,
                pad: *pad,
            }),
            Some(Child::FootprintText { footprint, text }) => selected.texts.push(super::TextRef {
                footprint: *footprint,
                text: *text,
            }),
            None => selected.objects.push(*id),
        }
    }
    selected
}

pub fn box_selection(
    shell: &Shell,
    document: Uuid,
    from: (f64, f64),
    to: (f64, f64),
    modifiers: egui::Modifiers,
) -> Option<(String, Value)> {
    let tab = shell.document_tab(document)?;
    let scene = pick_scene(&tab.scene.scene);
    let picked = crate::canvas::interact::box_select_on(
        &scene,
        tab.scene.layers.as_deref(),
        from,
        to,
        rules::box_mode(shell, from, to),
        |i| rank(shell, document, i).is_some(),
    );
    let ids = crate::canvas::interact::combine(
        &shell.selection().canvas_ids(),
        &picked,
        Combine::of(modifiers),
    );
    Some((
        "selection.set".into(),
        serde_json::to_value(selected(shell, document, &ids)).expect("selection serializes"),
    ))
}

#[derive(Clone, Debug)]
pub struct MovingText {
    document: Uuid,
    child: Child,
    grab: reda_engine::model::geom::Point,
    start: reda_engine::model::geom::Point,
    by: reda_engine::model::geom::Point,
}

/// Text is a real child target: dragging it commits its placement Command, never a
/// parent move. A selected parent continues to carry all of its own text.
pub fn text_pointer(
    shell: &mut Shell,
    document: Uuid,
    events: &[crate::canvas::interact::Pointer],
    tolerance: f64,
) -> Option<Vec<(String, Value)>> {
    use crate::canvas::LiveHandle;
    use crate::canvas::interact::{Pointer, snap_to_grid};
    use reda_engine::model::geom::Point;
    if shell.tool() != crate::tools::Tool::Select {
        return None;
    }
    let mut moving = shell.selection_picking_mut().moving.take();
    let mut commands = Vec::new();
    let mut consumed = moving.is_some();
    for event in events {
        if moving.is_none() {
            if let Pointer::DragStart { from, at, .. } = *event {
                let hit = hit(shell, document, from, tolerance)?;
                let item = shell
                    .document_tab(document)?
                    .scene
                    .scene
                    .items
                    .iter()
                    .find(|i| i.child.as_ref().is_some_and(|c| c.id() == hit))?;
                let child = item.child.clone()?;
                if matches!(child, Child::Pad { .. })
                    || shell.selection().objects.contains(&child.owner())
                {
                    return None;
                }
                let reda_engine::render::Primitive::Glyphs(run) = &item.primitive else {
                    return None;
                };
                let start = run.at;
                let grab = snap_to_grid(from, shell.grid(document));
                let by = snap_to_grid(at, shell.grid(document)) - grab;
                commands.push((
                    "selection.set".into(),
                    serde_json::to_value(selected(shell, document, &[hit]))
                        .expect("selection serializes"),
                ));
                moving = Some(MovingText {
                    document,
                    child,
                    grab,
                    start,
                    by,
                });
                consumed = true;
            }
            continue;
        }
        let m = moving.as_mut().expect("a move is active");
        if m.document != document {
            break;
        }
        match *event {
            Pointer::Drag(at) | Pointer::Hover(at) => {
                m.by = snap_to_grid(at, shell.grid(document)) - m.grab
            }
            Pointer::DragEnd(at) => {
                m.by = snap_to_grid(at, shell.grid(document)) - m.grab;
                if m.by != Point::default() {
                    let at = m.start + m.by;
                    commands.push(match &m.child {
                        Child::Field {unit,field} => ("schematic.field.move".into(), json!({"unit":unit,"field":field,"at":{"x":at.x.0,"y":at.y.0}})),
                        Child::FootprintText {footprint,text} => ("pcb.footprint.text.set".into(), json!({"instances":[footprint],"texts":[text],"at":{"x":at.x.0,"y":at.y.0}})),
                        Child::Pad {..} => unreachable!(),
                    });
                }
                moving = None;
            }
            _ => {}
        }
    }
    if consumed {
        if let Some(tab) = shell.document_tab_mut(document) {
            let ids: Vec<_> = moving.iter().map(|m| m.child.id()).collect();
            tab.live = LiveHandle::new(None, &ids, &[]);
            tab.moving_offset = moving
                .as_ref()
                .map_or((0.0, 0.0), |m| (m.by.x.0 as f64, m.by.y.0 as f64));
        }
    }
    shell.selection_picking_mut().moving = moving;
    consumed.then_some(commands)
}

pub fn movement_hit(shell: &Shell, document: Uuid, at: (f64, f64), tolerance: f64) -> Option<Uuid> {
    let id = hit(shell, document, at, tolerance)?;
    let child = shell
        .document_tab(document)?
        .scene
        .scene
        .items
        .iter()
        .filter_map(|i| i.child.as_ref())
        .find(|c| c.id() == id);
    Some(
        child
            .filter(|c| shell.selection().objects.contains(&c.owner()))
            .map_or(id, |c| c.owner()),
    )
}

/// Hover supplies Shift+Tab's stack even before the first click.
pub fn hover(shell: &mut Shell, document: Uuid, at: (f64, f64), tolerance: f64) {
    let objects = stack(shell, document, at, tolerance);
    let targets = objects
        .iter()
        .map(|id| (*id, selected(shell, document, &[*id])))
        .collect();
    let current = shell.selection().canvas_ids();
    let picker = shell.selection_picking_mut();
    if picker
        .stacks
        .get(&document)
        .is_some_and(|s| s.objects == objects)
    {
        return;
    }
    let index = objects
        .iter()
        .position(|id| current.contains(id))
        .unwrap_or(objects.len().saturating_sub(1));
    picker.stacks.insert(
        document,
        Stack {
            at,
            objects,
            index,
            time: f64::NEG_INFINITY,
            targets,
        },
    );
}

#[derive(Clone, Debug)]
pub struct PickList {
    document: Uuid,
    objects: Vec<Uuid>,
    at: egui::Pos2,
}

/// A double click explicitly offers protected objects as well, so they can be unlocked.
pub fn double_click(
    shell: &mut Shell,
    document: Uuid,
    at: (f64, f64),
    tolerance: f64,
    screen: egui::Pos2,
) {
    let Some(tab) = shell.document_tab(document) else {
        return;
    };
    let scene = pick_scene(&tab.scene.scene);
    let mut objects = hit_stack_on(&scene, tab.scene.layers.as_deref(), at, tolerance, |i| {
        rank_with_locks(shell, document, i, false)
    });
    if objects.len() <= 1 {
        if let Some(id) = objects.first() {
            let params = serde_json::to_value(selected(shell, document, &[*id]))
                .expect("selection serializes");
            let _ = shell.run_reporting("selection.set", params);
            let _ = shell.run_reporting("panel.show", json!({"panel":"properties"}));
        }
        return;
    }
    let layer = |id: Uuid| scene.items.iter().find(|i| i.object == id).map(|i| i.layer);
    objects.sort_by_key(|id| (layer(*id), *id));
    shell.selection_picking_mut().list = Some(PickList {
        document,
        objects,
        at: screen,
    });
}

/// Layer-grouped visual choices with a live canvas preview. Hover never changes Selection.
pub fn show(ui: &egui::Ui, shell: &mut Shell, document: Uuid) {
    let Some(list) = shell
        .selection_picking_mut()
        .list
        .clone()
        .filter(|l| l.document == document)
    else {
        return;
    };
    let mut open = true;
    let mut preview = None;
    let mut choose = None;
    let scene = shell
        .document_tab(document)
        .map(|t| pick_scene(&t.scene.scene))
        .unwrap_or_default();
    egui::Window::new("Pick Object")
        .id(egui::Id::NULL.with(("pick-object", document)))
        .default_pos(list.at)
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ui.ctx(), |ui| {
            let mut previous = None;
            for id in &list.objects {
                if !rules::allowed(shell, document, *id) {
                    continue;
                }
                let layer = scene
                    .items
                    .iter()
                    .find(|i| i.object == *id)
                    .map(|i| i.layer);
                if layer != previous {
                    let name = match layer {
                        Some(SceneLayer::Board(layer)) => shell
                            .engine()
                            .project()
                            .and_then(|p| p.pcb_document(document))
                            .and_then(|p| p.layers().layer(layer))
                            .map(|l| format!("{:?} {:?}", l.side, l.kind))
                            .unwrap_or_else(|| "Layer".into()),
                        Some(SceneLayer::Drill) => "Through hole".into(),
                        _ => "Sheet".into(),
                    };
                    ui.strong(name);
                    previous = layer;
                }
                let kind = rules::kind(shell, document, *id).unwrap_or_else(|| "object".into());
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(28.0, 20.0), egui::Sense::hover());
                    let color = ui.visuals().selection.stroke.color;
                    ui.painter().rect_stroke(
                        rect.shrink(3.0),
                        2.0,
                        egui::Stroke::new(1.5, color),
                        egui::StrokeKind::Inside,
                    );
                    let row = ui.selectable_label(false, format!("{kind} · {id}"));
                    if row.hovered() {
                        preview = Some(*id);
                    }
                    if row.clicked() {
                        choose = Some(*id);
                    }
                    if rules::locked(shell, document, *id) {
                        ui.label("🔒").on_hover_text("Locked");
                    }
                });
            }
        });
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        open = false;
    }
    if let Some(id) = choose {
        let params =
            serde_json::to_value(selected(shell, document, &[id])).expect("selection serializes");
        let _ = shell.run_reporting("selection.set", params);
        open = false;
    }
    if let Some(tab) = shell.document_tab_mut(document) {
        tab.live = crate::canvas::LiveHandle::new(if open { preview } else { None }, &[], &[]);
    }
    if !open {
        shell.selection_picking_mut().list = None;
    }
}
