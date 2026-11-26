//! Board Templates: the load checks a `.redaboard` runs and the one evaluation function
//! preview and creation share (`template-format.md`, `commands.md`).

use std::collections::BTreeMap;

use reda_library::board_template::{
    BoardTemplateChoice, Frame, ProblemObject, ResolvedValue, TemplateValue, check, combinations,
    evaluate, length_ranges,
};
use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{BoardTemplate, FootprintCopy, Side, TemplateParameter, TemplatePart, Uuid};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

fn id(n: u128) -> Uuid {
    Uuid::v4_from_bits(n)
}

fn s(n: u128) -> String {
    id(n).to_string()
}

fn rows<T: DeserializeOwned>(value: Value) -> Vec<T> {
    serde_json::from_value(value).unwrap()
}

fn mm(n: i64) -> Nm {
    Nm::from_mm(n)
}

/// A Custom-rectangle-like template: width, height and corner radius lengths.
fn rectangle() -> BoardTemplate {
    let mut t = BoardTemplate::new(id(1), "Rect");
    t.parameters = rows(json!([
        {"kind":"length","name":"width","label":"Width","default":"100","min":"1","max":"1000","unit":"mm"},
        {"kind":"length","name":"height","label":"Height","default":"80","min":"1","max":"1000","unit":"mm"},
        {"kind":"length","name":"corner_radius","label":"Corner radius","default":"0","min":"0","max":"500","unit":"mm"}
    ]));
    t.outline = rows(json!([{"uuid": s(2), "locked": true, "vertices": [
        {"x":"corner_radius","y":0},
        {"x":"width - corner_radius","y":0,"sweep":"90"},
        {"x":"width","y":"corner_radius"},
        {"x":"width","y":"height - corner_radius","sweep":"90"},
        {"x":"width - corner_radius","y":"height"},
        {"x":"corner_radius","y":"height","sweep":"90"},
        {"x":0,"y":"height - corner_radius"},
        {"x":0,"y":"corner_radius","sweep":"90"}
    ]}]));
    t
}

fn at(template: &BoardTemplate, values: Value) -> Result<Frame, String> {
    let values: BTreeMap<String, TemplateValue> = serde_json::from_value(values).unwrap();
    evaluate(
        template,
        &BoardTemplateChoice {
            element: template.uuid,
            values,
            exclude: vec![],
        },
    )
    .map_err(|e| format!("{} {}", e.pointer, e.message))
}

fn excluding(template: &BoardTemplate, exclude: &[u128]) -> Result<Frame, String> {
    evaluate(
        template,
        &BoardTemplateChoice {
            element: template.uuid,
            values: BTreeMap::new(),
            exclude: exclude.iter().map(|n| id(*n)).collect(),
        },
    )
    .map_err(|e| format!("{} {}", e.pointer, e.message))
}

fn points(frame: &Frame) -> Vec<(i64, i64, i64)> {
    frame
        .outline
        .vertices
        .iter()
        .map(|v| (v.x.0, v.y.0, v.sweep.0))
        .collect()
}

#[test]
fn a_rectangle_evaluates_exactly_and_drops_the_zero_length_corner_edges() {
    let t = rectangle();
    assert_eq!(check(&t), Vec::<String>::new());
    let frame = at(&t, json!({})).unwrap();
    let (w, h) = (mm(100).0, mm(80).0);
    assert_eq!(points(&frame), [(0, 0, 0), (w, 0, 0), (w, h, 0), (0, h, 0)]);
    assert!(frame.outline.locked);
    assert!(frame.is_valid(), "{:?}", frame.problems);
    assert_eq!(frame.values["width"], ResolvedValue::Length(mm(100)));
    assert_eq!(frame.suggested_layers, 2);

    // Lengths as integer nm or unit strings; a third of a millimetre rounds only at the end.
    let frame = at(
        &t,
        json!({"width": 50_000_000, "height": "1in", "corner_radius": "5mm"}),
    )
    .unwrap();
    let (w, h, r) = (mm(50).0, 25_400_000, mm(5).0);
    assert_eq!(
        points(&frame),
        [
            (r, 0, 0),
            (w - r, 0, 90_000),
            (w, r, 0),
            (w, h - r, 90_000),
            (w - r, h, 0),
            (r, h, 90_000),
            (0, h - r, 0),
            (0, r, 90_000)
        ]
    );
    assert!(frame.is_valid(), "{:?}", frame.problems);
    let frame = at(&t, json!({"width": "100/3"})).unwrap();
    assert_eq!(frame.outline.vertices[1].x, Nm(33_333_333));
}

#[test]
fn a_radius_over_half_a_side_gives_a_located_outline_problem() {
    let t = rectangle();
    let frame = at(&t, json!({"width": 10_000_000, "corner_radius": 6_000_000})).unwrap();
    assert!(!frame.is_valid());
    let problem = &frame.problems[0];
    assert_eq!(problem.kind, ProblemObject::Outline);
    assert_eq!(problem.object, id(2));
    assert!(problem.at.is_some(), "{problem:?}");
    // Exactly half on every side is a circle, which is fine.
    let frame = at(
        &t,
        json!({"width": "20", "height": "20", "corner_radius": "10"}),
    )
    .unwrap();
    assert!(frame.is_valid(), "{:?}", frame.problems);
    assert_eq!(frame.outline.vertices.len(), 4);
}

#[test]
fn bad_values_are_refused_with_a_pointer() {
    let t = rectangle();
    let err = at(&t, json!({"width": "2000"})).unwrap_err();
    assert!(err.starts_with("/values/width "), "{err}");
    assert!(
        err.contains("out of range: from 1 to 1000 mm, not 2000 mm"),
        "{err}"
    );
    let err = at(&t, json!({"depth": "10"})).unwrap_err();
    assert!(err.starts_with("/values/depth "), "{err}");
    assert!(err.contains("unknown parameter"), "{err}");
    let err = at(&t, json!({"width": "wide"})).unwrap_err();
    assert!(err.starts_with("/values/width "), "{err}");
}

/// A card with a lane-count choice: each option binds `card_width`, one connector row per
/// option, a Keepout that protects a hole, a drawing group and a Dimension that follows
/// the hole.
fn card() -> BoardTemplate {
    let mut t = BoardTemplate::new(id(10), "Card");
    t.suggested_layers = 4;
    t.parameters = rows(json!([
        {"kind":"choice","name":"lanes","label":"Lanes","default":"x4","options":[
            {"key":"x1","label":"x1","bind":{"card_width":"40"}},
            {"key":"x4","label":"x4","bind":{"card_width":"60"},"suggested_layers":6}
        ]},
        {"kind":"choice","name":"bracket","label":"Bracket","when":{"lanes":["x4"]},"options":[
            {"key":"none","label":"None","bind":{}},
            {"key":"full","label":"Full","bind":{}}
        ]},
        {"kind":"length","name":"length","label":"Length","default":"100","min":"70","max":"170","unit":"mm"}
    ]));
    t.outline = rows(json!([{"uuid": s(11), "vertices": [
        {"x":0,"y":0,"plated":true},
        {"x":"length","y":0},
        {"x":"length","y":"card_width"},
        {"x":0,"y":"card_width"}
    ]}]));
    t.cutouts = rows(
        json!([{"uuid": s(12), "when": {"bracket": ["full"]}, "vertices": [
            {"x":"10","y":"10"},{"x":"20","y":"10"},{"x":"20","y":"20"},{"x":"10","y":"20"}
        ]}]),
    );
    t.parts = vec![part_entry()];
    t.footprint_copies = vec![footprint_copy()];
    t.connectors = rows(json!([
        {"uuid": s(20), "part": s(30), "footprint": s(31), "at": {"x":"5","y":"card_width / 2"},
         "rotation":"90", "side":"bottom", "locked": true, "when": {"lanes": ["x1"]}},
        {"uuid": s(21), "part": s(30), "footprint": s(31), "at": {"x":"5","y":"card_width / 2"},
         "rotation":0, "side":"top", "when": {"lanes": ["x4"]}}
    ]));
    t.holes = rows(json!([
        {"uuid": s(22), "at": {"x":"length - 5","y":"5"}, "locked": true,
         "pad": {"kind":"non_plated","shape":"circle","width":"3.2","height":"3.2",
                 "hole":{"width":"3.2","height":"3.2"}}}
    ]));
    t.keepouts = rows(json!([
        {"uuid": s(23), "layer":"all_copper", "restrict":["tracks","vias"], "shape":"polygon",
         "protects": s(22), "vertices": [
            {"x":"length - 9","y":"1"},{"x":"length - 1","y":"1"},
            {"x":"length - 1","y":"9"},{"x":"length - 9","y":"9"}
        ]}
    ]));
    t.drawing_groups = rows(json!([
        {"uuid": s(24), "name": "Key dimensions", "purpose": "dimensions"}
    ]));
    t.drawings = rows(json!([
        {"uuid": s(25), "group": s(24), "kind": "dimension",
         "a": {"follow": {"object": s(11), "vertex": 0}}, "b": {"follow": {"object": s(22)}},
         "offset": "-3"},
        {"uuid": s(26), "group": s(24), "kind": "line", "width": "0.1",
         "from": {"x":0,"y":"-2"}, "to": {"x":"length","y":"-2"}}
    ]));
    t
}

fn part_entry() -> TemplatePart {
    TemplatePart {
        uuid: id(30),
        hash: "0123".into(),
        part: serde_json::from_value(json!({
            "uuid": s(40), "name": "Header_1x02", "component_type": "standard",
            "in_bom": true, "in_cpl": true, "footprints": [s(41)],
            "fields": {"value": "Header"}, "pin_map": {}
        }))
        .unwrap(),
        symbol: None,
        footprints: vec![id(31)],
    }
}

/// Two 1.7 mm square Pads 2.54 mm apart along x.
fn footprint_copy() -> FootprintCopy {
    let pad = |n: u128, number: &str, x: i64| {
        json!({"uuid": s(n), "number": number, "kind": "plated_through_hole", "shape": "rect",
               "at": {"x": x, "y": 0}, "rotation": 0, "width": 1_700_000, "height": 1_700_000,
               "hole": {"width": 1_000_000, "height": 1_000_000}})
    };
    serde_json::from_value(json!({
        "uuid": s(31), "hash": "4567",
        "footprint": {"uuid": s(41), "name": "Header", "description": "",
                      "pads": [pad(42, "1", 0), pad(43, "2", 2_540_000)],
                      "graphics": [], "texts": []}
    }))
    .unwrap()
}

#[test]
fn a_choice_binds_names_picks_rows_and_overrides_the_layer_count() {
    let t = card();
    assert_eq!(check(&t), Vec::<String>::new());
    let frame = at(&t, json!({})).unwrap();
    assert_eq!(frame.values["lanes"], ResolvedValue::Choice("x4".into()));
    assert_eq!(
        frame.values["bracket"],
        ResolvedValue::Choice("none".into())
    );
    assert_eq!(frame.suggested_layers, 6);
    assert_eq!(frame.outline.vertices[2].y, mm(60));
    assert!(frame.outline.vertices[0].plated);
    assert!(!frame.outline.vertices[1].plated);
    assert_eq!(
        frame.connectors.iter().map(|c| c.uuid).collect::<Vec<_>>(),
        [id(21)]
    );
    assert!(frame.cutouts.is_empty());

    let frame = at(&t, json!({"lanes": "x1"})).unwrap();
    assert_eq!(frame.suggested_layers, 4);
    assert_eq!(frame.outline.vertices[2].y, mm(40));
    let c = &frame.connectors[0];
    assert_eq!(c.uuid, id(20));
    assert_eq!(
        (c.at, c.rotation, c.side),
        (Point::new(mm(5), mm(20)), Millideg(90_000), Side::Bottom)
    );
    // Bottom side flips x, then the quarter turn takes pad 2 to (+0, -2.54) from the origin.
    assert_eq!(c.pads[1].number, "2");
    assert_eq!(c.pads[1].at, Point::new(mm(5), mm(20) - Nm(2_540_000)));
    assert_eq!(c.pads[0].hole.unwrap().width, mm(1));
    assert_eq!(c.pads[0].outline.len(), 4);

    let err = at(&t, json!({"lanes": "x8"})).unwrap_err();
    assert!(
        err.starts_with("/values/lanes ") && err.contains("x1, x4"),
        "{err}"
    );
}

#[test]
fn a_value_for_a_hidden_parameter_is_accepted_and_ignored() {
    let t = card();
    let frame = at(&t, json!({"lanes": "x1", "bracket": "full"})).unwrap();
    // Hidden, `bracket` gates at its default but is no resolved value of the frame.
    assert!(!frame.values.contains_key("bracket"), "{:?}", frame.values);
    assert_eq!(frame.values.keys().collect::<Vec<_>>(), ["lanes", "length"]);
    assert!(frame.cutouts.is_empty());
    let frame = at(&t, json!({"lanes": "x4", "bracket": "full"})).unwrap();
    assert_eq!(frame.cutouts.len(), 1);
}

#[test]
fn holes_keepouts_and_dimensions_are_placed_and_follow_their_objects() {
    let frame = at(&card(), json!({})).unwrap();
    let hole = &frame.holes[0];
    assert_eq!(hole.at, Point::new(mm(95), mm(5)));
    let pad = hole.free_pad.as_ref().unwrap();
    assert_eq!(
        (pad.uuid, pad.width, pad.at, pad.locked),
        (id(22), Nm(3_200_000), hole.at, true)
    );
    assert_eq!(hole.pads[0].hole.unwrap().width, Nm(3_200_000));
    assert_eq!(
        frame.keepouts[0].vertices[0].point(),
        Point::new(mm(91), mm(1))
    );
    let group = &frame.drawing_groups[0];
    assert_eq!(group.name, "Key dimensions");
    let dim = &group.drawings[0];
    assert_eq!(dim.a.unwrap().point, Point::new(Nm(0), Nm(0)));
    assert_eq!(dim.b.unwrap().point, hole.at);
    assert_eq!(dim.offset, Some(mm(-3)));
}

#[test]
fn excluding_a_hole_takes_its_keepout_and_dimensions_with_it() {
    let t = card();
    let frame = excluding(&t, &[22]).unwrap();
    assert!(frame.holes[0].excluded);
    assert!(frame.keepouts[0].excluded);
    let drawings = &frame.drawing_groups[0].drawings;
    assert!(drawings[0].excluded, "the Dimension follows the hole");
    assert!(!drawings[1].excluded);

    let frame = excluding(&t, &[24]).unwrap();
    assert!(frame.drawing_groups[0].excluded);
    assert!(frame.drawing_groups[0].drawings.iter().all(|d| d.excluded));

    // A row a gate removes may be named, so the dialog keeps its ticks.
    let frame = excluding(&t, &[20]).unwrap();
    assert!(
        frame
            .connectors
            .iter()
            .all(|c| c.uuid == id(21) && !c.excluded)
    );

    for (n, what) in [
        (23, "a Keepout"),
        (11, "the Board Outline"),
        (25, "a single drawing"),
    ] {
        let err = excluding(&t, &[n]).unwrap_err();
        assert!(
            err.starts_with("/exclude/0 ") && err.contains(what),
            "{err}"
        );
    }
    let err = excluding(&t, &[99]).unwrap_err();
    assert!(
        err.contains("not a connector, hole or drawing group"),
        "{err}"
    );
}

#[test]
fn a_keepout_with_nothing_to_restrict_is_a_frame_problem() {
    let mut t = card();
    t.keepouts[0].restrict.clear();
    let frame = at(&t, json!({})).unwrap();
    assert_eq!(frame.problems.len(), 1);
    assert_eq!(frame.problems[0].kind, ProblemObject::Keepout);
    // An excluded Keepout is not checked.
    let frame = excluding(&t, &[22]).unwrap();
    assert!(frame.is_valid());
}

#[test]
fn combinations_walk_every_shown_choice_option_once() {
    // `bracket` is hidden at x1, so x1 is one combination, not one per bracket option.
    let combos = combinations(&card());
    let shown: Vec<String> = combos
        .iter()
        .map(|c| {
            c.iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect();
    assert_eq!(
        shown,
        ["lanes=x1", "bracket=none,lanes=x4", "bracket=full,lanes=x4"]
    );
    assert_eq!(combinations(&rectangle()), [BTreeMap::new()]);
}

fn failing(edit: impl FnOnce(&mut BoardTemplate)) -> String {
    let mut t = card();
    edit(&mut t);
    let problems = check(&t);
    assert!(!problems.is_empty(), "the edit should fail a load check");
    problems.join("\n")
}

fn choice(t: &mut BoardTemplate, i: usize) -> &mut reda_model::ChoiceParameter {
    match &mut t.parameters[i] {
        TemplateParameter::Choice(c) => c,
        TemplateParameter::Length(_) => panic!("not a choice"),
    }
}

#[test]
fn load_checks_refuse_bad_expressions_and_names() {
    let set = |text: &'static str| failing(move |t| t.outline[0].vertices[1].x = text.into());
    assert!(set("length + 1 +").contains("vertex 1 x"));
    assert!(set("! + 1").contains("`!` is not allowed"));
    assert!(set("max(length, 1)").contains("function"));
    assert!(set("Width + 1").contains("names `Width`, which is not a parameter"));
    assert!(
        set("length * length").contains("area"),
        "a frame expression is evaluated at defaults"
    );
    let p = failing(|t| choice(t, 0).name = "Lanes".into());
    assert!(p.contains("not a snake_case name"), "{p}");
    let p = failing(|t| {
        choice(t, 0).options[0]
            .bind
            .insert("length".into(), "1".into());
        choice(t, 0).options[1]
            .bind
            .insert("length".into(), "1".into());
    });
    assert!(p.contains("`length` is used twice"), "{p}");
}

#[test]
fn load_checks_refuse_inconsistent_choices_and_gates() {
    let p = failing(|t| {
        choice(t, 0).options[1].bind.clear();
    });
    assert!(p.contains("every option must bind the same names"), "{p}");
    let p = failing(|t| choice(t, 0).default = Some("x16".into()));
    assert!(p.contains("default `x16`"), "{p}");
    let p = failing(|t| {
        t.connectors[0].when = Some(BTreeMap::from([("length".into(), vec!["x".into()])]));
    });
    assert!(p.contains("not a choice parameter"), "{p}");
    let p = failing(|t| {
        t.connectors[0].when = Some(BTreeMap::from([("lanes".into(), vec!["x2".into()])]));
    });
    assert!(p.contains("`x2` of `lanes`"), "{p}");
    let p = failing(|t| t.suggested_layers = 40);
    assert!(p.contains("from 2 to 32"), "{p}");
    let p = failing(|t| choice(t, 0).options[0].suggested_layers = Some(1));
    assert!(p.contains("from 2 to 32"), "{p}");
}

#[test]
fn load_checks_need_exactly_one_outline_in_every_combination() {
    let p = failing(|t| {
        t.outline[0].when = Some(BTreeMap::from([("lanes".into(), vec!["x1".into()])]));
    });
    assert!(
        p.contains("0 outline rows survive at bracket=none, lanes=x4"),
        "{p}"
    );
    let p = failing(|t| {
        let mut second = t.outline[0].clone();
        second.uuid = id(99);
        t.outline.push(second);
    });
    assert!(p.contains("2 outline rows survive"), "{p}");
    // Two gated outlines that never meet are fine.
    let mut t = card();
    let mut second = t.outline[0].clone();
    second.uuid = id(99);
    t.outline[0].when = Some(BTreeMap::from([("lanes".into(), vec!["x1".into()])]));
    second.when = Some(BTreeMap::from([("lanes".into(), vec!["x4".into()])]));
    t.outline.push(second);
    assert_eq!(check(&t), Vec::<String>::new());
}

#[test]
fn load_checks_resolve_every_reference() {
    let p = failing(|t| t.connectors[0].part = id(98));
    assert!(p.contains("names part entry"), "{p}");
    let p = failing(|t| t.connectors[0].footprint = id(98));
    assert!(p.contains("names Footprint copy"), "{p}");
    let p = failing(|t| t.keepouts[0].protects = Some(id(98)));
    assert!(p.contains("not a connector or hole"), "{p}");
    let p = failing(|t| t.drawings[0].group = id(98));
    assert!(p.contains("drawing_groups does not hold"), "{p}");
    let p = failing(|t| {
        t.drawings[0]
            .a
            .as_mut()
            .unwrap()
            .follow
            .as_mut()
            .unwrap()
            .vertex = Some(4)
    });
    assert!(p.contains("vertex 4"), "{p}");
    let p = failing(|t| t.keepouts[0].vertices[0].plated = true);
    assert!(p.contains("only outline and cutout edges"), "{p}");
    let p = failing(|t| t.holes[0].part = Some(id(30)));
    assert!(p.contains("Mechanical Part row"), "{p}");
}

/// The card with its length's `min` an expression over the lane width's bound name, as
/// the PCIe card's is (#416 decision): x1 from 70, x4 from 90.
fn card_with_lane_min() -> BoardTemplate {
    let mut t = card();
    length(&mut t).min = "card_width + 30".into();
    t
}

fn length(t: &mut BoardTemplate) -> &mut reda_model::LengthParameter {
    match &mut t.parameters[2] {
        TemplateParameter::Length(l) => l,
        TemplateParameter::Choice(_) => panic!("not a length"),
    }
}

#[test]
fn a_length_bound_may_name_choice_bound_names_and_is_checked_per_combination() {
    let t = card_with_lane_min();
    assert_eq!(check(&t), Vec::<String>::new());
    // 80 mm is in range at x1 (from 70) and out of range at x4 (from 90).
    assert!(at(&t, json!({"lanes": "x1", "length": "80"})).is_ok());
    let err = at(&t, json!({"lanes": "x4", "length": "80"})).unwrap_err();
    assert!(err.starts_with("/values/length "), "{err}");
    assert!(
        err.contains("out of range: from 90 to 170 mm at lanes=x4, not 80 mm"),
        "{err}"
    );
    assert!(at(&t, json!({"lanes": "x4", "length": "90"})).is_ok());
}

#[test]
fn the_ranges_of_a_length_are_listed_per_option_of_the_choices_it_names() {
    let t = card_with_lane_min();
    let ranges = length_ranges(&t, "length").unwrap();
    let shown: Vec<(String, i64, i64)> = ranges
        .iter()
        .map(|r| (format!("{:?}", r.when), r.min.0, r.max.0))
        .collect();
    assert_eq!(
        shown,
        [
            (r#"{"lanes": ["x1"]}"#.to_owned(), mm(70).0, mm(170).0),
            (r#"{"lanes": ["x4"]}"#.to_owned(), mm(90).0, mm(170).0),
        ]
    );
    // A constant range is one range with no gate.
    let ranges = length_ranges(&rectangle(), "width").unwrap();
    assert_eq!(ranges.len(), 1);
    assert!(ranges[0].when.is_empty());
    assert_eq!((ranges[0].min, ranges[0].max), (mm(1), mm(1000)));
}

#[test]
fn load_checks_hold_a_length_bound_to_choice_bound_names_and_its_default_in_every_range() {
    let p = failing(|t| length(t).min = "length - 30".into());
    assert!(p.contains("length `length` min"), "{p}");
    assert!(p.contains("only the names choice options bind"), "{p}");
    let p = failing(|t| length(t).max = "depth".into());
    assert!(p.contains("only the names choice options bind"), "{p}");
    // From 110 at x4, above the 100 default.
    let p = failing(|t| length(t).min = "card_width + 50".into());
    assert!(
        p.contains("length `length` needs min ≤ default ≤ max at lanes=x4"),
        "{p}"
    );
    assert!(!p.contains("lanes=x1"), "{p}");
    // The default stays a plain value.
    let p = failing(|t| length(t).default = "card_width + 50".into());
    assert!(p.contains("length `length` default"), "{p}");
    assert!(p.contains("must be a plain value"), "{p}");
}
