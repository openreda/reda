//! Stack ordering, clicks, rectangles, drags and cycling shared by all canvas editors.
//! Dragging text is in [`moving_text`], the double-click pick list in [`pick_list`].

mod moving_text;
mod pick_list;

use std::collections::BTreeMap;
use std::time::Instant;

use reda_engine::command::{Command, CommandError, Context, Kind, Meta};
use reda_engine::model::Uuid;
use reda_engine::render::{Item, Scene, SceneLayer};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

pub use moving_text::text_pointer;
pub use pick_list::{double_click, show};

use super::{Selection, SelectionSetResult, rules};
use crate::canvas::interact::{Combine, click_selection, combine, hit_stack_on};
use crate::shell::Shell;
use crate::working_state::FilterKind;

/// The objects under one pointer position, best first, and which one is picked.
#[derive(Clone, Debug)]
struct Stack {
    at: (f64, f64),
    tolerance: f64,
    objects: Vec<Uuid>,
    index: usize,
    /// When a plain click last picked from it; a repeat click soon after cycles.
    time: f64,
    targets: BTreeMap<Uuid, Selection>,
}

/// Each canvas's picking state, kept in the engine's view state.
#[derive(Debug)]
pub struct Picking {
    stacks: BTreeMap<Uuid, Stack>,
    epoch: Instant,
    time: Option<f64>,
    moving: Option<moving_text::MovingText>,
    list: Option<pick_list::PickList>,
    /// The selection before the click that began the current click gesture, by Document:
    /// a double-click opens its pick list over it.
    gesture: Option<(Uuid, Selection)>,
    idle: bool,
}

impl Default for Picking {
    fn default() -> Self {
        Self {
            stacks: BTreeMap::new(),
            epoch: Instant::now(),
            time: None,
            moving: None,
            list: None,
            gesture: None,
            idle: true,
        }
    }
}

impl Picking {
    fn now(&self) -> f64 {
        self.time
            .unwrap_or_else(|| self.epoch.elapsed().as_secs_f64())
    }

    /// The canvas's input time (seconds) for this frame, which repeat clicks are timed by.
    pub fn set_time(&mut self, time: f64) {
        self.time = Some(time);
    }

    /// Whether no canvas operation (a drag) is in progress, so Shift+Tab may cycle.
    pub fn set_idle(&mut self, idle: bool) {
        self.idle = idle;
    }
}

/// The board's explicit tiers, with sheet body primitives after Pins and text.
fn rank(shell: &Shell, document: Uuid, item: &Item) -> Option<u8> {
    rank_with_locks(shell, document, item, true)
}

fn rank_with_locks(shell: &Shell, document: Uuid, item: &Item, protect: bool) -> Option<u8> {
    if item.layer == SceneLayer::Ratsnest || !rules::item_pickable(shell, document, item, protect) {
        return None;
    }
    let kind = rules::item_kind(shell, document, item)?;
    if crate::bars::document_type(shell, document) == Some(crate::bars::DocumentType::Footprint) {
        let active = shell
            .document_tab(document)
            .and_then(|t| t.scene.layers.as_deref())
            .and_then(|ls| {
                ls.iter()
                    .rev()
                    .find(|c| matches!(c.layer, SceneLayer::Board(_)))
            })
            .map(|c| c.layer);
        return Some(
            if Some(item.layer) == active || item.layer == SceneLayer::Drill {
                0
            } else {
                1
            },
        );
    }
    let Some(project) = shell
        .engine()
        .project()
        .filter(|p| p.pcb_document(document).is_some())
    else {
        use reda_engine::render::{ColorKey, Primitive};
        let body = kind == FilterKind::Unit
            && matches!(item.color, ColorKey::Schematic(c) if c.is_unit_body())
            && !matches!(item.primitive, Primitive::Glyphs(_));
        return Some(u8::from(body));
    };
    Some(match kind {
        FilterKind::FootprintInstance => 2,
        FilterKind::Zone | FilterKind::Keepout | FilterKind::ZoneCutout => 3,
        FilterKind::BoardOutline | FilterKind::BoardCutout => 4,
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
        &document_scene(shell, document),
        tab.scene.layers.as_deref(),
        at,
        tolerance,
        |i| rank(shell, document, i),
    )
}

pub fn hit(shell: &Shell, document: Uuid, at: (f64, f64), tolerance: f64) -> Option<Uuid> {
    stack(shell, document, at, tolerance).first().copied()
}

/// The `selection.set` Command that selects `selection`.
pub fn set_command(selection: &Selection) -> (String, Value) {
    (
        "selection.set".into(),
        serde_json::to_value(selection).expect("selection serializes"),
    )
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
    let before = shell.selection().clone();
    let kept_segments = before.segments.clone();
    let current = before.canvas_ids();
    let targets: BTreeMap<_, _> = objects
        .iter()
        .map(|id| (*id, selected(shell, document, &[*id])))
        .collect();
    let picker = shell.selection_picking_mut();
    let now = picker.now();
    // Only plain clicks cycle: a Shift+click toggles, and the plain click after it on
    // the same spot starts over at the top of the stack.
    let repeat = picker
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
        .map(|s| s.index);
    let index = repeat.map_or(0, |i| (i + 1) % objects.len());
    if repeat.is_none() || picker.gesture.as_ref().is_none_or(|(d, _)| *d != document) {
        picker.gesture = Some((document, before));
    }
    let picked: Vec<_> = objects.get(index).copied().into_iter().collect();
    picker.stacks.insert(
        document,
        Stack {
            at,
            tolerance,
            objects,
            index,
            time: if how == Combine::Replace {
                now
            } else {
                f64::NEG_INFINITY
            },
            targets,
        },
    );
    let selected = click_selection(&current, &picked, how);
    let mut selection = self::selected(shell, document, &selected);
    // Shift keeps the Wire segments selected, but those of a Wire now selected whole.
    if how != Combine::Replace {
        selection.segments = (kept_segments.into_iter())
            .filter(|s| !selection.objects.contains(&s.wire))
            .collect();
    }
    Some(set_command(&selection))
}

/// How long after the click that picked something a click on it again is a slow second
/// click (in-place edit) rather than a double-click or a cycling repeat, in seconds.
pub const SLOW_SECOND_CLICK: f64 = 0.6;

/// What a plain click at `at` hits when it is a slow second click: it lands on exactly
/// what is selected, which the last plain click picked from the same spot more than
/// [`SLOW_SECOND_CLICK`] ago. The hit is an object, or a field's child id.
pub fn slow_second_click(
    shell: &Shell,
    document: Uuid,
    at: (f64, f64),
    tolerance: f64,
) -> Option<Uuid> {
    let picker = shell.engine().view().get::<Picking>()?;
    let last = picker.stacks.get(&document)?;
    let now = picker.now();
    let top = *stack(shell, document, at, tolerance).first()?;
    let picked = last.objects.get(last.index) == Some(&top);
    let slow = last.time.is_finite() && now - last.time > SLOW_SECOND_CLICK;
    let only = shell.selection().canvas_ids() == selected(shell, document, &[top]).canvas_ids();
    (picked && slow && only).then_some(top)
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
        if cx.view::<crate::tools::ActiveTool>()?.0 != crate::tools::Tool::Select {
            return Err(CommandError::unavailable(
                "finish the active Tool before cycling",
            ));
        }
        let current = cx.view::<Selection>()?.canvas_ids();
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
        let previous = (current.len() == 1)
            .then(|| stack.objects.iter().position(|id| *id == current[0]))
            .flatten()
            .unwrap_or(stack.index);
        stack.index = (previous + 1) % stack.objects.len();
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
    use reda_engine::render::Child;
    let mut selected = Selection::default();
    for id in ids {
        let child = rules::child(shell, document, *id);
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
    let scene = document_scene(shell, document);
    let picked = crate::canvas::interact::box_select_on(
        &scene,
        tab.scene.layers.as_deref(),
        from,
        to,
        rules::box_mode(shell, document, from, to),
        |i| rank(shell, document, i).is_some(),
    );
    let picked = in_designator_order(shell, picked);
    let ids = combine(
        &shell.selection().canvas_ids(),
        &picked,
        Combine::of(modifiers),
    );
    Some(set_command(&selected(shell, document, &ids)))
}

/// A box has no pick order: Units and Footprints in it go in natural Designator order
/// (R1, R2, R10), ahead of the rest, which keep their order. Cross-select and the place
/// queue read this order.
fn in_designator_order(shell: &Shell, mut picked: Vec<Uuid>) -> Vec<Uuid> {
    use reda_engine::model::Object;
    let Some(project) = shell.engine().project() else {
        return picked;
    };
    let designator = |o: &Uuid| match project.object(*o) {
        Some(Object::Unit(u)) => Some(u.object.designator.clone()),
        Some(Object::FootprintInstance(f)) => Some(f.object.designator.clone()),
        _ => None,
    };
    let mut keyed: Vec<(Option<String>, Uuid)> =
        picked.drain(..).map(|o| (designator(&o), o)).collect();
    keyed.sort_by(|(a, _), (b, _)| match (a, b) {
        (Some(a), Some(b)) => reda_engine::schematic::natural_cmp(a, b),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    keyed.into_iter().map(|(_, o)| o).collect()
}

/// What a drag picks up: the object under the pointer, or the selected parent of the
/// child under it, so a selected Component drags with its text.
pub fn movement_hit(shell: &Shell, document: Uuid, at: (f64, f64), tolerance: f64) -> Option<Uuid> {
    let id = hit(shell, document, at, tolerance)?;
    let child = rules::child(shell, document, id);
    Some(
        child
            .filter(|c| {
                shell.selection().objects.contains(&c.owner())
                    && rules::pickable(shell, document, c.owner())
            })
            .map_or(id, |c| c.owner()),
    )
}

/// What a drag that starts on `hit` selects and moves, the same on every canvas.
pub struct DragStart {
    /// `selection.set` for the selection the drag carries, when it isn't the current one.
    pub select: Option<(String, Value)>,
    /// The carried objects that move: pickable, and accepted by the canvas's `movable`.
    /// Filtered or protected members stay selected, and stay put.
    pub objects: Vec<Uuid>,
}

/// A drag on a selected object carries the whole selection; on anything else it selects
/// that object first (Shift adds it to the selection).
pub fn drag_start(
    shell: &Shell,
    document: Uuid,
    hit: Uuid,
    modifiers: egui::Modifiers,
    movable: impl Fn(Uuid) -> bool,
) -> DragStart {
    let current = shell.selection();
    let ids = current.canvas_ids();
    let carried = if ids.contains(&hit) {
        current.clone()
    } else if modifiers.shift {
        selected(shell, document, &combine(&ids, &[hit], Combine::Add))
    } else {
        selected(shell, document, &[hit])
    };
    let objects = carried
        .objects
        .iter()
        .copied()
        .filter(|id| movable(*id) && rules::pickable(shell, document, *id))
        .collect();
    DragStart {
        select: (carried != *current).then(|| set_command(&carried)),
        objects,
    }
}

/// Keyboard Commands run before the next canvas hover. Rebuild at the cached pointer
/// position so current Filters, visible Layers, locks and edits govern cycling too.
pub fn prepare_cycle(shell: &mut Shell, document: Uuid) {
    let position = shell
        .selection_picking_mut()
        .stacks
        .get(&document)
        .map(|s| (s.at, s.tolerance));
    if let Some((at, tolerance)) = position {
        hover(shell, document, at, tolerance);
    }
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
            tolerance,
            objects,
            index,
            time: f64::NEG_INFINITY,
            targets,
        },
    );
}

/// A Footprint can be picked by its body even when its only ink consists of Pads.
fn document_scene(shell: &Shell, document: Uuid) -> Scene {
    let Some(tab) = shell.document_tab(document) else {
        return Scene::default();
    };
    let mut scene = pick_scene(&tab.scene.scene);
    if let Some(pcb) = shell
        .engine()
        .project()
        .and_then(|p| p.pcb_document(document))
    {
        let layer = shell
            .board_view()
            .active_layer(shell.engine().project().expect("board Project"))
            .map(SceneLayer::Board)
            .unwrap_or(SceneLayer::Drill);
        for footprint in pcb.footprints() {
            let b = reda_engine::pcb::placed::footprint_bounds(pcb, footprint);
            scene.items.push(Item {
                object: footprint.uuid,
                child: None,
                layer,
                color: reda_engine::render::ColorKey::Board(reda_engine::render::BoardColor::Hole),
                primitive: reda_engine::render::Primitive::Rect {
                    center: b.center(),
                    size: (b.width(), b.height()),
                    rotation: reda_engine::model::geom::Millideg(0),
                    corner_radius: reda_engine::model::geom::Nm(0),
                    stroke: None,
                },
                look: Default::default(),
            });
        }
    }
    scene
}

/// Everything eligible on this canvas, with children collapsed into a picked parent.
pub fn all(shell: &Shell, document: Uuid) -> Selection {
    let scene = document_scene(shell, document);
    let layers = shell
        .document_tab(document)
        .and_then(|t| t.scene.layers.as_deref());
    let mut ids = Vec::new();
    for item in &scene.items {
        if layers.is_none_or(|ls| ls.iter().any(|c| c.layer == item.layer))
            && rank_with_locks(shell, document, item, false).is_some()
            && !ids.contains(&item.object)
        {
            ids.push(item.object);
        }
    }
    selected(shell, document, &ids)
}

pub fn cancel(shell: &mut Shell) {
    let picker = shell.selection_picking_mut();
    let document = picker.moving.take().map(|m| m.document);
    picker.list = None;
    picker.idle = true;
    if let Some(tab) = document.and_then(|d| shell.document_tab_mut(d)) {
        tab.live = crate::canvas::LiveHandle::default();
        tab.moving_offset = (0.0, 0.0);
    }
}
