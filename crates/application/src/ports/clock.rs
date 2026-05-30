//! Clock port.

use bus_contracts::metadata::Timestamp;

/// A source of current time for application services.
pub trait ClockPort: Send + Sync {
    /// Returns the current timestamp.
    fn now(&self) -> Timestamp;
}
