//! The text size a fit test draws at. Every fit test runs its screens at the default
//! size in the four variants, and again at the two ends of the range
//! (`app.md#preferences`, 11 and 16 px), where text that only just fitted at 13 px
//! wraps or is cut first.
//!
//! The size is set for the test's thread by [`at`] and read where the test builds its
//! `theme::Settings` or writes the Preferences, so the screens' own code needs no
//! parameter for it. Each `#[test]` runs on a thread of its own.

use std::cell::Cell;

use reda_ui::engine::preferences::{DEFAULT_TEXT_SIZE, TEXT_SIZES};

thread_local! {
    static SIZE: Cell<u8> = const { Cell::new(DEFAULT_TEXT_SIZE) };
}

/// The ends of the text-size range.
pub const SMALLEST: u8 = *TEXT_SIZES.start();
pub const LARGEST: u8 = *TEXT_SIZES.end();

/// The text size this thread's screens are drawn at.
pub fn get() -> u8 {
    SIZE.get()
}

/// Runs `check` with its screens drawn at `size` px.
pub fn at<R>(size: u8, check: impl FnOnce() -> R) -> R {
    SIZE.set(size);
    let out = check();
    SIZE.set(DEFAULT_TEXT_SIZE);
    out
}
