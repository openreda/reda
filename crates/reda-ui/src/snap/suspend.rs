//! Suspending snap levels with held keys while a Tool or drag is in flight: `Mod`
//! suspends object snap (and alignment guides), `Mod`+Shift the grid as well. A `Mod`
//! already held when the operation started means detach, so it suspends nothing until it
//! is released and pressed again.

/// The suspend keys' state, fed every frame by [`Suspend::update`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Suspend {
    /// `Mod` has been up since the operation started (or before it).
    armed: bool,
    objects: bool,
    grid: bool,
}

impl Default for Suspend {
    fn default() -> Suspend {
        Suspend {
            armed: true,
            objects: false,
            grid: false,
        }
    }
}

impl Suspend {
    /// This frame's keys: whether an operation is `in_flight`, and whether `Mod` and Shift
    /// are held.
    pub fn update(&mut self, in_flight: bool, modifier: bool, shift: bool) {
        if !in_flight {
            *self = Suspend {
                armed: !modifier,
                ..Suspend::default()
            };
            return;
        }
        if !modifier {
            self.armed = true;
        }
        self.objects = self.armed && modifier;
        self.grid = self.objects && shift;
    }

    /// Object snap and alignment guides are suspended.
    pub fn objects(&self) -> bool {
        self.objects
    }

    /// The grid is suspended too.
    pub fn grid(&self) -> bool {
        self.grid
    }
}
