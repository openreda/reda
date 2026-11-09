//! Annotating the board through the Shell with no window (docs/spec/v0.1/pcb.md): `Mod`+M
//! measuring, kept for the session and logged to Messages; the arc Tool's placement
//! modes; graphic lines that never take a Net; the Dimension Tool, its handles and the
//! question deleting a referenced object asks.

use std::path::{Path, PathBuf};

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{SeededIds, UserDirs};
use reda_ui::engine::model::geom::{Nm, Point};
use reda_ui::engine::model::{LayerKind, Side, Uuid};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library/starter")
}

const TOLERANCE: Nm = Nm::from_um(200);

struct Board {
    shell: Shell,
    _home: TempDir,
    _work: TempDir,
}

fn run(shell: &mut Shell, id: &str, params: Value) -> Value {
    match shell.run(id, params.clone()) {
        Ok(out) => out,
        Err(e) => panic!("{id} {params} failed: {e}"),
    }
}

fn mm(x: f64, y: f64) -> Point {
    Point::new(Nm((x * 1e6).round() as i64), Nm((y * 1e6).round() as i64))
}

/// A new Project with its board open and the grid at 0.5 mm.
fn board() -> Board {
    let home = TempDir::new("pcb-annotate-home");
    let work = TempDir::new("pcb-annotate-work");
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(library()))
        .commands(reda_testkit::probe_commands)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Board").unwrap();
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    run(&mut shell, "document.open", json!({"document": pcb}));
    run(
        &mut shell,
        "view.grid.set",
        json!({"document": pcb, "grid": "0.5mm"}),
    );
    Board {
        shell,
        _home: home,
        _work: work,
    }
}

impl Board {
    fn tool(&mut self, tool: &str) {
        run(&mut self.shell, &format!("tool.activate:{tool}"), json!({}));
    }

    fn click(&mut self, at: Point) {
        self.shell.pcb_hover(at);
        self.shell.pcb_press(at, false, TOLERANCE);
        self.shell.pcb_release();
    }

    fn drag(&mut self, from: Point, to: Point) {
        self.shell.pcb_hover(from);
        self.shell.pcb_press(from, false, TOLERANCE);
        self.shell.pcb_drag(to);
        self.shell.pcb_release();
    }

    fn mark(&self) -> usize {
        self.shell.engine().journal().len()
    }

    /// The edit Commands run since `from`, view ones left out.
    fn edits_since(&self, from: usize) -> Vec<String> {
        let registry = self.shell.engine().registry();
        self.shell.engine().journal()[from..]
            .iter()
            .filter(|e| {
                let base = e.id.split(':').next().unwrap();
                registry.get(base).map(|m| m.kind) != Some(reda_ui::engine::command::Kind::View)
            })
            .map(|e| e.id.clone())
            .collect()
    }

    fn pcb(&self) -> &reda_ui::engine::model::PcbDocument {
        self.shell.engine().project().unwrap().pcb().unwrap()
    }

    fn layer(&self, kind: LayerKind, side: Option<Side>) -> Uuid {
        self.pcb().layers().find(kind, side).unwrap().uuid
    }

    fn activate(&mut self, layer: Uuid) {
        run(
            &mut self.shell,
            "pcb.layer.activate",
            json!({"layer": layer}),
        );
    }
}

// ---- Measuring ----

#[test]
fn mod_m_measures_on_the_board() {
    use reda_ui::bars::{Binding, DocumentType};
    use reda_ui::keymap::{Active, Chord};
    let b = board();
    let active = Active {
        tool: None,
        document: Some(DocumentType::Pcb),
        part: false,
    };
    let action = b
        .shell
        .keymap()
        .lookup(&Chord::parse("Mod+M").unwrap(), &active)
        .map(|a| a.binding.clone());
    assert_eq!(
        action,
        Some(Binding::Preset("tool.activate:measure".into()))
    );
}

#[test]
fn measurements_repeat_and_stay_for_the_session_without_editing() {
    let mut b = board();
    let start = b.mark();
    b.tool("measure");
    b.click(mm(0.0, 0.0));
    b.click(mm(3.0, 4.0));
    // The next click starts another measurement; the first stays.
    b.click(mm(10.0, 0.0));
    b.shell.pcb_hover(mm(10.0, 2.0));
    let live = b.shell.pcb_draw().measurement().unwrap();
    assert_eq!((live.from, live.to), (mm(10.0, 0.0), mm(10.0, 2.0)));
    b.click(mm(10.0, 2.0));
    b.tool("select");
    let kept = reda_ui::pcb::measure::shown(&b.shell);
    assert_eq!(kept.len(), 2, "{kept:?}");
    assert_eq!(kept[0].distance(), Nm::from_mm(5));
    assert_eq!(kept[1].distance(), Nm::from_mm(2));
    assert!(b.edits_since(start).is_empty());
}

#[test]
fn each_measurement_is_logged_to_messages_and_shift_c_clears_the_canvas_only() {
    let mut b = board();
    b.tool("measure");
    b.click(mm(0.0, 0.0));
    b.click(mm(3.0, 4.0));
    b.click(mm(0.0, 0.0));
    b.click(mm(0.0, 2.0));
    let rows: Vec<String> = b
        .shell
        .messages()
        .notes()
        .iter()
        .filter(|n| n.source == reda_ui::pcb::measure::SOURCE)
        .map(|n| n.text.clone())
        .collect();
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert!(rows[0].contains("5 mm"), "{rows:?}");
    assert!(
        rows[0].contains("3 mm") && rows[0].contains("4 mm"),
        "{rows:?}"
    );
    // Shift+C clears what the canvas shows, never the log.
    run(&mut b.shell, "view.highlight.clear", json!({}));
    assert!(reda_ui::pcb::measure::shown(&b.shell).is_empty());
    assert_eq!(
        b.shell
            .messages()
            .notes()
            .iter()
            .filter(|n| n.source == reda_ui::pcb::measure::SOURCE)
            .count(),
        2
    );
}

#[test]
fn a_double_click_in_messages_shows_a_measurement_again() {
    use reda_ui::messages::LineKey;
    let mut b = board();
    b.tool("measure");
    b.click(mm(0.0, 0.0));
    b.click(mm(3.0, 4.0));
    b.click(mm(0.0, 0.0));
    b.click(mm(0.0, 2.0));
    run(&mut b.shell, "view.highlight.clear", json!({}));
    let index = b
        .shell
        .messages()
        .notes()
        .iter()
        .position(|n| n.source == reda_ui::pcb::measure::SOURCE)
        .unwrap();
    // What a double-click on the row does.
    b.shell.open_details(LineKey::Note(index));
    let shown = reda_ui::pcb::measure::shown(&b.shell);
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].distance(), Nm::from_mm(5));
}

// ---- Arcs and lines ----

fn set_pref(b: &mut Board, key: &str, value: Value) {
    run(
        &mut b.shell,
        "app.preferences.set",
        json!({"key": key, "value": value}),
    );
}

fn uuid(v: &Value) -> Uuid {
    v.as_str().unwrap().parse().unwrap()
}

#[test]
fn the_arc_tool_places_by_centre_storing_the_same_arc() {
    use reda_ui::engine::model::GraphicShape;
    use reda_ui::engine::model::geom::Millideg;
    let mut b = board();
    let silk = b.layer(LayerKind::Silkscreen, Some(Side::Top));
    b.activate(silk);
    set_pref(&mut b, "pcb.arc.mode", json!("center"));
    b.tool("arc");
    let start = b.mark();
    b.click(mm(0.0, 0.0)); // the centre
    b.click(mm(5.0, 0.0)); // the start, setting the radius
    b.shell.pcb_hover(mm(0.0, 8.0));
    assert!(b.shell.pcb_draw().preview().is_some());
    b.click(mm(0.0, 8.0)); // the end's direction
    assert_eq!(b.edits_since(start), ["pcb.graphic.add_arc"]);
    let shapes: Vec<GraphicShape> = b.pcb().graphics().map(|g| g.shape).collect();
    assert_eq!(
        shapes,
        [GraphicShape::Arc {
            from: mm(5.0, 0.0),
            to: mm(0.0, 5.0),
            sweep: Millideg::from_deg(90),
        }]
    );
}

#[test]
fn the_arc_tool_places_a_full_circle_as_one_undo_step() {
    use reda_ui::engine::model::GraphicShape;
    use reda_ui::engine::model::geom::Millideg;
    let mut b = board();
    let silk = b.layer(LayerKind::Silkscreen, Some(Side::Top));
    b.activate(silk);
    set_pref(&mut b, "pcb.arc.mode", json!("circle"));
    b.tool("arc");
    let start = b.mark();
    b.click(mm(0.0, 0.0)); // the centre
    b.click(mm(3.0, 0.0)); // a point on the circle
    assert_eq!(b.edits_since(start), ["batch"]);
    let shapes: Vec<GraphicShape> = b.pcb().graphics().map(|g| g.shape).collect();
    assert_eq!(
        shapes,
        [
            GraphicShape::Arc {
                from: mm(3.0, 0.0),
                to: mm(-3.0, 0.0),
                sweep: Millideg::from_deg(180),
            },
            GraphicShape::Arc {
                from: mm(-3.0, 0.0),
                to: mm(3.0, 0.0),
                sweep: Millideg::from_deg(180),
            },
        ]
    );
    let pcb = b.pcb().uuid;
    run(&mut b.shell, "edit.undo", json!({"document": pcb}));
    assert_eq!(b.pcb().graphics().count(), 0);
}

#[test]
fn a_graphic_line_on_copper_never_adopts_a_net() {
    let mut b = board();
    let gnd = uuid(&run(&mut b.shell, "test.net.add", json!({"name": "GND"}))["result"]["uuid"]);
    run(
        &mut b.shell,
        "pcb.track.add",
        json!({"layer": "copper_top", "net": gnd, "width": "0.5mm",
               "points": [{"x": 0, "y": 0}, {"x": "4mm", "y": 0}]}),
    );
    let top = b.layer(LayerKind::Copper, Some(Side::Top));
    b.activate(top);
    b.tool("line");
    b.click(mm(4.0, 0.0)); // from the Track's end
    b.click(mm(4.0, 6.0));
    let graphic = b.pcb().graphics().next().unwrap().clone();
    assert_eq!(graphic.layer, top);
    let stored = serde_json::to_value(&graphic).unwrap();
    assert!(stored.get("net").is_none(), "{stored}");
    let copper = reda_ui::engine::pcb::Copper::of(b.shell.engine().project().unwrap()).unwrap();
    assert_eq!(copper.net_at(top, mm(4.0, 6.0)), None);
    assert_eq!(copper.net_at(top, mm(2.0, 0.0)), Some(gnd));
}

// ---- Dimensions ----

/// Two Free Pads, at (0, 0) and (10, 0) mm.
fn two_pads(b: &mut Board) -> (Uuid, Uuid) {
    let via = |b: &mut Board, x: &str| {
        let out = run(
            &mut b.shell,
            "pcb.free_pad.add",
            json!({"kind": "plated_through_hole", "shape": "circle", "width": "2mm",
                   "hole": {"width": "1mm"}, "at": {"x": x, "y": "0mm"}}),
        );
        uuid(&out["result"]["uuid"])
    };
    (via(b, "0mm"), via(b, "10mm"))
}

fn the_dimension(b: &Board) -> reda_ui::engine::model::Dimension {
    b.pcb().dimensions().next().expect("a Dimension").clone()
}

/// A Dimension on the top silkscreen from Via `a` to Via `b`, 3 mm above them.
fn place_dimension(b: &mut Board) -> (Uuid, Uuid) {
    let (va, vb) = two_pads(b);
    let silk = b.layer(LayerKind::Silkscreen, Some(Side::Top));
    b.activate(silk);
    b.tool("dimension");
    b.click(mm(0.0, 0.0));
    b.click(mm(10.0, 0.0));
    b.click(mm(5.0, 3.0));
    (va, vb)
}

#[test]
fn the_dimension_tool_attaches_its_ends_to_the_hotspots_under_the_clicks() {
    use reda_ui::engine::model::{Hotspot, HotspotRef};
    let mut b = board();
    let start = b.mark();
    let (va, vb) = place_dimension(&mut b);
    assert_eq!(
        b.edits_since(start),
        ["pcb.free_pad.add", "pcb.free_pad.add", "pcb.dimension.add"]
    );
    let d = the_dimension(&b);
    assert_eq!(d.layer, b.layer(LayerKind::Silkscreen, Some(Side::Top)));
    assert_eq!(
        d.a.reference,
        Some(HotspotRef {
            object: va,
            hotspot: Hotspot::Center
        })
    );
    assert_eq!(
        d.b.reference,
        Some(HotspotRef {
            object: vb,
            hotspot: Hotspot::Center
        })
    );
    assert_eq!(d.offset, Nm::from_mm(3));
    assert_eq!(reda_ui::engine::pcb::dimension::value(&d), Nm::from_mm(10));
    // It follows the Via it is attached to.
    run(
        &mut b.shell,
        "pcb.move",
        json!({"objects": [vb], "by": {"x": "2mm", "y": "0mm"}}),
    );
    assert_eq!(
        reda_ui::engine::pcb::dimension::value(&the_dimension(&b)),
        Nm::from_mm(12)
    );
}

#[test]
fn a_dimension_end_in_free_space_has_no_reference_and_previews_while_placed() {
    let mut b = board();
    let silk = b.layer(LayerKind::Silkscreen, Some(Side::Top));
    b.activate(silk);
    b.tool("dimension");
    b.click(mm(20.0, 20.0));
    b.shell.pcb_hover(mm(25.0, 20.0));
    assert_eq!(
        b.shell.pcb_draw().preview().unwrap().points,
        [mm(20.0, 20.0), mm(25.0, 20.0)]
    );
    b.click(mm(25.0, 20.0));
    b.shell.pcb_hover(mm(22.0, 22.0));
    assert_eq!(
        b.shell.pcb_draw().preview().unwrap().points,
        [
            mm(20.0, 20.0),
            mm(20.0, 22.0),
            mm(25.0, 22.0),
            mm(25.0, 20.0)
        ]
    );
    b.click(mm(22.0, 22.0));
    let d = the_dimension(&b);
    assert_eq!(d.a.reference, None);
    assert_eq!((d.a.point, d.b.point), (mm(20.0, 20.0), mm(25.0, 20.0)));
}

fn select(b: &mut Board, objects: &[Uuid]) {
    run(&mut b.shell, "selection.set", json!({ "objects": objects }));
}

#[test]
fn handles_move_an_end_the_extension_length_and_the_text() {
    use reda_ui::engine::model::DimensionTextPosition;
    use reda_ui::pcb::dimension_edit::{Handle, handles};
    let mut b = board();
    place_dimension(&mut b);
    b.tool("select");
    let d = the_dimension(&b);
    select(&mut b, &[d.uuid]);
    let at = |b: &Board, h: Handle| {
        handles(&b.shell, &the_dimension(b))
            .into_iter()
            .find(|(k, _)| *k == h)
            .unwrap()
            .1
    };
    assert_eq!(at(&b, Handle::Offset), mm(5.0, 3.0));

    // An end dropped in free space lets go of its Via.
    let start = b.mark();
    b.drag(mm(10.0, 0.0), mm(12.0, 1.0));
    assert_eq!(b.edits_since(start), ["pcb.dimension.set"]);
    let d = the_dimension(&b);
    assert_eq!((d.b.point, d.b.reference), (mm(12.0, 1.0), None));

    // The extension length.
    let middle = at(&b, Handle::Offset);
    b.drag(middle, middle + mm(0.0, 2.0));
    assert_eq!(the_dimension(&b).offset, Nm::from_mm(5));

    // The text goes where it is dropped, as `manual`.
    let text = at(&b, Handle::Text);
    b.drag(text, mm(4.0, 9.0));
    assert_eq!(
        the_dimension(&b).text_position,
        DimensionTextPosition::Manual(mm(4.0, 9.0))
    );
}

#[test]
fn deleting_a_referenced_object_asks_whether_to_delete_the_dimension_too() {
    use reda_ui::pcb::delete;
    let mut b = board();
    let (va, _) = place_dimension(&mut b);
    let d = the_dimension(&b).uuid;
    b.tool("select");
    select(&mut b, &[va]);

    // Del asks first, and Cancel deletes nothing.
    let start = b.mark();
    run(
        &mut b.shell,
        "pcb.copper.delete",
        json!({ "objects": [va] }),
    );
    assert_eq!(delete::pending(&b.shell).unwrap().dimensions, [d]);
    assert!(b.edits_since(start).is_empty());
    run(
        &mut b.shell,
        "pcb.delete.confirm",
        json!({"answer": "cancel"}),
    );
    assert!(delete::pending(&b.shell).is_none());
    assert!(b.pcb().free_pad(va).is_some());

    // Keep: the Via goes, the Dimension stays with no `ref` there.
    run(
        &mut b.shell,
        "pcb.copper.delete",
        json!({ "objects": [va] }),
    );
    run(
        &mut b.shell,
        "pcb.delete.confirm",
        json!({"answer": "keep_dimensions"}),
    );
    assert!(b.pcb().free_pad(va).is_none());
    assert_eq!(the_dimension(&b).a.reference, None);
    let pcb = b.pcb().uuid;
    run(&mut b.shell, "edit.undo", json!({"document": pcb}));

    // Delete too: both go, as one undo step.
    let start = b.mark();
    run(
        &mut b.shell,
        "pcb.copper.delete",
        json!({ "objects": [va] }),
    );
    run(
        &mut b.shell,
        "pcb.delete.confirm",
        json!({"answer": "delete_dimensions"}),
    );
    assert_eq!(b.edits_since(start), ["batch"]);
    assert!(b.pcb().free_pad(va).is_none());
    assert_eq!(b.pcb().dimensions().count(), 0);
}

#[test]
fn del_deletes_a_selected_dimension_with_the_other_board_objects() {
    let mut b = board();
    let (va, _) = place_dimension(&mut b);
    let d = the_dimension(&b).uuid;
    let start = b.mark();
    run(
        &mut b.shell,
        "pcb.copper.delete",
        json!({ "objects": [va, d] }),
    );
    assert!(
        reda_ui::pcb::delete::pending(&b.shell).is_none(),
        "nothing to ask"
    );
    assert_eq!(b.edits_since(start), ["batch"]);
    assert!(b.pcb().free_pad(va).is_none());
    assert_eq!(b.pcb().dimensions().count(), 0);
}

#[test]
fn a_selection_length_from_the_window_is_logged_to_messages() {
    let mut b = board();
    let out = run(
        &mut b.shell,
        "pcb.track.add",
        json!({"layer": "copper_top", "width": "0.5mm",
               "points": [{"x": 0, "y": 0}, {"x": "3mm", "y": 0}, {"x": "3mm", "y": "4mm"}]}),
    );
    let tracks = out["result"]["tracks"].clone();
    run(
        &mut b.shell,
        "pcb.measure.length",
        json!({ "objects": tracks }),
    );
    let rows: Vec<String> = b
        .shell
        .messages()
        .notes()
        .iter()
        .filter(|n| n.source == reda_ui::pcb::measure::LENGTH_SOURCE)
        .map(|n| n.text.clone())
        .collect();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(
        rows[0].contains("7 mm") && rows[0].contains("2"),
        "{rows:?}"
    );
}
