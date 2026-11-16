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
use egui_kittest::Harness;
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

/// The window: a laptop's REDA window.
const WINDOW: [f32; 2] = [1280.0, 800.0];

fn uuid(v: &Value) -> Uuid {
    v.as_str().expect("a UUID").parse().expect("a UUID")
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
/// returns it with a Pad, a text and the graphic to select.
fn footprint(shell: &mut Shell) -> (Uuid, [Uuid; 3]) {
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
    shell
        .run(
            "library.footprint.pad.add",
            json!({"footprint": fp, "number": "2", "kind": "plated_through_hole",
                   "shape": "oval", "at": {"x": "3mm", "y": 0}, "width": "1.2mm",
                   "height": "2mm", "hole": {"width": "0.6mm", "height": "1.2mm"}}),
        )
        .unwrap();
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
    (fp, [uuid(&pad), text, graphic])
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

/// One screen: what it is called and how to bring it up in the window.
type Screen = (String, Box<dyn Fn(&mut Shell)>);

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
    let (fp, [pad, text, graphic]) = footprint(shell);
    let generated = shell
        .run(
            "library.footprint.generate.chip",
            json!({"library": MINE, "size": "0603"}),
        )
        .unwrap();
    let generated = uuid(&generated["element"]);
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
        ("Footprint".into(), Box::new(open(fp))),
        ("Footprint, a Pad".into(), Box::new(select(fp, pad))),
        ("Footprint, a text".into(), Box::new(select(fp, text))),
        ("Footprint, a graphic".into(), Box::new(select(fp, graphic))),
        ("generated Footprint".into(), Box::new(open(generated))),
        ("starter Footprint".into(), Box::new(open(starter_footprint))),
        ("Part".into(), Box::new(open(part))),
        ("starter Part".into(), Box::new(open(starter_part))),
        ("Symbol".into(), Box::new(open(symbol))),
    ]
}

/// What is wrong with the text drawn, one line per text: cut short with an ellipsis, out
/// of its clip sideways (a field, a dropdown, a column, a panel), or a label broken over
/// lines. Text a scroll area cuts at its top or bottom is only scrolled out of view.
fn misfits(h: &Harness<'static, App>) -> Vec<String> {
    let screen = h.ctx.content_rect();
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

fn check(dark: bool, density: Density) {
    let home = TempDir::new("element-editors-fit");
    let theme_name = if dark { "dark" } else { "light" };
    let mut shell = shell(home.path(), theme_name);
    let all = screens(&mut shell);
    let mut h = Harness::builder()
        .with_size(WINDOW)
        .with_max_steps(200)
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    theme::set(
        &h.ctx,
        &Settings {
            density,
            ..Settings::default()
        },
    );
    let mut wrong = Vec::new();
    for (name, show) in all {
        let shell = h.state_mut().shell_mut();
        show(shell);
        // The Pins Panel follows the Part or Symbol in front.
        shell.run("panel.show", json!({"panel": "pins"})).unwrap();
        // Tables measure their columns on their first frame and the rest settles over
        // a few more.
        h.run_steps(30);
        for line in misfits(&h) {
            wrong.push(format!("{name}: {line}"));
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
