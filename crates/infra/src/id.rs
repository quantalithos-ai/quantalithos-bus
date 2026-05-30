//! Deterministic ID generator adapters.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use bus_application::{BusRecordKind, IdGenerationError, IdGeneratorPort};

/// A deterministic record ID generator for tests.
#[derive(Clone, Default)]
pub struct DeterministicIdGenerator {
    counters: Arc<Mutex<BTreeMap<BusRecordKind, u64>>>,
}

impl DeterministicIdGenerator {
    /// Creates a fresh deterministic generator.
    pub fn new() -> Self {
        Self::default()
    }
}

impl IdGeneratorPort for DeterministicIdGenerator {
    fn next_record_id(&self, kind: BusRecordKind) -> Result<String, IdGenerationError> {
        let mut counters = self
            .counters
            .lock()
            .expect("deterministic id generator lock poisoned");
        let counter = counters.entry(kind).or_insert(0);
        *counter += 1;

        let prefix = match kind {
            BusRecordKind::AuditEntry => "audit",
            BusRecordKind::IdempotencyAnchor => "anchor",
        };

        Ok(format!("{prefix}_{counter:04}"))
    }
}
