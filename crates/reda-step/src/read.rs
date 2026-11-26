//! Reading: bytes (plain or gzip) to a [`StepModel`].

use std::collections::HashMap;
use std::io::Read as _;

use crate::model::{Header, Protocol, StepModel};
use crate::part21::{self, Instance, Param, Part};
use crate::records::Record;

/// A model as read, with the protocol its file declared and what the reader had to leave out.
#[derive(Clone, Debug)]
pub struct Read {
    pub model: StepModel,
    pub protocol: Protocol,
    /// One line for each instance that was skipped (malformed, a duplicate id, a reference
    /// to an instance that is not there).
    pub warnings: Vec<String>,
}

/// Why a file could not be read at all.
#[derive(Clone, Debug, PartialEq)]
pub struct ReadError {
    pub message: String,
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ReadError {}

/// A gzip file is not inflated past this many bytes (the largest real models are tens of MB).
const MAX_INFLATED: u64 = 1 << 30;

/// Reads a STEP file's bytes: `.step` and `.stp` text, or gzip (`.stpz`), told apart by the
/// gzip magic number rather than the name.
pub fn read(bytes: &[u8]) -> Result<Read, ReadError> {
    let inflated;
    let text = if bytes.starts_with(&[0x1F, 0x8B]) {
        inflated = gunzip(bytes)?;
        inflated.as_slice()
    } else {
        bytes
    };
    let exchange = part21::parse(text).map_err(|e| ReadError {
        message: e.to_string(),
    })?;
    let (header, protocol) = header(&exchange.header);
    let mut warnings = exchange.skipped;
    let records = records(exchange.data, &mut warnings);
    let mut model = StepModel::from_records(header, records);
    model.prune();
    Ok(Read {
        model,
        protocol,
        warnings,
    })
}

/// Reads a STEP file from disk.
pub fn read_file(path: impl AsRef<std::path::Path>) -> Result<Read, ReadError> {
    let path = path.as_ref();
    let bytes = std::fs::read(path).map_err(|e| ReadError {
        message: format!("{}: {e}", path.display()),
    })?;
    read(&bytes)
}

fn gunzip(bytes: &[u8]) -> Result<Vec<u8>, ReadError> {
    let mut out = Vec::new();
    flate2::read::MultiGzDecoder::new(bytes)
        .take(MAX_INFLATED + 1)
        .read_to_end(&mut out)
        .map_err(|e| ReadError {
            message: format!("gzip: {e}"),
        })?;
    if out.len() as u64 > MAX_INFLATED {
        return Err(ReadError {
            message: "gzip: inflates to more than 1 GiB".into(),
        });
    }
    Ok(out)
}

fn header(parts: &[Part]) -> (Header, Protocol) {
    let mut h = Header::default();
    let mut protocol = Protocol::Other(String::new());
    let text = |p: Option<&Param>| match p {
        Some(Param::String(s)) => s.clone(),
        _ => String::new(),
    };
    // A list holding one empty string is how a file says "none" (the lists may not be empty).
    let texts = |p: Option<&Param>| {
        let items: Vec<String> = match p {
            Some(Param::List(items)) => items.iter().map(|i| text(Some(i))).collect(),
            Some(Param::String(s)) => vec![s.clone()],
            _ => Vec::new(),
        };
        if items.len() == 1 && items[0].is_empty() {
            Vec::new()
        } else {
            items
        }
    };
    for part in parts {
        let p = &part.params;
        match part.name.as_str() {
            "FILE_DESCRIPTION" => h.description = texts(p.first()),
            "FILE_NAME" => {
                h.name = text(p.first());
                h.time_stamp = text(p.get(1));
                h.author = texts(p.get(2));
                h.organization = texts(p.get(3));
                h.preprocessor_version = text(p.get(4));
                h.originating_system = text(p.get(5));
                h.authorization = text(p.get(6));
            }
            "FILE_SCHEMA" => {
                if let Some(first) = texts(p.first()).first() {
                    protocol = Protocol::from_schema(first);
                }
            }
            _ => {}
        }
    }
    (h, protocol)
}

/// At most this many warnings name single instances; the rest are counted.
const MAX_WARNINGS: usize = 50;

/// Turns instances into records: drops duplicate ids and instances that depend on missing
/// ones, resolves `#n` references to record indices, and types each record.
fn records(data: Vec<Instance>, warnings: &mut Vec<String>) -> Vec<Record> {
    let mut by_id: HashMap<u64, usize> = HashMap::with_capacity(data.len());
    let mut instances = Vec::with_capacity(data.len());
    for instance in data {
        if by_id.contains_key(&instance.id) {
            warn(
                warnings,
                format!(
                    "#{} (line {}) skipped: the id is used twice",
                    instance.id, instance.line
                ),
            );
            continue;
        }
        by_id.insert(instance.id, instances.len());
        instances.push(instance);
    }

    // An instance whose single-valued reference (not a list element) points at nothing is
    // dropped, and so in turn is anything that needs it. A missing list element is only
    // left out of its list, so one bad face costs the shell that face and no more.
    let n = instances.len();
    let mut alive = vec![true; n];
    let mut needed_by: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut stack = Vec::new();
    for (i, instance) in instances.iter().enumerate() {
        let mut missing = None;
        for_each_hard_ref(instance, &mut |id| match by_id.get(&id) {
            Some(&j) => needed_by[j].push(i),
            None => missing = missing.or(Some(id)),
        });
        if let Some(id) = missing {
            alive[i] = false;
            warn(
                warnings,
                format!(
                    "#{} skipped: it refers to #{id}, which is not in the file",
                    instance.id
                ),
            );
            stack.push(i);
        }
    }
    while let Some(j) = stack.pop() {
        for &i in &needed_by[j] {
            if alive[i] {
                alive[i] = false;
                let (a, b) = (instances[i].id, instances[j].id);
                warn(
                    warnings,
                    format!("#{a} skipped: it refers to #{b}, which was skipped"),
                );
                stack.push(i);
            }
        }
    }

    let mut index = HashMap::with_capacity(n);
    for (i, instance) in instances.iter().enumerate() {
        if alive[i] {
            index.insert(instance.id, index.len() as u64);
        }
    }
    let records: Vec<Record> = instances
        .into_iter()
        .zip(alive)
        .filter(|(_, a)| *a)
        .map(|(instance, _)| {
            let parts = instance
                .parts
                .into_iter()
                .map(|part| Part {
                    name: part.name,
                    params: part
                        .params
                        .into_iter()
                        .map(|p| resolve(p, &index, true))
                        .collect(),
                })
                .collect();
            Record::from_instance(parts, instance.complex)
        })
        .collect();
    if warnings.len() > MAX_WARNINGS {
        let more = warnings.len() - MAX_WARNINGS;
        warnings.truncate(MAX_WARNINGS);
        warnings.push(format!("and {more} more"));
    }
    records
}

fn warn(warnings: &mut Vec<String>, message: String) {
    warnings.push(message);
}

/// Calls `f` with every reference of `instance` that is not a direct element of a top-level
/// list parameter.
fn for_each_hard_ref(instance: &Instance, f: &mut dyn FnMut(u64)) {
    fn walk(p: &Param, f: &mut dyn FnMut(u64)) {
        match p {
            Param::Ref(r) => f(*r),
            Param::List(items) => items.iter().for_each(|i| walk(i, f)),
            Param::Typed(_, inner) => walk(inner, f),
            _ => {}
        }
    }
    for p in instance.parts.iter().flat_map(|p| &p.params) {
        match p {
            Param::List(items) => {
                for item in items {
                    if !matches!(item, Param::Ref(_)) {
                        walk(item, f);
                    }
                }
            }
            p => walk(p, f),
        }
    }
}

/// Maps `#n` to its record index, leaving out list elements that refer to dropped
/// instances (only a top-level list's direct elements can, by [`for_each_hard_ref`]).
fn resolve(p: Param, index: &HashMap<u64, u64>, top: bool) -> Param {
    match p {
        Param::Ref(r) => Param::Ref(*index.get(&r).expect("hard references resolve")),
        Param::List(items) => Param::List(
            items
                .into_iter()
                .filter(|i| !(top && matches!(i, Param::Ref(r) if !index.contains_key(r))))
                .map(|i| resolve(i, index, false))
                .collect(),
        ),
        Param::Typed(name, inner) => Param::Typed(name, Box::new(resolve(*inner, index, false))),
        p => p,
    }
}
