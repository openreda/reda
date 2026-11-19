//! The Select Tool on Symbol and Footprint canvases.

use reda_engine::model::geom::Point;
use reda_engine::model::{Footprint, Symbol, Uuid};
use serde_json::json;

use super::{Invocation, parts, read_only};
use crate::canvas::interact::Pointer;
use crate::canvas::{Overlay, RubberBand};
use crate::selection::{picking, rules};
use crate::shell::Shell;
use crate::snap::{self, hotspots};

type P = (f64, f64);

/// The pick radius in screen points.
pub(crate) const PICK_POINTS: f64 = 5.0;

/// A Select Tool drag in progress on a Library canvas.
#[derive(Clone, Debug, PartialEq)]
pub enum Drag {
    /// A rectangle with the modifiers held when the drag began.
    Boxing {
        from: P,
        to: P,
        modifiers: egui::Modifiers,
    },
    /// The picked objects and their snapped offset from the initial anchor.
    Moving {
        objects: Vec<Uuid>,
        anchor: Point,
        offset: Point,
    },
}

impl Drag {
    /// The moving objects, their offset in nm, and the overlay for the current gesture.
    pub(crate) fn preview(&self, shell: &Shell, element: Uuid) -> (Vec<Uuid>, P, Overlay) {
        match self {
            Self::Boxing { from, to, .. } => (
                Vec::new(),
                (0.0, 0.0),
                Overlay {
                    rubber_band: Some(RubberBand {
                        from: *from,
                        to: *to,
                        mode: rules::box_mode(shell, element, *from, *to),
                    }),
                    ..Overlay::default()
                },
            ),
            Self::Moving {
                objects,
                anchor,
                offset,
            } => (
                objects.clone(),
                (offset.x.0 as f64, offset.y.0 as f64),
                Overlay {
                    cursor: Some(*anchor + *offset),
                    ..Overlay::default()
                },
            ),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Editor<'a> {
    Symbol(&'a Symbol),
    Footprint(&'a Footprint),
}

impl Editor<'_> {
    fn element(self) -> Uuid {
        match self {
            Self::Symbol(s) => s.uuid,
            Self::Footprint(f) => f.uuid,
        }
    }

    /// Snap onto this editor's Hotspots, excluding the objects it is carrying.
    pub(crate) fn snap(self, shell: &Shell, at: P, objects: &[Uuid]) -> Point {
        let (targets, grid) = match self {
            Self::Symbol(s) => (
                hotspots::symbol(s),
                crate::symbol_editor::snap_grid(shell, s.uuid, shell.tool()),
            ),
            Self::Footprint(f) => (hotspots::footprint(f), shell.grid(f.uuid)),
        };
        let rules = snap::rules_on(shell, self.element(), grid);
        snap::snap(at, &hotspots::without(targets, objects), &[], &rules).at
    }

    fn move_by(self, objects: &[Uuid], offset: Point) -> Invocation {
        match self {
            Self::Symbol(s) => (
                "library.symbol.move".into(),
                json!({"symbol": s.uuid, "objects": objects, "by": offset}),
            ),
            Self::Footprint(f) => (
                "library.footprint.move".into(),
                json!({"footprint": f.uuid, "objects": objects, "by": offset}),
            ),
        }
    }

    /// Advance one pointer event, returning the drag to carry into the next frame.
    pub(crate) fn select(
        self,
        shell: &mut Shell,
        drag: Option<Drag>,
        event: Pointer,
        tolerance: f64,
        hovered: &mut Option<Uuid>,
        commands: &mut Vec<Invocation>,
    ) -> Option<Drag> {
        let element = self.element();
        match (drag, event) {
            (
                Some(Drag::Moving {
                    objects, anchor, ..
                }),
                Pointer::Drag(at),
            ) => Some(Drag::Moving {
                offset: self.snap(shell, at, &objects) - anchor,
                objects,
                anchor,
            }),
            (
                Some(Drag::Moving {
                    objects, anchor, ..
                }),
                Pointer::DragEnd(at),
            ) => {
                let offset = self.snap(shell, at, &objects) - anchor;
                if offset != Point::default() && !read_only(shell.engine(), element) {
                    commands.push(self.move_by(&objects, offset));
                }
                None
            }
            (
                Some(Drag::Boxing {
                    from, modifiers, ..
                }),
                Pointer::Drag(to),
            ) => Some(Drag::Boxing {
                from,
                to,
                modifiers,
            }),
            (
                Some(Drag::Boxing {
                    from, modifiers, ..
                }),
                Pointer::DragEnd(to),
            ) => {
                commands.extend(picking::box_selection(shell, element, from, to, modifiers));
                None
            }
            (None, Pointer::Hover(at)) => {
                picking::hover(shell, element, at, tolerance);
                *hovered = picking::hit(shell, element, at, tolerance);
                None
            }
            (None, Pointer::Click { at, modifiers }) => {
                commands.extend(picking::click(shell, element, at, tolerance, modifiers));
                None
            }
            (
                None,
                Pointer::DragStart {
                    from,
                    at,
                    modifiers,
                },
            ) => {
                *hovered = None;
                let Some(hit) = picking::movement_hit(shell, element, from, tolerance) else {
                    return Some(Drag::Boxing {
                        from,
                        to: at,
                        modifiers,
                    });
                };
                let parts = parts(shell.engine(), element);
                let drag =
                    picking::drag_start(shell, element, hit, modifiers, |id| parts.contains(&id));
                commands.extend(drag.select);
                // A read-only Footprint previews the drag until release. A read-only
                // Symbol only picks the object.
                if matches!(self, Self::Symbol(_)) && read_only(shell.engine(), element) {
                    return None;
                }
                let objects = drag.objects;
                let anchor = self.snap(shell, from, &[]);
                Some(Drag::Moving {
                    offset: self.snap(shell, at, &objects) - anchor,
                    objects,
                    anchor,
                })
            }
            (drag, _) => drag,
        }
    }
}
