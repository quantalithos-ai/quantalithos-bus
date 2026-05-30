//! Idempotency and request-digest domain objects.

use bus_contracts::commands::AcceptPublicationCommand;
use bus_contracts::metadata::{IdempotencyKey, PublicationId, Timestamp, TraceContextRef};

use crate::errors::DomainError;

/// Distinguishes which inbound boundary owns an idempotency scope.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum IdempotencyEntryKind {
    /// A synchronous command boundary.
    Command,
}

/// Declares which business action is protected by idempotency.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum IdempotencyAction {
    /// The publication acceptance command.
    AcceptPublication,
}

/// A stable idempotency scope.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct IdempotencyScope {
    /// The inbound boundary kind.
    pub entry_kind: IdempotencyEntryKind,
    /// The protected business action.
    pub action: IdempotencyAction,
    /// The optional logical boundary reference.
    pub boundary_ref: Option<String>,
}

impl IdempotencyScope {
    /// Builds the command scope for `AcceptPublication`.
    pub fn for_accept_publication_command(command: &AcceptPublicationCommand) -> Self {
        Self {
            entry_kind: IdempotencyEntryKind::Command,
            action: IdempotencyAction::AcceptPublication,
            boundary_ref: Some(format!(
                "{}::{}",
                command.target_scope.project_id, command.target_scope.topic
            )),
        }
    }
}

/// The algorithm version used to derive a request digest.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DigestAlgorithmVersion {
    /// Stable FNV-1a 64-bit hashing over the canonical JSON payload.
    Fnv1a64V1,
}

/// A stable digest used to compare repeated requests.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestDigest {
    /// The canonical digest value.
    pub value: String,
    /// The digest algorithm version.
    pub algorithm_version: DigestAlgorithmVersion,
}

impl RequestDigest {
    /// Computes the request digest for `AcceptPublication`.
    pub fn from_accept_publication_command(
        command: &AcceptPublicationCommand,
    ) -> Result<Self, DomainError> {
        let payload = serde_json::to_vec(command).map_err(|_| DomainError::InvalidRequestDigest)?;
        let mut hash = 0xcbf29ce484222325_u64;
        for byte in payload {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }

        Ok(Self {
            value: format!("{hash:016x}"),
            algorithm_version: DigestAlgorithmVersion::Fnv1a64V1,
        })
    }
}

/// A stable local record reference bound to an idempotency anchor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecordRef {
    /// A publication acceptance result reference.
    Publication(PublicationId),
}

/// A committed idempotency anchor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdempotencyAnchor {
    /// The stable anchor identifier.
    pub anchor_id: String,
    /// The protected scope.
    pub scope: IdempotencyScope,
    /// The protected key.
    pub key: IdempotencyKey,
    /// The digest of the committed request.
    pub request_digest: RequestDigest,
    /// The committed result reference.
    pub bound_record_ref: RecordRef,
    /// The anchor creation timestamp.
    pub created_at: Timestamp,
    /// The trace reference attached to the anchor.
    pub trace_ref: TraceContextRef,
}

impl IdempotencyAnchor {
    /// Binds an idempotency key to a committed publication result.
    pub fn bind(
        anchor_id: String,
        scope: IdempotencyScope,
        key: IdempotencyKey,
        request_digest: RequestDigest,
        record_ref: RecordRef,
        created_at: Timestamp,
        trace_ref: TraceContextRef,
    ) -> Self {
        Self {
            anchor_id,
            scope,
            key,
            request_digest,
            bound_record_ref: record_ref,
            created_at,
            trace_ref,
        }
    }

    /// Returns whether the stored digest matches the incoming request.
    pub fn matches(&self, digest: &RequestDigest) -> bool {
        &self.request_digest == digest
    }

    /// Returns whether the anchor is bound to the provided record.
    pub fn is_bound_to(&self, record_ref: &RecordRef) -> bool {
        &self.bound_record_ref == record_ref
    }
}

/// A persisted idempotency conflict summary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdempotencyConflict {
    /// The protected scope.
    pub scope: IdempotencyScope,
    /// The reused idempotency key.
    pub key: IdempotencyKey,
    /// The previously committed digest.
    pub existing_digest: RequestDigest,
    /// The incoming conflicting digest.
    pub incoming_digest: RequestDigest,
    /// The conflict timestamp.
    pub occurred_at: Timestamp,
    /// The trace reference attached to the conflicting request.
    pub trace_ref: TraceContextRef,
}
