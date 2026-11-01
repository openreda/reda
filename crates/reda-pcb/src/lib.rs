//! PCB behaviour above the model: building a new board, and the PCB Commands for its
//! outline, cutouts, rules, DRC severities and Net Classes.
//!
//! Copper connectivity, the Ratsnest, DRC and Zone fill join here in later tickets.

mod board;
mod commands;

pub use board::{DEFAULT_NET_CLASS, GENERIC_RULES, V01_LAYERS, new_board};
pub use commands::register_all;
