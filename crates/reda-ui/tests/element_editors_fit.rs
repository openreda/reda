//! The element editors fit their text: the Footprint editor's panel (the Pad Tool, the
//! Footprint's texts, and the properties of a selected Pad, text and graphic, on a
//! Footprint of the user's, a generated one and a starter one), the Part editor's form
//! and its Pins Panel (a Part of the user's and a starter one) and the Symbol editor's
//! Unit strip. Each is drawn in the window at a laptop's size, in Comfortable and Compact
//! and in light and dark, and no text in it is cut short with an ellipsis, runs out of
//! the field, dropdown, column or panel that holds it, or breaks over lines unless it is
//! a sentence.

use std::path::Path;

use egui::Rect;
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::model::{LibraryElement, Uuid};
use reda_ui::engine::{Engine, preferences};
use reda_ui::shell::Shell;
use reda_ui::theme::{self, Density, Settings};
use serde_json::{Value, json};

const MINE: &str = "000000aa-0000-4000-8000-000000000000";
/// The starter Resistor's Symbol and two of its Footprints.
const RESISTOR_SYMBOL: &str = "06c71621-e7e4-49a4-9b90-88485bb936b5";
const RESC1608: &str = "0691e5e7-9eb6-49f3-9f3e-d4159258693e";
const RESC1005: &str = "cece716c-4fd0-42c8-900c-0f3bc3c14225";
/// The starter Button Part.
const BUTTON: &str = "a785152e-43cd-446a-b4db-4044e91e3761";

/// How many wheel turns a scroll area is given to reach its end.
const SCROLLS: usize = 12;

/// The window: a laptop's REDA window.
const WINDOW: [f32; 2] = [1280.0, 800.0];

fn uuid(v: &Value) -> Uuid {
    v.as_str()
        .unwrap_or_else(|| panic!("not a UUID: {v}"))
        .parse()
        .expect("a UUID")
}

/// A Shell with the starter Library and an empty user Library "Mine", in `theme`.
fn shell(home: &Path, theme: &str) -> Shell {
    let lib = home.join("lib");
    std::fs::create_dir_all(&lib).unwrap();
    std::fs::write(
        lib.join("Mine.redalib"),
        format!("{{\"format\": 1, \"library\": {{\"uuid\": \"{MINE}\", \"name\": \"Mine\"}}}}"),
    )
    .unwrap();
    let dirs = UserDirs::under(&home.join("home"));
    preferences::set(&dirs, "general.theme", json!(theme)).unwrap();
    preferences::set(&dirs, "libraries.paths", json!([lib])).unwrap();
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(dirs))
        .starter_library(Some(reda_testkit::starter_library()))
        .headless(false)
        .build();
    Shell::new(engine)
}

/// A Footprint of the user's with Pads of both kinds, a courtyard and its two texts;
/// returns it with its SMD Pad, its through-hole Pad, a text and the graphic to select.
fn footprint(shell: &mut Shell) -> (Uuid, [Uuid; 4]) {
    let out = shell
        .run(
            "library.element.new",
            json!({"library": MINE, "kind": "footprint", "name": "SOT-23-ish"}),
        )
        .unwrap();
    let fp = out["element"].clone();
    let pad = shell
        .run(
            "library.footprint.pad.add",
            json!({"footprint": fp, "number": "1", "kind": "smd", "shape": "rounded_rect",
                   "at": {"x": "-1mm", "y": "-1.1mm"}, "width": "0.6mm", "height": "0.9mm"}),
        )
        .unwrap()["result"]["pad"]
        .clone();
    let hole = shell
        .run(
            "library.footprint.pad.add",
            json!({"footprint": fp, "number": "2", "kind": "plated_through_hole",
                   "shape": "oval", "at": {"x": "3mm", "y": 0}, "width": "1.2mm",
                   "height": "2mm", "hole": {"width": "0.6mm", "height": "1.2mm"}}),
        )
        .unwrap()["result"]["pad"]
        .clone();
    shell
        .run(
            "library.footprint.graphic.add",
            json!({"footprint": fp, "layer": "courtyard_top",
                   "shape": {"kind": "rect", "start": {"x": "-2mm", "y": "-2mm"},
                             "end": {"x": "4mm", "y": "2mm"}}}),
        )
        .unwrap();
    let fp = uuid(&fp);
    let Some(LibraryElement::Footprint(f)) = shell.engine().element(fp) else {
        panic!("the Footprint is open");
    };
    let text = f.texts[0].uuid;
    let graphic = f.graphics[0].uuid;
    (fp, [uuid(&pad), uuid(&hole), text, graphic])
}

/// A Part of the user's on the starter Resistor's Symbol and two Footprints, with an
/// override in its pin-pad map and a custom field.
fn part(shell: &mut Shell) -> Uuid {
    let out = shell
        .run(
            "library.element.new",
            json!({"library": MINE, "kind": "part", "name": "Divider_10k"}),
        )
        .unwrap();
    let part = out["element"].clone();
    for (id, params) in [
        (
            "library.part.symbol.set",
            json!({"part": part, "symbol": RESISTOR_SYMBOL}),
        ),
        (
            "library.part.footprints.set",
            json!({"part": part, "footprints": [RESC1608, RESC1005]}),
        ),
        (
            "library.part.pin_map.set",
            json!({"part": part, "pins": [{"pin": "2", "pads": ["2", "3"]}]}),
        ),
        (
            "library.part.fields.set",
            json!({"part": part, "value": "10k", "mpn": "RC0603FR-0710KL",
                   "manufacturer": "Generic",
                   "custom": [{"name": "LCSC", "value": "C25804"}]}),
        ),
    ] {
        shell.run(id, params).unwrap();
    }
    uuid(&part)
}

/// One screen: what it is called, how to bring it up in the window, a text it shows (so
/// the check is known to look at all of it), and the labels of what scrolls in it (the
/// Footprint's panel, the Part's form, the Pins Panel), which are scrolled to their ends.
type Screen = (
    &'static str,
    Box<dyn Fn(&mut Shell)>,
    &'static str,
    &'static [&'static str],
);

const PANEL: &[&str] = &["Pad Tool"];
const PART: &[&str] = &["Component Type", "Pin-Pad Map"];

fn open(element: Uuid) -> impl Fn(&mut Shell) {
    move |shell: &mut Shell| {
        shell
            .run("library.element.open", json!({ "element": element }))
            .unwrap();
        let _ = shell.run("selection.set", json!({ "objects": [] }));
    }
}

fn select(element: Uuid, object: Uuid) -> impl Fn(&mut Shell) {
    move |shell: &mut Shell| {
        shell
            .run("library.element.open", json!({ "element": element }))
            .unwrap();
        shell
            .run("selection.set", json!({ "objects": [object] }))
            .unwrap();
    }
}

fn screens(shell: &mut Shell) -> Vec<Screen> {
    let (fp, [pad, hole, text, graphic]) = footprint(shell);
    let generated = shell
        .run(
            "library.footprint.generate.chip",
            json!({"library": MINE, "size": "0603"}),
        )
        .unwrap();
    let generated = uuid(&generated["footprint"]);
    let part = part(shell);
    let symbol = uuid(
        &shell
            .run(
                "library.element.new",
                json!({"library": MINE, "kind": "symbol", "name": "Bus"}),
            )
            .unwrap()["element"],
    );
    let starter_footprint: Uuid = RESC1608.parse().unwrap();
    let starter_part: Uuid = BUTTON.parse().unwrap();
    vec![
        ("Footprint", Box::new(open(fp)), "Pad Tool", PANEL),
        (
            "Footprint, an SMD Pad",
            Box::new(select(fp, pad)),
            "Corner ratio",
            PANEL,
        ),
        (
            "Footprint, a through-hole Pad",
            Box::new(select(fp, hole)),
            "Hole tolerance +",
            PANEL,
        ),
        (
            "Footprint, a text",
            Box::new(select(fp, text)),
            "Visible",
            PANEL,
        ),
        (
            "Footprint, a graphic",
            Box::new(select(fp, graphic)),
            "Stroke width",
            PANEL,
        ),
        (
            "generated Footprint",
            Box::new(open(generated)),
            "Regenerate…",
            PANEL,
        ),
        (
            "starter Footprint",
            Box::new(open(starter_footprint)),
            "Read-only: this Footprint is in the starter Library.",
            PANEL,
        ),
        ("Part", Box::new(open(part)), "Add Field", PART),
        (
            "starter Part",
            Box::new(open(starter_part)),
            "Read-only: this Part is in the starter Library.",
            PART,
        ),
        ("Symbol", Box::new(open(symbol)), "Add Unit", &[]),
    ]
}

/// What is wrong with the text drawn, one line per text: cut short with an ellipsis, out
/// of its clip sideways (a field, a dropdown, a column, a panel), or a label broken over
/// lines. Text a scroll area cuts at its top or bottom is only scrolled out of view.
///
/// The tab strips are the Shell's, not the editors': a strip with more tabs than room
/// scrolls, so text in a tab's band is left out.
fn misfits(h: &Harness<'static, App>) -> Vec<String> {
    let screen = h.ctx.content_rect();
    let strips: Vec<egui::Rangef> = h
        .get_all_by_role(Role::Tab)
        .map(|tab| tab.rect().y_range())
        .collect();
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
            if !clip.intersects(rect) || text.galley.text().is_empty() {
                continue;
            }
            if strips.iter().any(|band| band.contains(rect.center().y)) {
                continue;
            }
            let said = text.galley.text();
            if text.galley.elided {
                out.push(format!("cut short: {said:?}"));
            }
            if rect.right() > clip.right() + 0.5 || rect.left() < clip.left() - 0.5 {
                out.push(format!(
                    "runs out sideways by {}: {said:?}",
                    (rect.right() - clip.right()).max(clip.left() - rect.left())
                ));
            }
            if !screen.contains_rect(rect.intersect(clip)) {
                out.push(format!("off the window: {said:?}"));
            }
            // A sentence may wrap; a label, a title or a value may not.
            let sentence = said.trim_end().ends_with('.') || said.split(' ').count() > 8;
            if text.galley.rows.len() > 1 && !said.contains('\n') && !sentence {
                out.push(format!("wraps: {said:?}"));
            }
        }
    }
    out
}

/// Every text drawn this frame.
fn drawn(h: &Harness<'static, App>) -> Vec<String> {
    let mut out = Vec::new();
    for clipped in &h.output().shapes {
        let mut shapes = vec![&clipped.shape];
        while let Some(shape) = shapes.pop() {
            match shape {
                egui::Shape::Vec(inner) => shapes.extend(inner),
                egui::Shape::Text(text) => out.push(text.galley.text().to_owned()),
                _ => {}
            }
        }
    }
    out
}

fn check(dark: bool, density: Density) {
    let home = TempDir::new("element-editors-fit");
    let theme_name = if dark { "dark" } else { "light" };
    let mut shell = shell(home.path(), theme_name);
    let all = screens(&mut shell);
    // With `REDA_GALLERY_OUT` set, each screen is also written there as a picture, for a
    // reviewer (drawn by Mesa's llvmpipe: `LIBGL_ALWAYS_SOFTWARE=1 WGPU_BACKEND=gl`).
    let pictures = std::env::var_os("REDA_GALLERY_OUT").map(std::path::PathBuf::from);
    let mut builder = Harness::builder().with_size(WINDOW).with_max_steps(200);
    if pictures.is_some() {
        builder = builder.renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state({
            let render_state = egui_kittest::wgpu::create_render_state(
                egui_kittest::wgpu::default_wgpu_setup(),
                egui_wgpu::RendererOptions::PREDICTABLE,
            );
            reda_ui::install_canvas(&render_state, 1);
            render_state
        }));
    }
    let mut h = builder.build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    theme::set(
        &h.ctx,
        &Settings {
            density,
            ..Settings::default()
        },
    );
    let mut wrong = Vec::new();
    for (name, show, shows, scrolled) in all {
        let shell = h.state_mut().shell_mut();
        show(shell);
        // The Pins Panel follows the Part or Symbol in front.
        shell.run("panel.show", json!({"panel": "pins"})).unwrap();
        // Tables measure their columns on their first frame and the rest settles over
        // a few more.
        h.run_steps(30);
        if let Some(folder) = &pictures {
            std::fs::create_dir_all(folder).unwrap();
            let file = format!("{name}_{theme_name}_{density:?}.png").replace([' ', ','], "_");
            let image = h.render().expect("the window renders");
            image.save(folder.join(file.to_lowercase())).unwrap();
        }
        let mut seen = drawn(&h);
        let mut lines = misfits(&h);
        for label in scrolled {
            let at = h
                .get_all_by_label(label)
                .next()
                .expect(label)
                .rect()
                .center();
            h.event(egui::Event::PointerMoved(at));
            for _ in 0..SCROLLS {
                h.event(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -240.0),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
                    modifiers: egui::Modifiers::NONE,
                });
                h.run_steps(4);
                let now = drawn(&h);
                lines.extend(misfits(&h));
                let done = now.iter().all(|t| seen.contains(t));
                seen.extend(now);
                if done {
                    break;
                }
            }
        }
        assert!(
            seen.iter().any(|t| t == shows),
            "{name}: {shows:?} is not drawn"
        );
        for line in lines {
            let line = format!("{name}: {line}");
            if !wrong.contains(&line) {
                wrong.push(line);
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "{theme_name} {density:?}: text does not fit:\n{}",
        wrong.join("\n")
    );
}

#[test]
fn the_element_editors_fit_their_text_in_dark_comfortable() {
    check(true, Density::Comfortable);
}

#[test]
fn the_element_editors_fit_their_text_in_dark_compact() {
    check(true, Density::Compact);
}

#[test]
fn the_element_editors_fit_their_text_in_light_comfortable() {
    check(false, Density::Comfortable);
}

#[test]
fn the_element_editors_fit_their_text_in_light_compact() {
    check(false, Density::Compact);
}
