//! Domain errors for the publication acceptance flow.

use std::error::Error;
use std::fmt;

use bus_contracts::metadata::PublicationAcceptanceStatus;

/// A domain failure raised by publication objects and boundary policies.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DomainError {
    /// The publication material is missing a required reference or identifier.
    InvalidPublicationMaterial(&'static str),
    /// The publication material violates the payload boundary.
    PayloadBoundaryViolation,
    /// The requested status transition is not allowed.
    InvalidStateTransition {
        /// The current state.
        from: PublicationAcceptanceStatus,
        /// The requested target state.
        to: PublicationAcceptanceStatus,
    },
    /// A terminal acceptance state cannot be reopened.
    TerminalStateReopenRejected,
    /// A rejected publication cannot enter the delivery path.
    PublicationRejectedCannotScheduleDelivery,
}

impl fmt::Display for DomainError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPublicationMaterial(field) => {
                write!(formatter, "invalid publication material: {field}")
            }
            Self::PayloadBoundaryViolation => formatter.write_str("payload boundary violation"),
            Self::InvalidStateTransition { from, to } => {
                write!(formatter, "invalid state transition: {from:?} -> {to:?}")
            }
            Self::TerminalStateReopenRejected => {
                formatter.write_str("terminal acceptance state cannot be reopened")
            }
            Self::PublicationRejectedCannotScheduleDelivery => {
                formatter.write_str("rejected publication cannot schedule delivery")
            }
        }
    }
}

impl Error for DomainError {}
