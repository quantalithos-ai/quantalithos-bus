//! Application-layer errors and stable protocol mappings.

use std::error::Error;
use std::fmt;

use bus_domain::errors::DomainError;

/// A stable reference to auditable error details.
pub type ErrorDetailsRef = String;

/// Stable protocol error categories.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtocolErrorCategory {
    /// The caller sent invalid request input.
    Validation,
    /// The requested resource does not exist.
    NotFound,
    /// The request conflicts with committed state or idempotency.
    Conflict,
    /// The request violates a protected boundary.
    BoundaryViolation,
    /// A dependency is temporarily unavailable.
    Dependency,
    /// An internal invariant or transaction failed.
    Internal,
}

/// A validation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationError {
    /// Stable external error code.
    pub code: &'static str,
    /// Human-readable message.
    pub message: String,
    /// Optional auditable details reference.
    pub details_ref: Option<ErrorDetailsRef>,
}

/// A not-found failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NotFoundError {
    /// Stable external error code.
    pub code: &'static str,
    /// Human-readable message.
    pub message: String,
    /// Optional auditable details reference.
    pub details_ref: Option<ErrorDetailsRef>,
}

/// A conflict failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConflictError {
    /// Stable external error code.
    pub code: &'static str,
    /// Human-readable message.
    pub message: String,
    /// Optional auditable details reference.
    pub details_ref: Option<ErrorDetailsRef>,
}

/// A boundary-violation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundaryViolationError {
    /// Stable external error code.
    pub code: &'static str,
    /// Human-readable message.
    pub message: String,
    /// Optional auditable details reference.
    pub details_ref: Option<ErrorDetailsRef>,
}

/// A dependency failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DependencyError {
    /// Stable external error code.
    pub code: &'static str,
    /// Human-readable message.
    pub message: String,
    /// Optional auditable details reference.
    pub details_ref: Option<ErrorDetailsRef>,
    /// Whether the caller may retry automatically.
    pub retryable: bool,
}

/// An internal failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InternalError {
    /// Stable external error code.
    pub code: &'static str,
    /// Human-readable message.
    pub message: String,
    /// Optional auditable details reference.
    pub details_ref: Option<ErrorDetailsRef>,
    /// Whether manual intervention is required.
    pub manual_action_required: bool,
}

/// Repository-port failures.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RepositoryError {
    /// The store is temporarily unavailable.
    Unavailable,
    /// An optimistic version mismatch occurred.
    VersionConflict,
    /// A uniqueness constraint was violated.
    UniqueViolation,
    /// An append-only sequence allocation failed.
    SequenceConflict,
    /// A committed row could not be reconstructed.
    CorruptedRecord,
}

/// Unit-of-work failures.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UnitOfWorkError {
    /// A write transaction could not be opened.
    BeginFailed,
    /// A transaction failed to commit and is known uncommitted.
    CommitFailed,
    /// A transaction commit outcome is uncertain.
    CommitUncertain,
    /// A transaction failed to roll back.
    RollbackFailed,
    /// The transaction handle is unknown or expired.
    InvalidHandle,
}

/// ID-generation failures.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdGenerationError {
    /// The adapter could not allocate an identifier.
    Exhausted,
}

/// Application failures returned by services.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApplicationError {
    /// Invalid request input.
    Validation(ValidationError),
    /// Requested resource is missing.
    NotFound(NotFoundError),
    /// Request conflicts with committed state.
    Conflict(ConflictError),
    /// Request crosses a protected boundary.
    BoundaryViolation(BoundaryViolationError),
    /// A dependency failed.
    Dependency(DependencyError),
    /// An internal invariant failed.
    Internal(InternalError),
}

impl ApplicationError {
    /// Returns the stable protocol category for the error.
    pub fn category(&self) -> ProtocolErrorCategory {
        match self {
            Self::Validation(_) => ProtocolErrorCategory::Validation,
            Self::NotFound(_) => ProtocolErrorCategory::NotFound,
            Self::Conflict(_) => ProtocolErrorCategory::Conflict,
            Self::BoundaryViolation(_) => ProtocolErrorCategory::BoundaryViolation,
            Self::Dependency(_) => ProtocolErrorCategory::Dependency,
            Self::Internal(_) => ProtocolErrorCategory::Internal,
        }
    }

    /// Returns the stable external error code.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Validation(error) => error.code,
            Self::NotFound(error) => error.code,
            Self::Conflict(error) => error.code,
            Self::BoundaryViolation(error) => error.code,
            Self::Dependency(error) => error.code,
            Self::Internal(error) => error.code,
        }
    }

    /// Returns the user-facing message.
    pub fn message(&self) -> &str {
        match self {
            Self::Validation(error) => &error.message,
            Self::NotFound(error) => &error.message,
            Self::Conflict(error) => &error.message,
            Self::BoundaryViolation(error) => &error.message,
            Self::Dependency(error) => &error.message,
            Self::Internal(error) => &error.message,
        }
    }

    /// Returns whether the caller may retry automatically.
    pub fn retryable(&self) -> bool {
        matches!(self, Self::Dependency(error) if error.retryable)
    }

    /// Returns whether the error requires manual intervention.
    pub fn requires_manual_action(&self) -> bool {
        matches!(self, Self::Internal(error) if error.manual_action_required)
    }

    /// Returns the auditable details reference.
    pub fn details_ref(&self) -> Option<&str> {
        match self {
            Self::Validation(error) => error.details_ref.as_deref(),
            Self::NotFound(error) => error.details_ref.as_deref(),
            Self::Conflict(error) => error.details_ref.as_deref(),
            Self::BoundaryViolation(error) => error.details_ref.as_deref(),
            Self::Dependency(error) => error.details_ref.as_deref(),
            Self::Internal(error) => error.details_ref.as_deref(),
        }
    }

    /// Creates a validation error.
    pub fn validation(code: &'static str, message: impl Into<String>) -> Self {
        Self::Validation(ValidationError {
            code,
            message: message.into(),
            details_ref: None,
        })
    }

    /// Creates a conflict error.
    pub fn conflict(
        code: &'static str,
        message: impl Into<String>,
        details_ref: Option<ErrorDetailsRef>,
    ) -> Self {
        Self::Conflict(ConflictError {
            code,
            message: message.into(),
            details_ref,
        })
    }

    /// Creates a boundary violation error.
    pub fn boundary_violation(
        code: &'static str,
        message: impl Into<String>,
        details_ref: Option<ErrorDetailsRef>,
    ) -> Self {
        Self::BoundaryViolation(BoundaryViolationError {
            code,
            message: message.into(),
            details_ref,
        })
    }

    fn dependency(
        code: &'static str,
        message: impl Into<String>,
        retryable: bool,
        details_ref: Option<ErrorDetailsRef>,
    ) -> Self {
        Self::Dependency(DependencyError {
            code,
            message: message.into(),
            details_ref,
            retryable,
        })
    }

    fn internal(
        code: &'static str,
        message: impl Into<String>,
        manual_action_required: bool,
        details_ref: Option<ErrorDetailsRef>,
    ) -> Self {
        Self::Internal(InternalError {
            code,
            message: message.into(),
            details_ref,
            manual_action_required,
        })
    }
}

impl fmt::Display for ApplicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message())
    }
}

impl Error for ApplicationError {}

impl From<DomainError> for ApplicationError {
    fn from(error: DomainError) -> Self {
        match error {
            DomainError::InvalidPublicationMaterial(field) => Self::validation(
                "validation.publication_material",
                format!("invalid publication material field: {field}"),
            ),
            DomainError::InvalidRequestDigest => Self::internal(
                "internal.request_digest",
                "request digest could not be computed",
                false,
                None,
            ),
            DomainError::PayloadBoundaryViolation => Self::boundary_violation(
                "boundary.payload_body_rejected",
                "payload body is not accepted by bus protocol",
                None,
            ),
            DomainError::InvalidStateTransition { .. }
            | DomainError::TerminalStateReopenRejected
            | DomainError::PublicationRejectedCannotScheduleDelivery => {
                Self::conflict("conflict.publication_state", error.to_string(), None)
            }
        }
    }
}

impl From<RepositoryError> for ApplicationError {
    fn from(error: RepositoryError) -> Self {
        match error {
            RepositoryError::Unavailable => Self::dependency(
                "dependency.repository_unavailable",
                "repository unavailable",
                true,
                None,
            ),
            RepositoryError::VersionConflict => {
                Self::conflict("conflict.version", "version conflict", None)
            }
            RepositoryError::UniqueViolation => {
                Self::conflict("conflict.unique", "unique constraint conflict", None)
            }
            RepositoryError::SequenceConflict => {
                Self::conflict("conflict.audit_sequence", "audit sequence conflict", None)
            }
            RepositoryError::CorruptedRecord => {
                Self::internal("internal.corrupted_record", "corrupted record", true, None)
            }
        }
    }
}

impl From<UnitOfWorkError> for ApplicationError {
    fn from(error: UnitOfWorkError) -> Self {
        match error {
            UnitOfWorkError::BeginFailed => Self::dependency(
                "dependency.transaction_unavailable",
                "write transaction unavailable",
                true,
                None,
            ),
            UnitOfWorkError::CommitFailed => {
                Self::dependency("dependency.commit_failed", "commit failed", true, None)
            }
            UnitOfWorkError::CommitUncertain => Self::internal(
                "internal.commit_uncertain",
                "commit outcome is uncertain",
                true,
                None,
            ),
            UnitOfWorkError::RollbackFailed => {
                Self::internal("internal.rollback_failed", "rollback failed", true, None)
            }
            UnitOfWorkError::InvalidHandle => Self::internal(
                "internal.invalid_transaction_handle",
                "invalid transaction handle",
                true,
                None,
            ),
        }
    }
}

impl From<IdGenerationError> for ApplicationError {
    fn from(_: IdGenerationError) -> Self {
        Self::internal(
            "internal.id_generation_failed",
            "record id generation failed",
            false,
            None,
        )
    }
}
