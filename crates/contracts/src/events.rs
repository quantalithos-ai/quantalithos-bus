//! Event payload and source-page DTOs for inbound and outbound bus cooperation.

use serde::{Deserialize, Serialize};

use crate::metadata::{
    CommittedOutboxFactRef, CoreEventEnvelopeRef, EventId, EventSourceRef, IdempotencyKey,
    OutboxCursor, PayloadDigest, PayloadRef, SourceRecordRef,
};

/// One committed outbox fact produced by the upstream source.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommittedOutboxFact {
    /// The stable upstream event identifier.
    pub event_id: EventId,
    /// The stable upstream event-source reference.
    pub source_ref: EventSourceRef,
    /// The committed L0-core event-envelope reference.
    pub core_event_envelope_ref: CoreEventEnvelopeRef,
    /// The committed outbox fact reference.
    pub committed_fact_ref: CommittedOutboxFactRef,
    /// The upstream source-record reference.
    pub source_record_ref: SourceRecordRef,
    /// The referenced payload body location.
    pub payload_ref: PayloadRef,
    /// The digest for the referenced payload body.
    pub payload_digest: PayloadDigest,
    /// The idempotency key attached to the upstream event.
    pub idempotency_key: IdempotencyKey,
}

/// One page of committed outbox facts returned by a source adapter.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommittedOutboxFactPage {
    /// The committed facts returned in the current page.
    pub items: Vec<CommittedOutboxFact>,
    /// The cursor to use for the next poll.
    pub next_cursor: OutboxCursor,
}

impl CommittedOutboxFactPage {
    /// Builds an empty page that preserves the caller cursor.
    pub fn empty(next_cursor: OutboxCursor) -> Self {
        Self {
            items: Vec::new(),
            next_cursor,
        }
    }
}
