//! Column-mapping paste of Pin rows, for the Symbol Wizard's paste sub-dialog and paste
//! into the Pins Panel: pasted text is split into cells ([`Delimiter`]), a header row is
//! detected, each column is mapped to a Pin field ([`Column`]) from its heading or, with
//! no header, guessed from its cells, and the rows become [`WizardPin`]s. The dialog
//! edits the [`PasteTable`] (Remove Row, Remove Column, a heading per column, the header
//! switch) and calls [`PasteTable::pins`] again.
//!
//! Pin types are read through [`pin_type`], which Import part can share: every type's
//! own name, plus the aliases the spec lists, ignoring case.

use reda_model::PinType;

use super::{Side, WizardPin};

/// What separates the cells of a pasted line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delimiter {
    Tab,
    Comma,
    Semicolon,
    /// Any run of spaces or tabs.
    Whitespace,
}

impl Delimiter {
    /// The delimiter `text` most likely uses: a tab if any line has one (as copied from a
    /// spreadsheet), else a comma, else a semicolon, else whitespace.
    pub fn detect(text: &str) -> Delimiter {
        if text.contains('\t') {
            Delimiter::Tab
        } else if text.contains(',') {
            Delimiter::Comma
        } else if text.contains(';') {
            Delimiter::Semicolon
        } else {
            Delimiter::Whitespace
        }
    }
}

/// The Pin field a pasted column fills.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Column {
    Number,
    Name,
    Type,
    Side,
    Group,
    /// Not used.
    Ignore,
}

impl Column {
    /// The column a heading names, ignoring case, such as `Pin #` for [`Column::Number`].
    pub fn from_heading(heading: &str) -> Option<Column> {
        let h = heading.trim().to_ascii_lowercase();
        Some(match h.as_str() {
            "number" | "num" | "no" | "no." | "#" | "pin" | "pin #" | "pin number" | "pin no"
            | "pin no." | "pad" | "ball" => Column::Number,
            "name" | "pin name" | "signal" | "net" | "function" => Column::Name,
            "type" | "pin type" | "electrical type" | "electrical_type" | "etype" | "dir"
            | "direction" => Column::Type,
            "side" | "position" | "edge" => Column::Side,
            "group" | "bank" | "port" => Column::Group,
            _ => return None,
        })
    }
}

/// The Pin type `text` names, ignoring case and treating spaces, dashes and underscores
/// alike: `input`, `power in`, `tri-state` and the rest, plus `in`, `out` and the
/// aliases `I/O`, `IO`, `BIDIR` (bidirectional), `HIZ`, `3S` (tri-state), `PWR`, `POWER`
/// (power in), `OC` (open collector), `NC` and `N/C` (not connected).
pub fn pin_type(text: &str) -> Option<PinType> {
    let t: String = text
        .trim()
        .chars()
        .map(|c| match c {
            ' ' | '-' => '_',
            c => c.to_ascii_lowercase(),
        })
        .collect();
    Some(match t.as_str() {
        "input" | "in" | "i" => PinType::Input,
        "output" | "out" | "o" => PinType::Output,
        "bidirectional" | "bidir" | "i/o" | "io" => PinType::Bidirectional,
        "tri_state" | "tristate" | "hiz" | "3s" => PinType::TriState,
        "passive" | "pas" => PinType::Passive,
        "power_in" | "pwr" | "power" => PinType::PowerIn,
        "power_out" => PinType::PowerOut,
        "open_collector" | "oc" => PinType::OpenCollector,
        "open_emitter" | "oe" => PinType::OpenEmitter,
        "not_connected" | "nc" | "n/c" => PinType::NotConnected,
        _ => return None,
    })
}

/// The side `text` names, ignoring case: `left` or `l`, and so on.
pub fn side(text: &str) -> Option<Side> {
    Some(match text.trim().to_ascii_lowercase().as_str() {
        "left" | "l" => Side::Left,
        "right" | "r" => Side::Right,
        "top" | "t" => Side::Top,
        "bottom" | "b" => Side::Bottom,
        _ => return None,
    })
}

/// Pasted text as the paste dialog shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasteTable {
    /// Every non-blank line's cells, trimmed, each row as wide as the widest.
    pub rows: Vec<Vec<String>>,
    /// Whether the first row is headings rather than a Pin.
    pub header: bool,
    /// The field each column fills, one per column.
    pub columns: Vec<Column>,
}

/// Why the table cannot become Pins. `row` and `column` index [`PasteTable::rows`]
/// (header included) and its columns; both are `None` when no column is the number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasteError {
    pub row: Option<usize>,
    pub column: Option<usize>,
    pub message: String,
}

impl PasteTable {
    /// Splits `text` by `delimiter` (omitted, [`Delimiter::detect`]), detects a header
    /// and maps the columns.
    pub fn parse(text: &str, delimiter: Option<Delimiter>) -> PasteTable {
        let delimiter = delimiter.unwrap_or_else(|| Delimiter::detect(text));
        let mut rows: Vec<Vec<String>> = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| split_line(l, delimiter))
            .collect();
        let width = rows.iter().map(Vec::len).max().unwrap_or(0);
        for row in &mut rows {
            row.resize(width, String::new());
        }
        let header = rows.first().is_some_and(|r| is_header(r));
        let columns = if header {
            let mut seen = Vec::new();
            rows[0]
                .iter()
                .map(|h| match Column::from_heading(h) {
                    Some(c) if !seen.contains(&c) => {
                        seen.push(c);
                        c
                    }
                    _ => Column::Ignore,
                })
                .collect()
        } else {
            guess_columns(&rows, width)
        };
        PasteTable {
            rows,
            header,
            columns,
        }
    }

    /// Maps column `index` to `column`; another column mapped to it becomes
    /// [`Column::Ignore`], so each field comes from one column.
    pub fn set_column(&mut self, index: usize, column: Column) {
        if column != Column::Ignore {
            for c in self.columns.iter_mut().filter(|c| **c == column) {
                *c = Column::Ignore;
            }
        }
        if let Some(c) = self.columns.get_mut(index) {
            *c = column;
        }
    }

    /// Remove Row: drops row `index` (the header too, if it is 0).
    pub fn remove_row(&mut self, index: usize) {
        if index < self.rows.len() {
            self.rows.remove(index);
            if index == 0 {
                self.header = false;
            }
        }
    }

    /// Remove Column: drops column `index` from every row and the mapping.
    pub fn remove_column(&mut self, index: usize) {
        if index < self.columns.len() {
            self.columns.remove(index);
            for row in &mut self.rows {
                row.remove(index);
            }
        }
    }

    /// The Pins the data rows describe, skipping rows with an empty number; types and
    /// sides are read with [`pin_type`] and [`side`].
    pub fn pins(&self) -> Result<Vec<WizardPin>, PasteError> {
        let col = |c: Column| self.columns.iter().position(|x| *x == c);
        let Some(number) = col(Column::Number) else {
            return Err(PasteError {
                row: None,
                column: None,
                message: "no column is mapped to the Pin number".into(),
            });
        };
        let mut pins = Vec::new();
        for (r, row) in self.rows.iter().enumerate().skip(usize::from(self.header)) {
            if row[number].is_empty() {
                continue;
            }
            let mut pin = WizardPin::new(row[number].clone(), "");
            let bad = |c: usize, what: &str| PasteError {
                row: Some(r),
                column: Some(c),
                message: format!("{:?} is not {what}", row[c]),
            };
            if let Some(c) = col(Column::Name) {
                pin.name = row[c].clone();
            }
            if let Some(c) = col(Column::Type).filter(|&c| !row[c].is_empty()) {
                pin.electrical_type = pin_type(&row[c]).ok_or_else(|| bad(c, "a Pin type"))?;
            }
            if let Some(c) = col(Column::Side).filter(|&c| !row[c].is_empty()) {
                pin.side = Some(side(&row[c]).ok_or_else(|| bad(c, "a side"))?);
            }
            if let Some(c) = col(Column::Group).filter(|&c| !row[c].is_empty()) {
                pin.group = Some(row[c].clone());
            }
            pins.push(pin);
        }
        Ok(pins)
    }
}

/// One line's cells, trimmed. With a comma or semicolon, a cell in double quotes may
/// hold the delimiter, and `""` inside it is a quote.
fn split_line(line: &str, delimiter: Delimiter) -> Vec<String> {
    let sep = match delimiter {
        Delimiter::Whitespace => return line.split_whitespace().map(str::to_owned).collect(),
        Delimiter::Tab => return line.split('\t').map(|c| c.trim().to_owned()).collect(),
        Delimiter::Comma => ',',
        Delimiter::Semicolon => ';',
    };
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cell.push('"');
                chars.next();
            }
            '"' if quoted => quoted = false,
            '"' if cell.trim().is_empty() => {
                cell.clear();
                quoted = true;
            }
            c if c == sep && !quoted => cells.push(std::mem::take(&mut cell).trim().to_owned()),
            c => cell.push(c),
        }
    }
    cells.push(cell.trim().to_owned());
    cells
}

/// A first row is headings when at least half its non-empty cells name a column.
fn is_header(row: &[String]) -> bool {
    let cells: Vec<&String> = row.iter().filter(|c| !c.is_empty()).collect();
    let named = cells
        .iter()
        .filter(|c| Column::from_heading(c).is_some())
        .count();
    named > 0 && named * 2 >= cells.len()
}

/// A Pin number as packages write them: digits, after up to three letters (`7`, `A1`,
/// `AA12`).
fn number_like(cell: &str) -> bool {
    let letters = cell.chars().take_while(char::is_ascii_alphabetic).count();
    let rest = &cell[letters..];
    letters <= 3 && !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())
}

/// Each column's field from its cells: the first column of Pin numbers is the number,
/// the first of sides and of types are those, the first other non-empty column is the
/// name, and the rest are ignored.
fn guess_columns(rows: &[Vec<String>], width: usize) -> Vec<Column> {
    let all = |c: usize, test: &dyn Fn(&str) -> bool| {
        let mut cells = rows
            .iter()
            .map(|r| r[c].as_str())
            .filter(|s| !s.is_empty())
            .peekable();
        cells.peek().is_some() && cells.all(test)
    };
    let mut columns = vec![Column::Ignore; width];
    for c in 0..width {
        let column = if all(c, &number_like) {
            Column::Number
        } else if all(c, &|s| side(s).is_some()) {
            Column::Side
        } else if all(c, &|s| pin_type(s).is_some()) {
            Column::Type
        } else {
            continue;
        };
        if !columns.contains(&column) {
            columns[c] = column;
        }
    }
    if let Some(c) = (0..width).find(|&c| columns[c] == Column::Ignore && all(c, &|_| true)) {
        columns[c] = Column::Name;
    }
    columns
}
