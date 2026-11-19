/// REDA writes UTF-8 CSV with LF record separators and doubled quotes inside quoted cells.
pub fn read_csv(csv: &str) -> Vec<Vec<String>> {
    let mut records = Vec::new();
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = csv.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.next_if_eq(&'"').is_some() => cell.push('"'),
            '"' => quoted = !quoted,
            ',' if !quoted => cells.push(std::mem::take(&mut cell)),
            '\n' if !quoted => {
                cells.push(std::mem::take(&mut cell));
                records.push(std::mem::take(&mut cells));
            }
            c => cell.push(c),
        }
    }
    assert!(
        !quoted && cells.is_empty() && cell.is_empty(),
        "unterminated CPL record"
    );
    records
}
