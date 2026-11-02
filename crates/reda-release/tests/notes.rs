use reda_release::{Change, Version, parse_changes, release_notes};

fn v(text: &str) -> Version {
    text.parse().unwrap()
}

#[test]
fn notes_list_the_pull_request_titles_under_the_unsigned_build_warning() {
    let changes = [
        Change {
            pr: Some(41),
            title: "Add the PCB canvas".into(),
        },
        Change {
            pr: Some(42),
            title: "Fix Gerber arcs".into(),
        },
    ];
    let expected = "\
REDA 0.0.5 is an early tester build. The packages are not signed yet, so macOS and Windows \
warn before the first launch: see [Installing an unsigned build]\
(https://github.com/openreda/reda#installing-an-unsigned-build).

## Changes

- Add the PCB canvas (#41)
- Fix Gerber arcs (#42)

Every file's SHA-256 is in `SHA256SUMS`.
";
    assert_eq!(release_notes(v("0.0.5"), &changes), expected);
}

#[test]
fn a_change_without_a_pull_request_shows_its_title_alone() {
    let notes = release_notes(
        v("0.0.6"),
        &[Change {
            pr: None,
            title: "Bump the pinned toolchain".into(),
        }],
    );
    assert!(notes.contains("\n- Bump the pinned toolchain\n"), "{notes}");
}

#[test]
fn a_forced_release_with_no_new_pull_requests_says_so() {
    let notes = release_notes(v("0.0.7"), &[]);
    assert!(
        notes.contains("\n- No pull requests since the previous release.\n"),
        "{notes}"
    );
}

#[test]
fn changes_are_read_from_tab_separated_lines_and_deduplicated() {
    // What the workflow writes: `<pr number or empty>\t<title>`, oldest first. A PR can show
    // up twice when several of its commits landed on main.
    let text =
        "41\tAdd the PCB canvas\n\tBump the toolchain\n41\tAdd the PCB canvas\n\n42\tFix\tarcs\n";
    assert_eq!(
        parse_changes(text),
        vec![
            Change {
                pr: Some(41),
                title: "Add the PCB canvas".into()
            },
            Change {
                pr: None,
                title: "Bump the toolchain".into()
            },
            Change {
                pr: Some(42),
                title: "Fix\tarcs".into()
            },
        ]
    );
}
