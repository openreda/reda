//! Reading one symbol out of a `.kicad_sym` library, and building the REDA Symbol.
//!
//! A KiCad symbol keeps its drawing in sub-symbols named `<name>_<unit>_<style>`. Unit 0
//! is common to every Unit; style 1 is the normal body, style 2 the De Morgan one, which
//! REDA does not have, so it is skipped.

use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{Pin, PinType, Provenance, Shape, Symbol, SymbolGraphic, Unit, Uuid};

use crate::node::{self, Read, fail};
use crate::sexpr::Sexp;
use crate::{MIN_SYMBOL_VERSION, shapes};

/// KiCad's default line width, used where a file gives 0 ("default").
const DEFAULT_WIDTH: Nm = Nm(152_400);

/// Most Units a symbol may have; more is a broken or hostile file.
const MAX_UNITS: u32 = 256;

#[derive(Clone, Debug)]
pub struct PinSource {
    pub unit: u32,
    pub name: String,
    pub number: String,
    pub electrical_type: PinType,
    pub at: Point,
    pub rotation: Millideg,
    pub length: Nm,
    pub name_visible: bool,
    pub number_visible: bool,
}

#[derive(Clone, Debug)]
pub struct GraphicSource {
    /// 0 for every Unit.
    pub unit: u32,
    pub shape: Shape,
    pub width: Nm,
    pub fill: bool,
}

/// The symbol as read, before UUIDs are handed out.
#[derive(Clone, Debug)]
pub struct SymbolSource {
    pub name: String,
    /// Every property in file order, the inherited ones of a derived symbol first.
    pub properties: Vec<(String, String)>,
    pub unit_count: u32,
    pub pins: Vec<PinSource>,
    pub graphics: Vec<GraphicSource>,
    pub warnings: Vec<String>,
}

impl SymbolSource {
    fn property(&self, key: &str) -> Option<&str> {
        self.properties
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
}

/// Reads the symbol called `pick`, or the only one, from the library `root`.
pub fn read(root: &Sexp, pick: Option<&str>) -> Read<SymbolSource> {
    if root.head() != Some("kicad_symbol_lib") {
        return fail(
            root,
            "this is not a KiCad symbol library (`kicad_symbol_lib`)",
        );
    }
    let version = match root.child("version").and_then(|v| v.arg(0)) {
        Some(v) => v.parse::<u32>().ok(),
        None => {
            return fail(
                root,
                "the symbol library has no version; Import part needs a KiCad 6 or later \
                 `.kicad_sym`",
            );
        }
    };
    match version {
        Some(v) if v >= MIN_SYMBOL_VERSION => {}
        _ => {
            return fail(
                root,
                format!(
                    "the symbol library's version is older than {MIN_SYMBOL_VERSION}; \
                     Import part needs a KiCad 6 or later `.kicad_sym`"
                ),
            );
        }
    }
    let symbols: Vec<&Sexp> = root.children("symbol").collect();
    let names: Vec<&str> = symbols.iter().filter_map(|s| s.arg(0)).collect();
    let chosen = match pick {
        Some(name) => symbols.iter().find(|s| s.arg(0) == Some(name)).copied(),
        None if symbols.len() == 1 => Some(symbols[0]),
        None if symbols.is_empty() => return fail(root, "the symbol library holds no symbol"),
        None => {
            return fail(
                root,
                format!(
                    "the symbol library holds {} symbols; pick one of {}",
                    symbols.len(),
                    names.join(", ")
                ),
            );
        }
    };
    let Some(chosen) = chosen else {
        return fail(
            root,
            format!(
                "the symbol library has no symbol {:?}; it has {}",
                pick.unwrap_or(""),
                names.join(", ")
            ),
        );
    };
    let name = node::text(chosen, 0)?.to_owned();

    let mut source = SymbolSource {
        name,
        properties: vec![],
        unit_count: 1,
        pins: vec![],
        graphics: vec![],
        warnings: vec![],
    };
    // A derived symbol draws with its parent's body and overrides its properties.
    let body = match chosen.child("extends") {
        Some(ext) => {
            let parent_name = node::text(ext, 0)?;
            let Some(parent) = symbols.iter().find(|s| s.arg(0) == Some(parent_name)) else {
                return fail(
                    ext,
                    format!("the symbol extends {parent_name:?}, which the library lacks"),
                );
            };
            if parent.child("extends").is_some() {
                return fail(ext, "a symbol may extend only a symbol that extends none");
            }
            read_properties(parent, &mut source.properties)?;
            *parent
        }
        None => chosen,
    };
    read_properties(chosen, &mut source.properties)?;

    let names_hidden = body.child("pin_names").is_some_and(|p| p.flag("hide"));
    let numbers_hidden = body.child("pin_numbers").is_some_and(|p| p.flag("hide"));
    for sub in body.children("symbol") {
        let sub_name = node::text(sub, 0)?;
        let Some((unit, style)) = unit_and_style(sub_name) else {
            source.warnings.push(format!(
                "skipped sub-symbol {sub_name:?}, whose name has no unit"
            ));
            continue;
        };
        if style > 1 {
            continue;
        }
        if unit > MAX_UNITS {
            return fail(sub, format!("a symbol may have at most {MAX_UNITS} units"));
        }
        source.unit_count = source.unit_count.max(unit);
        for item in sub.args() {
            match item.head() {
                Some("pin") => {
                    source
                        .pins
                        .push(read_pin(item, unit, names_hidden, numbers_hidden)?)
                }
                Some(_) => read_graphic(item, unit, &mut source)?,
                None => {}
            }
        }
    }
    Ok(source)
}

fn read_properties(symbol: &Sexp, into: &mut Vec<(String, String)>) -> Read<()> {
    for p in symbol.children("property") {
        let key = node::text(p, 0)?.to_owned();
        let value = node::text(p, 1)?.to_owned();
        match into.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = value,
            None => into.push((key, value)),
        }
    }
    Ok(())
}

/// The unit and body style of a sub-symbol called `<name>_<unit>_<style>`.
fn unit_and_style(name: &str) -> Option<(u32, u32)> {
    let mut parts = name.rsplitn(3, '_');
    let style = parts.next()?.parse().ok()?;
    let unit = parts.next()?.parse().ok()?;
    parts.next()?;
    Some((unit, style))
}

fn pin_type(word: &str) -> Option<PinType> {
    Some(match word {
        "input" => PinType::Input,
        "output" => PinType::Output,
        "bidirectional" => PinType::Bidirectional,
        "tri_state" => PinType::TriState,
        "passive" | "free" | "unspecified" => PinType::Passive,
        "power_in" => PinType::PowerIn,
        "power_out" => PinType::PowerOut,
        "open_collector" => PinType::OpenCollector,
        "open_emitter" => PinType::OpenEmitter,
        "no_connect" | "unconnected" => PinType::NotConnected,
        _ => return None,
    })
}

fn read_pin(pin: &Sexp, unit: u32, names_hidden: bool, numbers_hidden: bool) -> Read<PinSource> {
    let word = node::text(pin, 0)?;
    let Some(electrical_type) = pin_type(word) else {
        return fail(pin, format!("unknown pin type {word:?}"));
    };
    let at = node::need(pin, "at")?;
    let name = node::text(node::need(pin, "name")?, 0)?;
    let number = node::text(node::need(pin, "number")?, 0)?;
    let hidden = pin.flag("hide");
    Ok(PinSource {
        unit,
        name: if name == "~" {
            String::new()
        } else {
            name.into()
        },
        number: number.into(),
        electrical_type,
        at: node::point(at)?,
        rotation: node::degrees_or_zero(at, 2)?,
        length: node::mm(node::need(pin, "length")?, 0)?,
        name_visible: !hidden && !names_hidden,
        number_visible: !hidden && !numbers_hidden,
    })
}

fn read_graphic(item: &Sexp, unit: u32, source: &mut SymbolSource) -> Read<()> {
    let width = match node::width(item)? {
        Some(w) if w.0 > 0 => w,
        _ => DEFAULT_WIDTH,
    };
    let fill = item
        .child("fill")
        .and_then(|f| f.child("type"))
        .and_then(|t| t.arg(0))
        .is_some_and(|t| t != "none");
    let shapes = match item.head() {
        Some("rectangle") => vec![Shape::Rect {
            start: node::point_of(item, "start")?,
            end: node::point_of(item, "end")?,
        }],
        Some("circle") => vec![Shape::Circle {
            center: node::point_of(item, "center")?,
            radius: node::mm(node::need(item, "radius")?, 0)?,
        }],
        Some("arc") => vec![read_arc(item)?],
        Some("polyline") => shapes::polyline(node::points(item)?, fill),
        Some("text") => {
            let at = node::need(item, "at")?;
            // Symbol text angles are in tenths of a degree.
            let tenths = node::degrees_or_zero(at, 2)?;
            vec![Shape::Text {
                at: node::point(at)?,
                text: node::text(item, 0)?.to_owned(),
                size: node::font_size(item)?.unwrap_or(Nm(1_270_000)),
                rotation: Millideg(tenths.0 / 10),
            }]
        }
        Some(other) => {
            source.warnings.push(format!(
                "skipped a symbol `{other}`, which REDA cannot draw"
            ));
            return Ok(());
        }
        None => return Ok(()),
    };
    for shape in shapes {
        source.graphics.push(GraphicSource {
            unit,
            shape,
            width,
            fill,
        });
    }
    Ok(())
}

/// `(arc (start) (mid) (end))`, or the early KiCad 6 form with a `(radius (at …))` centre.
fn read_arc(item: &Sexp) -> Read<Shape> {
    let start = node::point_of(item, "start")?;
    let end = node::point_of(item, "end")?;
    if let Some(mid) = item.child("mid") {
        return Ok(shapes::arc_through(start, node::point(mid)?, end));
    }
    let center = node::point_of(node::need(item, "radius")?, "at")?;
    let angle = |p: Point| ((p.y.0 - center.y.0) as f64).atan2((p.x.0 - center.x.0) as f64);
    let mut sweep = (angle(end) - angle(start)).to_degrees();
    if sweep > 180.0 {
        sweep -= 360.0;
    } else if sweep <= -180.0 {
        sweep += 360.0;
    }
    Ok(Shape::Arc {
        center,
        start,
        sweep: Millideg((sweep * 1000.0).round() as i64),
    })
}

/// The Designator prefix from the `Reference` property: `U?` and `U1` both give `U`.
fn designator_prefix(reference: Option<&str>) -> String {
    let prefix = reference
        .unwrap_or("")
        .trim_end_matches(|c: char| c == '?' || c.is_ascii_digit());
    if prefix.is_empty() {
        "U".into()
    } else {
        prefix.into()
    }
}

fn unit_name(n: u32) -> String {
    match n {
        1..=26 => char::from(b'A' + (n - 1) as u8).to_string(),
        _ => format!("U{n}"),
    }
}

pub fn build(
    source: &SymbolSource,
    name: String,
    provenance: Provenance,
    new_id: &mut dyn FnMut() -> Uuid,
) -> Symbol {
    let uuid = new_id();
    let units: Vec<Unit> = (1..=source.unit_count)
        .map(|n| Unit {
            uuid: new_id(),
            name: unit_name(n),
        })
        .collect();
    // Unit 0 is common to all Units: its Pins go on the first, its drawing on every one.
    let unit_of = |n: u32| units[n.max(1) as usize - 1].uuid;
    let mut pins: Vec<Pin> = source
        .pins
        .iter()
        .map(|p| Pin {
            uuid: new_id(),
            unit: unit_of(p.unit),
            name: p.name.clone(),
            number: p.number.clone(),
            electrical_type: p.electrical_type,
            at: p.at,
            rotation: p.rotation,
            length: p.length,
            name_visible: p.name_visible,
            number_visible: p.number_visible,
            description: String::new(),
            inside_edge: None,
            outside_edge: None,
        })
        .collect();
    pins.sort_by_key(|p| p.uuid);
    let mut graphics = vec![];
    for g in &source.graphics {
        let targets: Vec<Uuid> = if g.unit == 0 {
            units.iter().map(|u| u.uuid).collect()
        } else {
            vec![unit_of(g.unit)]
        };
        for unit in targets {
            graphics.push(SymbolGraphic {
                uuid: new_id(),
                unit,
                shape: g.shape.clone(),
                width: g.width,
                fill: g.fill,
            });
        }
    }
    graphics.sort_by_key(|g| g.uuid);
    let description = source
        .property("Description")
        .or(source.property("ki_description"))
        .unwrap_or("")
        .to_owned();
    Symbol {
        uuid,
        name,
        description,
        designator_prefix: designator_prefix(source.property("Reference")),
        units,
        pins,
        graphics,
        provenance: Some(Box::new(provenance)),
    }
}
