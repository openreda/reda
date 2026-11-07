//! The showcase: the screenshots on REDA's website, drawn by the real window.
//!
//! Each scene opens a copy of a reference Project (never the files in `tests/projects/`,
//! which opening would write a `.reda/` cache into), drives the window through Commands
//! and AccessKit, and renders it through wgpu at 1600×1000, once per theme. UUIDs and time
//! are seeded and fixed, and the copies sit at fixed paths, so a commit always draws the
//! same images on Mesa's llvmpipe.
//!
//! `cargo xtask showcase --out <dir>` runs this test with `REDA_SHOWCASE_OUT=<dir>`, and the
//! PNGs and `manifest.json` land there; `cargo xtask site` builds the website from them.
//! Without `REDA_SHOWCASE_OUT` the images go to a scratch folder, so the test only checks
//! that every scene renders. Like the PNG snapshots it runs on llvmpipe only, and is
//! skipped elsewhere unless `REDA_PNG_SNAPSHOTS` is set.

use std::path::{Path, PathBuf};

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::model::geom::{Nm, Point};
use reda_ui::engine::{preferences, state};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const SIZE: [f32; 2] = [1600.0, 1000.0];
const THEMES: [&str; 2] = ["dark", "light"];

/// One screenshot on the website, drawn in each theme.
struct Scene {
    id: &'static str,
    title: &'static str,
    caption: &'static str,
    /// Opens a fresh window on the stage and drives it to the moment to capture.
    show: fn(&Stage) -> Harness<'static, App>,
}

/// The fixed scene list, in the order the website shows it.
const SCENES: &[Scene] = &[
    Scene {
        id: "start",
        title: "Start page",
        caption: "Open a recent Project or start a new one.",
        show: start_page,
    },
    Scene {
        id: "schematic",
        title: "Schematic",
        caption: "A 555 LED blinker drawn on a Schematic Sheet.",
        show: blinky_schematic,
    },
    Scene {
        id: "pcb",
        title: "PCB layout",
        caption: "The 555 blinker laid out on a 2-layer board, with its ground Zone filled.",
        show: blinky_pcb,
    },
    Scene {
        id: "pcb-usb-c",
        title: "USB-C microcontroller board",
        caption: "A denser board, with Footprints on both sides and copper Zones.",
        show: usb_c_pcb,
    },
    Scene {
        id: "output-job",
        title: "Outputs for the fab",
        caption: "One Output Job writes Gerber X2, Excellon drill, a BOM and a CPL, zipped.",
        show: output_job,
    },
    Scene {
        id: "library",
        title: "Library and Part editor",
        caption: "Browse the starter Library and edit a Part's Symbol, Footprints and fields.",
        show: library,
    },
    Scene {
        id: "checks",
        title: "ERC and DRC",
        caption: "ERC and DRC list each Violation in the Messages panel and mark it on the canvas.",
        show: checks,
    },
];

/// The folder the scenes' Projects and per-user state are staged in.
fn stage_root() -> PathBuf {
    std::env::var_os("REDA_SHOWCASE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp/reda-showcase"))
}

/// Where a scene's window lives: its per-user folder (holding the theme) and its Projects'
/// folder.
struct Stage {
    home: PathBuf,
    work: PathBuf,
}

impl Stage {
    /// A fresh stage at a fixed path, so the paths the window shows are the same on every
    /// machine (the showcase is drawn on Linux). Scenes are drawn one at a time, so each
    /// takes the folder over.
    fn new(theme: &'static str) -> Stage {
        let root = stage_root();
        let _ = std::fs::remove_dir_all(&root);
        let (home, work) = (root.join("home"), root.join("Projects"));
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&work).unwrap();
        preferences::set(&UserDirs::under(&home), "general.theme", json!(theme)).unwrap();
        Stage { home, work }
    }

    /// Copies the reference Project in `tests/projects/<name>` into the stage and returns
    /// its `.reda` file.
    fn project(&self, name: &str) -> PathBuf {
        let from = repo().join("tests/projects").join(name);
        let to = self.work.join(name);
        std::fs::create_dir_all(&to).unwrap();
        let mut reda = None;
        for entry in std::fs::read_dir(&from).unwrap() {
            let path = entry.unwrap().path();
            if path.is_file() {
                let target = to.join(path.file_name().unwrap());
                std::fs::copy(&path, &target).unwrap();
                if path.extension().is_some_and(|e| e == "reda") {
                    reda = Some(target);
                }
            }
        }
        reda.unwrap_or_else(|| panic!("no .reda file in {}", from.display()))
    }

    /// A window on a deterministic engine, rendering through wgpu with the canvas.
    fn window(&self, starter_library: bool) -> Harness<'static, App> {
        let engine = Engine::builder()
            // Seeded apart from the reference Projects, whose UUIDs count from seed 0.
            .ids(SeededIds::new(0x5ca1e))
            .clock(FixedClock::EPOCH)
            .user_dirs(Some(UserDirs::under(&self.home)))
            .starter_library(starter_library.then(|| repo().join("library/starter")))
            .headless(false)
            .build();
        let render_state = egui_kittest::wgpu::create_render_state(
            egui_kittest::wgpu::default_wgpu_setup(),
            egui_wgpu::RendererOptions::PREDICTABLE,
        );
        reda_ui::install_canvas(&render_state, 1);
        let mut h = Harness::builder()
            .with_size(SIZE)
            .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(
                render_state,
            ))
            .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(Shell::new(engine)));
        h.run();
        h
    }

    /// A window with the Project `name` open.
    fn window_on(&self, name: &str, starter_library: bool) -> Harness<'static, App> {
        let file = self.project(name);
        let mut h = self.window(starter_library);
        h.state_mut().shell_mut().open_project(&file).unwrap();
        h.run();
        h
    }
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run(h: &mut Harness<'static, App>, id: &str, params: Value) -> Value {
    h.state_mut()
        .shell_mut()
        .run(id, params)
        .unwrap_or_else(|e| panic!("{id}: {e:?}"))
}

/// The UUID of the open Project's PCB Document.
fn pcb(h: &Harness<'static, App>) -> String {
    let project = h.state().shell().engine().project().unwrap();
    project.pcb().unwrap().uuid.to_string()
}

/// Shows the PCB Document with its Zones filled, fitted to the canvas.
fn show_pcb(h: &mut Harness<'static, App>) {
    let pcb = pcb(h);
    run(h, "pcb.zone.fill", json!({}));
    run(h, "document.open", json!({"document": pcb}));
    h.run();
    run(h, "view.zoom_fit", json!({}));
    h.run();
}

fn start_page(stage: &Stage) -> Harness<'static, App> {
    let recent: Vec<PathBuf> = ["blinky_555", "usb_c_mcu", "minimal_board"]
        .into_iter()
        .map(|name| stage.project(name))
        .collect();
    state::update(&UserDirs::under(&stage.home), |s| {
        s.recent_projects = recent
    })
    .unwrap();
    stage.window(false)
}

fn blinky_schematic(stage: &Stage) -> Harness<'static, App> {
    let mut h = stage.window_on("blinky_555", false);
    zoom_to_circuit(&mut h);
    h
}

/// Zooms the active sheet's canvas to its circuit (its Units, Wires and Net Labels), with
/// a margin, rather than to the whole sheet frame.
fn zoom_to_circuit(h: &mut Harness<'static, App>) {
    let shell = h.state_mut().shell_mut();
    let document = shell.active_document().expect("a sheet tab").document;
    let sheet = shell
        .engine()
        .project()
        .unwrap()
        .schematic_sheet(document)
        .unwrap();
    let objects: Vec<_> = (sheet.units().map(|u| u.uuid))
        .chain(sheet.wires().map(|w| w.uuid))
        .chain(sheet.net_labels().map(|l| l.uuid))
        .collect();
    let tab = shell.document_tab_mut(document).unwrap();
    let (lo, hi) = reda_ui::reveal::frame(&tab.scene.scene, &objects).expect("a circuit");
    let margin = Nm((hi.x.0 - lo.x.0).max(hi.y.0 - lo.y.0) / 8);
    tab.zoom_to = Some((
        Point::new(lo.x - margin, lo.y - margin),
        Point::new(hi.x + margin, hi.y + margin),
    ));
    h.run();
}

fn blinky_pcb(stage: &Stage) -> Harness<'static, App> {
    let mut h = stage.window_on("blinky_555", false);
    show_pcb(&mut h);
    h
}

fn usb_c_pcb(stage: &Stage) -> Harness<'static, App> {
    let mut h = stage.window_on("usb_c_mcu", false);
    show_pcb(&mut h);
    run(&mut h, "panel.show", json!({"panel": "layers"}));
    h.run();
    h
}

fn output_job(stage: &Stage) -> Harness<'static, App> {
    let mut h = stage.window_on("blinky_555", false);
    show_pcb(&mut h);
    let job = {
        let project = h.state().shell().engine().project().unwrap();
        project.output_jobs().next().unwrap().uuid
    };
    h.state_mut().open_output_job(job);
    h.run();
    h.get_by_role_and_label(Role::Button, "Run").click();
    let start = std::time::Instant::now();
    while h.query_by_label_contains("finished").is_none() {
        for stuck in ["did not run", "failed"] {
            assert!(
                h.query_by_label_contains(stuck).is_none(),
                "the Output Job {stuck}"
            );
        }
        assert!(
            start.elapsed().as_secs() < 60,
            "the Output Job never finished"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
        h.step();
    }
    h.event(egui::Event::PointerGone);
    h.run();
    h
}

/// The starter Library's 555 timer Part.
const TIMER_555: &str = "6ad2a4fb-15fe-4c32-b244-29a4bee87635";

fn library(stage: &Stage) -> Harness<'static, App> {
    let mut h = stage.window_on("blinky_555", true);
    run(&mut h, "pcb.zone.fill", json!({}));
    run(&mut h, "panel.show", json!({"panel": "library"}));
    run(
        &mut h,
        "library.element.open",
        json!({"element": TIMER_555}),
    );
    h.run();
    h.run();
    h
}

fn checks(stage: &Stage) -> Harness<'static, App> {
    let mut h = stage.window_on("blinky_555", false);
    let sheet = h.state().shell().active_document().unwrap().document;
    // A stray Wire whose ends touch nothing, for ERC to find; on the connection grid.
    run(
        &mut h,
        "schematic.wire.add",
        json!({"document": sheet,
               "points": [{"x": "800mil", "y": "800mil"}, {"x": "1600mil", "y": "800mil"}]}),
    );
    run(&mut h, "pcb.zone.fill", json!({}));
    zoom_to_circuit(&mut h);
    run(&mut h, "panel.show", json!({"panel": "messages"}));
    h.run();
    h.get_all_by_label("Run ERC").last().unwrap().click();
    h.run();
    h.get_by_label("Sheet 1 (2)");
    h.event(egui::Event::PointerGone);
    h.run();
    h
}

/// Why the showcase cannot be drawn here, if it cannot.
fn skip_reason() -> Option<String> {
    if !cfg!(target_os = "linux") {
        return Some("the showcase is drawn on Linux only".into());
    }
    match reda_ui::engine::render::gpu::Headless::new() {
        Ok(h) if h.adapter().name.contains("llvmpipe") => None,
        Ok(h) => Some(format!(
            "the adapter is {:?}, not Mesa's llvmpipe (set LIBGL_ALWAYS_SOFTWARE=1 WGPU_BACKEND=gl)",
            h.adapter().name
        )),
        Err(e) => Some(e.to_string()),
    }
}

#[test]
fn every_scene_renders_in_both_themes() {
    if let Some(why) = skip_reason() {
        if std::env::var_os("REDA_PNG_SNAPSHOTS").is_some() {
            panic!("REDA_PNG_SNAPSHOTS is set but the showcase cannot be drawn: {why}");
        }
        eprintln!("skipping the showcase: {why}");
        return;
    }
    let out = match std::env::var_os("REDA_SHOWCASE_OUT") {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(env!("CARGO_TARGET_TMPDIR")).join("showcase"),
    };
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();

    let mut entries = Vec::new();
    for scene in SCENES {
        for theme in THEMES {
            let stage = Stage::new(theme);
            let mut h = (scene.show)(&stage);
            let image = h
                .render()
                .unwrap_or_else(|e| panic!("{}-{theme}: {e}", scene.id));
            assert_eq!(
                (image.width(), image.height()),
                (SIZE[0] as u32, SIZE[1] as u32),
                "{}-{theme}",
                scene.id
            );
            let first = image.get_pixel(0, 0);
            assert!(
                image.pixels().any(|p| p != first),
                "{}-{theme} rendered blank",
                scene.id
            );
            let file = format!("{}-{theme}.png", scene.id);
            image.save(out.join(&file)).unwrap();
            entries.push(json!({
                "id": scene.id,
                "title": scene.title,
                "caption": scene.caption,
                "theme": theme,
                "file": file,
            }));
            drop(h);
        }
    }
    let _ = std::fs::remove_dir_all(stage_root());
    let manifest = json!({ "scenes": entries });
    std::fs::write(
        out.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).unwrap() + "\n",
    )
    .unwrap();
}
