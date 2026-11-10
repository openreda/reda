//! The Board Setup dialog in the window, driven through AccessKit with `egui_kittest`:
//! opened from the PCB Document's menu in the Projects panel, its rule pages, Net Classes
//! and DRC pages, and OK committing one `batch`.

use std::path::Path;

use egui::Key;
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::geom::Nm;
use reda_ui::engine::model::{DrcCheck, PcbDocument, RuleKind, Severity};
use reda_ui::shell::Shell;

fn harness(home: &Path, work: &Path, name: &str) -> Harness<'static, App> {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .commands(reda_testkit::probe_commands)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work, name).unwrap();
    let mut h = Harness::builder()
        .with_size([1100.0, 760.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    h.run();
    h
}

/// Opens Board Setup from the PCB Document's menu: its row is the tree item named after
/// the Project.
fn open_board_setup(h: &mut Harness<'static, App>, project: &str) {
    h.get_by_role_and_label(Role::TreeItem, project)
        .click_secondary();
    h.run();
    h.get_by_label("Board Setup…").click();
    h.run();
}

fn pcb(h: &Harness<'static, App>) -> PcbDocument {
    h.state()
        .shell()
        .engine()
        .project()
        .unwrap()
        .pcb()
        .unwrap()
        .clone()
}

fn disabled(h: &Harness<'static, App>, label: &str) -> bool {
    h.get_by_role_and_label(Role::Button, label)
        .accesskit_node()
        .is_disabled()
}

fn page(h: &mut Harness<'static, App>, title: &str) {
    h.get_by_role_and_label(Role::TreeItem, title).click();
    h.run();
}

#[test]
fn board_setup_opens_from_the_pcb_menu_with_a_page_per_rule() {
    let (home, work) = (TempDir::new("bsd-home"), TempDir::new("bsd-work"));
    let mut h = harness(home.path(), work.path(), "Dlg");
    open_board_setup(&mut h, "Dlg");
    h.get_by_label("Board Setup");
    for title in [
        "Clearance",
        "Track Width",
        "Vias",
        "Holes",
        "Board Edge",
        "Silkscreen",
        "Net Classes",
        "DRC",
    ] {
        h.get_by_role_and_label(Role::TreeItem, title);
    }
    // The Clearance page: the board-wide value and the check's severity. The Default Net
    // Class's values are the board-wide ones, so it has no override row.
    h.get_by_role_and_label(Role::TextInput, "Board-wide minimum Clearance (mm)");
    assert!(h.query_by_label("Default: Clearance").is_none());
    h.get_by_label("Clearance: Error");
    h.get_by_label("Apply Generic Rules…");
    assert!(disabled(&h, "Apply"));
    h.get_by_label("Cancel").click();
    h.run();
    assert!(h.query_by_label("Board Setup").is_none());
}

#[test]
fn an_invalid_rule_disables_ok_until_fixed_and_ok_commits_one_batch() {
    let (home, work) = (TempDir::new("bsd-home"), TempDir::new("bsd-work"));
    let mut h = harness(home.path(), work.path(), "Dlg");
    open_board_setup(&mut h, "Dlg");
    let field = "Board-wide minimum Clearance (mm)";
    h.get_by_role_and_label(Role::TextInput, field).click();
    h.run();
    h.key_press(Key::End);
    h.run();
    h.get_by_role_and_label(Role::TextInput, field)
        .type_text("x");
    h.run();
    h.get_by_role_and_label(Role::TreeItem, "Clearance (has invalid values)");
    assert!(disabled(&h, "OK") && disabled(&h, "Apply"));
    h.key_press(Key::Backspace);
    h.run();
    h.get_by_role_and_label(Role::TextInput, field)
        .type_text("5");
    h.run();

    page(&mut h, "DRC");
    h.get_by_label("Courtyard Overlap: Off").click();
    h.run();
    page(&mut h, "Net Classes");
    h.get_by_label("Add Net Class").click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Net Class 1 name");

    let journal = h.state().shell().engine().journal().len();
    h.get_by_label("OK").click();
    h.run();
    assert!(h.query_by_label("Board Setup").is_none(), "OK closes");
    let entries = h.state().shell().engine().journal();
    assert_eq!(entries.len(), journal + 1);
    assert_eq!(entries.last().unwrap().id, "batch");
    let board = pcb(&h);
    assert_eq!(
        board.board_rule(RuleKind::Clearance).values.value,
        Some(Nm::from_um(165))
    );
    assert_eq!(
        board.setup().severities.get(DrcCheck::CourtyardOverlap),
        Severity::Off
    );
    assert!(board.net_class_named("Class 1").is_some());
}

/// How far `label`'s text starts from the left edge of its page tree row, in points.
fn tree_label_indent(h: &Harness<'static, App>, label: &str) -> f32 {
    let row = h.get_by_role_and_label(Role::TreeItem, label).rect();
    let mut found = None;
    for clipped in &h.output().shapes {
        let mut shapes = vec![&clipped.shape];
        while let Some(shape) = shapes.pop() {
            match shape {
                egui::Shape::Vec(inner) => shapes.extend(inner),
                egui::Shape::Text(text)
                    if text.galley.text() == label && row.contains(text.pos) =>
                {
                    found = Some(text.pos.x - row.left());
                }
                _ => {}
            }
        }
    }
    found.unwrap_or_else(|| panic!("no text {label:?} in its row {row:?}"))
}
#[test]
fn page_tree_labels_are_left_aligned() {
    let (home, work) = (TempDir::new("bsd-home"), TempDir::new("bsd-work"));
    let mut h = harness(home.path(), work.path(), "Dlg");
    open_board_setup(&mut h, "Dlg");
    for title in ["Clearance", "Vias", "Board Edge", "DRC"] {
        let indent = tree_label_indent(&h, title);
        assert!(indent < 10.0, "{title:?} starts {indent} pt into its row");
    }
}

fn run(h: &mut Harness<'static, App>, id: &str, params: serde_json::Value) -> serde_json::Value {
    h.state_mut().shell_mut().run(id, params).unwrap()
}

fn uuid_of(out: &serde_json::Value) -> reda_ui::engine::model::Uuid {
    out["result"]["uuid"].as_str().unwrap().parse().unwrap()
}

/// The colour `text` is painted in, in the first text shape inside `within` showing it.
fn text_colour(h: &Harness<'static, App>, text: &str, within: egui::Rect) -> egui::Color32 {
    for clipped in &h.output().shapes {
        let mut shapes = vec![&clipped.shape];
        while let Some(shape) = shapes.pop() {
            match shape {
                egui::Shape::Vec(inner) => shapes.extend(inner),
                egui::Shape::Text(t)
                    if t.galley.text() == text && within.contains(t.pos + egui::vec2(1.0, 1.0)) =>
                {
                    let colour = t
                        .galley
                        .rows
                        .iter()
                        .flat_map(|r| r.glyphs.iter())
                        .next()
                        .and_then(|_| t.galley.job.sections.first())
                        .map(|s| s.format.color)
                        .unwrap_or(t.fallback_color);
                    return if colour == egui::Color32::PLACEHOLDER {
                        t.fallback_color
                    } else {
                        colour
                    };
                }
                _ => {}
            }
        }
    }
    panic!("no text {text:?} inside {within:?}");
}

fn value(h: &Harness<'static, App>, label: &str) -> String {
    h.get_by_role_and_label(Role::TextInput, label)
        .value()
        .unwrap_or_default()
}

fn type_into(h: &mut Harness<'static, App>, label: &str, text: &str) {
    h.get_by_role_and_label(Role::TextInput, label).click();
    h.run();
    h.key_press(Key::End);
    h.run();
    h.get_by_role_and_label(Role::TextInput, label)
        .type_text(text);
    h.run();
}

#[test]
fn rule_pages_hold_each_check_with_values_preset_limits_overrides_and_severity() {
    let (home, work) = (TempDir::new("bsd-home"), TempDir::new("bsd-work"));
    let mut h = harness(home.path(), work.path(), "Dlg");
    run(
        &mut h,
        "pcb.net_class.add",
        serde_json::json!({"name": "Power"}),
    );
    open_board_setup(&mut h, "Dlg");
    // Clearance: the Generic job's limit sits beside the value.
    h.get_by_label("Generic limit: at least 0.16");
    page(&mut h, "Track Width");
    h.get_by_role_and_label(Role::TextInput, "Board-wide maximum Track width (mm)");
    h.get_by_role_and_label(Role::TextInput, "Power: Maximum Track width");
    h.get_by_label("Generic limit: at least 0.2");
    h.get_by_label("Track Width: Error");
    page(&mut h, "Holes");
    for check in ["Hole Size", "Annular Ring", "Hole to Hole"] {
        h.get_by_label(&format!("{check}: Error"));
    }
    h.get_by_label("Generic limit: at most 6.3");
    h.get_by_role_and_label(Role::TextInput, "Power: Annular ring");
    h.get_by_role_and_label(Role::TextInput, "Power: Hole-to-hole spacing");
    page(&mut h, "Board Edge");
    h.get_by_label("Edge Clearance: Error");
    h.get_by_label("Silkscreen to Edge: Warning");
    h.get_by_role_and_label(Role::TextInput, "Power: Silkscreen to edge");
    page(&mut h, "Silkscreen");
    h.get_by_label("Silkscreen to Mask: Warning").click();
    h.run();
    h.get_by_role_and_label(Role::ComboBox, "Measure silkscreen to");
    type_into(&mut h, "Power: Silkscreen to mask", "0.2");
    h.get_by_label("Silkscreen to Mask: Off").click();
    h.run();
    h.get_by_label("OK").click();
    h.run();
    let board = pcb(&h);
    let power = board.net_class_named("Power").unwrap().uuid;
    assert_eq!(
        board
            .class_rule(RuleKind::SilkToMask, power)
            .unwrap()
            .values
            .value,
        Some(Nm::from_um(200))
    );
    assert_eq!(
        board.setup().severities.get(DrcCheck::SilkToMask),
        Severity::Off
    );
}

#[test]
fn the_drc_page_holds_every_severity_and_the_values_beside_their_checks() {
    let (home, work) = (TempDir::new("bsd-home"), TempDir::new("bsd-work"));
    let mut h = harness(home.path(), work.path(), "Dlg");
    open_board_setup(&mut h, "Dlg");
    page(&mut h, "DRC");
    for choice in [
        "Clearance: Error",
        "Via Size: Error",
        "Silkscreen to Edge: Warning",
        "Invalid Rule: Error",
        "Rule Outside Fab Preset: Warning",
        "Dead Copper: Warning",
    ] {
        h.get_by_label(choice);
    }
    assert_eq!(value(&h, "Courtyard clearance (mm)"), "0");
    assert_eq!(value(&h, "Dangling tolerance (mm)"), "0");
    assert_eq!(value(&h, "Micro-segment maximum length (mm)"), "0.01");
    // Each value sits on its check's row.
    let row_of = |h: &Harness<'static, App>, label: &str| {
        h.get_by_role_and_label(Role::TextInput, label)
            .rect()
            .center()
            .y
    };
    let check_y = |h: &Harness<'static, App>, check: &str| {
        h.get_by_label(&format!("{check}: Warning"))
            .rect()
            .center()
            .y
    };
    for (check, label) in [
        ("Courtyard Overlap", "Courtyard clearance (mm)"),
        ("Dangling Copper", "Dangling tolerance (mm)"),
        ("Micro-Segment", "Micro-segment maximum length (mm)"),
    ] {
        assert!(
            (row_of(&h, label) - check_y(&h, check)).abs() < 12.0,
            "{label}"
        );
    }
    type_into(&mut h, "Courtyard clearance (mm)", "5");
    h.get_by_label("OK").click();
    h.run();
    assert_eq!(
        pcb(&h)
            .board_rule(RuleKind::CourtyardClearance)
            .values
            .value,
        Some(Nm::from_um(5000)),
        "typed after the 0: 05 mm"
    );
}

#[test]
fn the_net_classes_grid_has_a_bold_default_row_and_grey_inherited_values() {
    let (home, work) = (TempDir::new("bsd-home"), TempDir::new("bsd-work"));
    let mut h = harness(home.path(), work.path(), "Dlg");
    run(
        &mut h,
        "pcb.net_class.add",
        serde_json::json!({"name": "Power", "rules": [{"kind": "clearance", "values": {"value": "0.3mm"}}]}),
    );
    open_board_setup(&mut h, "Dlg");
    page(&mut h, "Net Classes");
    let visuals = h.ctx.global_style().visuals.clone();
    let (strong, weak) = (visuals.strong_text_color(), visuals.weak_text_color());
    let default_row = h.get_by_label("Select Default").rect();
    let row = egui::Rect::from_x_y_ranges(default_row.left()..=2000.0, default_row.y_range());
    assert_eq!(
        text_colour(&h, "Default", row.expand2(egui::vec2(0.0, 4.0))),
        strong
    );
    assert_eq!(value(&h, "Default: Clearance"), "0.16");
    // Power overrides clearance (normal) and inherits Track width (grey).
    let track = h
        .get_by_role_and_label(Role::TextInput, "Power: Track width")
        .rect();
    assert_eq!(value(&h, "Power: Track width"), "");
    assert_eq!(text_colour(&h, "0.2", track), weak);
    let clearance = h
        .get_by_role_and_label(Role::TextInput, "Power: Clearance")
        .rect();
    assert_ne!(text_colour(&h, "0.3", clearance), weak);
    // The Clearance record has `enabled` and a comment; Track width has none yet.
    h.get_by_role_and_label(Role::CheckBox, "Power: Clearance enabled")
        .click();
    h.run();
    type_into(&mut h, "Power: Clearance comment", "HV");
    assert!(h.query_by_label("Power: Track width enabled").is_none());
    // Typing a preferred Track width fills the new record's min and max.
    type_into(&mut h, "Power: Preferred Track width", "0.4");
    assert_eq!(value(&h, "Power: Track width"), "0.4");
    assert_eq!(value(&h, "Power: Maximum Track width"), "0.4");
    h.get_by_label("OK").click();
    h.run();
    let board = pcb(&h);
    let power = board.net_class_named("Power").unwrap().uuid;
    let clearance = board.class_rule(RuleKind::Clearance, power).unwrap();
    assert!(!clearance.enabled);
    assert_eq!(clearance.comment.as_deref(), Some("HV"));
    let track = board
        .class_rule(RuleKind::TrackWidth, power)
        .unwrap()
        .values;
    assert_eq!(
        (track.min, track.preferred, track.max),
        (
            Some(Nm::from_um(400)),
            Some(Nm::from_um(400)),
            Some(Nm::from_um(400))
        )
    );
}

#[test]
fn grid_rows_are_dragged_to_rank_and_filtered() {
    let (home, work) = (TempDir::new("bsd-home"), TempDir::new("bsd-work"));
    let mut h = harness(home.path(), work.path(), "Dlg");
    run(
        &mut h,
        "pcb.net_class.add",
        serde_json::json!({"name": "Power"}),
    );
    run(
        &mut h,
        "pcb.net_class.add",
        serde_json::json!({"name": "Signal"}),
    );
    open_board_setup(&mut h, "Dlg");
    page(&mut h, "Net Classes");
    // Filters: by name, then only classes with overrides.
    type_into(&mut h, "Search Net Classes", "sig");
    assert!(h.query_by_label("Select or drag Power").is_none());
    h.get_by_label("Select or drag Signal");
    for _ in 0..3 {
        h.key_press(Key::Backspace);
    }
    h.run();
    h.get_by_label("Show only overrides").click();
    h.run();
    assert!(h.query_by_label("Select or drag Power").is_none());
    h.get_by_label("Select Default");
    h.get_by_label("Show only overrides").click();
    h.run();

    // Drag Signal above Power by its grip.
    let from = h.get_by_label("Select or drag Signal").rect().center();
    let to = h.get_by_label("Select or drag Power").rect().center_top() - egui::vec2(0.0, 3.0);
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    h.input_mut().events.push(egui::Event::PointerMoved(from));
    h.run();
    h.input_mut().events.push(button(from, true));
    h.run();
    for t in [0.3, 0.6, 1.0] {
        h.input_mut()
            .events
            .push(egui::Event::PointerMoved(from.lerp(to, t)));
        h.run();
    }
    h.input_mut().events.push(button(to, false));
    h.run();
    let power = h.get_by_label("Select or drag Power").rect().top();
    let signal = h.get_by_label("Select or drag Signal").rect().top();
    assert!(signal < power, "Signal now ranks above Power");
    h.get_by_label("OK").click();
    h.run();
    let board = pcb(&h);
    assert_eq!(board.net_class_named("Signal").unwrap().rank, 1);
    assert_eq!(board.net_class_named("Power").unwrap().rank, 2);
}

#[test]
fn grid_rows_copy_paste_and_clear_from_the_keyboard() {
    let (home, work) = (TempDir::new("bsd-home"), TempDir::new("bsd-work"));
    let mut h = harness(home.path(), work.path(), "Dlg");
    run(
        &mut h,
        "pcb.net_class.add",
        serde_json::json!({"name": "Power", "rules": [{"kind": "clearance", "values": {"value": "0.3mm"}}]}),
    );
    run(
        &mut h,
        "pcb.net_class.add",
        serde_json::json!({"name": "Signal"}),
    );
    open_board_setup(&mut h, "Dlg");
    page(&mut h, "Net Classes");
    h.get_by_label("Select or drag Power").click();
    h.run();
    h.input_mut().events.push(egui::Event::Copy);
    h.step();
    let copied: Vec<String> = h
        .output()
        .platform_output
        .commands
        .iter()
        .filter_map(|c| match c {
            egui::OutputCommand::CopyText(t) => Some(t.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(copied, ["0.3\t\t\t\t\t\t\t\t\t"]);
    h.run();
    h.get_by_label("Select or drag Signal").click();
    h.run();
    h.input_mut()
        .events
        .push(egui::Event::Paste(copied[0].clone()));
    h.run();
    assert_eq!(value(&h, "Signal: Clearance"), "0.3");
    // Ctrl+click adds Power to the selection; Delete clears both.
    h.get_by_label("Select or drag Power")
        .click_modifiers(egui::Modifiers::COMMAND);
    h.run();
    h.key_press(Key::Delete);
    h.run();
    assert_eq!(value(&h, "Signal: Clearance"), "");
    assert_eq!(value(&h, "Power: Clearance"), "");
}

#[test]
fn the_net_table_makes_a_class_from_selected_nets_and_assigns_by_pattern() {
    let (home, work) = (TempDir::new("bsd-home"), TempDir::new("bsd-work"));
    let mut h = harness(home.path(), work.path(), "Dlg");
    let usb_p = uuid_of(&run(
        &mut h,
        "test.net.add",
        serde_json::json!({"name": "USB_P"}),
    ));
    let usb_n = uuid_of(&run(
        &mut h,
        "test.net.add",
        serde_json::json!({"name": "USB_N"}),
    ));
    let vcc = uuid_of(&run(
        &mut h,
        "test.net.add",
        serde_json::json!({"name": "VCC"}),
    ));
    let gnd = uuid_of(&run(
        &mut h,
        "test.net.add",
        serde_json::json!({"name": "GND"}),
    ));
    run(
        &mut h,
        "pcb.net_class.add",
        serde_json::json!({"name": "Power"}),
    );
    open_board_setup(&mut h, "Dlg");
    page(&mut h, "Net Classes");
    assert!(disabled(&h, "New Class"), "nothing selected");
    h.get_by_label("USB_N").click();
    h.run();
    h.get_by_label("USB_P")
        .click_modifiers(egui::Modifiers::COMMAND);
    h.run();
    h.get_by_label("New Class").click();
    h.run();
    let name = "Net Class 2 name";
    h.get_by_role_and_label(Role::TextInput, name).click();
    h.run();
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
    h.run();
    h.get_by_role_and_label(Role::TextInput, name)
        .type_text("USB");
    h.run();
    h.get_by_label("Assign by Pattern…").click();
    h.run();
    type_into(&mut h, "Pattern", "?cc");
    h.get_by_role_and_label(Role::ComboBox, "Net Class").click();
    h.run();
    h.get_all_by_label("Power").last().unwrap().click();
    h.run();
    h.get_by_label("Assign").click();
    h.run();
    h.get_by_label("OK").click();
    h.run();
    let board = pcb(&h);
    let usb = board.net_class_named("USB").unwrap().uuid;
    let power = board.net_class_named("Power").unwrap().uuid;
    assert_eq!(board.net(usb_p).unwrap().classes, [usb]);
    assert_eq!(board.net(usb_n).unwrap().classes, [usb]);
    assert_eq!(board.net(vcc).unwrap().classes, [power]);
    assert!(board.net(gnd).unwrap().classes.is_empty());
}
