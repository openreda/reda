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
    pub tolerance: f64,
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

/// The board's explicit tiers, with sheet body primitives after Pins and text.
fn rank(shell: &Shell, document: Uuid, item: &Item) -> Option<u8> {
    rank_with_locks(shell, document, item, true)
}

fn rank_with_locks(shell: &Shell, document: Uuid, item: &Item, protect: bool) -> Option<u8> {
    if item.layer == SceneLayer::Ratsnest {
        return None;
    }
    let kind = rules::item_kind(shell, document, item)?;
    if !shell.selection_filter(document).allows(&kind)
        || !crate::bars::document_type(shell, document)
            .is_some_and(|ty| crate::working_state::filter_kinds(ty).contains(&kind.as_str()))
    {
        return None;
    }
    if protect
        && shell.preferences().editing.protect_locked_objects
        && !matches!(item.child, Some(Child::FootprintText { .. }))
    {
        let owner = item.child.as_ref().map_or(item.object, Child::owner);
        if shell
            .engine()
            .project()
            .and_then(|p| p.pcb_document(document))
            .and_then(|p| p.footprint(owner))
            .is_some_and(|f| f.locked)
        {
            return None;
        }
    }
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
        use reda_engine::render::{ColorKey, Primitive, SchematicColor};
        let body = kind == "unit"
            && matches!(
                item.color,
                ColorKey::Schematic(SchematicColor::Body | SchematicColor::BodyFill)
            )
            && !matches!(item.primitive, Primitive::Glyphs(_));
        return Some(u8::from(body));
    };
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
            tolerance,
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
    targets: Vec<(Child, reda_engine::model::geom::Point)>,
    objects: Vec<Uuid>,
    schematic: bool,
    keep_connections: bool,
    grab: reda_engine::model::geom::Point,
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
    use crate::canvas::interact::Pointer;
    use reda_engine::model::geom::Point;
    if shell.tool() != crate::tools::Tool::Select {
        return None;
    }
    let mut moving = shell.selection_picking_mut().moving.take();
    let mut commands = Vec::new();
    let mut consumed = moving.is_some();
    for event in events {
        if moving.is_none() {
            if let Pointer::DragStart {
                from,
                at,
                modifiers,
            } = *event
            {
                let hit = hit(shell, document, from, tolerance)?;
                let current = shell.selection().clone();
                let child = rules::child(shell, document, hit).cloned();
                let standalone_text = current
                    .fields
                    .iter()
                    .any(|f| !current.objects.contains(&f.unit))
                    || current
                        .texts
                        .iter()
                        .any(|t| !current.objects.contains(&t.footprint));
                let parent_selected = child.as_ref().is_some_and(|c| {
                    current.objects.contains(&c.owner())
                        && rules::allowed(shell, document, c.owner())
                        && !(shell.preferences().editing.protect_locked_objects
                            && rules::locked(shell, document, c.owner()))
                });
                if matches!(child, Some(Child::Pad { .. }))
                    || (parent_selected && !standalone_text)
                    || (child.is_none() && !(current.objects.contains(&hit) && standalone_text))
                {
                    return None;
                }
                let mut keys = current.canvas_ids();
                let already_selected = keys.contains(&hit) || parent_selected;
                if !already_selected && !modifiers.shift {
                    keys.clear();
                }
                if !keys.contains(&hit) {
                    keys.push(hit);
                }
                let picked = if already_selected {
                    current.clone()
                } else {
                    selected(shell, document, &keys)
                };
                let schematic = crate::bars::document_type(shell, document)
                    == Some(crate::bars::DocumentType::Schematic);
                let objects: Vec<_> = picked
                    .objects
                    .iter()
                    .copied()
                    .filter(|id| {
                        rules::allowed(shell, document, *id)
                            && !(shell.preferences().editing.protect_locked_objects
                                && rules::locked(shell, document, *id))
                            && shell
                                .engine()
                                .project()
                                .and_then(|p| p.object(*id))
                                .is_some_and(|o| {
                                    o.scope() == reda_engine::model::Scope::Document(document)
                                        && (schematic
                                            || matches!(
                                                o.object_type(),
                                                reda_engine::model::ObjectType::FootprintInstance
                                                    | reda_engine::model::ObjectType::FreePad
                                                    | reda_engine::model::ObjectType::OutputOrigin
                                            ))
                                })
                    })
                    .collect();
                let text_ids = picked.canvas_ids();
                let mut targets = Vec::new();
                for item in &shell.document_tab(document)?.scene.scene.items {
                    let Some(child) = &item.child else {
                        continue;
                    };
                    if matches!(child, Child::Pad { .. })
                        || !text_ids.contains(&child.id())
                        || objects.contains(&child.owner())
                        || !rules::allowed(shell, document, child.id())
                        || targets.iter().any(|(seen, _)| seen == child)
                    {
                        continue;
                    }
                    if let reda_engine::render::Primitive::Glyphs(run) = &item.primitive {
                        targets.push((child.clone(), run.at));
                    }
                }
                let grab = shell.snap_to_grid(document, from);
                let by = shell.snap_to_grid(document, at) - grab;
                if picked != current {
                    commands.push((
                        "selection.set".into(),
                        serde_json::to_value(picked).expect("selection serializes"),
                    ));
                }
                moving = Some(MovingText {
                    document,
                    targets,
                    objects,
                    schematic,
                    keep_connections: !modifiers.command,
                    grab,
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
                m.by = shell.snap_to_grid(document, at) - m.grab
            }
            Pointer::DragEnd(at) => {
                m.by = shell.snap_to_grid(document, at) - m.grab;
                if m.by != Point::default() {
                    let mut steps = Vec::new();
                    if !m.objects.is_empty() {
                        let id = if m.schematic {
                            if m.keep_connections {
                                "schematic.drag"
                            } else {
                                "schematic.move"
                            }
                        } else {
                            "pcb.move"
                        };
                        steps.push(json!({"id":id,"params":{"objects":m.objects,"by":{"x":m.by.x.0,"y":m.by.y.0}}}));
                    }
                    for (child, start) in &m.targets {
                        let at = *start + m.by;
                        let (id, params) = match child {
                            Child::Field { unit, field } => (
                                "schematic.field.move",
                                json!({"unit":unit,"field":field,"at":{"x":at.x.0,"y":at.y.0}}),
                            ),
                            Child::FootprintText { footprint, text } => (
                                "pcb.footprint.text.set",
                                json!({"instances":[footprint],"texts":[text],"at":{"x":at.x.0,"y":at.y.0}}),
                            ),
                            Child::Pad { .. } => unreachable!(),
                        };
                        steps.push(json!({"id":id,"params":params}));
                    }
                    if steps.len() == 1 {
                        commands.push((
                            steps[0]["id"].as_str().expect("step id").into(),
                            steps[0]["params"].clone(),
                        ));
                    } else if !steps.is_empty() {
                        commands.push(("batch".into(), json!({"commands":steps})));
                    }
                }
                moving = None;
            }
            _ => {}
        }
    }
    if consumed && let Some(tab) = shell.document_tab_mut(document) {
        let ids: Vec<_> = moving
            .iter()
            .flat_map(|m| {
                m.objects
                    .iter()
                    .copied()
                    .chain(m.targets.iter().map(|(child, _)| child.id()))
            })
            .collect();
        tab.live = LiveHandle::new(None, &ids, &[]);
        tab.moving_offset = moving
            .as_ref()
            .map_or((0.0, 0.0), |m| (m.by.x.0 as f64, m.by.y.0 as f64));
    }
    shell.selection_picking_mut().moving = moving;
    consumed.then_some(commands)
}

pub fn movement_hit(shell: &Shell, document: Uuid, at: (f64, f64), tolerance: f64) -> Option<Uuid> {
    let id = hit(shell, document, at, tolerance)?;
    let child = rules::child(shell, document, id);
    Some(
        child
            .filter(|c| {
                shell.selection().objects.contains(&c.owner())
                    && rules::allowed(shell, document, c.owner())
                    && !(shell.preferences().editing.protect_locked_objects
                        && rules::locked(shell, document, c.owner()))
            })
            .map_or(id, |c| c.owner()),
    )
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
    let scene = document_scene(shell, document);
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
    let layers = layer_list(shell, document);
    let layer = |id: Uuid| scene.items.iter().find(|i| i.object == id).map(|i| i.layer);
    objects.sort_by_key(|id| {
        (
            match layer(*id) {
                Some(SceneLayer::Board(id)) => layers
                    .iter()
                    .position(|l| l.uuid == id)
                    .unwrap_or(usize::MAX - 1),
                _ => usize::MAX,
            },
            *id,
        )
    });
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
    let scene = document_scene(shell, document);
    let layers = layer_list(shell, document);
    egui::Window::new(crate::tr!("selection-pick-title"))
        .id(egui::Id::NULL.with(("pick-object", document)))
        .default_pos(list.at)
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ui.ctx(), |ui| {
            let mut previous = None;
            for (ordinal, id) in list.objects.iter().enumerate() {
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
                            .map(|l| {
                                crate::panels::layers::layer_name(
                                    shell
                                        .engine()
                                        .project()
                                        .and_then(|p| p.pcb_document(document))
                                        .expect("board"),
                                    l,
                                )
                            })
                            .or_else(|| {
                                layers
                                    .iter()
                                    .find(|l| l.uuid == layer)
                                    .map(editor_layer_name)
                            })
                            .unwrap_or_else(|| crate::tr!("selection-pick-layer")),
                        Some(SceneLayer::Drill) => crate::tr!("selection-pick-drill"),
                        _ => crate::tr!("selection-pick-sheet"),
                    };
                    ui.strong(name);
                    previous = layer;
                }
                let label = entry_label(shell, document, *id, ordinal);
                ui.horizontal(|ui| {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(28.0, 20.0), egui::Sense::hover());
                    thumbnail(ui, &scene, *id, rect);
                    let row = ui.selectable_label(false, label);
                    if row.hovered() {
                        preview = Some(*id);
                    }
                    if row.clicked() {
                        choose = Some(*id);
                    }
                    if rules::locked(shell, document, *id) {
                        let (lock, response) =
                            ui.allocate_exact_size(egui::vec2(16.0, 20.0), egui::Sense::hover());
                        let color = ui.visuals().text_color();
                        ui.painter().circle_stroke(
                            lock.center() - egui::vec2(0.0, 3.0),
                            4.0,
                            egui::Stroke::new(1.5, color),
                        );
                        ui.painter().rect_filled(
                            egui::Rect::from_center_size(
                                lock.center() + egui::vec2(0.0, 3.0),
                                egui::vec2(11.0, 9.0),
                            ),
                            2.0,
                            color,
                        );
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::Role::Image,
                                true,
                                crate::tr!("selection-pick-locked"),
                            )
                        });
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

fn layer_list(shell: &Shell, document: Uuid) -> Vec<reda_engine::model::Layer> {
    if let Some(pcb) = shell
        .engine()
        .project()
        .and_then(|p| p.pcb_document(document))
    {
        return pcb.layers().layers.clone();
    }
    if let Some(reda_engine::model::LibraryElement::Footprint(f)) = shell.engine().element(document)
    {
        return reda_engine::footprint_scene(f, shell.engine().fonts()).layers;
    }
    Vec::new()
}

fn editor_layer_name(layer: &reda_engine::model::Layer) -> String {
    use reda_engine::model::{LayerKind, Side};
    let kind = match layer.kind {
        LayerKind::Copper => crate::tr!("layer-copper"),
        LayerKind::Silkscreen => crate::tr!("layer-silkscreen"),
        LayerKind::SolderMask => crate::tr!("layer-solder-mask"),
        LayerKind::Paste => crate::tr!("layer-paste"),
        LayerKind::Courtyard => crate::tr!("layer-courtyard"),
        LayerKind::Assembly => crate::tr!("layer-assembly"),
        LayerKind::BoardOutline => crate::tr!("layer-board-outline"),
        LayerKind::Mechanical => crate::tr!("layer-mechanical"),
        LayerKind::Documentation => crate::tr!("layer-documentation"),
    };
    match layer.side {
        Some(Side::Top) => crate::tr!("layer-top", kind = kind.as_str()),
        Some(Side::Bottom) => crate::tr!("layer-bottom", kind = kind.as_str()),
        None => kind,
    }
}

fn entry_label(shell: &Shell, document: Uuid, id: Uuid, ordinal: usize) -> String {
    use reda_engine::model::Object;
    let kind = super::filter_ui::kind_label(
        &rules::kind(shell, document, id).unwrap_or_else(|| "graphic".into()),
    );
    if let Some(object) = shell.engine().elements().and_then(|p| p.object(id)) {
        let detail = match object {
            Object::Unit(u) => u.object.designator,
            Object::FootprintInstance(f) => f.object.designator,
            Object::Wire(w) => crate::tr!(
                "selection-pick-length",
                length = format!(
                    "{:.2}",
                    w.segments()
                        .map(|(a, b)| ((b.x - a.x).0 as f64).hypot((b.y - a.y).0 as f64))
                        .sum::<f64>()
                        / 1e6
                )
            ),
            Object::FreePad(p) => crate::tr!(
                "selection-pick-size",
                width = format!("{:.2}", p.width.0 as f64 / 1e6),
                height = format!("{:.2}", p.height.0 as f64 / 1e6)
            ),
            _ => format!("{}", ordinal + 1),
        };
        return crate::tr!(
            "selection-pick-entry",
            kind = kind.as_str(),
            detail = detail
        );
    }
    if let Some(item) = rules::child_item(shell, document, id) {
        if let reda_engine::render::Primitive::Glyphs(run) = &item.primitive {
            return crate::tr!(
                "selection-pick-entry",
                kind = kind.as_str(),
                detail = run.text.as_str()
            );
        }
        if let Some(Child::Pad { footprint, pad }) = &item.child
            && let Some(pcb) = shell
                .engine()
                .project()
                .and_then(|p| p.pcb_document(document))
            && let Some(f) = pcb.footprint(*footprint)
            && let Some(p) = pcb
                .footprint_copy(f.footprint)
                .and_then(|copy| copy.footprint.pads.iter().find(|p| p.uuid == *pad))
        {
            return crate::tr!(
                "selection-pick-pad",
                kind = kind.as_str(),
                number = p.number.as_str(),
                designator = f.designator.as_str()
            );
        }
    }
    crate::tr!(
        "selection-pick-entry",
        kind = kind.as_str(),
        detail = (ordinal + 1).to_string()
    )
}

fn thumbnail(ui: &egui::Ui, scene: &Scene, object: Uuid, rect: egui::Rect) {
    use reda_engine::render::Primitive;
    let items: Vec<_> = scene.items.iter().filter(|i| i.object == object).collect();
    let bounds = items
        .iter()
        .filter_map(|i| i.primitive.bounds())
        .reduce(|(a, b), (c, d)| {
            (
                reda_engine::model::geom::Point::new(a.x.min(c.x), a.y.min(c.y)),
                reda_engine::model::geom::Point::new(b.x.max(d.x), b.y.max(d.y)),
            )
        });
    let Some((lo, hi)) = bounds else {
        return;
    };
    let scale = ((rect.width() - 4.0) as f64 / ((hi.x - lo.x).0.max(1) as f64))
        .min((rect.height() - 4.0) as f64 / ((hi.y - lo.y).0.max(1) as f64));
    let center = (
        ((lo.x.0 + hi.x.0) as f64 / 2.0),
        ((lo.y.0 + hi.y.0) as f64 / 2.0),
    );
    let at = |p: reda_engine::model::geom::Point| {
        rect.center()
            + egui::vec2(
                ((p.x.0 as f64 - center.0) * scale) as f32,
                ((center.1 - p.y.0 as f64) * scale) as f32,
            )
    };
    let color = ui.visuals().selection.stroke.color;
    let stroke = egui::Stroke::new(1.3, color);
    for item in items {
        match &item.primitive {
            Primitive::Capsule { a, b, .. } => {
                ui.painter().line_segment([at(*a), at(*b)], stroke);
            }
            Primitive::Ring { center, outer, .. } => {
                ui.painter()
                    .circle_stroke(at(*center), (outer.0 as f64 * scale) as f32, stroke);
            }
            Primitive::Glyphs(_) => {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "T",
                    egui::FontId::proportional(14.0),
                    color,
                );
            }
            other => {
                if let Some((a, b)) = other.bounds() {
                    ui.painter().rect_stroke(
                        egui::Rect::from_two_pos(at(a), at(b)),
                        1.0,
                        stroke,
                        egui::StrokeKind::Inside,
                    );
                }
            }
        }
    }
}
