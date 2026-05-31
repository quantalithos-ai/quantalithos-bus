//! Read-only DTOs returned by bus query APIs.

use serde::{Deserialize, Serialize};

use crate::metadata::{
    ActorRef, AuditEventKind, AuditRef, BackendCapabilityStatus, BackendId, BackendKind,
    ConsistencyMarker, CoreEventEnvelopeRef, CoreEventRef, DeliveryAttemptId, DeliveryHistoryId,
    DeliveryId, DeliveryMode, DeliveryStatus, FailureKind, FailureMaterialRef, FeedbackId,
    PageToken, PayloadRef, ProjectionVersion, PublicationAcceptanceStatus, PublicationId,
    RejectionReasonRef, SourceRecordRef, TargetScope, Timestamp, TransportViewId,
};

/// Returns the current committed state of one publication acceptance.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationAcceptanceView {
    /// The publication identifier.
    pub publication_id: PublicationId,
    /// The committed acceptance state.
    pub acceptance_status: PublicationAcceptanceStatus,
    /// The stable source-record reference.
    pub source_record_ref: SourceRecordRef,
    /// The stable referenced core event contract.
    pub core_event_ref: CoreEventRef,
    /// The optional committed core event-envelope reference.
    pub core_event_envelope_ref: Option<CoreEventEnvelopeRef>,
    /// The committed payload reference.
    pub payload_ref: PayloadRef,
    /// The committed delivery mode.
    pub delivery_mode: DeliveryMode,
    /// The committed target scope.
    pub target_scope: TargetScope,
    /// The optional rejection reason reference for rejected publications.
    pub rejection_reason_ref: Option<RejectionReasonRef>,
    /// The terminal audit reference.
    pub audit_ref: AuditRef,
}

/// Returns the current committed status of a delivery.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryStatusView {
    /// The delivery identifier.
    pub delivery_id: DeliveryId,
    /// The associated publication identifier.
    pub publication_id: PublicationId,
    /// The current delivery lifecycle state.
    pub delivery_status: DeliveryStatus,
    /// The current attempt identifier, if an attempt exists.
    pub current_attempt_id: Option<DeliveryAttemptId>,
    /// The last feedback identifier, if feedback has already been recorded.
    pub last_feedback_id: Option<FeedbackId>,
    /// The consistency marker for the returned view.
    pub consistency_marker: ConsistencyMarker,
}

/// One append-only delivery-history item.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryHistoryItemView {
    /// The history-entry identifier.
    pub history_id: DeliveryHistoryId,
    /// The previous delivery status.
    pub from_status: DeliveryStatus,
    /// The new delivery status.
    pub to_status: DeliveryStatus,
    /// The stable transition reason.
    pub reason: crate::metadata::HistoryReason,
    /// The timestamp when the transition occurred.
    pub occurred_at: Timestamp,
}

/// One page of append-only delivery-history items.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryHistoryPage {
    /// The target delivery identifier.
    pub delivery_id: DeliveryId,
    /// The returned history items.
    pub items: Vec<DeliveryHistoryItemView>,
    /// The next page token, if additional items remain.
    pub next_cursor: Option<PageToken>,
}

/// Returns one SDK-facing or operator-facing transport view.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransportView {
    /// The transport-view identifier.
    pub transport_view_id: TransportViewId,
    /// The underlying delivery identifier.
    pub delivery_id: DeliveryId,
    /// The current delivery status exposed by the view.
    pub transport_status: DeliveryStatus,
    /// The bus transport semantic exposed by the view.
    pub transport_semantic: DeliveryMode,
    /// The projection version used to build the view.
    pub projection_version: ProjectionVersion,
    /// The consistency marker for the returned view.
    pub consistency_marker: ConsistencyMarker,
}

/// Returns one governance-facing failure summary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailureSummaryView {
    /// The failure-summary identifier.
    pub failure_summary_id: crate::metadata::FailureSummaryId,
    /// The affected delivery identifier.
    pub delivery_id: DeliveryId,
    /// The committed failure-material reference.
    pub failure_material_ref: FailureMaterialRef,
    /// The stable failure classification.
    pub failure_kind: FailureKind,
    /// The optional governance decision reference.
    pub governance_decision_ref: Option<String>,
}

/// One append-only audit-trail item.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusAuditTrailItemView {
    /// The stable audit reference.
    pub audit_ref: AuditRef,
    /// The committed monotonic audit sequence.
    pub audit_sequence: u64,
    /// The audited record reference.
    pub record_ref: String,
    /// The stable event kind.
    pub event_kind: AuditEventKind,
    /// The actor that triggered the event.
    pub actor_ref: ActorRef,
    /// The timestamp when the event occurred.
    pub occurred_at: Timestamp,
}

/// One page of bus audit-trail items.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusAuditTrailView {
    /// The returned audit items.
    pub items: Vec<BusAuditTrailItemView>,
    /// The next page token, if additional items remain.
    pub next_cursor: Option<PageToken>,
}

/// Returns one backend-health view.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackendHealthView {
    /// The backend identifier.
    pub backend_id: BackendId,
    /// The backend kind.
    pub backend_kind: BackendKind,
    /// The current capability status.
    pub capability_status: BackendCapabilityStatus,
    /// The timestamp of the last successful capability check.
    pub last_checked_at: Timestamp,
    /// The optional secret reference used by the backend.
    pub secret_ref: Option<String>,
}
