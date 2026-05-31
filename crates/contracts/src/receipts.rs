//! Result DTOs returned by bus write-path operations.

use serde::{Deserialize, Serialize};

use crate::metadata::{
    AttemptCount, AuditRef, DeadLetterId, DeadLetterStatus, DeliveryId, DeliveryStatus,
    FailureMaterialRef, FeedbackId, FeedbackRecordStatus, PublicationAcceptanceStatus,
    PublicationId, RejectionReasonRef, ReplayPreparationId, ReplayPreparationStatus, RetryPlanId,
    RetryPlanStatus, Timestamp,
};

/// The result returned after publication acceptance is decided.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationAcceptanceResult {
    /// The stable publication identifier.
    pub publication_id: PublicationId,
    /// The resulting acceptance state.
    pub acceptance_status: PublicationAcceptanceStatus,
    /// The optional stable rejection reason reference.
    pub rejection_reason_ref: Option<RejectionReasonRef>,
    /// The audit entry that records the terminal decision.
    pub audit_ref: AuditRef,
}

impl PublicationAcceptanceResult {
    /// Creates a result for an accepted publication.
    pub fn accepted(publication_id: PublicationId, audit_ref: AuditRef) -> Self {
        Self {
            publication_id,
            acceptance_status: PublicationAcceptanceStatus::Accepted,
            rejection_reason_ref: None,
            audit_ref,
        }
    }

    /// Creates a result for a rejected publication.
    pub fn rejected(
        publication_id: PublicationId,
        rejection_reason_ref: RejectionReasonRef,
        audit_ref: AuditRef,
    ) -> Self {
        Self {
            publication_id,
            acceptance_status: PublicationAcceptanceStatus::Rejected,
            rejection_reason_ref: Some(rejection_reason_ref),
            audit_ref,
        }
    }
}

/// The stable relay outcome returned by the outbox consumer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutboxRelayStatus {
    /// A new publication acceptance was committed from the source fact.
    Accepted,
    /// The source fact matched an existing committed result.
    Duplicate,
}

/// The result returned after consuming one committed outbox fact.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutboxRelayResult {
    /// The stable publication identifier.
    pub publication_id: PublicationId,
    /// The resulting relay status.
    pub relay_status: OutboxRelayStatus,
    /// The audit entry that records the committed decision.
    pub audit_ref: AuditRef,
}

impl OutboxRelayResult {
    /// Creates a result for a newly accepted outbox fact.
    pub fn accepted(publication_id: PublicationId, audit_ref: AuditRef) -> Self {
        Self {
            publication_id,
            relay_status: OutboxRelayStatus::Accepted,
            audit_ref,
        }
    }

    /// Creates a result for a duplicate outbox fact that matched committed truth.
    pub fn duplicate(publication_id: PublicationId, audit_ref: AuditRef) -> Self {
        Self {
            publication_id,
            relay_status: OutboxRelayStatus::Duplicate,
            audit_ref,
        }
    }
}

/// The result returned after one delivery feedback is committed.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackRecordResult {
    /// The committed feedback identifier.
    pub feedback_id: FeedbackId,
    /// The associated delivery identifier.
    pub delivery_id: DeliveryId,
    /// The stable feedback receipt status.
    pub feedback_status: FeedbackRecordStatus,
    /// The resulting committed delivery lifecycle state.
    pub delivery_status: DeliveryStatus,
    /// The audit entry that records the committed feedback.
    pub audit_ref: AuditRef,
}

impl FeedbackRecordResult {
    /// Creates a result for a committed feedback record.
    pub fn recorded(
        feedback_id: FeedbackId,
        delivery_id: DeliveryId,
        delivery_status: DeliveryStatus,
        audit_ref: AuditRef,
    ) -> Self {
        Self {
            feedback_id,
            delivery_id,
            feedback_status: FeedbackRecordStatus::Recorded,
            delivery_status,
            audit_ref,
        }
    }
}

/// The stable processing status returned by a backend signal consumer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendSignalStatus {
    /// A new backend signal was normalized and committed.
    Recorded,
    /// The backend signal matched existing committed truth.
    Duplicate,
    /// The backend signal referenced no committed delivery truth.
    Ignored,
}

/// The normalized feedback outcome derived from a backend signal.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendSignalNormalizedResult {
    /// The backend signal normalized to an acknowledgement.
    Ack,
    /// The backend signal normalized to a failure result.
    Fail,
}

/// The result returned after consuming one backend delivery signal.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackendSignalResult {
    /// The target delivery identifier referenced by the signal.
    pub delivery_id: DeliveryId,
    /// The target delivery attempt identifier referenced by the signal.
    pub attempt_id: crate::metadata::DeliveryAttemptId,
    /// The stable backend signal processing status.
    pub signal_status: BackendSignalStatus,
    /// The normalized feedback outcome, if one was committed or reused.
    pub normalized_result: Option<BackendSignalNormalizedResult>,
    /// The committed feedback identifier, if one was committed or reused.
    pub feedback_id: Option<FeedbackId>,
    /// The audit entry that records the committed or ignored outcome.
    pub audit_ref: AuditRef,
}

impl BackendSignalResult {
    /// Creates a result for a newly recorded backend signal.
    pub fn recorded(
        delivery_id: DeliveryId,
        attempt_id: crate::metadata::DeliveryAttemptId,
        normalized_result: BackendSignalNormalizedResult,
        feedback_id: FeedbackId,
        audit_ref: AuditRef,
    ) -> Self {
        Self {
            delivery_id,
            attempt_id,
            signal_status: BackendSignalStatus::Recorded,
            normalized_result: Some(normalized_result),
            feedback_id: Some(feedback_id),
            audit_ref,
        }
    }

    /// Creates a result for a duplicate backend signal.
    pub fn duplicate(
        delivery_id: DeliveryId,
        attempt_id: crate::metadata::DeliveryAttemptId,
        normalized_result: BackendSignalNormalizedResult,
        feedback_id: FeedbackId,
        audit_ref: AuditRef,
    ) -> Self {
        Self {
            delivery_id,
            attempt_id,
            signal_status: BackendSignalStatus::Duplicate,
            normalized_result: Some(normalized_result),
            feedback_id: Some(feedback_id),
            audit_ref,
        }
    }

    /// Creates a result for a backend signal that was ignored safely.
    pub fn ignored(
        delivery_id: DeliveryId,
        attempt_id: crate::metadata::DeliveryAttemptId,
        audit_ref: AuditRef,
    ) -> Self {
        Self {
            delivery_id,
            attempt_id,
            signal_status: BackendSignalStatus::Ignored,
            normalized_result: None,
            feedback_id: None,
            audit_ref,
        }
    }
}

/// The stable processing status returned by a timeout signal consumer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeoutRecordStatus {
    /// A new timeout feedback was committed.
    TimeoutRecorded,
    /// The timeout signal matched existing committed truth.
    Duplicate,
    /// The timeout signal referenced no committed delivery truth.
    Ignored,
}

/// The result returned after consuming one delivery timeout signal.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimeoutRecordResult {
    /// The target delivery identifier referenced by the timeout signal.
    pub delivery_id: DeliveryId,
    /// The committed timeout feedback identifier, if one was committed or reused.
    pub feedback_id: Option<FeedbackId>,
    /// The stable timeout processing status.
    pub feedback_status: TimeoutRecordStatus,
    /// Whether the failed delivery should be considered for later recovery evaluation.
    pub recovery_candidate: bool,
    /// The audit entry that records the committed or ignored outcome.
    pub audit_ref: AuditRef,
}

impl TimeoutRecordResult {
    /// Creates a result for a newly recorded timeout signal.
    pub fn recorded(
        delivery_id: DeliveryId,
        feedback_id: FeedbackId,
        recovery_candidate: bool,
        audit_ref: AuditRef,
    ) -> Self {
        Self {
            delivery_id,
            feedback_id: Some(feedback_id),
            feedback_status: TimeoutRecordStatus::TimeoutRecorded,
            recovery_candidate,
            audit_ref,
        }
    }

    /// Creates a result for a duplicate timeout signal.
    pub fn duplicate(
        delivery_id: DeliveryId,
        feedback_id: FeedbackId,
        recovery_candidate: bool,
        audit_ref: AuditRef,
    ) -> Self {
        Self {
            delivery_id,
            feedback_id: Some(feedback_id),
            feedback_status: TimeoutRecordStatus::Duplicate,
            recovery_candidate,
            audit_ref,
        }
    }

    /// Creates a result for an ignored timeout signal.
    pub fn ignored(delivery_id: DeliveryId, audit_ref: AuditRef) -> Self {
        Self {
            delivery_id,
            feedback_id: None,
            feedback_status: TimeoutRecordStatus::Ignored,
            recovery_candidate: false,
            audit_ref,
        }
    }
}

/// The result returned after one retry plan is scheduled.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetryPlanResult {
    /// The committed retry-plan identifier.
    pub retry_plan_id: RetryPlanId,
    /// The associated delivery identifier.
    pub delivery_id: DeliveryId,
    /// The committed retry-plan status.
    pub retry_status: RetryPlanStatus,
    /// The remaining attempt budget attached to the scheduled retry plan.
    pub remaining_attempts: AttemptCount,
    /// The next execution time calculated for the retry plan.
    pub next_run_at: Timestamp,
    /// The audit entry that records the retry request.
    pub audit_ref: AuditRef,
}

impl RetryPlanResult {
    /// Creates a result for a scheduled retry plan.
    pub fn scheduled(
        retry_plan_id: RetryPlanId,
        delivery_id: DeliveryId,
        remaining_attempts: AttemptCount,
        next_run_at: Timestamp,
        audit_ref: AuditRef,
    ) -> Self {
        Self {
            retry_plan_id,
            delivery_id,
            retry_status: RetryPlanStatus::Scheduled,
            remaining_attempts,
            next_run_at,
            audit_ref,
        }
    }
}

/// The result returned after one delivery enters the dead-letter path.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeadLetterResult {
    /// The committed dead-letter identifier.
    pub dead_letter_id: DeadLetterId,
    /// The associated delivery identifier.
    pub delivery_id: DeliveryId,
    /// The committed dead-letter status.
    pub dead_letter_status: DeadLetterStatus,
    /// The linked failure-material reference.
    pub failure_material_ref: FailureMaterialRef,
    /// The audit entry that records dead-letter creation.
    pub audit_ref: AuditRef,
}

impl DeadLetterResult {
    /// Creates a result for a committed dead-letter entry.
    pub fn opened(
        dead_letter_id: DeadLetterId,
        delivery_id: DeliveryId,
        failure_material_ref: FailureMaterialRef,
        audit_ref: AuditRef,
    ) -> Self {
        Self {
            dead_letter_id,
            delivery_id,
            dead_letter_status: DeadLetterStatus::Open,
            failure_material_ref,
            audit_ref,
        }
    }
}

/// The result returned after one replay preparation becomes ready.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplayPreparationResult {
    /// The committed replay-preparation identifier.
    pub replay_preparation_id: ReplayPreparationId,
    /// The associated dead-letter identifier.
    pub dead_letter_id: DeadLetterId,
    /// The committed replay-preparation status.
    pub replay_preparation_status: ReplayPreparationStatus,
    /// The audit entry that records the ready transition.
    pub audit_ref: AuditRef,
}

impl ReplayPreparationResult {
    /// Creates a result for a ready replay preparation.
    pub fn ready(
        replay_preparation_id: ReplayPreparationId,
        dead_letter_id: DeadLetterId,
        audit_ref: AuditRef,
    ) -> Self {
        Self {
            replay_preparation_id,
            dead_letter_id,
            replay_preparation_status: ReplayPreparationStatus::Ready,
            audit_ref,
        }
    }
}
