//! A GUI call that lacks required params opens a form generated from the Command's
//! params schema, which flags each field as it is filled in; a Command can replace that
//! form with a hand-written dialog. A call with full params never opens a dialog.

use std::path::{Path, PathBuf};

use reda_testkit::TempDir;
use reda_ui::command_dialog::{Call, Dialogs, Input, Outcome, ParamForm, ParamsDialog};
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::shell::Shell;
use serde_json::{Value, json};

fn engine_in(home: &Path) -> Engine {
    Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build()
}

fn make_project(folder: &Path, name: &str) -> PathBuf {
    let mut engine = Engine::builder()
        .user_dirs(None)
        .starter_library(None)
        .build();
    engine
        .run("project.new", json!({"path": folder, "name": name}))
        .unwrap();
    folder.join(name).join(format!("{name}.reda"))
}

fn shell_with_project(home: &Path, work: &Path) -> Shell {
    let mut shell = Shell::new(engine_in(home));
    shell.open_project(&make_project(work, "Demo")).unwrap();
    shell
}

fn form(shell: &Shell, id: &str, given: Value) -> ParamForm {
    ParamForm::new(shell.engine().registry(), id, &given)
}

fn problems(shell: &Shell, form: &ParamForm) -> Vec<(String, String)> {
    form.problems(shell.engine().registry())
}

fn field_names(form: &ParamForm) -> Vec<&str> {
    form.fields().iter().map(|f| f.name.as_str()).collect()
}

#[test]
fn a_call_with_full_params_runs_at_once_and_never_opens_a_dialog() {
    let (home, work) = (TempDir::new("form-home"), TempDir::new("form-work"));
    let mut shell = shell_with_project(home.path(), work.path());
    let dialogs = Dialogs::new();
    let documents = shell.engine().project().unwrap().documents().count();
    match dialogs.call(&mut shell, "document.add", json!({"kind": "schematic"})) {
        Call::Ran(result) => assert!(result.is_ok(), "{result:?}"),
        Call::Dialog(_) => panic!("full params opened a dialog"),
    }
    assert_eq!(
        shell.engine().project().unwrap().documents().count(),
        documents + 1
    );
    // Optional params may be left out; a Preset's params are fixed.
    assert!(matches!(
        dialogs.call(&mut shell, "view.zoom_fit", json!({})),
        Call::Ran(_)
    ));
    assert!(matches!(
        dialogs.call(&mut shell, "panel.show:messages", json!({})),
        Call::Ran(Ok(_))
    ));
    // Full but wrong params run and report the error rather than open a form.
    match dialogs.call(&mut shell, "document.add", json!({"kind": "nope"})) {
        Call::Ran(Err(e)) => assert!(e.message.contains("kind"), "{}", e.message),
        other => panic!("expected an error, got {}", describe(&other)),
    }
}

fn describe(call: &Call) -> &'static str {
    match call {
        Call::Ran(_) => "a run",
        Call::Dialog(_) => "a dialog",
    }
}

#[test]
fn a_call_lacking_required_params_opens_the_generated_form_with_what_was_given() {
    let (home, work) = (TempDir::new("form-home"), TempDir::new("form-work"));
    let mut shell = shell_with_project(home.path(), work.path());
    let dialogs = Dialogs::new();
    let documents = shell.engine().project().unwrap().documents().count();
    let Call::Dialog(dialog) =
        dialogs.call(&mut shell, "document.rename", json!({"title": "Power"}))
    else {
        panic!("a missing document should open a form");
    };
    assert_eq!(dialog.command(), "document.rename");
    let form = dialog
        .as_any()
        .downcast_ref::<ParamForm>()
        .expect("the generated form");
    assert_eq!(field_names(form), ["document", "title"]);
    assert_eq!(
        form.field("title").unwrap().input,
        Input::Text("Power".into())
    );
    assert_eq!(
        shell.engine().project().unwrap().documents().count(),
        documents
    );
}

#[test]
fn fields_follow_the_schema_types() {
    let home = TempDir::new("form-home");
    let shell = Shell::new(engine_in(home.path()));

    let f = form(&shell, "project.new", json!({}));
    // Required fields first, in the Command's order, then the rest by name.
    assert_eq!(field_names(&f), ["path", "name", "board", "fab_preset"]);
    let name = f.field("name").unwrap();
    assert!(name.required);
    assert_eq!(name.label, "Name");
    assert!(
        name.description.contains("folder name"),
        "{}",
        name.description
    );
    assert_eq!(f.field("fab_preset").unwrap().label, "Fab preset");
    assert!(!f.field("fab_preset").unwrap().required);
    // An object param is typed as JSON.
    assert_eq!(f.field("board").unwrap().input, Input::Json(String::new()));

    let f = form(&shell, "document.add", json!({}));
    let Input::Choice { options, selected } = &f.field("kind").unwrap().input else {
        panic!("an enum is a choice");
    };
    assert_eq!(options, &[json!("schematic"), json!("pcb")]);
    assert_eq!(*selected, None);

    // Choices also come from `oneOf` consts.
    let f = form(&shell, "drc.set_severity", json!({"severity": "warning"}));
    let Input::Choice { options, selected } = &f.field("check").unwrap().input else {
        panic!("a oneOf of consts is a choice");
    };
    assert_eq!(options[0], json!("clearance"));
    assert_eq!(*selected, None);
    let Input::Choice { options, selected } = &f.field("severity").unwrap().input else {
        panic!("an enum is a choice");
    };
    assert_eq!(options[selected.unwrap()], json!("warning"));

    let f = form(&shell, "document.reorder", json!({"index": 2}));
    assert_eq!(f.field("index").unwrap().input, Input::Integer("2".into()));
    assert_eq!(
        f.field("document").unwrap().input,
        Input::Text(String::new())
    );

    let f = form(
        &shell,
        "pcb.board_outline.add_rect",
        json!({"width": "10mm"}),
    );
    assert_eq!(
        f.field("width").unwrap().input,
        Input::Measure("10mm".into())
    );
    assert_eq!(
        f.field("corner_radius").unwrap().input,
        Input::Measure(String::new())
    );

    let f = form(&shell, "layout.dock.collapse", json!({}));
    assert_eq!(f.field("collapsed").unwrap().input, Input::Flag(None));
}

#[test]
fn the_form_flags_missing_and_bad_fields_inline_until_the_params_fit() {
    let home = TempDir::new("form-home");
    let shell = Shell::new(engine_in(home.path()));
    let mut f = form(&shell, "pcb.board_outline.add_rect", json!({}));
    assert_eq!(
        problems(&shell, &f),
        [
            ("width".to_owned(), "Required".to_owned()),
            ("height".to_owned(), "Required".to_owned())
        ]
    );
    assert_eq!(f.params(shell.engine().registry()), None);

    f.set_text("width", "12ft");
    f.set_text("height", "5mm");
    let p = problems(&shell, &f);
    assert_eq!(p.len(), 1, "{p:?}");
    assert_eq!(p[0].0, "width");
    assert!(p[0].1.contains("unit"), "{}", p[0].1);

    f.set_text("width", "1000");
    f.set_text("corner_radius", "abc");
    assert_eq!(problems(&shell, &f)[0].0, "corner_radius");
    f.set_text("corner_radius", "");
    assert_eq!(problems(&shell, &f), []);
    assert_eq!(
        f.params(shell.engine().registry()),
        Some(json!({"width": 1000, "height": "5mm"}))
    );
}

#[test]
fn integers_choices_flags_and_json_turn_into_params() {
    let home = TempDir::new("form-home");
    let shell = Shell::new(engine_in(home.path()));
    let registry = shell.engine().registry();

    let mut f = form(&shell, "document.reorder", json!({}));
    f.set_text("document", "not a uuid");
    f.set_text("index", "1.5");
    let p = problems(&shell, &f);
    assert_eq!(
        p.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
        ["document", "index"]
    );
    assert_eq!(p[1].1, "Enter a whole number");
    f.set_text("document", "00000000-0000-4000-8000-000000000003");
    f.set_text("index", "-1");
    assert_eq!(problems(&shell, &f)[0].0, "index");
    f.set_text("index", "0");
    assert_eq!(
        f.params(registry),
        Some(json!({"document": "00000000-0000-4000-8000-000000000003", "index": 0}))
    );

    let mut f = form(&shell, "document.add", json!({}));
    f.choose("kind", Some(1));
    assert_eq!(f.params(registry), Some(json!({"kind": "pcb"})));

    let mut f = form(&shell, "layout.dock.collapse", json!({"dock": "left"}));
    assert_eq!(f.params(registry), Some(json!({"dock": "left"})));
    f.set_flag("collapsed", Some(false));
    assert_eq!(
        f.params(registry),
        Some(json!({"dock": "left", "collapsed": false}))
    );

    let mut f = form(&shell, "project.new", json!({"path": "/x", "name": "A"}));
    f.set_text("board", "{\"width\": ");
    assert!(problems(&shell, &f)[0].1.starts_with("Not valid JSON"));
    f.set_text("board", "{\"width\": \"1mm\", \"height\": true}");
    let p = problems(&shell, &f);
    assert_eq!(p[0].0, "board");
    assert!(p[0].1.contains("height"), "{}", p[0].1);
    f.set_text("board", "{\"width\": \"1mm\", \"height\": \"2mm\"}");
    assert_eq!(
        f.params(registry),
        Some(json!({"path": "/x", "name": "A", "board": {"width": "1mm", "height": "2mm"}}))
    );
}

/// A stand-in for a hand-written dialog such as Sync's Change List.
struct Fixed(Value);

impl ParamsDialog for Fixed {
    fn command(&self) -> &str {
        "document.rename"
    }
    fn show(&mut self, _ctx: &egui::Context, _shell: &mut Shell) -> Outcome {
        Outcome::Run(self.0.clone())
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[test]
fn a_command_can_replace_the_generated_form_with_a_hand_written_dialog() {
    let (home, work) = (TempDir::new("form-home"), TempDir::new("form-work"));
    let mut shell = shell_with_project(home.path(), work.path());
    let mut dialogs = Dialogs::new();
    dialogs.register("document.rename", |_shell, given| {
        Box::new(Fixed(given)) as Box<dyn ParamsDialog>
    });
    let Call::Dialog(dialog) = dialogs.call(&mut shell, "document.rename", json!({"title": "X"}))
    else {
        panic!("missing params open the dialog");
    };
    let fixed = dialog
        .as_any()
        .downcast_ref::<Fixed>()
        .expect("the hand-written dialog");
    assert_eq!(fixed.0, json!({"title": "X"}));
    // Other Commands still get the generated form.
    let Call::Dialog(dialog) = dialogs.call(&mut shell, "document.add", json!({})) else {
        panic!("missing params open a form");
    };
    assert!(dialog.as_any().is::<ParamForm>());
}
