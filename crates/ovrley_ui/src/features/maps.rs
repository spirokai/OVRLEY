//! The maps feature (phase 5 D9, D10): one timer for the maps' next deadline (a wheel's zoom
//! commit, the settled route download).

use ovrley_app::store::{DispatchHandle, Store};
use std::cell::Cell;
use std::time::Instant;

pub struct Projection {
    timer: slint::Timer,
    deadline: Cell<Option<Instant>>,
    dispatch: DispatchHandle,
}

impl Projection {
    pub fn new(dispatch: DispatchHandle) -> Self {
        Self { timer: slint::Timer::default(), deadline: Cell::new(None), dispatch }
    }

    /// Comes back at the maps' next deadline.
    pub fn project(&self, store: &Store) {
        let deadline = ovrley_app::features::maps::next_deadline(store);
        if deadline == self.deadline.get() {
            return;
        }
        self.deadline.set(deadline);
        let Some(deadline) = deadline else {
            self.timer.stop();
            return;
        };
        let dispatch = self.dispatch.clone();
        self.timer.start(slint::TimerMode::SingleShot, deadline.saturating_duration_since(Instant::now()), move || {
            dispatch.dispatch(|store, services| ovrley_app::features::maps::tick(store, services, Instant::now()));
        });
    }
}
