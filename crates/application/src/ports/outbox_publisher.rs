//! Outbound event publisher port.

use bus_contracts::events::{BusOutboundEvent, BusOutboundEventBatch};
use bus_contracts::metadata::{EventId, Timestamp, TraceContextRef};

use crate::errors::PublisherPortError;

/// One successful or idempotently reused publish receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishReceipt {
    /// The stable receipt reference.
    pub receipt_ref: String,
    /// The committed outbound event identifier.
    pub event_id: EventId,
    /// The emitted topic name.
    pub topic: String,
    /// The timestamp when the receipt was committed.
    pub published_at: Timestamp,
    /// Whether the receipt was reused from an earlier publish.
    pub duplicate: bool,
}

/// One batch publish receipt summary.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PublishBatchReceipt {
    /// The individual publish receipts.
    pub receipts: Vec<PublishReceipt>,
}

/// The stable result status recorded for publisher evidence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublishEvidenceStatus {
    /// The outbound event was published successfully.
    Published,
    /// The publish failed but may be retried later.
    RetryableFailed,
    /// The publish was rejected because the event contract was invalid.
    Rejected,
}

/// One persisted publish evidence record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublishEvidenceRecord {
    /// The stable evidence reference.
    pub evidence_ref: String,
    /// The committed outbound event identifier.
    pub event_id: EventId,
    /// The emitted topic name.
    pub topic: String,
    /// The publish-evidence idempotency record reference.
    pub record_ref: String,
    /// The payload schema version.
    pub schema_version: String,
    /// The persisted evidence status.
    pub status: PublishEvidenceStatus,
    /// The stable error code when the publish was not successful.
    pub error_code: Option<&'static str>,
}

/// Outbound publisher port for committed bus facts and projection updates.
pub trait OutboxPublisherPort: Send + Sync {
    /// Publishes one committed outbound event.
    async fn publish(
        &self,
        event: BusOutboundEvent,
        trace: TraceContextRef,
    ) -> Result<PublishReceipt, PublisherPortError>;

    /// Publishes one batch of committed outbound events.
    async fn publish_batch(
        &self,
        batch: BusOutboundEventBatch,
        trace: TraceContextRef,
    ) -> Result<PublishBatchReceipt, PublisherPortError>;
}
