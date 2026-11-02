//! Rows of Pins pasted into the pin table, as a spreadsheet or a datasheet table copies
//! them: one Pin per line, cells split by tabs (else commas, else semicolons).
//!
//! Columns, in order: number, name, electrical type, x, y, rotation, length. Only the
//! number is required. A first row whose first cell is `Number`, `No`, `#` or `Pin` is a
//! header and skipped. Lengths take a unit (`100mil`, `2.54mm`); bare numbers are mil, the
//! unit of the 50 mil Pin grid. Rotations are degrees (`90` or `90deg`). Types are matched
//! loosely: `power_in`, `Power In` and `power-in` are the same.

use reda_engine::model::PinType;
use reda_engine::model::geom::{Millideg, Nm, Point};

/// One pasted row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PinRow {
    pub number: String,
    pub name: String,
    pub electrical_type: Option<PinType>,
    pub at: Option<Point>,
    pub rotation: Option<Millideg>,
    pub length: Option<Nm>,
}

/// The rows of `text`, or what is wrong with the first bad one ("row 3: …", counting
/// the lines as pasted, blank ones included).
pub fn parse_pin_rows(text: &str) -> Result<Vec<PinRow>, String> {
    let mut rows = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let cells = split(line);
        if rows.is_empty() && i == first_line(text) && is_header(&cells[0]) {
            continue;
        }
        rows.push(row(&cells).map_err(|why| format!("row {}: {why}", i + 1))?);
    }
    Ok(rows)
}

fn first_line(text: &str) -> usize {
    text.lines().position(|l| !l.trim().is_empty()).unwrap_or(0)
}

fn split(line: &str) -> Vec<String> {
    let separator = [('\t'), (','), (';')]
        .into_iter()
        .find(|s| line.contains(*s))
        .unwrap_or('\t');
    line.split(separator).map(|c| c.trim().to_owned()).collect()
}

fn is_header(cell: &str) -> bool {
    matches!(
        cell.to_ascii_lowercase().as_str(),
        "number" | "no" | "no." | "#" | "pin" | "pin number"
    )
}

fn row(cells: &[String]) -> Result<PinRow, String> {
    let cell = |i: usize| cells.get(i).map(String::as_str).filter(|c| !c.is_empty());
    let number = cell(0).ok_or("the Pin number is empty")?.to_owned();
    let length = |i: usize, what: &str| -> Result<Option<Nm>, String> {
        cell(i).map(|c| length(c, what)).transpose()
    };
    let at = match (length(3, "x")?, length(4, "y")?) {
        (Some(x), Some(y)) => Some(Point::new(x, y)),
        (None, None) => None,
        _ => return Err("give both x and y, or neither".into()),
    };
    Ok(PinRow {
        number,
        name: cell(1).unwrap_or_default().to_owned(),
        electrical_type: cell(2).map(pin_type).transpose()?,
        at,
        rotation: cell(5).map(rotation).transpose()?,
        length: length(6, "length")?,
    })
}

fn length(cell: &str, what: &str) -> Result<Nm, String> {
    let text = match cell.parse::<f64>() {
        Ok(_) => format!("{cell}mil"),
        Err(_) => cell.to_owned(),
    };
    Nm::parse(&text).map_err(|e| format!("the {what} {cell:?} is not a length: {e}"))
}

fn rotation(cell: &str) -> Result<Millideg, String> {
    let text = match cell.parse::<f64>() {
        Ok(_) => format!("{cell}deg"),
        Err(_) => cell.to_owned(),
    };
    Millideg::parse(&text).map_err(|e| format!("the rotation {cell:?} is not an angle: {e}"))
}

/// The Pin type `cell` names, ignoring case, spaces, hyphens and underscores.
fn pin_type(cell: &str) -> Result<PinType, String> {
    let key: String = cell
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    Ok(match key.as_str() {
        "input" | "in" | "i" => PinType::Input,
        "output" | "out" | "o" => PinType::Output,
        "bidirectional" | "bidi" | "bi" | "io" | "inout" => PinType::Bidirectional,
        "tristate" | "tri" => PinType::TriState,
        "passive" | "pas" => PinType::Passive,
        "powerin" | "pwrin" | "power" => PinType::PowerIn,
        "powerout" | "pwrout" => PinType::PowerOut,
        "opencollector" | "oc" => PinType::OpenCollector,
        "openemitter" | "oe" => PinType::OpenEmitter,
        "notconnected" | "nc" => PinType::NotConnected,
        _ => return Err(format!("unknown electrical type {cell:?}")),
    })
}
