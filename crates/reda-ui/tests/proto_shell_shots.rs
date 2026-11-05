//! PROTOTYPE. Draws the shell's presets to PNGs through the real window. REDA_SHOT_OUT picks
//! the folder.

use std::path::{Path, PathBuf};

use egui_kittest::Harness;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::proto_shell::{self, PRESETS};
use reda_ui::shell::Shell;
use serde_json::json;

fn copy(from: &Path, to: &Path) -> PathBuf {
    std::fs::create_dir_all(to).unwrap();
    let mut reda = None;
    for e in std::fs::read_dir(from).unwrap() {
        let p = e.unwrap().path();
        if p.is_file() {
            let t = to.join(p.file_name().unwrap());
            std::fs::copy(&p, &t).unwrap();
            if p.extension().is_some_and(|x| x == "reda") {
                reda = Some(t);
            }
        }
    }
    reda.unwrap()
}

fn settle(h: &mut Harness<'static, App>) {
    for _ in 0..12 {
        h.step();
    }
}

fn shot(h: &mut Harness<'static, App>, out: &Path, name: &str) {
    settle(h);
    let image = h.render().unwrap();
    image.save(out.join(format!("{name}.png"))).unwrap();
}

#[test]
fn shots() {
    let Ok(out) = std::env::var("REDA_SHOT_OUT") else {
        return;
    };
    let out = PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    let root = out.join("stage");
    let _ = std::fs::remove_dir_all(&root);
    let home = root.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let reda = copy(&repo.join("tests/projects/usb_c_mcu"), &root.join("p"));
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(&home)))
        .starter_library(None)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.open_project(&reda).unwrap();
    let render_state = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    reda_ui::install_canvas(&render_state, 1);
    let app = App::new(shell).with_render_state(render_state.clone());
    let mut h = Harness::builder()
        .with_size([1500.0, 950.0])
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(
            render_state,
        ))
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    let pcb = {
        let p = h.state().shell().engine().project().unwrap();
        p.pcb().unwrap().uuid.to_string()
    };
    let _ = h.state_mut().shell_mut().run("pcb.zone.fill", json!({}));
    let _ = h
        .state_mut()
        .shell_mut()
        .run("document.open", json!({ "document": pcb }));
    settle(&mut h);
    let _ = h.state_mut().shell_mut().run("view.zoom_fit", json!({}));
    // The single proposals, on preset A.
    proto_shell::with(|s| s.axes = PRESETS[0].2);
    settle(&mut h);
    let _ = h
        .state_mut()
        .shell_mut()
        .run("tool.activate:route", json!({}));
    proto_shell::with(|s| s.key_card = true);
    shot(&mut h, &out, "x-route-keycard");
    proto_shell::with(|s| s.key_card = false);
    let _ = h
        .state_mut()
        .shell_mut()
        .run("tool.activate:select", json!({}));
    let _ = h.state_mut().shell_mut().run("panel.show:board", json!({}));
    shot(&mut h, &out, "x-navigator");
    let _ = h
        .state_mut()
        .shell_mut()
        .run("app.dialog.open:command_palette", json!({}));
    shot(&mut h, &out, "x-palette");
    h.key_press(egui::Key::Escape);
    settle(&mut h);
    let _ = h
        .state_mut()
        .shell_mut()
        .run("app.dialog.open:board_setup", json!({}));
    settle(&mut h);
    h.state_mut().settings_page("Stackup");
    shot(&mut h, &out, "x-stackup");
    proto_shell::with(|s| s.on_board = true);
    shot(&mut h, &out, "x-on-board");
    proto_shell::with(|s| s.on_board = false);
    h.state_mut().close_settings();
    settle(&mut h);
    for (key, _, axes) in PRESETS {
        proto_shell::with(|s| s.axes = axes);
        let name = format!("{key}");
        shot(&mut h, &out, &format!("{name}-pcb"));
        let _ = h
            .state_mut()
            .shell_mut()
            .run("app.dialog.open:preferences", json!({}));
        settle(&mut h);
        shot(&mut h, &out, &format!("{name}-prefs"));
        proto_shell::with(|s| s.prefs_modal = false);
        let _ = h
            .state_mut()
            .shell_mut()
            .run("layout.tab.close", json!({"tab": "preferences"}));
        let _ = h
            .state_mut()
            .shell_mut()
            .run("document.open", json!({ "document": pcb }));
        let _ = h
            .state_mut()
            .shell_mut()
            .run("app.dialog.open:board_setup", json!({}));
        settle(&mut h);
        shot(&mut h, &out, &format!("{name}-setup"));
        let _ = h
            .state_mut()
            .shell_mut()
            .run("layout.tab.close", json!({"tab": "board_setup"}));
        proto_shell::with(|s| s.setup = None);
        h.state_mut().close_settings();
        let _ = h
            .state_mut()
            .shell_mut()
            .run("document.open", json!({ "document": pcb }));
    }
}
