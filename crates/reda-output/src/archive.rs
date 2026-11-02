//! The zip a job run writes: every file at the top level, deflated, in the order given,
//! each stamped with the run's time from the injected `Clock`, so the same board and time
//! give the same bytes.

use std::io::{Cursor, Write};

use reda_command::Timestamp;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

use crate::civil::Civil;
use crate::plot::OutputFile;

/// The zip of `files`, stamped `created` (zip time stamps start in 1980: earlier times are
/// stamped 1980-01-01).
pub fn zip(files: &[OutputFile], created: Timestamp) -> Vec<u8> {
    let c = Civil::of(created);
    let narrow = |v: i64| u8::try_from(v).unwrap_or(0);
    let stamp = u16::try_from(c.year)
        .ok()
        .and_then(|year| {
            DateTime::from_date_and_time(
                year,
                narrow(c.month),
                narrow(c.day),
                narrow(c.hour),
                narrow(c.minute),
                narrow(c.second),
            )
            .ok()
        })
        .unwrap_or_default();
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(stamp)
        .unix_permissions(0o644);
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for file in files {
        writer
            .start_file(file.name.as_str(), options)
            .and_then(|()| writer.write_all(&file.bytes).map_err(Into::into))
            .expect("writing a zip in memory does not fail");
    }
    writer
        .finish()
        .expect("writing a zip in memory does not fail")
        .into_inner()
}
