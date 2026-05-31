//! In-memory observability fixtures for tap-output verification.

use std::sync::{Arc, Mutex};

use bus_contracts::events::BusOutboundEvent;
use bus_contracts::metadata::{Timestamp, TraceContextRef};

/// One tap-output record consumed by a fake observability client.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TapOutputRecord {
    /// The committed outbound event exposed as tap material.
    pub event: BusOutboundEvent,
    /// The trace reference supplied to the publisher call.
    pub trace_ref: TraceContextRef,
    /// The timestamp when the tap output became visible.
    pub published_at: Timestamp,
}

/// The shared fake sink used by tap-output tests.
pub type SharedTapOutputSink = Arc<Mutex<Vec<TapOutputRecord>>>;
