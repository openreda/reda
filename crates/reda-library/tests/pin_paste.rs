//! Column-mapping paste: splitting pasted text into cells, finding a header, mapping each
//! column to a Pin field, reading Pin types through their aliases, and the Remove Row and
//! Remove Column edits, all as pure functions the wizard dialog and the Pins Panel call.

use reda_library::wizard::Side;
use reda_library::wizard::paste::{Column, Delimiter, PasteTable, pin_type};
use reda_model::PinType;

#[test]
fn the_spec_type_aliases_are_read_ignoring_case() {
    for (text, ty) in [
        ("I/O", PinType::Bidirectional),
        ("io", PinType::Bidirectional),
        ("Bidir", PinType::Bidirectional),
        ("HIZ", PinType::TriState),
        ("3s", PinType::TriState),
        ("PWR", PinType::PowerIn),
        ("power", PinType::PowerIn),
        ("OC", PinType::OpenCollector),
        ("NC", PinType::NotConnected),
        ("n/c", PinType::NotConnected),
    ] {
        assert_eq!(pin_type(text), Some(ty), "{text}");
    }
}

#[test]
fn every_type_is_also_read_by_its_own_name() {
    for (text, ty) in [
        ("input", PinType::Input),
        ("Output", PinType::Output),
        ("tri-state", PinType::TriState),
        ("power in", PinType::PowerIn),
        ("power_out", PinType::PowerOut),
        ("Open Collector", PinType::OpenCollector),
        ("open emitter", PinType::OpenEmitter),
        ("passive", PinType::Passive),
        ("not connected", PinType::NotConnected),
        ("bidirectional", PinType::Bidirectional),
    ] {
        assert_eq!(pin_type(text), Some(ty), "{text}");
    }
    assert_eq!(pin_type("sideways"), None);
}

#[test]
fn the_delimiter_is_detected_from_the_text() {
    assert_eq!(Delimiter::detect("1\tVCC\n2\tGND"), Delimiter::Tab);
    assert_eq!(Delimiter::detect("1,VCC\n2,GND"), Delimiter::Comma);
    assert_eq!(Delimiter::detect("1;VCC"), Delimiter::Semicolon);
    assert_eq!(Delimiter::detect("1  VCC\n2 GND"), Delimiter::Whitespace);
}

#[test]
fn cells_are_split_trimmed_and_quoted_cells_keep_their_delimiters() {
    let t = PasteTable::parse("1, \"A,B\" ,x\n\n2,C\n", Some(Delimiter::Comma));
    assert_eq!(
        t.rows,
        [
            vec!["1".to_owned(), "A,B".into(), "x".into()],
            vec!["2".into(), "C".into(), String::new()],
        ]
    );
    let t = PasteTable::parse("1   VCC  power\n2 GND", Some(Delimiter::Whitespace));
    assert_eq!(t.rows[0], ["1", "VCC", "power"]);
}

#[test]
fn a_header_row_is_detected_and_maps_its_columns() {
    let t = PasteTable::parse(
        "Pin\tSignal\tType\tNotes\n1\tVCC\tpwr\tsupply\n2\tGND\tpower\t",
        None,
    );
    assert!(t.header);
    assert_eq!(
        t.columns,
        [Column::Number, Column::Name, Column::Type, Column::Ignore]
    );
    let pins = t.pins().unwrap();
    assert_eq!(pins.len(), 2);
    assert_eq!(
        (
            pins[0].number.as_str(),
            pins[0].name.as_str(),
            pins[0].electrical_type
        ),
        ("1", "VCC", PinType::PowerIn)
    );
}

#[test]
fn without_a_header_the_columns_are_guessed_from_their_cells() {
    let t = PasteTable::parse("VCC,1,PWR,left\nGND,2,PWR,left\nOUT,3,o,right", None);
    assert!(!t.header);
    assert_eq!(
        t.columns,
        [Column::Name, Column::Number, Column::Type, Column::Side]
    );
    let pins = t.pins().unwrap();
    assert_eq!(pins[2].number, "3");
    assert_eq!(pins[2].electrical_type, PinType::Output);
    assert_eq!(pins[2].side, Some(Side::Right));
}

#[test]
fn bga_style_numbers_are_recognised_as_numbers() {
    let t = PasteTable::parse("A1 VDD\nB12 VSS\nAA3 IO0", None);
    assert_eq!(t.columns, [Column::Number, Column::Name]);
}

#[test]
fn columns_can_be_remapped_and_rows_and_columns_removed() {
    let mut t = PasteTable::parse("1,VCC,x\n2,GND,y\n3,OUT,z", None);
    t.set_column(2, Column::Group);
    assert_eq!(t.pins().unwrap()[1].group.as_deref(), Some("y"));
    t.remove_row(1);
    t.remove_column(1);
    assert_eq!(t.rows, [vec!["1", "x"], vec!["3", "z"]]);
    assert_eq!(t.columns, [Column::Number, Column::Group]);
    let pins = t.pins().unwrap();
    assert_eq!(pins.len(), 2);
    assert_eq!(pins[1].name, "");
    assert_eq!(pins[1].electrical_type, PinType::Passive);
}

#[test]
fn turning_the_header_off_makes_its_row_data() {
    let mut t = PasteTable::parse("Number,Name\n1,VCC", None);
    assert!(t.header);
    assert_eq!(t.pins().unwrap().len(), 1);
    t.header = false;
    assert_eq!(t.pins().unwrap().len(), 2);
}

#[test]
fn a_bad_cell_is_reported_with_its_row_and_column() {
    let t = PasteTable::parse("Number,Type\n1,input\n2,sideways", None);
    let err = t.pins().unwrap_err();
    assert_eq!((err.row, err.column), (Some(2), Some(1)));
    assert!(err.message.contains("sideways"), "{}", err.message);

    let t = PasteTable::parse("Name,Side\nA,left\nB,middle", None);
    let err = t.pins().unwrap_err();
    assert_eq!((err.row, err.column), (None, None), "no Number column");
    let mut t = t;
    t.set_column(0, Column::Number);
    assert_eq!(t.pins().unwrap_err().row, Some(2));
}

#[test]
fn rows_with_an_empty_number_are_skipped() {
    let t = PasteTable::parse("1,VCC\n,\n2,GND", Some(Delimiter::Comma));
    assert_eq!(t.pins().unwrap().len(), 2);
}
