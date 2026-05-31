//! Event payload and source-page DTOs for inbound and outbound bus cooperation.

use serde::{Deserialize, Serialize};

use crate::metadata::{
    AuditRef, BackendCapabilityRef, BackendCapabilityStatus, BackendId, BackendKind,
    BackendResultRef, BackendStatus, CommittedOutboxFactRef, ConsistencyMarker,
    CoreEventEnvelopeRef, CoreEventRef, DeadLetterId, DeliveryAttemptId, DeliveryHistoryRef,
    DeliveryId, DeliveryMode, DeliveryStatus, EventId, EventSourceRef, FailureKind,
    FailureMaterialRef, FeedbackId, FeedbackKind, FeedbackRecordStatus, IdempotencyKey,
    OutboxCursor, PayloadDigest, PayloadKind, PayloadRef, ProjectionVersion, PublicationId,
    RejectionReasonRef, ReplayApprovalRef, ReplayPreparationId, SourceRecordRef, SourceSystem,
    TargetScope, TimeoutReason, Timestamp, TransportViewId,
};

/// The fixed payload schema version emitted by the bus.
pub const BUS_OUTBOUND_EVENT_SCHEMA_VERSION: &str = "v1";

/// One event payload validation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutboundEventValidationError {
    /// The payload uses an unsupported schema version.
    InvalidSchemaVersion,
    /// One required field was empty.
    MissingField(&'static str),
    /// One payload reference crossed the forbidden-body boundary.
    ForbiddenPayloadReference,
}

/// One committed publication-accepted outbound payload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationAcceptedEvent {
    /// The payload schema version.
    pub schema_version: String,
    /// The committed publication identifier.
    pub publication_id: PublicationId,
    /// The stable referenced core event contract.
    pub core_event_ref: CoreEventRef,
    /// The optional committed core event-envelope reference.
    pub core_event_envelope_ref: Option<CoreEventEnvelopeRef>,
    /// The committed source-record reference.
    pub source_record_ref: SourceRecordRef,
    /// The committed payload reference.
    pub payload_ref: PayloadRef,
    /// The committed delivery mode.
    pub delivery_mode: DeliveryMode,
    /// The committed target scope.
    pub target_scope: TargetScope,
    /// The committed terminal audit reference.
    pub audit_ref: AuditRef,
}

/// One committed publication-rejected outbound payload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationRejectedEvent {
    /// The payload schema version.
    pub schema_version: String,
    /// The committed publication identifier.
    pub publication_id: PublicationId,
    /// The committed source-record reference.
    pub source_record_ref: SourceRecordRef,
    /// The committed rejection reason reference.
    pub rejection_reason_ref: RejectionReasonRef,
    /// The committed terminal audit reference.
    pub audit_ref: AuditRef,
}

/// One committed delivery-state-changed outbound payload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryStateChangedEvent {
    /// The payload schema version.
    pub schema_version: String,
    /// The committed delivery identifier.
    pub delivery_id: DeliveryId,
    /// The previous delivery status.
    pub from_status: DeliveryStatus,
    /// The new delivery status.
    pub to_status: DeliveryStatus,
    /// The stable history reference for the transition.
    pub history_ref: DeliveryHistoryRef,
    /// The committed audit reference.
    pub audit_ref: AuditRef,
}

/// One committed feedback-recorded outbound payload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackRecordedEvent {
    /// The payload schema version.
    pub schema_version: String,
    /// The committed feedback identifier.
    pub feedback_id: FeedbackId,
    /// The committed delivery identifier.
    pub delivery_id: DeliveryId,
    /// The normalized feedback kind.
    pub feedback_kind: FeedbackKind,
    /// The stable feedback record status.
    pub feedback_status: FeedbackRecordStatus,
    /// The committed audit reference.
    pub audit_ref: AuditRef,
}

/// One committed dead-letter-created outbound payload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeadLetterCreatedEvent {
    /// The payload schema version.
    pub schema_version: String,
    /// The committed dead-letter identifier.
    pub dead_letter_id: DeadLetterId,
    /// The committed delivery identifier.
    pub delivery_id: DeliveryId,
    /// The committed failure-material reference.
    pub failure_material_ref: FailureMaterialRef,
    /// The committed audit reference.
    pub audit_ref: AuditRef,
}

/// One committed replay-preparation-ready outbound payload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayPreparationReadyEvent {
    /// The payload schema version.
    pub schema_version: String,
    /// The committed replay-preparation identifier.
    pub replay_preparation_id: ReplayPreparationId,
    /// The committed dead-letter identifier.
    pub dead_letter_id: DeadLetterId,
    /// The committed replay-approval reference.
    pub approval_ref: ReplayApprovalRef,
    /// The committed audit reference.
    pub audit_ref: AuditRef,
}

/// One committed transport-view-updated outbound payload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransportViewUpdatedEvent {
    /// The payload schema version.
    pub schema_version: String,
    /// The committed transport-view identifier.
    pub transport_view_id: TransportViewId,
    /// The committed delivery identifier.
    pub delivery_id: DeliveryId,
    /// The committed projection version.
    pub projection_version: ProjectionVersion,
    /// The current consistency marker exposed to readers.
    pub consistency_marker: ConsistencyMarker,
}

/// One committed failure-material-available outbound payload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailureMaterialAvailableEvent {
    /// The payload schema version.
    pub schema_version: String,
    /// The committed failure-material reference.
    pub failure_material_ref: FailureMaterialRef,
    /// The committed delivery identifier.
    pub delivery_id: DeliveryId,
    /// The stable failure classification.
    pub failure_kind: FailureKind,
    /// The committed audit reference.
    pub audit_ref: AuditRef,
}

/// One committed backend-capability-changed outbound payload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackendCapabilityChangedEvent {
    /// The payload schema version.
    pub schema_version: String,
    /// The committed backend identifier.
    pub backend_id: BackendId,
    /// The committed backend kind.
    pub backend_kind: BackendKind,
    /// The committed capability status.
    pub capability_status: BackendCapabilityStatus,
    /// The timestamp of the capability check.
    pub checked_at: Timestamp,
}

/// The supported committed outbound payloads emitted by the bus.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event_kind", content = "payload", rename_all = "snake_case")]
pub enum BusOutboundEventPayload {
    /// One publication-accepted payload.
    PublicationAccepted(PublicationAcceptedEvent),
    /// One publication-rejected payload.
    PublicationRejected(PublicationRejectedEvent),
    /// One delivery-state-changed payload.
    DeliveryStateChanged(DeliveryStateChangedEvent),
    /// One feedback-recorded payload.
    FeedbackRecorded(FeedbackRecordedEvent),
    /// One dead-letter-created payload.
    DeadLetterCreated(DeadLetterCreatedEvent),
    /// One replay-preparation-ready payload.
    ReplayPreparationReady(ReplayPreparationReadyEvent),
    /// One transport-view-updated payload.
    TransportViewUpdated(TransportViewUpdatedEvent),
    /// One failure-material-available payload.
    FailureMaterialAvailable(FailureMaterialAvailableEvent),
    /// One backend-capability-changed payload.
    BackendCapabilityChanged(BackendCapabilityChangedEvent),
}

/// One committed outbound event passed to the publisher boundary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusOutboundEvent {
    /// The stable outbound event identifier carried by the shared envelope.
    pub event_id: EventId,
    /// The committed payload emitted by the bus.
    pub payload: BusOutboundEventPayload,
}

/// One batch of committed outbound events.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusOutboundEventBatch {
    /// The ordered outbound events in the current batch.
    pub items: Vec<BusOutboundEvent>,
}

impl BusOutboundEvent {
    /// Returns the stable topic name for the current payload.
    pub fn topic(&self) -> &'static str {
        match &self.payload {
            BusOutboundEventPayload::PublicationAccepted(_) => "bus.publication.accepted.v1",
            BusOutboundEventPayload::PublicationRejected(_) => "bus.publication.rejected.v1",
            BusOutboundEventPayload::DeliveryStateChanged(_) => "bus.delivery.state_changed.v1",
            BusOutboundEventPayload::FeedbackRecorded(_) => "bus.feedback.recorded.v1",
            BusOutboundEventPayload::DeadLetterCreated(_) => "bus.dead_letter.created.v1",
            BusOutboundEventPayload::ReplayPreparationReady(_) => "bus.replay_preparation.ready.v1",
            BusOutboundEventPayload::TransportViewUpdated(_) => "bus.transport_view.updated.v1",
            BusOutboundEventPayload::FailureMaterialAvailable(_) => {
                "bus.failure_material.available.v1"
            }
            BusOutboundEventPayload::BackendCapabilityChanged(_) => {
                "bus.backend.capability_changed.v1"
            }
        }
    }

    /// Returns the stable schema version carried by the payload.
    pub fn schema_version(&self) -> &str {
        match &self.payload {
            BusOutboundEventPayload::PublicationAccepted(payload) => &payload.schema_version,
            BusOutboundEventPayload::PublicationRejected(payload) => &payload.schema_version,
            BusOutboundEventPayload::DeliveryStateChanged(payload) => &payload.schema_version,
            BusOutboundEventPayload::FeedbackRecorded(payload) => &payload.schema_version,
            BusOutboundEventPayload::DeadLetterCreated(payload) => &payload.schema_version,
            BusOutboundEventPayload::ReplayPreparationReady(payload) => &payload.schema_version,
            BusOutboundEventPayload::TransportViewUpdated(payload) => &payload.schema_version,
            BusOutboundEventPayload::FailureMaterialAvailable(payload) => &payload.schema_version,
            BusOutboundEventPayload::BackendCapabilityChanged(payload) => &payload.schema_version,
        }
    }

    /// Returns the stable event record reference used for publish evidence idempotency.
    pub fn record_ref(&self) -> String {
        match &self.payload {
            BusOutboundEventPayload::PublicationAccepted(payload) => {
                payload.source_record_ref.as_str().to_owned()
            }
            BusOutboundEventPayload::PublicationRejected(payload) => {
                payload.source_record_ref.as_str().to_owned()
            }
            BusOutboundEventPayload::DeliveryStateChanged(payload) => {
                payload.history_ref.as_str().to_owned()
            }
            BusOutboundEventPayload::FeedbackRecorded(payload) => {
                payload.feedback_id.as_str().to_owned()
            }
            BusOutboundEventPayload::DeadLetterCreated(payload) => {
                payload.dead_letter_id.as_str().to_owned()
            }
            BusOutboundEventPayload::ReplayPreparationReady(payload) => {
                payload.replay_preparation_id.as_str().to_owned()
            }
            BusOutboundEventPayload::TransportViewUpdated(payload) => {
                payload.transport_view_id.as_str().to_owned()
            }
            BusOutboundEventPayload::FailureMaterialAvailable(payload) => {
                payload.failure_material_ref.as_str().to_owned()
            }
            BusOutboundEventPayload::BackendCapabilityChanged(payload) => {
                payload.backend_id.as_str().to_owned()
            }
        }
    }

    /// Returns the optional payload reference that must remain reference-only.
    pub fn payload_ref(&self) -> Option<&PayloadRef> {
        match &self.payload {
            BusOutboundEventPayload::PublicationAccepted(payload) => Some(&payload.payload_ref),
            _ => None,
        }
    }

    /// Validates the payload schema version and required fields.
    pub fn validate_schema(&self) -> Result<(), OutboundEventValidationError> {
        if self.schema_version() != BUS_OUTBOUND_EVENT_SCHEMA_VERSION {
            return Err(OutboundEventValidationError::InvalidSchemaVersion);
        }
        if self.event_id.as_str().trim().is_empty() {
            return Err(OutboundEventValidationError::MissingField("event_id"));
        }

        match &self.payload {
            BusOutboundEventPayload::PublicationAccepted(payload) => {
                validate_non_empty(payload.publication_id.as_str(), "publication_id")?;
                validate_non_empty(payload.core_event_ref.as_str(), "core_event_ref")?;
                validate_non_empty(payload.source_record_ref.as_str(), "source_record_ref")?;
                validate_non_empty(payload.payload_ref.as_str(), "payload_ref")?;
                validate_non_empty(payload.audit_ref.as_str(), "audit_ref")?;
                if looks_like_forbidden_payload_reference(payload.payload_ref.as_str()) {
                    return Err(OutboundEventValidationError::ForbiddenPayloadReference);
                }
            }
            BusOutboundEventPayload::PublicationRejected(payload) => {
                validate_non_empty(payload.publication_id.as_str(), "publication_id")?;
                validate_non_empty(payload.source_record_ref.as_str(), "source_record_ref")?;
                validate_non_empty(
                    payload.rejection_reason_ref.as_str(),
                    "rejection_reason_ref",
                )?;
                validate_non_empty(payload.audit_ref.as_str(), "audit_ref")?;
            }
            BusOutboundEventPayload::DeliveryStateChanged(payload) => {
                validate_non_empty(payload.delivery_id.as_str(), "delivery_id")?;
                validate_non_empty(payload.history_ref.as_str(), "history_ref")?;
                validate_non_empty(payload.audit_ref.as_str(), "audit_ref")?;
            }
            BusOutboundEventPayload::FeedbackRecorded(payload) => {
                validate_non_empty(payload.feedback_id.as_str(), "feedback_id")?;
                validate_non_empty(payload.delivery_id.as_str(), "delivery_id")?;
                validate_non_empty(payload.audit_ref.as_str(), "audit_ref")?;
            }
            BusOutboundEventPayload::DeadLetterCreated(payload) => {
                validate_non_empty(payload.dead_letter_id.as_str(), "dead_letter_id")?;
                validate_non_empty(payload.delivery_id.as_str(), "delivery_id")?;
                validate_non_empty(
                    payload.failure_material_ref.as_str(),
                    "failure_material_ref",
                )?;
                validate_non_empty(payload.audit_ref.as_str(), "audit_ref")?;
            }
            BusOutboundEventPayload::ReplayPreparationReady(payload) => {
                validate_non_empty(
                    payload.replay_preparation_id.as_str(),
                    "replay_preparation_id",
                )?;
                validate_non_empty(payload.dead_letter_id.as_str(), "dead_letter_id")?;
                validate_non_empty(payload.approval_ref.as_str(), "approval_ref")?;
                validate_non_empty(payload.audit_ref.as_str(), "audit_ref")?;
            }
            BusOutboundEventPayload::TransportViewUpdated(payload) => {
                validate_non_empty(payload.transport_view_id.as_str(), "transport_view_id")?;
                validate_non_empty(payload.delivery_id.as_str(), "delivery_id")?;
                validate_non_empty(payload.projection_version.as_str(), "projection_version")?;
            }
            BusOutboundEventPayload::FailureMaterialAvailable(payload) => {
                validate_non_empty(
                    payload.failure_material_ref.as_str(),
                    "failure_material_ref",
                )?;
                validate_non_empty(payload.delivery_id.as_str(), "delivery_id")?;
                validate_non_empty(payload.audit_ref.as_str(), "audit_ref")?;
            }
            BusOutboundEventPayload::BackendCapabilityChanged(payload) => {
                validate_non_empty(payload.backend_id.as_str(), "backend_id")?;
                validate_non_empty(payload.checked_at.as_str(), "checked_at")?;
            }
        }

        Ok(())
    }
}

fn validate_non_empty(
    value: &str,
    field: &'static str,
) -> Result<(), OutboundEventValidationError> {
    if value.trim().is_empty() {
        return Err(OutboundEventValidationError::MissingField(field));
    }

    Ok(())
}

fn looks_like_forbidden_payload_reference(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.starts_with('{')
        || trimmed.starts_with('[')
        || trimmed.contains('\n')
        || trimmed.contains('\r')
        || trimmed.contains("\"secret\"")
        || trimmed.contains("\"payload\"")
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::{
        BackendCapabilityStatus, BackendId, BackendKind, ConsistencyMarker, DeliveryHistoryRef,
        DeliveryMode, DeliveryStatus, FailureKind, FailureMaterialRef, FeedbackId, FeedbackKind,
        FeedbackRecordStatus, ProjectionVersion, PublicationId, ReplayApprovalRef,
        ReplayPreparationId, SourceRecordRef, TargetScope, Timestamp, TransportViewId,
    };

    fn publication_accepted_event() -> BusOutboundEvent {
        BusOutboundEvent {
            event_id: EventId::new("event_publication_accepted"),
            payload: BusOutboundEventPayload::PublicationAccepted(PublicationAcceptedEvent {
                schema_version: BUS_OUTBOUND_EVENT_SCHEMA_VERSION.to_owned(),
                publication_id: PublicationId::new("pub_01"),
                core_event_ref: CoreEventRef::new("core_event_contract_01"),
                core_event_envelope_ref: None,
                source_record_ref: SourceRecordRef::new("process_event_01"),
                payload_ref: PayloadRef::new("artifact_ref_01"),
                delivery_mode: DeliveryMode::AtLeastOnce,
                target_scope: TargetScope {
                    project_id: "project_01".to_owned(),
                    topic: "workitem.events".to_owned(),
                },
                audit_ref: AuditRef::new("audit_01"),
            }),
        }
    }

    fn transport_view_updated_event() -> BusOutboundEvent {
        BusOutboundEvent {
            event_id: EventId::new("event_transport_view_updated"),
            payload: BusOutboundEventPayload::TransportViewUpdated(TransportViewUpdatedEvent {
                schema_version: BUS_OUTBOUND_EVENT_SCHEMA_VERSION.to_owned(),
                transport_view_id: TransportViewId::new("transport_view_01"),
                delivery_id: DeliveryId::new("delivery_01"),
                projection_version: ProjectionVersion::initial(),
                consistency_marker: ConsistencyMarker::Committed,
            }),
        }
    }

    fn failure_material_available_event() -> BusOutboundEvent {
        BusOutboundEvent {
            event_id: EventId::new("event_failure_material_available"),
            payload: BusOutboundEventPayload::FailureMaterialAvailable(
                FailureMaterialAvailableEvent {
                    schema_version: BUS_OUTBOUND_EVENT_SCHEMA_VERSION.to_owned(),
                    failure_material_ref: FailureMaterialRef::new("failure_material_01"),
                    delivery_id: DeliveryId::new("delivery_01"),
                    failure_kind: FailureKind::TransportFailure,
                    audit_ref: AuditRef::new("audit_07"),
                },
            ),
        }
    }

    fn backend_capability_changed_event() -> BusOutboundEvent {
        BusOutboundEvent {
            event_id: EventId::new("event_backend_capability_changed"),
            payload: BusOutboundEventPayload::BackendCapabilityChanged(
                BackendCapabilityChangedEvent {
                    schema_version: BUS_OUTBOUND_EVENT_SCHEMA_VERSION.to_owned(),
                    backend_id: BackendId::new("backend_01"),
                    backend_kind: BackendKind::InMemory,
                    capability_status: BackendCapabilityStatus::Available,
                    checked_at: Timestamp::new("2026-05-31T00:00:00Z"),
                },
            ),
        }
    }

    #[test]
    fn publication_accepted_event_roundtrips() {
        let event = publication_accepted_event();

        let encoded = serde_json::to_string(&event).expect("event should serialize");
        let decoded: BusOutboundEvent =
            serde_json::from_str(&encoded).expect("event should deserialize");

        assert_eq!(decoded, event);
        assert_eq!(decoded.topic(), "bus.publication.accepted.v1");
        assert_eq!(decoded.record_ref(), "process_event_01");
    }

    #[test]
    fn transport_and_failure_events_stay_reference_only() {
        let transport = serde_json::to_string(&transport_view_updated_event())
            .expect("transport event should serialize");
        let failure = serde_json::to_string(&failure_material_available_event())
            .expect("failure event should serialize");
        let backend = serde_json::to_string(&backend_capability_changed_event())
            .expect("backend event should serialize");

        for payload in [transport, failure, backend] {
            assert!(!payload.contains("payload_body"));
            assert!(!payload.contains("raw_secret"));
            assert!(!payload.contains("governance_decision_ref"));
        }
    }

    #[test]
    fn outbound_event_validation_rejects_inline_payload_like_reference() {
        let mut event = publication_accepted_event();
        let BusOutboundEventPayload::PublicationAccepted(payload) = &mut event.payload else {
            panic!("expected publication accepted payload");
        };
        payload.payload_ref = PayloadRef::new("{\"payload\":\"inline\"}");

        assert_eq!(
            event.validate_schema(),
            Err(OutboundEventValidationError::ForbiddenPayloadReference)
        );
    }

    #[test]
    fn outbound_event_validation_rejects_wrong_schema_version() {
        let mut event = failure_material_available_event();
        let BusOutboundEventPayload::FailureMaterialAvailable(payload) = &mut event.payload else {
            panic!("expected failure material payload");
        };
        payload.schema_version = "v2".to_owned();

        assert_eq!(
            event.validate_schema(),
            Err(OutboundEventValidationError::InvalidSchemaVersion)
        );
    }

    #[test]
    fn event_batch_defaults_to_empty() {
        let batch = BusOutboundEventBatch::default();

        assert!(batch.items.is_empty());
    }

    #[test]
    fn all_outbound_payload_variants_serialize() {
        let events = vec![
            publication_accepted_event(),
            BusOutboundEvent {
                event_id: EventId::new("event_publication_rejected"),
                payload: BusOutboundEventPayload::PublicationRejected(PublicationRejectedEvent {
                    schema_version: BUS_OUTBOUND_EVENT_SCHEMA_VERSION.to_owned(),
                    publication_id: PublicationId::new("pub_02"),
                    source_record_ref: SourceRecordRef::new("process_event_02"),
                    rejection_reason_ref: RejectionReasonRef::new("reject_reason_02"),
                    audit_ref: AuditRef::new("audit_02"),
                }),
            },
            BusOutboundEvent {
                event_id: EventId::new("event_delivery_state_changed"),
                payload: BusOutboundEventPayload::DeliveryStateChanged(DeliveryStateChangedEvent {
                    schema_version: BUS_OUTBOUND_EVENT_SCHEMA_VERSION.to_owned(),
                    delivery_id: DeliveryId::new("delivery_02"),
                    from_status: DeliveryStatus::Dispatching,
                    to_status: DeliveryStatus::Delivered,
                    history_ref: DeliveryHistoryRef::new("history_02"),
                    audit_ref: AuditRef::new("audit_03"),
                }),
            },
            BusOutboundEvent {
                event_id: EventId::new("event_feedback_recorded"),
                payload: BusOutboundEventPayload::FeedbackRecorded(FeedbackRecordedEvent {
                    schema_version: BUS_OUTBOUND_EVENT_SCHEMA_VERSION.to_owned(),
                    feedback_id: FeedbackId::new("feedback_01"),
                    delivery_id: DeliveryId::new("delivery_03"),
                    feedback_kind: FeedbackKind::Ack,
                    feedback_status: FeedbackRecordStatus::Recorded,
                    audit_ref: AuditRef::new("audit_04"),
                }),
            },
            BusOutboundEvent {
                event_id: EventId::new("event_dead_letter_created"),
                payload: BusOutboundEventPayload::DeadLetterCreated(DeadLetterCreatedEvent {
                    schema_version: BUS_OUTBOUND_EVENT_SCHEMA_VERSION.to_owned(),
                    dead_letter_id: DeadLetterId::new("dead_letter_01"),
                    delivery_id: DeliveryId::new("delivery_04"),
                    failure_material_ref: FailureMaterialRef::new("failure_material_04"),
                    audit_ref: AuditRef::new("audit_05"),
                }),
            },
            BusOutboundEvent {
                event_id: EventId::new("event_replay_preparation_ready"),
                payload: BusOutboundEventPayload::ReplayPreparationReady(
                    ReplayPreparationReadyEvent {
                        schema_version: BUS_OUTBOUND_EVENT_SCHEMA_VERSION.to_owned(),
                        replay_preparation_id: ReplayPreparationId::new("replay_preparation_01"),
                        dead_letter_id: DeadLetterId::new("dead_letter_02"),
                        approval_ref: ReplayApprovalRef::new("approval_01"),
                        audit_ref: AuditRef::new("audit_06"),
                    },
                ),
            },
            transport_view_updated_event(),
            failure_material_available_event(),
            backend_capability_changed_event(),
        ];

        let encoded = serde_json::to_string(&events).expect("events should serialize");
        let decoded: Vec<BusOutboundEvent> =
            serde_json::from_str(&encoded).expect("events should deserialize");

        assert_eq!(decoded.len(), 9);
    }
}
