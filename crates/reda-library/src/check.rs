//! The Library check: finds mistakes in Symbols, Footprints and Parts and reports them as
//! [`Violation`]s. It only reports and never blocks saving.

use std::collections::{BTreeMap, BTreeSet};

use reda_model::geom::Nm;
use reda_model::{
    Footprint, FootprintLayer, LibraryElement, PadKind, Part, Severity, Symbol, Uuid, Violation,
};

use crate::LibraryIndex;

/// The grid Symbol Pins must sit on: 50 mil.
pub const PIN_GRID: Nm = Nm::from_mil(50);

/// Where the check finds the elements a Part references. A Part may reference elements of
/// any Library.
pub trait Elements {
    fn symbol(&self, uuid: Uuid) -> Option<&Symbol>;
    fn footprint(&self, uuid: Uuid) -> Option<&Footprint>;
}

impl Elements for LibraryIndex {
    fn symbol(&self, uuid: Uuid) -> Option<&Symbol> {
        LibraryIndex::symbol(self, uuid)
    }

    fn footprint(&self, uuid: Uuid) -> Option<&Footprint> {
        LibraryIndex::footprint(self, uuid)
    }
}

/// Every Violation of one element, looking up what a Part references in `elements`.
pub fn check(elements: &dyn Elements, element: &LibraryElement) -> Vec<Violation> {
    match element {
        LibraryElement::Symbol(s) => check_symbol(s),
        LibraryElement::Footprint(f) => check_footprint(f),
        LibraryElement::Part(p) => check_part(elements, p),
    }
}

fn check_symbol(symbol: &Symbol) -> Vec<Violation> {
    let mut out = vec![];
    for pin in &symbol.pins {
        let on_grid = |n: Nm| n.0 % PIN_GRID.0 == 0;
        if !on_grid(pin.at.x) || !on_grid(pin.at.y) {
            out.push(
                Violation::new("off_grid_pin", Severity::Warning)
                    .arg("pin", pin.number.as_str())
                    .arg("grid", PIN_GRID)
                    .objects([symbol.uuid, pin.uuid]),
            );
        }
    }
    let mut by_number: BTreeMap<&str, Vec<Uuid>> = BTreeMap::new();
    for pin in &symbol.pins {
        by_number.entry(&pin.number).or_default().push(pin.uuid);
    }
    for (number, pins) in by_number {
        if pins.len() > 1 {
            out.push(
                Violation::new("duplicate_pin_number", Severity::Error)
                    .arg("number", number)
                    .object(symbol.uuid)
                    .objects(pins),
            );
        }
    }
    out
}

fn check_footprint(footprint: &Footprint) -> Vec<Violation> {
    let mut out = vec![];
    let courtyard = [
        FootprintLayer::CourtyardTop,
        FootprintLayer::CourtyardBottom,
    ];
    if !footprint
        .graphics
        .iter()
        .any(|g| courtyard.contains(&g.layer))
    {
        out.push(Violation::new("missing_courtyard", Severity::Warning).object(footprint.uuid));
    }
    for pad in &footprint.pads {
        if let Some(reda_model::Paste::Windows { windows }) = &pad.paste {
            let copper = pad.outline();
            for (window, w) in windows.iter().enumerate() {
                if !w.fits(&copper) {
                    out.push(
                        Violation::new("paste_window_outside_copper", Severity::Warning)
                            .arg("pad", pad.number.as_str())
                            .arg("window", window as i64)
                            .objects([footprint.uuid, pad.uuid]),
                    );
                }
            }
        }

        let outside = pad.kind == PadKind::PlatedThroughHole
            && pad
                .form()
                .annular_ring()
                .is_some_and(|ring| ring < Nm::ZERO);
        if outside {
            out.push(
                Violation::new("hole_outside_copper", Severity::Error)
                    .arg("pad", pad.number.as_str())
                    .objects([footprint.uuid, pad.uuid]),
            );
        }
    }
    out
}

fn check_part(elements: &dyn Elements, part: &Part) -> Vec<Violation> {
    let mut out = vec![];
    let broken = |kind: &str, reference: String| {
        Violation::new("broken_reference", Severity::Error)
            .arg("kind", kind)
            .arg("reference", reference)
            .object(part.uuid)
    };
    let symbol = match part.symbol {
        Some(uuid) => {
            let symbol = elements.symbol(uuid);
            if symbol.is_none() {
                out.push(broken("symbol", uuid.to_string()));
            }
            symbol
        }
        None => None,
    };
    let mut footprints = vec![];
    for &uuid in &part.footprints {
        match elements.footprint(uuid) {
            Some(footprint) => footprints.push(footprint),
            None => out.push(broken("footprint", uuid.to_string())),
        }
    }
    let Some(symbol) = symbol else {
        return out;
    };
    for over in &part.pin_map.pins {
        if !symbol.pins.iter().any(|p| p.number == over.pin) {
            out.push(broken("pin", over.pin.clone()).object(symbol.uuid));
        }
    }
    for footprint in footprints {
        check_pin_map(part, symbol, footprint, &mut out);
    }
    out
}

/// Pins that reach no Pad of `footprint`, and Pads no Pin reaches that are not marked
/// unconnected. Non-plated Pads carry no signal, so they need no Pin.
fn check_pin_map(part: &Part, symbol: &Symbol, footprint: &Footprint, out: &mut Vec<Violation>) {
    let mut mapped: BTreeSet<&str> = BTreeSet::new();
    for pin in &symbol.pins {
        let pads = part.pin_map.pads_of(&pin.number);
        if !footprint
            .pads
            .iter()
            .any(|pad| pads.contains(&pad.number.as_str()))
        {
            out.push(
                Violation::new("pin_without_pad", Severity::Error)
                    .arg("pin", pin.number.as_str())
                    .arg("footprint", footprint.name.as_str())
                    .objects([part.uuid, pin.uuid, footprint.uuid]),
            );
        }
        mapped.extend(pads);
    }
    let mut unmapped: BTreeMap<&str, Vec<Uuid>> = BTreeMap::new();
    for pad in &footprint.pads {
        let number = pad.number.as_str();
        let unconnected = part.pin_map.unconnected.iter().any(|n| n == number);
        if pad.kind != PadKind::NonPlated && !mapped.contains(number) && !unconnected {
            unmapped.entry(number).or_default().push(pad.uuid);
        }
    }
    for (number, pads) in unmapped {
        out.push(
            Violation::new("unmapped_pad", Severity::Warning)
                .arg("pad", number)
                .arg("footprint", footprint.name.as_str())
                .objects([part.uuid, footprint.uuid])
                .objects(pads),
        );
    }
}
