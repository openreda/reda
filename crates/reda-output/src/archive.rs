//! The zip a job run writes: every file at the top level, deflated, in the order given,
//! each dated 1980-01-01 00:00 (the earliest date a zip holds), so the same files give the
//! same bytes on every run and platform. The run's date is in the Output Manifest alone.

use std::io::{Cursor, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, System, ZipWriter};

use crate::plot::OutputFile;

/// The zip of `files`, every entry dated 1980-01-01 00:00.
pub fn zip(files: &[OutputFile]) -> Vec<u8> {
    let stamp = DateTime::default();
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(stamp)
        // The zip crate's default origin is the platform it was built for (DOS on Windows),
        // which changes the bytes; every platform says Unix, with its permissions.
        .system(System::Unix)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Every central directory entry of `bytes`: its "version made by" and external
    /// attributes.
    fn central_entries(bytes: &[u8]) -> Vec<(u16, u32)> {
        let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
        let u32_at = |i: usize| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
        (0..bytes.len().saturating_sub(4))
            .filter(|&i| bytes[i..i + 4] == *b"PK\x01\x02")
            .map(|i| (u16_at(i + 4), u32_at(i + 38)))
            .collect()
    }

    #[test]
    fn every_entry_says_unix_rw_r_r_whatever_platform_wrote_it() {
        let files = [
            OutputFile {
                name: "Demo.gtl".into(),
                function: "copper_top",
                bytes: b"G04 top*\nM02*\n".to_vec(),
                attributes: None,
            },
            OutputFile {
                name: "Demo-BOM.csv".into(),
                function: "bom",
                bytes: b"Designator\n".to_vec(),
                attributes: None,
            },
        ];
        let entries = central_entries(&zip(&files));
        assert_eq!(entries.len(), files.len());
        for (made_by, attributes) in entries {
            assert_eq!(made_by >> 8, 3, "made by Unix, not the build platform");
            assert_eq!(attributes, 0o100_644 << 16, "a regular file, rw-r--r--");
        }
    }
}
