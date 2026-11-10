//! Editing a placed Footprint from the Properties panel
//! (docs/spec/v0.1/pcb.md#placed-pad-edits, pcb.md#placed-footprint-graphics): the rows of
//! one selected Pad (`pcb.pad.edit`, every property but its number), and the graphics of
//! one selected Footprint with its lock (`pcb.footprint.unlock_graphics`,
//! `pcb.footprint.graphic.edit`, `pcb.footprint.graphic.delete`). Either edit makes a
//! Locally Edited Footprint, which the panel marks ([`super::LocalEdits`]).

use reda_engine::model::geom::{Millideg, Nm};
use reda_engine::model::{FootprintGraphic, FootprintLayer, Pad, PadKind, PadShape, Project, Uuid};
use serde_json::{Value, json};

use crate::selection::Selection;

/// The one Pad selected, as Properties edits it.
#[derive(Clone, Debug, PartialEq)]
pub struct PadRows {
    pub instance: Uuid,
    /// The Pad as its Footprint copy holds it.
    pub pad: Pad,
}

/// One edit of the Pad rows.
#[derive(Clone, Debug, PartialEq)]
pub enum PadEdit {
    Kind(PadKind),
    Shape(PadShape),
    Width(Nm),
    Height(Nm),
    Rotation(Millideg),
    /// A round hole this wide.
    Hole(Nm),
}

impl PadRows {
    /// The rows when exactly one Pad of a placed Footprint is selected.
    pub fn of(project: &Project, selection: &Selection) -> Option<PadRows> {
        let [pad] = selection.pads[..] else {
            return None;
        };
        if !selection.objects.is_empty() {
            return None;
        }
        let pcb = project.pcb()?;
        let instance = pcb.footprint(pad.footprint)?;
        let copy = pcb.footprint_copy(instance.footprint)?;
        let found = copy.footprint.pads.iter().find(|p| p.uuid == pad.pad)?;
        Some(PadRows {
            instance: instance.uuid,
            pad: found.clone(),
        })
    }

    /// The `pcb.pad.edit` that makes `edit`.
    pub fn command(&self, edit: &PadEdit) -> (&'static str, Value) {
        let mut p = json!({"instance": self.instance, "pad": self.pad.uuid});
        match edit {
            PadEdit::Kind(k) => p["kind"] = json!(k),
            PadEdit::Shape(s) => p["shape"] = json!(s),
            PadEdit::Width(w) => p["width"] = json!(w.0),
            PadEdit::Height(h) => p["height"] = json!(h.0),
            PadEdit::Rotation(r) => p["rotation"] = json!(r.0),
            PadEdit::Hole(w) => p["hole"] = json!({"width": w.0}),
        }
        ("pcb.pad.edit", p)
    }
}

/// The graphics of the one Footprint selected, and its lock.
#[derive(Clone, Debug, PartialEq)]
pub struct FootprintGraphics {
    pub instance: Uuid,
    /// `graphics_unlocked`: the graphics can be edited on the board.
    pub unlocked: bool,
    /// Edits go through even while locked: Protect Locked Primitives In Component is off.
    pub unprotected: bool,
    pub graphics: Vec<FootprintGraphic>,
}

/// One edit of the Footprint graphics rows.
#[derive(Clone, Debug, PartialEq)]
pub enum GraphicEdit {
    /// The lock icon: unlocks (true) or locks the graphics again.
    Unlock(bool),
    Layer(Uuid, FootprintLayer),
    Width(Uuid, Nm),
    Delete(Uuid),
}

impl FootprintGraphics {
    /// The rows when exactly one placed Footprint is selected; `protect` is the Protect
    /// Locked Primitives In Component Preference.
    pub fn of(
        project: &Project,
        selection: &Selection,
        protect: bool,
    ) -> Option<FootprintGraphics> {
        let [object] = selection.objects[..] else {
            return None;
        };
        let pcb = project.pcb()?;
        let instance = pcb.footprint(object)?;
        let copy = pcb.footprint_copy(instance.footprint)?;
        Some(FootprintGraphics {
            instance: instance.uuid,
            unlocked: instance.graphics_unlocked,
            unprotected: !protect,
            graphics: copy.footprint.graphics.clone(),
        })
    }

    /// Whether the graphic rows can be edited.
    pub fn editable(&self) -> bool {
        self.unlocked || self.unprotected
    }

    /// The Command that makes `edit`; `None` for a graphic edit while they are locked.
    pub fn command(&self, edit: &GraphicEdit) -> Option<(&'static str, Value)> {
        if let GraphicEdit::Unlock(unlocked) = edit {
            return Some((
                "pcb.footprint.unlock_graphics",
                json!({"instances": [self.instance], "unlocked": unlocked}),
            ));
        }
        if !self.editable() {
            return None;
        }
        let mut p = json!({"instance": self.instance});
        if !self.unlocked {
            p["include_locked"] = json!(true);
        }
        let id = match edit {
            GraphicEdit::Layer(g, layer) => {
                p["graphic"] = json!(g);
                p["layer"] = json!(layer);
                "pcb.footprint.graphic.edit"
            }
            GraphicEdit::Width(g, width) => {
                p["graphic"] = json!(g);
                p["width"] = json!(width.0);
                "pcb.footprint.graphic.edit"
            }
            GraphicEdit::Delete(g) => {
                p["graphics"] = json!([g]);
                "pcb.footprint.graphic.delete"
            }
            GraphicEdit::Unlock(_) => unreachable!("handled above"),
        };
        Some((id, p))
    }
}
