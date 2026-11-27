//! Arrow steps (docs/spec/v0.3/input-and-defaults.md#one-step, testing.md#property-tests):
//! from any point, N presses land on grid lines, and N presses one way then N back return
//! to the first landed point.

use proptest::prelude::*;
use reda_ui::edit_cursor::{GridAxis, step_axis};

/// Whether `at` is on a line of `axis`.
fn on_line(at: i64, axis: GridAxis) -> bool {
    (at - axis.origin).rem_euclid(axis.pitch) == 0
}

#[test]
fn a_press_from_off_the_grid_lands_on_the_next_line_in_that_direction() {
    // A 1 mm grid through 0.2 mm: lines at ..., -0.8, 0.2, 1.2, 2.2 mm.
    let axis = GridAxis {
        origin: 200_000,
        pitch: 1_000_000,
    };
    assert_eq!(step_axis(500_000, axis, 1), 1_200_000);
    assert_eq!(step_axis(500_000, axis, -1), 200_000);
    // Ten steps: the next line counts as the first.
    assert_eq!(step_axis(500_000, axis, 10), 10_200_000);
    assert_eq!(step_axis(500_000, axis, -10), -8_800_000);
    // From a line, a whole step.
    assert_eq!(step_axis(1_200_000, axis, 1), 2_200_000);
    assert_eq!(step_axis(1_200_000, axis, -1), 200_000);
    // Below the origin too.
    assert_eq!(step_axis(-1_300_000, axis, 1), -800_000);
    assert_eq!(step_axis(-1_300_000, axis, -1), -1_800_000);
    // No step leaves the point where it is, on the grid or off it.
    assert_eq!(step_axis(500_000, axis, 0), 500_000);
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    #[test]
    fn n_presses_land_on_grid_lines_and_n_back_return_to_the_first_landed_point(
        start in -1_000_000_000i64..1_000_000_000,
        origin in -10_000_000i64..10_000_000,
        pitch in 1i64..5_000_000,
        presses in 1usize..40,
        forward in any::<bool>(),
        shift in any::<bool>(),
    ) {
        let axis = GridAxis { origin, pitch };
        let steps = if shift { 10 } else { 1 };
        let (there, back) = if forward { (steps, -steps) } else { (-steps, steps) };
        // The first press lands on a line; N more one way and N back return to it.
        let first = step_axis(start, axis, there);
        prop_assert!(on_line(first, axis), "{start} -> {first} is off the grid");
        prop_assert!((first - start).signum() == there.signum(), "{start} -> {first}");
        let mut at = first;
        for _ in 0..presses {
            let next = step_axis(at, axis, there);
            prop_assert!(on_line(next, axis), "{at} -> {next} is off the grid");
            prop_assert_eq!(next - at, there * pitch);
            at = next;
        }
        for _ in 0..presses {
            at = step_axis(at, axis, back);
            prop_assert!(on_line(at, axis));
        }
        prop_assert_eq!(at, first);
    }
}
