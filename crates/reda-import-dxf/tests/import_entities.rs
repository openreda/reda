//! What the entities added after the first import land as, through the engine: free text,
//! fitted ellipses, exploded INSERTs, binary files and code-page warnings, and the DWG
//! refusal, which writes nothing.

use std::f64::consts::{PI, TAU};
use std::path::PathBuf;

use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{
    GraphicShape, HERSHEY_FONT, PcbDocument, PcbGraphic, PcbText, TextHJustify, TextJustify,
    TextVJustify, Uuid,
};
use reda_testkit::{TempDir, TestEngine, probe_engine, read_tree};
use serde_json::{Value, json};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn board(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Demo","layers":2}),
    );
    engine
}

fn pcb(e: &TestEngine) -> &PcbDocument {
    e.project().unwrap().pcb().unwrap()
}

fn layer_named(e: &TestEngine, title: &str) -> Uuid {
    let pcb = pcb(e);
    pcb.layers()
        .layers
        .iter()
        .find(|l| pcb.layer_title(l) == title)
        .unwrap_or_else(|| panic!("no Layer {title}"))
        .uuid
}

fn import(e: &mut TestEngine, params: Value) -> Value {
    e.run_ok("pcb.import_dxf", params)["result"].clone()
}

fn graphics(e: &TestEngine) -> Vec<PcbGraphic> {
    pcb(e).graphics().cloned().collect()
}

fn texts(e: &TestEngine) -> Vec<PcbText> {
    pcb(e).texts().cloned().collect()
}

fn mm(x: f64, y: f64) -> Point {
    Point::new(Nm((x * 1e6).round() as i64), Nm((y * 1e6).round() as i64))
}

#[test]
fn text_and_mtext_become_free_text_in_the_stroke_font() {
    let dir = TempDir::new("dxf-text");
    let mut e = board(&dir);
    let out = import(&mut e, json!({"path": fixture("text.dxf")}));
    let objects: Vec<Uuid> = serde_json::from_value(out["objects"].clone()).unwrap();
    let all = texts(&e);
    assert_eq!(all.len(), 6);
    assert_eq!(objects.len(), 6);
    assert!(objects.iter().all(|o| pcb(&e).text(*o).is_some()));
    assert!(graphics(&e).is_empty());
    let reference = layer_named(&e, "Reference");
    assert!(
        all.iter()
            .all(|t| t.layer == reference && t.font == HERSHEY_FONT)
    );
    let by_text = |s: &str| {
        all.iter()
            .find(|t| t.text == s)
            .unwrap_or_else(|| panic!("no text {s:?}"))
    };
    let hello = by_text("Hello °");
    assert_eq!(hello.position, mm(1.0, 2.0));
    assert_eq!(hello.height, Nm(2_500_000));
    assert_eq!(hello.rotation, Millideg(30_000));
    assert_eq!(
        hello.justify,
        TextJustify {
            h: TextHJustify::Left,
            v: TextVJustify::Bottom
        }
    );
    assert!(!hello.mirrored);
    let mtext = by_text("First line\nSecond line");
    assert_eq!(mtext.position, mm(10.0, 10.0));
    assert_eq!(mtext.height, Nm(3_000_000));
    assert_eq!(mtext.rotation, Millideg(90_000));
    assert_eq!(mtext.justify, TextJustify::CENTER);
    assert_eq!(out["skipped"], json!({"empty TEXT": 1}));

    let doc = pcb(&e).uuid;
    e.run_ok("edit.undo", json!({"document": doc}));
    assert!(texts(&e).is_empty());
}

#[test]
fn text_on_the_board_outline_row_is_skipped_and_counted() {
    let dir = TempDir::new("dxf-outline-text");
    let mut e = board(&dir);
    let out = import(
        &mut e,
        json!({"path": fixture("text.dxf"), "mapping": {"Notes": "board_outline"}}),
    );
    assert!(texts(&e).is_empty());
    assert_eq!(
        out["skipped"],
        json!({"empty TEXT": 1, "text on the Board Outline layer": 6})
    );
}

/// The centre of the arc from `from` to `to` sweeping `sweep`.
fn centre(from: Point, to: Point, sweep: Millideg) -> (f64, f64) {
    let (ax, ay, bx, by) = (
        from.x.0 as f64,
        from.y.0 as f64,
        to.x.0 as f64,
        to.y.0 as f64,
    );
    let s = (sweep.0 as f64 / 1000.0).to_radians();
    let (dx, dy) = (bx - ax, by - ay);
    let chord = dx.hypot(dy);
    let offset = chord / 2.0 / (s / 2.0).tan();
    (
        (ax + bx) / 2.0 - dy / chord * offset,
        (ay + by) / 2.0 + dx / chord * offset,
    )
}

/// How far (x, y), in nm, is from the nearest of `graphics`.
fn distance(graphics: &[PcbGraphic], x: f64, y: f64) -> f64 {
    graphics
        .iter()
        .map(|g| match g.shape {
            GraphicShape::Arc { from, to, sweep } => {
                let (cx, cy) = centre(from, to, sweep);
                let r = (from.x.0 as f64 - cx).hypot(from.y.0 as f64 - cy);
                let a0 = (from.y.0 as f64 - cy).atan2(from.x.0 as f64 - cx);
                let a = (y - cy).atan2(x - cx);
                let s = (sweep.0 as f64 / 1000.0).to_radians();
                let d = if s >= 0.0 { a - a0 } else { a0 - a };
                if d.rem_euclid(TAU) <= s.abs() {
                    ((x - cx).hypot(y - cy) - r).abs()
                } else {
                    let end = |p: Point| (x - p.x.0 as f64).hypot(y - p.y.0 as f64);
                    end(from).min(end(to))
                }
            }
            GraphicShape::Line { from, to } => {
                let (fx, fy) = (from.x.0 as f64, from.y.0 as f64);
                let (dx, dy) = (to.x.0 as f64 - fx, to.y.0 as f64 - fy);
                let s = (((x - fx) * dx + (y - fy) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
                (x - fx - dx * s).hypot(y - fy - dy * s)
            }
            GraphicShape::Rect { .. } => f64::INFINITY,
        })
        .fold(f64::INFINITY, f64::min)
}

#[test]
fn ellipses_land_as_arcs_within_5_um_in_the_unit_the_drawing_is_read_in() {
    let dir = TempDir::new("dxf-ellipse");
    let mut e = board(&dir);
    // In inches the ellipse is 203 mm wide: the fit holds 5 µm on the board, not 5 µm × 25.4.
    import(
        &mut e,
        json!({"path": fixture("ellipse.dxf"), "unit": "inch", "mapping": {
            "Circles": "skip", "Flipped": "skip"
        }}),
    );
    let all = graphics(&e);
    assert!(
        all.iter()
            .all(|g| matches!(g.shape, GraphicShape::Arc { .. }))
    );
    let inch = 25_400_000.0;
    for i in 0..=4000 {
        let t = TAU * i as f64 / 4000.0;
        let (x, y) = ((10.0 + 4.0 * t.cos()) * inch, (5.0 + 2.0 * t.sin()) * inch);
        let d = distance(&all, x, y);
        assert!(d <= 5_000.0, "{d} nm from the fit at t = {t}");
        if t <= PI {
            let (x, y) = (-0.75 * t.sin() * inch, 3.0 * t.cos() * inch);
            let d = distance(&all, x, y);
            assert!(
                d <= 5_000.0,
                "{d} nm from the half ellipse's fit at t = {t}"
            );
        }
    }
}

#[test]
fn a_circular_ellipse_lands_as_one_arc() {
    let dir = TempDir::new("dxf-circular");
    let mut e = board(&dir);
    import(
        &mut e,
        json!({"path": fixture("ellipse.dxf"), "mapping": {
            "Ellipses": "skip", "Flipped": "skip"
        }}),
    );
    let all = graphics(&e);
    assert_eq!(all.len(), 1);
    let GraphicShape::Arc { from, to, sweep } = all[0].shape else {
        panic!("not an arc: {:?}", all[0].shape)
    };
    assert_eq!(
        (from, to, sweep),
        (mm(3.0, 3.0), mm(1.0, 3.0), Millideg(90_000))
    );
}

#[test]
fn exploded_inserts_count_on_the_layers_their_entities_land_on() {
    let dir = TempDir::new("dxf-insert");
    let mut e = board(&dir);
    let report = e.run_ok(
        "pcb.import_dxf.preview",
        json!({"path": fixture("insert.dxf")}),
    );
    let rows: Vec<(&str, u64)> = report["layers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["name"].as_str().unwrap(), r["entities"].as_u64().unwrap()))
        .collect();
    assert_eq!(
        rows,
        [("Parts", 1), ("Holes", 3), ("Frame", 1), ("Array", 2)]
    );
    // Lines and half circles: 1 + 3×2 + 1 + 2.
    let out = import(&mut e, json!({"path": fixture("insert.dxf")}));
    assert_eq!(out["objects"].as_array().unwrap().len(), 10);
}

#[test]
fn a_binary_file_imports_like_its_ascii_twin() {
    let land = |name: &str| {
        let dir = TempDir::new("dxf-binary");
        let mut e = board(&dir);
        import(&mut e, json!({"path": fixture(name)}));
        let mut shapes: Vec<_> = graphics(&e).into_iter().map(|g| g.shape).collect();
        shapes.sort_by_key(|s| format!("{s:?}"));
        let mut words: Vec<_> = texts(&e)
            .into_iter()
            .map(|t| (t.text, t.position))
            .collect();
        words.sort_by_key(|w| format!("{w:?}"));
        (shapes, words)
    };
    let binary = land("binary.dxf");
    assert!(!binary.0.is_empty() && binary.1.len() == 2, "{binary:?}");
    assert_eq!(binary, land("binary_twin.dxf"));
}

#[test]
fn another_code_page_imports_with_a_warning() {
    let dir = TempDir::new("dxf-codepage");
    let mut e = board(&dir);
    let out = import(&mut e, json!({"path": fixture("codepage.dxf")}));
    let warnings = out["warnings"].as_array().unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("non-ASCII text may be wrong")),
        "{warnings:?}"
    );
    assert_eq!(graphics(&e).len(), 1);
}

#[test]
fn a_dwg_file_is_refused_with_a_clear_message_and_writes_nothing() {
    let dir = TempDir::new("dxf-dwg");
    let mut e = board(&dir);
    e.run_ok("project.save", json!({}));
    let before = pcb(&e).clone();
    let files = read_tree(dir.path());
    for id in ["pcb.import_dxf.preview", "pcb.import_dxf"] {
        let err = e.run_err(id, json!({"path": fixture("drawing.dwg")}));
        let message = err.to_string();
        assert!(
            message.contains("is a DWG drawing") && message.contains("save it as DXF"),
            "{message}"
        );
    }
    assert_eq!(pcb(&e), &before);
    assert_eq!(
        read_tree(dir.path()),
        files,
        "a refused import wrote something"
    );
}
