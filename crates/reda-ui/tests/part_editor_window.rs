//! The Part editor's depth in the window, driven through AccessKit with `egui_kittest`
//! (library.md#part-editor, #pins-panel): its four regions, In BOM and In CPL with the
//! reset on a type change, New and Duplicate & Edit for the Symbol and Footprints, Add ▸
//! Generate, and the Pins Panel's column per alternate Footprint with its red marks and
//! hover cross-probing.

use std::path::Path;

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FixedClock, SeededIds, UserDirs};
use reda_ui::engine::model::{LibraryElement, Part, Uuid};
use reda_ui::shell::Shell;
use serde_json::{Value, json};

const MINE: &str = "000000aa-0000-4000-8000-000000000000";

fn shell(home: &Path) -> Shell {
    let lib = home.join("lib");
    std::fs::create_dir_all(&lib).unwrap();
    std::fs::write(
        lib.join("Mine.redalib"),
        format!("{{\"format\": 1, \"library\": {{\"uuid\": \"{MINE}\", \"name\": \"Mine\"}}}}"),
    )
    .unwrap();
    let engine = Engine::builder()
        .ids(SeededIds::new(0))
        .clock(FixedClock::EPOCH)
        .user_dirs(Some(UserDirs::under(&home.join("home"))))
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

fn new_element(shell: &mut Shell, kind: &str, name: &str) -> Uuid {
    let out = shell
        .run(
            "library.element.new",
            json!({"library": MINE, "kind": kind, "name": name}),
        )
        .unwrap();
    out["element"].as_str().unwrap().parse().unwrap()
}

fn harness(shell: Shell) -> Harness<'static, App> {
    let mut h = Harness::builder()
        .with_size([1600.0, 1600.0])
        .with_max_steps(200)
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    h
}

fn part(h: &Harness<'_, App>, uuid: Uuid) -> Part {
    match h.state().shell().engine().element(uuid) {
        Some(LibraryElement::Part(p)) => p.clone(),
        other => panic!("no open Part: {other:?}"),
    }
}

fn name_of(h: &Harness<'_, App>, uuid: Uuid) -> String {
    h.state()
        .shell()
        .engine()
        .element(uuid)
        .map(|e| e.name().to_owned())
        .expect("open")
}

fn click(h: &mut Harness<'_, App>, label: &str) {
    let node = h.get_all_by_label(label).last().unwrap();
    node.scroll_to_me();
    h.run();
    h.get_all_by_label(label).last().unwrap().click();
    h.run_steps(8);
}

fn type_into(h: &mut Harness<'_, App>, label: &str, text: &str) {
    h.get_by_role_and_label(Role::TextInput, label)
        .scroll_to_me();
    h.run();
    h.get_by_role_and_label(Role::TextInput, label).click();
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run();
    h.get_by_role_and_label(Role::TextInput, label)
        .type_text(text);
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
}

fn checked(h: &Harness<'_, App>, label: &str) -> bool {
    h.get_by_role_and_label(Role::CheckBox, label)
        .accesskit_node()
        .toggled()
        == Some(egui::accesskit::Toggled::True)
}

fn show(h: &mut Harness<'_, App>, document: Uuid) {
    h.state_mut()
        .shell_mut()
        .run("document.open", json!({"document": document}))
        .unwrap();
    h.run();
}

#[test]
fn the_form_has_four_regions_and_a_type_change_resets_the_flags() {
    let home = TempDir::new("part-window-regions");
    let mut shell = shell(home.path());
    let uuid = new_element(&mut shell, "part", "Bolt");
    let mut h = harness(shell);
    for region in ["Part", "Symbol", "Footprints", "Fields"] {
        h.get_all_by_label(region).next().expect(region);
    }
    // Description and Keywords sit in the Part region.
    type_into(&mut h, "Description", "M3 bolt");
    type_into(&mut h, "Keywords", "screw");
    let p = part(&h, uuid);
    assert_eq!(
        (p.fields.description.as_str(), p.fields.keywords.as_str()),
        ("M3 bolt", "screw")
    );

    assert!(
        checked(&h, "In BOM") && checked(&h, "In CPL"),
        "Standard defaults"
    );
    h.get_by_role_and_label(Role::ComboBox, "Component Type")
        .click();
    h.run();
    click(&mut h, "Mechanical");
    assert!(checked(&h, "In BOM") && !checked(&h, "In CPL"));
    // A flag edited by hand stays until the type changes again.
    h.get_by_role_and_label(Role::CheckBox, "In CPL").click();
    h.run();
    assert!(part(&h, uuid).in_cpl);
    h.get_by_role_and_label(Role::ComboBox, "Component Type")
        .click();
    h.run();
    click(&mut h, "Net Tie");
    let p = part(&h, uuid);
    assert!(!p.in_bom && !p.in_cpl, "Net Tie resets both");
    assert!(!checked(&h, "In BOM") && !checked(&h, "In CPL"));
}

#[test]
fn new_and_duplicate_and_edit_make_the_parts_own_symbol_and_footprints() {
    let home = TempDir::new("part-window-dup");
    let mut shell = shell(home.path());
    let shared = new_element(&mut shell, "symbol", "Shared");
    let uuid = new_element(&mut shell, "part", "Opto");
    let mut h = harness(shell);

    // New makes an empty Symbol named after the Part, and opens it.
    click(&mut h, "New");
    let made = part(&h, uuid).symbol.expect("a new Symbol");
    assert_eq!(name_of(&h, made), "Opto");
    assert_eq!(h.state().shell().layout().active_document(), Some(made));

    // Duplicate & Edit points the Part at a copy, opened, without asking.
    show(&mut h, uuid);
    h.state_mut()
        .shell_mut()
        .run(
            "library.part.symbol.set",
            json!({"part": uuid, "symbol": shared}),
        )
        .unwrap();
    // Duplicate & Edit copies from the Library on disk.
    run(&mut h, "library.element.save", json!({"element": shared}));
    click(&mut h, "Duplicate & Edit Symbol");
    let copy = part(&h, uuid).symbol.unwrap();
    assert_ne!(copy, shared);
    assert_eq!(name_of(&h, copy), "Shared_1");
    assert_eq!(h.state().shell().layout().active_document(), Some(copy));
    assert!(reda_ui::shared_element::asking(h.state().shell()).is_none());

    // Footprints ▸ Add ▸ New, then Duplicate & Edit on its tile.
    show(&mut h, uuid);
    click(&mut h, "New Footprint");
    let footprint = part(&h, uuid).footprints[0];
    assert_eq!(name_of(&h, footprint), "Opto");
    run(
        &mut h,
        "library.element.save",
        json!({"element": footprint}),
    );
    show(&mut h, uuid);
    click(&mut h, "Duplicate & Edit Opto");
    let footprints = part(&h, uuid).footprints;
    assert_eq!(footprints.len(), 1);
    assert_ne!(footprints[0], footprint);
    assert_eq!(name_of(&h, footprints[0]), "Opto_1");
}

#[test]
fn add_generate_adds_the_generated_footprint_to_the_part() {
    let home = TempDir::new("part-window-generate");
    let mut shell = shell(home.path());
    let uuid = new_element(&mut shell, "part", "Res");
    let mut h = harness(shell);
    click(&mut h, "Generate");
    let generator = reda_ui::engine::library::GENERATORS[0];
    let meta = h
        .state()
        .shell()
        .engine()
        .registry()
        .get(generator)
        .unwrap();
    let first = format!("{}…", reda_ui::bars::command_title(generator, meta.title));
    click(&mut h, &first);
    h.get_by_role_and_label(Role::ComboBox, "Size (optional)")
        .click();
    h.run();
    click(&mut h, "0603");
    h.get_all_by_role_and_label(Role::Button, "Generate")
        .last()
        .unwrap()
        .click();
    h.run_steps(10);
    let p = part(&h, uuid);
    assert_eq!(
        p.footprints.len(),
        1,
        "the generated Footprint is the Part's"
    );
}

fn pin(h: &mut Harness<'_, App>, symbol: Uuid, number: &str) {
    let unit = match h.state().shell().engine().element(symbol) {
        Some(LibraryElement::Symbol(s)) => s.units[0].uuid,
        _ => unreachable!(),
    };
    let y = -2_540_000 * number.parse::<i64>().unwrap();
    h.state_mut()
        .shell_mut()
        .run(
            "library.symbol.pin.add",
            json!({"symbol": symbol, "unit": unit, "name": format!("P{number}"),
                   "number": number, "at": {"x": 0, "y": y}}),
        )
        .unwrap();
}

fn pad(h: &mut Harness<'_, App>, footprint: Uuid, number: &str, x: i64) {
    h.state_mut()
        .shell_mut()
        .run(
            "library.footprint.pad.add",
            json!({"footprint": footprint, "number": number, "kind": "smd", "shape": "rect",
                   "at": {"x": x, "y": 0}, "width": "1mm", "height": "1mm"}),
        )
        .unwrap();
}

fn run(h: &mut Harness<'_, App>, id: &str, params: Value) {
    h.state_mut().shell_mut().run(id, params).unwrap();
    h.run();
}

/// A Part with Pins 1-3 and two alternates: Wide with Pads 1-3, Narrow with Pads 1-2.
fn mapped(h: &mut Harness<'_, App>, shell_part: Uuid, symbol: Uuid, wide: Uuid, narrow: Uuid) {
    for n in ["1", "2", "3"] {
        pin(h, symbol, n);
    }
    for (n, x) in [("1", 0), ("2", 2_000_000), ("3", 4_000_000)] {
        pad(h, wide, n, x);
    }
    for (n, x) in [("1", 0), ("2", 2_000_000)] {
        pad(h, narrow, n, x);
    }
    run(
        h,
        "library.part.symbol.set",
        json!({"part": shell_part, "symbol": symbol}),
    );
    run(
        h,
        "library.part.footprints.set",
        json!({"part": shell_part, "footprints": [wide, narrow]}),
    );
    show(h, shell_part);
}

#[test]
fn the_pins_panel_has_a_column_per_alternate_footprint_with_red_marks() {
    let home = TempDir::new("part-window-pins");
    let mut shell = shell(home.path());
    let symbol = new_element(&mut shell, "symbol", "Tri");
    let wide = new_element(&mut shell, "footprint", "Wide");
    let narrow = new_element(&mut shell, "footprint", "Narrow");
    let uuid = new_element(&mut shell, "part", "Tri");
    let mut h = harness(shell);
    mapped(&mut h, uuid, symbol, wide, narrow);

    // The default Footprint's column, then one per alternate.
    h.get_by_role_and_label(Role::TextInput, "Pads of Pin 1");
    h.get_by_role_and_label(Role::TextInput, "Pads of Pin 1 on Narrow");
    // Pin 3 has no Pad 3 on Narrow: its cell is marked.
    h.get_by_role_and_label(Role::TextInput, "Pads of Pin 3 on Narrow (no Pad 3)");
    h.get_by_role_and_label(Role::TextInput, "Pads of Pin 3");

    // Pin 3 onto Pad 2 too: Pad 2 is now on two Pins, in both columns.
    type_into(&mut h, "Pads of Pin 3", "2");
    h.get_by_role_and_label(Role::TextInput, "Pads of Pin 2 (Pad 2 on two Pins)");
    h.get_by_role_and_label(
        Role::TextInput,
        "Pads of Pin 3 on Narrow (Pad 2 on two Pins)",
    );
}

#[test]
fn hovering_a_row_lights_its_pads_and_hovering_a_pad_lights_its_row() {
    let home = TempDir::new("part-window-probe");
    let mut shell = shell(home.path());
    let symbol = new_element(&mut shell, "symbol", "Tri");
    let wide = new_element(&mut shell, "footprint", "Wide");
    let narrow = new_element(&mut shell, "footprint", "Narrow");
    let uuid = new_element(&mut shell, "part", "Tri");
    let mut h = harness(shell);
    mapped(&mut h, uuid, symbol, wide, narrow);
    let probed = |h: &Harness<'_, App>| {
        let shell = h.state().shell();
        let p = match shell.engine().element(uuid) {
            Some(LibraryElement::Part(p)) => p.clone(),
            _ => unreachable!(),
        };
        let s = match shell.engine().element(symbol) {
            Some(LibraryElement::Symbol(s)) => Some(s.clone()),
            _ => None,
        };
        (
            reda_ui::part_editor::probed_pins(shell, &p, s.as_ref()),
            reda_ui::part_editor::probed_pads(shell, &p, s.as_ref()),
        )
    };

    h.get_by_role_and_label(Role::TextInput, "Pads of Pin 2")
        .hover();
    h.run();
    assert_eq!(probed(&h), (vec!["2".to_owned()], vec!["2".to_owned()]));

    // The pointer over Pad 1 of the first preview: the leftmost of three in a row.
    let preview = h
        .get_all_by_role_and_label(Role::Image, "Footprint preview")
        .next()
        .unwrap()
        .rect();
    // Three 1 mm Pads 2 mm apart fill the box's width but for the margin: Pad 1 is at
    // the left, a fifth of the drawing in.
    let drawn = preview.shrink(8.0);
    let scale = (drawn.width() / 5.0).min(drawn.height() / 1.0);
    let at = egui::pos2(preview.center().x - 2.0 * scale, preview.center().y);
    h.event(egui::Event::PointerMoved(at));
    h.run();
    assert_eq!(probed(&h), (vec!["1".to_owned()], vec!["1".to_owned()]));
}
