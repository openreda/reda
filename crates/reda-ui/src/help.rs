//! What the Help menu shows and does: the About box with the third-party notices, the
//! on-demand update check and, on macOS, putting the `reda` CLI on the PATH.

/// The third-party notices, built into the binary by the build script from
/// `packaging/notices/vendored.txt` and, in a release, cargo-about's crate licences.
pub const NOTICES: &str = include_str!(concat!(env!("OUT_DIR"), "/THIRD_PARTY_NOTICES"));
