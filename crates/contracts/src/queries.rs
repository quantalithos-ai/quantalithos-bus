//! Query DTOs for bus read-only outputs.

use serde::{Deserialize, Serialize};

use crate::metadata::{
    AuditEventKind, AuthorizationRef, BackendId, DeliveryId, FailureSummaryId, PageRequest,
    PublicationId, TransportViewId,
};

/// Queries one publication-acceptance result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetPublicationAcceptanceQuery {
    /// The target publication identifier.
    pub publication_id: PublicationId,
}

/// Queries the current state of one delivery.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetDeliveryStatusQuery {
    /// The target delivery identifier.
    pub delivery_id: DeliveryId,
}

/// Queries the append-only history of one delivery.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListDeliveryHistoryQuery {
    /// The target delivery identifier.
    pub delivery_id: DeliveryId,
    /// The requested page boundary.
    pub page: PageRequest,
}

/// Queries one transport-view projection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetTransportViewQuery {
    /// The target transport-view identifier.
    pub transport_view_id: TransportViewId,
}

/// Queries one failure-summary projection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetFailureSummaryQuery {
    /// The target failure-summary identifier.
    pub failure_summary_id: FailureSummaryId,
    /// The trusted authorization reference for this privileged read.
    pub authorization_ref: Option<AuthorizationRef>,
}

/// Filters one audit-trail query.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditFilter {
    /// The optional audited record reference.
    pub record_ref: Option<String>,
    /// The optional event kind filter.
    pub event_kind: Option<AuditEventKind>,
}

/// Queries the append-only bus audit trail.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetBusAuditTrailQuery {
    /// The requested audit filter.
    pub filter: AuditFilter,
    /// The requested page boundary.
    pub page: PageRequest,
    /// The trusted authorization reference for this privileged read.
    pub authorization_ref: Option<AuthorizationRef>,
}

/// Queries the current backend-health view.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetBackendHealthViewQuery {
    /// The target backend identifier.
    pub backend_id: BackendId,
}
