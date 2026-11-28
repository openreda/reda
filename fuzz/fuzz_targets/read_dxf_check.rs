// The body of the `read_dxf` fuzz target, kept apart so a stable test
// (`crates/reda-import-dxf/tests/fuzz_seeds.rs`) runs the same check over the seeds.
//
// The DXF reader must refuse bad input with an error, never a panic, whether the input is
// ASCII or binary DXF, and whatever its INSERTs, splines and text codes hold. What reads
// must read the same again, and break into pieces without a panic too.

/// The most entities whose pieces are made, so one input stays quick.
const PIECED: usize = 2_000;

/// Runs one fuzz input; returns the drawing if it read.
pub fn check(data: &[u8]) -> Option<reda_import_dxf::Drawing> {
    let drawing = reda_import_dxf::read(data).ok()?;
    // Compared as text, so a not-a-number that overflowing arithmetic made equals itself.
    let again = reda_import_dxf::read(data).expect("it read the first time");
    assert_eq!(
        format!("{again:?}"),
        format!("{drawing:?}"),
        "reading the same bytes twice gave two drawings"
    );
    for entity in drawing.entities.iter().take(PIECED) {
        for piece in entity.shape.pieces(0.005) {
            if let reda_import_dxf::Piece::Arc { sweep, .. } = piece {
                assert!(
                    sweep.is_nan() || sweep.abs() <= 360.0,
                    "an arc piece sweeps more than a full turn: {sweep}"
                );
            }
        }
    }
    Some(drawing)
}
