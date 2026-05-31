//! Audit records for publication write-path decisions.

use bus_contracts::metadata::{
    ActorContext, AuditChainRef, AuditRef, DeadLetterId, DeliveryId, FailureReason, FeedbackStatus,
    IdempotencyKey, PublicationId, ReplayPreparationId, RetryPlanId, Timestamp,
};

use crate::idempotency::IdempotencyScope;
use crate::publication::PublicationRejectReason;

/// A stable audit subject reference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SubjectRef {
    /// A publication acceptance subject.
    Publication(PublicationId),
    /// A delivery progression subject.
    Delivery(DeliveryId),
    /// A read-only output or privileged access subject.
    ReadOutput(String),
    /// A retry-plan subject.
    RetryPlan(RetryPlanId),
    /// A dead-letter subject.
    DeadLetter(DeadLetterId),
    /// A replay-preparation subject.
    ReplayPreparation(ReplayPreparationId),
    /// An idempotency key scoped by operation.
    IdempotencyKey {
        /// The idempotency scope.
        scope: IdempotencyScope,
        /// The idempotency key.
        key: IdempotencyKey,
    },
}

impl SubjectRef {
    /// Returns the stable record reference used by read-only audit views.
    pub fn record_ref(&self) -> String {
        match self {
            Self::Publication(publication_id) => publication_id.as_str().to_owned(),
            Self::Delivery(delivery_id) => delivery_id.as_str().to_owned(),
            Self::ReadOutput(record_ref) => record_ref.clone(),
            Self::RetryPlan(retry_plan_id) => retry_plan_id.as_str().to_owned(),
            Self::DeadLetter(dead_letter_id) => dead_letter_id.as_str().to_owned(),
            Self::ReplayPreparation(replay_preparation_id) => {
                replay_preparation_id.as_str().to_owned()
            }
            Self::IdempotencyKey { key, .. } => key.as_str().to_owned(),
        }
    }
}

/// The privileged scope recorded by one access audit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrivilegedAccessScope {
    /// One failure-summary projection read.
    FailureSummary,
    /// One bus-audit-trail read.
    BusAuditTrail,
    /// One replay-preparation operation.
    ReplayPreparation,
}

/// The stable rejection reason for a privileged access attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrivilegedAccessRejectionReason {
    /// The trusted authorization reference was missing.
    MissingAuthorizationRef,
    /// The trusted actor context did not carry any role hint.
    MissingRoleHint,
}

/// The decision recorded for a privileged access attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrivilegedAccessDecision {
    /// The privileged access attempt was granted.
    Granted,
    /// The privileged access attempt was rejected.
    Rejected(PrivilegedAccessRejectionReason),
}

/// A stable audit action emitted by the publication write path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuditAction {
    /// A publication was accepted.
    PublicationAccepted,
    /// A publication was rejected.
    PublicationRejected(PublicationRejectReason),
    /// Delivery entered the dispatching state.
    DeliveryDispatchStarted,
    /// Delivery reached `Delivered`.
    DeliveryDelivered,
    /// Delivery reached `Failed`.
    DeliveryFailed(FailureReason),
    /// Feedback was recorded for a delivery.
    FeedbackRecorded(FeedbackStatus),
    /// A retry plan was scheduled.
    RetryRequested,
    /// A retry attempt was executed.
    RetryAttempted,
    /// A retry plan was exhausted.
    RetryExhausted,
    /// A dead-letter entry was created.
    DeadLetterCreated,
    /// A replay preparation became ready.
    ReplayPreparationReady,
    /// A privileged read or operation was granted or rejected.
    PrivilegedAccess {
        /// The scope protected by the access seam.
        scope: PrivilegedAccessScope,
        /// The access decision that was taken.
        decision: PrivilegedAccessDecision,
    },
    /// A backend signal was ignored because no committed delivery truth matched it.
    BackendSignalIgnored,
    /// A timeout signal was ignored because no committed delivery truth matched it.
    TimeoutSignalIgnored,
    /// A request reused an idempotency key with a different digest.
    IdempotencyConflict,
}

impl AuditAction {
    /// Returns the stable event kind used by read-only audit views.
    pub fn event_kind(&self) -> bus_contracts::metadata::AuditEventKind {
        let value = match self {
            Self::PublicationAccepted => "publication_accepted",
            Self::PublicationRejected(_) => "publication_rejected",
            Self::DeliveryDispatchStarted => "delivery_dispatch_started",
            Self::DeliveryDelivered => "delivery_delivered",
            Self::DeliveryFailed(_) => "delivery_failed",
            Self::FeedbackRecorded(_) => "feedback_recorded",
            Self::RetryRequested => "retry_requested",
            Self::RetryAttempted => "retry_attempted",
            Self::RetryExhausted => "retry_exhausted",
            Self::DeadLetterCreated => "dead_letter_created",
            Self::ReplayPreparationReady => "replay_preparation_ready",
            Self::PrivilegedAccess { scope, decision } => match (scope, decision) {
                (PrivilegedAccessScope::FailureSummary, PrivilegedAccessDecision::Granted) => {
                    "failure_summary_access_granted"
                }
                (PrivilegedAccessScope::FailureSummary, PrivilegedAccessDecision::Rejected(_)) => {
                    "failure_summary_access_rejected"
                }
                (PrivilegedAccessScope::BusAuditTrail, PrivilegedAccessDecision::Granted) => {
                    "bus_audit_trail_access_granted"
                }
                (PrivilegedAccessScope::BusAuditTrail, PrivilegedAccessDecision::Rejected(_)) => {
                    "bus_audit_trail_access_rejected"
                }
                (PrivilegedAccessScope::ReplayPreparation, PrivilegedAccessDecision::Granted) => {
                    "replay_preparation_access_granted"
                }
                (
                    PrivilegedAccessScope::ReplayPreparation,
                    PrivilegedAccessDecision::Rejected(_),
                ) => "replay_preparation_access_rejected",
            },
            Self::BackendSignalIgnored => "backend_signal_ignored",
            Self::TimeoutSignalIgnored => "timeout_signal_ignored",
            Self::IdempotencyConflict => "idempotency_conflict",
        };

        bus_contracts::metadata::AuditEventKind::new(value)
    }
}

/// One auditable recovery chain loaded by reference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuditChain {
    /// The stable chain reference.
    pub chain_ref: AuditChainRef,
    /// The committed audit entries that belong to the chain.
    pub entries: Vec<BusAuditEntry>,
}

impl AuditChain {
    /// Returns whether the chain contains at least one committed entry.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// An append-only bus audit entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BusAuditEntry {
    /// The stable audit reference.
    pub audit_ref: AuditRef,
    /// The audited subject reference.
    pub subject_ref: SubjectRef,
    /// The recorded action.
    pub action: AuditAction,
    /// The actor that triggered the action.
    pub actor: ActorContext,
    /// The timestamp when the action occurred.
    pub occurred_at: Timestamp,
    /// The distributed trace reference.
    pub trace_ref: bus_contracts::metadata::TraceContextRef,
}

impl BusAuditEntry {
    /// Records an append-only audit entry.
    pub fn record(
        audit_ref: AuditRef,
        subject_ref: SubjectRef,
        action: AuditAction,
        actor: ActorContext,
        trace_ref: bus_contracts::metadata::TraceContextRef,
        occurred_at: Timestamp,
    ) -> Self {
        Self {
            audit_ref,
            subject_ref,
            action,
            actor,
            occurred_at,
            trace_ref,
        }
    }

    /// Returns whether the entry belongs to the provided subject.
    pub fn is_for_subject(&self, subject_ref: &SubjectRef) -> bool {
        &self.subject_ref == subject_ref
    }
}
