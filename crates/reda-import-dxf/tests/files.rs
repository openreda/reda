//! The forms a DXF file comes in: binary as well as ASCII, text in UTF-8, Windows-1252 or
//! another code page, and DWG, which is refused (docs/spec/v0.3/dxf.md#files-and-entities).

use std::path::Path;

use reda_import_dxf::{Drawing, ReadError, Shape, read};

fn bytes(name: &str) -> Vec<u8> {
    std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .unwrap()
}

fn fixture(name: &str) -> Drawing {
    read(&bytes(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn text(drawing: &Drawing) -> &str {
    drawing
        .entities
        .iter()
        .find_map(|e| match &e.shape {
            Shape::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .expect("a text")
}

#[test]
fn a_binary_file_reads_the_same_as_its_ascii_twin() {
    let binary = fixture("binary.dxf");
    let ascii = fixture("binary_twin.dxf");
    assert_eq!(binary, ascii);
    assert_eq!(binary.insunits, Some(4));
    let layers: Vec<&str> = binary.entities.iter().map(|e| e.layer.as_str()).collect();
    assert_eq!(layers, ["Sketch", "Holes", "Slot", "Notes", "Notes"]);
    assert_eq!(
        binary.entities[1].shape,
        Shape::Circle {
            centre: reda_import_dxf::Xy::new(3.0, 4.0),
            radius: 1.5
        }
    );
    let Shape::Text(mtext) = &binary.entities[4].shape else {
        panic!("not a text")
    };
    assert_eq!(mtext.text, "Two\nlines");
}

#[test]
fn a_cut_short_binary_file_is_refused_with_the_byte() {
    let file = bytes("binary.dxf");
    let err = read(&file[..file.len() - 30]).unwrap_err();
    assert!(err.to_string().starts_with("byte "), "{err}");
}

#[test]
fn windows_1252_text_before_2007_is_decoded() {
    let drawing = fixture("cp1252.dxf");
    assert_eq!(drawing.entities[0].layer, "Café");
    assert_eq!(text(&drawing), "Größe 25° – “ok”");
    assert!(drawing.warnings.is_empty(), "{:?}", drawing.warnings);
}

#[test]
fn utf8_text_from_2007_is_decoded() {
    let drawing = fixture("utf8.dxf");
    assert_eq!(drawing.entities[0].layer, "Ωhm");
    assert_eq!(text(&drawing), "Größe ±1 Ω");
    assert!(drawing.warnings.is_empty(), "{:?}", drawing.warnings);
}

#[test]
fn another_code_page_is_read_with_a_warning() {
    let drawing = fixture("codepage.dxf");
    assert_eq!(drawing.entities.len(), 1);
    // The Cyrillic layer name's bytes, read as Windows-1252.
    assert_eq!(drawing.entities[0].layer, "Ñëîé");
    assert_eq!(drawing.warnings.len(), 1);
    assert!(
        drawing.warnings[0].contains("ANSI_1251")
            && drawing.warnings[0].contains("non-ASCII text may be wrong"),
        "{:?}",
        drawing.warnings
    );
}

#[test]
fn a_dwg_file_is_refused_as_dwg() {
    let err = read(&bytes("drawing.dwg")).unwrap_err();
    assert_eq!(err, ReadError::Dwg);
    assert!(err.to_string().contains("save it as DXF"), "{err}");
}
