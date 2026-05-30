//! Outbox fact source port.

use bus_contracts::events::CommittedOutboxFactPage;
use bus_contracts::metadata::{CommittedOutboxFactRef, ConsumerMarker, OutboxCursor, PageLimit};

use crate::errors::SourcePortError;

/// Source port for committed upstream outbox facts.
pub trait OutboxFactSourcePort: Send + Sync {
    /// Polls committed outbox facts after the provided cursor.
    async fn poll_committed(
        &self,
        cursor: OutboxCursor,
        limit: PageLimit,
    ) -> Result<CommittedOutboxFactPage, SourcePortError>;

    /// Acknowledges that one committed fact has been consumed by the bus.
    async fn ack_consumed(
        &self,
        fact_ref: CommittedOutboxFactRef,
        marker: ConsumerMarker,
    ) -> Result<(), SourcePortError>;
}
