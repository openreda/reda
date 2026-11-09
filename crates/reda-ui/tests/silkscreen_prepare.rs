//! Silkscreen Preparation in the GUI (pcb.md#silkscreen-preparation): Tools menu items on
//! the board and in the Footprint editor open one modal options dialog at the defaults,
//! and text that found no clear place gets a Messages row.

use std::path::Path;

use reda_testkit::TempDir;
use reda_ui::bars::{self, Binding, DocumentType, Item};
use reda_ui::command_dialog::{Call, Dialogs, Input, ParamForm};
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::Severity;
use reda_ui::pcb::silkscreen;
use reda_ui::shell::Shell;
use serde_json::json;

fn shell_with_board(home: &Path, work: &Path) -> Shell {
    let mut engine = Engine::builder()
        .user_dirs(None)
        .starter_library(None)
        .build();
    engine
        .run(
            "project.new",
            json!({"path": work, "name": "Demo", "board": {"width": "30mm", "height": "20mm"}}),
        )
        .unwrap();
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell
        .open_project(&work.join("Demo").join("Demo.reda"))
        .unwrap();
    shell
}

fn tools(ty: DocumentType) -> Vec<(String, Binding)> {
    let bars = bars::for_document(Some(ty));
    let menu = bars.menus.iter().find(|m| m.id == "tools").unwrap();
    menu.items
        .iter()
        .filter_map(|i| match i {
            Item::Action(a) => Some((a.id.clone(), a.binding.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn the_tools_menus_hold_silkscreen_preparation() {
    let board = tools(DocumentType::Pcb);
    assert!(
        board.contains(&(
            "tools.silkscreen_prepare".into(),
            Binding::Command("pcb.silkscreen.prepare".into())
        )),
        "{board:?}"
    );
    let footprint = tools(DocumentType::Footprint);
    assert!(
        footprint.contains(&(
            "tools.silkscreen_prepare".into(),
            Binding::Raw {
                command: "library.footprint.silkscreen.prepare".into(),
                params: json!({"footprint": bars::ACTIVE_DOCUMENT}),
            }
        )),
        "{footprint:?}"
    );
}

#[test]
fn the_dialog_opens_on_every_call_with_the_defaults_filled_in() {
    let (home, work) = (TempDir::new("silk-ui-home"), TempDir::new("silk-ui-work"));
    let mut shell = shell_with_board(home.path(), work.path());
    let mut dialogs = Dialogs::new();
    silkscreen::register_dialogs(&mut dialogs);

    // Every param is optional, but the call still asks.
    let Call::Dialog(dialog) = dialogs.call(&mut shell, "pcb.silkscreen.prepare", json!({})) else {
        panic!("Silkscreen Preparation opens its dialog");
    };
    let form = dialog.as_any().downcast_ref::<ParamForm>().unwrap();
    let input = |name: &str| form.field(name).unwrap().input.clone();
    assert_eq!(
        input("min_remaining_length"),
        Input::Measure("0.5mm".into())
    );
    assert_eq!(input("max_distance"), Input::Measure("2mm".into()));
    assert_eq!(input("clearance"), Input::Measure("0.1mm".into()));
    assert_eq!(input("use_design_rules"), Input::Flag(Some(true)));
    assert_eq!(input("delete_outside_board"), Input::Flag(Some(true)));
    assert_eq!(input("clip_locked"), Input::Flag(Some(false)));
    assert_eq!(input("move_text"), Input::Flag(Some(true)));
    assert!(form.problems(shell.engine().registry()).is_empty());

    // What the dialog gives back runs as one edit.
    let params = form.params(shell.engine().registry()).unwrap();
    assert_eq!(params["min_remaining_length"], "0.5mm");
    shell.run("pcb.silkscreen.prepare", params).unwrap();
    // Other Commands keep running at once when nothing required is missing.
    assert!(matches!(
        dialogs.call(&mut shell, "drc.run", json!({})),
        Call::Ran(_)
    ));
}

#[test]
fn text_with_no_clear_place_gets_a_messages_row() {
    let (home, work) = (TempDir::new("silk-msg-home"), TempDir::new("silk-msg-work"));
    let mut shell = shell_with_board(home.path(), work.path());
    shell
        .run(
            "pcb.free_pad.add",
            json!({"kind": "smd", "at": {"x": "10mm", "y": "10mm"}, "width": "6mm"}),
        )
        .unwrap();
    shell
        .run(
            "pcb.text.add",
            json!({"layer": "silkscreen_top", "text": "STUCK", "x": "10mm", "y": "10mm"}),
        )
        .unwrap();

    shell.run("pcb.silkscreen.prepare", json!({})).unwrap();

    let rows: Vec<_> = shell
        .messages()
        .notes()
        .iter()
        .filter(|n| n.source == silkscreen::SOURCE)
        .collect();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].severity, Severity::Warning);
    assert!(rows[0].text.contains("STUCK"), "{}", rows[0].text);
    let pcb = shell.engine().project().unwrap().pcb().unwrap().uuid;
    assert_eq!(rows[0].document, Some(pcb));

    // A run where every text finds its place clears the row.
    shell
        .run("pcb.silkscreen.prepare", json!({"max_distance": "6mm"}))
        .unwrap();
    assert!(
        !shell
            .messages()
            .notes()
            .iter()
            .any(|n| n.source == silkscreen::SOURCE)
    );
}
