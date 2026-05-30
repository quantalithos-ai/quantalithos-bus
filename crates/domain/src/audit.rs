//! Audit records for publication write-path decisions.

use bus_contracts::metadata::{
    ActorContext, AuditRef, DeliveryId, FailureReason, FeedbackStatus, IdempotencyKey,
    PublicationId, Timestamp,
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
    /// An idempotency key scoped by operation.
    IdempotencyKey {
        /// The idempotency scope.
        scope: IdempotencyScope,
        /// The idempotency key.
        key: IdempotencyKey,
    },
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
    /// A backend signal was ignored because no committed delivery truth matched it.
    BackendSignalIgnored,
    /// A timeout signal was ignored because no committed delivery truth matched it.
    TimeoutSignalIgnored,
    /// A request reused an idempotency key with a different digest.
    IdempotencyConflict,
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
