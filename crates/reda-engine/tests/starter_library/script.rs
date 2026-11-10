//! The driver the starter Library script runs on: an engine with a writable Library, and
//! helpers that run the library editing Commands and the Footprint generators.
//!
//! UUIDs come from [`KeyedIds`] and are frozen in `uuids.tsv` beside this file: each
//! element has a key (`symbol/Resistor`, `part/Resistor`, `generate/chip/{…}`) and its
//! UUIDs, in the order the script draws them, are read from the table under that key.
//! Renaming an element in the script and its key in the table keeps its UUIDs, so
//! Projects that placed it still find it; a key the table lacks gets fresh UUIDs, hashed
//! from the key, which `REDA_BLESS=1` adds to the table.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use reda_engine::Engine;
use reda_engine::command::{FixedClock, IdSource};
use reda_engine::format::library::write_library_info;
use reda_engine::model::{LibraryInfo, Uuid};
use serde_json::{Value, json};

/// The starter Library's UUID and name.
pub const LIBRARY: &str = "5a7e0000-0000-4000-8000-000000000001";
pub const NAME: &str = "Starter";

/// The key of UUIDs drawn outside any element (the scratch Project), which never reach the
/// Library and so are not frozen.
const SESSION: &str = "session";

/// The checked-in table of frozen UUIDs.
pub fn uuids_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/starter_library/uuids.tsv")
}

/// Each element key's UUIDs, in the order the script draws them: the text of `uuids.tsv`,
/// one `key<TAB>uuid` line per UUID.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Uuids(BTreeMap<String, Vec<Uuid>>);

impl Uuids {
    pub fn parse(text: &str) -> Uuids {
        let mut table = Uuids::default();
        for line in text
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
        {
            let (key, uuid) = line
                .split_once('\t')
                .unwrap_or_else(|| panic!("uuids.tsv: no tab in {line:?}"));
            let uuid = uuid
                .parse()
                .unwrap_or_else(|_| panic!("uuids.tsv: bad UUID in {line:?}"));
            table.0.entry(key.to_owned()).or_default().push(uuid);
        }
        table
    }

    /// The checked-in table.
    pub fn checked_in() -> Uuids {
        Uuids::parse(&std::fs::read_to_string(uuids_path()).unwrap_or_default())
    }

    pub fn to_text(&self) -> String {
        let mut text = String::from(
            "# The starter Library's frozen UUIDs: key<TAB>UUID, each key's UUIDs in the order\n\
             # the script draws them. To rename an element, rename its key here too. Written by\n\
             # REDA_BLESS=1 cargo test -p reda-engine --test starter_library.\n",
        );
        for (key, uuids) in &self.0 {
            for uuid in uuids {
                text.push_str(&format!("{key}\t{uuid}\n"));
            }
        }
        text
    }

    /// The keys here that `used` never drew from: elements removed or renamed.
    pub fn unused_keys(&self, used: &Uuids) -> Vec<String> {
        self.0
            .keys()
            .filter(|key| !used.0.contains_key(*key))
            .cloned()
            .collect()
    }
}

/// UUIDs for the current key (set by the script before each element): the frozen ones
/// first, then ones hashed from the key and a per-key counter.
#[derive(Clone, Default)]
pub struct KeyedIds(Arc<Mutex<Keys>>);

#[derive(Default)]
pub struct Keys {
    key: String,
    counts: BTreeMap<String, usize>,
    frozen: Uuids,
    used: Uuids,
}

impl KeyedIds {
    fn new(frozen: Uuids) -> KeyedIds {
        KeyedIds(Arc::new(Mutex::new(Keys {
            frozen,
            ..Keys::default()
        })))
    }

    fn set(&self, key: &str) {
        self.0.lock().unwrap().key = key.to_owned();
    }

    fn used(&self) -> Uuids {
        self.0.lock().unwrap().used.clone()
    }
}

impl IdSource for KeyedIds {
    fn next_id(&mut self) -> Uuid {
        let mut keys = self.0.lock().unwrap();
        let key = keys.key.clone();
        let count = keys.counts.entry(key.clone()).or_default();
        *count += 1;
        let count = *count;
        let uuid = match keys.frozen.0.get(&key).and_then(|u| u.get(count - 1)) {
            Some(&uuid) => uuid,
            None => Uuid::v4_from_bits(hash(format!("reda-starter\0{key}\0{count}").as_bytes())),
        };
        if key != SESSION {
            keys.used.0.entry(key).or_default().push(uuid);
        }
        uuid
    }
}

/// 128-bit FNV-1a with each half then mixed by SplitMix64's finaliser, so that keys
/// differing in one character give UUIDs differing throughout.
fn hash(bytes: &[u8]) -> u128 {
    let mut hash: u128 = 0x6c62272e07bb014262b821756295c58d;
    for &b in bytes {
        hash ^= u128::from(b);
        hash = hash.wrapping_mul(0x0000000001000000000000000000013b);
    }
    let (hi, lo) = ((hash >> 64) as u64, hash as u64);
    let a = mix(lo ^ hi.rotate_left(17));
    let b = mix(hi ^ a);
    (u128::from(b) << 64) | u128::from(a)
}

fn mix(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58476d1ce4e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}

/// Millimetres in nanometres.
pub fn mm(value: f64) -> i64 {
    (value * 1e6).round() as i64
}

/// Mils in nanometres.
pub const fn mil(value: i64) -> i64 {
    value * 25_400
}

pub fn pt(x: i64, y: i64) -> Value {
    json!({"x": x, "y": y})
}

pub fn line(x1: i64, y1: i64, x2: i64, y2: i64) -> Value {
    json!({"kind": "line", "start": pt(x1, y1), "end": pt(x2, y2)})
}

pub fn rect(x1: i64, y1: i64, x2: i64, y2: i64) -> Value {
    json!({"kind": "rect", "start": pt(x1, y1), "end": pt(x2, y2)})
}

pub fn circle(x: i64, y: i64, radius: i64) -> Value {
    json!({"kind": "circle", "center": pt(x, y), "radius": radius})
}

/// The arc from `start` around `center`, `sweep` degrees counter-clockwise.
pub fn arc(center: (i64, i64), start: (i64, i64), sweep: i64) -> Value {
    arc_millideg(center, start, sweep * 1000)
}

pub fn arc_millideg(center: (i64, i64), start: (i64, i64), sweep: i64) -> Value {
    json!({
        "kind": "arc",
        "center": pt(center.0, center.1),
        "start": pt(start.0, start.1),
        "sweep": sweep,
    })
}

pub fn polygon(points: &[(i64, i64)]) -> Value {
    let points: Vec<Value> = points.iter().map(|&(x, y)| pt(x, y)).collect();
    json!({"kind": "polygon", "points": points})
}

pub fn text(x: i64, y: i64, text: &str, size: i64) -> Value {
    json!({"kind": "text", "at": pt(x, y), "text": text, "size": size})
}

/// An open Symbol being drawn.
pub struct Sym {
    pub uuid: String,
    /// The Unit Pins and graphics go on.
    pub unit: String,
    /// Whether new Pins show their names and numbers.
    pub names: bool,
    pub numbers: bool,
}

/// One Pin: name, number, electrical type, connection end (mil), direction towards the
/// body (degrees) and length (mil).
pub struct Pin<'a> {
    pub name: &'a str,
    pub number: &'a str,
    pub kind: &'a str,
    pub at: (i64, i64),
    pub rotation: i64,
    pub length: i64,
}

/// A Pin pointing right into the body from its left end.
pub fn left<'a>(name: &'a str, number: &'a str, kind: &'a str, x: i64, y: i64) -> Pin<'a> {
    Pin {
        name,
        number,
        kind,
        at: (x, y),
        rotation: 0,
        length: 100,
    }
}

pub fn right<'a>(name: &'a str, number: &'a str, kind: &'a str, x: i64, y: i64) -> Pin<'a> {
    Pin {
        rotation: 180,
        ..left(name, number, kind, x, y)
    }
}

pub fn top<'a>(name: &'a str, number: &'a str, kind: &'a str, x: i64, y: i64) -> Pin<'a> {
    Pin {
        rotation: 270,
        ..left(name, number, kind, x, y)
    }
}

pub fn bottom<'a>(name: &'a str, number: &'a str, kind: &'a str, x: i64, y: i64) -> Pin<'a> {
    Pin {
        rotation: 90,
        ..left(name, number, kind, x, y)
    }
}

/// What a Part is made of.
pub struct PartSpec<'a> {
    pub name: &'a str,
    pub symbol: Option<&'a str>,
    pub footprints: Vec<String>,
    /// `library.part.fields.set` params, without `part`.
    pub fields: Value,
    /// `library.part.pin_map.set` params, without `part`; `None` keeps the implicit map.
    pub pin_map: Option<Value>,
    /// Its Component Type, In BOM and In CPL.
    pub kind: Kind,
}

/// The Component Type of a Part, with the In BOM and In CPL it gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Standard, in the BOM and CPL.
    Standard,
    /// Mechanical, out of the BOM and CPL: a mounting hole or a fiducial.
    Mechanical,
    /// A Net Tie, out of the BOM and CPL.
    NetTie,
    /// Standard but out of the BOM and CPL, like a test point: copper only.
    Unassembled,
}

/// The engine with a new, empty Library open for writing.
pub struct Script {
    engine: Engine,
    ids: KeyedIds,
    library: PathBuf,
    /// Every element made so far, by kind and name.
    elements: BTreeMap<(String, String), String>,
}

impl Script {
    /// A Project in `dir` whose Project Library `lib` is where the script writes, with the
    /// checked-in UUIDs.
    pub fn new(dir: &Path) -> Script {
        Script::with_uuids(dir, Uuids::checked_in())
    }

    /// [`Script::new`] with the frozen UUIDs `frozen`.
    pub fn with_uuids(dir: &Path, frozen: Uuids) -> Script {
        let ids = KeyedIds::new(frozen);
        ids.set(SESSION);
        let engine = Engine::builder()
            .ids(ids.clone())
            .clock(FixedClock::EPOCH)
            .user_dirs(None)
            .starter_library(None)
            .build();
        let mut script = Script {
            engine,
            ids,
            library: dir.join("Build/lib"),
            elements: BTreeMap::new(),
        };
        script.run("project.new", json!({"path": dir, "name": "Build"}));
        std::fs::create_dir_all(&script.library).unwrap();
        let info = LibraryInfo {
            uuid: LIBRARY.parse().unwrap(),
            name: NAME.into(),
            description: "The generic parts that ship with REDA. MIT licensed.".into(),
        };
        std::fs::write(
            script.library.join(format!("{NAME}.redalib")),
            write_library_info(&info).unwrap(),
        )
        .unwrap();
        script.run("project.library.add", json!({"path": "lib"}));
        script
    }

    /// The Library folder written so far.
    pub fn library(&self) -> &Path {
        &self.library
    }

    /// The UUIDs every element drew so far, by key: what `uuids.tsv` should hold.
    pub fn uuids(&self) -> Uuids {
        self.ids.used()
    }

    pub fn run(&mut self, id: &str, params: Value) -> Value {
        match self.engine.run(id, params.clone()) {
            Ok(value) => value,
            Err(err) => panic!("{id} {params} failed: {err}"),
        }
    }

    fn edit(&mut self, id: &str, params: Value) -> Value {
        self.run(id, params)["result"].clone()
    }

    fn new_element(&mut self, kind: &str, name: &str) -> String {
        self.ids.set(&format!("{kind}/{name}"));
        let out = self.run(
            "library.element.new",
            json!({"library": LIBRARY, "kind": kind, "name": name}),
        );
        let uuid = out["element"].as_str().unwrap().to_owned();
        self.elements
            .insert((kind.to_owned(), name.to_owned()), uuid.clone());
        uuid
    }

    /// The Symbol called `name` made earlier in the script.
    pub fn symbol_named(&self, name: &str) -> Option<String> {
        self.elements
            .get(&("symbol".to_owned(), name.to_owned()))
            .cloned()
    }

    fn save(&mut self, element: &str) {
        self.run("library.element.save", json!({ "element": element }));
        self.run("library.element.close", json!({ "element": element }));
    }

    // Symbols.

    pub fn symbol(&mut self, name: &str, prefix: &str, description: &str) -> Sym {
        let uuid = self.new_element("symbol", name);
        self.edit(
            "library.symbol.edit",
            json!({"symbol": uuid, "description": description, "designator_prefix": prefix}),
        );
        let object = self.run("object.get", json!({ "uuid": uuid }))["object"].clone();
        let unit = object["units"][0]["uuid"].as_str().unwrap().to_owned();
        Sym {
            uuid,
            unit,
            names: true,
            numbers: true,
        }
    }

    /// Adds a Unit called `name` and makes it the one drawn on.
    pub fn unit(&mut self, sym: &mut Sym, name: &str) {
        let out = self.edit(
            "library.symbol.unit.add",
            json!({"symbol": sym.uuid, "name": name}),
        );
        sym.unit = out["unit"].as_str().unwrap().to_owned();
    }

    pub fn pin(&mut self, sym: &Sym, pin: Pin<'_>) {
        self.edit(
            "library.symbol.pin.add",
            json!({
                "symbol": sym.uuid,
                "unit": sym.unit,
                "name": pin.name,
                "number": pin.number,
                "electrical_type": pin.kind,
                "at": pt(mil(pin.at.0), mil(pin.at.1)),
                "rotation": pin.rotation * 1000,
                "length": mil(pin.length),
                "name_visible": sym.names,
                "number_visible": sym.numbers,
            }),
        );
    }

    /// A graphic with coordinates already in nanometres.
    pub fn draw(&mut self, sym: &Sym, shape: Value, fill: bool) {
        self.edit(
            "library.symbol.graphic.add",
            json!({
                "symbol": sym.uuid,
                "unit": sym.unit,
                "shape": shape,
                "width": mil(10),
                "fill": fill,
            }),
        );
    }

    /// Moves the origin to Pin 1's connection end, as a rectangular IC body has it. Draw
    /// it about any convenient point first.
    pub fn origin_at_pin_1(&mut self, sym: &Sym) {
        self.edit(
            "library.symbol.origin.set",
            json!({"symbol": sym.uuid, "to": {"kind": "pin1"}}),
        );
    }

    pub fn finish_symbol(&mut self, sym: Sym) -> String {
        self.save(&sym.uuid);
        sym.uuid
    }

    // Footprints.

    /// Runs `library.footprint.generate.<family>` and returns the Footprint's UUID.
    pub fn generate(&mut self, family: &str, mut params: Value) -> String {
        self.ids.set(&format!("generate/{family}/{params}"));
        params["library"] = json!(LIBRARY);
        let out = self.run(&format!("library.footprint.generate.{family}"), params);
        let violations = out["violations"].as_array().unwrap();
        assert!(violations.is_empty(), "{family}: {violations:?}");
        out["footprint"].as_str().unwrap().to_owned()
    }

    /// A new hand-drawn Footprint; `height` is the part's greatest height above the board,
    /// in millimetres.
    pub fn footprint(&mut self, name: &str, description: &str, height: f64) -> String {
        let uuid = self.new_element("footprint", name);
        self.edit(
            "library.footprint.edit",
            json!({"footprint": uuid, "description": description, "height": mm(height)}),
        );
        uuid
    }

    /// Adds a Pad; `pad` is `library.footprint.pad.add` params without `footprint`.
    pub fn pad(&mut self, footprint: &str, mut pad: Value) {
        pad["footprint"] = json!(footprint);
        self.edit("library.footprint.pad.add", pad);
    }

    pub fn fp_draw(&mut self, footprint: &str, layer: &str, shape: Value, width: i64) {
        self.edit(
            "library.footprint.graphic.add",
            json!({"footprint": footprint, "layer": layer, "shape": shape, "width": width}),
        );
    }

    /// Puts the Designator above the courtyard, whose top edge is at `top`.
    pub fn finish_footprint(&mut self, footprint: &str, top: i64) -> String {
        let top = top - self.centre_on_courtyard(footprint);
        let object = self.run("object.get", json!({ "uuid": footprint }))["object"].clone();
        let designator = object["texts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["field"] == "Designator")
            .unwrap()["uuid"]
            .clone();
        self.edit(
            "library.footprint.text.edit",
            json!({"footprint": footprint, "text": designator, "at": pt(0, top + mm(0.8))}),
        );
        self.assembly_designator(footprint);
        self.save(footprint);
        footprint.to_owned()
    }

    /// Adds the Assembly Designator in the middle of the body and moves the Value below the
    /// courtyard, both sized as the generators size them: half the Assembly outline's
    /// shorter side, at most 1 mm, in 0.01 mm steps.
    fn assembly_designator(&mut self, footprint: &str) {
        let object = self.run("object.get", json!({ "uuid": footprint }))["object"].clone();
        let extent = |prefix: &str| {
            let (mut xs, mut ys) = (vec![], vec![]);
            for g in object["graphics"].as_array().unwrap() {
                if g["layer"].as_str().unwrap().starts_with(prefix) {
                    for (x, y) in shape_extent(&g["shape"]) {
                        xs.push(x);
                        ys.push(y);
                    }
                }
            }
            assert!(!xs.is_empty(), "{footprint}: nothing on {prefix}");
            (xs, ys)
        };
        let span = |v: &[i64]| v.iter().max().unwrap() - v.iter().min().unwrap();
        let (xs, ys) = extent("assembly");
        let size = (span(&xs).min(span(&ys)) / 2).min(mm(1.0)) / 10_000 * 10_000;
        self.edit(
            "library.footprint.text.add",
            json!({"footprint": footprint, "field": "Designator", "at": pt(0, 0),
                   "layer": "assembly_top", "height": size}),
        );
        let bottom = *extent("courtyard").1.iter().min().unwrap();
        let value = object["texts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["field"] == "Value")
            .unwrap()["uuid"]
            .clone();
        self.edit(
            "library.footprint.text.edit",
            json!({"footprint": footprint, "text": value, "height": size,
                   "at": pt(0, bottom - mm(0.2) - size / 2)}),
        );
    }

    /// Moves the origin of a Footprint whose courtyard box's centre is more than the
    /// Library check's 0.1 mm off it (`origin_off_centroid`) to that centre, and returns
    /// how far it moved in y. Rectangle, circle and polygon courtyards are read.
    fn centre_on_courtyard(&mut self, footprint: &str) -> i64 {
        let object = self.run("object.get", json!({ "uuid": footprint }))["object"].clone();
        let mut xs = vec![];
        let mut ys = vec![];
        for g in object["graphics"].as_array().unwrap() {
            if !g["layer"].as_str().unwrap().starts_with("courtyard") {
                continue;
            }
            for (x, y) in shape_extent(&g["shape"]) {
                xs.push(x);
                ys.push(y);
            }
        }
        let centre = |v: &[i64]| match (v.iter().min(), v.iter().max()) {
            (Some(a), Some(b)) => (a + b) / 2,
            _ => 0,
        };
        let (cx, cy) = (centre(&xs), centre(&ys));
        if cx.abs() <= mm(0.1) && cy.abs() <= mm(0.1) {
            return 0;
        }
        self.edit(
            "library.footprint.origin.set",
            json!({"footprint": footprint, "at": pt(cx, cy)}),
        );
        cy
    }

    // Parts.

    pub fn part(&mut self, spec: PartSpec<'_>) -> String {
        let uuid = self.new_element("part", spec.name);
        let component_type = match spec.kind {
            Kind::Standard => None,
            Kind::Mechanical => Some("mechanical"),
            Kind::NetTie => Some("net_tie"),
            Kind::Unassembled => Some("standard"),
        };
        if let Some(component_type) = component_type {
            self.edit(
                "library.part.edit",
                json!({"part": uuid, "component_type": component_type,
                       "in_bom": false, "in_cpl": false}),
            );
        }
        if let Some(symbol) = spec.symbol {
            self.edit(
                "library.part.symbol.set",
                json!({"part": uuid, "symbol": symbol}),
            );
        }
        self.edit(
            "library.part.footprints.set",
            json!({"part": uuid, "footprints": spec.footprints}),
        );
        let mut fields = spec.fields;
        fields["part"] = json!(uuid);
        self.edit("library.part.fields.set", fields);
        if let Some(mut map) = spec.pin_map {
            map["part"] = json!(uuid);
            self.edit("library.part.pin_map.set", map);
        }
        self.save(&uuid);
        uuid
    }
}

/// Points spanning a graphic's box, in nanometres; an arc counts as its whole circle.
fn shape_extent(shape: &Value) -> Vec<(i64, i64)> {
    let at = |p: &Value| (p["x"].as_i64().unwrap(), p["y"].as_i64().unwrap());
    match shape["kind"].as_str().unwrap() {
        "rect" | "line" => vec![at(&shape["start"]), at(&shape["end"])],
        "circle" => {
            let (x, y) = at(&shape["center"]);
            let r = shape["radius"].as_i64().unwrap();
            vec![(x - r, y - r), (x + r, y + r)]
        }
        "arc" => {
            let ((x, y), (sx, sy)) = (at(&shape["center"]), at(&shape["start"]));
            let r = ((sx - x) as f64).hypot((sy - y) as f64).round() as i64;
            vec![(x - r, y - r), (x + r, y + r)]
        }
        "polygon" => shape["points"].as_array().unwrap().iter().map(at).collect(),
        other => panic!("a {other} outline: {shape}"),
    }
}
