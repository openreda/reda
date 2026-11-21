//! Card edges: the gold fingers of M.2 and PCI Express add-in cards, the board's own
//! copper rather than a part that is bought and placed, so out of the BOM and CPL. The
//! built-in Board Templates place them on their finger edge.
//!
//! Every finger is an SMD Edge Connector Pad with no paste, odd or B-side fingers on top
//! and even or A-side ones on the bottom. The origin is on the finger edge, not at the
//! body centre, and the Courtyard is drawn on top only, so a component Keepout on the
//! bottom of the card doesn't flag its own fingers.
//!
//! The numbers are the v0.2.5 catalogue's (`docs/spec/v0.2.5/catalogue.md`, New starter
//! Parts), from the card-edge research on open hardware.

use serde_json::{Value, json};

use crate::passives::fields;
use crate::script::{Kind, PartSpec, Script, left, mil, mm, rect, right};

/// The height of a copper-only Footprint: 1 oz copper, 0.035 mm.
const COPPER: f64 = 0.035;

/// The Courtyard's margin around the fingers.
const COURTYARD: f64 = 0.25;

pub fn build(s: &mut Script) {
    for (key, removed) in [
        ("A", &[(8, 15)][..]),
        ("B", &[(12, 19)]),
        ("E", &[(24, 31)]),
        ("M", &[(59, 66)]),
        ("AE", &[(8, 15), (24, 31)]),
        ("BM", &[(12, 19), (59, 66)]),
    ] {
        m2(s, key, removed);
    }
    for (lanes, per_side, prsnt2) in [(1, 18, 17), (4, 32, 31), (8, 49, 48), (16, 82, 81)] {
        pcie(s, lanes, per_side, prsnt2);
    }
}

/// One finger: an SMD Edge Connector Pad `width` wide from `near` to `far` mm off the
/// edge, on top or below.
fn finger(number: &str, x: f64, width: f64, (near, far): (f64, f64), top: bool) -> Value {
    let mut pad = json!({
        "number": number,
        "kind": "smd",
        "role": "edge_connector",
        "shape": "rect",
        "at": {"x": mm(x), "y": mm((near + far) / 2.0)},
        "width": mm(width),
        "height": mm(far - near),
        "paste": "none",
    });
    if !top {
        pad["side"] = json!("bottom");
    }
    pad
}

/// The Assembly outline over the fingers from `x0` to `x1` and the edge out to `depth`,
/// the top-only Courtyard around it, starting at the edge, and the Designator above.
fn finish_card_edge(s: &mut Script, fp: &str, (x0, x1): (f64, f64), depth: f64) -> String {
    s.fp_draw(
        fp,
        "assembly_top",
        rect(mm(x0), 0, mm(x1), mm(depth)),
        mm(0.1),
    );
    let top = mm(depth + COURTYARD);
    let courtyard = rect(mm(x0 - COURTYARD), 0, mm(x1 + COURTYARD), top);
    s.fp_draw(fp, "courtyard_top", courtyard, mm(0.05));
    s.finish_footprint(fp, top)
}

/// A card-edge Symbol: `rows` rows 100 mil apart, top down, each with an optional Pin on
/// the left and on the right, named by its number.
fn card_edge_symbol(
    s: &mut Script,
    name: &str,
    description: &str,
    rows: &[(Option<String>, Option<String>)],
) -> String {
    let mut sym = s.symbol(name, "J", description);
    sym.names = false;
    let count = rows.len() as i64;
    for (i, (l, r)) in rows.iter().enumerate() {
        let y = ((count - 1) - 2 * i as i64) * 50;
        if let Some(n) = l {
            s.pin(&sym, left(n, n, "passive", -200, y));
        }
        if let Some(n) = r {
            s.pin(&sym, right(n, n, "passive", 200, y));
        }
    }
    let half = count * 50;
    s.draw(
        &sym,
        rect(mil(-100), mil(-half), mil(100), mil(half)),
        false,
    );
    s.finish_symbol(sym)
}

/// `CardEdge_M.2_Key_<key>`: 75 positions with Pin n at x = 9.25 − (n − 1) × 0.25 mm,
/// +x toward Pin 1; odd Pins on top from 0.50 to 2.00 mm, even ones below from 0.50 to
/// 2.50 mm, 0.35 mm wide. The key removes the Pins in `removed`.
fn m2(s: &mut Script, key: &str, removed: &[(u32, u32)]) {
    let name = format!("CardEdge_M.2_Key_{key}");
    let keyed = |n: u32| removed.iter().any(|&(a, b)| (a..=b).contains(&n));
    let numbers: Vec<u32> = (1..=75).filter(|&n| !keyed(n)).collect();
    let pins = numbers.len();
    let pin = |n: u32| (!keyed(n) && n <= 75).then(|| n.to_string());
    // Odd Pins on the left, even on the right, a row per pair, so the key shows as a gap.
    let rows: Vec<_> = (0..38).map(|i| (pin(2 * i + 1), pin(2 * i + 2))).collect();
    let keys = key
        .chars()
        .map(String::from)
        .collect::<Vec<_>>()
        .join(" and ");
    let description = format!("M.2 card edge, key {keys}, {pins} fingers");
    let sym = card_edge_symbol(s, &name, &description, &rows);

    let fp = s.footprint(
        &name,
        &format!(
            "{description}: Edge Connector Pads 0.35 mm wide on a 0.5 mm pitch per side, set \
             back 0.50 mm from the edge, the common open-hardware value; the origin is the \
             centre of the finger edge"
        ),
        COPPER,
    );
    for n in numbers {
        let x = 9.25 - f64::from(n - 1) * 0.25;
        let top = n % 2 == 1;
        let span = (0.5, if top { 2.0 } else { 2.5 });
        s.pad(&fp, finger(&n.to_string(), x, 0.35, span, top));
    }
    let fp = finish_card_edge(s, &fp, (-9.425, 9.425), 2.5);
    s.part(PartSpec {
        name: &name,
        symbol: Some(&sym),
        footprints: vec![fp],
        fields: fields(
            &format!("M.2 Key {key}"),
            &description,
            "J connector card edge fingers M.2 NGFF",
        ),
        pin_map: None,
        kind: Kind::Unassembled,
    });
}

/// `CardEdge_PCIe_x<lanes>`: `per_side` fingers a side, Pins 1 to 11 at x = −11.50 +
/// (n − 1) mm and 12 up at 1.50 + (n − 12) mm from the key centreline, +x away from the
/// bracket; B on top and A below, 0.70 mm wide from 1.40 to 5.60 mm, clear of a 1.30 mm
/// bevel. The present-detect Pins A1 and B`prsnt2` are short, from 2.40 mm.
fn pcie(s: &mut Script, lanes: u32, per_side: u32, prsnt2: u32) {
    let name = format!("CardEdge_PCIe_x{lanes}");
    let x = |n: u32| match n {
        1..=11 => -11.5 + f64::from(n - 1),
        _ => 1.5 + f64::from(n - 12),
    };
    // A on the left and B on the right, a row per Pin number, a gap row at the key.
    let mut rows = vec![];
    for n in 1..=per_side {
        if n == 12 {
            rows.push((None, None));
        }
        rows.push((Some(format!("A{n}")), Some(format!("B{n}"))));
    }
    let description = format!("PCI Express x{lanes} card edge, {per_side} fingers a side");
    let sym = card_edge_symbol(s, &name, &description, &rows);

    let fp = s.footprint(
        &name,
        &format!(
            "{description}: Edge Connector Pads 0.70 mm wide on a 1.0 mm pitch, B side on \
             top and A side below, from 1.40 mm off the edge (clear of a 1.30 mm bevel; \
             present-detect A1 and B{prsnt2} from 2.40 mm), the open-hardware values; the \
             origin is the key centreline on the finger edge"
        ),
        COPPER,
    );
    for n in 1..=per_side {
        for (side, top) in [("B", true), ("A", false)] {
            let short = (side, n) == ("A", 1) || (side, n) == ("B", prsnt2);
            let span = (if short { 2.4 } else { 1.4 }, 5.6);
            s.pad(&fp, finger(&format!("{side}{n}"), x(n), 0.7, span, top));
        }
    }
    let fp = finish_card_edge(s, &fp, (x(1) - 0.35, x(per_side) + 0.35), 5.6);
    s.part(PartSpec {
        name: &name,
        symbol: Some(&sym),
        footprints: vec![fp],
        fields: fields(
            &format!("PCIe x{lanes}"),
            &description,
            "J connector card edge fingers PCIe PCI Express",
        ),
        pin_map: None,
        kind: Kind::Unassembled,
    });
}
