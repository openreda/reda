//! The built-in, fixed pin-type matrix: which pairs of Pin types on one Net conflict.

use reda_model::{ErcCheck, PinType};

/// The check a pair of Pin types on one Net breaks, if any: the fixed matrix in
/// [`reda_model::pin_conflict`].
pub(super) fn conflict(a: PinType, b: PinType) -> Option<ErcCheck> {
    reda_model::pin_conflict(a, b)
}

/// A Pin type's name as files and Commands spell it.
pub(super) fn type_name(t: PinType) -> &'static str {
    match t {
        PinType::Input => "input",
        PinType::Output => "output",
        PinType::Bidirectional => "bidirectional",
        PinType::TriState => "tri_state",
        PinType::Passive => "passive",
        PinType::PowerIn => "power_in",
        PinType::PowerOut => "power_out",
        PinType::OpenCollector => "open_collector",
        PinType::OpenEmitter => "open_emitter",
        PinType::NotConnected => "not_connected",
    }
}
