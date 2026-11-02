//! PCB behaviour above the model: building a new board, text geometry, placed Footprint
//! geometry ([`placed`]), and the PCB Commands for its outline, cutouts, rules, DRC
//! severities, Net Classes, free text and graphics, and placement: moving, turning,
//! flipping, locking and arranging Footprints, their Designator text, Mechanical Parts,
//! Free Pads and the Placement Origin.
//!
//! Copper connectivity, the Ratsnest, DRC and Zone fill join here in later tickets.

mod board;
mod commands;
pub mod placed;
mod text;

pub use board::{DEFAULT_NET_CLASS, V01_LAYERS, new_board};
pub use commands::{rect_outline, register_all};
pub use text::{
    Fill, LINE_PITCH, PlacedText, Stroke, font_is_missing, missing_fonts, text_geometry,
};
