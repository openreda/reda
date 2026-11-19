//! What the Output Job window previews before a run: the paths the name template and
//! destination give, each Gerber and drill file and whether it is plotted, the zip's
//! contents, and the BOM and CPL rows. It must agree with what a run writes.

use reda_engine::output_preview::{FileState, job_preview};
use reda_model::{OutputJob, Uuid};
use reda_testkit::{TempDir, TestEngine, probe_engine, starter_library, test_engine_with_library};
use serde_json::{Value, json};

/// The Output Job `project.new` makes, on seed 0.
const JOB: &str = "00000000-0000-4000-8000-000000000028";
const SHEET: &str = "00000000-0000-4000-8000-000000000002";
/// The starter Library's Resistor and its 0603 Footprint.
const RESISTOR: &str = "84d5651a-bf4b-462e-9480-39e8f454a042";
const RESC1608: &str = "0691e5e7-9eb6-49f3-9f3e-d4159258693e";

fn pt(x: f64, y: f64) -> Value {
    json!({"x": (x * 1e6).round() as i64, "y": (y * 1e6).round() as i64})
}

fn board(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path": dir.path(), "name": "Demo", "board": {"width": "30mm", "height": "20mm"}}),
    );
    // A plated hole, so the PTH file is written and the NPTH one left out.
    engine.run_ok(
        "pcb.via.add",
        json!({"at": pt(5.0, 5.0), "diameter": "0.6mm", "hole": "0.3mm"}),
    );
    engine
}

fn job(engine: &TestEngine) -> OutputJob {
    let uuid: Uuid = JOB.parse().unwrap();
    engine.project().unwrap().output_job(uuid).unwrap().clone()
}

#[test]
fn the_preview_lists_each_gerber_and_drill_file_and_whether_it_is_plotted() {
    let dir = TempDir::new("preview-files");
    let engine = board(&dir);
    let preview = job_preview(&engine, &job(&engine)).unwrap();
    let gerber: Vec<(&str, &str, FileState)> = preview
        .gerber
        .iter()
        .map(|f| (f.layer.as_str(), f.name.as_str(), f.state))
        .collect();
    assert_eq!(
        gerber,
        [
            ("Top Copper", "Demo.gtl", FileState::Plotted),
            ("Bottom Copper", "Demo.gbl", FileState::Plotted),
            ("Top Solder Mask", "Demo.gts", FileState::Plotted),
            ("Bottom Solder Mask", "Demo.gbs", FileState::Plotted),
            ("Top Paste", "Demo.gtp", FileState::Empty),
            ("Bottom Paste", "Demo.gbp", FileState::Empty),
            ("Top Silkscreen", "Demo.gto", FileState::Empty),
            ("Bottom Silkscreen", "Demo.gbo", FileState::Empty),
            ("Board Outline", "Demo.gm1", FileState::Plotted),
        ]
    );
    let drill: Vec<(&str, FileState)> = preview
        .drill
        .iter()
        .map(|f| (f.name.as_str(), f.state))
        .collect();
    assert_eq!(
        drill,
        [
            ("Demo-PTH.drl", FileState::Plotted),
            ("Demo-NPTH.drl", FileState::Empty)
        ]
    );
}

#[test]
fn an_output_turned_off_is_previewed_as_off() {
    let dir = TempDir::new("preview-off");
    let engine = board(&dir);
    let mut job = job(&engine);
    job.outputs.gerber = false;
    job.outputs.fab_notes = false;
    let preview = job_preview(&engine, &job).unwrap();
    assert!(preview.gerber.iter().all(|f| f.state == FileState::Off));
    assert!(!preview.zip.iter().any(|n| n.ends_with(".gtl")));
    assert!(!preview.zip.iter().any(|n| n.contains("fab-notes")));
}

#[test]
fn the_previewed_zip_and_paths_are_what_a_run_writes() {
    let dir = TempDir::new("preview-run");
    let mut engine = board(&dir);
    engine.run_ok(
        "output.job.set",
        json!({"job": JOB, "name_template": "${project}-r${revision}",
               "destination": "out/${job}"}),
    );
    engine.run_ok("project.properties.set", json!({"revision": "B"}));
    let preview = job_preview(&engine, &job(&engine)).unwrap();
    assert_eq!(preview.stem.as_deref(), Ok("Demo-rB"));
    assert_eq!(preview.destination.as_deref(), Ok("out/Generic"));
    let out = engine.run_ok("output.job.run", json!({"job": JOB}));
    let zipped: Vec<String> = out["files"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|f| f["path"].as_str().unwrap().strip_prefix("fab/"))
        .map(str::to_owned)
        .collect();
    assert_eq!(preview.zip, zipped);
    let beside: Vec<&str> = out["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap())
        .filter(|p| !p.contains('/'))
        .collect();
    let previewed: Vec<&str> = preview.beside.iter().map(String::as_str).collect();
    assert_eq!(previewed, beside);
    let folder = dir.path().join("Demo").join("out").join("Generic");
    assert!(folder.join("Demo-rB.zip").is_file());
}

#[test]
fn the_bom_and_cpl_previews_are_the_writers_rows() {
    let dir = TempDir::new("preview-assembly");
    let mut engine = test_engine_with_library(&starter_library());
    engine.run_ok(
        "project.new",
        json!({"path": dir.path(), "name": "Demo", "board": {"width": "30mm", "height": "20mm"}}),
    );
    for i in 0..3 {
        engine.run_ok(
            "schematic.component.place",
            json!({"document": SHEET, "part": RESISTOR, "footprint": RESC1608,
                   "at": {"x": format!("{}mm", 20 * i), "y": "40mm"}}),
        );
    }
    let hash = engine.run_ok("sync.preview", json!({}))["hash"].clone();
    engine.run_ok("sync.apply", json!({"hash": hash}));
    let job = engine
        .project()
        .unwrap()
        .output_jobs()
        .next()
        .unwrap()
        .clone();
    let preview = job_preview(&engine, &job).unwrap();
    assert_eq!(
        preview.cpl.header,
        ["Designator", "Mid X", "Mid Y", "Layer", "Rotation"]
    );
    let designators: Vec<&str> = preview.cpl.rows.iter().map(|r| r[0].as_str()).collect();
    assert_eq!(designators, ["R1", "R2", "R3"]);
    assert_eq!(preview.bom.rows.len(), 1);
    assert_eq!(preview.bom.rows[0][..3], ["R1,R2,R3", "3", "10k"]);

    let out = engine.run_ok("output.job.run", json!({"job": job.uuid}));
    let folder = std::path::PathBuf::from(out["destination"].as_str().unwrap());
    for (file, table) in [
        ("Demo-Generic-CPL.csv", &preview.cpl),
        ("Demo-Generic-BOM.csv", &preview.bom),
    ] {
        let written = std::fs::read_to_string(folder.join(file)).unwrap();
        let mut lines = written.lines();
        assert_eq!(lines.next().unwrap(), table.header.join(","));
        let rows: Vec<String> = table
            .rows
            .iter()
            .map(|r| {
                let cells: Vec<String> = r
                    .iter()
                    .map(|c| {
                        if c.contains(',') {
                            format!("\"{c}\"")
                        } else {
                            c.clone()
                        }
                    })
                    .collect();
                cells.join(",")
            })
            .collect();
        assert_eq!(lines.collect::<Vec<_>>(), rows, "{file}");
    }
}

#[test]
fn multilayer_preview_matches_the_physical_order_in_the_zip_manifest_and_job() {
    let dir = TempDir::new("multilayer-preview");
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Demo","layers":4,
                                         "board":{"width":"20mm","height":"15mm"}}),
    );
    let pcb = engine.project().unwrap().pcb().unwrap();
    let inner = pcb.copper_layers().nth(1).unwrap().uuid;
    engine.run_ok(
        "pcb.layer.set",
        json!({"layer":inner,"name":"GND Plane","role":"plane"}),
    );
    let job = engine
        .project()
        .unwrap()
        .output_jobs()
        .next()
        .unwrap()
        .clone();
    engine.run_ok(
        "fab_preset.set",
        json!({"preset":job.fab_preset,"values":{
            "gerber":{"naming":"descriptive","x2_attributes":false,"outline_on_every_plot":true}
        }}),
    );
    let names = [
        "Demo-Top_Copper.gbr",
        "Demo-GND_Plane.gbr",
        "Demo-L3.gbr",
        "Demo-Bottom_Copper.gbr",
    ];
    let labels = ["Top Copper", "L2 GND Plane", "L3", "Bottom Copper"];
    let preview = job_preview(&engine, &job).unwrap();
    let run = engine.run_ok("output.job.run", json!({"job":job.uuid}));
    let dest = std::path::Path::new(run["destination"].as_str().unwrap());
    let read_json =
        |path| serde_json::from_slice::<Value>(&std::fs::read(dest.join(path)).unwrap()).unwrap();
    let manifest = read_json("manifest.json");
    let gerber_job = read_json("fab/Demo-job.gbrjob");
    let zipped = std::fs::File::open(run["zip"].as_str().unwrap()).unwrap();
    let mut zip = zip::ZipArchive::new(zipped).unwrap();
    for (index, (name, label)) in names.iter().zip(labels).enumerate() {
        assert_eq!(preview.gerber[index].layer, label);
        assert_eq!(preview.gerber[index].name, *name);
        assert_eq!(preview.gerber[index].state, FileState::Plotted);
        assert_eq!(preview.zip[index], *name);
        assert_eq!(zip.by_index(index).unwrap().name().unwrap(), *name);
        let path = format!("fab/{name}");
        assert_eq!(run["files"][index]["path"], path);
        assert_eq!(manifest["files"][index]["path"], path);
        assert_eq!(gerber_job["FilesAttributes"][index]["Path"], *name);
    }
    let inner_artwork = std::fs::read_to_string(dest.join("fab/Demo-GND_Plane.gbr")).unwrap();
    assert!(inner_artwork.contains("G04 #@! TF.FileFunction,Copper,L2,Inr,Plane*"));
    assert!(
        inner_artwork.contains("D01*"),
        "the empty inner plane carries the requested board outline"
    );
}

#[test]
fn preview_names_both_files_in_an_active_zip_clash_and_keeps_empty_plots_out() {
    let dir = TempDir::new("preview-clash");
    let mut engine = board(&dir);
    let job = job(&engine);
    engine.run_ok(
        "fab_preset.set",
        json!({"preset":job.fab_preset,"values":{"zip_assembly":true,"gerber":{
            "file_names":[{"layer":"Top Copper","name":"DEMO-bom.csv"},{"layer":"Top Paste","name":"DEMO-bom.csv"}]
        }}}),
    );
    let preview = job_preview(&engine, &job).unwrap();
    let why = preview
        .invalid_file_name
        .expect("the preview identifies the name clash");
    for label in ["Copper,L1,Top", "bom", "Demo-BOM.csv"] {
        assert!(why.contains(label), "{why}");
    }
    let error = engine.run_err("output.job.run", json!({"job":job.uuid}));
    assert_eq!(why, error.message);

    let mut draft = job;
    draft.outputs.bom = false;
    let preview = job_preview(&engine, &draft).unwrap();
    assert_eq!(preview.invalid_file_name, None);
    assert_eq!(
        preview.zip.iter().filter(|f| *f == "DEMO-bom.csv").count(),
        1
    );
    let paste = preview
        .gerber
        .iter()
        .find(|f| f.layer == "Top Paste")
        .unwrap();
    assert_eq!(
        (paste.function, paste.state),
        ("paste_top", FileState::Empty)
    );
}

#[test]
fn a_layer_name_with_a_slash_blocks_only_jobs_that_write_it_as_a_filename() {
    let dir = TempDir::new("preview-inner-file-name");
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path":dir.path(),"name":"Board","layers":4,
               "board":{"width":"20mm","height":"15mm"}}),
    );
    let project = engine.project().unwrap();
    let job = project.output_jobs().next().unwrap().clone();
    let inner = project.pcb().unwrap().copper_layers().nth(1).unwrap().uuid;
    engine.run_ok("pcb.layer.set", json!({"layer":inner,"name":"Power/GND"}));
    engine.run_ok(
        "fab_preset.set",
        json!({"preset":job.fab_preset,"values":{"gerber":{"naming":"descriptive"}}}),
    );
    let why = job_preview(&engine, &job)
        .unwrap()
        .invalid_file_name
        .unwrap();
    assert!(why.contains("Board-Power/GND.gbr"), "{why}");
    assert_eq!(
        why,
        engine
            .run_err("output.job.run", json!({"job":job.uuid}))
            .message
    );

    engine.run_ok(
        "output.job.set",
        json!({"job":job.uuid,"outputs":{"gerber":false}}),
    );
    let disabled = engine.project().unwrap().output_job(job.uuid).unwrap();
    assert_eq!(
        job_preview(&engine, disabled).unwrap().invalid_file_name,
        None
    );
    engine.run_ok("output.job.run", json!({"job":job.uuid}));
}
