//! The library search query: a substring search over Parts with Library and Component Type
//! filters.

mod support;

use reda_library::{LibraryIndex, LibraryRoot, Origin, SearchQuery, scan};
use reda_model::{ComponentType, Field, Fields};
use support::{MemFiles, library, part, uuid};

/// Two Libraries: passives (1) with three Parts, mechanical (2) with one.
fn index() -> LibraryIndex {
    let mut files = MemFiles::default();
    files.library("/p/p.redalib", library(1, "Passives"));
    let mut r = part(30, "R_0603", 10, &[20]);
    r.fields = Fields {
        value: "10k".into(),
        description: "Thick-film resistor".into(),
        keywords: "res smd".into(),
        ..Fields::default()
    };
    files.part("/p/parts/r.redapart", r);
    let mut c = part(31, "C_0603", 11, &[20]);
    c.fields = Fields {
        value: "100n".into(),
        mpn: "GRM188R71H104KA93D".into(),
        custom: vec![Field {
            name: "LCSC".into(),
            value: "C14663".into(),
        }],
        ..Fields::default()
    };
    files.part("/p/parts/c.redapart", c);
    let mut led = part(32, "LED_0603", 12, &[20]);
    led.fields.manufacturer = "Würth".into();
    files.part("/p/parts/led.redapart", led);

    files.library("/m/m.redalib", library(2, "Mechanical"));
    let mut hole = part(40, "MountingHole_M3", 0, &[21]);
    hole.symbol = None;
    hole.component_type = ComponentType::Mechanical;
    hole.fields.description = "M3 mounting hole".into();
    files.part("/m/parts/hole.redapart", hole);

    scan(
        &[
            LibraryRoot::new("/p", Origin::User),
            LibraryRoot::new("/m", Origin::Starter),
        ],
        &mut files,
    )
}

fn names(index: &LibraryIndex, query: &SearchQuery) -> Vec<String> {
    index
        .search(query)
        .into_iter()
        .map(|hit| hit.part.name.clone())
        .collect()
}

fn text(text: &str) -> SearchQuery {
    SearchQuery {
        text: text.into(),
        ..SearchQuery::default()
    }
}

#[test]
fn an_empty_query_lists_every_part_sorted_by_name() {
    let index = index();
    assert_eq!(
        names(&index, &SearchQuery::default()),
        ["C_0603", "LED_0603", "MountingHole_M3", "R_0603"]
    );
}

#[test]
fn the_text_matches_a_substring_of_the_name_ignoring_case() {
    let index = index();
    assert_eq!(
        names(&index, &text("0603")),
        ["C_0603", "LED_0603", "R_0603"]
    );
    assert_eq!(names(&index, &text("led")), ["LED_0603"]);
}

#[test]
fn the_text_matches_description_keywords_value_mpn_and_field_values() {
    let index = index();
    assert_eq!(names(&index, &text("thick-FILM")), ["R_0603"]);
    assert_eq!(names(&index, &text("smd")), ["R_0603"]);
    assert_eq!(names(&index, &text("100n")), ["C_0603"]);
    assert_eq!(names(&index, &text("grm188")), ["C_0603"]);
    assert_eq!(names(&index, &text("C14663")), ["C_0603"]);
    assert_eq!(names(&index, &text("würth")), ["LED_0603"]);
    assert_eq!(names(&index, &text("no such part")), Vec::<String>::new());
}

#[test]
fn a_field_name_alone_does_not_match() {
    let index = index();
    assert_eq!(names(&index, &text("LCSC")), Vec::<String>::new());
}

#[test]
fn the_library_filter_keeps_one_library() {
    let index = index();
    let query = SearchQuery {
        library: Some(uuid(2)),
        ..SearchQuery::default()
    };
    assert_eq!(names(&index, &query), ["MountingHole_M3"]);
}

#[test]
fn the_component_type_filter_keeps_one_type() {
    let index = index();
    let query = SearchQuery {
        text: "m".into(),
        component_type: Some(ComponentType::Standard),
        ..SearchQuery::default()
    };
    // "m" is in "MountingHole_M3", but that Part is Mechanical.
    assert_eq!(names(&index, &query), ["C_0603", "R_0603"]);
    let query = SearchQuery {
        component_type: Some(ComponentType::Mechanical),
        ..SearchQuery::default()
    };
    assert_eq!(names(&index, &query), ["MountingHole_M3"]);
}

#[test]
fn a_hit_names_its_library() {
    let index = index();
    let hits = index.search(&text("hole"));
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].library.info.name, "Mechanical");
    assert_eq!(hits[0].library.origin, Origin::Starter);
}
