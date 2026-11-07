// The body of the `import_kicad` fuzz target, kept apart so a stable test
// (`crates/reda-engine/tests/fuzz_seeds.rs`) runs the same check over the seeds.
//
// Import part's KiCad reader must refuse bad input with an error, never a panic. What
// imports must then write and read back as the same elements.
//
// Input layout: the first two bytes (little-endian) give the length of the `.kicad_sym`
// text; the rest of the input is the `.kicad_mod` text. Both are read as lossy UTF-8.

use std::path::Path;

use reda_engine::format::library::{
    read_footprint, read_part, read_symbol, write_footprint, write_part, write_symbol,
};
use reda_engine::model::Uuid;
use reda_import_kicad::{ImportOptions, Imported, import};

/// Runs one fuzz input; returns what imported, if anything did.
pub fn check(data: &[u8]) -> Option<Imported> {
    let (len, rest) = data.split_first_chunk::<2>()?;
    let split = usize::from(u16::from_le_bytes(*len)).min(rest.len());
    let (symbol_bytes, footprint_bytes) = rest.split_at(split);
    let symbol_text = String::from_utf8_lossy(symbol_bytes);
    let footprint_text = String::from_utf8_lossy(footprint_bytes);

    let mut n = 0u128;
    let mut new_id = || {
        n += 1;
        Uuid::v4_from_bits(n)
    };
    let imported = import(
        &symbol_text,
        &footprint_text,
        &ImportOptions::default(),
        &mut new_id,
    )
    .ok()?;

    let bytes = write_symbol(&imported.symbol).expect("an imported Symbol encodes");
    let symbol = read_symbol(Path::new("s.redasym"), &bytes).expect("its file reads back");
    assert_eq!(symbol, imported.symbol);
    let bytes = write_footprint(&imported.footprint).expect("an imported Footprint encodes");
    let footprint = read_footprint(Path::new("f.redafp"), &bytes).expect("its file reads back");
    assert_eq!(footprint, imported.footprint);
    let bytes = write_part(&imported.part).expect("an imported Part encodes");
    let part = read_part(Path::new("p.redapart"), &bytes).expect("its file reads back");
    assert_eq!(part, imported.part);
    Some(imported)
}
