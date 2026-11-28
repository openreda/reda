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
            "Teardrops",
            "Stackup",
            "Net Classes",
            "DRC",
            "Grids"
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
    let mask = &pcb(&shell).board_rule(RuleKind::MaskExpansion).values;
    assert_eq!(mask.bottom.unwrap().0, -20_000);
    assert_eq!(mask.tent_vias_bottom, Some(false));
}

#[test]
fn silkscreen_minimums_can_be_edited_then_reset_to_the_project_preset() {
    let (home, work) = (TempDir::new("bsp-home"), TempDir::new("bsp-work"));
    let mut shell = shell_with_board(&home, &work);
    let mut d = dialog(&shell);
    {
        let mut board = d.page::<DrcPage>().unwrap().board();
        assert_eq!(board.rule(RuleField::SilkWidth), "0.15");
        assert_eq!(board.rule(RuleField::SilkTextHeight), "1");
        board.set_rule(RuleField::SilkWidth, "0.2");
        board.set_rule(RuleField::SilkTextHeight, "1.2");
    }
    d.apply(&mut shell).unwrap();
    assert_eq!(RuleField::SilkWidth.get(pcb(&shell)).unwrap().0, 200_000);
    assert_eq!(
        RuleField::SilkTextHeight.get(pcb(&shell)).unwrap().0,
        1_200_000
    );
    {
        let mut board = d.page::<DrcPage>().unwrap().board();
        board.reset_page(Rule::Silkscreen);
        assert_eq!(board.fab_preview().unwrap().changes.len(), 2);
        board.accept_fab_preview();
    }
    d.apply(&mut shell).unwrap();
    assert_eq!(RuleField::SilkWidth.get(pcb(&shell)).unwrap().0, 150_000);
    assert_eq!(
        RuleField::SilkTextHeight.get(pcb(&shell)).unwrap().0,
        1_000_000
    );
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
    let paste = &pcb(&shell).board_rule(RuleKind::PasteExpansion).values;
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
        assert_eq!(board.rule(RuleField::Connection(T::Via, V::Style)), "solid");
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
        *board.fab_notes_mut() = "Do not panelize".into();
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
    assert_eq!(d.page::<DrcPage>().unwrap().board().fab_notes_mut(), "");
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
        d.page::<DrcPage>()
            .unwrap()
            .board()
            .rule(RuleField::Clearance),
        "0.25"
    );
    d.apply(&mut shell).unwrap();
    assert!(!d.step_back(), "after Apply the edits are one undo entry");
}

fn preset(shell: &Shell) -> Uuid {
    shell
        .engine()
        .project()
        .unwrap()
        .fab_presets()
        .next()
        .unwrap()
        .uuid
}

#[test]
fn the_fab_preset_preview_ticks_limits_and_applies_only_the_ticked_rows() {
    let (home, work) = (TempDir::new("bsp-home"), TempDir::new("bsp-work"));
    let mut shell = shell_with_board(&home, &work);
    shell
        .run(
            "pcb.rules.set",
            json!({"kind": "via_size", "values": {"hole": {"min": "0.4mm"}}}),
        )
        .unwrap();
    let preset = preset(&shell);
    let d = dialog(&shell);
    let mut board = d.page::<DrcPage>().unwrap().board();
    board.set_rule(RuleField::Clearance, "0.3");
    board.set_rule(RuleField::TrackPreferred, "0.4");
    board.set_rule(RuleField::MaskTop, "0.1");
    board.preview_fab_preset(preset);
    let rows: Vec<(RuleField, String, bool)> = board
        .fab_preview()
        .unwrap()
        .changes
        .iter()
        .map(|c| (c.rule, c.preset.clone(), c.ticked))
        .collect();
    // The preferred Track width stays inside the new range, so it starts unticked.
    assert_eq!(
        rows,
        [
            (RuleField::Clearance, "0.16".to_owned(), true),
            (RuleField::TrackPreferred, "0.25".to_owned(), false),
            (RuleField::ViaHole, "0.3".to_owned(), true),
            (RuleField::MaskTop, "0.05".to_owned(), true),
            (RuleField::MaskBottom, "0.05".to_owned(), true),
        ]
    );
    board.set_preview_ticked(0, false);
    board.set_preview_ticked(1, true);
    board.accept_fab_preview();
    assert_eq!(board.rule(RuleField::Clearance), "0.3", "unticked");
    assert_eq!(board.rule(RuleField::ViaHole), "0.3");
    assert_eq!(board.rule(RuleField::TrackPreferred), "0.25");
    assert_eq!(board.rule(RuleField::MaskTop), "0.05");
}

#[test]
fn reset_page_to_fab_preset_previews_only_that_pages_values() {
    let (home, work) = (TempDir::new("bsp-home"), TempDir::new("bsp-work"));
    let shell = shell_with_board(&home, &work);
    let d = dialog(&shell);
    let mut board = d.page::<DrcPage>().unwrap().board();
    let seeded = board.rule(RuleField::TrackWidth).to_owned();
    board.set_rule(RuleField::Clearance, "0.3");
    board.set_rule(RuleField::TrackWidth, "0.4");
    assert_eq!(board.page_preset(), Some(preset(&shell)));
    board.reset_page(Rule::TrackWidth);
    let preview = board.fab_preview().unwrap();
    assert_eq!(preview.page, Some(Rule::TrackWidth));
    let rules: Vec<RuleField> = preview.changes.iter().map(|c| c.rule).collect();
    assert_eq!(rules, [RuleField::TrackWidth]);
    board.accept_fab_preview();
    assert_eq!(board.rule(RuleField::TrackWidth), seeded);
    assert_eq!(
        board.rule(RuleField::Clearance),
        "0.3",
        "another page's value"
    );
}

#[test]
fn rules_export_to_a_file_and_import_after_a_preview() {
    use reda_ui::settings::FileRequest;
    let (home, work) = (TempDir::new("bsp-home"), TempDir::new("bsp-work"));
    let mut shell = shell_with_board(&home, &work);
    shell
        .run(
            "pcb.rules.set",
            json!({"kind": "clearance", "values": {"value": "0.3mm"}}),
        )
        .unwrap();
    let file = work.path().join("house.redarules");
    let mut d = dialog(&shell);
    d.picked_file(&mut shell, FileRequest::ExportRules, &file);
    assert!(file.exists());
    let notice = d
        .page::<DrcPage>()
        .unwrap()
        .board()
        .notice()
        .unwrap()
        .to_owned();
    assert!(
        notice.contains("house.redarules") && !notice.starts_with("Exported 0 "),
        "the dialog says what it wrote where: {notice}"
    );
    shell
        .run(
            "pcb.rules.set",
            json!({"kind": "clearance", "values": {"value": "0.2mm"}}),
        )
        .unwrap();
    d.sync(shell.engine().project().unwrap());
    d.picked_file(&mut shell, FileRequest::ImportRules, &file);
    {
        let board = d.page::<DrcPage>().unwrap().board();
        let preview = board.import_preview().unwrap();
        assert!(
            preview.rows.iter().any(|r| r.contains("Clearance")),
            "{:?}",
            preview.rows
        );
    }
    assert_eq!(
        RuleField::Clearance.get(pcb(&shell)).unwrap().0,
        200_000,
        "a preview changes nothing"
    );
    let journal = shell.engine().journal().len();
    d.page::<DrcPage>()
        .unwrap()
        .board()
        .confirm_import(&mut shell);
    assert_eq!(shell.engine().journal().len(), journal + 1);
    assert_eq!(RuleField::Clearance.get(pcb(&shell)).unwrap().0, 300_000);
    d.sync(shell.engine().project().unwrap());
    let board = d.page::<DrcPage>().unwrap().board();
    assert!(board.import_preview().is_none());
    assert_eq!(board.rule(RuleField::Clearance), "0.3", "the field follows");
}

#[test]
fn rules_save_as_a_fab_preset_copy_the_dialog_then_offers() {
    let (home, work) = (TempDir::new("bsp-home"), TempDir::new("bsp-work"));
    let mut shell = shell_with_board(&home, &work);
    let mut d = dialog(&shell);
    d.page::<DrcPage>()
        .unwrap()
        .board()
        .save_as_preset(&mut shell, "House rules")
        .unwrap();
    d.sync(shell.engine().project().unwrap());
    let board = d.page::<DrcPage>().unwrap().board();
    let names: Vec<&str> = board.presets().map(|(_, n)| n).collect();
    assert!(names.contains(&"House rules"), "{names:?}");
}

#[test]
fn show_on_board_counts_what_a_row_affects() {
    use reda_ui::show_on_board::Shown;
    let (home, work) = (TempDir::new("bsp-home"), TempDir::new("bsp-work"));
    let mut shell = shell_with_board(&home, &work);
    shell
        .run(
            "pcb.track.add",
            json!({"layer": "copper_top", "width": "0.25mm",
                   "points": [{"x": "10mm", "y": "10mm"}, {"x": "15mm", "y": "10mm"}]}),
        )
        .unwrap();
    let rule = pcb(&shell).board_rule(RuleKind::Clearance).uuid;
    shell.show_on_board(&[Shown::Rule(rule)]);
    assert_eq!(shell.shown_on_board(), Some(1));
}

#[test]
fn selecting_a_rules_file_position_refreshes_the_preview_and_applies_only_that_row() {
    use reda_ui::settings::FileRequest;
    let (home, work) = (TempDir::new("bsp-map-home"), TempDir::new("bsp-map-work"));
    let mut shell = shell_with_board(&home, &work);
    let path = work.path().join("four-layer.redarules");
    std::fs::write(
        &path,
        json!({"format":2,"copper_layers":4,"net_classes":[{"name":"RF"}],"severities":{},
        "rules":[{"kind":"clearance","net_class":"RF","layers":[1,3],"values":{"value":450000}},
                 {"kind":"clearance","layers":[3,4],"values":{"value":750000}}] })
        .to_string(),
    )
    .unwrap();
    let mut d = dialog(&shell);
    d.picked_file(&mut shell, FileRequest::ImportRules, &path);
    let page = d.page::<DrcPage>().unwrap();
    let mut board = page.board();
    let view = board.import_preview().unwrap();
    assert_eq!(view.positions.len(), 2);
    assert!(!view.positions[0].selected);
    assert!(!view.positions[1].available);
    assert!(view.rows.is_empty());
    board.select_import_position(&mut shell, 0, true);
    let view = board.import_preview().unwrap();
    assert!(view.positions[0].selected);
    assert!(view.rows.iter().any(|s| s.contains("RF")));
    board.confirm_import(&mut shell);
    let target = pcb(&shell);
    let rf = target.net_class_named("RF").unwrap().uuid;
    let layer = target.copper_layers().next().unwrap().uuid;
    let scope = reda_engine::model::RuleScope {
        net_class: Some(rf),
        net_class_pair: None,
        layers: Some(reda_engine::model::RuleLayers::Named(vec![layer])),
    };
    assert_eq!(
        target
            .scoped_rule(reda_engine::model::RuleKind::Clearance, &scope)
            .unwrap()
            .values
            .value
            .unwrap()
            .0,
        450_000
    );
}

#[test]
fn a_conflict_in_the_default_selection_still_allows_importing_other_rows() {
    use reda_ui::settings::FileRequest;
    let (home, work) = (
        TempDir::new("bsp-conflict-home"),
        TempDir::new("bsp-conflict-work"),
    );
    let mut shell = shell_with_board(&home, &work);
    let first = pcb(&shell).copper_layers().next().unwrap().uuid;
    shell
        .run(
            "pcb.rules.set",
            json!({"kind":"clearance","scope":{"layers":[first]},
        "values":{"value":"0.3mm"}}),
        )
        .unwrap();
    let path = work.path().join("scoped.redarules");
    let file = json!({"format":2,"copper_layers":2,"net_classes":[{"name":"RF"}],"severities":{},"rules":[
        {"kind":"clearance","layers":[1,2],"values":{"value":400000}},
        {"kind":"clearance","net_class":"RF","layers":[1],"values":{"value":500000}}]});
    std::fs::write(&path, file.to_string()).unwrap();
    let mut d = dialog(&shell);
    d.picked_file(&mut shell, FileRequest::ImportRules, &path);
    let page = d.page::<DrcPage>().unwrap();
    let mut board = page.board();
    assert!(board.notice().unwrap().contains("Layer"));
    let view = board.import_preview().unwrap();
    assert_eq!(view.positions.len(), 2);
    assert!(view.positions.iter().all(|row| !row.selected));
    board.select_import_position(&mut shell, 1, true);
    board.confirm_import(&mut shell);
    let target = pcb(&shell);
    let class = target.net_class_named("RF").unwrap();
    let scope = reda_engine::model::RuleScope {
        net_class: Some(class.uuid),
        layers: Some(reda_engine::model::RuleLayers::Named(vec![first])),
        ..Default::default()
    };
    assert_eq!(
        target
            .scoped_rule(RuleKind::Clearance, &scope)
            .unwrap()
            .values
            .value
            .unwrap()
            .0,
        500_000
    );
}
