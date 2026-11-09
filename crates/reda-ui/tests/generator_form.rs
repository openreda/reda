//! The Footprint generator form: every value the generator calculates shown in its row
//! and editable as an override, flagged when it breaks IPC-7351 compliance; the density
//! and each family's own fields; Regenerate… on a stored Footprint with its preview; and
//! the dimension callouts on the preview.

use std::path::Path;

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::command_dialog::Call;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::model::{Footprint, LibraryElement, Uuid};
use reda_ui::library_dialogs::{self, Axis, GenerateDialog, Preview, Section, Side};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const MINE: &str = "000000aa-0000-4000-8000-000000000000";
const CHIP: &str = "library.footprint.generate.chip";

/// A Shell with an empty user Library "Mine" registered.
fn shell(home: &TempDir) -> Shell {
    let lib = home.path().join("lib");
    std::fs::create_dir_all(&lib).unwrap();
    std::fs::write(
        lib.join("Mine.redalib"),
        format!("{{\"format\": 1, \"library\": {{\"uuid\": \"{MINE}\", \"name\": \"Mine\"}}}}"),
    )
    .unwrap();
    let starter = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library/starter");
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(&home.path().join("home"))))
        .starter_library(Some(starter))
        .headless(false)
        .build();
    let mut shell = Shell::new(engine);
    shell
        .run(
            "app.preferences.set",
            json!({"key": "libraries.paths", "value": [lib]}),
        )
        .unwrap();
    shell
}

fn chip_form(shell: &Shell) -> GenerateDialog {
    let mut dialog = GenerateDialog::new(shell, CHIP, &json!({"library": MINE}));
    dialog.form_mut().set_text("/library", MINE);
    dialog.form_mut().choose("/size", Some(&json!("0603")));
    dialog
}

/// The row's text and whether it is calculated.
fn shown(dialog: &GenerateDialog, shell: &Shell, pointer: &str) -> (String, bool, bool) {
    let preview = dialog.preview(shell);
    let s = dialog
        .shown(&preview, pointer)
        .unwrap_or_else(|| panic!("no value shown for {pointer}"));
    (s.text, s.calculated, s.overrides_ipc)
}

fn name(dialog: &GenerateDialog, shell: &Shell) -> String {
    match dialog.preview(shell) {
        Preview::Footprint(g) => g.footprint.name.clone(),
        other => panic!("no land pattern: {other:?}"),
    }
}

fn footprint(shell: &Shell, uuid: Uuid) -> Footprint {
    match shell.engine().element(uuid) {
        Some(LibraryElement::Footprint(f)) => f.clone(),
        other => panic!("no open Footprint: {other:?}"),
    }
}

#[test]
fn every_calculated_value_shows_in_its_row_and_an_override_warns_where_it_was_typed() {
    let home = TempDir::new("genform-calc");
    let shell = shell(&home);
    let mut dialog = chip_form(&shell);
    // Worked by hand for a 0603 at Nominal (see the generator's own test): toe 0.35,
    // heel 0, side 0, courtyard excess 0.25, F 0.10, P 0.05; Pads 0.90 × 0.95 at 1.55.
    for (pointer, text) in [
        ("/goals/fillet/toe", "0.35mm"),
        ("/goals/fillet/heel", "0.00mm"),
        ("/goals/fillet/side", "0.00mm"),
        ("/tolerances/courtyard_excess", "0.25mm"),
        ("/tolerances/fabrication", "0.10mm"),
        ("/tolerances/placement", "0.05mm"),
        ("/pads/length", "0.90mm"),
        ("/pads/width", "0.95mm"),
        ("/pads/spacing", "1.55mm"),
        ("/goals/density", "nominal"),
        ("/outputs/silkscreen_width", "0.12mm"),
    ] {
        assert_eq!(
            shown(&dialog, &shell, pointer),
            (text.to_owned(), true, false),
            "{pointer}"
        );
    }
    // A Pad length typed in overrides IPC-7351's: the row says so, and so do the
    // preview's warnings.
    dialog.form_mut().set_text("/pads/length", "1.2mm");
    assert_eq!(
        shown(&dialog, &shell, "/pads/length"),
        ("1.2mm".to_owned(), false, true)
    );
    let Preview::Footprint(generated) = dialog.preview(&shell) else {
        panic!("no land pattern");
    };
    assert_eq!(generated.warnings.len(), 1);
    assert_eq!(generated.warnings[0].params, ["/pads/length"]);
    assert_eq!(generated.footprint.pads[0].width.0, 1_200_000);
    // Only the typed value goes into the params; the rest stays calculated.
    let params = dialog.run_params(&shell).unwrap();
    assert_eq!(params["pads"], json!({"length": "1.2mm"}));
    assert!(params.get("goals").is_none());
    // A toe goal typed in moves the derived Pad length once the override is cleared.
    dialog.form_mut().clear("/pads/length");
    dialog.form_mut().set_text("/goals/fillet/toe", "0.55mm");
    assert_eq!(
        shown(&dialog, &shell, "/pads/length"),
        ("1.10mm".into(), true, false)
    );
    assert!(shown(&dialog, &shell, "/goals/fillet/toe").2);
}

#[test]
fn the_form_has_the_density_and_each_familys_own_fields_in_sections() {
    let home = TempDir::new("genform-family");
    let shell = shell(&home);
    let mut dialog = chip_form(&shell);
    let section = |d: &GenerateDialog, p: &str| d.form().row(p).map(|r| r.section);
    assert_eq!(section(&dialog, "/library"), Some(Section::Target));
    assert_eq!(section(&dialog, "/size"), Some(Section::Package));
    assert_eq!(
        section(&dialog, "/dimensions/length/min"),
        Some(Section::Package)
    );
    assert_eq!(section(&dialog, "/goals/density"), Some(Section::Goals));
    assert_eq!(
        section(&dialog, "/tolerances/round_off/size"),
        Some(Section::Tolerances)
    );
    assert_eq!(section(&dialog, "/pads/corner_ratio"), Some(Section::Pads));
    assert_eq!(
        section(&dialog, "/outputs/courtyard"),
        Some(Section::Outputs)
    );
    // Most density: larger Pads, a Most name, all recalculated.
    assert_eq!(name(&dialog, &shell), "RESC1608X55N");
    dialog
        .form_mut()
        .choose("/goals/density", Some(&json!("most")));
    assert_eq!(name(&dialog, &shell), "RESC1608X55M");
    assert_eq!(shown(&dialog, &shell, "/goals/fillet/toe").0, "0.55mm");
    assert_eq!(
        shown(&dialog, &shell, "/tolerances/courtyard_excess").0,
        "0.50mm"
    );
    // Another family brings its own fields: a header's rows, pins and pitch, and no
    // density (through-hole stays at level B).
    dialog.set_family(&shell, "library.footprint.generate.header");
    assert_eq!(dialog.form().command(), "library.footprint.generate.header");
    for pointer in ["/rows", "/pins_per_row", "/pitch", "/socket", "/pads/hole"] {
        assert!(section(&dialog, pointer).is_some(), "{pointer}");
    }
    assert!(dialog.form().row("/goals/density").is_none());
    assert_eq!(dialog.form().given("/library"), Some(json!(MINE)));
    dialog.form_mut().set_text("/rows", "2");
    dialog.form_mut().set_text("/pins_per_row", "5");
    dialog.form_mut().choose("/pitch", Some(&json!("2.54")));
    assert!(name(&dialog, &shell).starts_with("HDRV254"));
    assert!(shown(&dialog, &shell, "/pads/hole").1);
}

/// Generates a 0603 into Mine with `extra` params merged in and opens it.
fn generated_chip(shell: &mut Shell, extra: Value) -> Uuid {
    let mut params = json!({"library": MINE, "size": "0603"});
    for (k, v) in extra.as_object().unwrap() {
        params[k] = v.clone();
    }
    let out = shell.run(CHIP, params).unwrap();
    let uuid: Uuid = out["footprint"].as_str().unwrap().parse().unwrap();
    shell
        .run("library.element.open", json!({"element": uuid}))
        .unwrap();
    uuid
}

#[test]
fn regenerate_opens_on_the_stored_params_with_calculated_values_left_to_recalculate() {
    let home = TempDir::new("genform-regen");
    let mut shell = shell(&home);
    let uuid = generated_chip(&mut shell, json!({}));
    let stored = footprint(&shell, uuid).generator.unwrap().params;
    // The stored params hold every value resolved...
    assert_eq!(stored["pads"]["length"], 900_000);
    let dialogs = library_dialogs::dialogs();
    let Call::Dialog(dialog) = dialogs.call(
        &mut shell,
        "library.footprint.regenerate",
        json!({"footprint": uuid}),
    ) else {
        panic!("Regenerate… opens its form");
    };
    let opened = dialog
        .as_any()
        .downcast_ref::<GenerateDialog>()
        .expect("the generator form");
    assert_eq!(opened.regenerating(), Some(uuid));
    assert_eq!(dialog.command(), "library.footprint.regenerate");
    let mut dialog = GenerateDialog::regenerate(&shell, uuid).unwrap();
    assert_eq!(dialog.regenerating(), Some(uuid));
    assert_eq!(dialog.form().command(), CHIP);
    // ...but the form leaves the calculated ones empty, showing them as calculated, and
    // keeps what the package says.
    assert_eq!(dialog.form().given("/pads/length"), None);
    assert_eq!(
        shown(&dialog, &shell, "/pads/length"),
        ("0.90mm".into(), true, false)
    );
    assert_eq!(
        dialog.form().given("/dimensions/length/min"),
        Some(json!("1.45mm"))
    );
    // The preview of the stored params changes nothing.
    let Preview::Footprint(g) = dialog.preview(&shell) else {
        panic!("no preview");
    };
    assert_eq!(g.unchanged, Some(true));
    assert!(g.lost.is_empty());
    // A change of density recalculates the Pads, and the preview lists what changes.
    dialog
        .form_mut()
        .choose("/goals/density", Some(&json!("most")));
    assert_eq!(shown(&dialog, &shell, "/pads/length").0, "1.10mm");
    let Preview::Footprint(g) = dialog.preview(&shell) else {
        panic!("no preview");
    };
    assert_eq!(g.unchanged, Some(false));
    assert!(g.lost.iter().any(|l| l.what == "Pad 1"), "{:?}", g.lost);
    // Regenerate runs with the form's params: no stale Pad size goes with them.
    let params = dialog.run_params(&shell).unwrap();
    assert_eq!(params["footprint"], json!(uuid));
    assert_eq!(params["params"]["goals"]["density"], "most");
    assert!(params["params"].get("pads").is_none(), "{params}");
    // The suggested description follows the density; the package's dimensions and
    // the target stay as stored.
    assert_eq!(
        params["params"]["description"],
        "Chip resistor, 0603 (1608 metric), IPC-7351 Most"
    );
    assert_eq!(params["params"]["dimensions"]["height"], "0.55mm");
    shell.run("library.footprint.regenerate", params).unwrap();
    let after = footprint(&shell, uuid);
    assert_eq!(after.uuid, uuid);
    assert_eq!(after.pads[0].width.0, 1_100_000);
    assert_eq!(after.generator.unwrap().params["goals"]["density"], "most");
}

#[test]
fn regenerate_keeps_a_stored_override_and_its_warning() {
    let home = TempDir::new("genform-regen-override");
    let mut shell = shell(&home);
    let uuid = generated_chip(&mut shell, json!({"goals": {"fillet": {"toe": "0.6mm"}}}));
    let dialog = GenerateDialog::regenerate(&shell, uuid).unwrap();
    assert_eq!(
        dialog.form().given("/goals/fillet/toe"),
        Some(json!("0.60mm"))
    );
    assert!(shown(&dialog, &shell, "/goals/fillet/toe").2);
    // The derived Pad length is left to follow the toe goal.
    assert_eq!(dialog.form().given("/pads/length"), None);
    assert!(shown(&dialog, &shell, "/pads/length").1);
    // Not made by a generator: no Regenerate… form.
    let plain = shell
        .run(
            "library.element.new",
            json!({"kind": "footprint", "library": MINE, "name": "Plain"}),
        )
        .unwrap();
    let plain: Uuid = plain["element"].as_str().unwrap().parse().unwrap();
    assert!(GenerateDialog::regenerate(&shell, plain).is_none());
}

#[test]
fn the_preview_has_dimension_callouts_for_the_span_the_pad_and_the_pitch() {
    let home = TempDir::new("genform-callouts");
    let shell = shell(&home);
    let dialog = chip_form(&shell);
    let Preview::Footprint(g) = dialog.preview(&shell) else {
        panic!("no preview");
    };
    let texts: Vec<(Axis, Side, String)> = library_dialogs::callouts(&g.footprint)
        .into_iter()
        .map(|c| (c.axis, c.side, c.text))
        .collect();
    // Z 2.45 overall and X 0.95 across; Pad 1 is 0.90 × 0.95.
    assert_eq!(
        texts,
        [
            (Axis::X, Side::Low, "2.45".to_owned()),
            (Axis::Y, Side::Low, "0.95".to_owned()),
            (Axis::X, Side::High, "0.90".to_owned()),
            (Axis::Y, Side::High, "0.95".to_owned()),
        ]
    );
    // A row of Pads also gets its pitch.
    let mut soic = GenerateDialog::new(&shell, "library.footprint.generate.soic", &json!({}));
    soic.form_mut().set_text("/library", MINE);
    soic.form_mut().set_text("/pins", "8");
    let Preview::Footprint(g) = soic.preview(&shell) else {
        panic!("no preview");
    };
    let callouts = library_dialogs::callouts(&g.footprint);
    assert!(callouts.iter().any(|c| c.text == "1.27"), "{callouts:?}");
}

/// The window on Mine's generated 0603, open in its editor.
fn window(home: &TempDir) -> (Harness<'static, App>, Uuid) {
    let mut shell = shell(home);
    let uuid = generated_chip(&mut shell, json!({}));
    let mut h = Harness::builder()
        .with_size([1400.0, 1000.0])
        .with_max_steps(200)
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    (h, uuid)
}

#[test]
fn regenerate_in_the_footprint_properties_opens_the_form_and_regenerates_in_place() {
    let home = TempDir::new("genform-window");
    let (mut h, uuid) = window(&home);
    h.get_by_label("Made by Generate Chip Footprint");
    h.get_by_role_and_label(Role::Button, "Regenerate…").click();
    h.run();
    // The form, in sections, with its preview and callouts.
    h.get_by_label("Goals");
    h.get_by_label("Tolerances");
    h.get_by_label("2 Pads");
    h.get_by_role_and_label(Role::ComboBox, "Density (optional)")
        .click();
    h.run();
    h.get_by_label("least").click();
    h.run();
    h.get_by_label_contains("Lost or changed: Pad 1");
    let from = h.state().shell().engine().journal().len();
    h.get_by_role_and_label(Role::Button, "Regenerate").click();
    h.run();
    let entries: Vec<_> = h.state().shell().engine().journal()[from..]
        .iter()
        .filter(|e| e.outcome.is_ok())
        .map(|e| (e.id.clone(), e.params.clone()))
        .collect();
    assert_eq!(entries[0].0, "library.footprint.regenerate");
    assert_eq!(entries[0].1["params"]["goals"]["density"], "least");
    let after = footprint(h.state().shell(), uuid);
    // The Footprint keeps its name; its Pads are Least's now.
    assert_eq!(after.name, "RESC1608X55N");
    assert!(
        after.description.ends_with("IPC-7351 Least"),
        "{}",
        after.description
    );
    assert_eq!(after.generator.unwrap().params["goals"]["density"], "least");
}

#[test]
fn a_typed_override_shows_its_warning_in_the_window() {
    let home = TempDir::new("genform-window-warn");
    let (mut h, _) = window(&home);
    h.get_by_role_and_label(Role::Button, "Regenerate…").click();
    h.run();
    assert!(h.query_by_label_contains("Overrides IPC-7351").is_none());
    h.get_by_role_and_label(Role::TextInput, "Fillet toe (optional)")
        .click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Fillet toe (optional)")
        .type_text("0.6mm");
    h.run();
    h.run();
    h.get_by_label_contains("Overrides IPC-7351");
    h.get_by_role_and_label(Role::Button, "Use calculated")
        .click();
    h.run();
    h.run();
    assert!(h.query_by_label_contains("Overrides IPC-7351").is_none());
}
