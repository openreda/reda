use reda_release::{Bump, Plan, PlanInput, Version, plan};

fn v(text: &str) -> Version {
    text.parse().unwrap()
}

fn input<'a>(tags: &'a [&'a str], labels: &'a [&'a str], message: &'a str) -> PlanInput<'a> {
    PlanInput {
        tags,
        labels,
        message,
        forced: None,
    }
}

#[test]
fn first_release_without_any_tag_is_0_0_1() {
    assert_eq!(
        plan(&input(&[], &[], "Add the canvas (#12)")),
        Plan::Release {
            version: v("0.0.1"),
            previous: None,
        }
    );
}

fn version_after(tags: &[&str], labels: &[&str]) -> Version {
    match plan(&input(tags, labels, "Finish the milestone (#77)")) {
        Plan::Release { version, .. } => version,
        skip => panic!("expected a release, got {skip:?}"),
    }
}

#[test]
fn release_minor_label_finishes_a_milestone() {
    // Finishing v0.1 releases 0.1.0.
    assert_eq!(
        version_after(&["v0.0.37"], &["enhancement", "release:minor"]),
        v("0.1.0")
    );
}

#[test]
fn release_major_label_means_1_0() {
    assert_eq!(version_after(&["v0.4.2"], &["release:major"]), v("1.0.0"));
}

#[test]
fn the_largest_bump_label_wins() {
    assert_eq!(
        version_after(&["v0.4.2"], &["release:minor", "release:major"]),
        v("1.0.0")
    );
}

#[test]
fn release_skip_label_skips() {
    let got = plan(&input(&["v0.0.3"], &["release:skip"], "Fix a typo (#9)"));
    assert!(matches!(got, Plan::Skip { .. }), "{got:?}");
}

#[test]
fn skip_release_marker_in_the_commit_message_skips() {
    let message = "Tidy the README (#10)\n\nDocs only. [skip release]";
    let got = plan(&input(&["v0.0.3"], &[], message));
    assert!(matches!(got, Plan::Skip { .. }), "{got:?}");
}

#[test]
fn workflow_dispatch_forces_a_release_with_its_bump_even_when_skipping() {
    let got = plan(&PlanInput {
        forced: Some(Bump::Minor),
        ..input(&["v0.0.3"], &["release:skip"], "Docs (#11) [skip release]")
    });
    assert_eq!(
        got,
        Plan::Release {
            version: v("0.1.0"),
            previous: Some(v("0.0.3")),
        }
    );
}

#[test]
fn a_forced_patch_still_honours_a_larger_label() {
    let got = plan(&PlanInput {
        forced: Some(Bump::Patch),
        ..input(&["v0.0.3"], &["release:minor"], "Finish v0.1 (#50)")
    });
    assert_eq!(
        got,
        Plan::Release {
            version: v("0.1.0"),
            previous: Some(v("0.0.3")),
        }
    );
}

#[test]
fn a_merge_bumps_the_patch_of_the_highest_version_tag() {
    // `latest` is the rolling release; v0.0.10 sorts above v0.0.9 numerically.
    let tags = [
        "v0.0.9",
        "latest",
        "v0.0.10",
        "v0.0.2",
        "not-a-version",
        "v0.1.0-rc1",
    ];
    assert_eq!(
        plan(&input(&tags, &[], "Fix the ratsnest (#40)")),
        Plan::Release {
            version: v("0.0.11"),
            previous: Some(v("0.0.10")),
        }
    );
}
