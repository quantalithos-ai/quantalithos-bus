//! Result DTOs returned by bus publication commands.

use serde::{Deserialize, Serialize};

use crate::metadata::{AuditRef, PublicationAcceptanceStatus, PublicationId, RejectionReasonRef};

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
