//! An in-memory `LibraryFiles` and element builders for the scan and search tests.

#![allow(
    dead_code,
    reason = "each test file uses a different part of the support"
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use reda_library::LibraryFiles;
use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{
    ComponentType, Fields, Footprint, FootprintGraphic, FootprintLayer, FootprintText, LibraryInfo,
    Pad, PadKind, PadShape, Part, Pin, PinMap, PinType, Shape, Symbol, Unit, Uuid,
};

pub fn uuid(n: u128) -> Uuid {
    Uuid::v4_from_bits(n)
}

enum Entry {
    Library(LibraryInfo),
    Symbol(Symbol),
    Footprint(Footprint),
    Part(Part),
    Broken(String),
    Other,
}

/// Files held in memory by absolute path, already decoded.
#[derive(Default)]
pub struct MemFiles {
    files: BTreeMap<PathBuf, Entry>,
}

impl MemFiles {
    pub fn library(&mut self, path: &str, info: LibraryInfo) {
        self.files.insert(path.into(), Entry::Library(info));
    }
    pub fn symbol(&mut self, path: &str, symbol: Symbol) {
        self.files.insert(path.into(), Entry::Symbol(symbol));
    }
    pub fn footprint(&mut self, path: &str, footprint: Footprint) {
        self.files.insert(path.into(), Entry::Footprint(footprint));
    }
    pub fn part(&mut self, path: &str, part: Part) {
        self.files.insert(path.into(), Entry::Part(part));
    }
    pub fn broken(&mut self, path: &str, why: &str) {
        self.files.insert(path.into(), Entry::Broken(why.into()));
    }
    pub fn other(&mut self, path: &str) {
        self.files.insert(path.into(), Entry::Other);
    }

    fn get(&self, path: &Path) -> Result<&Entry, String> {
        match self.files.get(path) {
            Some(Entry::Broken(why)) => Err(why.clone()),
            Some(entry) => Ok(entry),
            None => Err(format!("{} does not exist", path.display())),
        }
    }
}

fn wrong(path: &Path) -> String {
    format!("{} holds another kind of file", path.display())
}

impl LibraryFiles for MemFiles {
    fn list(&mut self, dir: &Path) -> Result<Vec<String>, String> {
        Ok(self
            .files
            .keys()
            .filter(|p| p.parent() == Some(dir))
            .map(|p| p.file_name().unwrap().to_str().unwrap().to_owned())
            .collect())
    }

    fn read_library(&mut self, path: &Path) -> Result<LibraryInfo, String> {
        match self.get(path)? {
            Entry::Library(info) => Ok(info.clone()),
            _ => Err(wrong(path)),
        }
    }

    fn read_symbol(&mut self, path: &Path) -> Result<Symbol, String> {
        match self.get(path)? {
            Entry::Symbol(symbol) => Ok(symbol.clone()),
            _ => Err(wrong(path)),
        }
    }

    fn read_footprint(&mut self, path: &Path) -> Result<Footprint, String> {
        match self.get(path)? {
            Entry::Footprint(footprint) => Ok(footprint.clone()),
            _ => Err(wrong(path)),
        }
    }

    fn read_part(&mut self, path: &Path) -> Result<Part, String> {
        match self.get(path)? {
            Entry::Part(part) => Ok(part.clone()),
            _ => Err(wrong(path)),
        }
    }
}

pub fn library(n: u128, name: &str) -> LibraryInfo {
    LibraryInfo {
        uuid: uuid(n),
        name: name.into(),
        description: String::new(),
    }
}

/// A one-Unit Symbol with two passive Pins.
pub fn symbol(n: u128, name: &str) -> Symbol {
    let unit = uuid(n * 1000);
    let pin = |i: u128| Pin {
        uuid: uuid(n * 1000 + i),
        unit,
        name: "~".into(),
        number: i.to_string(),
        electrical_type: PinType::Passive,
        at: Point::new(Nm(0), Nm::from_mil(100) * (i as i64)),
        rotation: Millideg(0),
        length: Nm::from_mil(100),
        name_visible: true,
        number_visible: true,
        description: String::new(),
        inside_edge: None,
        outside_edge: None,
    };
    Symbol {
        uuid: uuid(n),
        name: name.into(),
        description: String::new(),
        designator_prefix: "R".into(),
        units: vec![Unit {
            uuid: unit,
            name: "A".into(),
        }],
        pins: vec![pin(1), pin(2)],
        graphics: vec![],
        provenance: None,
    }
}

/// A Footprint with two SMD Pads and a top courtyard.
pub fn footprint(n: u128, name: &str) -> Footprint {
    let pad = |i: u128| Pad {
        uuid: uuid(n * 1000 + i),
        number: i.to_string(),
        kind: PadKind::Smd,
        shape: PadShape::RoundedRect,
        at: Point::new(Nm::from_um(1000) * (i as i64), Nm(0)),
        rotation: Millideg(0),
        width: Nm::from_um(800),
        height: Nm::from_um(900),
        corner_ratio: Some(reda_model::Ratio::percent(50)),
        chamfer_ratio: None,
        corners: None,
        offset: None,
        hole: None,
        side: None,
        mask: None,
        paste: None,
        zone_connection: None,
    };
    Footprint {
        uuid: uuid(n),
        name: name.into(),
        description: String::new(),
        pads: vec![pad(1), pad(2)],
        graphics: vec![FootprintGraphic {
            uuid: uuid(n * 1000 + 999),
            layer: FootprintLayer::CourtyardTop,
            shape: Shape::Rect {
                start: Point::new(Nm::from_um(-200), Nm::from_um(-700)),
                end: Point::new(Nm::from_um(2600), Nm::from_um(700)),
            },
            width: Nm::from_um(50),
            fill: false,
        }],
        height: None,
        texts: vec![
            FootprintText::designator(uuid(n * 1000 + 997)),
            FootprintText::value(uuid(n * 1000 + 998)),
        ],
        generator: None,
        provenance: None,
    }
}

/// A Standard Part on the given Symbol and Footprints.
pub fn part(n: u128, name: &str, symbol: u128, footprints: &[u128]) -> Part {
    Part {
        uuid: uuid(n),
        name: name.into(),
        component_type: ComponentType::Standard,
        in_bom: ComponentType::Standard.default_in_bom(),
        in_cpl: ComponentType::Standard.default_in_cpl(),
        symbol: Some(uuid(symbol)),
        footprints: footprints.iter().map(|f| uuid(*f)).collect(),
        fields: Fields::default(),
        pin_map: PinMap::default(),
        provenance: None,
    }
}
