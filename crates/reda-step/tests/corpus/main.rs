//! The model corpus (docs/spec/v0.3/testing.md#model-corpus): the trial's 58 real models
//! (#442), which cannot be committed, listed in `step-corpus.tsv` with where each came from
//! and FreeCAD's reference volume and box.
//!
//! - `fetch.sh <folder>` downloads the 40 openly fetchable models (KiCad's library and
//!   Würth's KiCad library, at pinned commits) and checks their SHA-256. The other 18 stay on
//!   the maintainer's disk, in the same `<group>/<file>` layout.
//! - [`corpus_models`] reads every model in the folder named by `REDA_STEP_CORPUS`, each in a
//!   process of its own with the trial's 300 s limit (`REDA_STEP_CORPUS_TIMEOUT`, seconds,
//!   overrides it), grades it as the trial did ([`grade`]) and prints a Markdown report,
//!   also written to the file `REDA_STEP_CORPUS_REPORT` names. Falling short of
//!   OpenCascade's 54 of 58 is reported, not failed; a crash or a timeout fails the run.
//! - [`time_reading_the_largest_corpus_model`] times reading and meshing the largest model
//!   in the folder.
//!
//! ```text
//! crates/reda-step/tests/corpus/fetch.sh ~/step-corpus
//! REDA_STEP_CORPUS=~/step-corpus cargo test --release -p reda-step --test corpus -- \
//!     --ignored --nocapture corpus_models time_reading
//! ```
//!
//! The weekly `step-corpus.yml` workflow does this with the 40; it never gates a merge.
//! The other tests here check the harness itself on the fixtures.

mod grade;
mod manifest;
mod report;
mod run;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use grade::{Grade, Graded};
use manifest::{Entry, Manifest};
use reda_step::MeshOptions;

/// The corpus folder `REDA_STEP_CORPUS` names.
fn corpus_folder() -> PathBuf {
    let dir = std::env::var_os("REDA_STEP_CORPUS")
        .expect("set REDA_STEP_CORPUS to the corpus folder (fetch.sh fills one)");
    let dir = PathBuf::from(dir);
    assert!(
        dir.is_dir(),
        "REDA_STEP_CORPUS: {} is not a folder",
        dir.display()
    );
    dir
}

#[test]
#[ignore = "needs the model corpus: REDA_STEP_CORPUS=<folder>, filled by fetch.sh"]
fn corpus_models() {
    let dir = corpus_folder();
    let timeout = std::env::var("REDA_STEP_CORPUS_TIMEOUT").map_or(run::TIMEOUT, |s| {
        Duration::from_secs_f64(s.parse().expect("REDA_STEP_CORPUS_TIMEOUT is seconds"))
    });
    let manifest = Manifest::committed();
    let mut graded = Vec::new();
    for e in &manifest.entries {
        let g = grade::grade(e, run::model(&dir.join(&e.name), timeout));
        eprintln!("{:8} {}", g.grade.word(), e.name);
        graded.push(g);
    }
    let text = report::render(&graded);
    println!("\n{text}");
    if let Some(path) = std::env::var_os("REDA_STEP_CORPUS_REPORT") {
        std::fs::write(&path, &text).unwrap();
    }
    let failed: Vec<&str> = graded
        .iter()
        .filter(|g| matches!(g.grade, Grade::Crash | Grade::Timeout))
        .map(|g| g.name.as_str())
        .collect();
    assert!(failed.is_empty(), "crashed or timed out: {failed:?}");
}

#[test]
#[ignore = "timing; needs the model corpus: REDA_STEP_CORPUS=<folder>"]
fn time_reading_the_largest_corpus_model() {
    let dir = corpus_folder();
    let (size, path) = Manifest::committed()
        .entries
        .iter()
        .map(|e| dir.join(&e.name))
        .filter_map(|p| Some((std::fs::metadata(&p).ok()?.len(), p)))
        .max()
        .expect("no corpus model in the folder");
    let bytes = std::fs::read(&path).unwrap();
    println!(
        "{} ({:.1} MB)",
        path.strip_prefix(&dir).unwrap().display(),
        size as f64 / 1e6
    );
    let mut best = f64::INFINITY;
    for run in 1..=3 {
        let start = Instant::now();
        let read = reda_step::read(&bytes).unwrap();
        let parsed = start.elapsed();
        let mut model = read.model;
        model.to_millimetres();
        let meshed = model.mesh(&MeshOptions::default());
        let total = start.elapsed();
        let triangles: usize = meshed
            .instances()
            .iter()
            .flat_map(|i| &i.part.solids)
            .map(|s| s.mesh.triangles.len())
            .sum();
        println!(
            "run {run}: read {:.0} ms, converted and meshed {:.0} ms, {:.0} ms in all \
             ({triangles} triangles)",
            parsed.as_secs_f64() * 1e3,
            (total - parsed).as_secs_f64() * 1e3,
            total.as_secs_f64() * 1e3
        );
        best = best.min(total.as_secs_f64());
    }
    // docs/spec/v0.3/3d-view.md#frame-time: a scene builds in under 2 s, off the UI thread.
    println!(
        "best {:.0} ms; the 3D view's scene-build target is 2 s",
        best * 1e3
    );
}

/// The child process of [`run::model`]: does nothing unless `run::CHILD_FILE` is set.
#[test]
#[ignore = "the corpus run's child process"]
fn corpus_child() {
    run::child();
}

/// A child that crashes or hangs, for the tests of those reports.
#[test]
#[ignore = "a child process that fails, for the corpus run's own tests"]
fn corpus_failing_child() {
    match std::env::var(FAIL).as_deref() {
        Ok("crash") => panic!("the reader fell over"),
        Ok("hang") => std::thread::sleep(Duration::from_secs(60)),
        _ => {}
    }
}

const FAIL: &str = "REDA_STEP_CORPUS_FAIL";

#[test]
fn a_model_whose_process_dies_is_reported_as_a_crash_with_its_panic() {
    let outcome = run::in_child(
        "corpus_failing_child",
        (FAIL, "crash".as_ref()),
        run::TIMEOUT,
    );
    let run::Outcome::Crash(said) = &outcome else {
        panic!("{outcome:?}");
    };
    assert!(said.contains("the reader fell over"), "{said}");
    let g = grade::grade(&fixture_entry("any.step", None, true), outcome);
    assert_eq!(g.grade, Grade::Crash);
}

#[test]
fn a_model_still_running_at_the_limit_is_stopped_and_reported() {
    let limit = Duration::from_millis(300);
    let start = std::time::Instant::now();
    let outcome = run::in_child("corpus_failing_child", (FAIL, "hang".as_ref()), limit);
    assert_eq!(outcome, run::Outcome::Timeout(limit));
    // Stopped, not waited for.
    assert!(start.elapsed() < Duration::from_secs(30));
    let g = grade::grade(&fixture_entry("any.step", None, true), outcome);
    assert_eq!(g.grade, Grade::Timeout);
}

#[test]
fn a_model_not_in_the_folder_is_missing() {
    let outcome = run::model(&fixtures().join("no such model.step"), run::TIMEOUT);
    assert_eq!(outcome, run::Outcome::Missing);
}

#[test]
fn the_manifest_lists_the_trial_models_with_pinned_sources() {
    let m = Manifest::committed();
    assert_eq!(m.entries.len(), 58);
    let fetchable: Vec<&Entry> = m.entries.iter().filter(|e| e.fetch).collect();
    assert_eq!(fetchable.len(), 40);
    // The fetchable ones are KiCad's library and Würth's KiCad library, at pinned commits.
    assert_eq!(
        fetchable
            .iter()
            .filter(|e| e.name.starts_with("kicad/"))
            .count(),
        26
    );
    assert_eq!(
        fetchable
            .iter()
            .filter(|e| e.name.starts_with("wurth/"))
            .count(),
        14
    );
    for e in &fetchable {
        let commit = e.commit.as_deref().unwrap();
        assert!(e.url.contains(commit), "{}: {}", e.name, e.url);
    }
    for e in &m.entries {
        assert_eq!(e.sha256.len(), 64, "{}", e.name);
        assert!(e.sha256.bytes().all(|b| b.is_ascii_hexdigit()));
        if let Some(c) = &e.commit {
            assert_eq!(c.len(), 40, "{}", e.name);
        }
        assert!(e.bbox_min.iter().zip(&e.bbox_max).all(|(a, b)| a <= b));
    }
    let mut names: Vec<&str> = m.entries.iter().map(|e| e.name.as_str()).collect();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), 58);
    // FreeCAD gave a volume for every model but the SolveSpace one, which is open shells.
    let unmeasured: Vec<&str> = m
        .entries
        .iter()
        .filter(|e| e.volume.is_none())
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(unmeasured, ["ap203/multicolor_solvespace.step"]);
    // The trial's figures for one model (results/fc_ref.jsonl).
    let soic = m.get("kicad/SOIC-8_3.9x4.9mm_P1.27mm.step").unwrap();
    assert_eq!(soic.schema, "AUTOMOTIVE_DESIGN");
    assert!(soic.colours);
    assert_eq!(soic.volume, Some(30.187110));
    assert_eq!(soic.bbox_min, [-3.0, -2.45, 0.0]);
    assert_eq!(soic.bbox_max, [3.0, 2.45, 1.75]);
}

/// A manifest row for a fixture, with the reference figures given.
fn fixture_entry(file: &str, volume: Option<f64>, colours: bool) -> Entry {
    Entry {
        name: file.to_string(),
        url: String::new(),
        commit: None,
        sha256: String::new(),
        schema: "AUTOMOTIVE_DESIGN".to_string(),
        fetch: false,
        colours,
        volume,
        bbox_min: [0.0; 3],
        bbox_max: [10.0, 20.0, 5.0],
    }
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Runs one fixture through the corpus run, in its own process as the corpus does.
fn graded(entry: &Entry) -> Graded {
    let outcome = run::model(&fixtures().join(&entry.name), run::TIMEOUT);
    grade::grade(entry, outcome)
}

#[test]
fn a_model_meshed_whole_at_its_volume_and_closed_is_good() {
    let g = graded(&fixture_entry(
        "ap214_face_colours.step",
        Some(1000.0),
        true,
    ));
    assert_eq!(g.grade, Grade::Good, "{g:?}");
    assert!(g.every_face && g.right_volume && g.closed, "{g:?}");
    let m = g.measured().unwrap();
    assert_eq!((m.faces, m.faces_meshed), (6, 6));
    assert!((m.volume - 1000.0).abs() < 1e-6, "{}", m.volume);
    // Six faces, each its own colour.
    assert_eq!(m.colours, 6);
    assert_eq!(g.colours_read, Some(true));
    assert_eq!(m.protocol, "Ap214");
    assert!(g.bbox_off.unwrap() < 1e-9, "{:?}", g.bbox_off);
}

#[test]
fn a_solid_that_cannot_be_meshed_leaves_faces_unmeshed() {
    // Two blocks; the second's bottom lies on a sphere of negative radius.
    let g = graded(&fixture_entry("failing_solid.step", Some(2000.0), true));
    assert_eq!(g.grade, Grade::Partial, "{g:?}");
    assert!(!g.every_face && !g.right_volume);
    let m = g.measured().unwrap();
    assert_eq!((m.faces, m.faces_meshed), (12, 6));
    assert!(m.first_warning.contains("is skipped"), "{m:?}");
}

#[test]
fn a_volume_more_than_one_percent_off_is_graded_so() {
    let g = graded(&fixture_entry(
        "ap214_face_colours.step",
        Some(1011.0),
        true,
    ));
    assert_eq!(g.grade, Grade::Volume, "{g:?}");
    assert!(g.every_face && !g.right_volume);
    // Within 1 % passes.
    let g = graded(&fixture_entry(
        "ap214_face_colours.step",
        Some(1009.0),
        true,
    ));
    assert_eq!(g.grade, Grade::Good, "{g:?}");
    // With no reference volume the volume cannot be right, as in the trial.
    let g = graded(&fixture_entry("ap214_face_colours.step", None, true));
    assert_eq!(g.grade, Grade::Volume, "{g:?}");
}

#[test]
fn an_open_shell_is_not_closed_and_missing_colours_are_seen() {
    // Five faces of the block in an open shell: meshed whole but open.
    let g = graded(&fixture_entry("open_box.step", None, true));
    assert!(g.every_face && !g.measured().unwrap().closed, "{g:?}");
    // A cylinder the file gives no colour: read as having none.
    let g = graded(&fixture_entry("ap203_cylinder.step", None, true));
    assert_eq!(g.colours_read, Some(false), "{g:?}");
    assert_eq!(g.measured().unwrap().protocol, "Ap203");
}

/// `fetch.sh` on a manifest whose rows point at fixtures through `file://` URLs.
#[cfg(unix)]
#[test]
fn the_fetch_script_downloads_the_fetchable_rows_and_checks_their_hashes() {
    use std::process::Command;
    let work = std::env::temp_dir().join(format!("reda-step-corpus-fetch-{}", std::process::id()));
    std::fs::create_dir_all(&work).unwrap();
    let url = |f: &str| format!("file://{}", fixtures().join(f).display()).replace(' ', "%20");
    let row = |name: &str, file: &str, sha: &str, fetch: &str| {
        let cells = [
            name,
            &url(file),
            "0123456789abcdef0123456789abcdef01234567",
            sha,
            "AUTOMOTIVE_DESIGN",
            fetch,
            "yes",
            "1000.0",
            "0 0 0",
            "10 20 5",
        ];
        cells.join("\t")
    };
    // `sha256sum tests/fixtures/<file>`.
    let block = "2251d900987a7db83d40dbe235dd1b13120530c65f6c4a8d129b2eb2ca7c13f3";
    let sphere = "585f8373b8afb361e9edbcd50286a3a638e8fef9eccb5b218945f16025d32613";
    let head = "name\turl\tcommit\tsha256\tschema\tfetch\tcolours\tvolume\tbbox_min\tbbox_max";
    let good = row("lib/block.step", "ap214_face_colours.step", block, "yes");
    let wrong = row("lib/sphere.step", "sphere.step", block, "yes");
    let kept_back = row("vendor/sphere.step", "sphere.step", sphere, "no");
    let fetch = |manifest: &str| {
        let path = work.join("manifest.tsv");
        std::fs::write(&path, manifest).unwrap();
        let out = Command::new("bash")
            .arg(manifest::folder().join("fetch.sh"))
            .arg(work.join("corpus"))
            .arg(&path)
            .output()
            .unwrap();
        let said = String::from_utf8_lossy(&out.stdout).to_string()
            + &String::from_utf8_lossy(&out.stderr);
        (out.status.success(), said)
    };

    let (ok, said) = fetch(&[head, &good, &wrong, &kept_back, ""].join("\n"));
    assert!(
        !ok,
        "a file with the wrong hash must fail the fetch: {said}"
    );
    assert!(
        said.contains("lib/sphere.step has SHA-256 585f8373"),
        "{said}"
    );
    assert!(said.contains("fetched 1, kept 0, failed 1"), "{said}");
    let corpus = work.join("corpus");
    assert_eq!(
        std::fs::read(corpus.join("lib/block.step")).unwrap(),
        std::fs::read(fixtures().join("ap214_face_colours.step")).unwrap()
    );
    assert!(!corpus.join("lib/sphere.step").exists());
    assert!(!corpus.join("lib/sphere.step.part").exists());
    assert!(
        !corpus.join("vendor").exists(),
        "rows not marked fetch are left alone"
    );

    // A second run keeps what it has.
    let (ok, said) = fetch(&[head, &good, ""].join("\n"));
    assert!(ok, "{said}");
    assert!(said.contains("fetched 0, kept 1, failed 0"), "{said}");
    std::fs::remove_dir_all(&work).unwrap();
}

/// Figures for a model read and meshed, for grading without a file.
fn measured(faces_meshed: usize, volume: f64, closed: bool, colours: usize) -> run::Outcome {
    run::Outcome::Measured(run::Measured {
        secs: 0.5,
        faces: 10,
        faces_meshed,
        triangles: 100,
        volume,
        closed,
        colours,
        protocol: "Ap214".to_string(),
        bbox: Some(([0.0; 3], [10.0, 20.0, 5.0])),
        warnings: 0,
        first_warning: String::new(),
    })
}

#[test]
fn the_summary_counts_each_tier_as_the_trial_did() {
    let e = |name: &str| fixture_entry(name, Some(100.0), true);
    let graded = [
        grade::grade(&e("good.step"), measured(10, 100.5, true, 3)),
        // At the right volume but open, like the trial's TRS jack.
        grade::grade(&e("open.step"), measured(10, 99.8, false, 3)),
        grade::grade(&e("off.step"), measured(10, 90.0, true, 0)),
        grade::grade(&e("partial.step"), measured(9, 100.0, false, 2)),
        grade::grade(&e("refused.step"), run::Outcome::Refused("no".into())),
        grade::grade(&e("crash.step"), run::Outcome::Crash("boom".into())),
        grade::grade(&e("slow.step"), run::Outcome::Timeout(run::TIMEOUT)),
        grade::grade(&e("missing.step"), run::Outcome::Missing),
    ];
    let s = report::Summary::of(&graded);
    assert_eq!(
        s,
        report::Summary {
            models: 8,
            present: 7,
            read: 4,
            every_face: 3,
            right_volume: 2,
            closed: 1,
            // Of the models present whose files have colours, as the trial's "of 55".
            with_colours: 7,
            colours_read: 3,
            crashes: 1,
            timeouts: 1,
        }
    );
    let text = report::render(&graded);
    // One row per model, then the figures against OpenCascade's 54 of 58.
    for g in &graded {
        assert!(text.contains(&format!("| {} |", g.name)), "{text}");
    }
    assert!(text.contains("Right volume: 2 of 8"), "{text}");
    assert!(text.contains("54 of 58"), "{text}");
    assert!(text.contains("boom"), "{text}");
}
