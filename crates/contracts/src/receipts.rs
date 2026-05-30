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
