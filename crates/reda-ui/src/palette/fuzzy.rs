//! Fuzzy matching: every query letter must appear in the text in order. Matches score
//! higher when letters start words or follow each other, so initials ("zf" for Zoom to
//! Fit) and prefixes ("save proj") rank first.

const MATCH: i32 = 1;
const WORD_START: i32 = 8;
const RUN: i32 = 7;
const GAP: i32 = 3;

/// How well `query` matches `text`, ignoring case and the query's spaces; `None` when it
/// does not. An empty query matches everything with 0.
pub fn score(query: &str, text: &str) -> Option<i32> {
    let query: Vec<char> = query
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    if query.is_empty() {
        return Some(0);
    }
    let raw: Vec<char> = text.chars().collect();
    let text: Vec<char> = raw
        .iter()
        .map(|c| c.to_lowercase().next().unwrap_or(*c))
        .collect();
    let starts: Vec<bool> = (0..raw.len())
        .map(|j| {
            j == 0
                || matches!(raw[j - 1], ' ' | '.' | '_' | ':' | '-' | '/')
                || (raw[j - 1].is_lowercase() && raw[j].is_uppercase())
        })
        .collect();
    // best[j]: the best score with the query so far matched and its last letter at j.
    let mut best: Vec<Option<i32>> = vec![None; text.len()];
    for (i, q) in query.iter().enumerate() {
        let mut next = vec![None; text.len()];
        // The best score of the previous letter at any position before j - 1.
        let mut earlier: Option<i32> = None;
        for j in 0..text.len() {
            if j >= 2 {
                earlier = earlier.max(best[j - 2]);
            }
            if text[j] != *q {
                continue;
            }
            let own = MATCH + if starts[j] { WORD_START } else { 0 };
            next[j] = if i == 0 {
                Some(own - if j == 0 { 0 } else { GAP })
            } else {
                let run = j.checked_sub(1).and_then(|k| best[k]).map(|s| s + RUN);
                let gap = earlier.map(|s| s - GAP);
                run.max(gap).map(|s| s + own)
            };
        }
        best = next;
    }
    best.into_iter().flatten().max()
}

#[cfg(test)]
mod tests {
    use super::score;

    #[test]
    fn letters_must_come_in_order() {
        assert!(score("zf", "Zoom to Fit").is_some());
        assert!(score("fz", "Zoom to Fit").is_none());
        assert!(score("", "anything") == Some(0));
    }

    #[test]
    fn word_starts_and_runs_beat_scattered_letters() {
        assert!(score("zf", "Zoom to Fit") > score("zf", "Fuzzy filter"));
        assert!(score("save", "Save Project") > score("save", "Show active vertex"));
        assert!(score("pro", "Project") > score("pro", "Set Pin Role Options"));
    }
}
