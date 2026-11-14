//! Throwaway: the before picture for the theme baseline (issue #270), built on the showcase harness.
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

/// The fixed scene list.
const SCENES: &[Scene] = &[
    Scene { id: "01-start", title: "Start page", caption: "", show: start_page },
    Scene { id: "02-schematic", title: "Schematic editor", caption: "", show: schematic_selected },
    Scene { id: "03-pcb", title: "PCB editor with Layers", caption: "", show: usb_c_pcb },
    Scene { id: "04-menu", title: "Main menu open", caption: "", show: menu_open },
    Scene { id: "05-palette", title: "Command Palette", caption: "", show: palette },
    Scene { id: "06-tab-menu", title: "Tab context menu with submenu", caption: "", show: tab_menu },
    Scene { id: "07-collapsed-dock", title: "Collapsed Dock with a flyout", caption: "", show: flyout },
    Scene { id: "08-preferences", title: "Preferences", caption: "", show: preferences_window },
    Scene { id: "09-project-settings", title: "Project Settings", caption: "", show: project_settings },
    Scene { id: "10-board-setup", title: "Board Setup", caption: "", show: board_setup },
    Scene { id: "11-output-job", title: "Output Job", caption: "", show: output_job },
    Scene { id: "12-library", title: "Library panel and Part editor", caption: "", show: library },
    Scene { id: "13-messages", title: "Messages panel after ERC", caption: "", show: checks },
    Scene { id: "14-focus-tab-switcher", title: "Keyboard focus and tab switcher", caption: "", show: tab_switcher },
    Scene { id: "15-params-form", title: "Command params form (modal)", caption: "", show: params_form },
    Scene { id: "16-tooltip", title: "Tool tooltip", caption: "", show: tooltip },
];

/// The folder the scenes' Projects and per-user state are staged in: `REDA_SHOWCASE_ROOT`
/// when set; for the website's images (`REDA_SHOWCASE_OUT`), a fixed path, so the paths the
/// window shows are the same on every machine; otherwise a folder of this run's own under
/// the temp directory, so runs from two checkouts never share one.
fn stage_root() -> PathBuf {
    static ROOT: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| {
        if let Some(root) = std::env::var_os("REDA_SHOWCASE_ROOT") {
            PathBuf::from(root)
        } else if std::env::var_os("REDA_SHOWCASE_OUT").is_some() {
            PathBuf::from("/tmp/reda-chrome-before")
        } else {
            std::env::temp_dir().join(format!("reda-chrome-before-{}", std::process::id()))
        }
    })
    .clone()
}

/// An exclusive lock on `<stage root>.lock` when the stage is a fixed folder, held while
/// the scenes are drawn, so another run staging into it waits instead of deleting it under
/// this one. A folder of this run's own needs none.
fn lock_stage() -> Option<std::fs::File> {
    if std::env::var_os("REDA_SHOWCASE_ROOT").is_none()
        && std::env::var_os("REDA_SHOWCASE_OUT").is_none()
    {
        return None;
    }
    let root = stage_root();
    let parent = root.parent().expect("the stage root has a parent");
    std::fs::create_dir_all(parent).unwrap();
    let mut name = root.file_name().unwrap().to_owned();
    name.push(".lock");
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(parent.join(name))
        .unwrap();
    lock.lock().unwrap();
    Some(lock)
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

#[allow(dead_code)]
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

fn schematic_selected(stage: &Stage) -> Harness<'static, App> {
    let mut h = blinky_schematic(stage);
    run(&mut h, "panel.show", json!({"panel": "properties"}));
    h.run();
    h
}

fn menu_open(stage: &Stage) -> Harness<'static, App> {
    let mut h = blinky_schematic(stage);
    h.get_by_role_and_label(Role::Button, "View").click();
    h.run();
    h
}

fn palette(stage: &Stage) -> Harness<'static, App> {
    let mut h = blinky_schematic(stage);
    h.key_press_modifiers(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::P);
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Command Palette").type_text("show");
    h.run();
    h
}

fn tab_menu(stage: &Stage) -> Harness<'static, App> {
    let mut h = blinky_schematic(stage);
    h.get_by_label("Projects").click_secondary();
    h.run();
    h.get_by_label("Move to").hover();
    h.run();
    h
}

fn flyout(stage: &Stage) -> Harness<'static, App> {
    let mut h = blinky_schematic(stage);
    h.get_by_label("Projects").click_secondary();
    h.run();
    h.get_by_label("Collapse Dock").click();
    h.run();
    h.get_by_role_and_label(Role::Button, "Projects").click();
    h.run();
    h
}

fn preferences_window(stage: &Stage) -> Harness<'static, App> {
    let mut h = blinky_schematic(stage);
    h.state_mut().open_preferences(None);
    h.run();
    h
}

fn project_settings(stage: &Stage) -> Harness<'static, App> {
    let mut h = blinky_schematic(stage);
    h.state_mut().open_project_settings();
    h.run();
    h.get_by_role_and_label(Role::TreeItem, "ERC").click();
    h.run();
    h
}

fn board_setup(stage: &Stage) -> Harness<'static, App> {
    let mut h = stage.window_on("usb_c_mcu", false);
    show_pcb(&mut h);
    h.state_mut().open_board_setup();
    h.run();
    h.get_by_role_and_label(Role::TreeItem, "Net Classes").click();
    h.run();
    h
}

fn tab_switcher(stage: &Stage) -> Harness<'static, App> {
    let mut h = blinky_schematic(stage);
    h.key_press(egui::Key::F6);
    h.run();
    h.event(egui::Event::ModifiersChanged(egui::Modifiers::CTRL));
    for pressed in [true, false] {
        h.event(egui::Event::Key {
            key: egui::Key::Tab,
            pressed,
            modifiers: egui::Modifiers::CTRL,
            repeat: false,
            physical_key: None,
        });
    }
    h.run();
    h
}

fn params_form(stage: &Stage) -> Harness<'static, App> {
    let mut h = blinky_schematic(stage);
    h.key_press_modifiers(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::P);
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Command Palette").type_text("split tab");
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    h
}

fn tooltip(stage: &Stage) -> Harness<'static, App> {
    let mut h = blinky_schematic(stage);
    h.get_by_role_and_label(Role::Button, "Wire").hover();
    for _ in 0..90 {
        h.step();
    }
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
fn contrast() {
    println!("{}", reda_ui::theme::contrast_report());
}

/// PROTOTYPE (issue #272): a handful of scenes, per direction × theme × Density.
#[test]
fn directions() {
    use reda_ui::theme::{self, Direction};
    if let Some(why) = skip_reason() {
        panic!("the directions cannot be drawn: {why}");
    }
    let out = PathBuf::from(std::env::var_os("REDA_SHOWCASE_OUT").expect("REDA_SHOWCASE_OUT"));
    std::fs::create_dir_all(&out).unwrap();
    let only = std::env::var("REDA_PROTO_SCENES").ok();
    let scenes: Vec<&Scene> = SCENES
        .iter()
        .filter(|s| match &only {
            Some(list) => list.split(',').any(|id| s.id.starts_with(id)),
            None => true,
        })
        .collect();
    let looks = [
        ("a-graphite", Direction::Graphite),
        ("b-paper", Direction::Paper),
        ("c-ink", Direction::Ink),
        ("z-current", Direction::Current),
    ];
    let only_dir = std::env::var("REDA_PROTO_ONLY").ok();
    for (name, direction) in looks {
        if only_dir.as_ref().is_some_and(|d| !name.contains(d.as_str())) {
            continue;
        }
        for (density, compact) in [("comfortable", false), ("compact", true)] {
            if compact && (direction == Direction::Current || std::env::var_os("REDA_PROTO_COMPACT").is_none()) {
                continue;
            }
            for scene in &scenes {
                for theme_name in THEMES {
                    let stage = Stage::new(theme_name);
                    // The statics are read when the window installs its theme.
                    let ctx = egui::Context::default();
                    theme::set_look(&ctx, direction, compact);
                    let accent = std::env::var("REDA_PROTO_ACCENT").unwrap_or_default();
                    let index = theme::ACCENTS
                        .iter()
                        .position(|a| a.eq_ignore_ascii_case(&accent))
                        .unwrap_or(0);
                    theme::set_accent(&ctx, index);
                    let mut h = (scene.show)(&stage);
                    h.run();
                    let image = h.render().unwrap();
                    let tag = if accent.is_empty() { String::new() } else { format!("-{}", accent.to_lowercase()) };
                    let file = format!("{}-{name}{tag}-{theme_name}-{density}.png", scene.id);
                    image.save(out.join(&file)).unwrap();
                    eprintln!("drew {file}");
                }
            }
        }
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
        None => Path::new(env!("CARGO_TARGET_TMPDIR")).join("chrome-before"),
    };
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();
    let _stage = lock_stage();

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
