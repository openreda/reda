//! PCB behaviour above the model: building a new board, text geometry, and the PCB
//! Commands for its outline, cutouts, rules, DRC severities, Net Classes, and free text and
//! graphics.
//!
//! Copper connectivity and the Ratsnest are in [`copper`], the per-Net rules in
//! [`net_rules`], Zone fill in [`zone`]. DRC joins here in a later ticket.

mod board;
mod commands;
pub mod copper;
pub mod net_rules;
mod text;
pub mod zone;

pub use board::{DEFAULT_NET_CLASS, V01_LAYERS, new_board};
pub use commands::{rect_outline, register_all};
pub use copper::{ClearanceHit, Copper, Item, Proposal, RatsnestLine, RatsnestVisibility};
pub use text::{
    Fill, LINE_PITCH, PlacedText, Stroke, font_is_missing, missing_fonts, text_geometry,
};
