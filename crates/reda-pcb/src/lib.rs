//! PCB behaviour above the model: building a new board, text geometry, placed Footprint
//! geometry ([`placed`]), and the PCB Commands for its outline, cutouts, rules, DRC
//! severities, Net Classes, free text and graphics, and placement: moving, turning,
//! flipping, locking and arranging Footprints, their Designator text, Mechanical Parts,
//! Free Pads and the Output Origin.
//!
//! Dimensions (their Hotspots, values and layout) are in [`dimension`].
//! Copper connectivity and the Ratsnest are in [`copper`], the rule resolver in
//! [`rules`], Zone fill in [`zone`], the design rules check in [`drc`], and the packing
//! of new Footprints into clusters outside the board in [`cluster`].

mod board;
pub mod cluster;
mod commands;
pub mod copper;
pub mod dimension;
pub mod drc;
pub mod keepout;
pub mod placed;
pub mod rules;
mod text;
pub mod zone;

pub use board::{
    DEAD_COPPER_MIN_AREA, DEFAULT_NET_CLASS, PAD_CONNECTION_SEED, V01_LAYERS, VIA_CONNECTION_SEED,
    board_fab_rules, default_rules, new_board, seed_rules,
};
pub use commands::placement::arrange::natural_cmp;
pub use commands::{
    AreaParam, DrcRunParams, DrcRunResult, MechanicalSet, MechanicalSetParams, Placed, PlacementOp,
    SyncCheck, SyncState, TrackDragPlan, drc_run, glob_match, placement_anchor, placement_preview,
    rect_outline, register_all, register_all_with_user, track_drag_plan,
};
pub use copper::{ClearanceHit, Copper, Item, Proposal, RatsnestLine, RatsnestVisibility};
pub use text::{
    FOOTPRINT_TEXT_PEN, Fill, LINE_PITCH, PlacedText, Stroke, board_layer, element_texts,
    font_is_missing, footprint_texts, missing_fonts, text_geometry,
};

pub mod openings;
