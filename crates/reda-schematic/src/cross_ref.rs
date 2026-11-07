//! Port Cross-References: where a Port's partners are, and the note drawn past its name,
//! such as `[2:B3, 3:A1]`. Both are derived and never stored.
//!
//! The note's styles come from the Project or, when it follows them, the Preferences,
//! which live above this crate: callers resolve them with
//! [`SchematicOptions::cross_reference_styles`](reda_model::SchematicOptions).

use std::collections::BTreeMap;

use reda_model::geom::{Nm, Point};
use reda_model::net_name::name_key;
use reda_model::{
    BorderZones, CrossReferenceStyles, DocumentKind, LocationStyle, Project, SheetStyle, Uuid,
};
use schemars::JsonSchema;
use serde::Serialize;

/// Another Port of the same name, on another sheet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Partner {
    pub port: Uuid,
    /// The Schematic Sheet it is on.
    pub document: Uuid,
    /// That sheet's number in the Projects panel, from 1.
    pub sheet_number: usize,
    pub sheet_title: String,
    /// Its connection point.
    pub at: Point,
    /// The Border Zone holding it, such as `B3`, or null on a sheet with Border Zones off.
    pub zone: Option<String>,
}

/// The partners of Port `port`: the Ports with the same name (ignoring case) on the
/// Project's other Schematic Sheets, in sheet order and then from the top left. `None`
/// when no sheet holds a Port with that UUID.
pub fn partners(project: &Project, port: Uuid) -> Option<Vec<Partner>> {
    let all = ports_by_sheet(project);
    let (home, name) = all.iter().find_map(|(sheet, ports)| {
        ports
            .iter()
            .find(|p| p.uuid == port)
            .map(|p| (sheet.uuid, name_key(&p.name)))
    })?;
    Some(partners_in(&all, home, &name))
}

/// Every Port's cross-reference note in `styles`, keyed by Port UUID. Ports with no
/// partners, a hidden note or nothing to say are left out, as is everything when
/// `styles` is `None` (the Project shows no cross-references).
pub fn notes(project: &Project, styles: Option<CrossReferenceStyles>) -> BTreeMap<Uuid, String> {
    let Some(styles) = styles else {
        return BTreeMap::new();
    };
    let all = ports_by_sheet(project);
    let mut out = BTreeMap::new();
    for (sheet, ports) in &all {
        for port in ports.iter().filter(|p| !p.hide_cross_reference) {
            let partners = partners_in(&all, sheet.uuid, &name_key(&port.name));
            if let Some(note) = note(&partners, styles) {
                out.insert(port.uuid, note);
            }
        }
    }
    out
}

/// The note for `partners` in `styles`, such as `[2:B3, 3:A1]`: one entry per partner,
/// its sheet then its location joined by `:`, with repeats dropped. A style of `none`
/// leaves its half out, and `zone` on a sheet without Border Zones gives only the sheet.
/// `None` when no entry says anything.
pub fn note(partners: &[Partner], styles: CrossReferenceStyles) -> Option<String> {
    let mut entries: Vec<String> = Vec::new();
    for partner in partners {
        let sheet = match styles.sheet {
            SheetStyle::None => None,
            SheetStyle::Name => Some(partner.sheet_title.clone()),
            SheetStyle::Number => Some(partner.sheet_number.to_string()),
        };
        let location = match styles.location {
            LocationStyle::None => None,
            LocationStyle::Zone => partner.zone.clone(),
            LocationStyle::Xy => Some(format!("({},{})", mm(partner.at.x), mm(partner.at.y))),
        };
        let entry = match (sheet, location) {
            (Some(s), Some(l)) => format!("{s}:{l}"),
            (Some(one), None) | (None, Some(one)) => one,
            (None, None) => continue,
        };
        if !entries.contains(&entry) {
            entries.push(entry);
        }
    }
    (!entries.is_empty()).then(|| format!("[{}]", entries.join(", ")))
}

/// A length in millimetres, to at most two decimals with trailing zeros dropped.
fn mm(v: Nm) -> String {
    let hundredths = (v.0 as f64 / 10_000.0).round() as i64;
    let (sign, abs) = (if hundredths < 0 { "-" } else { "" }, hundredths.abs());
    let (whole, frac) = (abs / 100, abs % 100);
    match frac {
        0 => format!("{sign}{whole}"),
        f if f % 10 == 0 => format!("{sign}{whole}.{}", f / 10),
        f => format!("{sign}{whole}.{f:02}"),
    }
}

/// A Schematic Sheet's place and title, as cross-references name it.
struct SheetInfo {
    uuid: Uuid,
    number: usize,
    title: String,
    zones: Option<BorderZones>,
}

fn ports_by_sheet(project: &Project) -> Vec<(SheetInfo, Vec<&reda_model::Port>)> {
    project
        .documents()
        .filter(|d| d.kind == DocumentKind::Schematic)
        .enumerate()
        .filter_map(|(i, d)| {
            let body = project.schematic_sheet(d.uuid)?;
            let info = SheetInfo {
                uuid: d.uuid,
                number: i + 1,
                title: d.title.clone(),
                zones: body.zones(),
            };
            Some((info, body.ports().collect()))
        })
        .collect()
}

fn partners_in(
    all: &[(SheetInfo, Vec<&reda_model::Port>)],
    home: Uuid,
    name: &str,
) -> Vec<Partner> {
    let mut out = Vec::new();
    for (sheet, ports) in all.iter().filter(|(s, _)| s.uuid != home) {
        let mut here: Vec<_> = ports.iter().filter(|p| name_key(&p.name) == name).collect();
        // From the top left: higher first, then left first.
        here.sort_by_key(|p| (std::cmp::Reverse(p.at.y), p.at.x, p.uuid));
        out.extend(here.into_iter().map(|p| Partner {
            port: p.uuid,
            document: sheet.uuid,
            sheet_number: sheet.number,
            sheet_title: sheet.title.clone(),
            at: p.at,
            zone: sheet.zones.map(|z| z.zone(p.at)),
        }));
    }
    out
}
