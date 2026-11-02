//! The sheet's paper, frame border and built-in title block.

use reda_model::geom::{Millideg, Nm, Point};
use reda_model::{DocumentKind, Project, SchematicSheet, Uuid};

use super::Builder;
use crate::color::SchematicColor as C;
use crate::scene::{Align, HAlign, Primitive, VAlign};
use crate::text::TextStyle;

/// The border sits this far inside the paper edge.
const MARGIN: Nm = Nm::from_mm(10);
const BORDER_WIDTH: Nm = Nm::from_um(300);
const LINE_WIDTH: Nm = Nm::from_um(200);
const BLOCK_WIDTH: i64 = 110_000_000;
const TITLE_ROW: i64 = 10_000_000;
const ROW: i64 = 7_000_000;
const TEXT: Nm = Nm::from_um(2_500);
const TITLE_TEXT: Nm = Nm::from_um(3_500);
const PAD: i64 = 2_000_000;

/// The title block's text, filled from the Project and the sheet.
struct TitleBlock {
    title: String,
    revision: String,
    date: String,
    author: String,
    company: String,
    sheet_title: String,
    sheet_number: usize,
    sheet_count: usize,
}

impl TitleBlock {
    fn of(project: &Project, sheet: Uuid) -> TitleBlock {
        let sheets: Vec<_> = project
            .documents()
            .filter(|d| d.kind == DocumentKind::Schematic)
            .collect();
        let index = sheets.iter().position(|d| d.uuid == sheet).unwrap_or(0);
        let props = &project.props().properties;
        TitleBlock {
            title: props.title.clone(),
            revision: props.revision.clone(),
            date: props.date.clone(),
            author: props.author.clone(),
            company: props.company.clone(),
            sheet_title: sheets
                .get(index)
                .map(|d| d.title.clone())
                .unwrap_or_default(),
            sheet_number: index + 1,
            sheet_count: sheets.len(),
        }
    }
}

pub(super) fn build(b: &mut Builder, project: &Project, sheet: &SchematicSheet) {
    let object = sheet.frame().map_or(sheet.uuid, |f| f.uuid);
    let (w, h) = sheet.size().dimensions();
    b.push(
        object,
        C::Paper,
        rect(Point::default(), Point::new(w, h), None),
    );
    let inner_lo = Point::new(MARGIN, MARGIN);
    let inner_hi = Point::new(w - MARGIN, h - MARGIN);
    b.push(
        object,
        C::Frame,
        rect(inner_lo, inner_hi, Some(BORDER_WIDTH)),
    );

    let tb = TitleBlock::of(project, sheet.uuid);
    let right = inner_hi.x.0;
    let left = right - BLOCK_WIDTH;
    let mid = left + BLOCK_WIDTH / 2;
    let bottom = inner_lo.y.0;
    // Rows from the bottom up: author | company, revision | date, sheet title | n of m,
    // then the Project title across the whole block.
    let rows = [bottom, bottom + ROW, bottom + 2 * ROW, bottom + 3 * ROW];
    let top = rows[3] + TITLE_ROW;
    b.push(
        object,
        C::Frame,
        rect(
            Point::new(Nm(left), Nm(bottom)),
            Point::new(Nm(right), Nm(top)),
            Some(LINE_WIDTH),
        ),
    );
    for &y in &rows[1..] {
        line(b, object, (left, y), (right, y));
    }
    line(b, object, (mid, bottom), (mid, rows[3]));

    let cells = [
        (left, rows[0], ROW, format!("Author: {}", tb.author)),
        (mid, rows[0], ROW, format!("Company: {}", tb.company)),
        (left, rows[1], ROW, format!("Revision: {}", tb.revision)),
        (mid, rows[1], ROW, format!("Date: {}", tb.date)),
        (left, rows[2], ROW, tb.sheet_title.clone()),
        (
            mid,
            rows[2],
            ROW,
            format!("Sheet {} of {}", tb.sheet_number, tb.sheet_count),
        ),
        (left, rows[3], TITLE_ROW, format!("Title: {}", tb.title)),
    ];
    for (x, y, height, text) in cells {
        let size = if height == TITLE_ROW {
            TITLE_TEXT
        } else {
            TEXT
        };
        b.text(
            object,
            C::FrameText,
            &text,
            Point::new(Nm(x + PAD), Nm(y + height / 2)),
            TextStyle {
                height: size,
                rotation: Millideg(0),
                align: Align::new(HAlign::Left, VAlign::Middle),
                view_label: false,
            },
        );
    }
}

fn line(b: &mut Builder, object: Uuid, a: (i64, i64), z: (i64, i64)) {
    b.push(
        object,
        C::Frame,
        Primitive::Capsule {
            a: Point::new(Nm(a.0), Nm(a.1)),
            b: Point::new(Nm(z.0), Nm(z.1)),
            width: LINE_WIDTH,
        },
    );
}

/// An axis-aligned rectangle between two corners.
pub(super) fn rect(a: Point, z: Point, stroke: Option<Nm>) -> Primitive {
    let (lo_x, hi_x) = (a.x.min(z.x), a.x.max(z.x));
    let (lo_y, hi_y) = (a.y.min(z.y), a.y.max(z.y));
    Primitive::Rect {
        center: Point::new(Nm((lo_x.0 + hi_x.0) / 2), Nm((lo_y.0 + hi_y.0) / 2)),
        size: (hi_x - lo_x, hi_y - lo_y),
        rotation: Millideg(0),
        corner_radius: Nm(0),
        stroke,
    }
}
