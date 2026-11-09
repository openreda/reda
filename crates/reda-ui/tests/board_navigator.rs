//! The Board navigator Panel (docs/spec/v0.1/app.md#board-navigator), driven through
//! AccessKit with `egui_kittest`: its Nets, Footprints and Zones modes, the search box,
//! and the Dim, Select and Zoom toggles that say what clicking a row does.

use std::path::{Path, PathBuf};

use egui::accesskit::Toggled;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::model::{Side, Uuid};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

struct Board {
    h: Harness<'static, App>,
    pcb: Uuid,
    gnd: Uuid,
    sig: Uuid,
    gnd_tracks: [Uuid; 2],
    r1: Uuid,
    r2: Uuid,
    _dir: TempDir,
    _home: TempDir,
}

fn run(shell: &mut Shell, id: &str, params: Value) -> Value {
    match shell.run(id, params.clone()) {
        Ok(out) => out,
        Err(e) => panic!("{id} {params} failed: {e}"),
    }
}

fn uuid(v: &Value) -> Uuid {
    v.as_str().unwrap().parse().unwrap()
}

/// Nets GND (two Tracks, 4 mm and 6 mm) and SIG (one 3 mm Track); Footprints R1 on top
/// and R2 on the bottom; the board open and the Board navigator shown.
fn board() -> Board {
    let dir = TempDir::new("board-navigator");
    let home = TempDir::new("board-navigator-home");
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(library()))
        .commands(reda_testkit::probe_commands)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(dir.path(), "Nav").unwrap();
    run(
        &mut shell,
        "app.preferences.set",
        json!({"key": "pcb.zone.auto_refill", "value": "off"}),
    );
    let net = |shell: &mut Shell, name: &str| {
        uuid(&run(shell, "test.net.add", json!({ "name": name }))["result"]["uuid"])
    };
    let gnd = net(&mut shell, "GND");
    let sig = net(&mut shell, "SIG");
    let track = |shell: &mut Shell, net: Uuid, y: &str, length: &str| {
        let out = run(
            shell,
            "pcb.track.add",
            json!({"layer": "copper_top", "net": net, "width": "0.5mm",
                   "points": [{"x": 0, "y": y}, {"x": length, "y": y}]}),
        );
        uuid(&out["result"]["tracks"][0])
    };
    let gnd_tracks = [
        track(&mut shell, gnd, "0mm", "4mm"),
        track(&mut shell, gnd, "2mm", "6mm"),
    ];
    track(&mut shell, sig, "4mm", "3mm");
    let footprint = |shell: &mut Shell, designator: &str, side: &str, x: &str| {
        let out = run(
            shell,
            "test.footprint.add",
            json!({"at": {"x": x, "y": "10mm"}, "side": side, "designator": designator,
                   "pads": [{"at": {"x": 0, "y": 0}, "width": "1mm", "height": "1mm"}]}),
        );
        uuid(&out["result"]["footprint"])
    };
    let r1 = footprint(&mut shell, "R1", "top", "0mm");
    let r2 = footprint(&mut shell, "R2", "bottom", "5mm");
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    run(&mut shell, "document.open", json!({ "document": pcb }));
    run(&mut shell, "panel.show:navigator", json!({}));
    let mut h = Harness::builder()
        .with_size([1400.0, 900.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    Board {
        h,
        pcb,
        gnd,
        sig,
        gnd_tracks,
        r1,
        r2,
        _dir: dir,
        _home: home,
    }
}

impl Board {
    fn shell(&mut self) -> &mut Shell {
        self.h.state_mut().shell_mut()
    }

    fn click(&mut self, label: &str) {
        self.h.get_by_label(label).click();
        self.h.run();
    }

    fn has(&self, label: &str) -> bool {
        self.h.query_by_label(label).is_some()
    }

    fn checked(&self, label: &str) -> bool {
        self.h.get_by_label(label).accesskit_node().toggled() == Some(Toggled::True)
    }

    /// The last Command run, as `(id, params)`, past the selection and tab changes the
    /// window makes on its own after an edit.
    fn last(&self) -> (String, Value) {
        let journal = self.h.state().shell().engine().journal();
        let entry = journal
            .iter()
            .rfind(|e| e.id != "selection.set" && !e.id.starts_with("layout."))
            .unwrap();
        (entry.id.clone(), entry.params.clone())
    }

    fn mode(&mut self, mode: &str) {
        self.click(&format!("Navigator mode: {mode}"));
    }
}

#[test]
fn nets_mode_shows_each_nets_ratsnest_colour_name_track_count_and_length() {
    let mut b = board();
    assert!(b.has("Board navigator"));
    for (net, tracks, length) in [("GND", "2", "10 mm"), ("SIG", "1", "3 mm")] {
        assert!(b.has(&format!("Net {net}")), "{net}");
        assert!(b.has(&format!("Tracks of {net}: {tracks}")), "{net}");
        assert!(b.has(&format!("Track length of {net}: {length}")), "{net}");
        assert!(b.checked(&format!("Show Ratsnest of {net}")), "{net}");
        assert!(b.has(&format!("Net colour of {net}: none")), "{net}");
    }

    b.click("Show Ratsnest of GND");
    let gnd = b.gnd;
    assert_eq!(
        b.last(),
        (
            "pcb.ratsnest.show".to_owned(),
            json!({"net": gnd, "visible": false})
        )
    );
    assert!(!b.checked("Show Ratsnest of GND"));
    assert!(b.checked("Show Ratsnest of SIG"));

    // The swatch opens a palette; a pick sets the Net colour, and No colour unsets it.
    b.click("Net colour of SIG: none");
    b.click("Colour #e6194b");
    let sig = b.sig;
    assert_eq!(
        b.last(),
        (
            "pcb.net.set_color".to_owned(),
            json!({"nets": [sig], "color": "#e6194b"})
        )
    );
    assert!(b.has("Net colour of SIG: #e6194b"));
    b.click("Net colour of SIG: #e6194b");
    b.click("No colour");
    assert_eq!(b.last().1, json!({ "nets": [sig] }));
    assert!(b.has("Net colour of SIG: none"));
    // Any other colour is typed as #rrggbb.
    b.click("Net colour of SIG: none");
    b.click("Colour of SIG as #rrggbb");
    b.h.get_by_label("Colour of SIG as #rrggbb")
        .type_text("#12AB56");
    b.h.key_press(egui::Key::Enter);
    b.h.run();
    assert_eq!(b.last().1, json!({"nets": [sig], "color": "#12ab56"}));
    assert!(b.has("Net colour of SIG: #12ab56"));
}

#[test]
fn the_search_box_filters_the_rows() {
    let mut b = board();
    b.h.get_by_label("Search the board").click();
    b.h.run();
    b.h.get_by_label("Search the board").type_text("si");
    b.h.run();
    assert!(b.has("Net SIG"));
    assert!(!b.has("Net GND"));
    b.mode("Footprints");
    assert!(!b.has("Footprint R1"), "the search applies in every mode");
}

#[test]
fn clicking_a_row_dims_selects_and_zooms_as_the_toggles_say() {
    let mut b = board();
    for toggle in ["Dim", "Select", "Zoom"] {
        assert!(b.checked(&format!("On click: {toggle}")), "{toggle}");
    }
    let camera = |b: &Board| b.h.state().shell().document_tab(b.pcb).unwrap().camera;
    let before = camera(&b);
    b.click("Net GND");
    let shell = b.h.state().shell();
    let highlight = shell.highlight().objects.clone();
    for track in b.gnd_tracks {
        assert!(
            highlight.contains(&track),
            "Dim highlights the Net's copper"
        );
        assert!(shell.selection().objects.contains(&track), "Select");
    }
    assert!(shell.selection().nets.contains(&b.gnd));
    assert_ne!(camera(&b), before, "Zoom frames the Net");

    // With only Dim on, a click highlights and leaves the selection and the view alone.
    b.click("On click: Select");
    b.click("On click: Zoom");
    assert!(!b.checked("On click: Select"));
    let r1 = b.r1;
    b.mode("Footprints");
    b.shell().run("selection.set", json!({})).unwrap();
    b.h.run();
    let before = camera(&b);
    b.click("Footprint R1");
    let shell = b.h.state().shell();
    assert_eq!(shell.highlight().objects, vec![r1]);
    assert!(shell.selection().objects.is_empty());
    assert_eq!(camera(&b), before);

    // With Dim off, a click clears the highlight and only selects.
    b.click("On click: Dim");
    b.click("On click: Select");
    let r2 = b.r2;
    b.click("Footprint R2");
    let shell = b.h.state().shell();
    assert!(shell.highlight().objects.is_empty());
    assert_eq!(shell.selection().objects, vec![r2]);
}

#[test]
fn footprints_mode_shows_designator_side_locked_and_not_fitted() {
    let mut b = board();
    let r2 = b.r2;
    b.shell()
        .run("pcb.lock", json!({"objects": [r2], "locked": true}))
        .unwrap();
    b.h.run();
    b.mode("Footprints");
    assert!(b.has("Footprint R1"));
    assert!(b.has("Side of R1: Top"));
    assert!(b.has("Side of R2: Bottom"));
    assert!(!b.checked("Locked: R1"));
    assert!(b.checked("Locked: R2"));
    assert!(!b.checked("Not Fitted: R1"));

    b.click("Not Fitted: R1");
    let r1 = b.r1;
    assert_eq!(
        b.last(),
        (
            "pcb.footprint.set".to_owned(),
            json!({"footprints": [r1], "not_fitted": true})
        )
    );
    b.click("Locked: R1");
    assert_eq!(
        b.last(),
        (
            "pcb.lock".to_owned(),
            json!({"objects": [r1], "locked": true})
        )
    );
    let shell = b.h.state().shell();
    let pcb = shell.engine().project().unwrap().pcb().unwrap();
    let r1 = pcb.footprints().find(|f| f.uuid == r1).unwrap();
    assert!(r1.not_fitted && r1.locked);
    assert_eq!(r1.side, Side::Top);
    assert!(b.checked("Not Fitted: R1"));
    assert!(b.checked("Locked: R1"));
}

/// Adds Zone `name` on Top Copper over the square `x0..x1` × `y0..y1` mm.
fn zone(b: &mut Board, name: &str, net: Uuid, (x0, x1): (i64, i64), (y0, y1): (i64, i64)) -> Uuid {
    let mm = |v: i64| format!("{v}mm");
    let vertices = json!([
        {"x": mm(x0), "y": mm(y0)}, {"x": mm(x1), "y": mm(y0)},
        {"x": mm(x1), "y": mm(y1)}, {"x": mm(x0), "y": mm(y1)},
    ]);
    let out = run(
        b.shell(),
        "pcb.zone.add",
        json!({"layer": "copper_top", "net": net, "name": name, "vertices": vertices}),
    );
    b.h.run();
    uuid(&out["result"]["uuid"])
}

/// The pour order on Top Copper, by Zone name.
fn order(b: &mut Board) -> Vec<String> {
    let out = run(b.shell(), "pcb.zone.list", json!({"layer": "copper_top"}));
    out["layers"][0]["zones"]
        .as_array()
        .unwrap()
        .iter()
        .map(|z| z["name"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn zones_mode_lists_the_pour_order_and_reorders_by_drag_and_auto_order_and_shelves() {
    let mut b = board();
    let (gnd, sig) = (b.gnd, b.sig);
    zone(&mut b, "BIG", gnd, (0, 20), (0, 20));
    let small = zone(&mut b, "SMALL", sig, (2, 4), (2, 4));
    b.mode("Zones");
    assert!(b.has("Zone BIG") && b.has("Zone SMALL"));
    assert!(b.has("Net of BIG: GND"));
    assert!(b.has("Net of SMALL: SIG"));
    assert!(b.has("Area of BIG: 400 mm²"));
    assert!(b.has("Area of SMALL: 4 mm²"));

    // Auto order puts the smallest first; each change is one undo step.
    b.click("Auto order");
    assert_eq!(b.last().0, "pcb.zone.auto_order");
    assert_eq!(order(&mut b), ["SMALL", "BIG"]);
    assert!(
        b.h.get_by_label("Zone SMALL").rect().top() < b.h.get_by_label("Zone BIG").rect().top(),
        "the rows follow the pour order"
    );

    // Dragging BIG's handle onto the top half of SMALL's row puts it first.
    let from = b.h.get_by_label("Drag BIG to reorder").rect().center();
    let target = b.h.get_by_label("Zone SMALL").rect();
    let to = egui::pos2(target.center().x, target.top() + 2.0);
    b.h.hover_at(from);
    b.h.run();
    b.h.drag_at(from);
    b.h.run();
    for step in 1..=5 {
        b.h.hover_at(from + (to - from) * (step as f32 / 5.0));
        b.h.run();
    }
    b.h.drop_at(to);
    b.h.run();
    let big_first = vec!["BIG".to_owned(), "SMALL".to_owned()];
    assert_eq!(b.last().0, "pcb.zone.reorder");
    assert_eq!(order(&mut b), big_first);
    let pcb = b.pcb;
    run(b.shell(), "edit.undo", json!({ "document": pcb }));
    assert_eq!(order(&mut b), ["SMALL", "BIG"], "one undo step");
    run(b.shell(), "edit.redo", json!({ "document": pcb }));
    assert_eq!(order(&mut b), big_first);

    // Stale marker: after a fill, changing a Zone marks it.
    run(b.shell(), "pcb.zone.fill", json!({}));
    b.h.run();
    assert!(!b.has("SMALL needs refilling"));
    run(
        b.shell(),
        "pcb.zone.set",
        json!({"zone": small, "clearance": "0.5mm"}),
    );
    b.h.run();
    assert!(b.has("SMALL needs refilling"));

    // Shelving from the row, and Restore All.
    assert!(!b.checked("Shelve SMALL"));
    assert!(!b.has("Restore All"));
    b.click("Shelve SMALL");
    assert_eq!(
        b.last(),
        ("pcb.zone.shelve".to_owned(), json!({ "zones": [small] }))
    );
    assert!(b.checked("Shelve SMALL"), "a shelved Zone stays listed");
    b.click("Restore All");
    assert_eq!(b.last(), ("pcb.zone.restore".to_owned(), json!({})));
    assert!(!b.checked("Shelve SMALL"));
}
