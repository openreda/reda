//! The Board Setup pages added after the rule pages, without a window: Mask & Paste, Zone
//! Connections (with the Zone naming scheme) and Stackup, plus the dialog's own undo of
//! pending edits and its rule actions.

use reda_testkit::TempDir;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::model::{
    ConnectionTarget as T, ConnectionValue as V, PcbDocument, RuleKind, Uuid, ZoneNameScheme,
};
use reda_ui::settings::board_setup::{self, ClassKey, DrcPage, PasteMode, Rule, RuleField};
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

fn add_class(shell: &mut Shell, name: &str) -> Uuid {
    let out = shell
        .run("pcb.net_class.add", json!({ "name": name }))
        .unwrap();
    out["result"]["uuid"].as_str().unwrap().parse().unwrap()
}

#[test]
fn the_rules_group_gains_mask_and_paste_and_zone_connections_then_stackup() {
    let (home, work) = (TempDir::new("bsp-home"), TempDir::new("bsp-work"));
    let shell = shell_with_board(&home, &work);
    assert_eq!(
        dialog(&shell).page_titles(),
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
    // Neither has a DRC severity.
    for rule in [Rule::MaskPaste, Rule::ZoneConnections] {
        assert!(rule.sections().iter().all(|s| s.check.is_none()));
    }
}

#[test]
fn mask_expansion_links_top_and_bottom_and_takes_negative_values() {
    let (home, work) = (TempDir::new("bsp-home"), TempDir::new("bsp-work"));
    let mut shell = shell_with_board(&home, &work);
    let mut d = dialog(&shell);
    {
        let mut board = d.page::<DrcPage>().unwrap().board();
        // The Generic preset seeds 0.05 mm on both sides, Vias tented.
        assert_eq!(board.rule(RuleField::MaskTop), "0.05");
        assert_eq!(board.rule(RuleField::MaskBottom), "0.05");
        assert_eq!(board.rule(RuleField::TentViasTop), "true");
        assert_eq!(board.rule(RuleField::MaskFromHoleEdge), "false");
        assert!(board.mask_linked(), "equal sides start linked");
        board.set_rule(RuleField::MaskTop, "0.1");
        assert_eq!(board.rule(RuleField::MaskBottom), "0.1", "linked");
        board.set_mask_linked(false);
        board.set_rule(RuleField::MaskBottom, "-0.02");
        assert_eq!(board.rule(RuleField::MaskTop), "0.1");
        board.set_rule(RuleField::TentViasBottom, "false");
        board.set_rule(RuleField::MaskFromHoleEdge, "true");
    }
    assert!(d.problems().is_empty(), "{:?}", d.problems());
    assert_eq!(
        d.steps(),
        [Step::new(
            "pcb.rules.set",
            json!({"kind": "mask_expansion", "values": {
                "top": 100_000, "bottom": -20_000,
                "from_hole_edge": true, "tent_vias_bottom": false}})
        )]
    );
    d.apply(&mut shell).unwrap();
    let mask = pcb(&shell).board_rule(RuleKind::MaskExpansion).values;
    assert_eq!(mask.bottom.unwrap().0, -20_000);
    assert_eq!(mask.tent_vias_bottom, Some(false));
}

#[test]
fn paste_expansion_is_absolute_or_a_percentage() {
    let (home, work) = (TempDir::new("bsp-home"), TempDir::new("bsp-work"));
    let mut shell = shell_with_board(&home, &work);
    let mut d = dialog(&shell);
    {
        let mut board = d.page::<DrcPage>().unwrap().board();
        assert_eq!(board.paste_mode(), PasteMode::Absolute);
        assert_eq!(board.rule(RuleField::PasteExpansion), "0");
        board.set_paste_mode(PasteMode::Percent);
        assert_eq!(board.rule(RuleField::PasteExpansion), "");
        board.set_rule(RuleField::PastePercent, "-150");
    }
    assert_eq!(
        d.problems()
            .into_iter()
            .map(|(page, p)| (page, p.field))
            .collect::<Vec<_>>(),
        [(
            "Mask & Paste".to_owned(),
            "rule/paste_expansion.percent".to_owned()
        )]
    );
    d.page::<DrcPage>()
        .unwrap()
        .board()
        .set_rule(RuleField::PastePercent, "-10");
    assert_eq!(
        d.steps(),
        [Step::new(
            "pcb.rules.set",
            json!({"kind": "paste_expansion",
                   "values": {"expansion": null, "percent": -10}})
        )]
    );
    d.apply(&mut shell).unwrap();
    let paste = pcb(&shell).board_rule(RuleKind::PasteExpansion).values;
    assert_eq!((paste.expansion, paste.percent), (None, Some(-10)));
    let board = d.page::<DrcPage>().unwrap().board();
    assert_eq!(board.paste_mode(), PasteMode::Percent);
}

#[test]
fn zone_connections_set_values_per_target_and_class_and_the_naming_scheme() {
    let (home, work) = (TempDir::new("bsp-home"), TempDir::new("bsp-work"));
    let mut shell = shell_with_board(&home, &work);
    let power = add_class(&mut shell, "Power");
    let mut d = dialog(&shell);
    let smd = |v| RuleField::Connection(T::Smd, v);
    {
        let mut board = d.page::<DrcPage>().unwrap().board();
        assert_eq!(board.rule(smd(V::Style)), "relief");
        assert_eq!(board.rule(smd(V::Spokes)), "4");
        assert_eq!(board.rule(smd(V::Gap)), "0.25");
        assert_eq!(board.rule(smd(V::Angle)), "0");
        assert_eq!(
            board.rule(RuleField::Connection(T::Via, V::Style)),
            "solid"
        );
        board.set_rule(smd(V::Spokes), "2");
        board.set_rule(smd(V::Angle), "90");
        board.set_rule(smd(V::Gap), "0.3");
        board.set_class_override(ClassKey::Existing(power), smd(V::Style), "solid");
        assert_eq!(board.zone_name_scheme(), ZoneNameScheme::NetLxxPxxx);
        board.set_zone_name_scheme(ZoneNameScheme::LxxNetPxxx);
    }
    assert_eq!(
        d.steps(),
        [
            Step::new(
                "pcb.rules.set",
                json!({"kind": "zone_connection", "values": {
                    "smd": {"spokes": 2, "angle": 90_000, "gap": 300_000}}})
            ),
            Step::new(
                "pcb.zone.set_name_scheme",
                json!({"scheme": "LXX_NET_PXXX"})
            ),
            Step::new(
                "pcb.rules.set",
                json!({"kind": "zone_connection", "scope": {"net_class": power},
                       "values": {"smd": {"style": "solid"}}})
            ),
        ]
    );
    d.apply(&mut shell).unwrap();
    let board = pcb(&shell);
    assert_eq!(board.setup().zone_name_scheme, ZoneNameScheme::LxxNetPxxx);
    let class = board.class_rule(RuleKind::ZoneConnection, power).unwrap();
    assert_eq!(
        class.values.smd.unwrap().style,
        Some(reda_ui::engine::model::ConnectionStyle::Solid)
    );
}

#[test]
fn a_bad_zone_connection_value_is_flagged_on_its_page() {
    let (home, work) = (TempDir::new("bsp-home"), TempDir::new("bsp-work"));
    let shell = shell_with_board(&home, &work);
    let d = dialog(&shell);
    {
        let mut board = d.page::<DrcPage>().unwrap().board();
        board.set_rule(RuleField::Connection(T::ThroughHole, V::Spokes), "3");
        board.set_rule(RuleField::Connection(T::Via, V::Angle), "30");
    }
    let flagged: Vec<(String, String)> = d
        .problems()
        .into_iter()
        .map(|(page, p)| (page, p.field))
        .collect();
    assert_eq!(
        flagged,
        [
            (
                "Zone Connections".to_owned(),
                "rule/zone_connection.through_hole.spokes".to_owned()
            ),
            (
                "Zone Connections".to_owned(),
                "rule/zone_connection.via.angle".to_owned()
            ),
        ]
    );
}

#[test]
fn the_stackup_page_edits_thicknesses_with_live_totals_finish_colours_and_fab_notes() {
    use reda_ui::engine::model::geom::Nm;
    use reda_ui::settings::board_setup::Band;
    let (home, work) = (TempDir::new("bsp-home"), TempDir::new("bsp-work"));
    let mut shell = shell_with_board(&home, &work);
    let top = pcb(&shell).stackup().copper().next().unwrap().layer;
    let mut d = dialog(&shell);
    {
        let mut board = d.page::<DrcPage>().unwrap().board();
        let stackup = board.stackup_mut();
        let rows: Vec<(String, bool, String)> = stackup
            .rows()
            .iter()
            .map(|r| (r.name.clone(), r.copper, r.thickness().to_owned()))
            .collect();
        assert_eq!(
            rows,
            [
                ("Top Copper".to_owned(), true, "0.035".to_owned()),
                ("Dielectric 1".to_owned(), false, String::new()),
                ("Bottom Copper".to_owned(), true, "0.035".to_owned()),
            ]
        );
        assert_eq!(stackup.target(), "1.6");
        let totals = stackup.totals().unwrap();
        assert_eq!(totals.copper, Nm::from_um(70));
        assert_eq!(totals.remainder, Some(Nm::from_um(1530)));
        // Copper takes oz; the blank dielectric's remainder follows as you type.
        stackup.set_row(0, "2oz");
        let totals = stackup.totals().unwrap();
        assert_eq!(totals.copper, Nm::from_um(105));
        assert_eq!(totals.remainder, Some(Nm::from_um(1495)));
        assert_eq!(totals.total, Nm::from_um(1600));
        assert!(stackup.warning().is_none());
        // Every dielectric typed in: the total shows against the target, with a warning.
        stackup.set_row(1, "1.6");
        assert_eq!(stackup.totals().unwrap().total, Nm::from_um(1705));
        assert!(stackup.warning().is_some());
        stackup.set_row(1, "");
        // The cross-section lists mask and silkscreen outside the copper.
        let bands: Vec<(Band, String)> = stackup
            .cross_section()
            .into_iter()
            .map(|b| (b.band, b.thickness))
            .collect();
        assert_eq!(
            bands,
            [
                (Band::Silkscreen, String::new()),
                (Band::Mask, String::new()),
                (Band::Copper, "0.07".to_owned()),
                (Band::Dielectric, "1.495".to_owned()),
                (Band::Copper, "0.035".to_owned()),
                (Band::Mask, String::new()),
                (Band::Silkscreen, String::new()),
            ]
        );
        assert!(!stackup.suggestions().finish.is_empty());
        stackup.set_finish("ENIG");
        stackup.set_mask_color("Green");
        stackup.set_fab_notes("Impedance control not needed.");
    }
    assert!(d.problems().is_empty(), "{:?}", d.problems());
    assert_eq!(
        d.steps(),
        [
            Step::new(
                "pcb.stackup.set",
                json!({"copper": [{"layer": top, "thickness": 70_000}],
                       "finish": "ENIG", "mask_color": "Green"})
            ),
            Step::new(
                "pcb.fab_notes.set",
                json!({"text": "Impedance control not needed."})
            ),
        ]
    );
    d.apply(&mut shell).unwrap();
    let board = pcb(&shell);
    assert_eq!(board.stackup().finish.as_deref(), Some("ENIG"));
    assert_eq!(
        board.setup().fab_notes.as_deref(),
        Some("Impedance control not needed.")
    );
    assert!(!d.has_changes());
}

#[test]
fn a_stackup_that_leaves_nothing_for_the_blank_dielectric_is_flagged() {
    let (home, work) = (TempDir::new("bsp-home"), TempDir::new("bsp-work"));
    let shell = shell_with_board(&home, &work);
    let d = dialog(&shell);
    {
        let mut board = d.page::<DrcPage>().unwrap().board();
        let stackup = board.stackup_mut();
        stackup.set_target("0.06");
        stackup.set_row(0, "2oz");
        stackup.set_row(2, "thick");
    }
    let flagged: Vec<(String, String)> = d
        .problems()
        .into_iter()
        .map(|(page, p)| (page, p.field))
        .collect();
    assert_eq!(
        flagged,
        [
            ("Stackup".to_owned(), "stackup/row/2".to_owned()),
            ("Stackup".to_owned(), "stackup/thickness".to_owned()),
        ]
    );
}

#[test]
fn stepping_back_undoes_pending_edits_one_at_a_time_and_apply_clears_them() {
    let (home, work) = (TempDir::new("bsp-home"), TempDir::new("bsp-work"));
    let mut shell = shell_with_board(&home, &work);
    let mut d = dialog(&shell);
    assert!(!d.step_back(), "nothing pending");
    {
        let mut board = d.page::<DrcPage>().unwrap().board();
        board.set_rule(RuleField::Clearance, "0.2");
    }
    d.checkpoint();
    {
        let mut board = d.page::<DrcPage>().unwrap().board();
        board.set_rule(RuleField::TrackWidth, "0.3");
        board.stackup_mut().set_finish("ENIG");
    }
    d.checkpoint();
    // A check with no change in between adds no step.
    d.checkpoint();
    assert!(d.step_back());
    assert_eq!(
        d.steps(),
        [Step::new(
            "pcb.rules.set",
            json!({"kind": "clearance", "values": {"value": 200_000}})
        )]
    );
    assert_eq!(d.page::<DrcPage>().unwrap().board().stackup().finish(), "");
    assert!(d.step_back());
    assert!(!d.has_changes());
    assert!(!d.step_back());

    // Edits not yet checked are stepped back too.
    d.page::<DrcPage>()
        .unwrap()
        .board()
        .set_rule(RuleField::Clearance, "0.25");
    d.checkpoint();
    d.page::<DrcPage>()
        .unwrap()
        .board()
        .set_rule(RuleField::Clearance, "0.3");
    assert!(d.step_back());
    assert_eq!(
        d.page::<DrcPage>().unwrap().board().rule(RuleField::Clearance),
        "0.25"
    );
    d.apply(&mut shell).unwrap();
    assert!(!d.step_back(), "after Apply the edits are one undo entry");
}
