//! Read the fab files of the `castellated_module` reference Project back against its PCB
//! (docs/spec/v0.2.5/testing.md#read-back): the plated-edges Gerber holds exactly the
//! marked edges, the Profile is plated, and the job file says the board has plated edges
//! and Castellated Pads.
mod gerber_reader;
mod job_reader;

use std::fs;
use std::path::Path;

use gerber_reader::Object;
use reda_testkit::open_reference_copy;
use serde_json::json;

#[test]
fn the_castellated_module_fab_files_read_back_as_its_plated_edges_and_castellations() {
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/projects/castellated_module");
    let (_dir, mut engine) = open_reference_copy(&source, "Castellated.reda");
    let job = engine.project().unwrap().output_jobs().next().unwrap().uuid;
    let report = engine.run_ok("output.job.run", json!({"job": job}));
    let output = Path::new(report["destination"].as_str().unwrap());
    let pcb = engine.project().unwrap().pcb().unwrap();
    let fab = output.join("fab");

    let job = job_reader::read(&fs::read(fab.join("Castellated-job.gbrjob")).unwrap());
    assert_eq!(job.general_specs.edge_plating, Some(true));
    assert_eq!(job.general_specs.castellated, Some(true));
    assert_eq!(job.general_specs.edge_connector, None);
    let listed = |function: &str| {
        job.files_attributes
            .iter()
            .find(|f| f.file_function == function)
            .map(|f| f.path.clone())
    };
    assert_eq!(
        listed("OtherDrawing,Plated edges").as_deref(),
        Some("Castellated-plated_edges.gbr")
    );
    let profile = listed("Profile,P").expect("a plated Profile");
    let profile = gerber_reader::read(&fs::read(fab.join(profile)).unwrap());
    assert_eq!(profile.file[".FileFunction"], "Profile,P");

    let plated = gerber_reader::read(&fs::read(fab.join("Castellated-plated_edges.gbr")).unwrap());
    assert!(plated.ended);
    assert_eq!(plated.file[".FileFunction"], "OtherDrawing,Plated edges");
    let mut draws: Vec<_> = plated
        .draws()
        .map(|d| match d {
            Object::Draw { from, to, .. } => (*from, *to),
            _ => unreachable!(),
        })
        .collect();
    draws.sort();
    let xy = |p: reda_model::geom::Point| (p.x.0, p.y.0);
    let mut marked: Vec<_> = reda_pcb::plated::board_edges(pcb)
        .iter()
        .map(|e| (xy(e.from), xy(e.to)))
        .collect();
    marked.sort();
    assert_eq!(draws, marked);
    // The bottom edge and the chamfer it meets, both on GND.
    assert_eq!(marked.len(), 2);
    let gnd = pcb.nets().find(|n| n.name == "GND").unwrap().uuid;
    assert!(reda_pcb::plated::board_edges(pcb).iter().all(|e| e.net == Some(gnd)));
    // Castellated Pads on two edges: the left (x = 0) and the top (y = 15 mm).
    let castellated: Vec<_> = pcb.free_pads().filter(|p| p.castellated).collect();
    assert!(castellated.iter().any(|p| p.at.x.0 == 0));
    assert!(castellated.iter().any(|p| p.at.y.0 == 15_000_000));
}
