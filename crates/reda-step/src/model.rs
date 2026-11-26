//! [`StepModel`]: a file's header and its records.

use crate::records::{
    ApplicationContext, ApplicationProtocolDefinition, Record, RecordId, RecordKind,
};

/// The header of a Part 21 file, apart from its schema: the writer always declares AP214.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Header {
    /// `FILE_DESCRIPTION`'s description lines.
    pub description: Vec<String>,
    /// `FILE_NAME`'s fields.
    pub name: String,
    pub time_stamp: String,
    pub author: Vec<String>,
    pub organization: Vec<String>,
    pub preprocessor_version: String,
    pub originating_system: String,
    pub authorization: String,
}

/// The protocol (application protocol schema) a file declares.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Protocol {
    /// AP203, `CONFIG_CONTROL_DESIGN` or its second edition.
    Ap203,
    /// AP214, `AUTOMOTIVE_DESIGN`.
    Ap214,
    /// AP242, `AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF`.
    Ap242,
    /// Any other schema, as the file names it.
    Other(String),
}

impl Protocol {
    /// The protocol a `FILE_SCHEMA` name stands for.
    pub fn from_schema(schema: &str) -> Protocol {
        let upper = schema.trim().to_ascii_uppercase();
        let name = upper
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .next()
            .unwrap_or("");
        if name == "CONFIG_CONTROL_DESIGN" || name.starts_with("AP203") {
            Protocol::Ap203
        } else if name.starts_with("AUTOMOTIVE_DESIGN") || name.starts_with("AP214") {
            Protocol::Ap214
        } else if name.starts_with("AP242") {
            Protocol::Ap242
        } else {
            Protocol::Other(schema.trim().to_string())
        }
    }
}

/// The `FILE_SCHEMA` name the writer declares.
pub const AP214_SCHEMA: &str = "AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }";
/// The `APPLICATION_CONTEXT` text of AP214.
pub const AP214_APPLICATION: &str = "core data for automotive mechanical design processes";

/// A STEP model: the header and the records, in the order they are written. Record `i` is
/// written as `#(i + 1)`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StepModel {
    pub header: Header,
    records: Vec<Record>,
}

impl StepModel {
    pub fn new(header: Header) -> Self {
        StepModel {
            header,
            records: Vec::new(),
        }
    }

    pub(crate) fn from_records(header: Header, records: Vec<Record>) -> Self {
        StepModel { header, records }
    }

    pub fn records(&self) -> &[Record] {
        &self.records
    }

    /// The record `id` names. Panics on an id from another model.
    pub fn record(&self, id: RecordId) -> &Record {
        &self.records[id.index()]
    }

    pub fn record_mut(&mut self, id: RecordId) -> &mut Record {
        &mut self.records[id.index()]
    }

    /// Record `id` as a `T`, or `None` when it is another type (or not in the model).
    pub fn get<T: RecordKind>(&self, id: RecordId) -> Option<&T> {
        self.records.get(id.index()).and_then(T::of)
    }

    pub fn get_mut<T: RecordKind>(&mut self, id: RecordId) -> Option<&mut T> {
        self.records.get_mut(id.index()).and_then(T::of_mut)
    }

    /// Every record of type `T`, with its id, in model order.
    pub fn iter<T: RecordKind>(&self) -> impl Iterator<Item = (RecordId, &T)> {
        self.records
            .iter()
            .enumerate()
            .filter_map(|(i, r)| T::of(r).map(|t| (RecordId::from_index(i), t)))
    }

    /// Every record with its id.
    pub fn ids(&self) -> impl Iterator<Item = (RecordId, &Record)> {
        self.records
            .iter()
            .enumerate()
            .map(|(i, r)| (RecordId::from_index(i), r))
    }

    /// Adds a record and returns its id.
    pub fn add(&mut self, record: impl Into<Record>) -> RecordId {
        self.records.push(record.into());
        RecordId::from_index(self.records.len() - 1)
    }

    /// Appends every record of `other`, with its references moved along, and returns the
    /// id each of `other`'s records has now: `other`'s record `i` becomes `map(i)`.
    pub fn append(&mut self, other: &StepModel) -> impl Fn(RecordId) -> RecordId + use<> {
        let offset = self.records.len() as u32;
        let shift = move |id: RecordId| RecordId(id.0 + offset);
        for record in &other.records {
            let mut record = record.clone();
            record.refs_mut(&mut |id| *id = shift(*id));
            self.records.push(record);
        }
        shift
    }

    /// Drops every record that nothing reaches from the model's roots (product structure,
    /// representations and styled items), and renumbers the rest in their order.
    pub fn prune(&mut self) {
        let n = self.records.len();
        let mut keep = vec![false; n];
        let mut stack: Vec<usize> = (0..n).filter(|&i| self.records[i].is_root()).collect();
        for &i in &stack {
            keep[i] = true;
        }
        while let Some(i) = stack.pop() {
            self.records[i].refs(&mut |id| {
                let j = id.index();
                if j < n && !keep[j] {
                    keep[j] = true;
                    stack.push(j);
                }
            });
        }
        let mut new_index = vec![u32::MAX; n];
        let mut next = 0u32;
        for i in 0..n {
            if keep[i] {
                new_index[i] = next;
                next += 1;
            }
        }
        let old = std::mem::take(&mut self.records);
        self.records = old
            .into_iter()
            .zip(&keep)
            .filter(|(_, k)| **k)
            .map(|(mut r, _)| {
                r.refs_mut(&mut |id| *id = RecordId(new_index[id.index()]));
                r
            })
            .collect();
    }

    /// Rewrites the records that name a protocol (`APPLICATION_CONTEXT` and
    /// `APPLICATION_PROTOCOL_DEFINITION`) to AP214, the protocol the writer declares.
    pub fn to_ap214(&mut self) {
        for record in &mut self.records {
            to_ap214(record);
        }
    }
}

/// Rewrites one record to AP214 when it names a protocol; returns whether it did.
pub(crate) fn to_ap214(record: &mut Record) -> bool {
    match record {
        Record::ApplicationContext(ApplicationContext { application }) => {
            *application = AP214_APPLICATION.to_string();
            true
        }
        Record::ApplicationProtocolDefinition(ApplicationProtocolDefinition {
            status,
            schema_name,
            year,
            ..
        }) => {
            *status = "international standard".to_string();
            *schema_name = "automotive_design".to_string();
            *year = 2000;
            true
        }
        _ => false,
    }
}
