//! PROTOTYPE (issue #330): draws the Stackup editor variants in the real window.
//! `REDA_PROTO_OUT=<dir> cargo test -p reda-ui --test stackup_proto -- --nocapture`

use std::path::{Path, PathBuf};

use egui_kittest::Harness;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::preferences;
use reda_ui::proto::{self, model};
use reda_ui::shell::Shell;
use serde_json::json;

fn window(root: &Path, theme: &str) -> Harness<'static, App> {
    let _ = std::fs::remove_dir_all(root);
    let (home, work) = (root.join("home"), root.join("Projects/usb_c_mcu"));
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&work).unwrap();
    preferences::set(&UserDirs::under(&home), "general.theme", json!(theme)).unwrap();
    let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/projects/usb_c_mcu");
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
    h.state_mut().shell_mut().open_project(&reda.unwrap()).unwrap();
    h.run();
    let pcb = {
        let project = h.state().shell().engine().project().unwrap();
        project.pcb().unwrap().uuid.to_string()
    };
    let shell = h.state_mut().shell_mut();
    shell.run("document.open", json!({"document": pcb})).unwrap();
    h.run();
    h
}

#[test]
fn variants() {
    let Some(out) = std::env::var_os("REDA_PROTO_OUT").map(PathBuf::from) else {
        eprintln!("REDA_PROTO_OUT is not set; nothing drawn");
        return;
    };
    std::fs::create_dir_all(&out).unwrap();
    let root = std::env::temp_dir().join(format!("reda-stackup-proto-{}", std::process::id()));
    proto::set_on(true);
    // (file name, variant, sample, to scale, what is selected, theme)
    let shots: [(&str, usize, usize, bool, &str, &str); 15] = [
        ("a1-4-layer-schematic", 0, 1, false, "entry:2", "light"),
        ("a2-4-layer-to-scale", 0, 1, true, "entry:1", "light"),
        ("a3-8-layer-hdi-via", 0, 3, false, "via:2", "light"),
        ("a4-6-layer-dark", 0, 2, true, "entry:4", "dark"),
        ("b1-4-layer", 1, 1, false, "entry:2", "light"),
        ("b2-8-layer-hdi-via", 1, 3, false, "via:2", "light"),
        ("b3-6-layer-dark", 1, 2, false, "entry:4", "dark"),
        ("c1-4-layer-to-scale", 2, 1, true, "entry:2", "light"),
        ("c2-8-layer-hdi-via", 2, 3, true, "via:2", "light"),
        ("c3-6-layer-schematic-dark", 2, 2, false, "entry:4", "dark"),
        ("c4-2-layer", 2, 0, true, "none", "light"),
        ("d1-4-layer-to-scale", 3, 1, true, "entry:2", "light"),
        ("d2-8-layer-hdi-via", 3, 3, true, "via:2", "light"),
        ("d3-6-layer-schematic", 3, 2, false, "via:0", "light"),
        ("d4-6-layer-dark", 3, 2, true, "entry:4", "dark"),
    ];
    for (name, variant, sample, to_scale, sel, theme) in shots {
        let mut h = window(&root, theme);
        proto::set_variant(variant);
        proto::load(model::PRESETS[sample]);
        proto::with(|s, v| {
            v.to_scale = to_scale;
            s.sel = match sel.split_once(':') {
                Some(("entry", i)) => model::Sel::Entry(s.entries[i.parse::<usize>().unwrap()].id),
                Some(("via", j)) => model::Sel::Via(j.parse().unwrap()),
                _ => model::Sel::None,
            };
        });
        h.run();
        h.run();
        h.render().unwrap().save(out.join(format!("{name}.png"))).unwrap();
        eprintln!("drew {name}");
    }
    let _ = std::fs::remove_dir_all(&root);
}
