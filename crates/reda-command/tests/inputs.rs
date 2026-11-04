use std::collections::BTreeSet;

use reda_command::{Clock, FixedClock, IdSource, RandomIds, SeededIds, SystemClock, Timestamp};

#[test]
fn seeded_ids_count_up_as_readable_v4_uuids() {
    let mut ids = SeededIds::new(0);
    let got: Vec<String> = (0..2).map(|_| ids.next_id().to_string()).collect();
    assert_eq!(
        got,
        [
            "00000000-0000-4000-8000-000000000001",
            "00000000-0000-4000-8000-000000000002"
        ]
    );
}

#[test]
fn the_seed_separates_streams() {
    let mut ids = SeededIds::new(7);
    assert_eq!(
        ids.next_id().to_string(),
        "00000000-0007-4000-8000-000000000001"
    );
}

#[test]
fn random_ids_are_distinct_v4_uuids() {
    let mut ids = RandomIds::new();
    let got: BTreeSet<String> = (0..1000).map(|_| ids.next_id().to_string()).collect();
    assert_eq!(got.len(), 1000);
    for id in &got {
        assert_eq!(&id[14..15], "4", "{id}");
        assert!("89ab".contains(&id[19..20]), "{id}");
    }
}

#[test]
fn fixed_clock_is_the_test_epoch() {
    // 2026-01-01T00:00:00Z in Unix milliseconds.
    assert_eq!(FixedClock::EPOCH.now(), Timestamp(1_767_225_600_000));
}

#[test]
fn system_clock_is_after_the_test_epoch() {
    assert!(SystemClock.now() > FixedClock::EPOCH.now());
}

#[test]
fn seeded_random_ids_repeat_for_a_seed_and_differ_between_seeds() {
    // What a wasm host, which has no clock or process id in std, seeds itself.
    let take = |seed| {
        let mut ids = RandomIds::from_seed(seed);
        (ids.next_id(), ids.next_id())
    };
    assert_eq!(take(42), take(42));
    assert_ne!(take(42), take(43));
    assert_ne!(take(42).0, take(42).1);
}
