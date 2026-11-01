//! The sheet frame: `schematic.sheet.set_size` and `schematic.sheet.title_block`.

use reda_command::{Command, CommandError, Context, Kind, Meta};
use reda_model::geom::Nm;
use reda_model::{DocumentKind, InDocument, Object, Project, SheetFrame, SheetSize, Uuid};
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

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SheetSetSizeParams {
    /// The Schematic Sheet's Document UUID.
    pub document: Uuid,
    /// The paper size; sheets are landscape.
    pub size: SheetSize,
}

/// A sheet's paper size with its landscape dimensions.
#[derive(Debug, Serialize, JsonSchema)]
pub struct SheetFrameView {
    pub size: SheetSize,
    pub width: Nm,
    pub height: Nm,
}

impl SheetFrameView {
    fn of(size: SheetSize) -> SheetFrameView {
        let (width, height) = size.dimensions();
        SheetFrameView {
            size,
            width,
            height,
        }
    }
}

pub struct SheetSetSize;

impl Command for SheetSetSize {
    const META: Meta = Meta {
        id: "schematic.sheet.set_size",
        title: "Set Sheet Size",
        description: "Sets a Schematic Sheet's paper size (A4, A3, Letter…). Sheets are \
                      landscape. A new sheet is A4.",
        category: "schematic",
        icon: "sheet_size",
        kind: Kind::Edit,
    };
    type Params = SheetSetSizeParams;
    type Output = SheetFrameView;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: SheetSetSizeParams,
    ) -> Result<SheetFrameView, CommandError> {
        let existing = sheet(cx.project()?, p.document)?.frame().map(|f| f.uuid);
        let uuid = existing.unwrap_or_else(|| cx.new_id());
        let frame = Object::SheetFrame(InDocument::new(
            p.document,
            SheetFrame { uuid, size: p.size },
        ));
        let tx = cx.tx()?;
        match existing {
            Some(_) => tx.replace(frame)?,
            None => tx.insert(frame)?,
        }
        Ok(SheetFrameView::of(p.size))
    }
}

/// A sheet's built-in title block, filled from the Project properties and the sheet.
#[derive(Debug, Serialize, JsonSchema)]
pub struct TitleBlock {
    /// The Project title.
    pub title: String,
    pub revision: String,
    pub date: String,
    pub author: String,
    pub company: String,
    /// The sheet's own title, such as "Sheet 1".
    pub sheet_title: String,
    /// The sheet's place among the Project's Schematic Sheets, from 1: "sheet n of m".
    pub sheet_number: usize,
    pub sheet_count: usize,
    pub size: SheetSize,
    pub width: Nm,
    pub height: Nm,
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
    let frame = SheetFrameView::of(body.size());
    Ok(TitleBlock {
        title: props.title.clone(),
        revision: props.revision.clone(),
        date: props.date.clone(),
        author: props.author.clone(),
        company: props.company.clone(),
        sheet_title: sheets[index].title.clone(),
        sheet_number: index + 1,
        sheet_count: sheets.len(),
        size: frame.size,
        width: frame.width,
        height: frame.height,
    })
}

pub struct SheetTitleBlock;

impl Command for SheetTitleBlock {
    const META: Meta = Meta {
        id: "schematic.sheet.title_block",
        title: "Sheet Title Block",
        description: "Returns a Schematic Sheet's title block as drawn: the Project \
                      properties (title, revision, date, author, company), the sheet's \
                      title, \"sheet n of m\" from the Document order, and the paper size.",
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
