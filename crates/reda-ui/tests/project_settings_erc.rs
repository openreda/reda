//! Project Settings ▸ ERC: the checks in a grouped grid with multi-select Set severity, and
//! the editable Pin Matrix, whose changed cells apply as one `erc.set_pin_matrix`.

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::{Cell, ErcCheck, MatrixType, PinType, PortDirection, Severity};
use reda_ui::settings::project::{self, ErcGroup, ErcPage};
use reda_ui::shell::Shell;
use serde_json::json;

fn shell_with_project(home: &TempDir, work: &TempDir) -> Shell {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(None)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Board").unwrap();
    shell
}

fn dialog(shell: &Shell) -> reda_ui::settings::SettingsDialog {
    project::dialog(shell.engine().project().unwrap())
}

fn pin(t: PinType) -> MatrixType {
    MatrixType::Pin(t)
}

#[test]
fn every_check_sits_in_one_group_and_the_matrix_checks_are_set_by_it() {
    let mut seen = Vec::new();
    for group in ErcGroup::ALL {
        for check in group.checks() {
            assert!(!seen.contains(check), "{check:?} in two groups");
            seen.push(*check);
        }
    }
    assert_eq!(seen.len(), ErcCheck::ALL.len(), "every check has a group");
    assert_eq!(
        ErcGroup::of(ErcCheck::PinConflict),
        ErcGroup::of(ErcCheck::UnconnectedPin)
    );
    assert_eq!(ErcGroup::of(ErcCheck::OffGrid).title(), "Drawing");
    assert_eq!(ErcGroup::of(ErcCheck::DuplicateDesignator).title(), "Components");
}

#[test]
fn several_checks_picked_take_one_severity_together() {
    let (home, work) = (TempDir::new("pse-home"), TempDir::new("pse-work"));
    let mut shell = shell_with_project(&home, &work);
    let mut d = dialog(&shell);
    let page = d.page_mut::<ErcPage>().unwrap();
    page.pick(ErcCheck::OffGrid, false);
    page.pick(ErcCheck::OffSheet, true);
    page.pick(ErcCheck::DanglingWire, true);
    // A plain pick starts again; an extending one toggles.
    page.pick(ErcCheck::DanglingWire, true);
    assert_eq!(page.picked(), [ErcCheck::OffGrid, ErcCheck::OffSheet]);
    // The matrix checks have no severity of their own to pick.
    page.pick(ErcCheck::PinConflict, true);
    assert_eq!(page.picked().len(), 2);
    page.set_picked(Severity::Error);
    assert_eq!(page.get(ErcCheck::OffGrid), Severity::Error);
    assert_eq!(page.get(ErcCheck::OffSheet), Severity::Error);
    // A whole group is picked by its heading.
    page.pick_group(ErcGroup::of(ErcCheck::DanglingWire));
    assert!(page.picked().contains(&ErcCheck::DanglingWire));
    assert!(!page.picked().contains(&ErcCheck::OffGrid));

    d.apply(&mut shell).unwrap();
    let erc = &shell.engine().project().unwrap().props().erc;
    assert_eq!(erc.severities.get(ErcCheck::OffSheet), Some(Severity::Error));
}

#[test]
fn a_click_cycles_a_cell_and_the_changes_apply_as_one_pin_matrix_command() {
    let (home, work) = (TempDir::new("pse-home"), TempDir::new("pse-work"));
    let mut shell = shell_with_project(&home, &work);
    let mut d = dialog(&shell);
    let page = d.page_mut::<ErcPage>().unwrap();
    let outputs = Cell::new(pin(PinType::Output), pin(PinType::Output)).unwrap();
    let passive = Cell::unconnected(PinType::Passive);
    assert_eq!(page.cell(outputs), Severity::Error);
    assert!(!page.is_changed_from_default(outputs));
    // Off, Warning, Error, then Off again.
    page.cycle(outputs);
    assert_eq!(page.cell(outputs), Severity::Off);
    assert!(page.is_changed_from_default(outputs));
    page.cycle(passive);
    assert_eq!(page.cell(passive), Severity::Error);
    assert_eq!(
        page.sentence(outputs),
        "Output Pin connected to Output Pin: Off"
    );
    assert_eq!(page.sentence(passive), "Passive Pin unconnected: Error");
    let port = Cell::new(pin(PinType::Output), MatrixType::Port(PortDirection::In)).unwrap();
    assert_eq!(
        page.sentence(port),
        "Output Pin connected to Port in: Warning"
    );

    let steps = d.steps();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].id, "erc.set_pin_matrix");
    assert_eq!(
        steps[0].params,
        json!({"cells": [
            {"a": "output", "b": "output", "severity": "off"},
            {"a": "passive", "b": "unconnected", "severity": "error"},
        ]})
    );
    d.apply(&mut shell).unwrap();
    let matrix = &shell.engine().project().unwrap().props().erc.pin_matrix_overrides;
    assert_eq!(matrix.get(outputs), Severity::Off);

    // A cell put back to its default is reset with a null severity.
    let page = d.page_mut::<ErcPage>().unwrap();
    page.cycle(outputs);
    page.cycle(outputs);
    assert_eq!(page.cell(outputs), Severity::Error);
    assert_eq!(
        d.steps()[0].params,
        json!({"cells": [{"a": "output", "b": "output", "severity": null}]})
    );
}

#[test]
fn the_matrix_menu_sets_every_cell_and_reset_to_defaults_restores_it() {
    let (home, work) = (TempDir::new("pse-home"), TempDir::new("pse-work"));
    let mut shell = shell_with_project(&home, &work);
    let mut d = dialog(&shell);
    let page = d.page_mut::<ErcPage>().unwrap();
    page.set_all(Some(Severity::Warning));
    assert!(Cell::all().all(|c| page.cell(c) == Severity::Warning));
    page.set_all(None);
    assert!(Cell::all().all(|c| page.cell(c) == c.default_severity()));
    assert!(d.steps().is_empty(), "back to the defaults: nothing to apply");

    let page = d.page_mut::<ErcPage>().unwrap();
    page.set_all(Some(Severity::Off));
    d.apply(&mut shell).unwrap();
    d.select(1);
    d.reset_page(&mut shell);
    let page = d.page::<ErcPage>().unwrap();
    assert!(Cell::all().all(|c| page.cell(c) == c.default_severity()));
}
