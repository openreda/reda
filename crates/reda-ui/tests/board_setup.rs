//! The Board Setup dialog's model, without a window: one page per rule, Net Classes and
//! DRC, all drafts of the PCB Document's setup, committed as one `batch` on Apply.

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::geom::Nm;
use reda_ui::engine::model::{DrcCheck, PcbDocument, Severity, Uuid};
use reda_ui::settings::board_setup::{self, ClassKey, DrcPage, NetClassesPage, RuleField};
use reda_ui::settings::{SettingsDialog, Step};
use reda_ui::shell::Shell;
use serde_json::json;

fn shell_with_board(home: &TempDir, work: &TempDir) -> Shell {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home.path())))
        .starter_library(None)
        .headless(false)
        .commands(reda_testkit::probe_commands)
        .build();
    let mut shell = Shell::new(engine);
    shell.new_project(work.path(), "Board").unwrap();
    shell
}

fn dialog(shell: &Shell) -> SettingsDialog {
    board_setup::dialog(shell.engine().project().unwrap())
}

fn pcb(shell: &Shell) -> &PcbDocument {
    shell.engine().project().unwrap().pcb().unwrap()
}

fn add_net(shell: &mut Shell, name: &str) -> Uuid {
    let out = shell.run("test.net.add", json!({"name": name})).unwrap();
    out["result"]["uuid"].as_str().unwrap().parse().unwrap()
}

fn journal_ids(shell: &Shell) -> Vec<String> {
    shell
        .engine()
        .journal()
        .iter()
        .map(|e| e.id.clone())
        .collect()
}

#[test]
fn board_setup_has_a_page_per_rule_then_net_classes_and_drc() {
    let (home, work) = (TempDir::new("bs-home"), TempDir::new("bs-work"));
    let shell = shell_with_board(&home, &work);
    let d = dialog(&shell);
    assert_eq!(d.title(), "Board Setup");
    assert_eq!(
        d.page_titles(),
        [
            "Clearance",
            "Track Width",
            "Via",
            "Annular Ring",
            "Hole Spacing",
            "Edge Clearance",
            "Net Classes",
            "DRC"
        ]
    );
    assert!(!d.has_changes());
}

#[test]
fn rules_overrides_and_severities_apply_as_one_batch_and_one_undo_entry_on_the_pcb() {
    let (home, work) = (TempDir::new("bs-home"), TempDir::new("bs-work"));
    let mut shell = shell_with_board(&home, &work);
    let mut d = dialog(&shell);
    let default = ClassKey::Existing(pcb(&shell).setup().default_net_class);
    {
        let mut board = d.page::<DrcPage>().unwrap().board();
        assert_eq!(board.rule(RuleField::Clearance), "0.16");
        board.set_rule(RuleField::Clearance, "0.2");
        board.set_rule(RuleField::EdgeClearance, "20mil");
        board.set_class_override(default, RuleField::TrackWidth, "0.25");
        board.set_severity(DrcCheck::CourtyardOverlap, Severity::Off);
        board.set_severity(DrcCheck::Clearance, Severity::Warning);
    }
    let pcb_doc = pcb(&shell).uuid;
    let before = journal_ids(&shell).len();
    d.apply(&mut shell).unwrap();
    assert_eq!(journal_ids(&shell)[before..], ["batch"]);
    let setup = pcb(&shell).setup().clone();
    assert_eq!(setup.rules.clearance, Nm::from_um(200));
    assert_eq!(setup.rules.edge_clearance, Nm(508_000));
    assert_eq!(
        setup.severities.get(DrcCheck::CourtyardOverlap),
        Severity::Off
    );
    assert_eq!(setup.severities.get(DrcCheck::Clearance), Severity::Warning);
    let class = pcb(&shell).net_class(setup.default_net_class).unwrap();
    assert_eq!(class.track_width, Some(Nm::from_um(250)));
    assert!(!d.has_changes(), "applied changes are no longer pending");
    // Typing the same value another way is not a change.
    d.page::<DrcPage>()
        .unwrap()
        .board()
        .set_rule(RuleField::Clearance, "0.200mm");
    assert!(!d.has_changes());

    // One undo on the PCB Document puts everything back, and the dialog follows.
    shell
        .run("edit.undo", json!({"document": pcb_doc}))
        .unwrap();
    d.sync(shell.engine().project().unwrap());
    let board = d.page::<DrcPage>().unwrap().board();
    assert_eq!(pcb(&shell).setup().rules.clearance, Nm::from_um(160));
    assert_eq!(board.rule(RuleField::EdgeClearance), "0.4");
    assert_eq!(
        board.severity(DrcCheck::CourtyardOverlap),
        Severity::Warning
    );
}

#[test]
fn undo_with_board_setup_open_undoes_its_changes_on_the_pcb() {
    let (home, work) = (TempDir::new("bs-home"), TempDir::new("bs-work"));
    let mut shell = shell_with_board(&home, &work);
    // Something on the Project stack that the dialog's undo must leave alone.
    shell
        .run("project.properties.set", json!({"title": "Amp"}))
        .unwrap();
    let mut d = dialog(&shell);
    d.page::<DrcPage>()
        .unwrap()
        .board()
        .set_rule(RuleField::Clearance, "0.3");
    d.apply(&mut shell).unwrap();
    assert_eq!(pcb(&shell).setup().rules.clearance, Nm::from_um(300));

    shell.undo(false, Some(d.undo_scope())).unwrap();
    assert_eq!(pcb(&shell).setup().rules.clearance, Nm::from_um(160));
    assert_eq!(
        shell.engine().project().unwrap().props().properties.title,
        "Amp"
    );
}

#[test]
fn each_rule_page_flags_its_own_invalid_values() {
    let (home, work) = (TempDir::new("bs-home"), TempDir::new("bs-work"));
    let shell = shell_with_board(&home, &work);
    let d = dialog(&shell);
    let default = ClassKey::Existing(pcb(&shell).setup().default_net_class);
    {
        let mut board = d.page::<DrcPage>().unwrap().board();
        board.set_rule(RuleField::Clearance, "-0.1");
        board.set_rule(RuleField::ViaHole, "");
        board.set_class_override(default, RuleField::TrackWidth, "wide");
    }
    let flagged: Vec<(String, String)> = d
        .problems()
        .into_iter()
        .map(|(page, p)| (page, p.field))
        .collect();
    assert_eq!(
        flagged,
        [
            ("Clearance".to_owned(), "rule/clearance".to_owned()),
            (
                "Track Width".to_owned(),
                format!("class/{default}/track_width")
            ),
            ("Via".to_owned(), "rule/via_hole".to_owned()),
        ]
    );
    assert!(!d.can_ok() && !d.can_apply());
}

#[test]
fn apply_fab_preset_rules_previews_then_drafts_the_presets_values() {
    let (home, work) = (TempDir::new("bs-home"), TempDir::new("bs-work"));
    let mut shell = shell_with_board(&home, &work);
    shell
        .run(
            "pcb.rules.set",
            json!({"clearance": "0.3mm", "via_hole": "0.4mm"}),
        )
        .unwrap();
    let mut d = dialog(&shell);
    let project = shell.engine().project().unwrap();
    let preset = project.fab_presets().next().unwrap().uuid;
    {
        let mut board = d.page::<DrcPage>().unwrap().board();
        board.preview_fab_preset(preset);
        let preview = board.fab_preview().unwrap();
        assert_eq!(preview.name, "Generic");
        let changes: Vec<(RuleField, &str, Nm)> = preview
            .changes
            .iter()
            .map(|c| (c.rule, c.board.as_str(), c.preset))
            .collect();
        assert_eq!(
            changes,
            [
                (RuleField::Clearance, "0.3", Nm::from_um(160)),
                (RuleField::ViaHole, "0.4", Nm::from_um(300)),
            ]
        );
    }
    assert!(!d.has_changes(), "a preview changes nothing");
    d.page::<DrcPage>().unwrap().board().accept_fab_preview();
    assert_eq!(
        d.steps(),
        [Step::new(
            "pcb.rules.set",
            json!({"clearance": 160_000, "via_hole": 300_000})
        )]
    );
    d.apply(&mut shell).unwrap();
    assert_eq!(pcb(&shell).setup().rules.via_hole, Nm::from_um(300));
}

#[test]
fn net_classes_are_added_renamed_and_deleted_and_nets_assigned_in_one_batch() {
    let (home, work) = (TempDir::new("bs-home"), TempDir::new("bs-work"));
    let mut shell = shell_with_board(&home, &work);
    let (vcc, gnd, sig) = (
        add_net(&mut shell, "VCC"),
        add_net(&mut shell, "GND"),
        add_net(&mut shell, "SIG"),
    );
    let old = shell
        .run("pcb.net_class.add", json!({"name": "Old", "nets": [sig]}))
        .unwrap();
    let old: Uuid = old["result"]["uuid"].as_str().unwrap().parse().unwrap();
    let mut d = dialog(&shell);
    let default = pcb(&shell).setup().default_net_class;
    {
        let mut board = d.page::<NetClassesPage>().unwrap().board();
        let names: Vec<&str> = board.live_classes().map(|c| c.name()).collect();
        assert_eq!(names, ["Default", "Old"]);
        board.net_filter = "c".into();
        let shown: Vec<&str> = board.filtered_nets().map(|n| n.name.as_str()).collect();
        assert_eq!(shown, ["VCC"], "the filter ignores case");

        let power = board.add_class();
        assert_eq!(board.class_name_mut(power).unwrap(), "Class 1");
        board.rename_class(power, "Power");
        board.set_class_override(power, RuleField::Clearance, "0.3");
        board.set_net_class(vcc, power);
        board.set_net_class(gnd, ClassKey::Existing(old));
        board.rename_class(ClassKey::Existing(default), "Signal");
        board.delete_class(ClassKey::Existing(old));
        let gnd_row = board.nets().iter().find(|n| n.uuid == gnd).unwrap();
        assert_eq!(gnd_row.class(), ClassKey::Existing(default));
    }
    let ids: Vec<String> = d.steps().into_iter().map(|s| s.id).collect();
    assert_eq!(
        ids,
        [
            "pcb.net_class.delete",
            "pcb.net_class.rename",
            "pcb.net_class.add"
        ]
    );
    let before = journal_ids(&shell).len();
    d.apply(&mut shell).unwrap();
    assert_eq!(journal_ids(&shell)[before..], ["batch"]);
    let board = pcb(&shell);
    assert!(board.net_class(old).is_none());
    assert_eq!(board.net_class(default).unwrap().name, "Signal");
    let power = board.net_class_named("Power").unwrap();
    assert_eq!(power.clearance, Some(Nm::from_um(300)));
    assert_eq!(board.net(vcc).unwrap().class, power.uuid);
    assert_eq!(board.net(gnd).unwrap().class, default);
    assert_eq!(board.net(sig).unwrap().class, default);
}

#[test]
fn two_net_classes_can_swap_names_in_one_apply() {
    let (home, work) = (TempDir::new("bs-home"), TempDir::new("bs-work"));
    let mut shell = shell_with_board(&home, &work);
    let add = |shell: &mut Shell, name: &str| -> Uuid {
        let out = shell
            .run("pcb.net_class.add", json!({"name": name}))
            .unwrap();
        out["result"]["uuid"].as_str().unwrap().parse().unwrap()
    };
    let (power, signal) = (add(&mut shell, "Power"), add(&mut shell, "Signal"));
    let mut d = dialog(&shell);
    {
        let mut board = d.page::<NetClassesPage>().unwrap().board();
        board.rename_class(ClassKey::Existing(power), "Signal");
        board.rename_class(ClassKey::Existing(signal), "Power");
    }
    assert!(d.can_apply());
    d.apply(&mut shell).unwrap();
    assert_eq!(pcb(&shell).net_class(power).unwrap().name, "Signal");
    assert_eq!(pcb(&shell).net_class(signal).unwrap().name, "Power");
}

#[test]
fn a_net_moved_between_existing_classes_is_one_set_class_step() {
    let (home, work) = (TempDir::new("bs-home"), TempDir::new("bs-work"));
    let mut shell = shell_with_board(&home, &work);
    let vcc = add_net(&mut shell, "VCC");
    let power = shell
        .run("pcb.net_class.add", json!({"name": "Power"}))
        .unwrap();
    let power: Uuid = power["result"]["uuid"].as_str().unwrap().parse().unwrap();
    let mut d = dialog(&shell);
    d.page::<NetClassesPage>()
        .unwrap()
        .board()
        .set_net_class(vcc, ClassKey::Existing(power));
    assert_eq!(
        d.steps(),
        [Step::new(
            "pcb.net.set_class",
            json!({"nets": [vcc], "class": power})
        )]
    );
    d.apply(&mut shell).unwrap();
    assert_eq!(pcb(&shell).net(vcc).unwrap().class, power);
}

#[test]
fn a_blank_or_duplicate_class_name_is_flagged_on_the_net_classes_page() {
    let (home, work) = (TempDir::new("bs-home"), TempDir::new("bs-work"));
    let shell = shell_with_board(&home, &work);
    let d = dialog(&shell);
    let (a, b) = {
        let mut board = d.page::<NetClassesPage>().unwrap().board();
        let (a, b) = (board.add_class(), board.add_class());
        board.rename_class(a, "default");
        board.rename_class(b, " ");
        (a, b)
    };
    let flagged: Vec<(String, String)> = d
        .problems()
        .into_iter()
        .map(|(page, p)| (page, p.field))
        .collect();
    assert_eq!(
        flagged,
        [
            ("Net Classes".to_owned(), format!("class/{a}/name")),
            ("Net Classes".to_owned(), format!("class/{b}/name")),
        ]
    );
    let mut board = d.page::<NetClassesPage>().unwrap().board();
    board.delete_class(b);
    board.rename_class(a, "Power");
    drop(board);
    assert!(d.can_apply());
}
