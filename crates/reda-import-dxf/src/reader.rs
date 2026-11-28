//! The entity layer: the HEADER variables import uses and the ENTITIES section's model
//! space, read from the tag pairs (DXF Reference, "HEADER Section" and "ENTITIES Section").
//! BLOCKS, TABLES and OBJECTS are read past.

use std::collections::BTreeMap;

use crate::shape::{Shape, Vertex, Xy};
use crate::tags::{ReadError, Tag, decode, tags};

/// What a DXF file holds that import uses.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Drawing {
    /// `$INSUNITS` as written, if the HEADER has it.
    pub insunits: Option<i64>,
    /// The model-space entities read, in file order.
    pub entities: Vec<Entity>,
    /// How many entities of each other type were read past, by type name, such as
    /// `HATCH`. A readable entity in paper space counts as `<TYPE> in paper space`.
    pub skipped: BTreeMap<String, usize>,
}

/// One read entity and the DXF layer it is on.
#[derive(Clone, Debug, PartialEq)]
pub struct Entity {
    pub layer: String,
    pub shape: Shape,
}

/// Reads an ASCII DXF file.
pub fn read(bytes: &[u8]) -> Result<Drawing, ReadError> {
    let text = decode(bytes);
    let tags = tags(&text)?;
    let mut drawing = Drawing::default();
    for section in sections(&tags)? {
        match section.name {
            "HEADER" => drawing.insunits = header_int(section.tags, "$INSUNITS")?,
            "ENTITIES" => read_entities(section.tags, &mut drawing)?,
            _ => {}
        }
    }
    Ok(drawing)
}

struct Section<'t, 'a> {
    name: &'a str,
    tags: &'t [Tag<'a>],
}

/// Every `SECTION` … `ENDSEC`, its tags after the name.
fn sections<'t, 'a>(tags: &'t [Tag<'a>]) -> Result<Vec<Section<'t, 'a>>, ReadError> {
    let mut sections = Vec::new();
    let mut i = 0;
    while i < tags.len() {
        let tag = tags[i];
        if !(tag.code == 0 && tag.text() == "SECTION") {
            i += 1;
            continue;
        }
        let name = match tags.get(i + 1) {
            Some(t) if t.code == 2 => t.text(),
            _ => {
                return Err(ReadError {
                    line: tag.line,
                    message: "a SECTION has no name".into(),
                });
            }
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

fn read_entities(tags: &[Tag<'_>], drawing: &mut Drawing) -> Result<(), ReadError> {
    let starts: Vec<usize> = (0..tags.len()).filter(|&i| tags[i].code == 0).collect();
    for (n, &start) in starts.iter().enumerate() {
        let end = starts.get(n + 1).copied().unwrap_or(tags.len());
        let kind = tags[start].text();
        let groups = &tags[start + 1..end];
        let shape = match kind {
            "LINE" => Some(line(groups)?),
            "ARC" => Some(arc(groups)?),
            "CIRCLE" => Some(circle(groups)?),
            "LWPOLYLINE" => Some(lwpolyline(groups)?),
            // The vertices and end of an R12 POLYLINE, counted with it.
            "VERTEX" | "SEQEND" => continue,
            _ => None,
        };
        let paper_space = groups.iter().any(|t| t.code == 67 && t.int() == Ok(1));
        match shape {
            Some(shape) if !paper_space => drawing.entities.push(Entity {
                layer: groups
                    .iter()
                    .find(|t| t.code == 8)
                    .map_or("0", |t| t.text())
                    .to_owned(),
                shape,
            }),
            Some(_) => {
                *drawing
                    .skipped
                    .entry(format!("{kind} in paper space"))
                    .or_default() += 1
            }
            None => *drawing.skipped.entry(kind.to_owned()).or_default() += 1,
        }
    }
    Ok(())
}

/// The last value of each real group in `codes`, 0 where a group is missing.
fn reals<const N: usize>(groups: &[Tag<'_>], codes: [i32; N]) -> Result<[f64; N], ReadError> {
    let mut values = [0.0; N];
    for tag in groups {
        if let Some(i) = codes.iter().position(|c| *c == tag.code) {
            values[i] = tag.real()?;
        }
    }
    Ok(values)
}

fn line(groups: &[Tag<'_>]) -> Result<Shape, ReadError> {
    let [x1, y1, x2, y2] = reals(groups, [10, 20, 11, 21])?;
    Ok(Shape::Line {
        from: Xy::new(x1, y1),
        to: Xy::new(x2, y2),
    })
}

fn arc(groups: &[Tag<'_>]) -> Result<Shape, ReadError> {
    let [x, y, radius, start, end] = reals(groups, [10, 20, 40, 50, 51])?;
    Ok(Shape::Arc {
        centre: Xy::new(x, y),
        radius,
        start,
        end,
    })
}

fn circle(groups: &[Tag<'_>]) -> Result<Shape, ReadError> {
    let [x, y, radius] = reals(groups, [10, 20, 40])?;
    Ok(Shape::Circle {
        centre: Xy::new(x, y),
        radius,
    })
}

/// LWPOLYLINE: each 10 starts a vertex; the 20 and 42 after it are its y and bulge.
fn lwpolyline(groups: &[Tag<'_>]) -> Result<Shape, ReadError> {
    let mut vertices: Vec<Vertex> = Vec::new();
    let mut closed = false;
    for tag in groups {
        match tag.code {
            70 => closed = tag.int()? & 1 == 1,
            10 => vertices.push(Vertex {
                at: Xy::new(tag.real()?, 0.0),
                bulge: 0.0,
            }),
            20 | 42 => {
                let Some(vertex) = vertices.last_mut() else {
                    return Err(ReadError {
                        line: tag.line,
                        message: format!("group {} comes before a vertex's 10", tag.code),
                    });
                };
                if tag.code == 20 {
                    vertex.at.y = tag.real()?;
                } else {
                    vertex.bulge = tag.real()?;
                }
            }
            _ => {}
        }
    }
    Ok(Shape::Polyline { vertices, closed })
}
