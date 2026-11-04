//! The engine's injected inputs: where new UUIDs and the current time come from.
//!
//! Nothing in the engine reads a global clock or random source. Real sessions use
//! [`RandomIds`] and [`SystemClock`]; tests use [`SeededIds`] and [`FixedClock::EPOCH`] so
//! saved files and results are byte-for-byte reproducible.

#[cfg(not(target_arch = "wasm32"))]
use std::hash::{BuildHasher, Hasher};

use reda_model::Uuid;

/// Hands out UUIDs for newly created objects.
pub trait IdSource: Send {
    fn next_id(&mut self) -> Uuid;
}

/// A deterministic counter for tests: `00000000-SSSS-4000-8000-NNNNNNNNNNNN`, where the
/// low 48 bits of the seed go to `S` (after the first group) and the count, starting at 1,
/// goes to `N`.
#[derive(Clone, Debug)]
pub struct SeededIds {
    seed: u64,
    count: u64,
}

impl SeededIds {
    pub fn new(seed: u64) -> SeededIds {
        SeededIds { seed, count: 0 }
    }
}

impl IdSource for SeededIds {
    fn next_id(&mut self) -> Uuid {
        self.count += 1;
        let seed = u128::from(self.seed & 0xffff_ffff_ffff);
        let count = u128::from(self.count & 0xffff_ffff_ffff);
        Uuid::v4_from_bits((seed << 80) | count)
    }
}

/// Random version-4 UUIDs for real sessions.
///
/// SplitMix64, seeded by [`RandomIds::new`] from the standard library's per-process random
/// hasher keys, the time and the process id. That is not a cryptographic source, which
/// UUIDs here do not need: they only have to be unique across sessions and machines.
/// wasm32 has neither a clock nor process ids in `std`, so a wasm host seeds it itself
/// with [`RandomIds::from_seed`] (from `crypto.getRandomValues`, say).
#[derive(Clone, Debug)]
pub struct RandomIds {
    state: u64,
}

impl RandomIds {
    /// Seeded from the hasher keys, the time and the process id.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new() -> RandomIds {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        if let Ok(since) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            hasher.write_u128(since.as_nanos());
        }
        hasher.write_u32(std::process::id());
        RandomIds::from_seed(hasher.finish())
    }

    /// Seeded by the caller, which must give each session a different seed.
    pub fn from_seed(seed: u64) -> RandomIds {
        RandomIds { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for RandomIds {
    fn default() -> RandomIds {
        RandomIds::new()
    }
}

impl IdSource for RandomIds {
    fn next_id(&mut self) -> Uuid {
        let bits = (u128::from(self.next_u64()) << 64) | u128::from(self.next_u64());
        Uuid::v4_from_bits(bits)
    }
}

/// A point in time: milliseconds since the Unix epoch, UTC.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(pub i64);

/// Tells the engine the current time.
pub trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;
}

/// The wall clock. Not on wasm32, whose `std` has no clock: a wasm host passes its own
/// [`Clock`] (from `Date.now()`, say).
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

#[cfg(not(target_arch = "wasm32"))]
impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        let since = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        Timestamp(i64::try_from(since.as_millis()).unwrap_or(i64::MAX))
    }
}

/// A clock that always reads the same time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FixedClock(pub Timestamp);

impl FixedClock {
    /// The fixed epoch tests use: 2026-01-01T00:00:00Z.
    pub const EPOCH: FixedClock = FixedClock(Timestamp(1_767_225_600_000));
}

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        self.0
    }
}
