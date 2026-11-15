//! Every page of the Preferences window fits its text: at the size the window opens at
//! (1000 × 680), in Comfortable and Compact and in light and dark, no text is cut short
//! with an ellipsis, none runs out of the page sideways, and no label breaks over lines.
//! Sentences that are meant to wrap (a setting's description on All settings) may.

use std::path::Path;

use egui::Rect;
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::preferences;
use reda_ui::preferences_dialog::PageId;
use reda_ui::shell::Shell;
use serde_json::json;

fn harness(home: &Path, theme: &str, density: &str) -> Harness<'static, App> {
    let dirs = UserDirs::under(home);
    preferences::set(&dirs, "general.theme", json!(theme)).unwrap();
    preferences::set(&dirs, "general.density", json!(density)).unwrap();
    // A changed setting, so a page has its count and a row its reset dot.
    preferences::set(&dirs, "user.name", json!("REDA Author")).unwrap();
    preferences::set(&dirs, "libraries.paths", json!(["/libs/a-user-library"])).unwrap();
    let engine = Engine::builder()
        .user_dirs(Some(dirs))
        .starter_library(None)
        .headless(false)
        .build();
    // Room for the window at the size it opens at.
    let mut h = Harness::builder()
        .with_size([1400.0, 900.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(Shell::new(engine)));
    h.run();
    h
}

/// The Preferences window's content: from its search box to its Close button.
fn window(h: &Harness<'static, App>) -> Rect {
    let search = h
        .get_by_role_and_label(Role::SearchInput, "Search settings")
        .rect();
    let close = h.get_by_role_and_label(Role::Button, "Close").rect();
    Rect::from_min_max(search.min, close.max).expand(8.0)
}

/// What is wrong with the text drawn inside `window`, one line per text.
fn misfits(h: &Harness<'static, App>, window: Rect) -> Vec<String> {
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
            if rect.right() > clip.right() + 0.5 || rect.left() < clip.left() - 0.5 {
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
fn on_screen(h: &Harness<'static, App>, window: Rect) -> Vec<(String, i32)> {
    let mut out = Vec::new();
    for clipped in &h.output().shapes {
        let mut shapes = vec![&clipped.shape];
        while let Some(shape) = shapes.pop() {
            match shape {
                egui::Shape::Vec(inner) => shapes.extend(inner),
                egui::Shape::Text(text) if window.contains(text.pos) => {
                    out.push((text.galley.text().to_owned(), text.pos.y.round() as i32));
                }
                _ => {}
            }
        }
    }
    out
}

/// How many screens of a page are looked at. Every page but Keyboard Shortcuts and All
/// settings ends before that; those two go on for hundreds of rows that are all alike,
/// and a frame of them is slow.
const SCREENS: usize = 8;

/// What does not fit on the page shown: the page is scrolled a screen at a time until
/// nothing moves, or [`SCREENS`] screens have been looked at.
fn misfits_down_the_page(h: &mut Harness<'static, App>) -> Vec<String> {
    let mut wrong = Vec::new();
    let mut before = Vec::new();
    for _ in 0..SCREENS {
        let window = window(h);
        for line in misfits(h, window) {
            if !wrong.contains(&line) {
                wrong.push(line);
            }
        }
        let now = on_screen(h, window);
        if now == before {
            return wrong;
        }
        before = now;
        // Over the page, right of the sidebar.
        let at = window.center() + egui::vec2(window.width() / 4.0, 0.0);
        h.event(egui::Event::PointerMoved(at));
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -360.0),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(4);
    }
    wrong
}

fn check(theme: &str, density: &str) {
    let home = TempDir::new("prefs-fit");
    let mut h = harness(home.path(), theme, density);
    h.state_mut().open_preferences(None);
    h.run();
    // The window opens at 1000 × 680; its title row, tab strip and padding take the rest.
    let size = window(&h).size();
    assert!(
        (900.0..=1000.0).contains(&size.x) && size.y <= 680.0,
        "the content is {size:?}, which is not that of a 1000 × 680 window"
    );
    let mut wrong = Vec::new();
    let mut seen = 0;
    for page in PageId::tree() {
        h.state_mut().open_preferences(Some(page));
        h.run_steps(4);
        let top = on_screen(&h, window(&h));
        for line in misfits_down_the_page(&mut h) {
            wrong.push(format!("{}: {line}", page.id()));
        }
        if on_screen(&h, window(&h)) != top {
            seen += 1;
        }
    }
    assert!(
        seen > 3,
        "the pages longer than the window were not scrolled"
    );
    // The search results, which gather rows from every page.
    for query in ["grid", "detail"] {
        h.state_mut().open_preferences(Some(PageId::GENERAL));
        h.run();
        h.get_by_role_and_label(Role::SearchInput, "Search settings")
            .click();
        h.run();
        h.get_by_role_and_label(Role::SearchInput, "Search settings")
            .type_text(query);
        h.run_steps(4);
        for line in misfits_down_the_page(&mut h) {
            wrong.push(format!("search {query:?}: {line}"));
        }
        h.state_mut().open_preferences(Some(PageId::ALL_SETTINGS));
        h.run();
    }
    assert!(
        wrong.is_empty(),
        "{theme} {density}: text does not fit:\n{}",
        wrong.join("\n")
    );
}

#[test]
fn every_page_fits_its_text_in_dark_comfortable() {
    check("dark", "comfortable");
}

#[test]
fn every_page_fits_its_text_in_dark_compact() {
    check("dark", "compact");
}

#[test]
fn every_page_fits_its_text_in_light_comfortable() {
    check("light", "comfortable");
}

#[test]
fn every_page_fits_its_text_in_light_compact() {
    check("light", "compact");
}
