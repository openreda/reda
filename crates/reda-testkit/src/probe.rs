//! Test-only edit Commands over the Probe object, for exercising undo, redo and dirty
//! tracking on any scope before real Document object kinds exist.

use reda_engine::Engine;
use reda_engine::command::{
    Command, CommandError, Context, FixedClock, Kind, Meta, Registry, SeededIds,
};
use reda_engine::model::{DocumentEntry, DocumentKind, Object, Probe, Uuid};
use serde::{Deserialize, Serialize};

use crate::TestEngine;

/// Like [`crate::test_engine`], plus `test.probe.set` and `test.sheet.add`.
pub fn probe_engine() -> TestEngine {
    TestEngine(
        Engine::builder()
            .ids(SeededIds::new(0))
            .clock(FixedClock::EPOCH)
            .commands(register_all)
            .build(),
    )
}

fn register_all(registry: &mut Registry) {
    registry.add(ProbeSet);
    registry.add(SheetAdd);
}

/// `test.probe.set {probes: [{uuid, document, value}]}`: creates or replaces each Probe,
/// all in one transaction. Probes in several Documents make it a cross-Document edit.
struct ProbeSet;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProbeSetParams {
    probes: Vec<ProbeRow>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProbeRow {
    uuid: Uuid,
    document: Option<Uuid>,
    value: i64,
}

impl Command for ProbeSet {
    const META: Meta = Meta {
        id: "test.probe.set",
        title: "Set Probes",
        description: "Test only: creates or replaces Probes.",
        category: "test",
        icon: "bug",
        kind: Kind::Edit,
    };
    type Params = ProbeSetParams;
    type Output = ();

    fn run(&self, cx: &mut Context<'_>, p: ProbeSetParams) -> Result<(), CommandError> {
        let tx = cx.tx()?;
        for row in p.probes {
            let probe = Object::Probe(Probe {
                uuid: row.uuid,
                document: row.document,
                value: row.value,
            });
            if tx.project().object(row.uuid).is_some() {
                tx.replace(probe)?;
            } else {
                tx.insert(probe)?;
            }
        }
        Ok(())
    }
}

/// `test.sheet.add {name}`: appends a Schematic Sheet, an edit on the Project stack.
struct SheetAdd;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SheetAddParams {
    name: String,
}

#[derive(Serialize)]
struct SheetAddResult {
    uuid: Uuid,
}

impl Command for SheetAdd {
    const META: Meta = Meta {
        id: "test.sheet.add",
        title: "Add Sheet",
        description: "Test only: appends a Schematic Sheet.",
        category: "test",
        icon: "bug",
        kind: Kind::Edit,
    };
    type Params = SheetAddParams;
    type Output = SheetAddResult;

    fn run(&self, cx: &mut Context<'_>, p: SheetAddParams) -> Result<SheetAddResult, CommandError> {
        let uuid = cx.new_id();
        let tx = cx.tx()?;
        let mut props = tx.project().props().clone();
        props.documents.push(uuid);
        tx.insert(Object::Document(DocumentEntry {
            uuid,
            kind: DocumentKind::Schematic,
            title: p.name.clone(),
            name: p.name,
        }))?;
        tx.replace(Object::Project(props))?;
        Ok(SheetAddResult { uuid })
    }
}
