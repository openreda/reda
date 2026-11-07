//! The Library check: finds mistakes in Symbols, Footprints and Parts and reports them as
//! [`Violation`]s, each with the severity the user's [`LibrarySeverities`] give its check.
//! A check set to Off does not run. It only reports and never blocks saving.

use std::collections::{BTreeMap, BTreeSet};

use reda_model::footprint_copper;
use reda_model::geom::Nm;
use reda_model::{
    ComponentType, Footprint, FootprintLayer, LibraryCheck, LibraryElement, LibrarySeverities,
    PadKind, Part, Severity, Side, Symbol, Uuid, Violation,
};

use crate::LibraryIndex;

/// The grid Symbol Pins must sit on: 50 mil.
pub const PIN_GRID: Nm = Nm::from_mil(50);

/// How far a Footprint origin may sit from its body box's centre, in x and in y: 0.1 mm.
pub const CENTROID_TOLERANCE: Nm = Nm::from_um(100);

/// Where the check finds the elements a Part references, and the other Parts it compares
/// names and MPNs with. A Part may reference elements of any Library.
pub trait Elements {
    fn symbol(&self, uuid: Uuid) -> Option<&Symbol>;
    fn footprint(&self, uuid: Uuid) -> Option<&Footprint>;
    /// Every Part of the registered Libraries.
    fn parts(&self) -> Vec<&Part>;
}

impl Elements for LibraryIndex {
    fn symbol(&self, uuid: Uuid) -> Option<&Symbol> {
        LibraryIndex::symbol(self, uuid)
    }

    fn footprint(&self, uuid: Uuid) -> Option<&Footprint> {
        LibraryIndex::footprint(self, uuid)
    }

    fn parts(&self) -> Vec<&Part> {
        LibraryIndex::parts(self).map(|(_, part)| part).collect()
    }
}

/// Every Violation of one element, looking up what a Part references in `elements`, with
/// the severities in `severities`.
pub fn check(
    elements: &dyn Elements,
    element: &LibraryElement,
    severities: &LibrarySeverities,
) -> Vec<Violation> {
    let mut report = Report {
        severities,
        found: vec![],
    };
    match element {
        LibraryElement::Symbol(s) => check_symbol(elements, s, &mut report),
        LibraryElement::Footprint(f) => check_footprint(f, &mut report),
        LibraryElement::Part(p) => check_part(elements, p, &mut report),
    }
    report.found
}

/// The Violations found so far, each at its check's severity; a check set to Off adds
/// nothing.
struct Report<'a> {
    severities: &'a LibrarySeverities,
    found: Vec<Violation>,
}

impl Report<'_> {
    fn on(&self, check: LibraryCheck) -> bool {
        self.severities.get(check) != Severity::Off
    }

    /// Adds a Violation of `check`, filled in by `fill`.
    fn add(&mut self, check: LibraryCheck, fill: impl FnOnce(Violation) -> Violation) {
        if self.on(check) {
            let severity = self.severities.get(check);
            self.found.push(fill(Violation::new(check.id(), severity)));
        }
    }
}

fn blank(text: &str) -> bool {
    text.trim().is_empty()
}

fn check_symbol(elements: &dyn Elements, symbol: &Symbol, r: &mut Report<'_>) {
    if blank(&symbol.description) {
        r.add(LibraryCheck::MissingDescription, |v| v.object(symbol.uuid));
    }
    if blank(&symbol.designator_prefix) {
        r.add(LibraryCheck::MissingDesignatorPrefix, |v| {
            v.object(symbol.uuid)
        });
    }
    for pin in &symbol.pins {
        let on_grid = |n: Nm| n.0 % PIN_GRID.0 == 0;
        if !on_grid(pin.at.x) || !on_grid(pin.at.y) {
            r.add(LibraryCheck::OffGridPin, |v| {
                v.arg("pin", pin.number.as_str())
                    .arg("grid", PIN_GRID)
                    .objects([symbol.uuid, pin.uuid])
            });
        }
        if blank(&pin.number) {
            r.add(LibraryCheck::MissingPinNumber, |v| {
                v.arg("name", pin.name.as_str())
                    .objects([symbol.uuid, pin.uuid])
            });
        }
        if blank(&pin.name) {
            r.add(LibraryCheck::MissingPinName, |v| {
                v.arg("pin", pin.number.as_str())
                    .objects([symbol.uuid, pin.uuid])
            });
        }
    }
    let mut by_number: BTreeMap<&str, Vec<Uuid>> = BTreeMap::new();
    for pin in symbol.pins.iter().filter(|p| !blank(&p.number)) {
        by_number.entry(&pin.number).or_default().push(pin.uuid);
    }
    for (number, pins) in by_number {
        if pins.len() > 1 {
            r.add(LibraryCheck::DuplicatePinNumber, |v| {
                v.arg("number", number).object(symbol.uuid).objects(pins)
            });
        }
    }
    if r.on(LibraryCheck::MissingPinsInSequence) {
        let missing = missing_in_sequence(elements, symbol);
        if !missing.is_empty() {
            r.add(LibraryCheck::MissingPinsInSequence, |v| {
                v.arg("missing", ranges(&missing)).object(symbol.uuid)
            });
        }
    }
    let graphics: Vec<_> = symbol
        .graphics
        .iter()
        .map(|g| (g.uuid, (g.unit, &g.shape)))
        .collect();
    for twins in duplicates(&graphics) {
        r.add(LibraryCheck::DuplicatePrimitives, |v| {
            v.object(symbol.uuid).objects(twins)
        });
    }
}

/// The numbers from 1 to the greatest numeric Pin number that no Pin has, leaving out the
/// `absent_pins` of any generated Footprint of a Part on this Symbol.
fn missing_in_sequence(elements: &dyn Elements, symbol: &Symbol) -> Vec<u64> {
    let numbers: BTreeSet<u64> = symbol
        .pins
        .iter()
        .filter_map(|p| p.number.trim().parse().ok())
        .collect();
    let Some(&last) = numbers.last() else {
        return vec![];
    };
    let mut absent = BTreeSet::new();
    for part in elements.parts() {
        if part.symbol != Some(symbol.uuid) {
            continue;
        }
        for footprint in part
            .footprints
            .iter()
            .filter_map(|f| elements.footprint(*f))
        {
            let listed = footprint
                .generator
                .as_ref()
                .and_then(|g| g.params["absent_pins"].as_array().cloned())
                .unwrap_or_default();
            absent.extend(listed.iter().filter_map(serde_json::Value::as_u64));
        }
    }
    (1..last)
        .filter(|n| !numbers.contains(n) && !absent.contains(n))
        .collect()
}

/// Sorted numbers written as runs: `3-4, 6`.
fn ranges(numbers: &[u64]) -> String {
    let mut runs: Vec<(u64, u64)> = vec![];
    for &n in numbers {
        match runs.last_mut() {
            Some((_, end)) if *end + 1 == n => *end = n,
            _ => runs.push((n, n)),
        }
    }
    runs.iter()
        .map(|&(a, b)| match a == b {
            true => a.to_string(),
            false => format!("{a}-{b}"),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// The UUIDs of each group of two or more items with equal keys, each group sorted, the
/// groups in order.
fn duplicates<K: PartialEq>(items: &[(Uuid, K)]) -> Vec<Vec<Uuid>> {
    let mut groups: Vec<Vec<Uuid>> = vec![];
    let mut taken = vec![false; items.len()];
    for (i, (uuid, key)) in items.iter().enumerate() {
        if taken[i] {
            continue;
        }
        let mut group = vec![*uuid];
        for (j, (other, other_key)) in items.iter().enumerate().skip(i + 1) {
            if !taken[j] && other_key == key {
                taken[j] = true;
                group.push(*other);
            }
        }
        if group.len() > 1 {
            group.sort();
            groups.push(group);
        }
    }
    groups.sort();
    groups
}

fn check_footprint(footprint: &Footprint, r: &mut Report<'_>) {
    if blank(&footprint.description) {
        r.add(LibraryCheck::MissingDescription, |v| {
            v.object(footprint.uuid)
        });
    }
    let courtyard = [
        FootprintLayer::CourtyardTop,
        FootprintLayer::CourtyardBottom,
    ];
    if !footprint
        .graphics
        .iter()
        .any(|g| courtyard.contains(&g.layer))
    {
        r.add(LibraryCheck::MissingCourtyard, |v| v.object(footprint.uuid));
    }
    let mut smd = footprint
        .pads
        .iter()
        .filter(|p| p.kind == PadKind::Smd)
        .peekable();
    let through = footprint.pads.iter().any(|p| p.hole.is_some());
    let mirrored =
        !through && smd.peek().is_some() && smd.all(|p| p.copper_side() == Some(Side::Bottom));
    if mirrored {
        r.add(LibraryCheck::MirroredFootprint, |v| {
            v.object(footprint.uuid)
        });
    }
    check_origin(footprint, r);
    for pad in &footprint.pads {
        if let Some(reda_model::Paste::Windows { windows }) = &pad.paste {
            let copper = pad.outline();
            for (window, w) in windows.iter().enumerate() {
                if !w.fits(&copper) {
                    r.add(LibraryCheck::PasteWindowOutsideCopper, |v| {
                        v.arg("pad", pad.number.as_str())
                            .arg("window", window as i64)
                            .objects([footprint.uuid, pad.uuid])
                    });
                }
            }
        }

        let outside = pad.kind == PadKind::PlatedThroughHole
            && pad
                .form()
                .annular_ring()
                .is_some_and(|ring| ring < Nm::ZERO);
        if outside {
            r.add(LibraryCheck::HoleOutsideCopper, |v| {
                v.arg("pad", pad.number.as_str())
                    .objects([footprint.uuid, pad.uuid])
            });
        }
    }
    let graphics: Vec<_> = footprint
        .graphics
        .iter()
        .map(|g| (g.uuid, (g.layer, &g.shape)))
        .collect();
    for twins in duplicates(&graphics) {
        r.add(LibraryCheck::DuplicatePrimitives, |v| {
            v.object(footprint.uuid).objects(twins)
        });
    }
    let stale = || crate::generate::still_made(footprint) == Some(false);
    if r.on(LibraryCheck::GeneratorOutOfDate) && stale() {
        let command = footprint.generator.as_ref().map(|g| g.command.as_str());
        r.add(LibraryCheck::GeneratorOutOfDate, |v| {
            v.arg("generator", command.unwrap_or_default())
                .object(footprint.uuid)
        });
    }
}

/// `origin_outside_body` when the origin is outside the body box, else
/// `origin_off_centroid` when the box's centre is more than [`CENTROID_TOLERANCE`] from it
/// in x or y, with the centre as `x` and `y`.
fn check_origin(footprint: &Footprint, r: &mut Report<'_>) {
    let Some(body) = footprint.body_box() else {
        return;
    };
    let inside = (body.min.x..=body.max.x).contains(&Nm::ZERO)
        && (body.min.y..=body.max.y).contains(&Nm::ZERO);
    if !inside {
        r.add(LibraryCheck::OriginOutsideBody, |v| {
            v.object(footprint.uuid)
        });
        return;
    }
    // Twice the centre, so an odd-sized box doesn't round.
    let cx = body.min.x.0 + body.max.x.0;
    let cy = body.min.y.0 + body.max.y.0;
    let limit = 2 * CENTROID_TOLERANCE.0;
    if cx.abs() > limit || cy.abs() > limit {
        r.add(LibraryCheck::OriginOffCentroid, |v| {
            v.arg("x", Nm(cx / 2))
                .arg("y", Nm(cy / 2))
                .object(footprint.uuid)
        });
    }
}

fn check_part(elements: &dyn Elements, part: &Part, r: &mut Report<'_>) {
    if blank(&part.fields.description) {
        r.add(LibraryCheck::MissingDescription, |v| v.object(part.uuid));
    }
    let others: Vec<&Part> = elements
        .parts()
        .into_iter()
        .filter(|p| p.uuid != part.uuid)
        .collect();
    let alike = |same: &dyn Fn(&Part) -> bool| -> Vec<Uuid> {
        let mut uuids: Vec<Uuid> = others.iter().filter(|p| same(p)).map(|p| p.uuid).collect();
        uuids.sort();
        uuids.dedup();
        uuids
    };
    let named = alike(&|p| p.name == part.name);
    if !named.is_empty() {
        r.add(LibraryCheck::DuplicatePartName, |v| {
            v.arg("name", part.name.as_str())
                .object(part.uuid)
                .objects(named)
        });
    }
    let mpn = part.fields.mpn.trim();
    if !mpn.is_empty() {
        let sharing = alike(&|p| p.fields.mpn.trim() == mpn);
        if !sharing.is_empty() {
            r.add(LibraryCheck::DuplicateMpn, |v| {
                v.arg("mpn", mpn).object(part.uuid).objects(sharing)
            });
        }
    }
    let broken = |r: &mut Report<'_>, kind: &str, reference: String, also: Option<Uuid>| {
        r.add(LibraryCheck::BrokenReference, |v| {
            v.arg("kind", kind)
                .arg("reference", reference)
                .object(part.uuid)
                .objects(also)
        });
    };
    let symbol = match part.symbol {
        Some(uuid) => {
            let symbol = elements.symbol(uuid);
            if symbol.is_none() {
                broken(r, "symbol", uuid.to_string(), None);
            }
            symbol
        }
        None => None,
    };
    let mut footprints = vec![];
    for &uuid in &part.footprints {
        match elements.footprint(uuid) {
            Some(footprint) => footprints.push(footprint),
            None => broken(r, "footprint", uuid.to_string(), None),
        }
    }
    for footprint in &footprints {
        tie_copper(part, footprint, r);
        pad_numbers(part, footprint, r);
    }
    let Some(symbol) = symbol else {
        return;
    };
    for over in &part.pin_map.pins {
        if !symbol.pins.iter().any(|p| p.number == over.pin) {
            broken(r, "pin", over.pin.clone(), Some(symbol.uuid));
        }
    }
    for footprint in footprints {
        check_pin_map(part, symbol, footprint, r);
    }
}

/// `missing_pad_number` for each Pad of `footprint` with no number, unless it is
/// non-plated or `part` is Mechanical, so mounting holes and fiducials pass.
fn pad_numbers(part: &Part, footprint: &Footprint, r: &mut Report<'_>) {
    if part.component_type == ComponentType::Mechanical {
        return;
    }
    for pad in &footprint.pads {
        if blank(&pad.number) && pad.kind != PadKind::NonPlated {
            r.add(LibraryCheck::MissingPadNumber, |v| {
                v.arg("footprint", footprint.name.as_str()).objects([
                    part.uuid,
                    footprint.uuid,
                    pad.uuid,
                ])
            });
        }
    }
}

/// `net_tie_not_joined` for a Net Tie's Footprint that isn't a valid tie, `shorted_copper`
/// for touching Pads with different numbers on any other Part's.
fn tie_copper(part: &Part, footprint: &Footprint, r: &mut Report<'_>) {
    if part.component_type == ComponentType::NetTie {
        if let Some(problem) = footprint_copper::tie_problem(footprint) {
            r.add(LibraryCheck::NetTieNotJoined, |v| {
                v.arg("footprint", footprint.name.as_str())
                    .arg("reason", problem.id())
                    .objects([part.uuid, footprint.uuid])
            });
        }
        return;
    }
    for (a, b) in footprint_copper::shorted_pads(footprint) {
        let (a, b) = (&footprint.pads[a], &footprint.pads[b]);
        r.add(LibraryCheck::ShortedCopper, |v| {
            v.arg("pad_a", a.number.as_str())
                .arg("pad_b", b.number.as_str())
                .arg("footprint", footprint.name.as_str())
                .objects([part.uuid, footprint.uuid, a.uuid, b.uuid])
        });
    }
}

/// Pins that reach no Pad of `footprint`, and Pads no Pin reaches that are not marked
/// unconnected. Non-plated Pads carry no signal, so they need no Pin; a Pad with no
/// number is left to `missing_pad_number`.
fn check_pin_map(part: &Part, symbol: &Symbol, footprint: &Footprint, r: &mut Report<'_>) {
    let mut mapped: BTreeSet<&str> = BTreeSet::new();
    for pin in &symbol.pins {
        let pads = part.pin_map.pads_of(&pin.number);
        if !footprint
            .pads
            .iter()
            .any(|pad| pads.contains(&pad.number.as_str()))
        {
            r.add(LibraryCheck::PinWithoutPad, |v| {
                v.arg("pin", pin.number.as_str())
                    .arg("footprint", footprint.name.as_str())
                    .objects([part.uuid, pin.uuid, footprint.uuid])
            });
        }
        mapped.extend(pads);
    }
    let mut unmapped: BTreeMap<&str, Vec<Uuid>> = BTreeMap::new();
    for pad in &footprint.pads {
        let number = pad.number.as_str();
        let unconnected = part.pin_map.unconnected.iter().any(|n| n == number);
        let signal = pad.kind != PadKind::NonPlated && !blank(number);
        if signal && !mapped.contains(number) && !unconnected {
            unmapped.entry(number).or_default().push(pad.uuid);
        }
    }
    for (number, pads) in unmapped {
        r.add(LibraryCheck::UnmappedPad, |v| {
            v.arg("pad", number)
                .arg("footprint", footprint.name.as_str())
                .objects([part.uuid, footprint.uuid])
                .objects(pads)
        });
    }
}
