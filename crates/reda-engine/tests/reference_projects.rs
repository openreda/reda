//! The reference Projects under `tests/projects/`, driven end to end: ERC, DRC, the Sync
//! preview, every Output Job (the schematic PDF among its files), each checked against goldens in
//! `tests/projects/goldens/<project>/`. `REDA_BLESS=1` rewrites them.
//!
//! **Where they come from.** Each one is built by its scenario script in
//! `tests/scenarios/` (`<project>.jsonl`, whose goldens beside it are the Project as the
//! script leaves it), and the files here must equal those goldens (the `scenarios` test
//! checks it). Change a reference Project by changing its script: `REDA_BLESS=1` on the
//! `scenarios` test rewrites the goldens and copies them here, then `REDA_BLESS=1` on this
//! test rewrites the outputs made from them.
//!
//! **What they cover.** Between them the Projects hold every kind of object the output
//! goldens must show; [`between_them_the_projects_hold_every_object_the_goldens_cover`]
//! keeps it so.
//!
//! **Format bumps.** On every `format::FORMAT` bump, before the migration lands, copy each
//! reference Project as it is, still in the old format, to
//! `tests/projects/formats/v<old FORMAT>/<project>/`. Those copies are never edited again.
//! [`every_older_format_has_a_frozen_copy_that_opens_and_saves`] requires one per older
//! format and checks that each still opens through the migration chain and saves.
//!
//! **Timing.** The `#[ignore]`d `time_*` tests time the main steps on each Project; run
//! them by hand with `cargo test -p reda-engine --test reference_projects -- --ignored
//! --nocapture`. There are no performance gates in v0.1.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use reda_engine::format::FORMAT;
use reda_engine::store;
use reda_testkit::{
    TempDir, TestEngine, assert_golden, open_reference_copy, read_tree, starter_library,
    test_engine_with_library,
};
use serde_json::{Value, json};

#[path = "reference_projects/migration.rs"]
mod migration;

/// Every current reference Project, in release order: its folder and Project file.
const PROJECTS: &[(&str, &str)] = &[
    ("minimal_board", "Minimal.reda"),
    ("blinky_555", "Blinky.reda"),
    ("usb_c_mcu", "UsbMcu.reda"),
    ("four_layer", "FourLayer.reda"),
    ("six_layer_hdi", "SixLayerHdi.reda"),
    ("castellated_module", "Castellated.reda"),
    ("m2_card", "M2Card.reda"),
];

/// The three Projects shipped with format 1 and frozen for its migration tests.
const V1_PROJECTS: &[(&str, &str)] = PROJECTS.split_at(3).0;

/// Folders under `tests/projects/` that are not Projects.
const NOT_PROJECTS: &[&str] = &["goldens", "formats"];

fn projects_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/projects")
}

fn goldens(project: &str) -> PathBuf {
    projects_dir().join("goldens").join(project)
}

/// Copies `from` (every file below it) into `to`.
fn copy_tree(from: &Path, to: &Path) {
    for (rel, bytes) in read_tree(from) {
        let path = to.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
}

/// Opens a copy of the reference Project in a fresh folder, so runs never write into
/// the repo.
fn open_copy(dir: &str, file: &str) -> (TempDir, TestEngine) {
    open_reference_copy(&projects_dir().join(dir), file)
}

/// Pretty JSON with a final newline, for goldens.
fn pretty(value: &Value) -> Vec<u8> {
    let mut text = serde_json::to_string_pretty(value).unwrap();
    text.push('\n');
    text.into_bytes()
}

/// Every path string under `folder` in `value`, made relative to it, so goldens do not
/// hold the temporary folder.
fn relative(value: &mut Value, folder: &Path) {
    match value {
        Value::String(s) => {
            if let Ok(rest) = Path::new(s.as_str()).strip_prefix(folder) {
                *s = rest.to_string_lossy().replace('\\', "/");
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|v| relative(v, folder)),
        Value::Object(map) => map.values_mut().for_each(|v| relative(v, folder)),
        _ => {}
    }
}

/// Checks a folder of files against a golden folder, both ways: with `REDA_BLESS=1` it
/// rewrites the golden folder, removing stale files.
fn assert_golden_tree(golden: &Path, actual: &BTreeMap<String, Vec<u8>>) {
    if std::env::var_os("REDA_BLESS").is_some_and(|v| v == "1") {
        let _ = std::fs::remove_dir_all(golden);
    }
    for (rel, bytes) in actual {
        assert_golden(golden.join(rel), bytes);
    }
    let stale: Vec<_> = read_tree(golden)
        .into_keys()
        .filter(|rel| !actual.contains_key(rel))
        .collect();
    assert!(
        stale.is_empty(),
        "{}: stale goldens {stale:?}",
        golden.display()
    );
}

#[test]
fn every_folder_under_tests_projects_is_a_listed_reference_project() {
    let mut folders: Vec<String> = std::fs::read_dir(projects_dir())
        .unwrap()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| !NOT_PROJECTS.contains(&name.as_str()))
        .collect();
    folders.sort();
    let mut listed: Vec<String> = PROJECTS.iter().map(|(d, _)| d.to_string()).collect();
    listed.sort();
    assert_eq!(folders, listed);
}

#[test]
fn opening_and_saving_leaves_every_file_as_it_was() {
    for (dir, file) in PROJECTS {
        let (work, mut engine) = open_copy(dir, file);
        engine.run_ok("project.save", json!({}));
        let kept = read_tree(&projects_dir().join(dir));
        let saved: BTreeMap<_, _> = read_tree(work.path())
            .into_iter()
            .filter(|(rel, _)| !rel.starts_with(".reda/"))
            .collect();
        assert_eq!(
            saved.keys().collect::<Vec<_>>(),
            kept.keys().collect::<Vec<_>>(),
            "{dir}"
        );
        for (rel, bytes) in &kept {
            assert!(saved[rel] == *bytes, "{dir}: saving rewrote {rel}");
        }
    }
}

#[test]
fn erc_drc_and_sync_preview_match_their_goldens() {
    for (dir, file) in PROJECTS {
        let (_work, mut engine) = open_copy(dir, file);
        for (id, golden) in [
            ("erc.run", "erc.json"),
            ("drc.run", "drc.json"),
            ("sync.preview", "sync.json"),
        ] {
            let mut result = engine.run_ok(id, json!({}));
            // The staleness fingerprint hashes the model's Debug output, so it is only
            // stable within one build.
            if let Some(map) = result.as_object_mut() {
                map.remove("design");
            }
            assert_golden(goldens(dir).join(golden), &pretty(&result));
        }
    }
}

#[test]
fn every_output_matches_its_goldens() {
    for (dir, file) in PROJECTS {
        let (work, mut engine) = open_copy(dir, file);
        let jobs: Vec<String> = engine
            .project()
            .unwrap()
            .output_jobs()
            .map(|job| job.uuid.to_string())
            .collect();
        assert!(!jobs.is_empty(), "{dir} has no Output Job");
        let mut reports = Vec::new();
        for job in jobs {
            let mut report = engine.run_ok("output.job.run", json!({"job": job}));
            relative(&mut report, work.path());
            reports.push(report);
        }
        assert_golden(goldens(dir).join("outputs.json"), &pretty(&json!(reports)));

        let outputs = read_tree(&work.path().join("outputs"));
        assert_golden_tree(&goldens(dir).join("outputs"), &outputs);
    }
}

/// The kept Project's Documents as JSON, by file extension.
fn documents(dir: &str) -> Vec<(String, Value)> {
    read_tree(&projects_dir().join(dir))
        .into_iter()
        .filter(|(rel, _)| !rel.contains('/'))
        .map(|(rel, bytes)| {
            let ext = rel.rsplit('.').next().unwrap().to_string();
            (ext, serde_json::from_slice(&bytes).unwrap())
        })
        .collect()
}

/// The rows of `table` in every Document with extension `ext` across the Projects.
fn rows<'a>(docs: &'a [(String, Value)], ext: &str, table: &str) -> Vec<&'a Value> {
    docs.iter()
        .filter(|(e, _)| e == ext)
        .filter_map(|(_, doc)| doc[table].as_array())
        .flatten()
        .collect()
}

#[test]
fn between_them_the_projects_hold_every_object_the_goldens_cover() {
    let docs: Vec<(String, Value)> = PROJECTS.iter().flat_map(|(d, _)| documents(d)).collect();
    let footprints = rows(&docs, "redapcb", "footprints");
    let units = rows(&docs, "redasch", "units");
    let slot =
        |pad: &Value| pad["kind"] == "non_plated" && pad["hole"]["width"] != pad["hole"]["height"];
    let copy_pads = rows(&docs, "redapcb", "footprint_copies")
        .into_iter()
        // A copy row keeps its Pads beside its `footprint` header.
        .flat_map(|c| c["pads"].as_array().cloned().unwrap_or_default())
        .collect::<Vec<_>>();
    let held = [
        (
            "a Net Tie",
            footprints.iter().any(|f| f["component_type"] == "net_tie"),
        ),
        (
            "a bottom-side part",
            footprints.iter().any(|f| f["side"] == "bottom"),
        ),
        (
            "a Zone Cutout",
            !rows(&docs, "redapcb", "zone_cutouts").is_empty(),
        ),
        (
            "a Dimension",
            !rows(&docs, "redapcb", "dimensions").is_empty(),
        ),
        (
            "a Locally Edited Footprint",
            rows(&docs, "redapcb", "footprint_copies")
                .iter()
                .any(|c| c.get("edited_from").is_some()),
        ),
        (
            "a Mechanical Component with no Footprint",
            units
                .iter()
                .any(|u| u["component_type"] == "mechanical" && u.get("footprint").is_none()),
        ),
        (
            "a Not Fitted Component",
            units.iter().any(|u| u["not_fitted"] == true)
                && footprints.iter().any(|f| f["not_fitted"] == true),
        ),
        (
            "a Plated Edge with a Net",
            (docs.iter())
                .filter(|(e, _)| e == "redapcb")
                .map(|(_, doc)| &doc["board_outline"])
                .chain(rows(&docs, "redapcb", "board_cutouts"))
                .flat_map(|c| c["vertices"].as_array().cloned().unwrap_or_default())
                .any(|v| v["plated"]["net"].is_string()),
        ),
        (
            "a Castellated Pad",
            rows(&docs, "redapcb", "free_pads")
                .into_iter()
                .any(|p| p["castellated"] == true)
                || copy_pads.iter().any(|p| p["castellated"] == true),
        ),
        (
            "a non-plated slot",
            rows(&docs, "redapcb", "free_pads").into_iter().any(slot) || copy_pads.iter().any(slot),
        ),
        (
            "an Edge Connector Pad",
            copy_pads.iter().any(|p| p["role"] == "edge_connector"),
        ),
        (
            "a bevelled, hard-gold edge connector",
            (docs.iter())
                .filter(|(e, _)| e == "redapcb")
                .any(|(_, doc)| {
                    let e = &doc["board"]["edge_connector"];
                    e["bevel"].is_object() && e["hard_gold"] == true
                }),
        ),
    ];
    let missing: Vec<&str> = held.iter().filter(|(_, h)| !h).map(|(w, _)| *w).collect();
    assert!(missing.is_empty(), "no reference Project holds {missing:?}");
}

#[test]
fn every_older_format_has_a_frozen_copy_that_opens_and_saves() {
    // Empty while REDA is on its first format.
    #[allow(clippy::reversed_empty_ranges)]
    for n in 1..FORMAT {
        let released_projects = if n == 1 { V1_PROJECTS } else { PROJECTS };
        for (dir, file) in released_projects {
            let frozen = projects_dir()
                .join("formats")
                .join(format!("v{n}"))
                .join(dir);
            assert!(
                frozen.join(file).is_file(),
                "no format {n} copy of {dir}: freeze one at {} before bumping FORMAT",
                frozen.display()
            );
            let work = TempDir::new(&format!("frozen-v{n}-{dir}"));
            copy_tree(&frozen, work.path());
            let mut engine = test_engine_with_library(&starter_library());
            engine.run_ok("project.open", json!({"path": work.path().join(file)}));
            engine.run_ok("project.save", json!({}));
            store::encode(engine.project().unwrap()).unwrap();
        }
    }
}

/// Runs `f` and prints how long it took.
fn timed<T>(what: &str, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let out = f();
    println!("{what}: {:.1} ms", start.elapsed().as_secs_f64() * 1e3);
    out
}

#[test]
#[ignore = "timing; run by hand with --ignored --nocapture"]
fn time_open_checks_and_outputs() {
    for (dir, file) in PROJECTS {
        let (work, mut engine) = timed(&format!("{dir}: open"), || open_copy(dir, file));
        for id in ["erc.run", "sync.preview", "drc.run", "pcb.ratsnest"] {
            timed(&format!("{dir}: {id}"), || engine.run_ok(id, json!({})));
        }
        timed(&format!("{dir}: pcb.zone.fill"), || {
            engine.run_ok("pcb.zone.fill", json!({}))
        });
        let job = engine
            .project()
            .unwrap()
            .output_jobs()
            .next()
            .unwrap()
            .uuid
            .to_string();
        timed(&format!("{dir}: output.job.run"), || {
            engine.run_ok("output.job.run", json!({"job": job}))
        });
        timed(&format!("{dir}: project.save"), || {
            engine.run_ok("project.save", json!({}))
        });
        drop(work);
    }
}

#[test]
#[ignore = "timing; run by hand with --ignored --nocapture"]
fn time_drc_after_a_track_edit() {
    for (dir, file) in PROJECTS {
        let (_work, mut engine) = open_copy(dir, file);
        engine.run_ok("drc.run", json!({}));
        engine.run_ok(
            "pcb.track.add",
            json!({"layer": "copper_top", "points": [{"x": "1mm", "y": "1mm"}, {"x": "2mm", "y": "1mm"}]}),
        );
        timed(&format!("{dir}: drc.run after an edit"), || {
            engine.run_ok("drc.run", json!({}))
        });
    }
}

#[test]
fn six_layer_hdi_is_a_clean_six_layer_board() {
    check_clean_board("six_layer_hdi", "SixLayerHdi.reda", 6);
}

#[test]
fn castellated_module_is_clean_with_its_plating_joined_to_gnd() {
    check_clean_board("castellated_module", "Castellated.reda", 2);
    let (_work, engine) = open_copy("castellated_module", "Castellated.reda");
    let pcb = engine.project().unwrap().pcb().unwrap();
    let gnd = pcb.nets().find(|n| n.name == "GND").unwrap().uuid;
    let edges = pcb.plated_edges();
    // The bottom edge and the chamfer meet at a corner, both on GND.
    assert_eq!(edges.len(), 2);
    assert_eq!(edges[0].to, edges[1].from);
    assert!(edges.iter().all(|e| e.net == Some(gnd)));
    let castellated: Vec<_> = pcb.free_pads().filter(|p| p.castellated).collect();
    assert!(castellated.iter().any(|p| p.at.x.0 == 0));
    assert!(castellated.iter().any(|p| p.at.y.0 == 15_000_000));
    // The GND Zones join the plating: it is one island with the GND Pads.
    let copper = reda_engine::pcb::Copper::of(engine.project().unwrap()).unwrap();
    let plating = reda_engine::pcb::Item::PlatedEdge {
        contour: edges[0].contour,
        edge: 0,
    };
    let island = copper
        .islands(gnd)
        .into_iter()
        .find(|i| i.contains(&plating))
        .unwrap();
    assert!(
        island
            .iter()
            .any(|i| matches!(i, reda_engine::pcb::Item::Pad { .. }))
    );
}

#[test]
fn four_layer_is_clean_and_uses_the_signal_to_return_clearance() {
    use reda_engine::model::{RuleKind, RuleScope, UnusedLayers, geom::Nm};
    use reda_engine::pcb::rules::{clearance_between_on, net_rules_on};

    check_clean_board("four_layer", "FourLayer.reda", 4);
    let (_work, engine) = open_copy("four_layer", "FourLayer.reda");
    let pcb = engine.project().unwrap().pcb().unwrap();
    let net = |name| pcb.nets().find(|n| n.name == name).unwrap();
    let signal = net("SIGNAL");
    let ret = net("RETURN");
    let mut classes = [
        pcb.net_class_named("Signal").unwrap().uuid,
        pcb.net_class_named("Return").unwrap().uuid,
    ];
    assert_eq!(signal.classes, [classes[0]]);
    assert_eq!(ret.classes, [classes[1]]);
    classes.sort();
    assert!(
        pcb.scoped_rule(
            RuleKind::Clearance,
            &RuleScope {
                net_class_pair: Some(classes),
                ..RuleScope::BOARD
            }
        )
        .unwrap()
        .enabled
    );
    assert!(
        pcb.scoped_rule(RuleKind::TrackWidth, &RuleScope::INNER)
            .unwrap()
            .enabled
    );
    let layer = pcb.copper_layers().nth(1).unwrap().uuid;
    let width = net_rules_on(pcb, Some(signal.uuid), Some(layer)).track_width;
    assert_eq!(
        (width.min, width.preferred),
        (Some(Nm(150_000)), Nm(250_000))
    );
    assert_eq!(
        clearance_between_on(pcb, Some(signal.uuid), Some(ret.uuid), Some(layer)),
        Some(Nm(250_000))
    );
    assert_eq!(pcb.stackup().via_types.len(), 1);
    assert_eq!(pcb.vias().count(), 2);
    for via in pcb.vias() {
        assert_eq!(via.span, pcb.stackup().through_type().span());
        assert_eq!(via.unused_layers, UnusedLayers::Remove);
    }
    // The through-hole Free Pad leaves out its unconnected inner lands.
    let pad = pcb
        .free_pads()
        .find(|p| p.unused_layers == UnusedLayers::Remove)
        .unwrap();
    let copper = reda_engine::pcb::Copper::of(engine.project().unwrap()).unwrap();
    let item = reda_engine::pcb::Item::FreePad { uuid: pad.uuid };
    let lands: Vec<bool> = pcb
        .copper_layers()
        .map(|l| copper.items_at(l.uuid, pad.at).contains(&item))
        .collect();
    assert_eq!(lands, [true, false, false, true]);
}

#[test]
fn m2_card_passes_drc_with_its_half_moon_on_the_far_edge() {
    // testing.md#outputs: built from the M.2 template (key M, 2280), with the fixture
    // bevel {20°, 1.30} and hard gold the script sets. The fingers cross the outline's
    // edge clearance and the half-moon crosses the outline itself, and neither fires.
    let (_work, mut engine) = open_copy("m2_card", "M2Card.reda");
    let drc = engine.run_ok("drc.run", json!({}));
    assert_eq!(drc["violations"], json!([]), "{drc}");
    // The card's fingers are wired to nothing yet: ERC only finds J1's open pins.
    let erc = engine.run_ok("erc.run", json!({}));
    let erc = erc["violations"].as_array().unwrap();
    assert_eq!(erc.len(), 67);
    assert!(erc.iter().all(|v| v["check"] == "unconnected_pin"
        && v["args"]["pin"].as_str().unwrap().starts_with("J1-")));
    assert_eq!(engine.run_ok("sync.preview", json!({}))["count"], 0);
    let pcb = engine.project().unwrap().pcb().unwrap();
    let edge = pcb.setup().edge_connector.as_ref().unwrap();
    let bevel = edge.bevel.unwrap();
    assert_eq!((bevel.angle.0, bevel.depth.0), (20_000, 1_300_000));
    assert!(edge.hard_gold);
    assert!(pcb.has_edge_connector_pads());
    assert!(pcb.has_castellated_pads());
    let moon = pcb
        .footprints()
        .find(|f| f.designator.starts_with('H'))
        .unwrap();
    assert_eq!((moon.at.x.0, moon.at.y.0), (11_000_000, 80_000_000));
    assert!(moon.locked);
}

fn check_clean_board(name: &str, file: &str, expected_layers: usize) {
    let (_work, mut engine) = open_copy(name, file);
    assert_eq!(
        engine
            .project()
            .unwrap()
            .pcb()
            .unwrap()
            .copper_layers()
            .count(),
        expected_layers
    );
    for check in ["erc.run", "drc.run"] {
        let report = engine.run_ok(check, json!({}));
        assert_eq!(report["violations"], json!([]), "{check}: {report}");
        assert_eq!(report["waived"], 0, "{check}: {report}");
    }
    assert_eq!(engine.run_ok("sync.preview", json!({}))["count"], 0);
    assert_eq!(engine.run_ok("pcb.ratsnest", json!({}))["lines"], json!([]));
}

#[test]
fn six_layer_hdi_routes_its_allowed_types_through_a_stacked_laser_pair() {
    use reda_engine::model::{CopperRole, PadKind, PadShape, RuleKind};
    use std::collections::BTreeSet;

    let (_work, engine) = open_copy("six_layer_hdi", "SixLayerHdi.reda");
    let pcb = engine.project().unwrap().pcb().unwrap();
    let layers: Vec<_> = pcb.copper_layers().collect();
    assert_eq!(layers[2].name.as_deref(), Some("HDI breakout"));
    assert_eq!(layers[2].role, CopperRole::Mixed);

    let net = pcb.nets().find(|n| n.name == "HDI_PATH").unwrap();
    let class = pcb.net_class_named("HDI signal").unwrap();
    assert_eq!(net.classes, [class.uuid]);
    let allowed = pcb
        .rules()
        .find(|r| r.enabled && r.kind == RuleKind::ViaSpan && r.scope.net_class == Some(class.uuid))
        .unwrap()
        .values
        .allowed
        .as_ref()
        .unwrap();
    let mut spans = BTreeMap::new();
    let mut used = BTreeSet::new();
    for via in pcb.vias() {
        assert_eq!(via.net, Some(net.uuid));
        let kind = pcb.stackup().type_for_span(via.span).unwrap();
        let span = (
            pcb.copper_number(via.span.from).unwrap(),
            pcb.copper_number(via.span.to).unwrap(),
        );
        assert!(spans.insert(span, (kind.micro, via.at)).is_none());
        used.insert(kind.uuid);
    }
    assert_eq!(
        spans.keys().copied().collect::<Vec<_>>(),
        [(1, 2), (2, 3), (3, 5), (6, 5)]
    );
    assert!(spans[&(1, 2)].0 && spans[&(2, 3)].0);
    assert_eq!(
        spans[&(1, 2)].1,
        spans[&(2, 3)].1,
        "end-to-end Micro Vias share a centre"
    );
    assert!(!spans[&(3, 5)].0 && !spans[&(6, 5)].0);
    assert_eq!(allowed.iter().copied().collect::<BTreeSet<_>>(), used);

    let pad = pcb
        .free_pads()
        .find(|p| p.kind == PadKind::PlatedThroughHole)
        .unwrap();
    assert_eq!(
        (
            pad.shape,
            pad.middle.unwrap().shape,
            pad.bottom.unwrap().shape
        ),
        (PadShape::Oval, PadShape::Rect, PadShape::Circle)
    );
}
