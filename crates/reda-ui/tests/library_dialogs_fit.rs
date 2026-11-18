//! The Library dialogs fit their text: New Library, Import part, Copy to Library and Make
//! Project Library, Paste Array, the column-mapping paste, the Symbol Wizard, the
//! Footprint generator form (new and Regenerate…) and the generated params form of every
//! Command. Each is drawn alone over a window of a common size, in Comfortable and Compact
//! and in light and dark, and no text in it is cut short with an ellipsis, runs out of the
//! field, dropdown or column that holds it, or breaks over lines unless it is a sentence.

#[path = "support/text_size.rs"]
mod text_size;

use std::path::Path;

use egui::Rect;
use egui_kittest::Harness;
use reda_testkit::TempDir;
use reda_ui::command_dialog::{Outcome, ParamForm, ParamsDialog, lacks_required};
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::model::{LibraryElement, Uuid};
use reda_ui::library_dialogs::{
    GenerateDialog, ImportDialog, NewLibraryDialog, PasteArrayDialog, PasteMapping, PasteOutcome,
    SymbolWizardDialog, ToLibraryDialog,
};
use reda_ui::shell::Shell;
use reda_ui::theme::{self, Density, Settings};
use serde_json::{Value, json};

const MINE: &str = "000000aa-0000-4000-8000-000000000000";
const CHIP: &str = "library.footprint.generate.chip";

/// The window the dialogs are drawn over: a laptop's REDA window.
const WINDOW: [f32; 2] = [1280.0, 800.0];

/// A Shell on a Project, with an empty user Library "Mine" registered and a Symbol "Bus"
/// open in it with two Pins.
fn shell(home: &Path) -> (Shell, Uuid, Vec<Uuid>) {
    let lib = home.join("lib");
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
        .user_dirs(Some(UserDirs::under(&home.join("home"))))
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
    let work = home.join("work");
    std::fs::create_dir_all(&work).unwrap();
    shell.new_project(&work, "Fit").unwrap();
    let symbol: Uuid = shell
        .run(
            "library.element.new",
            json!({"library": MINE, "kind": "symbol", "name": "Bus"}),
        )
        .unwrap()["element"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let unit = match shell.engine().element(symbol) {
        Some(LibraryElement::Symbol(s)) => s.units[0].uuid,
        _ => panic!("the Symbol is not open"),
    };
    for (number, name) in [("1", "D0"), ("2", "D1")] {
        shell
            .run(
                "library.symbol.pin.add",
                json!({"symbol": symbol, "unit": unit, "name": name, "number": number,
                       "at": {"x": 0, "y": 0}, "electrical_type": "bidirectional"}),
            )
            .unwrap();
    }
    let pins = match shell.engine().element(symbol) {
        Some(LibraryElement::Symbol(s)) => s.pins.iter().map(|p| p.uuid).collect(),
        _ => unreachable!(),
    };
    (shell, symbol, pins)
}

/// A dialog and the Shell it is drawn for.
struct Window {
    dialog: Box<dyn ParamsDialog>,
    shell: Shell,
}

/// The column-mapping paste as a dialog of its own.
struct Paste(PasteMapping);

impl ParamsDialog for Paste {
    fn command(&self) -> &str {
        "paste"
    }

    fn show(&mut self, ctx: &egui::Context, _shell: &mut Shell) -> Outcome {
        match self.0.show(ctx, egui::Id::NULL.with("paste"), "Apply") {
            PasteOutcome::Open => Outcome::Open,
            PasteOutcome::Cancel => Outcome::Cancel,
            PasteOutcome::Apply(_) => Outcome::Run(Value::Null),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn harness(window: Window, dark: bool, density: Density) -> Harness<'static, Window> {
    let settings = Settings {
        density,
        text_size: text_size::get(),
        ..Settings::default()
    };
    let mut themed = false;
    let mut h = Harness::builder().with_size(WINDOW).build_ui_state(
        move |ui, w: &mut Window| {
            let ctx = ui.ctx().clone();
            // The theme and its fonts go in before the first pass lays out any text.
            if !themed {
                themed = true;
                theme::set(&ctx, &settings);
                ctx.set_theme(if dark {
                    egui::Theme::Dark
                } else {
                    egui::Theme::Light
                });
            }
            w.dialog.show(&ctx, &mut w.shell);
        },
        window,
    );
    // Tables measure their columns on their first frame and dialogs settle their size
    // and fade in over a few more.
    h.run_steps(30);
    h
}

/// What is wrong with the text drawn, one line per text: cut short with an ellipsis, out
/// of its clip sideways (a field, a dropdown, a column), or a label broken over lines.
/// Text a scroll area cuts at its top or bottom is only scrolled out of view.
fn misfits(h: &Harness<'static, Window>) -> Vec<String> {
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
            // What a scroll area shows of it must be inside the window.
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

/// The dialogs to look at, by name, made for `shell`.
fn dialogs(shell: &Shell, symbol: Uuid, pins: &[Uuid]) -> Vec<(String, Box<dyn ParamsDialog>)> {
    let mut chip = GenerateDialog::new(shell, CHIP, &json!({"library": MINE}));
    chip.form_mut().choose("/size", Some(&json!("0805")));
    let mut soic = GenerateDialog::new(
        shell,
        "library.footprint.generate.soic",
        &json!({"library": MINE}),
    );
    soic.form_mut().set_text("/pins", "8");
    let mut paste = PasteMapping::new(
        "Number\tName\tType\n1\tVCC\tpower_in\n2\tGND\tpower_in\n3\tSDA_LONG_NAME\tbidirectional",
    );
    paste.table.header = true;
    let mut out: Vec<(String, Box<dyn ParamsDialog>)> = vec![
        (
            "New Library".into(),
            Box::new(NewLibraryDialog::new(shell, &json!({}))),
        ),
        (
            "Import part".into(),
            Box::new(ImportDialog::new(&json!({"library": MINE}))),
        ),
        (
            "Make Project Library".into(),
            Box::new(ToLibraryDialog::new(
                shell,
                "library.extract_from_project",
                &json!({}),
            )),
        ),
        (
            "Paste Array".into(),
            Box::new(PasteArrayDialog::new(
                shell,
                &json!({"element": symbol, "objects": pins}),
            )),
        ),
        ("Paste".into(), Box::new(Paste(paste))),
        (
            "Symbol Wizard".into(),
            Box::new(SymbolWizardDialog::new(shell, &json!({}))),
        ),
        ("Generate Chip".into(), Box::new(chip)),
        ("Generate SOIC".into(), Box::new(soic)),
    ];
    // The generated form of every Command that opens one when called with nothing.
    let registry = shell.engine().registry();
    let mut ids: Vec<&str> = registry
        .commands()
        .map(|m| m.id)
        .filter(|id| lacks_required(registry, id, &json!({})))
        .collect();
    ids.sort_unstable();
    for id in ids {
        out.push((
            format!("form {id}"),
            Box::new(ParamForm::new(registry, id, &json!({}))),
        ));
    }
    out
}

fn check(dark: bool, density: Density) {
    let home = TempDir::new("library-dialogs-fit");
    let (shell, symbol, pins) = shell(home.path());
    let all = dialogs(&shell, symbol, &pins);
    let mut wrong = Vec::new();
    let mut shell = Some(shell);
    for (name, dialog) in all {
        let w = Window {
            dialog,
            shell: shell.take().expect("the Shell comes back"),
        };
        let h = harness(w, dark, density);
        for line in misfits(&h) {
            wrong.push(format!("{name}: {line}"));
        }
        shell = Some(h.into_state().shell);
    }
    let theme = if dark { "dark" } else { "light" };
    assert!(
        wrong.is_empty(),
        "{theme} {density:?} {} px: text does not fit:\n{}",
        text_size::get(),
        wrong.join("\n")
    );
}

#[test]
fn the_library_dialogs_fit_their_text_in_dark_comfortable() {
    check(true, Density::Comfortable);
}

#[test]
fn the_library_dialogs_fit_their_text_in_dark_compact() {
    check(true, Density::Compact);
}

#[test]
fn the_library_dialogs_fit_their_text_in_light_comfortable() {
    check(false, Density::Comfortable);
}

#[test]
fn the_library_dialogs_fit_their_text_in_light_compact() {
    check(false, Density::Compact);
}

#[test]
fn the_library_dialogs_fit_their_text_in_light_comfortable_at_the_smallest_text_size() {
    text_size::at(text_size::SMALLEST, || check(false, Density::Comfortable));
}

#[test]
fn the_library_dialogs_fit_their_text_in_dark_compact_at_the_smallest_text_size() {
    text_size::at(text_size::SMALLEST, || check(true, Density::Compact));
}

#[test]
fn the_library_dialogs_fit_their_text_in_dark_comfortable_at_the_largest_text_size() {
    text_size::at(text_size::LARGEST, || check(true, Density::Comfortable));
}

#[test]
fn the_library_dialogs_fit_their_text_in_light_compact_at_the_largest_text_size() {
    text_size::at(text_size::LARGEST, || check(false, Density::Compact));
}
