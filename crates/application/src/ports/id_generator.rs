//! Record ID generation port.

use crate::errors::IdGenerationError;

/// Supported record kinds for deterministic ID generation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BusRecordKind {
    /// An audit entry identifier.
    AuditEntry,
    /// An idempotency anchor identifier.
    IdempotencyAnchor,
}

/// Internal record ID generator.
pub trait IdGeneratorPort: Send + Sync {
    /// Allocates the next stable record identifier.
    fn next_record_id(&self, kind: BusRecordKind) -> Result<String, IdGenerationError>;
}
