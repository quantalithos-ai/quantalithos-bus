//! Command DTOs for the bus publication flow.

use serde::{Deserialize, Serialize};

use crate::metadata::{
    CoreEventRef, DeliveryAttemptId, DeliveryId, DeliveryMode, ExternalFeedbackRef, FeedbackKind,
    FeedbackReason, PayloadDigest, PayloadKind, PayloadRef, SourceRecordRef, SourceSystem,
    TargetScope, Timestamp,
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
