//! The entity layer: the HEADER variables import uses, the BLOCKS an INSERT draws, and the
//! ENTITIES section's model space, read from the tag pairs (DXF Reference, "HEADER
//! Section", "BLOCKS Section" and "ENTITIES Section"). TABLES and OBJECTS are read past.
//! Each entity's geometry is in [`entities`].

mod entities;

use std::collections::{BTreeMap, HashMap};

use crate::shape::Shape;
use crate::tags::{ReadError, Tag, tags};
use crate::transform::{Affine, V3};

use entities::Read;

/// What a DXF file holds that import uses.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Drawing {
    /// `$INSUNITS` as written, if the HEADER has it.
    pub insunits: Option<i64>,
    /// The model-space entities read, in file order, each INSERT exploded in place into
    /// its block's entities.
    pub entities: Vec<Entity>,
    /// How many entities of each other type were read past, by type name, such as
    /// `HATCH`. A readable entity in paper space counts as `<TYPE> in paper space`, and one
    /// that can't be drawn says why, such as `malformed SPLINE`.
    pub skipped: BTreeMap<String, usize>,
    /// What may have been read wrongly, such as text in a code page import doesn't decode.
    pub warnings: Vec<String>,
}

/// One read entity and the DXF layer it is on.
#[derive(Clone, Debug, PartialEq)]
pub struct Entity {
    pub layer: String,
    pub shape: Shape,
}

/// The most entities, read and skipped, a file may explode into through its INSERTs.
const MAX_ENTITIES: usize = 1_000_000;
/// The deepest INSERTs may nest.
const MAX_NESTING: usize = 64;

/// Reads a DXF file, ASCII or binary.
pub fn read(bytes: &[u8]) -> Result<Drawing, ReadError> {
    let (tags, warning) = tags(bytes)?;
    let sections = sections(&tags)?;
    let mut reader = Reader {
        blocks: HashMap::new(),
        drawing: Drawing {
            warnings: warning.into_iter().collect(),
            ..Drawing::default()
        },
        visited: 0,
    };
    for section in sections.iter().filter(|s| s.name == "BLOCKS") {
        read_blocks(section.tags, &mut reader.blocks);
    }
    for section in &sections {
        match section.name {
            "HEADER" => reader.drawing.insunits = header_int(section.tags, "$INSUNITS")?,
            "ENTITIES" => {
                let top = Frame {
                    map: Affine::IDENTITY,
                    layer: None,
                    blocks: Vec::new(),
                };
                for record in records(section.tags) {
                    reader.draw(&record, &top)?;
                }
            }
            _ => {}
        }
    }
    Ok(reader.drawing)
}

struct Section<'t, 'a> {
    name: &'t str,
    tags: &'t [Tag<'a>],
}

/// Every `SECTION` … `ENDSEC`, its tags after the name.
fn sections<'t, 'a>(tags: &'t [Tag<'a>]) -> Result<Vec<Section<'t, 'a>>, ReadError> {
    let mut sections = Vec::new();
    let mut i = 0;
    while i < tags.len() {
        let tag = &tags[i];
        if !(tag.code == 0 && tag.text() == "SECTION") {
            i += 1;
            continue;
        }
        let name = match tags.get(i + 1) {
            Some(t) if t.code == 2 => t.text(),
            _ => return Err(tag.error("a SECTION has no name")),
        };
        let start = i + 2;
        let end = (start..tags.len())
            .find(|&j| tags[j].code == 0 && tags[j].text() == "ENDSEC")
            .unwrap_or(tags.len());
        sections.push(Section {
            name,
            tags: &tags[start..end],
        });
        i = end + 1;
    }
    Ok(sections)
}

/// The first integer group after the HEADER variable `name`.
fn header_int(tags: &[Tag<'_>], name: &str) -> Result<Option<i64>, ReadError> {
    let Some(at) = tags.iter().position(|t| t.code == 9 && t.text() == name) else {
        return Ok(None);
    };
    match tags[at + 1..]
        .iter()
        .take_while(|t| t.code != 9)
        .find(|t| (60..100).contains(&t.code))
    {
        Some(tag) => tag.int().map(Some),
        None => Ok(None),
    }
}

/// One entity: its type, its groups, and the VERTEXes of a POLYLINE or the ATTRIBs of an
/// INSERT that follow it up to their SEQEND.
struct Record<'t, 'a> {
    kind: &'t str,
    groups: &'t [Tag<'a>],
    children: Vec<&'t [Tag<'a>]>,
}

/// The entities of a section or block, in order. A VERTEX, ATTRIB or SEQEND with nothing
/// before it to belong to is dropped.
fn records<'t, 'a>(tags: &'t [Tag<'a>]) -> Vec<Record<'t, 'a>> {
    let starts: Vec<usize> = (0..tags.len()).filter(|&i| tags[i].code == 0).collect();
    let entity = |n: usize| {
        let start = starts[n];
        let end = starts.get(n + 1).copied().unwrap_or(tags.len());
        (tags[start].text(), &tags[start + 1..end])
    };
    let mut records: Vec<Record<'t, 'a>> = Vec::new();
    let mut n = 0;
    while n < starts.len() {
        let (kind, groups) = entity(n);
        n += 1;
        let child = match kind {
            "POLYLINE" => Some("VERTEX"),
            "INSERT" => Some("ATTRIB"),
            "VERTEX" | "ATTRIB" | "SEQEND" => continue,
            _ => None,
        };
        let mut children = Vec::new();
        if let Some(child) = child {
            while n < starts.len() && entity(n).0 == child {
                children.push(entity(n).1);
                n += 1;
            }
            if n < starts.len() && entity(n).0 == "SEQEND" {
                n += 1;
            }
        }
        records.push(Record {
            kind,
            groups,
            children,
        });
    }
    records
}

/// A block definition: its base point and its entities.
struct Block<'t, 'a> {
    base: V3,
    external: bool,
    records: Vec<Record<'t, 'a>>,
}

/// Every `BLOCK` … `ENDBLK`, by name in upper case: block names ignore case.
fn read_blocks<'t, 'a>(tags: &'t [Tag<'a>], blocks: &mut HashMap<String, Block<'t, 'a>>) {
    let mut i = 0;
    while i < tags.len() {
        if !(tags[i].code == 0 && tags[i].text() == "BLOCK") {
            i += 1;
            continue;
        }
        let head_end = (i + 1..tags.len())
            .find(|&j| tags[j].code == 0)
            .unwrap_or(tags.len());
        let end = (head_end..tags.len())
            .find(|&j| tags[j].code == 0 && tags[j].text() == "ENDBLK")
            .unwrap_or(tags.len());
        let head = &tags[i + 1..head_end];
        let name = head
            .iter()
            .find(|t| t.code == 2)
            .map(|t| t.text().to_owned())
            .unwrap_or_default();
        // A base point that does not read is taken as the origin.
        let base = entities::point(head, 10).unwrap_or([0.0; 3]);
        let flags = head
            .iter()
            .find(|t| t.code == 70)
            .and_then(|t| t.int().ok())
            .unwrap_or(0);
        blocks.insert(
            name.to_uppercase(),
            Block {
                base,
                external: flags & 4 != 0,
                records: records(&tags[head_end..end]),
            },
        );
        i = end + 1;
    }
}

/// Where the entities being read land: through the INSERTs they sit in.
struct Frame {
    map: Affine,
    /// The layer of the INSERT, which entities on layer 0 take.
    layer: Option<String>,
    /// The blocks being drawn, outermost first.
    blocks: Vec<String>,
}

struct Reader<'t, 'a> {
    blocks: HashMap<String, Block<'t, 'a>>,
    drawing: Drawing,
    visited: usize,
}

/// The entity types the reader draws.
const READ: [&str; 10] = [
    "LINE",
    "ARC",
    "CIRCLE",
    "ELLIPSE",
    "LWPOLYLINE",
    "POLYLINE",
    "SPLINE",
    "TEXT",
    "MTEXT",
    "INSERT",
];

impl Reader<'_, '_> {
    fn skip(&mut self, why: impl Into<String>) {
        *self.drawing.skipped.entry(why.into()).or_default() += 1;
    }

    /// Counts one more entity or INSERT cell against [`MAX_ENTITIES`].
    fn visit(&mut self) -> Result<(), ReadError> {
        self.visited += 1;
        if self.visited > MAX_ENTITIES {
            return Err(ReadError::Malformed {
                at: None,
                message: format!(
                    "its INSERTs explode into more than {MAX_ENTITIES} entities, more than a \
                     board drawing holds"
                ),
            });
        }
        Ok(())
    }

    fn draw(&mut self, record: &Record<'_, '_>, frame: &Frame) -> Result<(), ReadError> {
        self.visit()?;
        let kind = record.kind;
        let own = record
            .groups
            .iter()
            .find(|t| t.code == 8)
            .map_or("0", |t| t.text());
        let layer = match (&frame.layer, own) {
            (Some(insert), "0") => insert.clone(),
            _ => own.to_owned(),
        };
        let paper_space = frame.blocks.is_empty()
            && record
                .groups
                .iter()
                .any(|t| t.code == 67 && t.int() == Ok(1));
        if !READ.contains(&kind) {
            self.skip(kind);
            return Ok(());
        }
        if paper_space {
            self.skip(format!("{kind} in paper space"));
            return Ok(());
        }
        if kind == "INSERT" {
            return self.insert(record, frame, layer);
        }
        match entities::read(kind, record.groups, &record.children, &frame.map)? {
            Read::Shapes(shapes) => self
                .drawing
                .entities
                .extend(shapes.into_iter().map(|shape| Entity {
                    layer: layer.clone(),
                    shape,
                })),
            Read::Skipped(why) => self.skip(why),
        }
        Ok(())
    }

    /// INSERT: the block's entities, scaled, turned and moved onto the insertion point, once
    /// per cell of its rows and columns. Its ATTRIBs are skipped.
    fn insert(
        &mut self,
        record: &Record<'_, '_>,
        frame: &Frame,
        layer: String,
    ) -> Result<(), ReadError> {
        for _ in &record.children {
            self.skip("ATTRIB");
        }
        let g = entities::groups(record.groups);
        let name = g
            .iter()
            .find(|t| t.code == 2)
            .map(|t| t.text().to_uppercase())
            .unwrap_or_default();
        if frame.blocks.contains(&name) {
            self.skip("INSERT of a block inside itself");
            return Ok(());
        }
        let Some(block) = self.blocks.get(&name) else {
            self.skip("INSERT of a missing block");
            return Ok(());
        };
        if block.external {
            self.skip("INSERT of an external reference");
            return Ok(());
        }
        if frame.blocks.len() >= MAX_NESTING {
            self.skip("INSERT nested too deep");
            return Ok(());
        }
        let at = entities::point(g, 10)?;
        let scale = [
            entities::real_or(g, 41, 1.0)?,
            entities::real_or(g, 42, 1.0)?,
            entities::real_or(g, 43, 1.0)?,
        ];
        let rotation = entities::real_or(g, 50, 0.0)?;
        let columns = entities::int_or(g, 70, 1)?.max(1);
        let rows = entities::int_or(g, 71, 1)?.max(1);
        let spacing = [
            entities::real_or(g, 44, 0.0)?,
            entities::real_or(g, 45, 0.0)?,
        ];
        let ocs = Affine::ocs(entities::extrusion(g)?);
        let base = block.base;
        let to_base = Affine::placed([-base[0], -base[1], -base[2]], [1.0; 3], 0.0);
        let scaled = Affine::placed([0.0; 3], scale, 0.0).of(to_base);
        let turned = Affine::placed(at, [1.0; 3], rotation);

        // The block is taken out of the map while it is drawn, and put back after.
        let block = self.blocks.remove(&name).expect("found above");
        let mut blocks = frame.blocks.clone();
        blocks.push(name.clone());
        let mut result = Ok(());
        'cells: for row in 0..rows {
            for column in 0..columns {
                result = self.visit();
                if result.is_err() {
                    break 'cells;
                }
                let cell =
                    Affine::translation([spacing[0] * column as f64, spacing[1] * row as f64, 0.0]);
                let inner = Frame {
                    map: frame.map.of(ocs).of(turned).of(cell).of(scaled),
                    layer: Some(layer.clone()),
                    blocks: blocks.clone(),
                };
                for child in &block.records {
                    result = self.draw(child, &inner);
                    if result.is_err() {
                        break 'cells;
                    }
                }
            }
        }
        self.blocks.insert(name, block);
        result
    }
}
