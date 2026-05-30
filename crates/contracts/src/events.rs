//! Event payload and source-page DTOs for inbound and outbound bus cooperation.

use serde::{Deserialize, Serialize};

use crate::metadata::{
    BackendCapabilityRef, BackendResultRef, BackendStatus, CommittedOutboxFactRef,
    CoreEventEnvelopeRef, CoreEventRef, DeliveryAttemptId, DeliveryId, DeliveryMode, EventId,
    EventSourceRef, IdempotencyKey, OutboxCursor, PayloadDigest, PayloadKind, PayloadRef,
    SourceRecordRef, SourceSystem, TargetScope, TimeoutReason, Timestamp,
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
    /// The committed L0-core event-contract reference.
    pub core_event_ref: CoreEventRef,
    /// The committed outbox fact reference.
    pub committed_fact_ref: CommittedOutboxFactRef,
    /// The upstream source-system identifier.
    pub source_system: SourceSystem,
    /// The upstream source-record reference.
    pub source_record_ref: SourceRecordRef,
    /// The referenced payload body location.
    pub payload_ref: PayloadRef,
    /// The referenced payload kind.
    pub payload_kind: PayloadKind,
    /// The digest for the referenced payload body.
    pub payload_digest: PayloadDigest,
    /// The requested platform delivery mode.
    pub delivery_mode: DeliveryMode,
    /// The requested publication target scope.
    pub target_scope: TargetScope,
    /// The idempotency key attached to the upstream event.
    pub idempotency_key: IdempotencyKey,
}

/// One committed outbox fact converted into consumer-ready bus input.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommittedOutboxFactInput {
    /// The stable upstream event identifier.
    pub event_id: EventId,
    /// The stable upstream event-source reference.
    pub source_ref: EventSourceRef,
    /// The committed L0-core event-envelope reference.
    pub core_event_envelope_ref: CoreEventEnvelopeRef,
    /// The committed L0-core event-contract reference.
    pub core_event_ref: CoreEventRef,
    /// The committed outbox fact reference.
    pub committed_fact_ref: CommittedOutboxFactRef,
    /// The upstream source-system identifier.
    pub source_system: SourceSystem,
    /// The upstream source-record reference.
    pub source_record_ref: SourceRecordRef,
    /// The referenced payload body location.
    pub payload_ref: PayloadRef,
    /// The referenced payload kind.
    pub payload_kind: PayloadKind,
    /// The digest for the referenced payload body.
    pub payload_digest: PayloadDigest,
    /// The requested platform delivery mode.
    pub delivery_mode: DeliveryMode,
    /// The requested publication target scope.
    pub target_scope: TargetScope,
    /// The idempotency key attached to the upstream event.
    pub idempotency_key: IdempotencyKey,
}

impl CommittedOutboxFactInput {
    /// Converts one source fact into the consumer-ready input DTO.
    pub fn from_fact(fact: CommittedOutboxFact) -> Self {
        Self {
            event_id: fact.event_id,
            source_ref: fact.source_ref,
            core_event_envelope_ref: fact.core_event_envelope_ref,
            core_event_ref: fact.core_event_ref,
            committed_fact_ref: fact.committed_fact_ref,
            source_system: fact.source_system,
            source_record_ref: fact.source_record_ref,
            payload_ref: fact.payload_ref,
            payload_kind: fact.payload_kind,
            payload_digest: fact.payload_digest,
            delivery_mode: fact.delivery_mode,
            target_scope: fact.target_scope,
            idempotency_key: fact.idempotency_key,
        }
    }
}

/// One backend delivery signal received from a transport adapter.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackendDeliverySignalInput {
    /// The stable inbound signal identifier.
    pub event_id: EventId,
    /// The stable transport-adapter source reference.
    pub source_ref: EventSourceRef,
    /// The target delivery identifier.
    pub delivery_id: DeliveryId,
    /// The target delivery attempt identifier.
    pub attempt_id: DeliveryAttemptId,
    /// The backend capability that produced the signal.
    pub backend_capability_ref: BackendCapabilityRef,
    /// The raw backend status summary.
    pub backend_status: BackendStatus,
    /// The backend result reference without private response body.
    pub backend_result_ref: BackendResultRef,
    /// The signal idempotency key.
    pub idempotency_key: IdempotencyKey,
}

/// One timeout signal received from a scheduler or clock source.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryTimeoutSignalInput {
    /// The stable inbound signal identifier.
    pub event_id: EventId,
    /// The stable scheduler or clock source reference.
    pub source_ref: EventSourceRef,
    /// The target delivery identifier.
    pub delivery_id: DeliveryId,
    /// The target delivery attempt identifier.
    pub attempt_id: DeliveryAttemptId,
    /// The normalized timeout reason.
    pub timeout_reason: TimeoutReason,
    /// The externally observed timeout timestamp.
    pub occurred_at: Timestamp,
    /// The signal idempotency key.
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
