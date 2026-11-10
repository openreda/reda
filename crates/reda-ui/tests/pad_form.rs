//! The shared Pad form (library.md#pad-model): what each edit of the Footprint editor's
//! Pad rows and Free Pad Properties asks the engine for, and ratio entry by length.

use reda_ui::engine::model::geom::{Nm, Point};
use reda_ui::engine::model::{Corner, Mask, MaskSide, Pad, PadKind, PadShape, Ratio, Side};
use reda_ui::pad_form::{MaskChoice, PadEdit, PasteChoice, parse_ratio, patch};
use serde_json::{Value, json};

fn pad() -> Pad {
    serde_json::from_value(json!({
        "uuid": "00000000-0000-4000-8000-0000000000a1",
        "number": "1",
        "kind": "smd",
        "shape": "rounded_rect",
        "at": {"x": 0, "y": 0},
        "rotation": 0,
        "width": 2_000_000,
        "height": 1_000_000,
        "corner_ratio": 25000,
    }))
    .unwrap()
}

fn fields(pad: &Pad, edit: PadEdit) -> Value {
    Value::Object(patch(pad, &edit).expect("the edit changes something"))
}

#[test]
fn a_ratio_is_typed_as_a_percentage_or_as_a_length_at_the_pads_size() {
    let (w, h) = (Nm(2_000_000), Nm(1_000_000));
    assert_eq!(parse_ratio("25%", w, h), Some(Ratio::percent(25)));
    assert_eq!(parse_ratio("12.5", w, h), Some(Ratio(12_500)));
    // Half the shorter side is 0.5 mm: 0.1 mm is 20%, and more than it is fully round.
    assert_eq!(parse_ratio("0.1mm", w, h), Some(Ratio::percent(20)));
    assert_eq!(parse_ratio("1mm", w, h), Some(Ratio::FULL));
    assert_eq!(parse_ratio("rounded", w, h), None);
    let p = pad();
    assert_eq!(
        fields(
            &p,
            PadEdit::CornerRatio(parse_ratio("0.25mm", p.width, p.height).unwrap())
        ),
        json!({"corner_ratio": 50000})
    );
}

#[test]
fn corners_side_offset_and_hole_edits_name_the_whole_new_value() {
    let p = pad();
    assert_eq!(
        fields(&p, PadEdit::Corner(Corner::TopLeft, false)),
        json!({"corners": ["top_right", "bottom_left", "bottom_right"]})
    );
    let mut three = p.clone();
    three.corners =
        Some(serde_json::from_value(json!(["top_right", "bottom_left", "bottom_right"])).unwrap());
    // All four again is the default: the field clears.
    assert_eq!(
        fields(&three, PadEdit::Corner(Corner::TopLeft, true)),
        json!({"corners": null})
    );
    assert_eq!(
        fields(&p, PadEdit::OffsetX(Nm(100_000))),
        json!({"offset": {"x": 100_000, "y": 0}})
    );
    // The mask override follows the copper to the bottom.
    let mut masked = p.clone();
    masked.mask = Some(Mask {
        top: Some(MaskSide::Expansion {
            expansion: Nm(20_000),
        }),
        ..Default::default()
    });
    assert_eq!(
        fields(&masked, PadEdit::Side(Side::Bottom)),
        json!({"side": "bottom", "mask": {"bottom": {"expansion": 20_000}}})
    );
    // Turning it through-hole gives it a hole no wider than the Pad.
    assert_eq!(
        fields(&p, PadEdit::Kind(PadKind::PlatedThroughHole)),
        json!({"kind": "plated_through_hole", "hole": {"width": 800_000, "height": 800_000}})
    );
    let mut th = p.clone();
    th.kind = PadKind::PlatedThroughHole;
    th.hole = Some(serde_json::from_value(json!({"width": 800_000, "height": 800_000})).unwrap());
    assert_eq!(
        fields(
            &th,
            PadEdit::HoleRotation(reda_ui::engine::model::geom::Millideg(90_000))
        ),
        json!({"hole": {"width": 800_000, "height": 800_000, "rotation": 90_000}})
    );
    assert_eq!(
        fields(&th, PadEdit::Kind(PadKind::Smd)),
        json!({"kind": "smd", "hole": null})
    );
}

#[test]
fn mask_paste_and_zone_connection_edits_inherit_when_cleared() {
    let p = pad();
    assert_eq!(
        fields(&p, PadEdit::Mask(Side::Top, MaskChoice::Tent)),
        json!({"mask": {"top": "tent"}})
    );
    assert_eq!(
        fields(&p, PadEdit::Paste(PasteChoice::Percent(-10))),
        json!({"paste": {"percent": -10}})
    );
    assert_eq!(
        fields(&p, PadEdit::Paste(PasteChoice::None)),
        json!({"paste": "none"})
    );
    let mut solid = p.clone();
    solid.zone_connection = Some(serde_json::from_value(json!({"style": "solid"})).unwrap());
    assert_eq!(
        fields(&solid, PadEdit::ZoneStyle(None)),
        json!({"zone_connection": null})
    );
    assert_eq!(
        fields(&p, PadEdit::ZoneSpokes(Some(2))),
        json!({"zone_connection": {"spokes": 2}})
    );
    // Nothing changes, nothing runs.
    assert!(patch(&p, &PadEdit::Shape(PadShape::RoundedRect)).is_none());
    assert!(patch(&p, &PadEdit::OffsetY(Nm(0))).is_none());
    let _ = Point::default();
}
