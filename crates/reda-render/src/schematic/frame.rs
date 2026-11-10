//! The sheet's paper, frame border and built-in title block.

use reda_model::geom::{Millideg, Nm, Point};
use reda_model::text::display;
use reda_model::{BorderZones, DocumentKind, Project, SchematicSheet, Uuid};

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

/// The Border Zones' labels sit in a band this far inside the paper edge, out to the
/// border.
const ZONE_EDGE: Nm = Nm::from_mm(5);

/// The title block's text, filled from the Project and the sheet.
struct TitleBlock {
    title: String,
    /// The sheet's own revision, or else the Project's.
    revision: String,
    /// Whether `revision` is the Project's, which the canvas greys.
    revision_inherited: bool,
    drawing_number: String,
    date: String,
    author: String,
    company: String,
    sheet_title: String,
    sheet_number: usize,
    sheet_count: usize,
}

impl TitleBlock {
    fn of(project: &Project, sheet: &SchematicSheet) -> TitleBlock {
        let sheets: Vec<_> = project
            .documents()
            .filter(|d| d.kind == DocumentKind::Schematic)
            .collect();
        let index = sheets
            .iter()
            .position(|d| d.uuid == sheet.uuid)
            .unwrap_or(0);
        let props = &project.props().properties;
        let frame = sheet.frame_settings();
        let shown = |raw: &str| display(raw).into_owned();
        let revision_inherited = frame.revision.is_empty();
        TitleBlock {
            title: shown(&props.title),
            revision: match revision_inherited {
                true => shown(&props.revision),
                false => shown(&frame.revision),
            },
            revision_inherited,
            drawing_number: shown(&frame.drawing_number),
            date: shown(&props.date),
            author: shown(&props.author),
            company: shown(&props.company),
            sheet_title: sheets
                .get(index)
                .map(|d| d.title.clone())
                .unwrap_or_default(),
            sheet_number: index + 1,
            sheet_count: sheets.len(),
        }
    }
}

pub(super) fn build(b: &mut Builder, project: &Project, sheet: &SchematicSheet, canvas: bool) {
    let object = sheet.frame().map_or(sheet.uuid, |f| f.uuid);
    let (w, h) = sheet.dimensions();
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
    if let Some(zones) = sheet.zones() {
        border_zones(b, object, &zones);
    }
    if sheet.frame_settings().title_block {
        title_block(
            b,
            object,
            &TitleBlock::of(project, sheet),
            inner_lo,
            inner_hi,
            canvas,
        );
    }
}

/// The Border Zones: a band between the paper edge and the border, split at the zones'
/// edges, with the column numbers along the top and bottom and the row letters (A at the
/// top) down both sides.
fn border_zones(b: &mut Builder, object: Uuid, zones: &BorderZones) {
    let (w, h) = (zones.width.0, zones.height.0);
    let (edge, margin) = (ZONE_EDGE.0, MARGIN.0);
    b.push(
        object,
        C::Zone,
        rect(
            Point::new(ZONE_EDGE, ZONE_EDGE),
            Point::new(Nm(w - edge), Nm(h - edge)),
            Some(LINE_WIDTH),
        ),
    );
    let label = |b: &mut Builder, text: &str, at: (i64, i64)| {
        b.text(
            object,
            C::Zone,
            text,
            Point::new(Nm(at.0), Nm(at.1)),
            TextStyle {
                height: TEXT,
                rotation: Millideg(0),
                align: Align::new(HAlign::Center, VAlign::Middle),
                view_label: false,
            },
        );
    };
    let band = (edge + margin) / 2;
    let columns = zones.column_edges();
    for (i, pair) in columns.windows(2).enumerate() {
        if i > 0 {
            let x = pair[0].0;
            zone_line(b, object, (x, edge), (x, margin));
            zone_line(b, object, (x, h - margin), (x, h - edge));
        }
        let x = (pair[0].0 + pair[1].0) / 2;
        let name = (i + 1).to_string();
        label(b, &name, (x, h - band));
        label(b, &name, (x, band));
    }
    // Row edges run from the bottom up; row A is the top one.
    let rows = zones.row_edges();
    let count = rows.len().saturating_sub(1);
    for (i, pair) in rows.windows(2).enumerate() {
        if i > 0 {
            let y = pair[0].0;
            zone_line(b, object, (edge, y), (margin, y));
            zone_line(b, object, (w - margin, y), (w - edge, y));
        }
        let y = (pair[0].0 + pair[1].0) / 2;
        let row = u32::try_from(count - 1 - i).unwrap_or(u32::MAX);
        let name = BorderZones::row_name(row);
        label(b, &name, (band, y));
        label(b, &name, (w - band, y));
    }
}

fn zone_line(b: &mut Builder, object: Uuid, a: (i64, i64), z: (i64, i64)) {
    b.push(
        object,
        C::Zone,
        Primitive::Capsule {
            a: Point::new(Nm(a.0), Nm(a.1)),
            b: Point::new(Nm(z.0), Nm(z.1)),
            width: LINE_WIDTH,
        },
    );
}

/// One title block cell: its corner, size, caption and value.
struct Cell {
    x: i64,
    y: i64,
    width: i64,
    height: i64,
    caption: &'static str,
    value: String,
    /// Whether the value is one the sheet takes from the Project (greyed on the canvas).
    inherited: bool,
    /// Whether an empty value is tinted on the canvas: a title field the user fills.
    field: bool,
}

/// The built-in title block in the border's bottom right corner. On the canvas, empty
/// title fields are tinted and a revision taken from the Project is greyed.
fn title_block(
    b: &mut Builder,
    object: Uuid,
    tb: &TitleBlock,
    inner_lo: Point,
    inner_hi: Point,
    canvas: bool,
) {
    let right = inner_hi.x.0;
    let left = right - BLOCK_WIDTH;
    let half = BLOCK_WIDTH / 2;
    let mid = left + half;
    let bottom = inner_lo.y.0;
    // Rows from the bottom up: author | company, revision | date, drawing number,
    // sheet title | n of m, then the Project title across the whole block.
    let rows: [i64; 5] = std::array::from_fn(|i| bottom + i as i64 * ROW);
    let top = rows[4] + TITLE_ROW;
    let cell = |x, y, width, height, caption, value: &str, field| Cell {
        x,
        y,
        width,
        height,
        caption,
        value: value.to_owned(),
        inherited: false,
        field,
    };
    let cells = [
        cell(left, rows[0], half, ROW, "Author: ", &tb.author, true),
        cell(mid, rows[0], half, ROW, "Company: ", &tb.company, true),
        Cell {
            inherited: tb.revision_inherited && !tb.revision.is_empty(),
            ..cell(left, rows[1], half, ROW, "Revision: ", &tb.revision, true)
        },
        cell(mid, rows[1], half, ROW, "Date: ", &tb.date, true),
        cell(
            left,
            rows[2],
            BLOCK_WIDTH,
            ROW,
            "Drawing number: ",
            &tb.drawing_number,
            true,
        ),
        cell(left, rows[3], half, ROW, "", &tb.sheet_title, false),
        cell(
            mid,
            rows[3],
            half,
            ROW,
            "",
            &format!("Sheet {} of {}", tb.sheet_number, tb.sheet_count),
            false,
        ),
        cell(
            left,
            rows[4],
            BLOCK_WIDTH,
            TITLE_ROW,
            "Title: ",
            &tb.title,
            true,
        ),
    ];
    if canvas {
        for c in cells.iter().filter(|c| c.field && c.value.is_empty()) {
            b.push(
                object,
                C::EmptyField,
                rect(
                    Point::new(Nm(c.x), Nm(c.y)),
                    Point::new(Nm(c.x + c.width), Nm(c.y + c.height)),
                    None,
                ),
            );
        }
    }
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
    line(b, object, (mid, bottom), (mid, rows[2]));
    line(b, object, (mid, rows[3]), (mid, rows[4]));

    for c in &cells {
        let size = if c.height == TITLE_ROW {
            TITLE_TEXT
        } else {
            TEXT
        };
        let color = match canvas && c.inherited {
            true => C::FrameTextInherited,
            false => C::FrameText,
        };
        b.text(
            object,
            color,
            &format!("{}{}", c.caption, c.value),
            Point::new(Nm(c.x + PAD), Nm(c.y + c.height / 2)),
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
