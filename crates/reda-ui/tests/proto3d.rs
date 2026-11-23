//! PROTOTYPE (issue #433): draws the 3D view variants in the real window.
//! `REDA_PROTO_OUT=<dir> cargo test -p reda-ui --test proto3d -- --nocapture`

use std::path::{Path, PathBuf};

use egui_kittest::Harness;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::preferences;
use reda_ui::proto3d::{self, Preset};
use reda_ui::shell::Shell;
use serde_json::json;

fn window(root: &Path, theme: &str, project: &str) -> Harness<'static, App> {
    let _ = std::fs::remove_dir_all(root);
    let (home, work) = (root.join("home"), root.join("Projects").join(project));
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&work).unwrap();
    preferences::set(&UserDirs::under(&home), "general.theme", json!(theme)).unwrap();
    let from = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/projects")
        .join(project);
    let mut reda = None;
    for entry in std::fs::read_dir(&from).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            let target = work.join(path.file_name().unwrap());
            std::fs::copy(&path, &target).unwrap();
            if path.extension().is_some_and(|e| e == "reda") {
                reda = Some(target);
            }
        }
    }
    let engine = Engine::builder()
        .ids(SeededIds::new(0x5ca1e))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(&home)))
        .starter_library(None)
        .headless(false)
        .build();
    let render_state = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    reda_ui::install_canvas(&render_state, 1);
    let mut h = Harness::builder()
        .with_size([1600.0, 1000.0])
        .with_accessibility_check(false)
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(
            render_state,
        ))
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(Shell::new(engine)));
    h.run();
    h.state_mut()
        .shell_mut()
        .open_project(&reda.unwrap())
        .unwrap();
    h.run();
    let pcb = {
        let project = h.state().shell().engine().project().unwrap();
        project.pcb().unwrap().uuid.to_string()
    };
    let shell = h.state_mut().shell_mut();
    shell
        .run("document.open", json!({"document": pcb}))
        .unwrap();
    h.run();
    h
}

/// Selects the first Footprint whose Designator starts with `prefix`.
fn select(h: &mut Harness<'static, App>, prefix: &str) {
    let uuid = {
        let project = h.state().shell().engine().project().unwrap();
        let pcb = project.pcb().unwrap();
        pcb.footprints()
            .find(|f| f.designator.starts_with(prefix))
            .or_else(|| pcb.footprints().next())
            .map(|f| f.uuid)
    };
    if let Some(uuid) = uuid {
        let shell = h.state_mut().shell_mut();
        shell
            .run("selection.set", json!({"objects": [uuid]}))
            .unwrap();
    }
}

#[test]
fn variants() {
    let Some(out) = std::env::var_os("REDA_PROTO_OUT").map(PathBuf::from) else {
        eprintln!("REDA_PROTO_OUT is not set; nothing drawn");
        return;
    };
    std::fs::create_dir_all(&out).unwrap();
    let root = std::env::temp_dir().join(format!("reda-proto3d-{}", std::process::id()));
    // (file name, variant, project, theme, what to set up once the view is open)
    type Setup = fn(&mut Harness<'static, App>);
    let shots: Vec<(&str, usize, &str, &str, Setup)> = vec![
        ("a1-arrived", 0, "usb_c_mcu", "light", |_| {}),
        ("a2-top-measure", 0, "usb_c_mcu", "light", |_| {
            proto3d::with(|s| {
                (s.cam.yaw, s.cam.pitch) = Preset::Top.angles();
                s.measure.on = true;
                s.measure.a = Some([-8.0, -4.0, 0.8]);
                s.measure.b = Some([9.0, 5.0, 0.8]);
            })
        }),
        ("a3-bottom-dark", 0, "usb_c_mcu", "dark", |h| {
            select(h, "U");
            proto3d::with(|s| (s.cam.yaw, s.cam.pitch) = (2.6, -0.8))
        }),
        ("a4-four-layer-close", 0, "four_layer", "light", |_| {
            proto3d::with(|s| {
                s.cam.dist *= 0.45;
                s.cam.pitch = 0.5;
            })
        }),
        ("b1-arrived-selected", 1, "usb_c_mcu", "light", |h| {
            select(h, "U")
        }),
        ("b2-faded-measure", 1, "usb_c_mcu", "light", |_| {
            proto3d::with(|s| {
                s.alpha[4] = 0.35;
                s.measure.on = true;
                s.measure.a = Some([-8.0, -4.0, 0.8]);
                s.measure.b = Some([9.0, 5.0, 2.4]);
            })
        }),
        ("b3-dark-low", 1, "usb_c_mcu", "dark", |h| {
            select(h, "J");
            proto3d::with(|s| (s.cam.yaw, s.cam.pitch) = (0.5, 0.3))
        }),
        ("b4-no-mask", 1, "six_layer_hdi", "light", |_| {
            proto3d::with(|s| s.look.mask = false)
        }),
        ("c1-arrived", 2, "usb_c_mcu", "dark", |_| {}),
        ("c2-spread-selected", 2, "four_layer", "dark", |h| {
            select(h, "U");
            proto3d::with(|s| {
                s.explode = 0.8;
                s.cam.pitch = 0.45;
            })
        }),
        ("c3-cut-measure", 2, "six_layer_hdi", "light", |_| {
            proto3d::with(|s| {
                s.cut = 0.5;
                s.explode = 0.5;
                s.cam.pitch = 0.4;
                s.cam.yaw = -0.9;
                s.measure.on = true;
                s.measure.a = Some([-6.0, -3.0, 0.8]);
                s.measure.b = Some([-1.0, 4.0, -0.8]);
            })
        }),
    ];
    let only = std::env::var("REDA_PROTO_ONLY").ok();
    for (name, variant, project, theme, setup) in shots {
        if only.as_ref().is_some_and(|o| !name.starts_with(o.as_str())) {
            continue;
        }
        proto3d::set_on(false);
        let mut h = window(&root, theme, project);
        proto3d::set_variant(variant);
        if name.starts_with("b1") {
            // The way in, over the 2D canvas.
            h.run();
            h.render()
                .unwrap()
                .save(out.join("b0-2d-with-switch.png"))
                .unwrap();
        }
        proto3d::set_on(true);
        h.run();
        setup(&mut h);
        h.run();
        h.run();
        h.render()
            .unwrap()
            .save(out.join(format!("{name}.png")))
            .unwrap();
        eprintln!("drew {name}");
    }
    let _ = std::fs::remove_dir_all(&root);
}

/// The camera's maths agrees with itself: a point on the ray under a pixel lands back on
/// that pixel, and turning about a pivot leaves the pivot where it was on screen.
#[test]
fn camera_round_trips() {
    use reda_ui::proto3d::camera::{Cam, add, scale};
    let rect = egui::Rect::from_min_size(egui::pos2(300.0, 80.0), egui::vec2(980.0, 650.0));
    for ortho in [true, false] {
        let mut cam = Cam {
            target: [3.0, -2.0, 0.5],
            yaw: -0.6,
            pitch: 0.7,
            dist: 120.0,
            ortho,
        };
        let pos = egui::pos2(700.0, 300.0);
        let (o, d) = cam.ray(rect, pos);
        let p = add(o, scale(d, 90.0));
        assert!(
            cam.project(rect, p).unwrap().distance(pos) < 0.05,
            "ray and project"
        );
        cam.orbit(0.4, -0.3, Some(p));
        assert!(
            cam.project(rect, p).unwrap().distance(pos) < 0.05,
            "pivot stays"
        );
        cam.zoom(rect, pos, 0.5);
        let (o, d) = cam.ray(rect, pos);
        let q = add(o, scale(d, 45.0));
        assert!(
            cam.project(rect, q).unwrap().distance(pos) < 0.05,
            "zoom keeps the pixel"
        );
    }
}
