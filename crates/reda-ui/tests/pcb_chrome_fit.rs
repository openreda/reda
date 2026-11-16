//! The PCB editor's chrome fits its text: the Layer swatch and its menu, the Layer strip
//! and its Layer sets menu, the grid readout, the Routing Bar with its width chooser, the
//! Placement Bar, the Show on board banner, the "No Board Outline" hint and its form,
//! Choose Component and the PCB's Update from Library. Each is drawn in the window in
//! Comfortable and Compact and in light and dark, and no text in it is cut short with an
//! ellipsis, runs out of what holds it, or breaks over lines unless it is a sentence.

use std::path::Path;

use egui::Rect;
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::{TempDir, starter_library};
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::Uuid;
use reda_ui::engine::model::geom::{Nm, Point};
use reda_ui::shell::Shell;
use reda_ui::show_on_board::Sides;
use serde_json::{Value, json};

/// The window: a laptop's REDA window.
const WINDOW: [f32; 2] = [1280.0, 800.0];
/// The Starter Library's plated M3 mounting hole, and its Footprint's file.
const HOLE_PAD: &str = "1a21d727-dfca-41ae-9639-309d8abf4714";
const HOLE_FILE: &str = "footprints/MountingHole_M3_Pad.redafp";

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

struct Window {
    h: Harness<'static, App>,
    pcb: Uuid,
    hole: Uuid,
    _dirs: (TempDir, TempDir, TempDir),
}

/// The window on a board with no Board Outline, two Pads on Net SIG at (0, 0) and
/// (10, 0) mm, and a mounting hole whose Library Footprint has changed since it was
/// placed, in `theme` and `density`, its PCB tab focused.
fn window(theme: &str, density: &str) -> Window {
    let lib_dir = TempDir::new("pcb-chrome-fit-lib");
    let library = lib_dir.path().join("Lib");
    copy_dir(&starter_library(), &library);
    let home = TempDir::new("pcb-chrome-fit-home");
    let work = TempDir::new("pcb-chrome-fit-work");
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(Some(library.clone()))
        .commands(reda_testkit::probe_commands)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Board").unwrap();
    for (key, value) in [("general.theme", theme), ("general.density", density)] {
        shell
            .run("app.preferences.set", json!({"key": key, "value": value}))
            .unwrap();
    }
    let net = shell.run("test.net.add", json!({"name": "SIG"})).unwrap()["result"]["uuid"].clone();
    for x in ["0mm", "10mm"] {
        shell
            .run(
                "test.footprint.add",
                json!({"at": {"x": x, "y": 0}, "pads": [
                    {"at": {"x": 0, "y": 0}, "width": "2mm", "height": "2mm", "net": net}
                ]}),
            )
            .unwrap();
    }
    let hole: Uuid = shell
        .run(
            "pcb.mechanical.place",
            json!({"part": HOLE_PAD, "at": {"x": "20mm", "y": "10mm"}}),
        )
        .unwrap()["result"]["footprint"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let file = library.join(HOLE_FILE);
    let text = std::fs::read_to_string(&file).unwrap();
    std::fs::write(
        &file,
        text.replace(
            r#""width":5500000,"height":5500000"#,
            r#""width":6000000,"height":6000000"#,
        ),
    )
    .unwrap();
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    shell
        .run("document.open", json!({"document": pcb}))
        .unwrap();
    shell
        .run("layout.tab.activate", json!({"tab": pcb.to_string()}))
        .unwrap();
    shell
        .run("view.zoom_fit", json!({"document": pcb}))
        .unwrap();
    let mut h = Harness::builder()
        .with_size(WINDOW)
        .with_max_steps(12)
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run_steps(8);
    Window {
        h,
        pcb,
        hole,
        _dirs: (lib_dir, home, work),
    }
}

impl Window {
    fn run(&mut self, id: &str, params: Value) {
        self.h.state_mut().shell_mut().run(id, params).unwrap();
        self.settle();
    }

    /// Menus, dialogs and tables take a few frames to open, measure and settle.
    fn settle(&mut self) {
        self.h.run_steps(12);
    }

    fn click(&mut self, label: &str) {
        self.h.get_all_by_label(label).last().unwrap().click();
        self.settle();
    }

    fn canvas(&self) -> Rect {
        self.h.get_by_role(Role::Canvas).rect()
    }

    /// The rects of every node whose label starts with one of `labels`.
    fn nodes(&self, labels: &[&str]) -> Vec<Rect> {
        self.h
            .get_all_by(|n| {
                n.label()
                    .is_some_and(|l| labels.iter().any(|p| l.starts_with(p)))
            })
            .map(|n| n.rect())
            .collect()
    }

    /// The rect of the window or dialog titled `title`.
    fn dialog(&self, title: &str) -> Rect {
        self.h.get_by_role_and_label(Role::Window, title).rect()
    }

    fn screen(&self, p: Point) -> egui::Pos2 {
        let camera = self
            .h
            .state()
            .shell()
            .document_tab(self.pcb)
            .unwrap()
            .camera
            .unwrap();
        camera.screen_point(self.canvas(), (p.x.0 as f64, p.y.0 as f64))
    }
}

fn mm(x: f64, y: f64) -> Point {
    Point::new(Nm((x * 1e6) as i64), Nm((y * 1e6) as i64))
}

/// What is wrong with the text drawn wholly inside one of `regions`, one line per text:
/// cut short with an ellipsis, out of its clip sideways (a field, a chip, a column), or a
/// label broken over lines. Text that pokes out of a region belongs to what is behind it.
fn misfits(h: &Harness<'static, App>, regions: &[Rect]) -> Vec<String> {
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
            let clip: Rect = clipped.clip_rect;
            let said = text.galley.text();
            if said.trim().is_empty()
                || !clip.intersects(rect)
                || !regions.iter().any(|r| r.expand(0.5).contains_rect(rect))
            {
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
            if rect.bottom() > clip.bottom() + 0.5 && rect.top() < clip.bottom() {
                out.push(format!("cut at its foot: {said:?}"));
            }
            let sentence = said.trim_end().ends_with('.') || said.split(' ').count() > 8;
            if text.galley.rows.len() > 1 && !said.contains('\n') && !sentence {
                out.push(format!("wraps: {said:?}"));
            }
        }
    }
    out
}

/// Draws every PCB chrome screen in `theme` and `density`; returns what does not fit,
/// each line prefixed with the screen.
fn screens(theme: &str, density: &str) -> Vec<String> {
    let mut w = window(theme, density);
    let mut out = Vec::new();
    let mut check = |w: &Window, screen: &str, regions: Vec<Rect>| {
        out.extend(
            misfits(&w.h, &regions)
                .into_iter()
                .map(|m| format!("{theme} {density}, {screen}: {m}")),
        );
    };

    // The swatch, the grid readout and the "No Board Outline" hint over the canvas.
    check(&w, "canvas", vec![w.canvas()]);
    // The swatch's menu: the Layers and the Layer sets.
    w.click("Active Layer: Top Copper");
    let menu = w.nodes(&["Layer: ", "Signal", "Save current as"]);
    check(&w, "swatch menu", menu);
    w.h.key_press(egui::Key::Escape);
    w.settle();

    // The outline form.
    w.click("Create rectangle…");
    check(&w, "outline form", vec![w.dialog("Rectangular Board Outline")]);
    w.h.key_press(egui::Key::Escape);
    w.settle();

    // The strip and its Layer sets menu.
    w.run(
        "app.preferences.set",
        json!({"key": "pcb.view.layer_control", "value": "strip"}),
    );
    check(&w, "strip", vec![w.canvas()]);
    w.click("Layer sets");
    let menu = w.nodes(&["All", "Signal", "NonSignal", "Mechanical", "Top", "Bottom", "Fab", "Save current as"]);
    check(&w, "strip menu", menu);
    w.h.key_press(egui::Key::Escape);
    w.settle();

    // The Placement Bar, with a Footprint selected.
    w.run("tool.activate", json!({"tool": "select"}));
    w.run("selection.set", json!({"objects": [w.hole]}));
    check(&w, "placement bar", vec![w.canvas()]);
    w.run("selection.set", json!({"objects": []}));

    // Choose Component.
    w.run("pcb.place.choose", json!({}));
    check(&w, "choose component", vec![w.dialog("Choose Component")]);
    w.h.key_press(egui::Key::Escape);
    w.settle();

    // The Routing Bar with a route in flight and its width chooser open.
    w.run("tool.activate", json!({"tool": "route"}));
    w.h.state_mut().shell_mut().route_click(mm(0.0, 0.0));
    w.h.state_mut().shell_mut().route_hover(mm(0.0, 4.0));
    w.h.hover_at(w.screen(mm(0.0, 4.0)));
    w.settle();
    w.run("pcb.route.chooser", json!({"kind": "width"}));
    assert!(w.h.state().shell().router().chooser().is_some());
    check(&w, "routing bar and chooser", vec![w.canvas()]);
    w.run("pcb.route.end", json!({}));
    w.run("tool.activate", json!({"tool": "select"}));

    // The Show on board banner.
    let hole = w.hole;
    w.h.state_mut().shell_mut().show_change(Sides {
        board: vec![hole],
        sheet: None,
    });
    w.settle();
    assert!(w.h.state().shell().shown_on_board().is_some());
    check(&w, "show on board", vec![w.canvas()]);
    w.h.state_mut().shell_mut().back_from_show();
    w.settle();

    // The PCB's Update from Library.
    w.click("Design");
    w.click("Update from Library…");
    check(
        &w,
        "update from library",
        vec![w.dialog("Update Footprints from Library")],
    );
    out
}

#[test]
fn the_pcb_chrome_fits_its_text_in_both_densities_and_themes() {
    let mut out = Vec::new();
    for theme in ["light", "dark"] {
        for density in ["comfortable", "compact"] {
            out.extend(screens(theme, density));
        }
    }
    assert!(out.is_empty(), "{}", out.join("\n"));
}
