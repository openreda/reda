//! Building the Scene of a Schematic Sheet.

mod cross_over;
mod drawing;
mod frame;
mod ports;
mod unit;

pub use unit::symbol_scene;

use std::collections::BTreeMap;

use reda_model::geom::{Nm, Point};
use reda_model::{Project, UnitSuffix, Uuid};

use crate::color::{ColorKey, SchematicColor};
use crate::scene::{Primitive, Scene, SceneLayer, SceneSource};
use crate::text::{self, TextStyle, glyph_run};
use reda_model::net_name::overbars;

/// The Scene of the Schematic Sheet `sheet`, or `None` when the Project has no such sheet.
///
/// `junctions` are the sheet's derived Junctions, which this crate does not derive
/// itself: pass `reda_schematic::connectivity::derive(sheet).junctions` (or the
/// `schematic.nets` result's `junctions`).
///
/// Draw order: the paper, frame, Border Zones and title block, free lines and rectangles, free text,
/// then Units (body, Pins, fields, a Not Fitted overlay), Wires,
/// Junctions, Net Labels, No-Connect Markers, Ports and Power Ports; each kind in UUID order.
pub fn schematic_scene(project: &Project, sheet: Uuid, junctions: &[Point]) -> Option<Scene> {
    schematic_scene_styled(project, sheet, junctions, &SchematicStyle::default())
}

/// How a user's Preferences (and what is derived from the Project, such as Port
/// Cross-References) change the drawing of a sheet, never its content.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SchematicStyle {
    /// How a Unit's suffix is written after its Designator (`U1A` or `U1:1`).
    pub unit_suffix: UnitSuffix,
    /// Single '\' Negation: a leading backslash draws an overbar over a whole Net Label,
    /// Port or Power Port name.
    pub single_negation: bool,
    /// Drawn for the canvas rather than an output: empty title fields are tinted and a
    /// revision the sheet takes from the Project is greyed.
    pub canvas: bool,
    /// Each Port's cross-reference note, such as `[2:B3]`, keyed by Port UUID: derived
    /// from the Project and the styles it uses (see `reda_schematic::cross_ref::notes`).
    pub cross_references: BTreeMap<Uuid, String>,
    /// Where each entry of a Port's note leads, keyed by Port UUID: each entry becomes a
    /// [`Link`](crate::Link) over its text, in a PDF a link to that sheet's page.
    pub cross_reference_links: BTreeMap<Uuid, Vec<NoteLink>>,
    /// Display Cross-Overs: a Wire hops over each Wire it crosses without connecting.
    /// Canvas only: outputs never set it.
    pub cross_overs: bool,
}

/// Where one entry of a Port's cross-reference note leads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteLink {
    /// The entry as the note writes it, such as `2:B3`.
    pub entry: String,
    /// The partner's Schematic Sheet, by Document UUID.
    pub sheet: Uuid,
    /// The partner's connection point there.
    pub at: Point,
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
    frame::build(&mut b, project, body, style.canvas);
    drawing::graphics(&mut b, body);
    drawing::texts(&mut b, body);
    unit::build(
        &mut b,
        body,
        style,
        project.props().schematic.not_fitted_display,
    );
    drawing::wires(&mut b, body, junctions, style.cross_overs);
    drawing::junctions(&mut b, body, junctions);
    drawing::net_labels(&mut b, body, style.single_negation);
    drawing::no_connects(&mut b, body);
    ports::ports(&mut b, body, style);
    ports::power_ports(&mut b, body, style.single_negation);
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

    /// A Net Label, Port, Power Port or Pin name: its characters without the overbar
    /// markup, then an overbar over each run of negated ones. `single_negation` says
    /// whether a leading backslash negates the whole name (never for Pin names).
    fn name(
        &mut self,
        object: Uuid,
        color: SchematicColor,
        name: &str,
        single_negation: bool,
        at: Point,
        style: TextStyle,
    ) {
        let glyphs = overbars(name, single_negation);
        let text: String = glyphs.iter().map(|g| g.ch).collect();
        self.text(object, color, &text, at, style);
        let width = Nm((style.height.0 / OVERBAR_WIDTH).max(1));
        for (a, b) in text::overbars(&glyphs, at, style) {
            self.push(object, color, Primitive::Capsule { a, b, width });
        }
    }
}

/// An overbar is this many times thinner than the capitals it is over.
const OVERBAR_WIDTH: i64 = 10;

/// `name` as drawn: its characters without the overbar markup.
pub(crate) fn plain_name(name: &str, single_negation: bool) -> String {
    overbars(name, single_negation)
        .iter()
        .map(|g| g.ch)
        .collect()
}
