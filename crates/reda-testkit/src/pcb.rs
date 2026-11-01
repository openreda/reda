//! Test-only PCB Commands, for objects that real Commands create only in later tickets.

use reda_engine::command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_engine::model::{InDocument, Net, Object, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(NetAdd);
}

/// `test.net.add {name}`: adds a PCB Net in the Default Net Class, as Sync will.
struct NetAdd;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NetAddParams {
    name: String,
}

#[derive(Serialize, JsonSchema)]
struct NetAddResult {
    uuid: Uuid,
}

impl Command for NetAdd {
    const META: Meta = Meta {
        id: "test.net.add",
        title: "Add Net",
        description: "Test only: adds a PCB Net in the Default Net Class.",
        category: "test",
        icon: "bug",
        kind: Kind::Edit,
    };
    type Params = NetAddParams;
    type Output = NetAddResult;

    fn run(&self, cx: &mut Context<'_>, p: NetAddParams) -> Result<NetAddResult, CommandError> {
        let pcb = cx
            .project()?
            .pcb()
            .ok_or_else(|| CommandError::unavailable("no PCB Document"))?;
        let (document, class) = (pcb.uuid, pcb.setup().default_net_class);
        let uuid = cx.new_id();
        let net = Net {
            uuid,
            name: p.name,
            class,
        };
        cx.tx()?
            .insert(Object::Net(InDocument::new(document, net)))?;
        Ok(NetAddResult { uuid })
    }
}
