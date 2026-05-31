//! Domain errors for bus publication, delivery, feedback, and recovery flows.

use std::error::Error;
use std::fmt;

use bus_contracts::metadata::{
    DeadLetterStatus, DeliveryStatus, PublicationAcceptanceStatus, ReplayPreparationStatus,
    RetryPlanStatus,
};

/// A domain failure raised by publication objects and boundary policies.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DomainError {
    /// The publication material is missing a required reference or identifier.
    InvalidPublicationMaterial(&'static str),
    /// The subscriber scope is missing a required field.
    InvalidSubscriberScope(&'static str),
    /// The delivery record is missing a required reference or identifier.
    InvalidDeliveryRecord(&'static str),
    /// The delivery attempt is missing a required reference or timestamp.
    InvalidDeliveryAttempt(&'static str),
    /// The feedback result is missing a required reference or timestamp.
    InvalidFeedbackResult(&'static str),
    /// The retry plan is missing a required reference, count, or timestamp.
    InvalidRetryPlan(&'static str),
    /// The dead-letter entry is missing a required reference or chain.
    InvalidDeadLetterEntry(&'static str),
    /// The replay preparation is missing a required reference or chain.
    InvalidReplayPreparation(&'static str),
    /// The failure material is missing a required reference or audit link.
    InvalidFailureMaterial(&'static str),
    /// The transport-view projection is missing a required reference.
    InvalidTransportViewProjection(&'static str),
    /// The failure-summary projection is missing a required reference.
    InvalidFailureSummaryProjection(&'static str),
    /// The recovery policy configuration is invalid.
    InvalidRecoveryPolicy(&'static str),
    /// The request digest could not be computed.
    InvalidRequestDigest,
    /// The publication material violates the payload boundary.
    PayloadBoundaryViolation,
    /// The requested status transition is not allowed.
    InvalidStateTransition {
        /// The current state.
        from: PublicationAcceptanceStatus,
        /// The requested target state.
        to: PublicationAcceptanceStatus,
    },
    /// The requested delivery transition is not allowed.
    InvalidDeliveryTransition {
        /// The current delivery state.
        from: DeliveryStatus,
        /// The requested target delivery state.
        to: DeliveryStatus,
    },
    /// The requested retry-plan transition is not allowed.
    InvalidRetryPlanTransition {
        /// The current retry-plan state.
        from: RetryPlanStatus,
        /// The requested target retry-plan state.
        to: RetryPlanStatus,
    },
    /// The requested dead-letter transition is not allowed.
    InvalidDeadLetterTransition {
        /// The current dead-letter state.
        from: DeadLetterStatus,
        /// The requested target dead-letter state.
        to: DeadLetterStatus,
    },
    /// The requested replay-preparation transition is not allowed.
    InvalidReplayPreparationTransition {
        /// The current replay-preparation state.
        from: ReplayPreparationStatus,
        /// The requested target replay-preparation state.
        to: ReplayPreparationStatus,
    },
    /// The requested read-projection transition is not allowed.
    InvalidProjectionStatusTransition {
        /// The current projection state.
        from: crate::read_output::ProjectionStatus,
        /// The requested target projection state.
        to: crate::read_output::ProjectionStatus,
    },
    /// A terminal acceptance state cannot be reopened.
    TerminalStateReopenRejected,
    /// A terminal delivery state cannot be reopened.
    TerminalDeliveryStateReopenRejected,
    /// A rejected publication cannot enter the delivery path.
    PublicationRejectedCannotScheduleDelivery,
    /// The provided target scope does not match the accepted publication material.
    TargetScopeMismatch,
    /// The transport semantic does not require a durable delivery record.
    NonDurableTransportSemantic,
    /// The selected backend capability does not map the platform semantic.
    BackendCapabilityMappingRejected,
    /// Backend-private data leaked into a platform semantic reference.
    BackendPrivateFieldLeak,
    /// The target delivery is not eligible for controlled retry.
    RetryNotAllowed,
    /// The target delivery is not eligible for dead-letter handling.
    DeadLetterNotAllowed,
    /// The target dead-letter entry is not eligible for replay preparation.
    ReplayPreparationNotAllowed,
    /// An exhausted retry plan must not dispatch again.
    RetryExhaustedCannotDispatch,
    /// A closed dead-letter entry must not prepare replay again.
    DeadLetterClosed,
    /// Dead-letter entries must not dispatch delivery directly.
    DeadLetterCannotDispatchDirectly,
    /// Replay preparation is not the replay executor boundary.
    ReplayPreparationIsNotExecutor,
    /// Read-only projection logic attempted to mutate bus truth.
    ReadOnlyProjectionViolation,
    /// The provided delivery attempt has not been finished.
    AttemptNotFinished,
    /// The provided delivery attempt was already finished.
    AttemptAlreadyFinished,
    /// The provided delivery attempt belongs to a different delivery.
    AttemptDoesNotBelongToDelivery,
    /// The provided attempt outcome does not justify a delivered transition.
    AttemptOutcomeDoesNotDeliver,
    /// The provided attempt does not match the last recorded attempt reference.
    AttemptRefMismatch,
    /// The provided attempt finish timestamp predates the start timestamp.
    AttemptFinishedBeforeStart,
}

impl fmt::Display for DomainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPublicationMaterial(field) => {
                write!(formatter, "invalid publication material: {field}")
            }
            Self::InvalidSubscriberScope(field) => {
                write!(formatter, "invalid subscriber scope: {field}")
            }
            Self::InvalidDeliveryRecord(field) => {
                write!(formatter, "invalid delivery record: {field}")
            }
            Self::InvalidDeliveryAttempt(field) => {
                write!(formatter, "invalid delivery attempt: {field}")
            }
            Self::InvalidFeedbackResult(field) => {
                write!(formatter, "invalid feedback result: {field}")
            }
            Self::InvalidRetryPlan(field) => write!(formatter, "invalid retry plan: {field}"),
            Self::InvalidDeadLetterEntry(field) => {
                write!(formatter, "invalid dead letter entry: {field}")
            }
            Self::InvalidReplayPreparation(field) => {
                write!(formatter, "invalid replay preparation: {field}")
            }
            Self::InvalidFailureMaterial(field) => {
                write!(formatter, "invalid failure material: {field}")
            }
            Self::InvalidTransportViewProjection(field) => {
                write!(formatter, "invalid transport view projection: {field}")
            }
            Self::InvalidFailureSummaryProjection(field) => {
                write!(formatter, "invalid failure summary projection: {field}")
            }
            Self::InvalidRecoveryPolicy(field) => {
                write!(formatter, "invalid recovery policy: {field}")
            }
            Self::InvalidRequestDigest => formatter.write_str("invalid request digest"),
            Self::PayloadBoundaryViolation => formatter.write_str("payload boundary violation"),
            Self::InvalidStateTransition { from, to } => {
                write!(formatter, "invalid state transition: {from:?} -> {to:?}")
            }
            Self::InvalidDeliveryTransition { from, to } => {
                write!(formatter, "invalid delivery transition: {from:?} -> {to:?}")
            }
            Self::InvalidRetryPlanTransition { from, to } => {
                write!(
                    formatter,
                    "invalid retry plan transition: {from:?} -> {to:?}"
                )
            }
            Self::InvalidDeadLetterTransition { from, to } => {
                write!(
                    formatter,
                    "invalid dead letter transition: {from:?} -> {to:?}"
                )
            }
            Self::InvalidReplayPreparationTransition { from, to } => {
                write!(
                    formatter,
                    "invalid replay preparation transition: {from:?} -> {to:?}"
                )
            }
            Self::InvalidProjectionStatusTransition { from, to } => {
                write!(
                    formatter,
                    "invalid projection status transition: {from:?} -> {to:?}"
                )
            }
            Self::TerminalStateReopenRejected => {
                formatter.write_str("terminal acceptance state cannot be reopened")
            }
            Self::TerminalDeliveryStateReopenRejected => {
                formatter.write_str("terminal delivery state cannot be reopened")
            }
            Self::PublicationRejectedCannotScheduleDelivery => {
                formatter.write_str("rejected publication cannot schedule delivery")
            }
            Self::TargetScopeMismatch => {
                formatter.write_str("target scope does not match accepted publication material")
            }
            Self::NonDurableTransportSemantic => {
                formatter.write_str("transport semantic does not require a durable record")
            }
            Self::BackendCapabilityMappingRejected => {
                formatter.write_str("backend capability mapping rejected")
            }
            Self::BackendPrivateFieldLeak => {
                formatter.write_str("backend private field leaked into platform semantic")
            }
            Self::RetryNotAllowed => formatter.write_str("retry is not allowed"),
            Self::DeadLetterNotAllowed => formatter.write_str("dead letter is not allowed"),
            Self::ReplayPreparationNotAllowed => {
                formatter.write_str("replay preparation is not allowed")
            }
            Self::RetryExhaustedCannotDispatch => {
                formatter.write_str("exhausted retry plan cannot dispatch")
            }
            Self::DeadLetterClosed => formatter.write_str("dead letter is already closed"),
            Self::DeadLetterCannotDispatchDirectly => {
                formatter.write_str("dead letter cannot dispatch directly")
            }
            Self::ReplayPreparationIsNotExecutor => {
                formatter.write_str("replay preparation is not the replay executor")
            }
            Self::ReadOnlyProjectionViolation => {
                formatter.write_str("read-only projection attempted to mutate truth")
            }
            Self::AttemptNotFinished => formatter.write_str("delivery attempt is not finished"),
            Self::AttemptAlreadyFinished => {
                formatter.write_str("delivery attempt was already finished")
            }
            Self::AttemptDoesNotBelongToDelivery => {
                formatter.write_str("delivery attempt belongs to a different delivery")
            }
            Self::AttemptOutcomeDoesNotDeliver => {
                formatter.write_str("delivery attempt outcome does not justify delivery")
            }
            Self::AttemptRefMismatch => {
                formatter.write_str("delivery attempt does not match the recorded attempt")
            }
            Self::AttemptFinishedBeforeStart => {
                formatter.write_str("delivery attempt finished before it started")
            }
        }
    }
}

impl Error for DomainError {}
