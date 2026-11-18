//! Splits Rust source into code, string literals and whole-line comments, so the rules
//! match code only. It is a text pass, not a parser: enough to keep a pattern named in a
//! comment or a message from counting as a use.

/// One source file, taken apart.
pub struct Source {
    /// The text with every comment and the inside of every string and character literal
    /// replaced by spaces. Line breaks are kept, so offsets map to the same lines.
    pub code: String,
    /// Each string literal's contents, with the line it starts on (from 1).
    pub strings: Vec<(usize, String)>,
}

pub fn lex(text: &str) -> Source {
    let bytes = text.as_bytes();
    let mut code = bytes.to_vec();
    let mut strings = Vec::new();
    let mut line = 1;
    let mut i = 0;
    // Blanks `from..to` in the copy, keeping line breaks, and counts the lines passed.
    let blank = |code: &mut Vec<u8>, from: usize, to: usize, line: &mut usize| {
        for b in &mut code[from..to] {
            if *b == b'\n' {
                *line += 1;
            } else {
                *b = b' ';
            }
        }
    };
    while i < bytes.len() {
        let rest = &bytes[i..];
        if rest.starts_with(b"//") {
            let end = rest
                .iter()
                .position(|&b| b == b'\n')
                .map_or(bytes.len(), |n| i + n);
            blank(&mut code, i, end, &mut line);
            i = end;
        } else if rest.starts_with(b"/*") {
            let end = block_comment_end(bytes, i);
            blank(&mut code, i, end, &mut line);
            i = end;
        } else if let Some((open, close, end)) = string_at(bytes, i) {
            strings.push((
                line,
                String::from_utf8_lossy(&bytes[open..close]).into_owned(),
            ));
            blank(&mut code, open, close, &mut line);
            i = end;
        } else if let Some(end) = char_literal_end(bytes, i) {
            blank(&mut code, i + 1, end - 1, &mut line);
            i = end;
        } else {
            if bytes[i] == b'\n' {
                line += 1;
            }
            i += 1;
        }
    }
    Source {
        code: String::from_utf8_lossy(&code).into_owned(),
        strings,
    }
}

/// The offset after the block comment that opens at `start`. Block comments nest.
fn block_comment_end(bytes: &[u8], start: usize) -> usize {
    let mut depth = 0;
    let mut i = start;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"/*") {
            depth += 1;
            i += 2;
        } else if bytes[i..].starts_with(b"*/") {
            depth -= 1;
            i += 2;
            if depth == 0 {
                return i;
            }
        } else {
            i += 1;
        }
    }
    bytes.len()
}

/// A string literal that starts at `i`: where its contents begin and end, and the offset
/// after its closing quote. Covers plain, byte and raw strings.
fn string_at(bytes: &[u8], i: usize) -> Option<(usize, usize, usize)> {
    let after_ident = i > 0 && is_ident(bytes[i - 1]);
    let mut at = i;
    if !after_ident && bytes[at] == b'b' {
        at += 1;
    }
    if !after_ident && bytes.get(at) == Some(&b'r') {
        let hashes = bytes[at + 1..].iter().take_while(|&&b| b == b'#').count();
        let open = at + 1 + hashes;
        if bytes.get(open) == Some(&b'"') {
            let mut close = open + 1;
            while close < bytes.len() {
                let tail = &bytes[close + 1..];
                if bytes[close] == b'"'
                    && tail.len() >= hashes
                    && tail[..hashes].iter().all(|&b| b == b'#')
                {
                    return Some((open + 1, close, close + 1 + hashes));
                }
                close += 1;
            }
            return Some((open + 1, bytes.len(), bytes.len()));
        }
    }
    if bytes.get(at) != Some(&b'"') {
        return None;
    }
    let mut close = at + 1;
    while close < bytes.len() && bytes[close] != b'"' {
        close += if bytes[close] == b'\\' { 2 } else { 1 };
    }
    let close = close.min(bytes.len());
    Some((at + 1, close, (close + 1).min(bytes.len())))
}

/// The offset after the character literal that starts at `i`, or `None` when the quote
/// there opens a lifetime (or is not a quote).
fn char_literal_end(bytes: &[u8], i: usize) -> Option<usize> {
    if bytes[i] != b'\'' {
        return None;
    }
    if bytes.get(i + 1) == Some(&b'\\') {
        let close = bytes[i + 3..].iter().position(|&b| b == b'\'')?;
        return Some(i + 3 + close + 1);
    }
    // One character, of one to four bytes, then the closing quote.
    (2..=5)
        .find(|&n| {
            bytes.get(i + n) == Some(&b'\'')
                && std::str::from_utf8(&bytes[i + 1..i + n]).is_ok_and(|s| s.chars().count() == 1)
        })
        .map(|n| i + n + 1)
}

pub fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}
