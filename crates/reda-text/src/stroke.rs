//! Hershey stroke fonts in James Hurt's `.jhf` format.
//!
//! Each glyph record is a 5-column glyph number, a 3-column vertex count, then that many
//! two-character vertices; long records wrap onto following lines. A vertex is two characters
//! whose offsets from `R` give x and y in Hershey units (y grows downwards). The first vertex
//! holds the left and right bearings, and ` R` lifts the pen.

use std::collections::BTreeMap;
use std::fmt;

const ROMAN_SIMPLEX: &str = include_str!("../fonts/hershey/rowmans.jhf");

/// A point in Hershey units: x to the right, y downwards, relative to the glyph centre.
pub type HersheyPoint = (i32, i32);

/// One parsed Hershey glyph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StrokeGlyph {
    /// Left bearing in Hershey units (usually negative).
    pub left: i32,
    /// Right bearing in Hershey units.
    pub right: i32,
    /// Pen-down polylines.
    pub strokes: Vec<Vec<HersheyPoint>>,
}

impl StrokeGlyph {
    /// The advance width in Hershey units.
    pub fn advance(&self) -> i32 {
        self.right - self.left
    }
}

/// A Hershey stroke font, keyed by character.
#[derive(Clone, Debug)]
pub struct StrokeFont {
    glyphs: BTreeMap<char, StrokeGlyph>,
    cap_top: i32,
    baseline: i32,
}

/// Why a `.jhf` file failed to parse. `record` counts from 0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JhfError {
    Truncated { record: usize },
    BadHeader { record: usize },
    BadVertex { record: usize },
}

impl fmt::Display for JhfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JhfError::Truncated { record } => write!(f, "jhf record {record} is truncated"),
            JhfError::BadHeader { record } => write!(f, "jhf record {record} has a bad header"),
            JhfError::BadVertex { record } => write!(f, "jhf record {record} has a bad vertex"),
        }
    }
}

impl std::error::Error for JhfError {}

impl StrokeFont {
    /// The vendored Hershey Roman Simplex, REDA's default PCB text font.
    pub fn hershey_roman_simplex() -> StrokeFont {
        StrokeFont::parse_jhf(ROMAN_SIMPLEX, ' ', -12, 9)
            .expect("the vendored Roman Simplex parses")
    }

    /// Parses `.jhf` source whose records map to consecutive characters from `first`.
    /// `cap_top` and `baseline` are the Hershey y of capital tops and of the baseline.
    pub fn parse_jhf(
        source: &str,
        first: char,
        cap_top: i32,
        baseline: i32,
    ) -> Result<StrokeFont, JhfError> {
        let records = parse_records(source)?;
        let glyphs = records
            .into_iter()
            .enumerate()
            .filter_map(|(i, glyph)| {
                let c = char::from_u32(first as u32 + u32::try_from(i).ok()?)?;
                Some((c, glyph))
            })
            .collect();
        Ok(StrokeFont {
            glyphs,
            cap_top,
            baseline,
        })
    }

    pub fn glyph(&self, c: char) -> Option<&StrokeGlyph> {
        self.glyphs.get(&c)
    }

    /// Hershey y of capital tops.
    pub fn cap_top(&self) -> i32 {
        self.cap_top
    }

    /// Hershey y of the baseline.
    pub fn baseline(&self) -> i32 {
        self.baseline
    }

    /// Capital height in Hershey units.
    pub fn cap_height(&self) -> i32 {
        self.baseline - self.cap_top
    }
}

fn parse_records(source: &str) -> Result<Vec<StrokeGlyph>, JhfError> {
    let mut glyphs = Vec::new();
    let mut lines = source.lines().filter(|l| !l.trim().is_empty());
    while let Some(line) = lines.next() {
        let record = glyphs.len();
        let header = line.get(..8).ok_or(JhfError::BadHeader { record })?;
        let count: usize = header[5..]
            .trim()
            .parse()
            .map_err(|_| JhfError::BadHeader { record })?;
        let mut data: String = line[8..].to_owned();
        while data.len() < count * 2 {
            let more = lines.next().ok_or(JhfError::Truncated { record })?;
            data.push_str(more);
        }
        glyphs.push(parse_glyph(data.as_bytes(), count, record)?);
    }
    Ok(glyphs)
}

fn parse_glyph(data: &[u8], count: usize, record: usize) -> Result<StrokeGlyph, JhfError> {
    if count == 0 || data.len() < count * 2 {
        return Err(JhfError::Truncated { record });
    }
    let coord = |b: u8| -> Result<i32, JhfError> {
        if b.is_ascii_graphic() {
            Ok(i32::from(b) - i32::from(b'R'))
        } else {
            Err(JhfError::BadVertex { record })
        }
    };
    let left = coord(data[0])?;
    let right = coord(data[1])?;
    let mut strokes = Vec::new();
    let mut current: Vec<HersheyPoint> = Vec::new();
    for pair in data[2..count * 2].chunks_exact(2) {
        if pair == b" R" {
            if !current.is_empty() {
                strokes.push(std::mem::take(&mut current));
            }
        } else {
            current.push((coord(pair[0])?, coord(pair[1])?));
        }
    }
    if !current.is_empty() {
        strokes.push(current);
    }
    Ok(StrokeGlyph {
        left,
        right,
        strokes,
    })
}
