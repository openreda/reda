//! The Board Setup dialog's model, without a window: one page per rule, Net Classes and
//! DRC, all drafts of the PCB Document's setup, committed as one `batch` on Apply.

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::geom::Nm;
use reda_ui::engine::model::{
    ConnectionTarget, DrcCheck, Limit, PcbDocument, RuleKind, Severity, SilkMaskMode, Uuid,
};
use reda_ui::settings::board_setup::{self, ClassKey, DrcPage, NetClassesPage, Rule, RuleField};
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

fn board_value(shell: &Shell, rule: RuleField) -> Option<Nm> {
    rule.get(pcb(shell))
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
            "Vias",
            "Holes",
            "Board Edge",
            "Mask & Paste",
            "Silkscreen",
            "Zone Connections",
            "Stackup",
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
    let power = shell
        .run("pcb.net_class.add", json!({"name": "Power"}))
        .unwrap();
    let power: Uuid = power["result"]["uuid"].as_str().unwrap().parse().unwrap();
    let mut d = dialog(&shell);
    {
        let mut board = d.page::<DrcPage>().unwrap().board();
        assert_eq!(board.rule(RuleField::Clearance), "0.16");
        assert_eq!(board.rule(RuleField::TrackPreferred), "0.25");
        board.set_rule(RuleField::Clearance, "0.2");
        board.set_rule(RuleField::EdgeClearance, "20mil");
        board.set_class_override(ClassKey::Existing(power), RuleField::TrackPreferred, "0.5");
        board.set_severity(DrcCheck::CourtyardOverlap, Severity::Off);
        board.set_severity(DrcCheck::Clearance, Severity::Warning);
    }
    let pcb_doc = pcb(&shell).uuid;
    let before = journal_ids(&shell).len();
    d.apply(&mut shell).unwrap();
    assert_eq!(journal_ids(&shell)[before..], ["batch"]);
    let setup = pcb(&shell).setup().clone();
    assert_eq!(
        board_value(&shell, RuleField::Clearance),
        Some(Nm::from_um(200))
    );
    assert_eq!(
        board_value(&shell, RuleField::EdgeClearance),
        Some(Nm(508_000))
    );
    assert_eq!(
        setup.severities.get(DrcCheck::CourtyardOverlap),
        Severity::Off
    );
    assert_eq!(setup.severities.get(DrcCheck::Clearance), Severity::Warning);
    let record = pcb(&shell).class_rule(RuleKind::TrackWidth, power).unwrap();
    assert_eq!(record.values.preferred, Some(Nm::from_um(500)));
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
    assert_eq!(
        board_value(&shell, RuleField::Clearance),
        Some(Nm::from_um(160))
    );
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
    assert_eq!(
        board_value(&shell, RuleField::Clearance),
        Some(Nm::from_um(300))
    );

    shell.undo(false, Some(d.undo_scope())).unwrap();
    assert_eq!(
        board_value(&shell, RuleField::Clearance),
        Some(Nm::from_um(160))
    );
    assert_eq!(
        shell.engine().project().unwrap().props().properties.title,
        "Amp"
    );
}

#[test]
fn each_rule_page_flags_its_own_invalid_values() {
    let (home, work) = (TempDir::new("bs-home"), TempDir::new("bs-work"));
    let mut shell = shell_with_board(&home, &work);
    let power = shell
        .run("pcb.net_class.add", json!({"name": "Power"}))
        .unwrap();
    let power: Uuid = power["result"]["uuid"].as_str().unwrap().parse().unwrap();
    let power = ClassKey::Existing(power);
    let d = dialog(&shell);
    {
        let mut board = d.page::<DrcPage>().unwrap().board();
        board.set_rule(RuleField::Clearance, "-0.1");
        board.set_rule(RuleField::ViaHole, "");
        board.set_class_override(power, RuleField::TrackWidth, "wide");
    }
    let flagged: Vec<(String, String)> = d
        .problems()
        .into_iter()
        .map(|(page, p)| (page, p.field))
        .collect();
    assert_eq!(
        flagged,
        [
            ("Clearance".to_owned(), "rule/clearance.value".to_owned()),
            (
                "Track Width".to_owned(),
                format!("class/{power}/track_width.min")
            ),
            ("Vias".to_owned(), "rule/via_size.hole.min".to_owned()),
            // The grid shows clearance, Track width and Via size too.
            ("Net Classes".to_owned(), "rule/clearance.value".to_owned()),
            (
                "Net Classes".to_owned(),
                "rule/via_size.hole.min".to_owned()
            ),
            (
                "Net Classes".to_owned(),
                format!("class/{power}/track_width.min")
            ),
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
            json!({"kind": "clearance", "values": {"value": "0.3mm"}}),
        )
        .unwrap();
    shell
        .run(
            "pcb.rules.set",
            json!({"kind": "via_size", "values": {"hole": {"min": "0.4mm", "preferred": "0.4mm"}}}),
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
        let changes: Vec<(RuleField, &str, &str, bool)> = preview
            .changes
            .iter()
            .map(|c| (c.rule, c.board.as_str(), c.preset.as_str(), c.ticked))
            .collect();
        // The preferred hole stays inside the new range, so it starts unticked.
        assert_eq!(
            changes,
            [
                (RuleField::Clearance, "0.3", "0.16", true),
                (RuleField::ViaHole, "0.4", "0.3", true),
                (RuleField::ViaHolePreferred, "0.4", "0.3", false),
            ]
        );
    }
    assert!(!d.has_changes(), "a preview changes nothing");
    d.page::<DrcPage>().unwrap().board().accept_fab_preview();
    assert_eq!(
        d.steps(),
        [
            Step::new(
                "pcb.rules.set",
                json!({"kind": "clearance", "values": {"value": 160_000}})
            ),
            Step::new(
                "pcb.rules.set",
                json!({"kind": "via_size", "values": {"hole": {"min": 300_000}}})
            ),
        ]
    );
    d.apply(&mut shell).unwrap();
    assert_eq!(
        board_value(&shell, RuleField::ViaHole),
        Some(Nm::from_um(300))
    );
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
    {
        let mut board = d.page::<NetClassesPage>().unwrap().board();
        let names: Vec<&str> = board.live_classes().map(|c| c.name()).collect();
        assert_eq!(
            names,
            ["Old"],
            "the Default Net Class is not a stored class"
        );
        board.net_filter = "c".into();
        let shown: Vec<&str> = board.filtered_nets().map(|n| n.name.as_str()).collect();
        assert_eq!(shown, ["VCC"], "the filter ignores case");

        let power = board.add_class();
        assert_eq!(board.class_name_mut(power).unwrap(), "Class 1");
        board.rename_class(power, "Power");
        board.set_class_override(power, RuleField::Clearance, "0.3");
        board.set_net_class(vcc, power);
        board.set_net_class(gnd, ClassKey::Existing(old));
        board.delete_class(ClassKey::Existing(old));
        let gnd_row = board.nets().iter().find(|n| n.uuid == gnd).unwrap();
        assert_eq!(gnd_row.class(), ClassKey::Default);
    }
    let ids: Vec<String> = d.steps().into_iter().map(|s| s.id).collect();
    assert_eq!(ids, ["pcb.net_class.delete", "pcb.net_class.add"]);
    let before = journal_ids(&shell).len();
    d.apply(&mut shell).unwrap();
    assert_eq!(journal_ids(&shell)[before..], ["batch"]);
    let board = pcb(&shell);
    assert!(board.net_class(old).is_none());
    let power = board.net_class_named("Power").unwrap();
    let clearance = board.class_rule(RuleKind::Clearance, power.uuid).unwrap();
    assert_eq!(clearance.values.value, Some(Nm::from_um(300)));
    assert_eq!(board.net(vcc).unwrap().classes, [power.uuid]);
    assert!(board.net(gnd).unwrap().classes.is_empty());
    assert!(board.net(sig).unwrap().classes.is_empty());
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
fn a_net_moved_into_an_existing_class_is_one_add_to_class_step() {
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
            "pcb.net.add_to_class",
            json!({"nets": [vcc], "class": power})
        )]
    );
    d.apply(&mut shell).unwrap();
    assert_eq!(pcb(&shell).net(vcc).unwrap().classes, [power]);
    // Back to the Default Net Class alone: one step out of Power.
    let d = dialog(&shell);
    d.page::<NetClassesPage>()
        .unwrap()
        .board()
        .set_net_class(vcc, ClassKey::Default);
    assert_eq!(
        d.steps(),
        [Step::new(
            "pcb.net.remove_from_class",
            json!({"nets": [vcc], "class": power})
        )]
    );
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

fn add_class(shell: &mut Shell, params: serde_json::Value) -> Uuid {
    let out = shell.run("pcb.net_class.add", params).unwrap();
    out["result"]["uuid"].as_str().unwrap().parse().unwrap()
}

#[test]
fn each_rule_page_holds_its_checks_with_their_board_wide_values() {
    use RuleField as F;
    type Sections = Vec<(Option<DrcCheck>, Vec<RuleField>)>;
    let pages: Vec<(Rule, Sections)> = Rule::ALL
        .iter()
        .map(|r| {
            let sections = r
                .sections()
                .iter()
                .map(|s| (s.check, s.fields.to_vec()))
                .collect();
            (*r, sections)
        })
        .collect();
    assert_eq!(
        pages,
        [
            (
                Rule::Clearance,
                vec![(Some(DrcCheck::Clearance), vec![F::Clearance])]
            ),
            (
                Rule::TrackWidth,
                vec![(
                    Some(DrcCheck::TrackWidth),
                    vec![F::TrackWidth, F::TrackPreferred, F::TrackMax]
                )]
            ),
            (
                Rule::Vias,
                vec![(
                    Some(DrcCheck::ViaSize),
                    vec![
                        F::ViaDiameter,
                        F::ViaDiameterPreferred,
                        F::ViaDiameterMax,
                        F::ViaHole,
                        F::ViaHolePreferred,
                        F::ViaHoleMax
                    ]
                )]
            ),
            (
                Rule::Holes,
                vec![
                    (Some(DrcCheck::HoleSize), vec![F::HoleMin, F::HoleMax]),
                    (Some(DrcCheck::AnnularRing), vec![F::AnnularRing]),
                    (Some(DrcCheck::HoleToHole), vec![F::HoleToHole]),
                ]
            ),
            (
                Rule::BoardEdge,
                vec![
                    (Some(DrcCheck::EdgeClearance), vec![F::EdgeClearance]),
                    (Some(DrcCheck::SilkToEdge), vec![F::SilkToEdge]),
                ]
            ),
            (
                Rule::MaskPaste,
                vec![
                    (
                        None,
                        vec![
                            F::MaskTop,
                            F::MaskBottom,
                            F::MaskFromHoleEdge,
                            F::TentViasTop,
                            F::TentViasBottom
                        ]
                    ),
                    (None, vec![F::PasteExpansion, F::PastePercent]),
                ]
            ),
            (
                Rule::Silkscreen,
                vec![(Some(DrcCheck::SilkToMask), vec![F::SilkToMask])]
            ),
            (
                Rule::ZoneConnections,
                [
                    ConnectionTarget::ThroughHole,
                    ConnectionTarget::Smd,
                    ConnectionTarget::Via
                ]
                .map(|t| (None, RuleField::connection(t).to_vec()))
                .to_vec()
            ),
        ]
    );
}

#[test]
fn silkscreen_maximums_and_drc_values_apply_through_pcb_rules_set() {
    let (home, work) = (TempDir::new("bs-home"), TempDir::new("bs-work"));
    let mut shell = shell_with_board(&home, &work);
    let power = add_class(&mut shell, json!({"name": "Power"}));
    let mut d = dialog(&shell);
    {
        let mut board = d.page::<DrcPage>().unwrap().board();
        // Seeded values (spec: silk_to_edge 0.254, silk_to_mask 0.10, micro_segment 0.01).
        assert_eq!(board.rule(RuleField::SilkToEdge), "0.254");
        assert_eq!(board.rule(RuleField::SilkToMask), "0.1");
        assert_eq!(board.rule(RuleField::MicroSegmentLength), "0.01");
        assert_eq!(board.rule(RuleField::CourtyardClearance), "0");
        assert_eq!(board.rule(RuleField::DanglingTolerance), "0");
        assert_eq!(board.rule(RuleField::TrackMax), "", "maximums start empty");
        assert_eq!(board.silk_mode(), SilkMaskMode::ExposedCopper);
        board.set_rule(RuleField::TrackMax, "1");
        board.set_rule(RuleField::SilkToEdge, "0.3");
        board.set_silk_mode(SilkMaskMode::MaskOpening);
        board.set_rule(RuleField::CourtyardClearance, "0.25");
        board.set_rule(RuleField::DanglingTolerance, "0.05");
        board.set_rule(RuleField::MicroSegmentLength, "0.02");
        board.set_class_override(ClassKey::Existing(power), RuleField::SilkToMask, "0.2");
    }
    d.apply(&mut shell).unwrap();
    let pcb = pcb(&shell);
    let value = |kind| pcb.board_rule(kind).values;
    assert_eq!(value(RuleKind::TrackWidth).max, Some(Nm::from_um(1000)));
    assert_eq!(value(RuleKind::SilkToEdge).value, Some(Nm::from_um(300)));
    assert_eq!(
        value(RuleKind::SilkToMask).mode,
        Some(SilkMaskMode::MaskOpening)
    );
    assert_eq!(
        value(RuleKind::CourtyardClearance).value,
        Some(Nm::from_um(250))
    );
    assert_eq!(value(RuleKind::Dangling).tolerance, Some(Nm::from_um(50)));
    assert_eq!(
        value(RuleKind::MicroSegment).max_length,
        Some(Nm::from_um(20))
    );
    let silk = pcb.class_rule(RuleKind::SilkToMask, power).unwrap();
    assert_eq!(silk.values.value, Some(Nm::from_um(200)));
}

#[test]
fn the_drc_page_owns_the_values_beside_its_checks() {
    let (home, work) = (TempDir::new("bs-home"), TempDir::new("bs-work"));
    let shell = shell_with_board(&home, &work);
    let d = dialog(&shell);
    assert_eq!(
        DrcPage::VALUES,
        [
            (DrcCheck::CourtyardOverlap, RuleField::CourtyardClearance),
            (DrcCheck::Dangling, RuleField::DanglingTolerance),
            (DrcCheck::MicroSegment, RuleField::MicroSegmentLength),
        ]
    );
    d.page::<DrcPage>()
        .unwrap()
        .board()
        .set_rule(RuleField::DanglingTolerance, "-1");
    let flagged: Vec<(String, String)> = d
        .problems()
        .into_iter()
        .map(|(page, p)| (page, p.field))
        .collect();
    assert_eq!(
        flagged,
        [("DRC".to_owned(), "rule/dangling.tolerance".to_owned())]
    );
}

#[test]
fn each_value_shows_the_strictest_limit_of_the_job_presets() {
    let (home, work) = (TempDir::new("bs-home"), TempDir::new("bs-work"));
    let mut shell = shell_with_board(&home, &work);
    let pcbway = shell
        .run("fab_preset.add", json!({"name": "PCBWay"}))
        .unwrap();
    let pcbway = pcbway["result"]["uuid"].as_str().unwrap().to_owned();
    shell
        .run("output.job.add", json!({"fab_preset": pcbway}))
        .unwrap();
    let d = dialog(&shell);
    let board = d.page::<DrcPage>().unwrap().board();
    let limit = |f| board.strictest(f).map(|(l, n, p)| (l, n, p.to_owned()));
    // Generic is stricter on clearance (0.16 against 0.10) and the Pad hole minimum
    // (0.50 against 0.20); both cap holes at 6.3 mm.
    assert_eq!(
        limit(RuleField::Clearance),
        Some((Limit::Min, Nm::from_um(160), "Generic".to_owned()))
    );
    assert_eq!(
        limit(RuleField::HoleMin),
        Some((Limit::Min, Nm::from_um(500), "Generic".to_owned()))
    );
    let (way, max, _) = limit(RuleField::HoleMax).unwrap();
    assert_eq!((way, max), (Limit::Max, Nm::from_um(6300)));
    assert_eq!(
        limit(RuleField::TrackPreferred),
        None,
        "preferred is never a limit"
    );
    assert_eq!(
        limit(RuleField::SilkToEdge),
        None,
        "presets set no silkscreen limit"
    );
}

#[test]
fn the_grid_has_a_default_row_then_the_classes_in_rank_order_and_dragging_reranks() {
    let (home, work) = (TempDir::new("bs-home"), TempDir::new("bs-work"));
    let mut shell = shell_with_board(&home, &work);
    let power = add_class(&mut shell, json!({"name": "Power"}));
    let signal = add_class(&mut shell, json!({"name": "Signal"}));
    let mut d = dialog(&shell);
    let (p, s) = (ClassKey::Existing(power), ClassKey::Existing(signal));
    let usb = {
        let mut board = d.page::<NetClassesPage>().unwrap().board();
        assert_eq!(board.grid_rows(), [ClassKey::Default, p, s]);
        let usb = board.add_class();
        board.rename_class(usb, "USB");
        // Drag USB to the top and Signal above Power.
        board.move_class(usb, 0);
        board.move_class(s, 1);
        assert_eq!(board.grid_rows(), [ClassKey::Default, usb, s, p]);
        usb
    };
    let steps = d.steps();
    let ids: Vec<&str> = steps.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["pcb.net_class.add", "pcb.net_class.reorder"]);
    assert_eq!(
        steps[1].params,
        json!({"classes": ["USB", signal.to_string(), power.to_string()]})
    );
    let _ = usb;
    d.apply(&mut shell).unwrap();
    let pcb = pcb(&shell);
    let rank = |name: &str| pcb.net_class_named(name).unwrap().rank;
    assert_eq!((rank("USB"), rank("Signal"), rank("Power")), (1, 2, 3));
    assert!(!d.has_changes(), "the applied order is the board's");
}

#[test]
fn the_default_row_edits_the_board_wide_records_and_class_cells_show_what_they_inherit() {
    let (home, work) = (TempDir::new("bs-home"), TempDir::new("bs-work"));
    let mut shell = shell_with_board(&home, &work);
    let power = add_class(
        &mut shell,
        json!({"name": "Power", "rules": [{"kind": "clearance", "values": {"value": "0.3mm"}}]}),
    );
    let d = dialog(&shell);
    let p = ClassKey::Existing(power);
    let mut board = d.page::<NetClassesPage>().unwrap().board();
    assert_eq!(board.cell(ClassKey::Default, RuleField::Clearance), "0.16");
    assert_eq!(board.cell(p, RuleField::Clearance), "0.3");
    assert_eq!(board.cell(p, RuleField::TrackWidth), "", "not overridden");
    assert_eq!(board.inherited(p, RuleField::TrackWidth), "0.2");
    board.type_cell(ClassKey::Default, RuleField::TrackWidth, "0.15");
    assert_eq!(board.rule(RuleField::TrackWidth), "0.15");
    assert_eq!(
        board.inherited(p, RuleField::TrackWidth),
        "0.15",
        "follows the draft"
    );
    assert!(board.has_record(p, RuleKind::Clearance));
    assert!(!board.has_record(p, RuleKind::TrackWidth));
}

#[test]
fn typing_min_or_preferred_for_a_class_without_a_record_fills_its_empty_values() {
    let (home, work) = (TempDir::new("bs-home"), TempDir::new("bs-work"));
    let mut shell = shell_with_board(&home, &work);
    let power = add_class(
        &mut shell,
        json!({"name": "Power", "rules": [{"kind": "via_size", "values": {"diameter": {"min": "0.5mm"}}}]}),
    );
    let d = dialog(&shell);
    let p = ClassKey::Existing(power);
    let mut board = d.page::<NetClassesPage>().unwrap().board();
    // Typed key by key: the empty values follow.
    board.type_cell(p, RuleField::TrackPreferred, "0");
    board.type_cell(p, RuleField::TrackPreferred, "0.");
    board.type_cell(p, RuleField::TrackPreferred, "0.4");
    let track = |b: &board_setup::BoardSetupDraft| {
        [
            RuleField::TrackWidth,
            RuleField::TrackPreferred,
            RuleField::TrackMax,
        ]
        .map(|f| b.cell(p, f).to_owned())
    };
    assert_eq!(track(&board), ["0.4", "0.4", "0.4"]);
    // A value typed straight into a filled cell is its own from then on.
    board.type_cell(p, RuleField::TrackMax, "1");
    board.type_cell(p, RuleField::TrackPreferred, "0.5");
    assert_eq!(track(&board), ["0.5", "0.5", "1"]);
    // Typing max fills nothing; a class with a record of that kind fills nothing.
    board.type_cell(p, RuleField::ViaHoleMax, "0.6");
    assert_eq!(board.cell(p, RuleField::ViaHole), "");
    board.type_cell(p, RuleField::ViaDiameterPreferred, "0.7");
    assert_eq!(board.cell(p, RuleField::ViaDiameterMax), "");
    // Delete clears any of them back to inherited.
    board.type_cell(p, RuleField::TrackMax, "");
    assert_eq!(board.cell(p, RuleField::TrackMax), "");
}

#[test]
fn overrides_carry_enabled_and_a_comment_per_record() {
    let (home, work) = (TempDir::new("bs-home"), TempDir::new("bs-work"));
    let mut shell = shell_with_board(&home, &work);
    let power = add_class(
        &mut shell,
        json!({"name": "Power", "rules": [{"kind": "clearance", "values": {"value": "0.3mm"}}]}),
    );
    let mut d = dialog(&shell);
    let p = ClassKey::Existing(power);
    {
        let mut board = d.page::<NetClassesPage>().unwrap().board();
        assert!(board.enabled(p, RuleKind::Clearance));
        board.set_enabled(p, RuleKind::Clearance, false);
        board.set_comment(p, RuleKind::Clearance, "HV spacing");
        board.set_enabled(ClassKey::Default, RuleKind::TrackWidth, false);
        board.set_comment(ClassKey::Default, RuleKind::TrackWidth, "fab says so");
        let usb = board.add_class();
        board.rename_class(usb, "USB");
        board.type_cell(usb, RuleField::Clearance, "0.2");
        board.set_comment(usb, RuleKind::Clearance, "90 ohm");
        board.set_enabled(usb, RuleKind::Clearance, false);
    }
    assert_eq!(
        d.steps(),
        [
            Step::new(
                "pcb.rules.set",
                json!({"kind": "track_width", "values": {}, "enabled": false,
                       "comment": "fab says so"})
            ),
            Step::new(
                "pcb.rules.set",
                json!({"kind": "clearance", "scope": {"net_class": power}, "values": {},
                       "enabled": false, "comment": "HV spacing"})
            ),
            Step::new(
                "pcb.net_class.add",
                json!({"name": "USB", "rules": [{"kind": "clearance",
                       "values": {"value": 200_000}, "enabled": false, "comment": "90 ohm"}]})
            ),
        ]
    );
    d.apply(&mut shell).unwrap();
    let pcb = pcb(&shell);
    let record = pcb.class_rule(RuleKind::Clearance, power).unwrap();
    assert!(!record.enabled);
    assert_eq!(record.comment.as_deref(), Some("HV spacing"));
    assert!(!pcb.board_rule(RuleKind::TrackWidth).enabled);
    let usb = pcb.net_class_named("USB").unwrap().uuid;
    assert_eq!(
        pcb.class_rule(RuleKind::Clearance, usb)
            .unwrap()
            .comment
            .as_deref(),
        Some("90 ohm")
    );
}

#[test]
fn grid_rows_copy_and_paste_as_tab_separated_values_and_delete_clears_them() {
    let (home, work) = (TempDir::new("bs-home"), TempDir::new("bs-work"));
    let mut shell = shell_with_board(&home, &work);
    let power = add_class(
        &mut shell,
        json!({"name": "Power", "rules": [
            {"kind": "clearance", "values": {"value": "0.3mm"}},
            {"kind": "track_width", "values": {"preferred": "0.5mm"}}]}),
    );
    let signal = add_class(&mut shell, json!({"name": "Signal"}));
    let d = dialog(&shell);
    let (p, s) = (ClassKey::Existing(power), ClassKey::Existing(signal));
    let mut board = d.page::<NetClassesPage>().unwrap().board();
    assert_eq!(
        board_setup::GRID_FIELDS,
        [
            RuleField::Clearance,
            RuleField::TrackWidth,
            RuleField::TrackPreferred,
            RuleField::TrackMax,
            RuleField::ViaDiameter,
            RuleField::ViaDiameterPreferred,
            RuleField::ViaDiameterMax,
            RuleField::ViaHole,
            RuleField::ViaHolePreferred,
            RuleField::ViaHoleMax,
        ]
    );
    board.selected_rows = [p].into();
    let copied = board.copy_rows();
    assert_eq!(copied, "0.3\t\t0.5\t\t\t\t\t\t\t");
    board.selected_rows = [s].into();
    board.paste_rows(&copied);
    assert_eq!(board.cell(s, RuleField::Clearance), "0.3");
    assert_eq!(board.cell(s, RuleField::TrackPreferred), "0.5");
    assert_eq!(
        board.cell(s, RuleField::TrackWidth),
        "",
        "pasting fills nothing"
    );
    // Two lines paste into the selected row and the one below it.
    board.selected_rows = [ClassKey::Default].into();
    board.paste_rows("0.17\n0.25");
    assert_eq!(board.rule(RuleField::Clearance), "0.17");
    assert_eq!(board.cell(p, RuleField::Clearance), "0.25");
    board.selected_rows = [p, s].into();
    board.clear_rows();
    assert_eq!(board.cell(p, RuleField::Clearance), "");
    assert_eq!(board.cell(s, RuleField::TrackPreferred), "");
}

#[test]
fn the_grid_filters_by_name_and_to_classes_with_overrides() {
    let (home, work) = (TempDir::new("bs-home"), TempDir::new("bs-work"));
    let mut shell = shell_with_board(&home, &work);
    let power = add_class(
        &mut shell,
        json!({"name": "Power", "rules": [{"kind": "clearance", "values": {"value": "0.3mm"}}]}),
    );
    let signal = add_class(&mut shell, json!({"name": "Signal"}));
    let d = dialog(&shell);
    let (p, s) = (ClassKey::Existing(power), ClassKey::Existing(signal));
    let mut board = d.page::<NetClassesPage>().unwrap().board();
    board.grid_search = "SIG".into();
    assert_eq!(board.grid_rows(), [s], "the search ignores case");
    board.grid_search.clear();
    board.only_overrides = true;
    assert_eq!(board.grid_rows(), [ClassKey::Default, p]);
}

#[test]
fn the_net_table_makes_a_class_from_the_selection_and_assigns_by_pattern() {
    let (home, work) = (TempDir::new("bs-home"), TempDir::new("bs-work"));
    let mut shell = shell_with_board(&home, &work);
    let (usb_p, usb_n, vcc, gnd) = (
        add_net(&mut shell, "USB_P"),
        add_net(&mut shell, "USB_N"),
        add_net(&mut shell, "VCC"),
        add_net(&mut shell, "GND"),
    );
    let power = add_class(&mut shell, json!({"name": "Power", "nets": [gnd]}));
    let mut d = dialog(&shell);
    {
        let mut board = d.page::<NetClassesPage>().unwrap().board();
        board.selected_nets = [usb_p, usb_n].into();
        let usb = board.new_class_from_selection();
        assert_eq!(board.class_name_mut(usb).unwrap(), "Class 1");
        board.rename_class(usb, "USB");
        let p = ClassKey::Existing(power);
        assert_eq!(board.assign_by_pattern("v*", p), 1, "VCC, ignoring case");
        // A Net can be in several classes.
        board.toggle_net_class(usb_p, p);
        let row = board.nets().iter().find(|n| n.uuid == usb_p).unwrap();
        assert_eq!(row.classes().len(), 2);
    }
    d.apply(&mut shell).unwrap();
    let pcb = pcb(&shell);
    let usb = pcb.net_class_named("USB").unwrap().uuid;
    let mut both = vec![usb, power];
    both.sort();
    assert_eq!(pcb.net(usb_p).unwrap().classes, both);
    assert_eq!(pcb.net(usb_n).unwrap().classes, [usb]);
    assert_eq!(pcb.net(vcc).unwrap().classes, [power]);
    assert_eq!(pcb.net(gnd).unwrap().classes, [power]);
}
