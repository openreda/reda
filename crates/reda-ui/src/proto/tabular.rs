//! PROTOTYPE (issue #276): bakes a font's `tnum` feature into its character map, so the
//! tabular digits are what plain text gets. egui cannot switch OpenType features on, so
//! the bundled Inter has to carry them by default. In-house: reads the `tnum` single
//! substitutions with skrifa, then writes a new `cmap` (one format-12 subtable) and
//! re-lays the font's tables. The real build does this once, ahead of time, and checks
//! the result in; here it runs when the fonts are installed.

use std::collections::BTreeMap;

use skrifa::raw::tables::gsub::{SingleSubst, SubstitutionSubtables};
use skrifa::raw::{FontRef, TableProvider};
use skrifa::{MetadataProvider, Tag};

/// `font` with every character whose glyph `tnum` replaces mapped to the replacement.
/// Returns the font unchanged if it has no `tnum` feature or cannot be read.
pub fn bake(font: &[u8]) -> Vec<u8> {
    try_bake(font).unwrap_or_else(|| font.to_vec())
}

fn try_bake(bytes: &[u8]) -> Option<Vec<u8>> {
    let font = FontRef::new(bytes).ok()?;
    let swaps = tnum(&font)?;
    if swaps.is_empty() {
        return None;
    }
    let mut map: BTreeMap<u32, u32> = BTreeMap::new();
    for (ch, glyph) in font.charmap().mappings() {
        let glyph = glyph.to_u32();
        map.insert(ch, swaps.get(&glyph).copied().unwrap_or(glyph));
    }
    replace_table(bytes, *b"cmap", &cmap(&map))
}

/// The glyph swaps the font's `tnum` feature makes (single substitutions only).
fn tnum(font: &FontRef) -> Option<BTreeMap<u32, u32>> {
    let gsub = font.gsub().ok()?;
    let features = gsub.feature_list().ok()?;
    let lookups = gsub.lookup_list().ok()?;
    let mut swaps = BTreeMap::new();
    for record in features.feature_records() {
        if record.feature_tag() != Tag::new(b"tnum") {
            continue;
        }
        let feature = record.feature(features.offset_data()).ok()?;
        for index in feature.lookup_list_indices() {
            let lookup = lookups.lookups().get(index.get() as usize).ok()?;
            let SubstitutionSubtables::Single(tables) = lookup.subtables().ok()? else {
                continue;
            };
            for table in tables.iter() {
                match table.ok()? {
                    SingleSubst::Format1(t) => {
                        let delta = i32::from(t.delta_glyph_id());
                        for glyph in t.coverage().ok()?.iter() {
                            let from = glyph.to_u32();
                            let to = (from as i32 + delta).rem_euclid(65536) as u32;
                            swaps.entry(from).or_insert(to);
                        }
                    }
                    SingleSubst::Format2(t) => {
                        let to = t.substitute_glyph_ids();
                        for (glyph, to) in t.coverage().ok()?.iter().zip(to) {
                            swaps.entry(glyph.to_u32()).or_insert(to.get().to_u32());
                        }
                    }
                }
            }
        }
    }
    Some(swaps)
}

/// A `cmap` table holding one format-12 subtable, for Unicode and for Windows.
fn cmap(map: &BTreeMap<u32, u32>) -> Vec<u8> {
    let mut groups: Vec<(u32, u32, u32)> = Vec::new();
    for (&ch, &glyph) in map {
        match groups.last_mut() {
            Some((start, end, first)) if ch == *end + 1 && glyph == *first + (ch - *start) => {
                *end = ch;
            }
            _ => groups.push((ch, ch, glyph)),
        }
    }
    let mut out = Vec::new();
    let subtable_at = 4 + 2 * 8;
    out.extend(0u16.to_be_bytes());
    out.extend(2u16.to_be_bytes());
    for (platform, encoding) in [(0u16, 4u16), (3, 10)] {
        out.extend(platform.to_be_bytes());
        out.extend(encoding.to_be_bytes());
        out.extend((subtable_at as u32).to_be_bytes());
    }
    out.extend(12u16.to_be_bytes());
    out.extend(0u16.to_be_bytes());
    out.extend((16 + 12 * groups.len() as u32).to_be_bytes());
    out.extend(0u32.to_be_bytes());
    out.extend((groups.len() as u32).to_be_bytes());
    for (start, end, glyph) in groups {
        out.extend(start.to_be_bytes());
        out.extend(end.to_be_bytes());
        out.extend(glyph.to_be_bytes());
    }
    out
}

/// The font with table `tag` replaced, every table laid out again on 4-byte boundaries.
fn replace_table(font: &[u8], tag: [u8; 4], with: &[u8]) -> Option<Vec<u8>> {
    let u16_at = |at: usize| Some(u16::from_be_bytes(font.get(at..at + 2)?.try_into().ok()?));
    let u32_at = |at: usize| Some(u32::from_be_bytes(font.get(at..at + 4)?.try_into().ok()?));
    let count = usize::from(u16_at(4)?);
    let mut tables: Vec<([u8; 4], Vec<u8>)> = Vec::new();
    for i in 0..count {
        let record = 12 + 16 * i;
        let name: [u8; 4] = font.get(record..record + 4)?.try_into().ok()?;
        let (at, len) = (u32_at(record + 8)? as usize, u32_at(record + 12)? as usize);
        let data = if name == tag {
            with.to_vec()
        } else {
            font.get(at..at + len)?.to_vec()
        };
        tables.push((name, data));
    }
    let mut out = font.get(..12)?.to_vec();
    let mut body = Vec::new();
    for (name, data) in &tables {
        let at = 12 + 16 * count + body.len();
        let mut sum = 0u32;
        for chunk in data.chunks(4) {
            let mut word = [0u8; 4];
            word[..chunk.len()].copy_from_slice(chunk);
            sum = sum.wrapping_add(u32::from_be_bytes(word));
        }
        out.extend(name);
        out.extend(sum.to_be_bytes());
        out.extend((at as u32).to_be_bytes());
        out.extend((data.len() as u32).to_be_bytes());
        body.extend(data);
        while body.len() % 4 != 0 {
            body.push(0);
        }
    }
    out.extend(body);
    Some(out)
}
