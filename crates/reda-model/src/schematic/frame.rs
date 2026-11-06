//! A sheet's frame: its paper size and orientation, its Border Zones, its title block and
//! the sheet's own drawing number and revision.

use reda_geom::{Nm, Point};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::Uuid;

/// A standard paper size. Its dimensions are given landscape; [`Orientation`] turns it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SheetSize {
    #[default]
    A4,
    A3,
    A2,
    A1,
    A0,
    Letter,
    Legal,
    Tabloid,
}

impl SheetSize {
    /// The landscape width and height.
    pub const fn dimensions(self) -> (Nm, Nm) {
        const MM: i64 = 1_000_000;
        const IN: i64 = 25_400_000;
        let (w, h) = match self {
            SheetSize::A4 => (297 * MM, 210 * MM),
            SheetSize::A3 => (420 * MM, 297 * MM),
            SheetSize::A2 => (594 * MM, 420 * MM),
            SheetSize::A1 => (841 * MM, 594 * MM),
            SheetSize::A0 => (1189 * MM, 841 * MM),
            SheetSize::Letter => (11 * IN, IN * 17 / 2),
            SheetSize::Legal => (14 * IN, IN * 17 / 2),
            SheetSize::Tabloid => (17 * IN, 11 * IN),
        };
        (Nm(w), Nm(h))
    }
}

/// A sheet's paper: a standard size, written as its name (`"a4"`), or a custom one,
/// written as `{"width": …, "height": …}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum FrameSize {
    Standard(SheetSize),
    /// Two positive lengths. Which side runs across follows the [`Orientation`], so
    /// `{"width": 200 mm, "height": 300 mm}` landscape is 300 mm wide.
    Custom {
        width: Nm,
        height: Nm,
    },
}

impl Default for FrameSize {
    fn default() -> FrameSize {
        FrameSize::Standard(SheetSize::default())
    }
}

impl FrameSize {
    /// The longer and the shorter side.
    pub fn sides(self) -> (Nm, Nm) {
        let (a, b) = match self {
            FrameSize::Standard(size) => size.dimensions(),
            FrameSize::Custom { width, height } => (width, height),
        };
        (a.max(b), a.min(b))
    }
}

/// Which way up a sheet's paper is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Orientation {
    /// The longer side runs across.
    #[default]
    Landscape,
    /// The longer side runs up.
    Portrait,
}

impl Orientation {
    fn is_landscape(&self) -> bool {
        *self == Orientation::Landscape
    }
}

/// A sheet's frame settings. A sheet has at most one; with none it uses
/// [`SheetFrame::defaults`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SheetFrame {
    pub uuid: Uuid,
    pub size: FrameSize,
    /// Landscape unless stated.
    #[serde(default, skip_serializing_if = "Orientation::is_landscape")]
    pub orientation: Orientation,
    /// The Border Zones along the frame, on by default. Their counts are derived from the
    /// paper ([`BorderZones::for_paper`]), never stored.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub border_zones: bool,
    /// The built-in title block, on by default.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub title_block: bool,
    /// The sheet's drawing number; empty by default.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub drawing_number: String,
    /// The sheet's own revision. Empty, the title block shows the Project's.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub revision: String,
}

fn yes() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

impl SheetFrame {
    /// The settings of a sheet that has no frame of its own: A4 landscape, with Border
    /// Zones and the title block, and no drawing number or revision.
    pub fn defaults(uuid: Uuid) -> SheetFrame {
        SheetFrame {
            uuid,
            size: FrameSize::default(),
            orientation: Orientation::default(),
            border_zones: true,
            title_block: true,
            drawing_number: String::new(),
            revision: String::new(),
        }
    }

    /// The paper's width and height as drawn.
    pub fn dimensions(&self) -> (Nm, Nm) {
        let (long, short) = self.size.sides();
        match self.orientation {
            Orientation::Landscape => (long, short),
            Orientation::Portrait => (short, long),
        }
    }

    /// The Border Zones, unless they are switched off.
    pub fn zones(&self) -> Option<BorderZones> {
        let (w, h) = self.dimensions();
        self.border_zones.then(|| BorderZones::for_paper(w, h))
    }
}

/// A sheet's Border Zones: the paper split into numbered columns (1 at the left) and
/// lettered rows (A at the top), each about [`BorderZones::PITCH`] square. The counts
/// follow from the paper alone, so a location such as `B3` means the same on every sheet
/// of a size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub struct BorderZones {
    pub columns: u32,
    pub rows: u32,
    /// The paper the zones split.
    pub width: Nm,
    pub height: Nm,
}

impl BorderZones {
    /// The size a zone comes close to.
    pub const PITCH: Nm = Nm::from_mm(50);

    /// The zones of paper `width` by `height`: each side gets the whole number of zones
    /// nearest to its length over [`BorderZones::PITCH`], and at least one.
    pub fn for_paper(width: Nm, height: Nm) -> BorderZones {
        let count = |len: Nm| {
            let n = (len.0 + Self::PITCH.0 / 2) / Self::PITCH.0;
            u32::try_from(n.max(1)).unwrap_or(u32::MAX)
        };
        BorderZones {
            columns: count(width),
            rows: count(height),
            width,
            height,
        }
    }

    /// Where column `i` (from 0) ends on the left and right: `x` values from 0 to the
    /// width.
    pub fn column_edges(&self) -> Vec<Nm> {
        edges(self.width, self.columns)
    }

    /// Where the rows split the paper, from the bottom edge up (`y` from 0 to the
    /// height). Row A is the top one.
    pub fn row_edges(&self) -> Vec<Nm> {
        edges(self.height, self.rows)
    }

    /// The column, numbered from 1 at the left, that holds `x`. Points off the paper
    /// count as the nearest edge column.
    pub fn column(&self, x: Nm) -> u32 {
        index(x, self.width, self.columns) + 1
    }

    /// The row, from 0 for row A at the top, that holds `y`. Points off the paper count as
    /// the nearest edge row.
    pub fn row(&self, y: Nm) -> u32 {
        self.rows - 1 - index(y, self.height, self.rows)
    }

    /// A row's letters, from 0: A to Z, then AA, AB…
    pub fn row_name(row: u32) -> String {
        let mut n = row + 1;
        let mut out = Vec::new();
        while n > 0 {
            n -= 1;
            out.push(b'A' + (n % 26) as u8);
            n /= 26;
        }
        out.reverse();
        String::from_utf8(out).expect("ASCII letters")
    }

    /// The zone holding `p`, written row then column, such as `B3`.
    pub fn zone(&self, p: Point) -> String {
        format!("{}{}", Self::row_name(self.row(p.y)), self.column(p.x))
    }
}

fn edges(len: Nm, count: u32) -> Vec<Nm> {
    (0..=i64::from(count))
        .map(|i| Nm(len.0 * i / i64::from(count)))
        .collect()
}

fn index(v: Nm, len: Nm, count: u32) -> u32 {
    if len.0 <= 0 {
        return 0;
    }
    let i = (v.0.clamp(0, len.0 - 1) as i128 * i128::from(count) / len.0 as i128) as u32;
    i.min(count - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zones_are_about_fifty_millimetres() {
        let a4 = BorderZones::for_paper(Nm::from_mm(297), Nm::from_mm(210));
        assert_eq!((a4.columns, a4.rows), (6, 4));
        let a3 = BorderZones::for_paper(Nm::from_mm(420), Nm::from_mm(297));
        assert_eq!((a3.columns, a3.rows), (8, 6));
        let tiny = BorderZones::for_paper(Nm::from_mm(10), Nm::from_mm(10));
        assert_eq!((tiny.columns, tiny.rows), (1, 1));
    }

    #[test]
    fn a_zone_reads_row_letter_then_column_number_from_the_top_left() {
        let a4 = BorderZones::for_paper(Nm::from_mm(297), Nm::from_mm(210));
        let at = |x, y| a4.zone(Point::new(Nm::from_mm(x), Nm::from_mm(y)));
        assert_eq!(at(1, 209), "A1");
        assert_eq!(at(296, 1), "D6");
        assert_eq!(at(120, 150), "B3");
        assert_eq!(at(-50, 500), "A1");
    }

    #[test]
    fn rows_past_z_take_two_letters() {
        assert_eq!(BorderZones::row_name(0), "A");
        assert_eq!(BorderZones::row_name(25), "Z");
        assert_eq!(BorderZones::row_name(26), "AA");
        assert_eq!(BorderZones::row_name(27), "AB");
    }
}
