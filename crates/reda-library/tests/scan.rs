//! Scanning registered Library folders through the `LibraryFiles` seam, held in memory.

mod support;

use reda_library::{LibraryRoot, Origin, scan};
use support::{MemFiles, footprint, library, part, symbol, uuid};

#[test]
fn a_library_folder_is_scanned_with_no_manifest() {
    let mut files = MemFiles::default();
    files.library("/libs/passives/passives.redalib", library(1, "Passives"));
    files.symbol("/libs/passives/symbols/R.redasym", symbol(10, "R"));
    files.footprint(
        "/libs/passives/footprints/R_0603.redafp",
        footprint(20, "R_0603"),
    );
    files.part(
        "/libs/passives/parts/R_10k.redapart",
        part(30, "R 10k", 10, &[20]),
    );
    // Files that are not elements of their folder are ignored.
    files.other("/libs/passives/README.md");
    files.other("/libs/passives/symbols/notes.txt");
    files.other("/libs/passives/parts/R.redasym");

    let index = scan(
        &[LibraryRoot::new("/libs/passives", Origin::User)],
        &mut files,
    );

    assert_eq!(index.problems(), &[]);
    let libraries: Vec<_> = index.libraries().collect();
    assert_eq!(libraries.len(), 1);
    assert_eq!(libraries[0].info.name, "Passives");
    assert_eq!(libraries[0].origin, Origin::User);
    assert_eq!(index.symbol(uuid(10)).unwrap().name, "R");
    assert_eq!(index.footprint(uuid(20)).unwrap().name, "R_0603");
    assert_eq!(index.part(uuid(30)).unwrap().name, "R 10k");
    assert_eq!(index.library_of(uuid(30)).unwrap().info.uuid, uuid(1));
    assert_eq!(index.parts().count(), 1);
}

#[test]
fn elements_reference_each_other_by_uuid_across_libraries() {
    let mut files = MemFiles::default();
    files.library("/a/a.redalib", library(1, "Symbols"));
    files.symbol("/a/symbols/Q.redasym", symbol(10, "NPN"));
    files.library("/b/b.redalib", library(2, "Footprints"));
    files.footprint("/b/footprints/SOT23.redafp", footprint(20, "SOT-23"));
    files.library("/c/c.redalib", library(3, "Parts"));
    files.part("/c/parts/BC847.redapart", part(30, "BC847", 10, &[20]));

    let index = scan(
        &[
            LibraryRoot::new("/c", Origin::Project),
            LibraryRoot::new("/a", Origin::Starter),
            LibraryRoot::new("/b", Origin::User),
        ],
        &mut files,
    );

    assert_eq!(index.problems(), &[]);
    let part = index.part(uuid(30)).unwrap();
    assert_eq!(index.symbol(part.symbol.unwrap()).unwrap().name, "NPN");
    assert_eq!(index.footprint(part.footprints[0]).unwrap().name, "SOT-23");
    assert_eq!(index.library_of(uuid(10)).unwrap().info.name, "Symbols");
    let order: Vec<_> = index.libraries().map(|l| l.info.name.as_str()).collect();
    assert_eq!(order, ["Parts", "Symbols", "Footprints"]);
}

#[test]
fn a_folder_without_a_redalib_file_is_reported_and_skipped() {
    let mut files = MemFiles::default();
    files.symbol("/x/symbols/R.redasym", symbol(10, "R"));

    let index = scan(&[LibraryRoot::new("/x", Origin::User)], &mut files);

    assert_eq!(index.libraries().count(), 0);
    assert_eq!(index.symbol(uuid(10)), None);
    let problems = index.problems();
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].path, std::path::Path::new("/x"));
    assert!(problems[0].message.contains(".redalib"), "{problems:?}");
}

#[test]
fn a_bad_element_file_is_reported_and_the_rest_still_load() {
    let mut files = MemFiles::default();
    files.library("/x/x.redalib", library(1, "X"));
    files.symbol("/x/symbols/R.redasym", symbol(10, "R"));
    files.broken("/x/symbols/broken.redasym", "not valid JSON");

    let index = scan(&[LibraryRoot::new("/x", Origin::User)], &mut files);

    assert_eq!(index.symbol(uuid(10)).unwrap().name, "R");
    let problems = index.problems();
    assert_eq!(problems.len(), 1);
    assert_eq!(
        problems[0].path,
        std::path::Path::new("/x/symbols/broken.redasym")
    );
    assert_eq!(problems[0].message, "not valid JSON");
}

#[test]
fn a_duplicate_uuid_keeps_the_first_element_found() {
    let mut files = MemFiles::default();
    files.library("/a/a.redalib", library(1, "A"));
    files.symbol("/a/symbols/R.redasym", symbol(10, "R from A"));
    files.library("/b/b.redalib", library(2, "B"));
    files.symbol("/b/symbols/R.redasym", symbol(10, "R from B"));

    let index = scan(
        &[
            LibraryRoot::new("/a", Origin::User),
            LibraryRoot::new("/b", Origin::User),
        ],
        &mut files,
    );

    assert_eq!(index.symbol(uuid(10)).unwrap().name, "R from A");
    let problems = index.problems();
    assert_eq!(problems.len(), 1);
    assert_eq!(
        problems[0].path,
        std::path::Path::new("/b/symbols/R.redasym")
    );
    let first = std::path::Path::new("/a").join("symbols").join("R.redasym");
    let first = first.display().to_string();
    assert!(problems[0].message.contains(&first), "{problems:?}");
}

#[test]
fn a_library_registered_twice_is_scanned_once() {
    let mut files = MemFiles::default();
    files.library("/a/a.redalib", library(1, "A"));
    files.library("/copy/a.redalib", library(1, "A"));

    let index = scan(
        &[
            LibraryRoot::new("/a", Origin::User),
            LibraryRoot::new("/copy", Origin::Project),
        ],
        &mut files,
    );

    assert_eq!(index.libraries().count(), 1);
    assert_eq!(index.problems().len(), 1);
    assert_eq!(index.problems()[0].path, std::path::Path::new("/copy"));
}
