//! PCB behaviour above the model: building a new board, text geometry, and the PCB
//! Commands for its outline, cutouts, rules, DRC severities, Net Classes, and free text and
//! graphics.
//!
//! Copper connectivity, the Ratsnest, DRC and Zone fill join here in later tickets.

mod board;
mod commands;
mod text;

pub use board::{DEFAULT_NET_CLASS, GENERIC_RULES, V01_LAYERS, new_board};
pub use commands::register_all;
pub use text::{
    Fill, LINE_PITCH, PlacedText, Stroke, font_is_missing, missing_fonts, text_geometry,
};
