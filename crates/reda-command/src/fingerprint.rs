//! A short fingerprint of the design data a check reads, for telling whether its results
//! are stale (`erc.stale`, `drc.stale`, Zone fill staleness).

use serde::Serialize;

/// 64-bit FNV-1a over each part's compact JSON, parts separated by 0x1f. JSON of the
/// model's ordered tables is canonical, so equal data gives an equal fingerprint in every
/// build of REDA, and a fingerprint recorded in a scenario stays valid.
#[derive(Clone, Copy, Debug)]
pub struct Fingerprint(u64);

impl Default for Fingerprint {
    fn default() -> Fingerprint {
        Fingerprint::new()
    }
}

impl Fingerprint {
    pub fn new() -> Fingerprint {
        Fingerprint(0xcbf2_9ce4_8422_2325)
    }

    /// Adds one part.
    pub fn add<T: Serialize + ?Sized>(&mut self, part: &T) -> &mut Fingerprint {
        serde_json::to_writer(&mut *self, part).expect("design data serialises to JSON");
        self.bytes(&[0x1f]);
        self
    }

    /// The fingerprint as 16 hex digits.
    pub fn finish(&self) -> String {
        format!("{:016x}", self.0)
    }

    fn bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
}

impl std::io::Write for Fingerprint {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.bytes(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
