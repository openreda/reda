//! The byte format of a PCB Document's Zone fill cache (`.reda/<document uuid>.zonefill`).
//!
//! Little-endian throughout: the magic `REDAFILL`, a `u32` format version, a `u32` Zone
//! count, then per Zone its 16-byte UUID, its input hash (`u32` length + UTF-8), a `u32`
//! shape count and per shape a `u32` ring count (outer first, then holes) and per ring a
//! `u32` point count and the points as `i64` x, y in nm. Anything malformed reads as no
//! cache, which only means a refill.

use std::collections::BTreeMap;

use reda_model::geom::{Nm, Point};
use reda_model::{FillShape, Uuid, ZoneFill, ZoneFills};

const MAGIC: &[u8; 8] = b"REDAFILL";
const VERSION: u32 = 2;

/// The cache bytes of `fills`.
pub fn encode(fills: &ZoneFills) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    put_u32(&mut out, VERSION);
    put_u32(&mut out, fills.zones.len() as u32);
    for (zone, fill) in &fills.zones {
        out.extend_from_slice(&zone.as_u128().to_le_bytes());
        put_u32(&mut out, fill.inputs.len() as u32);
        out.extend_from_slice(fill.inputs.as_bytes());
        put_u32(&mut out, fill.shapes.len() as u32);
        for shape in &fill.shapes {
            put_u32(&mut out, 1 + shape.holes.len() as u32);
            for ring in std::iter::once(&shape.outer).chain(&shape.holes) {
                put_u32(&mut out, ring.len() as u32);
                for p in ring {
                    out.extend_from_slice(&p.x.0.to_le_bytes());
                    out.extend_from_slice(&p.y.0.to_le_bytes());
                }
            }
        }
    }
    out
}

/// The fills in cache bytes, or `None` when they are not a cache this REDA wrote.
pub fn decode(bytes: &[u8]) -> Option<ZoneFills> {
    let mut r = Reader { bytes, at: 0 };
    if r.take(8)? != MAGIC || r.u32()? != VERSION {
        return None;
    }
    let mut zones = BTreeMap::new();
    for _ in 0..r.u32()? {
        let uuid = Uuid::from_u128(u128::from_le_bytes(r.take(16)?.try_into().ok()?));
        let len = r.u32()? as usize;
        let inputs = String::from_utf8(r.take(len)?.to_vec()).ok()?;
        let mut shapes = Vec::new();
        for _ in 0..r.u32()? {
            let rings = r.u32()?;
            let mut list = Vec::new();
            for _ in 0..rings {
                let n = r.u32()? as usize;
                // Each point takes 16 bytes: refuse counts the bytes cannot hold.
                if n > r.left() / 16 {
                    return None;
                }
                let mut ring = Vec::with_capacity(n);
                for _ in 0..n {
                    ring.push(Point::new(Nm(r.i64()?), Nm(r.i64()?)));
                }
                list.push(ring);
            }
            let mut list = list.into_iter();
            let outer = list.next()?;
            shapes.push(FillShape {
                outer,
                holes: list.collect(),
            });
        }
        zones.insert(uuid, ZoneFill { inputs, shapes });
    }
    (r.left() == 0).then_some(ZoneFills { zones })
}

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(n)?;
        let out = self.bytes.get(self.at..end)?;
        self.at = end;
        Some(out)
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn i64(&mut self) -> Option<i64> {
        Some(i64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }

    fn left(&self) -> usize {
        self.bytes.len() - self.at
    }
}
