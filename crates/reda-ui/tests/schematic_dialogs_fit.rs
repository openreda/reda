//! The schematic's dialogs and the schematic editor's own chrome fit their text: in a
//! 1280 × 800 window, in Comfortable and Compact and in light and dark, no text is cut
//! short with an ellipsis, none runs out of the field, row or list that clips it, and no
//! label breaks over lines. Sentences that are meant to wrap (an intro, a note) may.
//!
//! Re-annotate (from the schematic and from the board), Re-annotate (last settings),
//! Update from Library on both pages with the field grid, Replace Part, Convert Cross
//! Junctions, the words of new text and the shared-element question are drawn alone;
//! the pick list, the in-place field, the Paused chip and a Wire's tooltip are drawn in
//! the window, over the canvas.

#[path = "support/text_size.rs"]
mod text_size;

use std::path::{Path, PathBuf};

use egui::accesskit::Role;
use egui::{Key, Pos2, Rect};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::command_dialog::ParamsDialog as _;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::model::Uuid;
use reda_ui::schematic_dialogs::{
    ConvertCrossDialog, Page, ReannotateDialog, ReannotateLast, ReplaceDialog, TextDialog,
    UpdateDialog, UpdateScope,
};
use reda_ui::shell::Shell;
use reda_ui::theme::{self, Density, Settings};
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";
/// The user's own Library, for the shared-element question.
const MINE: &str = "b0000000-0000-4000-8000-000000000001";
const MIL: f64 = 25_400.0;

/// A laptop's window.
const WINDOW: [f32; 2] = [1280.0, 800.0];

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

fn test_library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

/// A Project on a writable copy of the test Library, with two resistors, one of them
/// out of date with the Library (its value), and a Wire.
struct Fixture {
    shell: Shell,
    components: Vec<Uuid>,
    _dirs: (TempDir, TempDir),
}

fn fixture(label: &str) -> Fixture {
    let lib_dir = TempDir::new(&format!("{label}-lib"));
    let library = lib_dir.path().join("Lib");
    copy_dir(&test_library(), &library);
    let dir = TempDir::new(label);
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(&dir.path().join("home"))))
        .starter_library(Some(library.clone()))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(dir.path(), "Fit").unwrap();
    let mut components = Vec::new();
    for x in ["2000mil", "0mil"] {
        let out = shell
            .run(
                "schematic.component.place",
                json!({"document": SHEET, "part": R_10K, "at": {"x": x, "y": 0}}),
            )
            .unwrap();
        components.push(
            out["result"]["component"]["uuid"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap(),
        );
    }
    shell
        .run(
            "schematic.wire.add",
            json!({"document": SHEET, "points": [
                {"x": "0mil", "y": "-1000mil"}, {"x": "2000mil", "y": "-1000mil"}]}),
        )
        .unwrap();
    let part = library.join("parts/R_10k.redapart");
    let text = std::fs::read_to_string(&part).unwrap();
    std::fs::write(
        &part,
        text.replace("\"value\":\"10k\"", "\"value\":\"22k\""),
    )
    .unwrap();
    Fixture {
        shell,
        components,
        _dirs: (lib_dir, dir),
    }
}

type Draw = Box<dyn FnMut(&egui::Context, &mut Shell)>;

struct Alone {
    shell: Shell,
    draw: Draw,
}

fn themed<S>(h: &mut Harness<'static, S>, dark: bool, density: Density) {
    let settings = Settings {
        density,
        text_size: text_size::get(),
        ..Settings::default()
    };
    theme::set(&h.ctx, &settings);
    h.ctx.set_theme(if dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    });
}

/// `draw` alone in the window, settled: a dialog grows in and sizes itself to its content
/// over a few frames.
fn alone(shell: Shell, draw: Draw, dark: bool, density: Density) -> Harness<'static, Alone> {
    let mut h = Harness::builder().with_size(WINDOW).build_ui_state(
        |ui, a: &mut Alone| (a.draw)(&ui.ctx().clone(), &mut a.shell),
        Alone { shell, draw },
    );
    themed(&mut h, dark, density);
    h.run_steps(30);
    h
}

/// What is wrong with the text drawn inside `area`, one line per text.
fn misfits(shapes: &[egui::epaint::ClippedShape], area: Rect) -> Vec<String> {
    let mut out = Vec::new();
    for clipped in shapes {
        let mut todo = vec![&clipped.shape];
        while let Some(shape) = todo.pop() {
            let text = match shape {
                egui::Shape::Vec(inner) => {
                    todo.extend(inner);
                    continue;
                }
                egui::Shape::Text(text) => text,
                _ => continue,
            };
            let rect = text.galley.rect.translate(text.pos.to_vec2());
            let clip = clipped.clip_rect;
            // Text scrolled out of its list is not on screen.
            if !area.intersects(rect) || !clip.intersects(rect) {
                continue;
            }
            let said = text.galley.text();
            if said.trim().is_empty() {
                continue;
            }
            if text.galley.elided {
                out.push(format!("cut short: {said:?}"));
            }
            // The lists scroll down, never across: text cut at a side is cut.
            if rect.right() > clip.right() + 0.5 || rect.left() < clip.left() - 0.5 {
                out.push(format!(
                    "runs out sideways by {}: {said:?}",
                    rect.right() - clip.right()
                ));
            }
            if rect.right() > area.right() + 0.5 || rect.left() < area.left() - 0.5 {
                out.push(format!("runs out of its window: {said:?}"));
            }
            // A sentence may wrap; a label, a title or a value may not.
            let sentence = said.trim_end().ends_with(['.', '?']) || said.split(' ').count() > 8;
            if text.galley.rows.len() > 1 && !said.contains('\n') && !sentence {
                out.push(format!("wraps: {said:?}"));
            }
        }
    }
    out
}

fn check_alone(name: &str, shell: Shell, draw: Draw, dark: bool, density: Density) -> Vec<String> {
    let h = alone(shell, draw, dark, density);
    let area = h.ctx.content_rect();
    let shapes = &h.output().shapes;
    assert!(
        shapes
            .iter()
            .any(|s| matches!(s.shape, egui::Shape::Text(_) | egui::Shape::Vec(_))),
        "{name}: nothing drawn"
    );
    misfits(shapes, area)
        .into_iter()
        .map(|line| format!("{name}: {line}"))
        .collect()
}

fn dialogs_alone(dark: bool, density: Density) -> Vec<String> {
    let mut wrong = Vec::new();

    let mut f = fixture("fit-reannotate");
    let mut dialog = ReannotateDialog::new(&mut f.shell);
    // A threshold that does not read, so its message shows.
    dialog.set_threshold("lots");
    let draw: Draw = Box::new(move |ctx, shell| {
        dialog.show(ctx, shell);
    });
    wrong.extend(check_alone("Re-annotate", f.shell, draw, dark, density));

    let mut f = fixture("fit-reannotate-board");
    let mut dialog = ReannotateDialog::new(&mut f.shell);
    dialog.set_source(reda_ui::engine::model::annotate::Source::Board);
    let draw: Draw = Box::new(move |ctx, shell| {
        dialog.show(ctx, shell);
    });
    wrong.extend(check_alone(
        "Re-annotate (board)",
        f.shell,
        draw,
        dark,
        density,
    ));

    let mut f = fixture("fit-reannotate-last");
    let mut first = ReannotateDialog::new(&mut f.shell);
    first.apply(&mut f.shell).unwrap();
    f.shell
        .run(
            "schematic.component.place",
            json!({"document": SHEET, "part": R_10K, "at": {"x": "-2000mil", "y": 0}}),
        )
        .unwrap();
    let mut last = ReannotateLast::new(&mut f.shell).unwrap();
    let draw: Draw = Box::new(move |ctx, shell| {
        last.show(ctx, shell);
    });
    wrong.extend(check_alone(
        "Re-annotate (last settings)",
        f.shell,
        draw,
        dark,
        density,
    ));

    let mut f = fixture("fit-update-setup");
    let mut dialog = UpdateDialog::new(&mut f.shell, UpdateScope::default());
    let draw: Draw = Box::new(move |ctx, shell| {
        dialog.show(ctx, shell);
    });
    wrong.extend(check_alone(
        "Update from Library ▸ 1",
        f.shell,
        draw,
        dark,
        density,
    ));

    let mut f = fixture("fit-update-preview");
    let mut dialog = UpdateDialog::new(&mut f.shell, UpdateScope::default());
    dialog.next(&mut f.shell);
    assert_eq!(dialog.page(), Page::Preview);
    let row = dialog.rows()[0].component;
    dialog.select_row(row, false);
    assert!(dialog.shown().is_some_and(|r| !r.cells.is_empty()));
    let draw: Draw = Box::new(move |ctx, shell| {
        dialog.show(ctx, shell);
    });
    wrong.extend(check_alone(
        "Update from Library ▸ 2",
        f.shell,
        draw,
        dark,
        density,
    ));

    let mut f = fixture("fit-replace");
    let mut dialog = ReplaceDialog::new(&mut f.shell, vec![f.components[0]]);
    let other = dialog.rows().iter().find(|r| !r.current).unwrap().part;
    dialog.choose(&mut f.shell, other);
    let draw: Draw = Box::new(move |ctx, shell| {
        dialog.show(ctx, shell);
    });
    wrong.extend(check_alone("Replace Part", f.shell, draw, dark, density));

    let mut f = fixture("fit-cross");
    f.shell
        .run("document.open", json!({"document": SHEET}))
        .unwrap();
    let mut dialog = ConvertCrossDialog::new(&f.shell).unwrap();
    let draw: Draw = Box::new(move |ctx, shell| {
        dialog.show(ctx, shell);
    });
    wrong.extend(check_alone(
        "Convert Cross Junctions",
        f.shell,
        draw,
        dark,
        density,
    ));

    let f = fixture("fit-text");
    let mut dialog = TextDialog::new(json!({"document": SHEET, "at": {"x": 0, "y": 0}}));
    let draw: Draw = Box::new(move |ctx, shell| {
        dialog.show(ctx, shell);
    });
    wrong.extend(check_alone("Text", f.shell, draw, dark, density));

    let mut f = fixture("fit-shared");
    let symbol = shared_symbol(&mut f.shell);
    assert!(
        reda_ui::shared_element::asking(&f.shell).is_some(),
        "{symbol}"
    );
    let draw: Draw = Box::new(move |ctx, shell| {
        reda_ui::shared_element::show(ctx, shell);
    });
    wrong.extend(check_alone("Shared element", f.shell, draw, dark, density));

    wrong
}

/// A Symbol of a writable Library of the user's that two Parts use, open and changed:
/// the change waits on the shared-element question.
fn shared_symbol(shell: &mut Shell) -> Uuid {
    let lib = TempDir::new("fit-shared-lib");
    std::fs::write(
        lib.path().join("Mine.redalib"),
        format!("{{\"format\": 1, \"library\": {{\"uuid\": \"{MINE}\", \"name\": \"Mine\"}}}}"),
    )
    .unwrap();
    shell
        .run(
            "app.preferences.set",
            json!({"key": "libraries.paths", "value": [lib.path()]}),
        )
        .unwrap();
    let mut new = |kind: &str, name: &str| -> Uuid {
        let out = shell
            .run(
                "library.element.new",
                json!({"library": MINE, "kind": kind, "name": name}),
            )
            .unwrap();
        out["element"].as_str().unwrap().parse().unwrap()
    };
    let symbol = new("symbol", "Shared");
    let parts = [new("part", "First"), new("part", "Second")];
    let save = |shell: &mut Shell, element: Uuid| {
        shell
            .run("library.element.save", json!({"element": element}))
            .unwrap();
    };
    save(shell, symbol);
    for part in parts {
        shell
            .run(
                "library.part.symbol.set",
                json!({"part": part, "symbol": symbol}),
            )
            .unwrap();
        save(shell, part);
    }
    let unit = match shell.engine().element(symbol) {
        Some(reda_ui::engine::model::LibraryElement::Symbol(s)) => s.units[0].uuid,
        _ => unreachable!(),
    };
    let params = json!({"symbol": symbol, "unit": unit, "number": "1", "name": "A",
                        "at": {"x": "-300mil", "y": "0mil"}});
    assert!(!reda_ui::shared_element::hold(
        shell,
        "library.symbol.pin.add",
        &params
    ));
    // The Library's folder stays for the test's length.
    std::mem::forget(lib);
    symbol
}

/// The window on the fixture's Project with its sheet open.
fn window(dark: bool, density: Density) -> (Harness<'static, App>, Fixture) {
    let mut f = fixture("fit-window");
    f.shell
        .run(
            "app.preferences.set",
            json!({"key": "editing.auto_pan.style", "value": "off"}),
        )
        .unwrap();
    let shell = std::mem::replace(&mut f.shell, Shell::new(Engine::builder().build()));
    let mut h = Harness::builder()
        .with_size(WINDOW)
        .with_step_dt(0.01)
        .with_max_steps(200)
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    themed(&mut h, dark, density);
    h.run_steps(10);
    (h, f)
}

fn run(h: &mut Harness<'_, App>, id: &str, params: Value) -> Value {
    h.state_mut().shell_mut().run(id, params).unwrap()
}

fn screen(h: &Harness<'_, App>, x: f64, y: f64) -> Pos2 {
    let rect = h.get_by_role(Role::Canvas).rect();
    let sheet = SHEET.parse().unwrap();
    let camera = h
        .state()
        .shell()
        .document_tab(sheet)
        .unwrap()
        .camera
        .unwrap();
    camera.screen_point(rect, (x * MIL, y * MIL))
}

/// The text in `area` of the window as it is drawn now. Only text wholly inside it is
/// looked at: what pokes out from under it belongs to the panels behind.
fn check_in(name: &str, h: &Harness<'_, App>, area: Rect) -> Vec<String> {
    let inside: Vec<_> = (h.output().shapes.iter())
        .filter(|clipped| match &clipped.shape {
            egui::Shape::Text(text) => {
                area.contains_rect(text.galley.rect.translate(text.pos.to_vec2()))
            }
            _ => false,
        })
        .cloned()
        .collect();
    misfits(&inside, area)
        .into_iter()
        .map(|line| format!("{name}: {line}"))
        .collect()
}

/// Around a floating field or chip: its frame, and a message under a field.
fn around(rect: Rect) -> Rect {
    rect.expand2(egui::vec2(24.0, 40.0))
}

fn editor_chrome(dark: bool, density: Density) -> Vec<String> {
    let mut wrong = Vec::new();

    // The pick list: a Wire across a resistor, double-clicked.
    let (mut h, _f) = window(dark, density);
    run(
        &mut h,
        "schematic.wire.add",
        json!({"document": SHEET, "points": [
            {"x": "1500mil", "y": "0mil"}, {"x": "2500mil", "y": "0mil"}]}),
    );
    h.run();
    let at = screen(&h, 2000.0, 0.0);
    for _ in 0..2 {
        h.drag_at(at);
        h.drop_at(at);
        h.run();
    }
    h.run_steps(30);
    let list = h.get_by_label("Pick Object").rect();
    wrong.extend(check_in("Pick Object", &h, list));

    // A Wire's tooltip.
    let (mut h, _f) = window(dark, density);
    run(
        &mut h,
        "schematic.net_label.add",
        json!({"document": SHEET, "name": "SDA", "at": {"x": "0mil", "y": "-1000mil"}}),
    );
    h.run();
    h.hover_at(screen(&h, 1500.0, -1000.0));
    h.run_steps(4);
    let tip = h.get_by_label("Net: SDA").rect();
    wrong.extend(check_in("Wire tooltip", &h, around(tip)));

    // F2 on a Net Label: the in-place field.
    let (mut h, _f) = window(dark, density);
    let label = run(
        &mut h,
        "schematic.net_label.add",
        json!({"document": SHEET, "name": "VBUS_SENSE", "at": {"x": 0, "y": "1000mil"}}),
    )["result"]["net_label"]
        .clone();
    run(&mut h, "selection.set", json!({"objects": [label]}));
    h.run();
    h.hover_at(screen(&h, 3000.0, 3000.0));
    h.run();
    h.key_press(Key::F2);
    h.run_steps(4);
    let field = h
        .get_by_role_and_label(Role::TextInput, "Edit in place")
        .rect();
    wrong.extend(check_in("In-place field", &h, around(field)));

    // Tab while placing a Net Label: the Paused chip and the next name.
    let (mut h, _f) = window(dark, density);
    h.hover_at(screen(&h, 1000.0, 1000.0));
    h.run();
    h.key_press(Key::N);
    h.run();
    h.hover_at(screen(&h, 1000.0, 1500.0));
    h.run();
    h.key_press(Key::Tab);
    h.run_steps(4);
    let chip = h.get_by_label("Paused").rect();
    wrong.extend(check_in("Paused chip", &h, around(chip)));
    let next = h.get_by_role_and_label(Role::TextInput, "Next name").rect();
    wrong.extend(check_in("Next name field", &h, around(next)));

    wrong
}

fn check(dark: bool, density: Density) {
    let mut wrong = dialogs_alone(dark, density);
    wrong.extend(editor_chrome(dark, density));
    let theme = if dark { "dark" } else { "light" };
    assert!(
        wrong.is_empty(),
        "{theme} {density:?} {} px: text does not fit:\n{}",
        text_size::get(),
        wrong.join("\n")
    );
}

#[test]
fn the_schematic_dialogs_and_editor_chrome_fit_their_text_in_dark_comfortable() {
    check(true, Density::Comfortable);
}

#[test]
fn the_schematic_dialogs_and_editor_chrome_fit_their_text_in_dark_compact() {
    check(true, Density::Compact);
}

#[test]
fn the_schematic_dialogs_and_editor_chrome_fit_their_text_in_light_comfortable() {
    check(false, Density::Comfortable);
}

#[test]
fn the_schematic_dialogs_and_editor_chrome_fit_their_text_in_light_compact() {
    check(false, Density::Compact);
}

#[test]
fn the_schematic_dialogs_and_editor_chrome_fit_their_text_in_light_comfortable_at_the_smallest_text_size()
 {
    text_size::at(text_size::SMALLEST, || check(false, Density::Comfortable));
}

#[test]
fn the_schematic_dialogs_and_editor_chrome_fit_their_text_in_dark_compact_at_the_smallest_text_size()
 {
    text_size::at(text_size::SMALLEST, || check(true, Density::Compact));
}

#[test]
fn the_schematic_dialogs_and_editor_chrome_fit_their_text_in_dark_comfortable_at_the_largest_text_size()
 {
    text_size::at(text_size::LARGEST, || check(true, Density::Comfortable));
}

#[test]
fn the_schematic_dialogs_and_editor_chrome_fit_their_text_in_light_compact_at_the_largest_text_size()
 {
    text_size::at(text_size::LARGEST, || check(false, Density::Compact));
}
