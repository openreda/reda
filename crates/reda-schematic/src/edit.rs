//! The editing Commands the schematic Tools commit, one per user gesture: delete, the
//! clipboard, duplicate, move and drag, rotate and mirror, align, distribute, snap to grid,
//! and free text and graphics.

mod arrange;
mod bounds;
mod delete;
mod graphics;
mod moving;
mod selection;
mod transform;
mod turn;

pub use arrange::{
    Align, AlignParams, AlignTo, Direction, Distribute, DistributeParams, SnapParams, SnapToGrid,
};
pub use delete::{Delete, DeleteParams, DeleteResult};
pub use graphics::{
    GraphicResult, GraphicSet, GraphicSetParams, LineAdd, LineAddParams, RectAdd, RectAddParams,
    TextAdd, TextAddParams, TextResult, TextSet, TextSetParams,
};
pub use moving::{Drag, EditResult, Move, ShiftParams};
pub use transform::Axis;
pub use turn::{Mirror, MirrorParams, Rotate, RotateParams};
