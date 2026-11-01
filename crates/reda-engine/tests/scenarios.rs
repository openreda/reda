//! Runs every data-driven scenario in `tests/scenarios/` and compares the resulting
//! Documents with the goldens beside each script. `REDA_BLESS=1` rewrites them.

use std::path::Path;

#[test]
fn every_scenario_replays_to_its_goldens() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/scenarios");
    reda_testkit::run_scenarios(dir);
}
