//! The editing Commands the schematic Tools commit, one per user gesture: delete, the
//! clipboard, duplicate, move and drag, rotate and mirror, align, distribute, snap to grid,
//! breaking Wires and converting cross-junctions, and free text and graphics.

mod arrange;
pub(crate) mod bounds;
mod clipboard;
mod delete;
mod graphics;
mod move_to_sheet;
mod moving;
mod selection;
mod transform;
mod turn;
mod wires;

pub use arrange::{
    Align, AlignParams, AlignTo, Direction, Distribute, DistributeParams, SnapParams, SnapToGrid,
};
pub use clipboard::{
    Clipboard, CopiedText, Copy, CopyAsText, CopyAsTextParams, CopyParams, Cut, CutParams, Duplicate, DuplicateParams, Paste, PasteParams,
    PasteResult,
};
pub use delete::{Delete, DeleteParams, DeleteResult, WireSegment};
pub use graphics::{
    GraphicResult, GraphicSet, GraphicSetParams, LineAdd, LineAddParams, RectAdd, RectAddParams,
    TextAdd, TextAddParams, TextResult, TextSet, TextSetParams,
};
pub use move_to_sheet::{MoveToSheet, MoveToSheetParams, MoveToSheetResult, NetChange};
pub use moving::{
    Drag, EditResult, FlightStep, Move, ShiftParams, ShiftPreview, flight_preview, shift_preview,
};
pub use selection::selectable;
pub use transform::Axis;
pub use turn::{Mirror, MirrorParams, Rotate, RotateParams};
pub use wires::{
    ConvertCross, ConvertCrossParams, ConvertCrossResult, ConvertedJunction, Gap, WireBreak,
    WireBreakParams, WireBreakResult, WireSetVertices, WireSetVerticesParams,
    WireSetVerticesResult, break_gap,
};
