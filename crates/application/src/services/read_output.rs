//! Read-only query service for committed bus outputs.

use bus_contracts::metadata::{
    ActorContext, BackendCapabilityStatus, ConsistencyMarker, FailureKind, PageRequest, PageToken,
};
use bus_contracts::queries::{
    GetBackendHealthViewQuery, GetBusAuditTrailQuery, GetDeliveryStatusQuery,
    GetFailureSummaryQuery, GetPublicationAcceptanceQuery, GetTransportViewQuery,
    ListDeliveryHistoryQuery,
};
use bus_contracts::views::{
    BackendHealthView, BusAuditTrailView, DeliveryHistoryItemView, DeliveryHistoryPage,
    DeliveryStatusView, FailureSummaryView, PublicationAcceptanceView, TransportView,
};
use bus_domain::read_output::{ProjectionStatus, ReadOnlyOutputPolicy};

use crate::errors::ApplicationError;
use crate::ports::{
    AuditTrailRepository, DeliveryRepository, FeedbackRepository, PublicationRepository,
    ReadProjectionRepository, RecoveryRepository,
};

/// Query use-case contract for read-only bus outputs.
pub trait ReadOutputUseCase: Send + Sync {
    /// Loads one publication-acceptance view.
    async fn get_publication_acceptance(
        &self,
        query: GetPublicationAcceptanceQuery,
        actor: ActorContext,
    ) -> Result<PublicationAcceptanceView, ApplicationError>;

    /// Loads one delivery-status view.
    async fn get_delivery_status(
        &self,
        query: GetDeliveryStatusQuery,
        actor: ActorContext,
    ) -> Result<DeliveryStatusView, ApplicationError>;

    /// Loads one page of delivery-history items.
    async fn list_delivery_history(
        &self,
        query: ListDeliveryHistoryQuery,
        actor: ActorContext,
    ) -> Result<DeliveryHistoryPage, ApplicationError>;

    /// Loads one transport-view projection.
    async fn get_transport_view(
        &self,
        query: GetTransportViewQuery,
        actor: ActorContext,
    ) -> Result<TransportView, ApplicationError>;

    /// Loads one failure-summary projection.
    async fn get_failure_summary(
        &self,
        query: GetFailureSummaryQuery,
        actor: ActorContext,
    ) -> Result<FailureSummaryView, ApplicationError>;

    /// Loads one page of audit-trail items.
    async fn get_bus_audit_trail(
        &self,
        query: GetBusAuditTrailQuery,
        actor: ActorContext,
    ) -> Result<BusAuditTrailView, ApplicationError>;

    /// Loads one backend-health view.
    async fn get_backend_health_view(
        &self,
        query: GetBackendHealthViewQuery,
        actor: ActorContext,
    ) -> Result<BackendHealthView, ApplicationError>;
}

/// Dependencies for the read-only query service.
pub struct ReadOutputServiceDeps<P, D, F, A, R, V> {
    /// Publication truth repository.
    pub publication_repository: P,
    /// Delivery truth repository.
    pub delivery_repository: D,
    /// Feedback truth repository.
    pub feedback_repository: F,
    /// Audit repository.
    pub audit_repository: A,
    /// Read-projection repository.
    pub projection_repository: R,
    /// Recovery repository.
    pub recovery_repository: V,
}

/// The read-only query service.
pub struct ReadOutputService<P, D, F, A, R, V> {
    deps: ReadOutputServiceDeps<P, D, F, A, R, V>,
    #[allow(dead_code)]
    policy: ReadOnlyOutputPolicy,
}

impl<P, D, F, A, R, V> ReadOutputService<P, D, F, A, R, V> {
    /// Creates a new read-only query service.
    pub fn new(deps: ReadOutputServiceDeps<P, D, F, A, R, V>) -> Self {
        Self {
            deps,
            policy: ReadOnlyOutputPolicy::default_for_projection(),
        }
    }
}

impl<P, D, F, A, R, V> ReadOutputService<P, D, F, A, R, V>
where
    P: PublicationRepository,
    D: DeliveryRepository,
    F: FeedbackRepository,
    A: AuditTrailRepository,
    R: ReadProjectionRepository,
    V: RecoveryRepository,
{
    fn not_found(code: &'static str, message: impl Into<String>) -> ApplicationError {
        ApplicationError::not_found(code, message, None)
    }

    fn page_offset(page: &PageRequest) -> Result<usize, ApplicationError> {
        page.page_token
            .as_ref()
            .map(|token| {
                token.as_str().parse::<usize>().map_err(|_| {
                    ApplicationError::validation(
                        "validation.page_token_invalid",
                        "page token must be a numeric offset",
                    )
                })
            })
            .transpose()
            .map(|value| value.unwrap_or(0))
    }

    fn next_cursor(offset: usize, returned: usize, total: usize) -> Option<PageToken> {
        let next = offset + returned;
        (next < total).then(|| PageToken::new(next.to_string()))
    }

    fn projection_marker(status: ProjectionStatus) -> ConsistencyMarker {
        match status {
            ProjectionStatus::Active => ConsistencyMarker::Committed,
            ProjectionStatus::Building | ProjectionStatus::Stale | ProjectionStatus::Rebuilding => {
                ConsistencyMarker::Stale
            }
        }
    }
}

impl<P, D, F, A, R, V> ReadOutputUseCase for ReadOutputService<P, D, F, A, R, V>
where
    P: PublicationRepository,
    D: DeliveryRepository,
    F: FeedbackRepository,
    A: AuditTrailRepository,
    R: ReadProjectionRepository,
    V: RecoveryRepository,
{
    async fn get_publication_acceptance(
        &self,
        query: GetPublicationAcceptanceQuery,
        _actor: ActorContext,
    ) -> Result<PublicationAcceptanceView, ApplicationError> {
        let acceptance = self
            .deps
            .publication_repository
            .get(&query.publication_id)
            .await?
            .ok_or_else(|| {
                Self::not_found(
                    "not_found.publication_acceptance",
                    format!(
                        "publication '{}' was not found",
                        query.publication_id.as_str()
                    ),
                )
            })?;
        let material = self
            .deps
            .publication_repository
            .get_material(&query.publication_id)
            .await?
            .ok_or_else(|| {
                Self::not_found(
                    "not_found.publication_material",
                    "publication material snapshot was not found",
                )
            })?;
        let audit_ref = acceptance.decision_audit_ref.clone().ok_or_else(|| {
            ApplicationError::from(crate::errors::RepositoryError::CorruptedRecord)
        })?;

        Ok(PublicationAcceptanceView {
            publication_id: acceptance.publication_id,
            acceptance_status: acceptance.status,
            source_record_ref: material.source_record_ref,
            core_event_ref: material.core_event_ref,
            core_event_envelope_ref: material.core_event_envelope_ref,
            payload_ref: material.payload_ref,
            delivery_mode: material.delivery_mode,
            target_scope: material.target_scope,
            rejection_reason_ref: acceptance.reject_reason.map(|reason| reason.reason_ref()),
            audit_ref,
        })
    }

    async fn get_delivery_status(
        &self,
        query: GetDeliveryStatusQuery,
        _actor: ActorContext,
    ) -> Result<DeliveryStatusView, ApplicationError> {
        let delivery = self
            .deps
            .delivery_repository
            .get(&query.delivery_id)
            .await?
            .ok_or_else(|| {
                Self::not_found(
                    "not_found.delivery",
                    format!("delivery '{}' was not found", query.delivery_id.as_str()),
                )
            })?;
        let feedbacks = self
            .deps
            .feedback_repository
            .find_by_delivery(
                &query.delivery_id,
                PageRequest {
                    limit: u32::MAX,
                    page_token: None,
                },
            )
            .await?;
        let last_feedback_id = feedbacks
            .iter()
            .max_by(|left, right| left.observed_at.as_str().cmp(right.observed_at.as_str()))
            .map(|feedback| feedback.feedback_id.clone());

        Ok(DeliveryStatusView {
            delivery_id: delivery.delivery_id,
            publication_id: delivery.publication_id,
            delivery_status: delivery.status,
            current_attempt_id: delivery.last_attempt_ref.as_ref().map(|attempt_ref| {
                bus_contracts::metadata::DeliveryAttemptId::new(attempt_ref.as_str())
            }),
            last_feedback_id,
            consistency_marker: ConsistencyMarker::Committed,
        })
    }

    async fn list_delivery_history(
        &self,
        query: ListDeliveryHistoryQuery,
        _actor: ActorContext,
    ) -> Result<DeliveryHistoryPage, ApplicationError> {
        let delivery = self
            .deps
            .delivery_repository
            .get(&query.delivery_id)
            .await?
            .ok_or_else(|| {
                Self::not_found(
                    "not_found.delivery",
                    format!("delivery '{}' was not found", query.delivery_id.as_str()),
                )
            })?;
        let offset = Self::page_offset(&query.page)?;
        let limit = usize::try_from(query.page.limit).unwrap_or(usize::MAX);
        let history = delivery.history().to_vec();
        let items = history
            .iter()
            .skip(offset)
            .take(limit)
            .map(|entry| DeliveryHistoryItemView {
                history_id: entry.history_id.clone(),
                from_status: entry.from_status,
                to_status: entry.to_status,
                reason: entry.reason.clone(),
                occurred_at: entry.occurred_at.clone(),
            })
            .collect::<Vec<_>>();

        Ok(DeliveryHistoryPage {
            delivery_id: delivery.delivery_id,
            next_cursor: Self::next_cursor(offset, items.len(), history.len()),
            items,
        })
    }

    async fn get_transport_view(
        &self,
        query: GetTransportViewQuery,
        _actor: ActorContext,
    ) -> Result<TransportView, ApplicationError> {
        let projection = self
            .deps
            .projection_repository
            .get_transport_view_projection(&query.transport_view_id)
            .await?
            .ok_or_else(|| {
                Self::not_found(
                    "not_found.transport_view",
                    format!(
                        "transport view '{}' was not found",
                        query.transport_view_id.as_str()
                    ),
                )
            })?;
        let delivery = self
            .deps
            .delivery_repository
            .get(&projection.delivery_id)
            .await?
            .ok_or_else(|| {
                Self::not_found(
                    "not_found.delivery",
                    format!(
                        "delivery '{}' was not found",
                        projection.delivery_id.as_str()
                    ),
                )
            })?;

        Ok(TransportView {
            transport_view_id: projection.view_id,
            delivery_id: projection.delivery_id,
            transport_status: delivery.status,
            transport_semantic: delivery.transport_semantic().delivery_mode,
            projection_version: projection.version,
            consistency_marker: Self::projection_marker(projection.status),
        })
    }

    async fn get_failure_summary(
        &self,
        query: GetFailureSummaryQuery,
        _actor: ActorContext,
    ) -> Result<FailureSummaryView, ApplicationError> {
        let projection = self
            .deps
            .projection_repository
            .get_failure_summary_projection(&query.failure_summary_id)
            .await?
            .ok_or_else(|| {
                Self::not_found(
                    "not_found.failure_summary",
                    format!(
                        "failure summary '{}' was not found",
                        query.failure_summary_id.as_str()
                    ),
                )
            })?;
        let material = self
            .deps
            .recovery_repository
            .get_failure_material(&projection.failure_material_id)
            .await?
            .ok_or_else(|| {
                Self::not_found(
                    "not_found.failure_material",
                    format!(
                        "failure material '{}' was not found",
                        projection.failure_material_id.as_str()
                    ),
                )
            })?;

        Ok(FailureSummaryView {
            failure_summary_id: projection.summary_id,
            delivery_id: material.delivery_id,
            failure_material_ref: bus_contracts::metadata::FailureMaterialRef::from(
                material.failure_material_id,
            ),
            failure_kind: FailureKind::TransportFailure,
            governance_decision_ref: None,
        })
    }

    async fn get_bus_audit_trail(
        &self,
        query: GetBusAuditTrailQuery,
        _actor: ActorContext,
    ) -> Result<BusAuditTrailView, ApplicationError> {
        self.deps
            .audit_repository
            .list(query.filter, query.page)
            .await
            .map_err(ApplicationError::from)
    }

    async fn get_backend_health_view(
        &self,
        query: GetBackendHealthViewQuery,
        _actor: ActorContext,
    ) -> Result<BackendHealthView, ApplicationError> {
        let view = self
            .deps
            .projection_repository
            .get_backend_health_view(&query.backend_id)
            .await?
            .ok_or_else(|| {
                Self::not_found(
                    "not_found.backend_health",
                    format!(
                        "backend health '{}' was not found",
                        query.backend_id.as_str()
                    ),
                )
            })?;

        if view.secret_ref.is_some() {
            return Err(ApplicationError::boundary_violation(
                "boundary.secret_ref_rejected",
                "backend health view must not expose secret content",
                None,
            ));
        }
        if !matches!(
            view.capability_status,
            BackendCapabilityStatus::Available | BackendCapabilityStatus::Degraded
        ) {
            return Err(ApplicationError::from(
                crate::errors::RepositoryError::CorruptedRecord,
            ));
        }

        Ok(view)
    }
}
