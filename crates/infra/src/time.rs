//! Clock adapters.

use bus_application::ClockPort;
use bus_contracts::metadata::Timestamp;

/// A fixed clock for deterministic tests.
#[derive(Clone, Debug)]
pub struct FixedClockAdapter {
    now: Timestamp,
}

impl FixedClockAdapter {
    /// Creates a fixed clock.
    pub fn new(now: Timestamp) -> Self {
        Self { now }
    }
}

impl ClockPort for FixedClockAdapter {
    fn now(&self) -> Timestamp {
        self.now.clone()
    }
}
