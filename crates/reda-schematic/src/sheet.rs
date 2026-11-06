//! The sheet frame: `schematic.sheet.set_frame` and `schematic.sheet.title_block`.

use reda_command::{Command, CommandError, Context, Kind, Length, Meta};
use reda_model::geom::Nm;
use reda_model::{
    BorderZones, DocumentKind, FrameSize, InDocument, Object, Orientation, Project, SheetFrame,
    SheetSize, Uuid,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::sheet;

/// Params naming one Schematic Sheet.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SheetRef {
    /// The Schematic Sheet's Document UUID.
    pub document: Uuid,
}

/// A paper size as a Command takes it: a standard size's name, or two lengths.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum FrameSizeParam {
    Standard(SheetSize),
    Custom { width: Length, height: Length },
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SheetSetFrameParams {
    /// The Schematic Sheet's Document UUID.
    pub document: Uuid,
    /// A standard paper size (`a4`, `a3`, `letter`…) or a custom `{width, height}`, both
    /// positive and at most 10 m. The orientation decides which side runs across.
    pub size: Option<FrameSizeParam>,
    /// `landscape` (a new sheet's) or `portrait`.
    pub orientation: Option<Orientation>,
    /// Border Zones along the frame, about 50 mm each and derived from the paper.
    pub border_zones: Option<bool>,
    /// The built-in title block.
    pub title_block: Option<bool>,
    /// The sheet's drawing number; empty for none.
    pub drawing_number: Option<String>,
    /// The sheet's own revision; empty shows the Project's.
    pub revision: Option<String>,
}

/// A sheet's frame settings, with the paper's dimensions as drawn and its Border Zones.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SheetFrameView {
    pub size: FrameSize,
    pub orientation: Orientation,
    pub width: Nm,
    pub height: Nm,
    /// The Border Zones' counts, or null when they are off.
    pub zones: Option<ZoneCounts>,
    pub title_block: bool,
    pub drawing_number: String,
    /// The sheet's own revision; empty when it shows the Project's.
    pub revision: String,
}

/// How many Border Zones run across and up a sheet.
#[derive(Debug, Serialize, JsonSchema)]
pub struct ZoneCounts {
    /// Numbered from 1 at the left.
    pub columns: u32,
    /// Lettered from A at the top.
    pub rows: u32,
}

impl ZoneCounts {
    fn of(zones: Option<BorderZones>) -> Option<ZoneCounts> {
        zones.map(|z| ZoneCounts {
            columns: z.columns,
            rows: z.rows,
        })
    }
}

impl SheetFrameView {
    fn of(frame: &SheetFrame) -> SheetFrameView {
        let (width, height) = frame.dimensions();
        SheetFrameView {
            size: frame.size,
            orientation: frame.orientation,
            width,
            height,
            zones: ZoneCounts::of(frame.zones()),
            title_block: frame.title_block,
            drawing_number: frame.drawing_number.clone(),
            revision: frame.revision.clone(),
        }
    }
}

/// The longest side a custom sheet may have.
const MAX_SIDE: Nm = Nm::from_mm(10_000);

fn frame_size(size: FrameSizeParam) -> Result<FrameSize, CommandError> {
    Ok(match size {
        FrameSizeParam::Standard(size) => FrameSize::Standard(size),
        FrameSizeParam::Custom { width, height } => {
            for (side, pointer) in [(width, "/size/width"), (height, "/size/height")] {
                if side.nm() <= Nm::ZERO || side.nm() > MAX_SIDE {
                    return Err(CommandError::invalid_params(
                        pointer,
                        "a custom sheet side must be positive and at most 10 m",
                    ));
                }
            }
            FrameSize::Custom {
                width: width.nm(),
                height: height.nm(),
            }
        }
    })
}

pub struct SheetSetFrame;

impl Command for SheetSetFrame {
    const META: Meta = Meta {
        id: "schematic.sheet.set_frame",
        title: "Set Sheet Frame",
        description: "Sets a Schematic Sheet's frame: its paper size (A4, A3, Letter… or a \
                      custom width and height), its orientation, whether it shows Border \
                      Zones and the title block, and its own drawing number and revision. \
                      Settings left out keep their value. A new sheet is A4 landscape with \
                      Border Zones and a title block.",
        category: "schematic",
        icon: "sheet_size",
        kind: Kind::Edit,
    };
    type Params = SheetSetFrameParams;
    type Output = SheetFrameView;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: SheetSetFrameParams,
    ) -> Result<SheetFrameView, CommandError> {
        let existing = sheet(cx.project()?, p.document)?.frame().cloned();
        let mut frame = match &existing {
            Some(frame) => frame.clone(),
            None => SheetFrame::defaults(cx.new_id()),
        };
        if let Some(size) = p.size {
            frame.size = frame_size(size)?;
        }
        if let Some(orientation) = p.orientation {
            frame.orientation = orientation;
        }
        if let Some(on) = p.border_zones {
            frame.border_zones = on;
        }
        if let Some(on) = p.title_block {
            frame.title_block = on;
        }
        if let Some(number) = p.drawing_number {
            frame.drawing_number = number.trim().to_owned();
        }
        if let Some(revision) = p.revision {
            frame.revision = revision.trim().to_owned();
        }
        let view = SheetFrameView::of(&frame);
        let object = Object::SheetFrame(InDocument::new(p.document, frame));
        let tx = cx.tx()?;
        match existing {
            Some(_) => tx.replace(object)?,
            None => tx.insert(object)?,
        }
        Ok(view)
    }
}

/// A sheet's built-in title block, filled from the Project properties and the sheet.
#[derive(Debug, Serialize, JsonSchema)]
pub struct TitleBlock {
    /// Whether the sheet draws it.
    pub shown: bool,
    /// The Project title.
    pub title: String,
    /// The sheet's own revision, or else the Project's.
    pub revision: String,
    /// Whether `revision` is the Project's, which the editor shows greyed.
    pub revision_from_project: bool,
    pub drawing_number: String,
    pub date: String,
    pub author: String,
    pub company: String,
    /// The sheet's own title, such as "Sheet 1".
    pub sheet_title: String,
    /// The sheet's place among the Project's Schematic Sheets, from 1: "sheet n of m".
    pub sheet_number: usize,
    pub sheet_count: usize,
    pub size: FrameSize,
    pub orientation: Orientation,
    pub width: Nm,
    pub height: Nm,
    /// The Border Zones' counts, or null when they are off.
    pub zones: Option<ZoneCounts>,
}

/// The title block of the Schematic Sheet `uuid`.
pub fn title_block(project: &Project, uuid: Uuid) -> Result<TitleBlock, CommandError> {
    let body = sheet(project, uuid)?;
    let sheets: Vec<_> = project
        .documents()
        .filter(|d| d.kind == DocumentKind::Schematic)
        .collect();
    let index = sheets
        .iter()
        .position(|d| d.uuid == uuid)
        .expect("every sheet body has an entry");
    let props = &project.props().properties;
    let frame = SheetFrameView::of(&body.frame_settings());
    let revision_from_project = frame.revision.is_empty();
    Ok(TitleBlock {
        shown: frame.title_block,
        title: props.title.clone(),
        revision: match revision_from_project {
            true => props.revision.clone(),
            false => frame.revision,
        },
        revision_from_project,
        drawing_number: frame.drawing_number,
        date: props.date.clone(),
        author: props.author.clone(),
        company: props.company.clone(),
        sheet_title: sheets[index].title.clone(),
        sheet_number: index + 1,
        sheet_count: sheets.len(),
        size: frame.size,
        orientation: frame.orientation,
        width: frame.width,
        height: frame.height,
        zones: frame.zones,
    })
}

pub struct SheetTitleBlock;

impl Command for SheetTitleBlock {
    const META: Meta = Meta {
        id: "schematic.sheet.title_block",
        title: "Sheet Title Block",
        description: "Returns a Schematic Sheet's title block as drawn: whether it is \
                      shown, the Project properties (title, date, author, company), the \
                      sheet's drawing number and revision (the Project's when it has none), \
                      the sheet's title, \"sheet n of m\" from the Document order, and the \
                      paper with its Border Zones.",
        category: "schematic",
        icon: "title_block",
        kind: Kind::Query,
    };
    type Params = SheetRef;
    type Output = TitleBlock;

    fn run(&self, cx: &mut Context<'_>, p: SheetRef) -> Result<TitleBlock, CommandError> {
        title_block(cx.project()?, p.document)
    }
}
