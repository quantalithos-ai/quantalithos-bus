//! Command DTOs for bus write-path operations.

use serde::{Deserialize, Serialize};

use crate::metadata::{
    AttemptLimit, AuditChainRef, CoreEventRef, DeadLetterId, DeadLetterReason, DeliveryAttemptId,
    DeliveryId, DeliveryMode, ExternalFeedbackRef, FailureMaterialRef, FeedbackKind,
    FeedbackReason, OperatorNoteRef, PayloadDigest, PayloadKind, PayloadRef, ReplayApprovalRef,
    ReplayReason, RetryPolicyRef, RetryRequestReason, SourceRecordRef, SourceSystem, TargetScope,
    Timestamp,
};

/// Accepts publication material references into the bus.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptPublicationCommand {
    /// The upstream system that produced the source record.
    pub source_system: SourceSystem,
    /// The stable source record reference from the upstream system.
    pub source_record_ref: SourceRecordRef,
    /// The referenced L0-core event contract.
    pub core_event_ref: CoreEventRef,
    /// The external payload reference. The payload body must never be inlined here.
    pub payload_ref: PayloadRef,
    /// The declared kind of payload reference.
    pub payload_kind: PayloadKind,
    /// The digest for the referenced payload body.
    pub payload_digest: PayloadDigest,
    /// The requested platform delivery mode.
    pub delivery_mode: DeliveryMode,
    /// The logical target scope requested by the caller.
    pub target_scope: TargetScope,
}

/// Records normalized feedback for one committed delivery attempt.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordDeliveryFeedbackCommand {
    /// The target delivery identifier.
    pub delivery_id: DeliveryId,
    /// The delivered attempt that produced the feedback.
    pub attempt_id: DeliveryAttemptId,
    /// The normalized feedback kind.
    pub feedback_kind: FeedbackKind,
    /// The stable feedback reason supplied by the caller.
    pub feedback_reason: FeedbackReason,
    /// The externally observed timestamp for the feedback.
    pub observed_at: Timestamp,
    /// The stable upstream feedback reference.
    pub external_feedback_ref: ExternalFeedbackRef,
}

/// Requests one controlled retry plan for a failed delivery.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestRetryCommand {
    /// The target failed delivery identifier.
    pub delivery_id: DeliveryId,
    /// The existing failure material reference that justifies retry.
    pub failure_material_ref: FailureMaterialRef,
    /// The retry-policy reference supplied by the caller.
    pub retry_policy_ref: RetryPolicyRef,
    /// The stable retry-request reason.
    pub requested_reason: RetryRequestReason,
    /// The maximum number of attempts allowed by this retry request.
    pub max_attempts: AttemptLimit,
}

/// Moves one failed delivery into the dead-letter path.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MoveDeliveryToDeadLetterCommand {
    /// The target failed delivery identifier.
    pub delivery_id: DeliveryId,
    /// The existing failure material reference to link into the dead-letter entry.
    pub failure_material_ref: FailureMaterialRef,
    /// The stable dead-letter reason supplied by the caller.
    pub dead_letter_reason: DeadLetterReason,
    /// The optional operator note reference.
    pub operator_note_ref: Option<OperatorNoteRef>,
}

/// Prepares replay material from an approved dead-letter entry.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrepareReplayCommand {
    /// The target dead-letter identifier.
    pub dead_letter_id: DeadLetterId,
    /// The trusted audit-chain reference that backs the replay preparation.
    pub audit_chain_ref: AuditChainRef,
    /// The external replay-approval reference.
    pub approval_ref: ReplayApprovalRef,
    /// The stable replay reason supplied by the caller.
    pub replay_reason: ReplayReason,
}
