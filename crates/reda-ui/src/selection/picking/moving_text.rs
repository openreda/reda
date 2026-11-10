//! Dragging Field and Designator text on its own, or along with selected parents.

use reda_engine::model::Uuid;
use reda_engine::render::Child;
use serde_json::{Value, json};

use super::{hit, rules, selected, set_command};
use crate::shell::Shell;

#[derive(Clone, Debug)]
pub struct MovingText {
    pub(super) document: Uuid,
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
                        && rules::pickable(shell, document, c.owner())
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
                        rules::pickable(shell, document, *id)
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
                    commands.push(set_command(&picked));
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
