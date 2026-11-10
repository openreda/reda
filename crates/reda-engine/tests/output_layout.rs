//! Where an Output Job run puts its files (outputs.md, "Output layout"): the zip and its
//! loose copies in `fab/`, the BOM and CPL beside it (or in it, with `zip_assembly`),
//! `reports/` and the Output Manifest; the name template and destination tokens; a zip
//! that is the same on every run; and `output.job.run_all`.

use std::io::Read;
use std::path::{Path, PathBuf};

use reda_engine::Engine;
use reda_engine::command::{ErrorCode, FixedClock, SeededIds, Timestamp};
use reda_testkit::{TempDir, TestEngine, probe_commands, probe_engine};
use serde_json::{Value, json};

/// The Output Job `project.new` makes, on seed 0.
const JOB: &str = "00000000-0000-4000-8000-000000000028";

fn pt(x: f64, y: f64) -> Value {
    json!({"x": (x * 1e6).round() as i64, "y": (y * 1e6).round() as i64})
}

fn board(dir: &TempDir) -> TestEngine {
    let mut engine = probe_engine();
    engine.run_ok(
        "project.new",
        json!({"path": dir.path(), "name": "Demo", "board": {"width": "30mm", "height": "20mm"}}),
    );
    engine.run_ok(
        "pcb.via.add",
        json!({"at": pt(5.0, 5.0), "diameter": "0.6mm", "hole": "0.3mm"}),
    );
    engine
}

fn run(engine: &mut TestEngine) -> Value {
    engine.run_ok("output.job.run", json!({"job": JOB}))
}

fn paths(out: &Value) -> Vec<String> {
    out["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap().to_owned())
        .collect()
}

/// Every file under `dir`, `/`-separated and sorted.
fn tree(dir: &Path) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let rel = path.strip_prefix(root).unwrap();
                let parts: Vec<_> = rel.iter().map(|p| p.to_string_lossy()).collect();
                out.push(parts.join("/"));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

fn zip_entries(path: &Path) -> Vec<(String, Vec<u8>)> {
    let bytes = std::fs::read(path).unwrap();
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    (0..archive.len())
        .map(|i| {
            let mut entry = archive.by_index(i).unwrap();
            let mut content = Vec::new();
            entry.read_to_end(&mut content).unwrap();
            (entry.name().unwrap().to_string(), content)
        })
        .collect()
}

fn manifest(folder: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(folder.join("manifest.json")).unwrap()).unwrap()
}

fn folder(dir: &TempDir) -> PathBuf {
    // One component at a time: the run normalises its destination, and the zip and
    // manifest paths are compared as JSON strings, so the separators must be the platform's.
    dir.path().join("Demo").join("outputs").join("Generic")
}

/// Rewrites `from` to `to` in the saved Project file and reopens it.
fn edit_project_file(engine: &mut TestEngine, dir: &TempDir, from: &str, to: &str) {
    engine.run_ok("project.save", json!({}));
    let file = dir.path().join("Demo/Demo.reda");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains(from), "{from} in the Project file");
    std::fs::write(&file, text.replace(from, to)).unwrap();
    engine.run_ok("project.open", json!({"path": dir.path().join("Demo")}));
}

#[test]
fn the_zip_holds_the_fab_files_and_the_bom_cpl_manifest_reports_and_fab_copies_sit_beside_it() {
    let dir = TempDir::new("layout");
    let mut engine = board(&dir);
    let out = run(&mut engine);
    let folder = folder(&dir);
    assert_eq!(out["zip"], json!(folder.join("Demo-Generic.zip")));
    assert_eq!(out["manifest"], json!(folder.join("manifest.json")));

    let fab = [
        "Demo.gtl",
        "Demo.gbl",
        "Demo.gts",
        "Demo.gbs",
        "Demo.gm1",
        "Demo-PTH.drl",
        "Demo-fab-notes.txt",
        "Demo-drill-report.txt",
        "Demo-job.gbrjob",
    ];
    let mut expected: Vec<String> = fab.iter().map(|n| format!("fab/{n}")).collect();
    expected.extend(
        [
            "Demo-Generic.zip",
            "Demo-Generic-BOM.csv",
            "Demo-Generic-CPL.csv",
            "Demo-Generic-schematic.pdf",
            "reports/Demo-erc.json",
            "reports/Demo-drc.json",
            "reports/Demo-change-list.json",
        ]
        .map(String::from),
    );
    assert_eq!(paths(&out), expected, "every file in kind order");
    let functions: Vec<&str> = out["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["function"].as_str().unwrap())
        .collect();
    assert_eq!(
        functions[9..],
        [
            "zip",
            "bom",
            "cpl",
            "schematic_pdf",
            "erc_report",
            "drc_report",
            "change_list_report"
        ]
    );

    let mut on_disk = expected.clone();
    on_disk.push("manifest.json".into());
    on_disk.sort();
    assert_eq!(
        tree(&folder),
        on_disk,
        "nothing else, and no loose Gerbers at the top"
    );

    let entries = zip_entries(&folder.join("Demo-Generic.zip"));
    let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names, fab,
        "BOM, CPL, the schematic PDF and reports stay out of the zip"
    );
    for (name, bytes) in &entries {
        assert_eq!(
            &std::fs::read(folder.join("fab").join(name)).unwrap(),
            bytes,
            "fab/{name} is the zip's copy"
        );
    }
    let notes = std::fs::read_to_string(folder.join("fab/Demo-fab-notes.txt")).unwrap();
    assert!(
        !notes.contains("BOM.csv"),
        "the Fab Notes list only the zip"
    );

    let report: Value =
        serde_json::from_slice(&std::fs::read(folder.join("reports/Demo-drc.json")).unwrap())
            .unwrap();
    assert_eq!(report["kind"], "drc");
    assert!(
        report.get("design").is_none(),
        "no build-dependent fingerprint"
    );
}

#[test]
fn zip_assembly_moves_the_bom_and_cpl_into_the_zip_named_from_the_project() {
    let dir = TempDir::new("layout-zip-assembly");
    let mut engine = board(&dir);
    edit_project_file(
        &mut engine,
        &dir,
        "\"zip_assembly\":false",
        "\"zip_assembly\":true",
    );
    let out = run(&mut engine);
    let folder = folder(&dir);
    let entries = zip_entries(&folder.join("Demo-Generic.zip"));
    let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names[names.len() - 2..], ["Demo-BOM.csv", "Demo-CPL.csv"]);
    assert!(folder.join("fab/Demo-BOM.csv").exists());
    assert!(!folder.join("Demo-Generic-BOM.csv").exists());
    assert!(!paths(&out).iter().any(|p| p == "Demo-Generic-BOM.csv"));
    let notes = std::fs::read_to_string(folder.join("fab/Demo-fab-notes.txt")).unwrap();
    assert!(notes.contains("Demo-BOM.csv"), "{notes}");
}

#[test]
fn the_same_design_and_version_give_the_same_zip_and_only_the_manifest_holds_the_date() {
    let dir = TempDir::new("layout-determinism");
    let mut engine = board(&dir);
    engine.run_ok("project.save", json!({}));
    run(&mut engine);
    let folder = folder(&dir);
    let first = std::fs::read(folder.join("Demo-Generic.zip")).unwrap();
    let first_manifest = manifest(&folder);
    assert_eq!(first_manifest["date"], "2026-01-01T00:00:00Z");

    // A day and an hour later, on another engine.
    let mut later = TestEngine(
        Engine::builder()
            .ids(SeededIds::new(1000))
            .clock(FixedClock(Timestamp(FixedClock::EPOCH.0.0 + 90_000_000)))
            .version(reda_testkit::TEST_VERSION)
            .commands(probe_commands)
            .build(),
    );
    later.run_ok("project.open", json!({"path": dir.path().join("Demo")}));
    run(&mut later);
    assert_eq!(
        std::fs::read(folder.join("Demo-Generic.zip")).unwrap(),
        first,
        "an identical zip"
    );
    let second = manifest(&folder);
    assert_eq!(second["date"], "2026-01-02T01:00:00Z");
    assert_eq!(second["zip"], first_manifest["zip"]);
    for (name, bytes) in zip_entries(&folder.join("Demo-Generic.zip")) {
        assert!(
            !String::from_utf8_lossy(&bytes).contains("2026-01-0"),
            "{name}: no date"
        );
    }
}

#[test]
fn the_manifest_lists_every_file_the_zip_hash_the_run_and_its_warnings() {
    let dir = TempDir::new("layout-manifest");
    let mut engine = board(&dir);
    engine.run_ok("project.properties.set", json!({"revision": "B"}));
    let out = run(&mut engine);
    let folder = folder(&dir);
    let m = manifest(&folder);
    let text = std::fs::read_to_string(folder.join("manifest.json")).unwrap();
    assert!(
        text.starts_with("{\n  \"format\": 1,\n"),
        "canonical JSON: {text}"
    );
    assert_eq!(m["reda"], reda_testkit::TEST_VERSION);
    assert_eq!(m["job"]["uuid"], JOB);
    assert_eq!(m["job"]["name"], "Generic");
    assert_eq!(m["fab_preset"]["name"], "Generic");
    assert_eq!(m["fab_preset"]["sha256"].as_str().unwrap().len(), 64);
    let pcb = engine.project().unwrap().pcb().unwrap().uuid.to_string();
    assert_eq!(m["pcb"], json!(pcb));
    assert_eq!(m["revision"], "B");

    let zip = std::fs::read(folder.join("Demo-Generic.zip")).unwrap();
    assert_eq!(
        m["zip"],
        json!({"path": "Demo-Generic.zip", "sha256": reda_output::sha256_hex(&zip)})
    );
    let listed: Vec<&str> = m["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        listed,
        paths(&out),
        "every file but the manifest, in run order"
    );
    for file in m["files"].as_array().unwrap() {
        let bytes = std::fs::read(folder.join(file["path"].as_str().unwrap())).unwrap();
        assert_eq!(file["size"], bytes.len(), "{file}");
        assert_eq!(file["sha256"], reda_output::sha256_hex(&bytes), "{file}");
    }
    assert_eq!(m["files"][0]["kind"], "copper_top");

    let warnings = out["warnings"].as_array().unwrap();
    assert_eq!(m["warnings"].as_array().unwrap().len(), warnings.len());
    for (listed, warning) in m["warnings"].as_array().unwrap().iter().zip(warnings) {
        assert_eq!(listed["check"], warning["check"]);
        assert!(!listed["message"].as_str().unwrap().is_empty());
    }
}

#[test]
fn a_run_warning_reaches_the_manifest_with_its_message() {
    let dir = TempDir::new("layout-manifest-warning");
    let mut engine = board(&dir);
    engine.run_ok(
        "pcb.via.add",
        json!({"at": pt(5.2, 5.0), "diameter": "0.6mm", "hole": "0.3mm"}),
    );
    let out = run(&mut engine);
    let checks: Vec<&str> = out["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["check"].as_str().unwrap())
        .collect();
    assert!(checks.contains(&"drc_errors"), "{checks:?}");
    let m = manifest(&folder(&dir));
    let drc = m["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["check"] == "drc_errors")
        .unwrap();
    assert!(drc["message"].as_str().unwrap().contains("DRC"), "{drc}");
}

#[test]
fn the_name_template_names_the_zip_bom_and_cpl_and_the_destination_resolves_the_same_tokens() {
    let dir = TempDir::new("layout-name-template");
    let mut engine = board(&dir);
    engine.run_ok("project.properties.set", json!({"revision": "C"}));
    engine.run_ok(
        "output.job.set",
        json!({"job": JOB, "name_template": "${project}_${revision}_${date}_${preset}",
               "destination": "outputs/${job}/${revision}"}),
    );
    let out = run(&mut engine);
    let folder = folder(&dir).join("C");
    assert_eq!(out["destination"], json!(folder));
    let stem = "Demo_C_2026-01-01_Generic";
    assert_eq!(out["zip"], json!(folder.join(format!("{stem}.zip"))));
    assert!(folder.join(format!("{stem}-BOM.csv")).exists());
    assert!(folder.join(format!("{stem}-CPL.csv")).exists());
    let entries = zip_entries(&folder.join(format!("{stem}.zip")));
    assert_eq!(
        entries[0].0, "Demo.gtl",
        "files in the zip stay named from the project"
    );
    assert!(folder.join("fab/Demo.gtl").exists());
}

#[test]
fn a_destination_with_an_unknown_token_is_refused() {
    let dir = TempDir::new("layout-destination-token");
    let mut engine = board(&dir);
    let err = engine.run_err(
        "output.job.set",
        json!({"job": JOB, "destination": "outputs/${nope}"}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams);
}

#[test]
fn a_token_value_a_file_name_cannot_hold_refuses_the_run() {
    let dir = TempDir::new("layout-bad-revision");
    let mut engine = board(&dir);
    engine.run_ok("project.properties.set", json!({"revision": "A/B"}));
    engine.run_ok(
        "output.job.set",
        json!({"job": JOB, "name_template": "${project}-${revision}"}),
    );
    let err = engine.run_err("output.job.run", json!({"job": JOB}));
    assert_eq!(err.code, ErrorCode::Rejected);
    assert!(err.message.contains("name"), "{}", err.message);
}

#[test]
fn a_new_run_removes_what_the_last_runs_manifest_listed() {
    let dir = TempDir::new("layout-rename");
    let mut engine = board(&dir);
    run(&mut engine);
    let folder = folder(&dir);
    std::fs::write(folder.join("notes.txt"), "mine").unwrap();
    engine.run_ok(
        "output.job.set",
        json!({"job": JOB, "name_template": "${project}-rev"}),
    );
    run(&mut engine);
    assert!(folder.join("Demo-rev.zip").exists());
    assert!(
        !folder.join("Demo-Generic.zip").exists(),
        "the old zip is gone"
    );
    assert!(!folder.join("Demo-Generic-BOM.csv").exists());
    assert!(
        folder.join("notes.txt").exists(),
        "other files are left alone"
    );
}

#[test]
fn the_schematic_pdf_is_every_sheet_beside_the_zip_in_the_jobs_colour_and_has_a_toggle() {
    let dir = TempDir::new("layout-schematic-pdf");
    let mut engine = board(&dir);
    engine.run_ok(
        "document.add",
        json!({"kind": "schematic", "title": "Power"}),
    );
    run(&mut engine);
    let folder = folder(&dir);
    let pdf = std::fs::read(folder.join("Demo-Generic-schematic.pdf")).unwrap();
    engine.run_ok("schematic.export_pdf", json!({"path": "all.pdf"}));
    assert_eq!(
        pdf,
        std::fs::read(dir.path().join("Demo/all.pdf")).unwrap(),
        "every sheet, as `schematic.export_pdf` draws them"
    );
    assert!(
        String::from_utf8_lossy(&pdf).contains("/Count 2 >>"),
        "two pages"
    );
    let listed = manifest(&folder)["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["path"] == "Demo-Generic-schematic.pdf")
        .cloned()
        .expect("the manifest lists the PDF");
    assert_eq!(listed["kind"], "schematic_pdf");
    assert_eq!(listed["size"], pdf.len());

    engine.run_ok(
        "output.job.set",
        json!({"job": JOB, "schematic_pdf_colour": "mono"}),
    );
    run(&mut engine);
    engine.run_ok(
        "schematic.export_pdf",
        json!({"path": "mono.pdf", "colour": "mono"}),
    );
    let mono = std::fs::read(folder.join("Demo-Generic-schematic.pdf")).unwrap();
    assert_ne!(mono, pdf);
    assert_eq!(
        mono,
        std::fs::read(dir.path().join("Demo/mono.pdf")).unwrap()
    );

    engine.run_ok(
        "output.job.set",
        json!({"job": JOB, "outputs": {"schematic_pdf": false}}),
    );
    let out = run(&mut engine);
    assert!(!paths(&out).iter().any(|p| p.ends_with(".pdf")));
    assert!(
        !folder.join("Demo-Generic-schematic.pdf").exists(),
        "turning it off drops the old PDF"
    );
}

#[test]
fn the_reports_toggle_leaves_out_the_reports_folder() {
    let dir = TempDir::new("layout-reports-off");
    let mut engine = board(&dir);
    run(&mut engine);
    let folder = folder(&dir);
    assert!(folder.join("reports/Demo-drc.json").exists());
    engine.run_ok(
        "output.job.set",
        json!({"job": JOB, "outputs": {"reports": false}}),
    );
    let out = run(&mut engine);
    assert!(!paths(&out).iter().any(|p| p.starts_with("reports/")));
    assert!(
        !folder.join("reports/Demo-drc.json").exists(),
        "the old report is gone"
    );
    assert!(
        folder.join("manifest.json").exists(),
        "the manifest has no toggle"
    );
}

#[test]
fn skipped_files_are_reported_in_the_run_and_the_manifest_names_none_of_them() {
    let dir = TempDir::new("layout-skipped");
    let mut engine = board(&dir);
    let out = run(&mut engine);
    let skipped: Vec<&str> = out["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        skipped,
        [
            "Demo.gtp",
            "Demo.gbp",
            "Demo.gto",
            "Demo.gbo",
            "Demo-NPTH.drl"
        ]
    );
    let folder = folder(&dir);
    for name in skipped {
        assert!(!folder.join("fab").join(name).exists(), "{name}");
    }
}

/// A second job on a PCBWay copy (relative origin), and a Track that only an absolute job
/// can't plot: the Generic job blocks, the PCBWay one runs.
fn two_jobs(dir: &TempDir) -> (TestEngine, String) {
    let mut engine = board(dir);
    let preset =
        engine.run_ok("fab_preset.add", json!({"name": "PCBWay"}))["result"]["uuid"].clone();
    let pcbway = engine.run_ok("output.job.add", json!({"fab_preset": preset}))["result"]["uuid"]
        .as_str()
        .unwrap()
        .to_owned();
    engine.run_ok(
        "pcb.track.add",
        json!({"layer": "copper_top", "width": "0.2mm",
               "points": [pt(5.0, 5.0), {"x": "2147.5mm", "y": "5mm"}]}),
    );
    engine.run_ok("pcb.output_origin.set", json!({"at": pt(100.0, 0.0)}));
    (engine, pcbway)
}

#[test]
fn run_all_runs_every_job_in_projects_panel_order_and_stops_at_a_block_by_default() {
    let dir = TempDir::new("layout-run-all");
    let (mut engine, pcbway) = two_jobs(&dir);
    let order: Vec<String> = engine
        .project()
        .unwrap()
        .output_jobs()
        .map(|j| j.uuid.to_string())
        .collect();
    assert_eq!(
        order,
        [JOB.to_owned(), pcbway.clone()],
        "the Projects panel order"
    );

    let out = engine.run_ok("output.job.run_all", json!({}));
    let jobs = out["jobs"].as_array().unwrap();
    assert_eq!(jobs.len(), 2);
    assert_eq!(jobs[0]["job"], JOB);
    assert_eq!(jobs[0]["status"], "blocked");
    assert_eq!(jobs[0]["blockers"][0]["check"], "coordinate_out_of_range");
    assert_eq!(jobs[1]["job"], json!(pcbway));
    assert_eq!(jobs[1]["status"], "not_run");
    assert_eq!(out["stopped"], true);
    assert!(!dir.path().join("Demo/outputs/PCBWay").exists());

    let out = engine.run_ok("output.job.run_all", json!({"stop_on_block": false}));
    let jobs = out["jobs"].as_array().unwrap();
    assert_eq!(jobs[0]["status"], "blocked");
    assert_eq!(jobs[1]["status"], "written");
    assert_eq!(jobs[1]["name"], "PCBWay");
    assert!(
        jobs[1]["result"]["zip"]
            .as_str()
            .unwrap()
            .ends_with("Demo-PCBWay.zip")
    );
    assert_eq!(out["stopped"], false);
    assert!(
        dir.path()
            .join("Demo/outputs/PCBWay/Demo-PCBWay.zip")
            .exists()
    );
}

#[test]
fn run_all_writes_every_job_when_none_blocks() {
    let dir = TempDir::new("layout-run-all-clean");
    let mut engine = board(&dir);
    engine.run_ok("output.job.duplicate", json!({"job": JOB}));
    let out = engine.run_ok("output.job.run_all", json!({"stop_on_block": true}));
    let statuses: Vec<&str> = out["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|j| j["status"].as_str().unwrap())
        .collect();
    assert_eq!(statuses, ["written", "written"]);
    assert_eq!(out["stopped"], false);
}
