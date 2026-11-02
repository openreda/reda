//! The editing Commands the schematic Tools commit, one per user gesture: delete, the
//! clipboard, duplicate, move and drag, rotate and mirror, align, distribute, snap to grid,
//! and free text and graphics.

mod arrange;
mod bounds;
mod clipboard;
mod delete;
mod graphics;
mod moving;
mod selection;
mod transform;
mod turn;

pub use arrange::{
    Align, AlignParams, AlignTo, Direction, Distribute, DistributeParams, SnapParams, SnapToGrid,
};
pub use clipboard::{
    Clipboard, Copy, CopyParams, Cut, Duplicate, DuplicateParams, Paste, PasteParams, PasteResult,
};
pub use delete::{Delete, DeleteParams, DeleteResult};
pub use graphics::{
    GraphicResult, GraphicSet, GraphicSetParams, LineAdd, LineAddParams, RectAdd, RectAddParams,
    TextAdd, TextAddParams, TextResult, TextSet, TextSetParams,
};
pub use moving::{Drag, EditResult, Move, ShiftParams, ShiftPreview, shift_preview};
pub use selection::selectable;
pub use transform::Axis;
pub use turn::{Mirror, MirrorParams, Rotate, RotateParams};
