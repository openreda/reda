//! The Panels' contents. Each draws from the [`Shell`](crate::shell::Shell) and returns
//! what was clicked: the Commands to run, or actions for the window to apply after the
//! frame.

pub mod find;
pub mod layers;
pub mod library;
pub mod messages;
pub mod navigator;
pub mod pins;
pub mod properties;
pub mod violation_details;
