//! The Panels' contents. Each draws from the [`Shell`](crate::shell::Shell) and returns
//! what was clicked: the Commands to run, or actions for the window to apply after the
//! frame.

pub mod layers;
pub mod library;
pub mod messages;
pub mod properties;
