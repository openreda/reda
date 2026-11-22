//! Save Board as Template: `library.board_template.save` captures the active board as a
//! new `.redaboard` (`docs/spec/v0.2.5/save-as-template.md`, `testing.md#save-board-as-template`).

use std::path::{Path, PathBuf};

use reda_engine::Engine;
use reda_engine::command::{ErrorCode, FixedClock, PartSource, SeededIds, Session, UserDirs};
use reda_engine::format::library::write_library_info;
use reda_engine::format::{read_board_template, write_board_template};
use reda_engine::library::board_template::{BoardTemplateChoice, Frame, evaluate};
use reda_engine::library::{DiskLibraryFiles, DiskParts, LibraryOrigin, LibraryRoot, scan};
use reda_engine::model::{
    BoardTemplate, DrawingKind, FootprintCopy, LibraryInfo, Side, TemplateCopperLayer,
    TemplateNumber, TemplatePart, Uuid,
};
use reda_testkit::{TempDir, TestEngine, starter_library};
use serde_json::{Value, json};

const HEADER_1X02: &str = "d0560bc9-5203-4a8b-b575-24058968bdd8";
const MOUNTING_HOLE_M3: &str = "326ace02-ef44-4af4-8f9d-21d6c65a74ef";
const STARTER_LIBRARY: &str = "5a7e0000-0000-4000-8000-000000000001";

fn id(n: u128) -> Uuid {
    Uuid::v4_from_bits(n)
}

fn s(n: u128) -> String {
    id(n).to_string()
}

fn mm(n: f64) -> i64 {
    (n * 1e6).round() as i64
}

/// A starter Part as a template part-table entry `entry`, its copies numbered after it.
fn copied_part(part: &str, entry: u128) -> (TemplatePart, Vec<FootprintCopy>) {
    let session = Session {
        starter_library: Some(starter_library()),
        ..Session::default()
    };
    let copy = DiskParts::new(&session)
        .part(part.parse().unwrap())
        .expect("a starter Part");
    let copies: Vec<FootprintCopy> = copy
        .footprints
        .iter()
        .enumerate()
        .map(|(i, f)| FootprintCopy {
            uuid: id(entry + 1 + i as u128),
            hash: f.hash.clone(),
            edited_from: None,
            footprint: f.footprint.clone(),
        })
        .collect();
    let entry = TemplatePart {
        uuid: id(entry),
        hash: copy.hash,
        part: copy.part,
        symbol: copy.symbol,
        footprints: copies.iter().map(|c| c.uuid).collect(),
    };
    (entry, copies)
}

/// A 4-layer fixture board whose outline starts at (10, 20) mm: a plated bottom edge, a
/// cutout, a top and an all-copper Keepout, a Locked header on the bottom and an unlocked
/// one, a Locked M3 hole, a Locked and an unlocked Free Pad, a Dimension from an outline
/// vertex to the Locked Free Pad, a free text on Documentation and an edge connector.
fn fixture() -> BoardTemplate {
    let mut t = BoardTemplate::new(id(100), "Offset board");
    t.suggested_layers = 4;
    t.board = serde_json::from_value(
        json!({"edge_connector": {"bevel": {"angle": "20deg", "depth": "1.3"}, "hard_gold": true}}),
    )
    .unwrap();
    t.outline = serde_json::from_value(json!([{"uuid": s(101), "locked": true, "vertices": [
        {"x":"10","y":"20","plated":true},
        {"x":"70","y":"20"},
        {"x":"70","y":"60"},
        {"x":"10","y":"60"}
    ]}]))
    .unwrap();
    t.cutouts = serde_json::from_value(json!([{"uuid": s(102), "vertices": [
        {"x":"30","y":"30"},{"x":"35","y":"30"},{"x":"35","y":"35","sweep":"90"},{"x":"30","y":"35"}
    ]}]))
    .unwrap();
    let (header, header_copies) = copied_part(HEADER_1X02, 200);
    let (hole, hole_copies) = copied_part(MOUNTING_HOLE_M3, 300);
    t.parts = vec![header, hole];
    t.footprint_copies = header_copies.into_iter().chain(hole_copies).collect();
    t.connectors = serde_json::from_value(json!([
        {"uuid": s(110), "part": s(200), "footprint": s(201),
         "at": {"x":"20","y":"40"}, "rotation":"90", "side":"bottom", "locked": true},
        {"uuid": s(111), "part": s(200), "footprint": s(201),
         "at": {"x":"50","y":"25"}, "rotation":0, "side":"top"}
    ]))
    .unwrap();
    t.holes = serde_json::from_value(json!([
        {"uuid": s(120), "part": s(300), "footprint": s(301), "side": "top",
         "at": {"x":"65","y":"55"}, "locked": true},
        {"uuid": s(121), "at": {"x":"15","y":"25"}, "locked": true,
         "pad": {"kind":"non_plated","shape":"circle","width":"3.2","height":"3.2",
                 "hole":{"width":"3.2","height":"3.2"}}},
        {"uuid": s(122), "at": {"x":"15","y":"55"},
         "pad": {"kind":"non_plated","shape":"circle","width":"2","height":"2",
                 "hole":{"width":"2","height":"2"}}}
    ]))
    .unwrap();
    t.keepouts = serde_json::from_value(json!([
        {"uuid": s(130), "layer":"top_copper", "restrict":["tracks","vias"], "shape":"polygon",
         "protects": s(120), "locked": true, "vertices": [
            {"x":"61","y":"51"},{"x":"69","y":"51"},{"x":"69","y":"59"},{"x":"61","y":"59"}
        ]},
        {"uuid": s(131), "layer":"all_copper", "restrict":["zones"], "shape":"line",
         "width": "1", "vertices": [{"x":"40","y":"40"},{"x":"45","y":"45"}]}
    ]))
    .unwrap();
    t.drawing_groups = serde_json::from_value(json!([
        {"uuid": s(140), "name": "Key dimensions", "layer": "mechanical"},
        {"uuid": s(141), "name": "Notes", "layer": "documentation"}
    ]))
    .unwrap();
    t.drawings = serde_json::from_value(json!([
        {"uuid": s(150), "group": s(140), "kind": "dimension", "locked": true,
         "a": {"follow": {"object": s(101), "vertex": 3}}, "b": {"follow": {"object": s(121)}},
         "offset": "-3", "suffix": " mm"},
        {"uuid": s(151), "group": s(140), "kind": "dimension",
         "a": {"follow": {"object": s(122)}}, "b": {"x": "10", "y": "20"}},
        {"uuid": s(152), "group": s(141), "kind": "text", "text": "HOST",
         "position": {"x":"20","y":"15"}, "height": "2", "rotation": "90"},
        {"uuid": s(153), "group": s(141), "kind": "line", "locked": true, "width": "0.1",
         "from": {"x":"10","y":"18"}, "to": {"x":"70","y":"18"}}
    ]))
    .unwrap();
    t
}

/// A Library in `folder` holding `templates`; its UUID is `n << 100`.
fn write_library(folder: &Path, n: u128, templates: &[(&str, &BoardTemplate)]) -> Uuid {
    std::fs::create_dir_all(folder.join("board-templates")).unwrap();
    let info = LibraryInfo {
        uuid: id(n << 100),
        name: format!("Lib {n}"),
        description: String::new(),
    };
    std::fs::write(
        folder.join(format!("lib{n}.redalib")),
        write_library_info(&info).unwrap(),
    )
    .unwrap();
    for (stem, template) in templates {
        std::fs::write(
            folder.join(format!("board-templates/{stem}.redaboard")),
            write_board_template(template).unwrap(),
        )
        .unwrap();
    }
    info.uuid
}

/// An engine with the starter Library and two user Libraries: `<work>/fixtures` holding
/// the fixture, and the empty `<work>/mine` templates are saved into.
struct Setup {
    engine: TestEngine,
    mine: Uuid,
    mine_folder: PathBuf,
    _home: TempDir,
    work: TempDir,
}

fn setup(name: &str) -> Setup {
    let home = TempDir::new(&format!("{name}-home"));
    let work = TempDir::new(&format!("{name}-work"));
    let fixtures = work.path().join("fixtures");
    write_library(&fixtures, 1, &[("Offset", &fixture())]);
    let mine_folder = work.path().join("mine");
    let mine = write_library(&mine_folder, 2, &[]);
    let mut engine = TestEngine(
        Engine::builder()
            .ids(SeededIds::new(0))
            .clock(FixedClock::EPOCH)
            .user_dirs(Some(UserDirs::under(home.path())))
            .starter_library(Some(starter_library()))
            .commands(reda_testkit::probe_commands)
            .build(),
    );
    engine.run_ok(
        "app.preferences.set",
        json!({"key": "libraries.paths", "value": [fixtures, mine_folder]}),
    );
    Setup {
        engine,
        mine,
        mine_folder,
        _home: home,
        work,
    }
}

impl Setup {
    /// A new Project `name` made from `choice`.
    fn create(&mut self, name: &str, choice: Value) -> Value {
        self.engine.run_ok(
            "project.new",
            json!({"path": self.work.path(), "name": name, "board_template": choice}),
        )
    }

    fn save(&mut self, name: &str, extra: Value) -> Value {
        let mut params = json!({"library": self.mine, "name": name});
        for (k, v) in extra.as_object().unwrap() {
            params[k] = v.clone();
        }
        self.engine.run_ok("library.board_template.save", params)
    }

    fn saved(&self, name: &str) -> BoardTemplate {
        let path = self
            .mine_folder
            .join(format!("board-templates/{name}.redaboard"));
        read_board_template(&path, &std::fs::read(&path).unwrap()).unwrap()
    }
}

fn int(n: &TemplateNumber) -> i64 {
    match n {
        TemplateNumber::Int(n) => *n,
        TemplateNumber::Expr(e) => panic!("{e} is an expression, not a literal"),
    }
}

#[test]
fn saving_captures_the_defaults_shifted_to_the_origin_as_literals() {
    let mut b = setup("save-defaults");
    b.create("Offset", json!({"element": s(100)}));
    let out = b.save(
        "Saved board",
        json!({"description": "Mine", "category": "Custom"}),
    );
    assert_eq!(out["warnings"], json!([]), "{out}");
    let path = PathBuf::from(out["path"].as_str().unwrap());
    assert_eq!(
        path,
        b.mine_folder.join("board-templates/Saved board.redaboard")
    );
    let t = b.saved("Saved board");
    assert_eq!(t.uuid.to_string(), out["element"].as_str().unwrap());
    assert_eq!(
        (t.description.as_str(), t.category.as_deref()),
        ("Mine", Some("Custom"))
    );
    assert_eq!(t.suggested_layers, 4);
    assert!(t.parameters.is_empty());

    // The outline: shifted so (10, 20) is the origin, plated mark kept, Locked.
    let outline = &t.outline[0];
    assert!(outline.locked);
    let points: Vec<(i64, i64, bool)> = outline
        .vertices
        .iter()
        .map(|v| (int(&v.x), int(&v.y), v.plated))
        .collect();
    assert_eq!(
        points,
        [
            (0, 0, true),
            (mm(60.0), 0, false),
            (mm(60.0), mm(40.0), false),
            (0, mm(40.0), false)
        ]
    );
    // The cutout keeps its arc and its unlocked state.
    let cutout = &t.cutouts[0];
    assert!(!cutout.locked);
    assert_eq!(int(&cutout.vertices[0].x), mm(20.0));
    assert_eq!(cutout.vertices[2].sweep.as_ref().map(int), Some(90_000));

    // Keepouts: symbolic Layers, no ties.
    let mut layers: Vec<TemplateCopperLayer> = t.keepouts.iter().map(|k| k.layer).collect();
    layers.sort_by_key(|l| format!("{l:?}"));
    assert_eq!(
        layers,
        [
            TemplateCopperLayer::AllCopper,
            TemplateCopperLayer::TopCopper
        ]
    );
    assert!(t.keepouts.iter().all(|k| k.protects.is_none()));
    let line = t.keepouts.iter().find(|k| k.width.is_some()).unwrap();
    assert_eq!(line.width.as_ref().map(int), Some(mm(1.0)));
    assert!(!line.locked);

    // Only the Locked header; its Part copy unedited, with its element UUID and hash.
    assert_eq!(t.connectors.len(), 1, "{:?}", t.connectors);
    let j1 = &t.connectors[0];
    assert_eq!((int(&j1.at.x), int(&j1.at.y)), (mm(10.0), mm(20.0)));
    assert_eq!(int(&j1.rotation), 90_000);
    assert_eq!(j1.side, Side::Bottom);
    assert!(j1.locked);
    let (header, _) = copied_part(HEADER_1X02, 0);
    let entry = t.parts.iter().find(|p| p.uuid == j1.part).unwrap();
    assert_eq!(entry.part.uuid, header.part.uuid);
    assert_eq!(entry.hash, header.hash);
    let copy = t
        .footprint_copies
        .iter()
        .find(|c| c.uuid == j1.footprint)
        .unwrap();
    assert_eq!(
        copy.footprint.uuid.to_string(),
        header.part.footprints[0].to_string()
    );

    // The Locked M3 Mechanical Part and Locked Free Pad; not the unlocked Free Pad.
    assert_eq!(t.holes.len(), 2, "{:?}", t.holes);
    let m3 = t.holes.iter().find(|h| h.part.is_some()).unwrap();
    assert_eq!((int(&m3.at.x), int(&m3.at.y)), (mm(55.0), mm(35.0)));
    let (hole, _) = copied_part(MOUNTING_HOLE_M3, 0);
    let entry = t.parts.iter().find(|p| Some(p.uuid) == m3.part).unwrap();
    assert_eq!((entry.part.uuid, &entry.hash), (hole.part.uuid, &hole.hash));
    let pad = t.holes.iter().find(|h| h.pad.is_some()).unwrap();
    assert_eq!((int(&pad.at.x), int(&pad.at.y)), (mm(5.0), mm(5.0)));
    assert_eq!(int(&pad.pad.as_ref().unwrap().width), mm(3.2));

    // Two drawing groups named after their Layer; a Dimension keeps the reference whose
    // target came along, and the end on the unlocked Free Pad becomes free.
    let mut groups: Vec<&str> = t.drawing_groups.iter().map(|g| g.name.as_str()).collect();
    groups.sort();
    assert_eq!(groups, ["Documentation drawings", "Mechanical drawings"]);
    assert_eq!(t.drawings.len(), 4);
    let followed = t
        .drawings
        .iter()
        .find(|d| d.suffix.as_deref() == Some(" mm"))
        .unwrap();
    let a = followed.a.as_ref().unwrap().follow.unwrap();
    assert_eq!((a.object, a.vertex), (outline.uuid, Some(3)));
    let to = followed.b.as_ref().unwrap().follow.unwrap();
    assert_eq!(to.object, pad.uuid);
    assert_eq!(followed.offset.as_ref().map(int), Some(mm(-3.0)));
    assert!(followed.locked);
    let free = t
        .drawings
        .iter()
        .find(|d| d.kind == DrawingKind::Dimension && d.suffix.is_none())
        .unwrap();
    let end = free.a.as_ref().unwrap();
    assert!(end.follow.is_none());
    assert_eq!(
        (end.x.as_ref().map(int), end.y.as_ref().map(int)),
        (Some(mm(5.0)), Some(mm(35.0)))
    );
    let text = t.drawings.iter().find(|d| d.text.is_some()).unwrap();
    assert_eq!(text.rotation.as_ref().map(int), Some(90_000));
    assert!(!text.locked);

    // The edge connector property comes along.
    let edge = t.board.as_ref().unwrap().edge_connector.as_ref().unwrap();
    assert!(edge.hard_gold);
    assert_eq!(edge.bevel.as_ref().map(|b| int(&b.depth)), Some(mm(1.3)));

    // Fresh UUIDs: none of the fixture's or the board's.
    let file = std::fs::read_to_string(&path).unwrap();
    for n in [100, 101, 102, 110, 120, 121, 130, 140, 150] {
        assert!(!file.contains(&s(n)), "{} is reused", s(n));
    }
    let board = b.engine.project().unwrap().pcb().unwrap();
    assert!(!file.contains(&board.outline().unwrap().uuid.to_string()));

    // The scan offers it as a user template.
    let listed = b.engine.run_ok("library.board_template.list", json!({}));
    let found = listed["templates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "Saved board")
        .unwrap_or_else(|| panic!("{listed}"));
    assert_eq!(found["origin"], "user");
}

#[test]
fn objects_names_exactly_what_comes_along() {
    let mut b = setup("save-objects");
    b.create("Offset", json!({"element": s(100)}));
    let pcb = b.engine.project().unwrap().pcb().unwrap().clone();
    let unlocked_pad = pcb.free_pads().find(|p| !p.locked).map(|p| p.uuid).unwrap();
    let documentation = pcb
        .layers()
        .layers
        .iter()
        .find(|l| l.kind == reda_engine::model::LayerKind::Documentation)
        .unwrap()
        .uuid;
    b.save(
        "Picked",
        json!({"objects": [unlocked_pad, documentation, pcb.outline().unwrap().uuid]}),
    );
    let t = b.saved("Picked");
    assert!(t.cutouts.is_empty() && t.keepouts.is_empty() && t.connectors.is_empty());
    assert_eq!(t.holes.len(), 1);
    assert!(!t.holes[0].locked);
    assert_eq!(t.drawing_groups.len(), 1);
    assert_eq!(t.drawing_groups[0].name, "Documentation drawings");
    assert_eq!(t.drawings.len(), 2);
}

#[test]
fn refusals_write_nothing() {
    let mut b = setup("save-refusals");
    // A board with no outline.
    b.engine.run_ok(
        "project.new",
        json!({"path": b.work.path(), "name": "Bare"}),
    );
    let err = b.engine.run_err(
        "library.board_template.save",
        json!({"library": b.mine, "name": "Bare"}),
    );
    assert_eq!(err.code, ErrorCode::Rejected, "{err:?}");
    assert!(err.message.contains("no outline"), "{err:?}");

    b.create("Offset", json!({"element": s(100)}));
    // An inner-layer Keepout is listed but refused.
    let inner = b.engine.run_ok(
        "pcb.keepout.add",
        json!({"layer": "copper_l2", "vertices": [
            {"x":"40mm","y":"50mm"},{"x":"45mm","y":"50mm"},{"x":"45mm","y":"55mm"}]}),
    );
    let inner = inner["result"]["uuid"].clone();
    let err = b.engine.run_err(
        "library.board_template.save",
        json!({"library": b.mine, "name": "Inner", "objects": [inner]}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams, "{err:?}");
    assert!(err.message.contains("inner-layer Keepouts"), "{err:?}");
    // The defaults leave it out.
    let t = {
        b.save("Defaults", json!({}));
        b.saved("Defaults")
    };
    assert_eq!(t.keepouts.len(), 2);

    // A name the Library already has.
    let err = b.engine.run_err(
        "library.board_template.save",
        json!({"library": b.mine, "name": "Defaults"}),
    );
    assert_eq!(err.code, ErrorCode::Rejected, "{err:?}");
    assert!(err.message.contains("already has"), "{err:?}");

    // The Starter Library.
    let err = b.engine.run_err(
        "library.board_template.save",
        json!({"library": STARTER_LIBRARY, "name": "Starter"}),
    );
    assert_eq!(err.code, ErrorCode::Rejected, "{err:?}");
    assert!(err.message.contains("read-only"), "{err:?}");

    // A read-only Library folder.
    let folder = b.mine_folder.join("board-templates");
    set_read_only(&folder, true);
    let err = b.engine.run_err(
        "library.board_template.save",
        json!({"library": b.mine, "name": "Locked away"}),
    );
    set_read_only(&folder, false);
    assert_eq!(err.code, ErrorCode::Rejected, "{err:?}");
    assert!(err.message.contains("read-only"), "{err:?}");
    assert!(!folder.join("Locked away.redaboard").exists());
    assert!(!folder.join("Inner.redaboard").exists());
}

fn set_read_only(folder: &Path, on: bool) {
    let mut permissions = std::fs::metadata(folder).unwrap().permissions();
    permissions.set_readonly(on);
    std::fs::set_permissions(folder, permissions).unwrap();
}

#[test]
fn local_pad_edits_stay_on_the_board_and_warn() {
    let mut b = setup("save-local-edits");
    b.create("Offset", json!({"element": s(100)}));
    let pcb = b.engine.project().unwrap().pcb().unwrap().clone();
    let j1 = pcb.footprints().find(|f| f.designator == "J1").unwrap();
    let pad = pcb.footprint_copy(j1.footprint).unwrap().footprint.pads[0].uuid;
    b.engine.run_ok(
        "pcb.pad.edit",
        json!({"instance": j1.uuid, "pad": pad, "width": "2.5mm"}),
    );
    let out = b.save("Edited", json!({}));
    assert_eq!(
        out["warnings"],
        json!(["J1 has local Pad edits that won't be saved"])
    );
    let t = b.saved("Edited");
    let (header, copies) = copied_part(HEADER_1X02, 0);
    let copy = t
        .footprint_copies
        .iter()
        .find(|c| c.uuid == t.connectors[0].footprint)
        .unwrap();
    assert_eq!(copy.hash, copies[0].hash);
    assert_eq!(copy.footprint, copies[0].footprint);
    assert_eq!(t.parts[0].hash.len(), header.hash.len());
}

/// `text` with every UUID replaced by `U`.
fn without_uuids(text: &str) -> String {
    let bytes = text.as_bytes();
    let shape = |b: &[u8]| {
        b.len() >= 36
            && b[..36].iter().enumerate().all(|(i, c)| match i {
                8 | 13 | 18 | 23 => *c == b'-',
                _ => c.is_ascii_hexdigit(),
            })
    };
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < bytes.len() {
        if shape(&bytes[i..]) {
            out.push('U');
            i += 36;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

/// A saved file's lines with UUIDs left out, sorted: equal for two Documents that differ
/// only in UUIDs (and the order UUIDs give their tables).
fn content(path: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut lines: Vec<String> = text.lines().map(without_uuids).collect();
    lines.sort();
    lines
}

/// The rows of `a` that `b` lacks.
fn only(a: &[String], b: &[String]) -> Vec<String> {
    a.iter().filter(|r| !b.contains(r)).cloned().collect()
}

fn value<T: serde::Serialize>(x: &T) -> Value {
    serde_json::to_value(x).unwrap()
}

/// The frame as the board gets it, with template-local UUIDs, ties and grouping left out:
/// what a saved template must give back at the values it was saved at.
fn comparable(frame: &Frame) -> Vec<String> {
    let mut rows = vec![];
    let mut push = |kind: &str, mut v: Value| {
        if let Some(o) = v.as_object_mut() {
            o.remove("protects");
            o.remove("excluded");
        }
        rows.push(format!("{kind} {}", without_uuids(&v.to_string())));
    };
    push("outline", value(&frame.outline));
    for c in &frame.cutouts {
        push("cutout", value(c));
    }
    for k in &frame.keepouts {
        push("keepout", value(k));
    }
    for c in &frame.connectors {
        push("connector", value(c));
    }
    for h in &frame.holes {
        push("hole", value(h));
    }
    for g in &frame.drawing_groups {
        for d in &g.drawings {
            let mut row = value(d);
            // A Dimension with no rotation measures along a → b, as creation makes it.
            if let (Some(a), Some(b), None) = (d.a, d.b, d.rotation) {
                let (dx, dy) = ((b.point.x - a.point.x).0, (b.point.y - a.point.y).0);
                let deg = (dy as f64).atan2(dx as f64).to_degrees();
                row["rotation"] = json!(((deg * 1000.0).round() as i64).rem_euclid(360_000));
            }
            push(&format!("drawing {:?}", g.layer), row);
        }
    }
    push("board", value(&frame.board));
    push("layers", json!(frame.suggested_layers));
    rows.sort();
    rows
}

/// The default-values frame of `template`.
fn frame_of(template: &BoardTemplate) -> Frame {
    evaluate(
        template,
        &BoardTemplateChoice {
            element: template.uuid,
            ..BoardTemplateChoice::default()
        },
    )
    .unwrap()
}

/// The one file with extension `ext` in `dir`.
fn file_in(dir: &Path, ext: &str) -> PathBuf {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|f| f.extension().is_some_and(|e| e == ext))
        .unwrap_or_else(|| panic!("no .{ext} in {}", dir.display()))
}

impl Setup {
    /// Saves the open Project `first`'s board as template `saved`, creates a Project from
    /// that template, and checks its board and sheet equal those of Project `like` apart
    /// from UUIDs. Returns the saved template.
    fn round_trip(&mut self, label: &str, first: &str, saved: &str, like: &str) -> BoardTemplate {
        self.engine.run_ok("project.save", json!({}));
        let out = self.save(saved, json!({}));
        assert_eq!(out["warnings"], json!([]), "{label}: {out}");
        let second = format!("{first}-again");
        self.create(&second, json!({"element": out["element"]}));
        self.engine.run_ok("project.save", json!({}));
        for ext in ["redapcb", "redasch"] {
            let a = file_in(&self.work.path().join(like), ext);
            let b = file_in(&self.work.path().join(&second), ext);
            let (a, b) = (content(&a), content(&b));
            assert_eq!(
                (only(&a, &b), only(&b, &a)),
                (vec![], vec![]),
                "{label}: the .{ext} files differ (first only, second only)"
            );
            assert_eq!(a.len(), b.len(), "{label}: .{ext}");
        }
        self.saved(saved)
    }
}

#[test]
fn every_built_in_survives_save_then_create() {
    let mut b = setup("save-round-trip");
    let starter = scan(
        &[LibraryRoot::new(starter_library(), LibraryOrigin::Starter)],
        &mut DiskLibraryFiles,
    );
    let built_ins: Vec<&BoardTemplate> = starter.board_templates().map(|(_, t)| t).collect();
    assert!(built_ins.len() >= 9, "{} built-ins", built_ins.len());
    for (i, built_in) in built_ins.into_iter().enumerate() {
        let first = format!("First{i}");
        b.create(&first, json!({"element": built_in.uuid}));
        let saved = b.round_trip(&built_in.name, &first, &format!("Saved {i}"), &first);
        // The saved frame is the built-in's at its defaults, as literals.
        let (got, want) = (
            comparable(&frame_of(&saved)),
            comparable(&frame_of(built_in)),
        );
        assert_eq!(
            (only(&got, &want), only(&want, &got)),
            (vec![], vec![]),
            "{}: the saved frame differs (saved only, built-in only)",
            built_in.name
        );
        assert_eq!(got.len(), want.len(), "{}", built_in.name);
    }
}

/// A random frame of literals: an outline anywhere, cutouts, Keepouts, Locked Free Pad
/// and Mechanical Part holes, and Locked Components, all inside the outline.
#[derive(Debug, Clone)]
struct RandomFrame {
    at: (i64, i64),
    size: (i64, i64),
    cutouts: Vec<(i64, i64)>,
    keepouts: Vec<(u8, i64, i64)>,
    pads: Vec<(i64, i64)>,
    holes: Vec<(i64, i64, u8)>,
    components: Vec<(i64, i64, u8, bool)>,
}

fn random_frame() -> impl proptest::strategy::Strategy<Value = RandomFrame> {
    use proptest::prelude::*;
    // Fractions of the board, in thousandths, so every object lands inside it.
    let spot = || (100i64..900, 100i64..900);
    (
        (-50_000i64..50_000, -50_000i64..50_000),
        (30_000i64..120_000, 30_000i64..120_000),
        proptest::collection::vec(spot(), 0..3),
        proptest::collection::vec((0u8..3, 100i64..900, 100i64..900), 0..3),
        proptest::collection::vec(spot(), 0..3),
        proptest::collection::vec((100i64..900, 100i64..900, 0u8..4), 0..3),
        proptest::collection::vec((100i64..900, 100i64..900, 0u8..4, any::<bool>()), 0..3),
    )
        .prop_map(
            |(at, size, cutouts, keepouts, pads, holes, components)| RandomFrame {
                at: (at.0 * 1000, at.1 * 1000),
                size: (size.0 * 1000, size.1 * 1000),
                cutouts,
                keepouts,
                pads,
                holes,
                components,
            },
        )
}

impl RandomFrame {
    /// The point `f` thousandths across the board.
    fn point(&self, (fx, fy): (i64, i64)) -> (i64, i64) {
        (
            self.at.0 + self.size.0 * fx / 1000,
            self.at.1 + self.size.1 * fy / 1000,
        )
    }

    fn square(&self, f: (i64, i64), half: i64) -> Value {
        let (x, y) = self.point(f);
        json!([
            {"x": x - half, "y": y - half}, {"x": x + half, "y": y - half},
            {"x": x + half, "y": y + half}, {"x": x - half, "y": y + half}
        ])
    }

    fn template(&self, uuid: Uuid) -> BoardTemplate {
        let mut t = BoardTemplate::new(uuid, format!("Random {uuid}"));
        let (x0, y0) = self.at;
        let (x1, y1) = (x0 + self.size.0, y0 + self.size.1);
        t.outline = serde_json::from_value(json!([{"uuid": s(1), "locked": true, "vertices": [
            {"x": x0, "y": y0}, {"x": x1, "y": y0}, {"x": x1, "y": y1}, {"x": x0, "y": y1}
        ]}]))
        .unwrap();
        // Cutouts sit along the bottom band and Keepouts along the top, apart.
        t.cutouts = (self.cutouts.iter().enumerate())
            .map(|(i, &(fx, _))| {
                let f = (100 + 300 * i as i64 + fx / 10, 200);
                serde_json::from_value(json!({"uuid": s(10 + i as u128), "locked": true,
                    "vertices": self.square(f, 1_000_000)}))
                .unwrap()
            })
            .collect();
        t.keepouts = (self.keepouts.iter().enumerate())
            .map(|(i, &(layer, fx, _))| {
                let f = (100 + 300 * i as i64 + fx / 10, 800);
                let layer = ["all_copper", "top_copper", "bottom_copper"][layer as usize];
                serde_json::from_value(json!({"uuid": s(20 + i as u128), "layer": layer,
                    "restrict": ["tracks", "vias"], "shape": "polygon", "locked": true,
                    "vertices": self.square(f, 1_000_000)}))
                .unwrap()
            })
            .collect();
        let (header, header_copies) = copied_part(HEADER_1X02, 200);
        let (hole, hole_copies) = copied_part(MOUNTING_HOLE_M3, 300);
        t.parts = vec![header, hole];
        t.footprint_copies = header_copies.into_iter().chain(hole_copies).collect();
        let mut holes = vec![];
        for (i, f) in self.pads.iter().enumerate() {
            let (x, y) = self.point(*f);
            holes.push(
                json!({"uuid": s(30 + i as u128), "at": {"x": x, "y": y}, "locked": true,
                "pad": {"kind": "non_plated", "shape": "circle", "width": 2_000_000,
                        "height": 2_000_000, "hole": {"width": 2_000_000, "height": 2_000_000}}}),
            );
        }
        for (i, &(fx, fy, turn)) in self.holes.iter().enumerate() {
            let (x, y) = self.point((fx, fy));
            holes.push(
                json!({"uuid": s(40 + i as u128), "part": s(300), "footprint": s(301),
                "side": "top", "at": {"x": x, "y": y}, "rotation": 90_000 * turn as i64,
                "locked": true}),
            );
        }
        t.holes = serde_json::from_value(Value::Array(holes)).unwrap();
        t.connectors = (self.components.iter().enumerate())
            .map(|(i, &(fx, fy, turn, bottom))| {
                let (x, y) = self.point((fx, fy));
                serde_json::from_value(json!({"uuid": s(50 + i as u128), "part": s(200),
                    "footprint": s(201), "at": {"x": x, "y": y},
                    "rotation": 90_000 * turn as i64,
                    "side": if bottom { "bottom" } else { "top" }, "locked": true}))
                .unwrap()
            })
            .collect();
        t
    }
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig {
        cases: 16,
        failure_persistence: None,
        ..proptest::prelude::ProptestConfig::default()
    })]

    #[test]
    fn a_random_frame_survives_save_then_create(frame in random_frame()) {
        let mut b = setup("save-random");
        // The same frame shifted to the origin is what saving must give back.
        let mut shifted = frame.clone();
        shifted.at = (0, 0);
        let (random, shifted) = (frame.template(id(900)), shifted.template(id(901)));
        write_library(
            &b.work.path().join("fixtures"),
            1,
            &[("Random", &random), ("Shifted", &shifted)],
        );
        b.create("Shifted", json!({"element": s(901)}));
        b.engine.run_ok("project.save", json!({}));
        b.create("Random", json!({"element": s(900)}));
        let saved = b.round_trip("random frame", "Random", "Saved random", "Shifted");
        let (got, want) = (comparable(&frame_of(&saved)), comparable(&frame_of(&shifted)));
        proptest::prop_assert_eq!(got, want);
    }
}
