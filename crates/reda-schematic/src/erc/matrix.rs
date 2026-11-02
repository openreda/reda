//! The built-in, fixed pin-type matrix: which pairs of Pin types on one Net conflict.

use reda_model::{ErcCheck, PinType};

/// The check a pair of Pin types on one Net breaks, if any. The matrix is symmetric:
/// - `pin_conflict` (drivers fighting): Output with Output or Power Out, Power Out with
///   Power Out.
/// - `pin_conflict_mixed` (types that should not mix): Tri-state, Open Collector or Open
///   Emitter with Output or Power Out; Bidirectional with Power Out; Open Collector with
///   Open Emitter or Tri-state; Open Emitter with Tri-state.
///
/// Every other pair, and anything with a Passive, Input, Power In or Not Connected Pin,
/// is fine.
pub(super) fn conflict(a: PinType, b: PinType) -> Option<ErcCheck> {
    use PinType::*;
    let driver = |t: PinType| matches!(t, Output | PowerOut);
    if driver(a) && driver(b) {
        return Some(ErcCheck::PinConflict);
    }
    let mixed = |x: PinType, y: PinType| match x {
        TriState | OpenCollector | OpenEmitter if driver(y) => true,
        Bidirectional => y == PowerOut,
        OpenCollector => matches!(y, OpenEmitter | TriState),
        OpenEmitter => y == TriState,
        _ => false,
    };
    (mixed(a, b) || mixed(b, a)).then_some(ErcCheck::PinConflictMixed)
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

#[cfg(test)]
mod tests {
    use super::*;
    use PinType::*;

    const ALL: [PinType; 10] = [
        Input,
        Output,
        Bidirectional,
        TriState,
        Passive,
        PowerIn,
        PowerOut,
        OpenCollector,
        OpenEmitter,
        NotConnected,
    ];

    #[test]
    fn the_matrix_is_symmetric() {
        for a in ALL {
            for b in ALL {
                assert_eq!(conflict(a, b), conflict(b, a), "{a:?} / {b:?}");
            }
        }
    }

    #[test]
    fn the_matrix_has_its_documented_cells() {
        let errors = [(Output, Output), (Output, PowerOut), (PowerOut, PowerOut)];
        let warnings = [
            (TriState, Output),
            (OpenCollector, Output),
            (OpenEmitter, Output),
            (TriState, PowerOut),
            (OpenCollector, PowerOut),
            (OpenEmitter, PowerOut),
            (Bidirectional, PowerOut),
            (OpenCollector, OpenEmitter),
            (OpenCollector, TriState),
            (OpenEmitter, TriState),
        ];
        for a in ALL {
            for b in ALL {
                let expected = if errors.contains(&(a, b)) || errors.contains(&(b, a)) {
                    Some(ErcCheck::PinConflict)
                } else if warnings.contains(&(a, b)) || warnings.contains(&(b, a)) {
                    Some(ErcCheck::PinConflictMixed)
                } else {
                    None
                };
                assert_eq!(conflict(a, b), expected, "{a:?} / {b:?}");
            }
        }
    }
}
