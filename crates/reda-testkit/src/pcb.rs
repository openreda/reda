//! Test-only PCB Commands, for objects that real Commands create only in later tickets.

use reda_engine::command::{Angle, Command, CommandError, Context, Kind, Length, Meta, Registry};
use reda_engine::model::geom::{Millideg, Point};
use reda_engine::model::{
    ComponentType, Corners, Fields, Footprint, FootprintCopy, FootprintGraphic, FootprintInstance,
    FootprintLayer, FootprintText, Hole, InDocument, Net, Object, Pad, PadKind, PadNet, PadShape,
    Ratio, Shape, Side, Uuid, ZoneConnection,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(NetAdd);
    registry.add(FootprintAdd);
}

/// `test.footprint.add {at, rotation?, side?, designator?, graphics?: [{layer, shape, width,
/// fill?}], pads: [{at, width, height, shape?, kind?, rotation?, corner_ratio?,
/// chamfer_ratio?, corners?, offset?, side?, net?, hole?, mask_margin?, paste_margin?,
/// zone_connection?}]}`: places a Footprint instance with its own one-off
/// Footprint copy (Pads numbered 1, 2, … in param order), as Sync will, so copper tests
/// need no Library. Pads default to rectangular SMD Pads.
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
    corner_ratio: Option<Ratio>,
    #[serde(default)]
    chamfer_ratio: Option<Ratio>,
    #[serde(default)]
    corners: Option<Corners>,
    #[serde(default)]
    offset: Option<Xy>,
    #[serde(default)]
    side: Option<Side>,
    #[serde(default)]
    zone_connection: Option<ZoneConnection>,
    #[serde(default)]
    net: Option<Uuid>,
    /// `{width, height}` in nm.
    #[serde(default)]
    hole: Option<Hole>,
    #[serde(default)]
    mask_margin: Option<Length>,
    #[serde(default)]
    paste_margin: Option<Length>,
}

/// A drawing shape of the Footprint, in nm.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct GraphicParams {
    layer: FootprintLayer,
    shape: Shape,
    width: Length,
    #[serde(default)]
    fill: bool,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct FootprintAddParams {
    at: Xy,
    #[serde(default)]
    rotation: Option<Angle>,
    #[serde(default)]
    side: Option<Side>,
    /// Defaults to `TP`.
    #[serde(default)]
    designator: Option<String>,
    #[serde(default)]
    graphics: Vec<GraphicParams>,
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
                corner_ratio: pad.corner_ratio,
                chamfer_ratio: pad.chamfer_ratio,
                corners: pad.corners,
                offset: pad.offset.as_ref().map(Xy::point),
                hole: pad.hole,
                side: pad.side,
                mask_margin: pad.mask_margin.map(|m| m.nm()),
                paste_margin: pad.paste_margin.map(|m| m.nm()),
                zone_connection: pad.zone_connection,
            });
            nets.push(PadNet {
                pad: uuid,
                net: pad.net,
            });
        }
        let order: Vec<Uuid> = pads.iter().map(|p| p.uuid).collect();
        pads.sort_by_key(|p| p.uuid);
        nets.sort_by_key(|p| p.pad);
        let mut graphics: Vec<FootprintGraphic> = p
            .graphics
            .into_iter()
            .map(|g| FootprintGraphic {
                uuid: cx.new_id(),
                layer: g.layer,
                shape: g.shape,
                width: g.width.nm(),
                fill: g.fill,
            })
            .collect();
        graphics.sort_by_key(|g| g.uuid);
        let (uuid, copy_uuid) = (cx.new_id(), cx.new_id());
        let instance_uuid = cx.new_id();
        // The texts' UUIDs come last, so the others keep theirs.
        let mut texts = vec![
            FootprintText::designator(cx.new_id()),
            FootprintText::value(cx.new_id()),
        ];
        texts.sort_by_key(|t| t.uuid);
        let footprint = Footprint {
            uuid,
            name: "Test".into(),
            description: String::new(),
            height: None,
            pads,
            graphics,
            texts,
            generator: None,
            provenance: None,
        };
        let copy = FootprintCopy {
            uuid: copy_uuid,
            hash: footprint.uuid.to_string(),
            footprint,
        };
        let instance = FootprintInstance {
            uuid: instance_uuid,
            component: None,
            footprint: copy.uuid,
            at: p.at.point(),
            rotation: p.rotation.map_or(Millideg::ZERO, |r| r.millideg()),
            side: p.side.unwrap_or(Side::Top),
            locked: false,
            designator: p.designator.unwrap_or_else(|| "TP".into()),
            fields: Fields::default(),
            component_type: ComponentType::Mechanical,
            in_bom: ComponentType::Mechanical.default_in_bom(),
            in_cpl: ComponentType::Mechanical.default_in_cpl(),
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
        description: "Test only: adds a PCB Net, in the Default Net Class only.",
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
        let document = pcb.uuid;
        let uuid = cx.new_id();
        let net = Net {
            uuid,
            name: p.name,
            classes: Vec::new(),
        };
        cx.tx()?
            .insert(Object::Net(InDocument::new(document, net)))?;
        Ok(NetAddResult { uuid })
    }
}
