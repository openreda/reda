//! The sheet frame in the Properties panel, with nothing selected on a sheet: paper size
//! (standard or custom), orientation, Border Zones, title block, drawing number and
//! revision, each one `schematic.sheet.set_frame`.

use egui::Key;
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::model::{FrameSize, Orientation, SheetFrame, SheetSize, Uuid};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const SHEET: &str = "00000000-0000-4000-8000-000000000002";

fn harness(home: &TempDir, work: &TempDir) -> Harness<'static, App> {
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(None)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Demo").unwrap();
    shell
        .run("document.open", json!({"document": SHEET}))
        .unwrap();
    let mut h = Harness::builder()
        .with_size([1400.0, 2600.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    h
}

fn frame(h: &Harness<'_, App>) -> SheetFrame {
    let sheet: Uuid = SHEET.parse().unwrap();
    let project = h.state().shell().engine().project().unwrap();
    project.schematic_sheet(sheet).unwrap().frame_settings()
}

fn last_entry(h: &Harness<'_, App>) -> (String, Value) {
    let entry = (h.state().shell().engine().journal().iter())
        .rfind(|e| e.outcome.is_ok())
        .unwrap();
    (entry.id.clone(), entry.params.clone())
}

fn type_into(h: &mut Harness<'static, App>, field: &str, text: &str) {
    h.get_by_role_and_label(Role::TextInput, field).click();
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
    h.get_by_role_and_label(Role::TextInput, field)
        .type_text(text);
    h.run();
    h.key_press(Key::Enter);
    h.run();
}

#[test]
fn the_sheet_region_sets_size_orientation_zones_and_title_block() {
    let (home, work) = (TempDir::new("sfp-home"), TempDir::new("sfp-work"));
    let mut h = harness(&home, &work);
    assert_eq!(frame(&h).size, FrameSize::Standard(SheetSize::A4));

    h.get_by_role_and_label(Role::ComboBox, "Size").click();
    h.run();
    h.get_by_label("A3").click();
    h.run();
    assert_eq!(
        last_entry(&h),
        (
            "schematic.sheet.set_frame".to_owned(),
            json!({"document": SHEET, "size": "a3"})
        )
    );
    assert_eq!(frame(&h).size, FrameSize::Standard(SheetSize::A3));

    h.get_by_role_and_label(Role::RadioButton, "Portrait")
        .click();
    h.run();
    assert_eq!(frame(&h).orientation, Orientation::Portrait);

    h.get_by_label("Border Zones").click();
    h.run();
    assert!(!frame(&h).border_zones);
    h.get_by_label("Title block").click();
    h.run();
    assert!(!frame(&h).title_block);
}

#[test]
fn a_custom_size_takes_a_width_and_height_and_the_sheet_its_own_numbers() {
    let (home, work) = (TempDir::new("sfp-home"), TempDir::new("sfp-work"));
    let mut h = harness(&home, &work);
    h.get_by_role_and_label(Role::ComboBox, "Size").click();
    h.run();
    h.get_by_label("Custom").click();
    h.run();
    // Custom starts from the paper it had: A4, 297 × 210 mm.
    let mm = |v: i64| reda_ui::engine::model::geom::Nm(v * 1_000_000);
    assert_eq!(
        frame(&h).size,
        FrameSize::Custom {
            width: mm(297),
            height: mm(210)
        }
    );
    // A bare number is in millimetres.
    type_into(&mut h, "Width", "500");
    assert_eq!(
        frame(&h).size,
        FrameSize::Custom {
            width: mm(500),
            height: mm(210)
        }
    );
    type_into(&mut h, "Height", "10in");
    assert_eq!(
        frame(&h).size,
        FrameSize::Custom {
            width: mm(500),
            height: reda_ui::engine::model::geom::Nm(254_000_000)
        }
    );

    type_into(&mut h, "Drawing number", "D-17");
    assert_eq!(frame(&h).drawing_number, "D-17");
    type_into(&mut h, "Sheet revision", "B");
    assert_eq!(frame(&h).revision, "B");
}
