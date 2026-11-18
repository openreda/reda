//! Every Panel in the window: the nine Panels with something in them, in light and dark,
//! both Densities and at the default text size and both ends of the range.
//!
//! - `every_panel_fits_its_text_*` fails on text in the window (the Panels, their tab
//!   strips, the Docks, the status bar and the bars over the canvas) that is cut short, runs
//!   out of the clip that holds it, or is a label broken over lines.
//! - `panel_approval_sheets` writes pictures for a reviewer, only when `REDA_GALLERY_OUT`
//!   is set, as `panel_<panel>_<theme>_<density>.png`. Like the snapshots, they are drawn
//!   by Mesa's llvmpipe: `LIBGL_ALWAYS_SOFTWARE=1 WGPU_BACKEND=gl REDA_GALLERY_OUT=<folder>
//!   cargo test -p reda-ui --test panel_sheets`.

#[path = "support/text_size.rs"]
mod text_size;

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
fn window(home: &Path, work: &Path, draw: bool) -> (Harness<'static, App>, Value) {
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
    let mut builder = Harness::builder().with_size([1280.0, 860.0]);
    if draw {
        let render_state = egui_kittest::wgpu::create_render_state(
            egui_kittest::wgpu::default_wgpu_setup(),
            egui_wgpu::RendererOptions::PREDICTABLE,
        );
        reda_ui::install_canvas(&render_state, 1);
        builder = builder.renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(
            render_state,
        ));
    }
    let harness = builder.build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    (harness, resistor)
}

/// Shows `panel` with what it is about: the board for the board Panels, the sheet with
/// the Resistor selected for the others, and a search for Find.
fn show(h: &mut Harness<'static, App>, panel: &str, resistor: &Value) {
    let shell = h.state_mut().shell_mut();
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
}

/// What is wrong with the text in the window, one line per text: cut short with an
/// ellipsis, out of its clip sideways, or a label (not a sentence) broken over lines. Text
/// a scroll area cuts at its foot is scrolled, not clipped, so only the sides count.
/// Text holding one of the temporary `folders` (a path the Library or Messages Panel may
/// show) is skipped: how long it is depends on the OS that resolves it.
fn misfits(h: &Harness<'static, App>, folders: &[&Path]) -> Vec<String> {
    let mut out = Vec::new();
    for clipped in &h.output().shapes {
        let mut shapes = vec![&clipped.shape];
        while let Some(shape) = shapes.pop() {
            let text = match shape {
                egui::Shape::Vec(inner) => {
                    shapes.extend(inner);
                    continue;
                }
                egui::Shape::Text(text) => text,
                _ => continue,
            };
            let rect = text.galley.rect.translate(text.pos.to_vec2());
            let clip = clipped.clip_rect;
            let said = text.galley.text();
            if said.trim().is_empty() || !clip.intersects(rect) || holds_folder(said, folders) {
                continue;
            }
            if text.galley.elided {
                out.push(format!("cut short: {said:?}"));
            }
            if rect.right() > clip.right() + 0.5 || rect.left() < clip.left() - 0.5 {
                out.push(format!(
                    "runs out sideways by {}: {said:?}",
                    (rect.right() - clip.right()).max(clip.left() - rect.left())
                ));
            }
            let sentence =
                said.trim_end().ends_with(['.', '?', ':', '…']) || said.split(' ').count() > 8;
            if text.galley.rows.len() > 1 && !said.contains('\n') && !sentence {
                out.push(format!("wraps: {said:?}"));
            }
        }
    }
    out
}

/// Whether `said` shows one of `folders`, as asked for or as the OS resolves it
/// (`/private/var/..` on macOS, `\\?\C:\..` on Windows), whole or shortened.
fn holds_folder(said: &str, folders: &[&Path]) -> bool {
    folders.iter().any(|folder| {
        let name = folder.file_name().map(|n| n.to_string_lossy().into_owned());
        let real = std::fs::canonicalize(folder).ok();
        said.contains(&*folder.display().to_string())
            || real.is_some_and(|r| said.contains(&*r.display().to_string()))
            || name.is_some_and(|n| said.contains(&n))
    })
}

/// Every Panel in turn, in `theme` and `density` at this thread's text size.
fn check(theme: &str, density: &str) {
    let (home, work) = (TempDir::new("sheets-home"), TempDir::new("sheets-work"));
    let dirs = UserDirs::under(home.path());
    preferences::set(&dirs, "general.theme", json!(theme)).unwrap();
    preferences::set(&dirs, "general.density", json!(density)).unwrap();
    preferences::set(&dirs, "general.text_size", json!(text_size::get())).unwrap();
    let (mut h, resistor) = window(home.path(), work.path(), false);
    h.run();
    h.get_all_by_label("Run ERC").last().unwrap().click();
    h.run();
    let mut wrong = Vec::new();
    for panel in PANELS {
        show(&mut h, panel, &resistor);
        let folders = [home.path(), work.path()];
        wrong.extend(
            misfits(&h, &folders)
                .into_iter()
                .map(|m| format!("{panel}: {m}")),
        );
    }
    wrong.dedup();
    assert!(
        wrong.is_empty(),
        "{theme} {density} {} px: text does not fit:\n{}",
        text_size::get(),
        wrong.join("\n")
    );
}

#[test]
fn every_panel_fits_its_text_in_dark_comfortable() {
    check("dark", "comfortable");
}

#[test]
fn every_panel_fits_its_text_in_dark_compact() {
    check("dark", "compact");
}

#[test]
fn every_panel_fits_its_text_in_light_comfortable() {
    check("light", "comfortable");
}

#[test]
fn every_panel_fits_its_text_in_light_compact() {
    check("light", "compact");
}

#[test]
fn every_panel_fits_its_text_in_light_comfortable_at_the_smallest_text_size() {
    text_size::at(text_size::SMALLEST, || check("light", "comfortable"));
}

#[test]
fn every_panel_fits_its_text_in_dark_compact_at_the_smallest_text_size() {
    text_size::at(text_size::SMALLEST, || check("dark", "compact"));
}

#[test]
fn every_panel_fits_its_text_in_dark_comfortable_at_the_largest_text_size() {
    text_size::at(text_size::LARGEST, || check("dark", "comfortable"));
}

#[test]
fn every_panel_fits_its_text_in_light_compact_at_the_largest_text_size() {
    text_size::at(text_size::LARGEST, || check("light", "compact"));
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
            let (mut h, resistor) = window(home.path(), work.path(), true);
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
                show(&mut h, panel, &resistor);
                let image = h.render().expect("the window renders");
                let name = format!("panel_{panel}_{theme_name}_{density_name}.png");
                image.save(out.join(name)).unwrap();
            }
        }
    }
}
