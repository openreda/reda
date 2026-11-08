//! Automatic Zone refill (the `pcb.zone.auto_refill` Preference). After a committed
//! edit leaves Zones stale, `pcb.zone.fill` is detached from the engine
//! ([`Engine::detach`](reda_engine::Engine::detach)) and runs on its own thread: it
//! refills only the stale Zones, which are the edited ones and those depending on them,
//! since each Zone has its own input hash. The window polls it each frame and keeps its
//! fills when it ends.
//!
//! A fill never runs during a drag: starting one cancels a background fill in flight,
//! and the drag's own commit starts the next.

use std::thread::JoinHandle;

use reda_engine::Finished;
use reda_engine::command::{CancelToken, Signals};
use reda_engine::preferences::AutoRefill;
use serde_json::json;

use crate::shell::Shell;

/// One background refill in flight.
pub struct ZoneRefill {
    cancel: CancelToken,
    thread: JoinHandle<Finished>,
}

impl ZoneRefill {
    /// Starts refilling the stale Zones of `engine`'s board on a thread of its own.
    pub fn start(engine: &reda_engine::Engine) -> Option<ZoneRefill> {
        let detached = engine.detach("pcb.zone.fill", json!({})).ok()?;
        let cancel = CancelToken::new();
        let signals = Signals::new(cancel.clone());
        let thread = std::thread::Builder::new()
            .name("zone-refill".into())
            .spawn(move || detached.run(&signals))
            .ok()?;
        Some(ZoneRefill { cancel, thread })
    }

    /// Asks the fill to stop at its next Zone.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    /// Whether the fill has ended, so [`ZoneRefill::join`] will not block.
    pub fn is_finished(&self) -> bool {
        self.thread.is_finished()
    }

    /// Waits for the fill to end; `None` if its thread failed.
    pub fn join(self) -> Option<Finished> {
        self.thread.join().ok()
    }
}

impl Shell {
    /// After a committed edit: starts a background refill when the Preference asks for
    /// one and Zones are stale, or asks for another when one is already running.
    pub(crate) fn auto_refill(&mut self) {
        if self.preferences().pcb.zone.auto_refill != AutoRefill::AfterEdit
            || !self.zones_need_refill()
        {
            return;
        }
        if self.zone_refill.is_some() || self.pcb_dragging {
            self.zone_refill_again = true;
            return;
        }
        self.zone_refill = ZoneRefill::start(self.engine());
    }

    /// A drag began: a refill in flight stops, to run again once the drag is done.
    pub(crate) fn drag_started(&mut self) {
        self.pcb_dragging = true;
        if let Some(refill) = &self.zone_refill
            && !refill.is_cancelled()
        {
            refill.cancel();
            self.zone_refill_again = true;
        }
    }

    /// Whether a background refill is in flight.
    pub fn zone_refill_running(&self) -> bool {
        self.zone_refill.is_some()
    }

    /// Called each frame: keeps the fills of a finished background refill (dropping a
    /// cancelled one) and starts the next one asked for, unless a drag is on. Returns
    /// whether a refill is still in flight.
    pub fn poll_zone_refill(&mut self) -> bool {
        if self
            .zone_refill
            .as_ref()
            .is_some_and(ZoneRefill::is_finished)
        {
            self.end_zone_refill();
        }
        if self.zone_refill.is_none() && self.zone_refill_again && !self.pcb_dragging {
            self.zone_refill_again = false;
            self.auto_refill();
        }
        self.zone_refill.is_some()
    }

    /// Waits for background refills to end, including the ones asked for meanwhile.
    pub fn wait_zone_refill(&mut self) {
        while self.zone_refill.is_some() || (self.zone_refill_again && !self.pcb_dragging) {
            self.end_zone_refill();
            self.poll_zone_refill();
        }
    }

    fn end_zone_refill(&mut self) {
        let Some(refill) = self.zone_refill.take() else {
            return;
        };
        let cancelled = refill.is_cancelled();
        if let Some(done) = refill.join()
            && !cancelled
        {
            let _ = self.finish_detached(done);
            self.refresh();
        }
    }
}
