//! Shipped board constructions, checked against the fabrication tables.

use reda_model::{FabPreset, Uuid, geom::Nm};
use reda_output::shipped_named;

#[test]
fn generic_offers_even_constructions_without_material_data() {
    let generic = shipped_named("Generic").unwrap();
    assert_eq!(generic.constructions.len(), 4);
    for count in [2, 4, 6, 8] {
        let build = generic.default_construction(count).unwrap();
        assert_eq!(build.name, format!("{count} Layer"));
        let board = stackup(&generic, &build.name);
        assert_eq!(board.thickness, Nm(1_600_000));
        assert!(board.copper().all(|c| c.thickness == Nm(35_000)));
        assert!(
            board
                .dielectrics()
                .all(|d| d.material.is_none() && d.dk.is_none())
        );
        if count == 2 {
            assert_eq!(
                board.dielectrics().next().unwrap().thickness,
                Some(Nm(1_530_000))
            );
        } else {
            let gaps: Vec<_> = board
                .dielectrics()
                .map(|d| d.thickness.unwrap().0)
                .collect();
            assert!(gaps.iter().max().unwrap() - gaps.iter().min().unwrap() <= 1);
        }
        assert_eq!(board.totals().total, board.thickness);
        assert!(board.info().symmetric);
    }
}

fn stackup(preset: &FabPreset, name: &str) -> reda_model::Stackup {
    let build = preset.construction(name).unwrap();
    let mut ids = (1..).map(Uuid::from_u128);
    let copper: Vec<_> = ids.by_ref().take(build.layer_count()).collect();
    build
        .board_stackup(&copper, Nm(300_000), Nm(700_000), || ids.next().unwrap())
        .unwrap()
}

#[test]
fn jlcpcb_keeps_the_order_codes_pressed_plies_and_catalogue_asymmetry() {
    let jlc = shipped_named("JLCPCB").unwrap();
    assert_eq!(jlc.constructions.len(), 37);
    assert_eq!(jlc.default_construction(4).unwrap().name, "JLC04161H-7628");
    assert_eq!(jlc.default_construction(6).unwrap().name, "JLC06161H-3313");
    assert!(jlc.default_construction(8).is_none());
    for count in [4, 6] {
        assert_eq!(
            jlc.constructions
                .iter()
                .filter(|b| b.layer_count() == count)
                .count(),
            18
        );
    }
    let four = stackup(&jlc, "JLC04161H-7628");
    assert_eq!(four.totals().total, Nm(1_586_200));
    let sheets: Vec<_> = four
        .dielectrics()
        .map(|d| (d.material.as_deref(), d.thickness.unwrap().0, d.dk))
        .collect();
    assert_eq!(
        sheets,
        [
            (Some("7628"), 210_400, Some(4400)),
            (Some("FR-4"), 1_065_000, Some(4600)),
            (Some("7628"), 210_400, Some(4400))
        ]
    );
    let asymmetric = stackup(&jlc, "JLC06161H-3313C");
    let plies: Vec<_> = asymmetric
        .dielectrics()
        .filter(|d| d.material.as_deref() == Some("1080"))
        .map(|d| d.thickness.unwrap().0)
        .collect();
    assert_eq!(plies, [76_400, 76_400, 76_400, 78_400]);
    assert!(!asymmetric.info().symmetric);
}

#[test]
fn pcbway_numbers_are_scoped_to_the_layer_count_and_retain_combined_pressed_thickness() {
    let pcbway = shipped_named("PCBWay").unwrap();
    assert_eq!(pcbway.constructions.len(), 48);
    for (count, numbers) in [
        (4, [1..=7, 44..=57].as_slice()),
        (6, [1..=4, 24..=33].as_slice()),
        (8, [1..=4, 19..=26].as_slice()),
    ] {
        assert_eq!(
            pcbway.default_construction(count).unwrap().name,
            format!("PCBWay {count}-Layer #1")
        );
        for number in numbers.iter().cloned().flatten() {
            let name = format!("PCBWay {count}-Layer #{number}");
            let build = pcbway.construction(&name).unwrap();
            assert_eq!(build.layer_count(), count);
            assert!(
                build
                    .notes
                    .as_ref()
                    .unwrap()
                    .contains("inner copper coverage")
            );
        }
    }
    let four = stackup(&pcbway, "PCBWay 4-Layer #45");
    let gaps: Vec<_> = four.dielectrics().collect();
    assert_eq!(gaps[0].name.as_deref(), Some("1080 + 7628"));
    assert_eq!(gaps[2].name.as_deref(), Some("7628 + 1080"));
    assert_eq!(gaps[0].thickness, Some(Nm(240_300)));
    assert_eq!(gaps[0].dk, None);
    assert_eq!(gaps[1].dk, Some(4600));
    assert!(four.info().symmetric);
    assert_eq!(four.totals().total, Nm(1_520_600));
    let first = stackup(&pcbway, "PCBWay 4-Layer #1");
    assert_eq!(first.dielectrics().next().unwrap().dk, Some(4740));
    let middle = stackup(&pcbway, "PCBWay 6-Layer #30");
    let gap = middle.dielectrics().nth(2).unwrap();
    assert_eq!(gap.material.as_deref(), Some("2116 + 1080 + 2116"));
    assert_eq!(gap.thickness, Some(Nm(271_000)));
    for (name, plies) in [
        ("PCBWay 6-Layer #31", "3313 + 7628"),
        ("PCBWay 6-Layer #33", "3313 + 2116"),
    ] {
        let board = stackup(&pcbway, name);
        assert_eq!(
            board.dielectrics().nth(2).unwrap().name.as_deref(),
            Some(plies)
        );
        assert!(
            pcbway
                .construction(name)
                .unwrap()
                .notes
                .as_deref()
                .unwrap()
                .contains("asymmetric")
        );
    }
    let eight = stackup(&pcbway, "PCBWay 8-Layer #26");
    assert_eq!(eight.totals().total, Nm(1_549_000));
}

#[test]
fn osh_park_prototype_materials_rules_and_files_match_the_fabrication_services() {
    let osh = shipped_named("osh park").unwrap();
    assert_eq!(osh.constructions.len(), 3);
    for (count, sum) in [(2, 1_595_200), (4, 1_511_800), (6, 1_506_600)] {
        let name = format!("{count} Layer Prototype");
        assert_eq!(osh.default_construction(count).unwrap().name, name);
        let board = stackup(&osh, &name);
        assert_eq!(board.totals().total, Nm(sum));
    }
    let two = stackup(&osh, "2 Layer Prototype");
    let core = two.dielectrics().next().unwrap();
    assert_eq!(
        (core.dk, core.test_frequency),
        (Some(4500), Some(10_000_000))
    );
    let six = stackup(&osh, "6 Layer Prototype");
    let sheets: Vec<_> = six.dielectrics().collect();
    assert_eq!(
        (sheets[0].dk, sheets[0].df, sheets[0].test_frequency),
        (Some(3230), Some(11000), Some(1_000_000_000))
    );
    assert_eq!((sheets[1].dk, sheets[1].df), (Some(3660), Some(9000)));
    assert_eq!(
        (sheets[2].dk, sheets[2].df, sheets[2].test_frequency),
        (None, None, None)
    );
    let r = osh.rules;
    assert_eq!(
        (r.track_width, r.clearance, r.annular_ring),
        (Nm(152_400), Nm(152_400), Nm(127_000))
    );
    assert_eq!(
        (r.via_hole, r.via_diameter, r.hole_min, r.hole_to_hole),
        (Nm(254_000), Nm(508_000), Nm(254_000), Nm(127_000))
    );
    assert_eq!(r.edge_clearance, Nm(381_000));
    assert_eq!(
        (r.clearance_hole.via, r.clearance_hole.pad),
        (Nm(254_000), Nm(254_000))
    );
    assert!(r.inner.is_none());
    let generic = shipped_named("Generic").unwrap();
    assert_eq!(
        osh.gerber,
        reda_model::GerberSettings {
            slot_command: reda_model::SlotCommand::G85,
            ..Default::default()
        }
    );
    assert_eq!(osh.bom, generic.bom);
    assert_eq!(osh.cpl, generic.cpl);
    assert!(!osh.zip_assembly);
    assert_eq!(osh.stackup_suggestions.copper, [Nm(35_600)]);
    assert_eq!(osh.stackup_suggestions.finish, ["ENIG"]);
    assert_eq!(osh.stackup_suggestions.mask_color, ["Purple"]);
}

#[test]
fn shipped_presets_state_their_plated_hole_isolation_without_layer_overrides() {
    for (name, via, pad) in [
        ("Generic", 250_000, 250_000),
        ("JLCPCB", 200_000, 300_000),
        ("PCBWay", 200_000, 200_000),
        ("OSH Park", 254_000, 254_000),
    ] {
        let rules = shipped_named(name).unwrap().rules;
        assert_eq!(
            (rules.clearance_hole.via, rules.clearance_hole.pad),
            (Nm(via), Nm(pad)),
            "{name}"
        );
        assert!(rules.inner.is_none());
    }
}

#[test]
fn every_shipped_construction_roundtrips_and_matches_the_published_physical_checks() {
    use std::fmt::Write as _;
    let mut totals = String::from("Fab Preset\tConstruction\tCopper + dielectric (nm)\n");
    for source in reda_output::shipped() {
        // The .redafab and Project copy readers validate the same Construction list.
        let preset: FabPreset =
            serde_json::from_value(serde_json::to_value(&source).unwrap()).unwrap();
        assert_eq!(source, preset);
        for build in &preset.constructions {
            assert!(build.via_types.is_empty());
            assert_eq!(build.thickness, Nm(1_600_000));
            let board = stackup(&preset, &build.name);
            assert!(build.matches_stackup(&board));
            let info = board.info();
            let sum = info.totals.total.0;
            assert!(
                (1_440_000..=1_760_000).contains(&sum),
                "{} {sum}",
                build.name
            );
            assert!(!info.thickness_warning, "{}", build.name);
            assert_eq!(
                info.symmetric,
                build.name != "JLC06161H-3313C",
                "{}",
                build.name
            );
            assert!(info.totals.remainder.is_none(), "{}", build.name);
            writeln!(totals, "{}\t{}\t{sum}", preset.name, build.name).unwrap();
        }
    }
    assert_eq!(totals, include_str!("golden/construction-thicknesses.tsv"));
}

#[test]
fn shipped_fab_silkscreen_limits_follow_each_standard_service() {
    for (name, width, height) in [
        ("Generic", 150_000, Some(1_000_000)),
        ("JLCPCB", 150_000, Some(1_000_000)),
        ("PCBWay", 150_000, Some(800_000)),
        ("OSH Park", 127_000, None),
    ] {
        let rules = serde_json::to_value(shipped_named(name).unwrap().rules).unwrap();
        assert_eq!(rules["silk_width"].as_i64(), Some(width), "{name}");
        assert_eq!(rules["silk_text_height"].as_i64(), height, "{name}");
    }
}
