//! Domain errors for the publication acceptance flow.

use std::error::Error;
use std::fmt;

use bus_contracts::metadata::{DeliveryStatus, PublicationAcceptanceStatus};

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
            Self::InvalidRequestDigest => formatter.write_str("invalid request digest"),
            Self::PayloadBoundaryViolation => formatter.write_str("payload boundary violation"),
            Self::InvalidStateTransition { from, to } => {
                write!(formatter, "invalid state transition: {from:?} -> {to:?}")
            }
            Self::InvalidDeliveryTransition { from, to } => {
                write!(formatter, "invalid delivery transition: {from:?} -> {to:?}")
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
