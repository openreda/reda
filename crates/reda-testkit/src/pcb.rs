//! Test-only PCB Commands, for objects that real Commands create only in later tickets.

use reda_engine::command::{Angle, Command, CommandError, Context, Kind, Length, Meta, Registry};
use reda_engine::model::geom::{Millideg, Point};
use reda_engine::model::{
    Fields, Footprint, FootprintCopy, FootprintInstance, FootprintText, Hole, InDocument, Net,
    Object, Pad, PadKind, PadNet, PadShape, Side, Uuid,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(NetAdd);
    registry.add(FootprintAdd);
}

/// `test.footprint.add {at, rotation?, side?, pads: [{at, width, height, shape?, kind?,
/// rotation?, corner_radius?, net?, hole?}]}`: places a Footprint instance with its own one-off
/// Footprint copy (Pads numbered 1, 2, … in param order), as Sync will, so copper tests
/// need no Library. Pads default to rectangular SMD Pads; the Designator text is hidden.
struct FootprintAdd;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Xy {
    x: Length,
    y: Length,
}

impl Xy {
    fn point(&self) -> Point {
        Point::new(self.x.nm(), self.y.nm())
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PadParams {
    at: Xy,
    width: Length,
    height: Length,
    #[serde(default)]
    shape: Option<PadShape>,
    #[serde(default)]
    kind: Option<PadKind>,
    #[serde(default)]
    rotation: Option<Angle>,
    #[serde(default)]
    corner_radius: Option<Length>,
    #[serde(default)]
    net: Option<Uuid>,
    /// `{width, height}` in nm.
    #[serde(default)]
    hole: Option<Hole>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct FootprintAddParams {
    at: Xy,
    #[serde(default)]
    rotation: Option<Angle>,
    #[serde(default)]
    side: Option<Side>,
    pads: Vec<PadParams>,
}

#[derive(Serialize, JsonSchema)]
struct FootprintAddResult {
    footprint: Uuid,
    /// The Pads, in param order.
    pads: Vec<Uuid>,
}

impl Command for FootprintAdd {
    const META: Meta = Meta {
        id: "test.footprint.add",
        title: "Add Footprint",
        description: "Test only: places a Footprint with the given Pads on the PCB.",
        category: "test",
        icon: "bug",
        kind: Kind::Edit,
    };
    type Params = FootprintAddParams;
    type Output = FootprintAddResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: FootprintAddParams,
    ) -> Result<FootprintAddResult, CommandError> {
        let document = cx
            .project()?
            .pcb()
            .ok_or_else(|| CommandError::unavailable("no PCB Document"))?
            .uuid;
        let mut pads = Vec::new();
        let mut nets = Vec::new();
        for (i, pad) in p.pads.iter().enumerate() {
            let uuid = cx.new_id();
            pads.push(Pad {
                uuid,
                number: (i + 1).to_string(),
                kind: pad.kind.unwrap_or(PadKind::Smd),
                shape: pad.shape.unwrap_or(PadShape::Rect),
                at: pad.at.point(),
                rotation: pad.rotation.map_or(Millideg::ZERO, |r| r.millideg()),
                width: pad.width.nm(),
                height: pad.height.nm(),
                corner_radius: pad.corner_radius.map(|r| r.nm()),
                hole: pad.hole,
                mask_margin: None,
                paste_margin: None,
            });
            nets.push(PadNet {
                pad: uuid,
                net: pad.net,
            });
        }
        let order: Vec<Uuid> = pads.iter().map(|p| p.uuid).collect();
        pads.sort_by_key(|p| p.uuid);
        nets.sort_by_key(|p| p.pad);
        let footprint = Footprint {
            uuid: cx.new_id(),
            name: "Test".into(),
            description: String::new(),
            pads,
            graphics: Vec::new(),
            // Hidden: these Footprints are for copper tests and draw no silkscreen.
            designator: FootprintText {
                visible: false,
                ..FootprintText::DESIGNATOR
            },
            value: FootprintText::VALUE,
            provenance: None,
        };
        let copy = FootprintCopy {
            uuid: cx.new_id(),
            hash: footprint.uuid.to_string(),
            footprint,
        };
        let instance = FootprintInstance {
            uuid: cx.new_id(),
            component: None,
            footprint: copy.uuid,
            at: p.at.point(),
            rotation: p.rotation.map_or(Millideg::ZERO, |r| r.millideg()),
            side: p.side.unwrap_or(Side::Top),
            locked: false,
            designator: "TP".into(),
            fields: Fields::default(),
            not_fitted: false,
            designator_text: None,
            pads: nets,
        };
        let footprint = instance.uuid;
        let tx = cx.tx()?;
        tx.insert(Object::FootprintCopy(InDocument::new(document, copy)))?;
        tx.insert(Object::FootprintInstance(InDocument::new(
            document, instance,
        )))?;
        Ok(FootprintAddResult {
            footprint,
            pads: order,
        })
    }
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
