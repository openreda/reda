//! Every Panel in the window, for a reviewer: the nine Panels with something in them, in
//! light and dark and both Densities. Written only when `REDA_GALLERY_OUT` is set, as
//! `panel_<panel>_<theme>_<density>.png`; nothing compares them. They are how wrapped or
//! clipped text in a Panel is looked for after a change to the metrics.
//!
//! Like the snapshots, they are drawn by Mesa's llvmpipe:
//! `LIBGL_ALWAYS_SOFTWARE=1 WGPU_BACKEND=gl REDA_GALLERY_OUT=<folder> cargo test -p reda-ui
//! --test panel_sheets`.

use std::path::{Path, PathBuf};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::{Engine, preferences};
use reda_ui::shell::Shell;
use reda_ui::theme::{self, Density, Settings};
use serde_json::{Value, json};

/// The Panels, by the id `panel.show` takes.
const PANELS: [&str; 9] = [
    "projects",
    "properties",
    "messages",
    "library",
    "layers",
    "journal",
    "navigator",
    "find",
    "pins",
];

fn mil(x: i64, y: i64) -> Value {
    json!({"x": format!("{x}mil"), "y": format!("{y}mil")})
}

/// Whether the adapter is the one the sheets are meant to be drawn by.
fn can_draw() -> bool {
    cfg!(target_os = "linux")
        && reda_ui::engine::render::gpu::Headless::new()
            .is_ok_and(|h| h.adapter().name.contains("llvmpipe"))
}

/// A window on a Project with something for every Panel: two sheets (a Resistor and a
/// dangling Wire, so ERC has Errors and Warnings), a board with Nets, Tracks, a Via and
/// a Zone, a Project Library folder and an Output Job.
fn window(home: &Path, work: &Path) -> (Harness<'static, App>, Value) {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(Some(reda_testkit::starter_library()))
        .ids(reda_ui::engine::command::SeededIds::new(0))
        .commands(reda_testkit::probe_commands)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work, "Demo").unwrap();
    let sheet = shell.engine().project().unwrap().props().documents[0];
    let parts = shell.run("library.search", json!({})).unwrap();
    let resistor = (parts["parts"].as_array().unwrap().iter())
        .find(|p| p["name"] == "Resistor")
        .map(|p| p["uuid"].clone())
        .unwrap();
    let placed = shell
        .run(
            "schematic.component.place",
            json!({"document": sheet, "part": resistor, "at": mil(2000, 2000)}),
        )
        .unwrap();
    let out = shell
        .run(
            "document.add",
            json!({"kind": "schematic", "title": "Power"}),
        )
        .unwrap();
    let power = out["result"]["uuid"].clone();
    shell
        .run(
            "schematic.wire.add",
            json!({"document": power, "points": [mil(1000, 1000), mil(2000, 1000)]}),
        )
        .unwrap();
    let mut net = |name: &str| {
        let out = shell.run("test.net.add", json!({ "name": name })).unwrap();
        out["result"]["uuid"].clone()
    };
    let (gnd, sig) = (net("GND"), net("USB_D_P"));
    for (net, y, length) in [
        (&gnd, "0mm", "4mm"),
        (&gnd, "2mm", "6mm"),
        (&sig, "4mm", "3mm"),
    ] {
        shell
            .run(
                "pcb.track.add",
                json!({"layer": "copper_top", "net": net, "width": "0.5mm",
                       "points": [{"x": "0mm", "y": y}, {"x": length, "y": y}]}),
            )
            .unwrap();
    }
    shell
        .run("pcb.via.add", json!({"at": {"x": "12mm", "y": "5mm"}}))
        .unwrap();
    let _ = shell.run(
        "pcb.net.set_color",
        json!({"nets": [gnd], "color": "#3cb44b"}),
    );
    let resistor = placed["result"]["unit"].clone();
    let harness = Harness::builder()
        .with_size([1280.0, 860.0])
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state({
            let render_state = egui_kittest::wgpu::create_render_state(
                egui_kittest::wgpu::default_wgpu_setup(),
                egui_wgpu::RendererOptions::PREDICTABLE,
            );
            reda_ui::install_canvas(&render_state, 1);
            render_state
        }))
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    (harness, resistor)
}

#[test]
fn panel_approval_sheets() {
    let Some(out) = std::env::var_os("REDA_GALLERY_OUT") else {
        return;
    };
    if !can_draw() {
        eprintln!("skipping the Panel sheets: the adapter is not Mesa's llvmpipe");
        return;
    }
    let out = PathBuf::from(out);
    std::fs::create_dir_all(&out).unwrap();
    for theme_name in ["light", "dark"] {
        for (density, density_name) in [
            (Density::Comfortable, "comfortable"),
            (Density::Compact, "compact"),
        ] {
            let (home, work) = (TempDir::new("sheets-home"), TempDir::new("sheets-work"));
            let dirs = UserDirs::under(home.path());
            preferences::set(&dirs, "general.theme", json!(theme_name)).unwrap();
            let (mut h, resistor) = window(home.path(), work.path());
            h.run();
            h.get_all_by_label("Run ERC").last().unwrap().click();
            h.run();
            // The Appearance page is not built yet: Density is set as it will set it.
            let settings = Settings {
                density,
                ..Settings::default()
            };
            theme::set(&h.ctx, &settings);
            for panel in PANELS {
                let shell = h.state_mut().shell_mut();
                // The board Panels show the board; the others the sheet with its
                // selection.
                let project = shell.engine().project().unwrap();
                let pcb = project.pcb().map(|p| p.uuid);
                let sheet = project.props().documents[0];
                let document = match panel {
                    "layers" | "navigator" => json!(pcb),
                    _ => json!(sheet),
                };
                let _ = shell.run("document.open", json!({ "document": document }));
                shell.run("panel.show", json!({ "panel": panel })).unwrap();
                if panel == "properties" {
                    // The Resistor's properties, not the sheet's overview.
                    let _ = shell.run("selection.set", json!({ "objects": [resistor] }));
                }
                if panel == "find" {
                    reda_ui::find::edit(shell, |p| p.text = "R".into());
                    reda_ui::find::search(shell);
                }
                h.run();
                h.run();
                let image = h.render().expect("the window renders");
                let name = format!("panel_{panel}_{theme_name}_{density_name}.png");
                image.save(out.join(name)).unwrap();
            }
        }
    }
}
