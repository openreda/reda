//! Every page of Project Settings, Board Setup, the Output Job window and the Change List
//! fits its text: at the size a settings window's content has when the window opens
//! (1000 × 680 less its tab strip and padding), in Comfortable and Compact and in light
//! and dark, no text is cut short with an ellipsis, none runs out of its page sideways,
//! and no label breaks over lines. Sentences that are meant to wrap (a page's notes) may.

use std::path::{Path, PathBuf};

use egui::Rect;
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds};
use reda_ui::settings::layout::SIDEBAR_WIDTH;
use reda_ui::settings::{Outcome, SettingsDialog, board_setup, change_list, output_job, project};
use reda_ui::shell::Shell;
use reda_ui::theme::{self, Density, Settings};
use serde_json::json;

const SHEET: &str = "00000000-0000-4000-8000-000000000002";
const R_10K: &str = "a0000000-0000-4000-8000-000000000100";

/// A settings window's content at its opening size: 1000 × 680 less the card gaps, the
/// tab strip and the card's padding in Comfortable, the roomier Density.
const CONTENT: [f32; 2] = [964.0, 610.0];

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../reda-schematic/tests/library/Test")
}

/// A Shell on a Project with something on every page: two resistors wired to a Net not
/// yet on the board (so the Change List has rows), a Net Class with an override, and a
/// board-wide rule.
fn shell(work: &Path) -> Shell {
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(None)
        .starter_library(Some(library()))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work, "Fit").unwrap();
    for x in ["0mm", "10mm"] {
        shell
            .run(
                "schematic.component.place",
                json!({"document": SHEET, "part": R_10K, "at": {"x": x, "y": 0}}),
            )
            .unwrap();
    }
    shell
        .run(
            "schematic.wire.add",
            json!({"document": SHEET, "points": [
                {"x": "0mm", "y": "-3.81mm"}, {"x": "10mm", "y": "-3.81mm"}]}),
        )
        .unwrap();
    shell
        .run(
            "schematic.net_label.add",
            json!({"document": SHEET, "name": "SIG", "at": {"x": "5mm", "y": "-3.81mm"}}),
        )
        .unwrap();
    shell
        .run(
            "pcb.rules.set",
            json!({"kind": "clearance", "values": {"value": "0.3mm"}}),
        )
        .unwrap();
    shell
        .run("pcb.net_class.add", json!({"name": "Power"}))
        .unwrap();
    shell
}

struct Window {
    dialog: SettingsDialog,
    shell: Shell,
}

/// The settings window `dialog` alone, drawn as its tab draws it, in `theme` and
/// `density`.
fn harness(
    shell: Shell,
    dialog: SettingsDialog,
    dark: bool,
    density: Density,
) -> Harness<'static, Window> {
    let mut h = Harness::builder().with_size(CONTENT).build_ui_state(
        |ui, w: &mut Window| {
            let outcome = w.dialog.show(ui, &mut w.shell);
            assert_eq!(outcome, Outcome::Open);
        },
        Window { dialog, shell },
    );
    let settings = Settings {
        density,
        ..Settings::default()
    };
    theme::set(&h.ctx, &settings);
    h.ctx.set_theme(if dark {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    });
    h.run();
    h
}

/// The page's scroll area on screen: the widest clip rect right of the sidebar.
fn viewport(h: &Harness<'static, Window>, window: Rect) -> Rect {
    let page = window.left() + SIDEBAR_WIDTH;
    h.output()
        .shapes
        .iter()
        .map(|c| c.clip_rect)
        .filter(|clip| clip.left() > page && window.contains_rect(*clip))
        .max_by(|a, b| a.width().total_cmp(&b.width()))
        .unwrap_or(window)
}

/// What is wrong with the text drawn inside `window`, one line per text.
///
/// The page scrolls both ways, so text the page's own edge cuts is only scrolled out of
/// view. What counts is text cut by a narrower clip (a field, a dropdown, the sidebar),
/// text cut short with an ellipsis, a single line wider than the page, and a label that
/// wraps.
fn misfits(h: &Harness<'static, Window>, window: Rect) -> Vec<String> {
    let view = viewport(h, window);
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
            // Text scrolled out of its page is not on screen.
            if !window.contains_rect(rect.intersect(clip)) || !clip.intersects(rect) {
                continue;
            }
            let said = text.galley.text();
            if text.galley.elided {
                out.push(format!("cut short: {said:?}"));
            }
            if rect.width() > view.width() + 0.5 {
                out.push(format!("wider than the page: {said:?}"));
            }
            let narrow = clip.width() < 0.8 * view.width();
            let half_scrolled = clip.center().x > view.left()
                && (rect.left() < view.left() - 0.5 || rect.right() > view.right() + 0.5);
            let out_sideways = rect.right() > clip.right() + 0.5 || rect.left() < clip.left() - 0.5;
            if narrow && !half_scrolled && out_sideways {
                out.push(format!(
                    "runs out sideways by {}: {said:?}",
                    rect.right() - clip.right()
                ));
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

/// The texts on screen inside `window`, to tell when scrolling has reached the end.
fn on_screen(h: &Harness<'static, Window>, window: Rect) -> Vec<(String, i32, i32)> {
    let mut out = Vec::new();
    for clipped in &h.output().shapes {
        let mut shapes = vec![&clipped.shape];
        while let Some(shape) = shapes.pop() {
            match shape {
                egui::Shape::Vec(inner) => shapes.extend(inner),
                egui::Shape::Text(text) if window.contains(text.pos) => out.push((
                    text.galley.text().to_owned(),
                    text.pos.x.round() as i32,
                    text.pos.y.round() as i32,
                )),
                _ => {}
            }
        }
    }
    out
}

/// How many screens of a page are looked at, down and then across.
const SCREENS: usize = 8;

/// Scrolls the page by `delta`, with the pointer over the page.
fn scroll(h: &mut Harness<'static, Window>, delta: egui::Vec2) {
    let window = h.ctx.content_rect();
    let at = egui::pos2(
        window.left() + SIDEBAR_WIDTH + (window.width() - SIDEBAR_WIDTH) / 2.0,
        window.center().y,
    );
    h.event(egui::Event::PointerMoved(at));
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta,
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Unknown,
        modifiers: egui::Modifiers::NONE,
    });
    h.run_steps(4);
}

/// What does not fit on the page shown: the page is scrolled a screen at a time, down
/// and then across, until nothing moves, or [`SCREENS`] screens have been looked at.
fn misfits_over_the_page(h: &mut Harness<'static, Window>) -> Vec<String> {
    let window = h.ctx.content_rect();
    let mut wrong = Vec::new();
    for delta in [egui::vec2(0.0, -360.0), egui::vec2(-480.0, 0.0)] {
        let mut before = Vec::new();
        for _ in 0..SCREENS {
            for line in misfits(h, window) {
                if !wrong.contains(&line) {
                    wrong.push(line);
                }
            }
            let now = on_screen(h, window);
            if now == before {
                break;
            }
            before = now;
            scroll(h, delta);
        }
    }
    wrong
}

/// Every page of the window `make` opens, looked over; returns what does not fit.
fn check_window(
    name: &str,
    dark: bool,
    density: Density,
    make: impl Fn(&Shell) -> SettingsDialog,
    prepare: impl Fn(&mut Harness<'static, Window>),
) -> Vec<String> {
    let work = TempDir::new("settings-fit");
    let shell = shell(work.path());
    let dialog = make(&shell);
    let titles = dialog.page_titles();
    let mut h = harness(shell, dialog, dark, density);
    prepare(&mut h);
    let mut wrong = Vec::new();
    for (i, title) in titles.iter().enumerate() {
        h.state_mut().dialog.select(i);
        h.run_steps(4);
        for line in misfits_over_the_page(&mut h) {
            wrong.push(format!("{name} ▸ {title}: {line}"));
        }
    }
    wrong
}

/// Board Setup with an invalid override typed in and a Fab Preset's rules previewed, so
/// the rule pages show their messages and the preview.
fn board_setup_prepared(h: &mut Harness<'static, Window>) {
    let field = "Power: Clearance";
    h.get_by_role_and_label(Role::TextInput, field).click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, field)
        .type_text("-1");
    h.run();
    h.get_by_label("Apply Generic Rules…").click();
    h.run();
}

fn check(dark: bool, density: Density) {
    let mut wrong = Vec::new();
    wrong.extend(check_window(
        "Project Settings",
        dark,
        density,
        |shell| project::dialog(shell.engine().project().unwrap()),
        |_| {},
    ));
    wrong.extend(check_window(
        "Board Setup",
        dark,
        density,
        |shell| board_setup::dialog(shell.engine().project().unwrap()),
        board_setup_prepared,
    ));
    wrong.extend(check_window(
        "Output Job",
        dark,
        density,
        |shell| {
            let project = shell.engine().project().unwrap();
            let job = project.output_jobs().next().unwrap().uuid;
            output_job::dialog(project, job).unwrap()
        },
        |_| {},
    ));
    wrong.extend(check_window(
        "Change List",
        dark,
        density,
        |shell| change_list::dialog(shell.engine().project().unwrap()),
        |_| {},
    ));
    let theme = if dark { "dark" } else { "light" };
    assert!(
        wrong.is_empty(),
        "{theme} {density:?}: text does not fit:\n{}",
        wrong.join("\n")
    );
}

#[test]
fn every_settings_page_fits_its_text_in_dark_comfortable() {
    check(true, Density::Comfortable);
}

#[test]
fn every_settings_page_fits_its_text_in_dark_compact() {
    check(true, Density::Compact);
}

#[test]
fn every_settings_page_fits_its_text_in_light_comfortable() {
    check(false, Density::Comfortable);
}

#[test]
fn every_settings_page_fits_its_text_in_light_compact() {
    check(false, Density::Compact);
}
