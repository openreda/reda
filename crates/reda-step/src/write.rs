//! Writing: a [`StepModel`] to AP214 Part 21 text, plain or gzip.

use std::io::Write as _;
use std::path::Path;

use crate::model::{AP214_SCHEMA, StepModel, to_ap214};
use crate::part21::encode_string;
use crate::records::Record;

/// How a written file is stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compression {
    /// `.step` and `.stp`: Part 21 text.
    Plain,
    /// `.stpz`: the text gzipped.
    Gzip,
}

impl Compression {
    /// `.stpz` (any case) is gzip; every other name is plain text.
    pub fn for_path(path: impl AsRef<Path>) -> Compression {
        match path.as_ref().extension().and_then(|e| e.to_str()) {
            Some(e) if e.eq_ignore_ascii_case("stpz") => Compression::Gzip,
            _ => Compression::Plain,
        }
    }
}

/// Writes `model` as an AP214 Part 21 file. The header is the model's, with AP214 as the
/// schema; records that name a protocol are written as AP214's; record `i` is `#(i + 1)`.
pub fn write(model: &StepModel) -> String {
    let h = &model.header;
    let mut out = String::with_capacity(64 * model.records().len() + 512);
    out.push_str("ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(");
    strings(&h.description, &mut out);
    out.push_str(",'2;1');\nFILE_NAME(");
    encode_string(&h.name, &mut out);
    out.push(',');
    encode_string(&h.time_stamp, &mut out);
    out.push(',');
    strings(&h.author, &mut out);
    out.push(',');
    strings(&h.organization, &mut out);
    for s in [
        &h.preprocessor_version,
        &h.originating_system,
        &h.authorization,
    ] {
        out.push(',');
        encode_string(s, &mut out);
    }
    out.push_str(");\nFILE_SCHEMA((");
    encode_string(AP214_SCHEMA, &mut out);
    out.push_str("));\nENDSEC;\nDATA;\n");
    for (i, record) in model.records().iter().enumerate() {
        out.push('#');
        out.push_str(&(i + 1).to_string());
        out.push('=');
        match record {
            Record::ApplicationContext(_) | Record::ApplicationProtocolDefinition(_) => {
                let mut r = record.clone();
                to_ap214(&mut r);
                r.write(&mut out);
            }
            _ => record.write(&mut out),
        }
        out.push_str(";\n");
    }
    out.push_str("ENDSEC;\nEND-ISO-10303-21;\n");
    out
}

/// Writes `model` as [`write`] does, gzipped when `compression` says so.
pub fn encode(model: &StepModel, compression: Compression) -> Vec<u8> {
    let text = write(model);
    match compression {
        Compression::Plain => text.into_bytes(),
        Compression::Gzip => {
            let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            gz.write_all(text.as_bytes()).expect("writing to memory");
            gz.finish().expect("writing to memory")
        }
    }
}

/// Writes `model` to `path`, gzipped when the name ends in `.stpz`.
pub fn write_file(model: &StepModel, path: impl AsRef<Path>) -> std::io::Result<()> {
    let path = path.as_ref();
    std::fs::write(path, encode(model, Compression::for_path(path)))
}

/// A header list of strings. The lists hold at least one string, so an empty one is `('')`.
fn strings(items: &[String], out: &mut String) {
    if items.is_empty() {
        out.push_str("('')");
        return;
    }
    out.push('(');
    for (i, s) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        encode_string(s, out);
    }
    out.push(')');
}
