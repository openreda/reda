//! Building the Scene of a Schematic Sheet.

mod drawing;
mod frame;
mod ports;
mod unit;

pub use unit::symbol_scene;

use reda_model::geom::Point;
use reda_model::{Project, UnitSuffix, Uuid};

use crate::color::{ColorKey, SchematicColor};
use crate::scene::{Primitive, Scene, SceneLayer, SceneSource};
use crate::text::{TextStyle, glyph_run};

/// The Scene of the Schematic Sheet `sheet`, or `None` when the Project has no such sheet.
///
/// `junctions` are the sheet's derived Junctions, which this crate does not derive
/// itself: pass `reda_schematic::connectivity::derive(sheet).junctions` (or the
/// `schematic.nets` result's `junctions`).
///
/// Draw order: the paper, frame and title block, free lines and rectangles, free text,
/// then Units (body, Pins, fields), Wires,
/// Junctions, Net Labels, No-Connect Markers, Ports and Power Ports; each kind in UUID order.
pub fn schematic_scene(project: &Project, sheet: Uuid, junctions: &[Point]) -> Option<Scene> {
    schematic_scene_styled(project, sheet, junctions, &SchematicStyle::default())
}

/// How a user's Preferences change the drawing of a sheet, never its content.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SchematicStyle {
    /// How a Unit's suffix is written after its Designator (`U1A` or `U1:1`).
    pub unit_suffix: UnitSuffix,
}

/// [`schematic_scene`] drawn in `style`.
pub fn schematic_scene_styled(
    project: &Project,
    sheet: Uuid,
    junctions: &[Point],
    style: &SchematicStyle,
) -> Option<Scene> {
    let body = project.schematic_sheet(sheet)?;
    let mut b = Builder {
        scene: Scene::new(SceneSource::Schematic(sheet)),
    };
    frame::build(&mut b, project, body);
    drawing::graphics(&mut b, body);
    drawing::texts(&mut b, body);
    unit::build(&mut b, body, style);
    drawing::wires(&mut b, body);
    drawing::junctions(&mut b, body, junctions);
    drawing::net_labels(&mut b, body);
    drawing::no_connects(&mut b, body);
    ports::ports(&mut b, body);
    ports::power_ports(&mut b, body);
    Some(b.scene)
}

pub(crate) struct Builder {
    scene: Scene,
}

impl Builder {
    fn push(&mut self, object: Uuid, color: SchematicColor, primitive: Primitive) {
        self.scene.push(
            object,
            SceneLayer::Schematic,
            ColorKey::Schematic(color),
            primitive,
        );
    }

    fn text(
        &mut self,
        object: Uuid,
        color: SchematicColor,
        text: &str,
        at: Point,
        style: TextStyle,
    ) {
        if text.is_empty() {
            return;
        }
        self.push(object, color, Primitive::Glyphs(glyph_run(text, at, style)));
    }
}
