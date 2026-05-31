//! Minimal query APIs for read-only bus outputs.

use bus_application::{ApplicationError, ReadOutputUseCase};
use bus_contracts::metadata::{ActorContext, QueryMetadata};
use bus_contracts::queries::{
    GetBackendHealthViewQuery, GetBusAuditTrailQuery, GetDeliveryStatusQuery,
    GetFailureSummaryQuery, GetPublicationAcceptanceQuery, GetTransportViewQuery,
    ListDeliveryHistoryQuery,
};
use bus_contracts::views::{
    BackendHealthView, BusAuditTrailView, DeliveryHistoryPage, DeliveryStatusView,
    FailureSummaryView, PublicationAcceptanceView, TransportView,
};

use crate::ApiError;

/// Minimal query API surface for read-only bus outputs.
pub struct BusQueryApi<U> {
    read_output: U,
}

impl<U> BusQueryApi<U> {
    /// Creates a new query API wrapper.
    pub fn new(read_output: U) -> Self {
        Self { read_output }
    }
}

impl<U> BusQueryApi<U>
where
    U: ReadOutputUseCase,
{
    /// Loads one publication-acceptance view.
    pub async fn get_publication_acceptance(
        &self,
        query: GetPublicationAcceptanceQuery,
        actor: ActorContext,
        meta: QueryMetadata,
    ) -> Result<PublicationAcceptanceView, ApiError> {
        self.read_output
            .get_publication_acceptance(query, actor, meta.clone())
            .await
            .map_err(|error| ApiError::from_request(error, &meta.request))
    }

    /// Loads one delivery-status view.
    pub async fn get_delivery_status(
        &self,
        query: GetDeliveryStatusQuery,
        actor: ActorContext,
        meta: QueryMetadata,
    ) -> Result<DeliveryStatusView, ApiError> {
        self.read_output
            .get_delivery_status(query, actor, meta.clone())
            .await
            .map_err(|error| ApiError::from_request(error, &meta.request))
    }

    /// Loads one page of delivery history.
    pub async fn list_delivery_history(
        &self,
        query: ListDeliveryHistoryQuery,
        actor: ActorContext,
        meta: QueryMetadata,
    ) -> Result<DeliveryHistoryPage, ApiError> {
        self.read_output
            .list_delivery_history(query, actor, meta.clone())
            .await
            .map_err(|error| ApiError::from_request(error, &meta.request))
    }

    /// Loads one transport view.
    pub async fn get_transport_view(
        &self,
        query: GetTransportViewQuery,
        actor: ActorContext,
        meta: QueryMetadata,
    ) -> Result<TransportView, ApiError> {
        self.read_output
            .get_transport_view(query, actor, meta.clone())
            .await
            .map_err(|error| ApiError::from_request(error, &meta.request))
    }

    /// Loads one failure summary.
    pub async fn get_failure_summary(
        &self,
        query: GetFailureSummaryQuery,
        actor: ActorContext,
        meta: QueryMetadata,
    ) -> Result<FailureSummaryView, ApiError> {
        self.read_output
            .get_failure_summary(query, actor, meta.clone())
            .await
            .map_err(|error| ApiError::from_request(error, &meta.request))
    }

    /// Loads one page of audit-trail items.
    pub async fn get_bus_audit_trail(
        &self,
        query: GetBusAuditTrailQuery,
        actor: ActorContext,
        meta: QueryMetadata,
    ) -> Result<BusAuditTrailView, ApiError> {
        self.read_output
            .get_bus_audit_trail(query, actor, meta.clone())
            .await
            .map_err(|error| ApiError::from_request(error, &meta.request))
    }

    /// Loads one backend-health view.
    pub async fn get_backend_health_view(
        &self,
        query: GetBackendHealthViewQuery,
        actor: ActorContext,
        meta: QueryMetadata,
    ) -> Result<BackendHealthView, ApiError> {
        self.read_output
            .get_backend_health_view(query, actor, meta.clone())
            .await
            .map_err(|error| ApiError::from_request(error, &meta.request))
    }
}

#[allow(dead_code)]
fn _assert_error_mapping_signature(error: ApplicationError, meta: &QueryMetadata) -> ApiError {
    ApiError::from_request(error, &meta.request)
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::pin;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    use bus_application::{ReadOutputService, ReadOutputServiceDeps};
    use bus_contracts::fixtures::{
        BackendFixtureBuilder, FeedbackFixtureBuilder, PublicationFixtureBuilder, TestRun,
        TestRunBuilder,
    };
    use bus_contracts::metadata::{
        ActorContext, AuthorizationRef, CommandMetadata, DeliveryAttemptId, DeliveryMode,
        DeliveryStatus, FailureKind, FailureMaterialRef, HistoryReason, IdempotencyKey,
        QueryConsistency, QueryMetadata, RequestMetadata, RequestOrigin, RoleRef, SubscriberRef,
        SubscriberScope, Timestamp,
    };
    use bus_contracts::queries::{GetFailureSummaryQuery, GetTransportViewQuery};
    use bus_domain::audit::{AuditAction, BusAuditEntry, SubjectRef};
    use bus_domain::delivery::DeliveryHistoryEntry;
    use bus_domain::delivery::DeliveryRecord;
    use bus_domain::feedback::FeedbackResult;
    use bus_domain::publication::{PublicationMaterial, TransportSemantic};
    use bus_domain::read_output::{FailureSummaryProjection, TransportViewProjection};
    use bus_domain::recovery::FailureMaterial;
    use bus_infra::{
        InMemoryAuditTrailRepository, InMemoryDeliveryRepository, InMemoryFeedbackRepository,
        InMemoryPublicationRepository, InMemoryReadProjectionRepository,
        InMemoryRecoveryRepository, SharedMemoryStore,
    };

    use super::BusQueryApi;

    type QueryService = ReadOutputService<
        InMemoryPublicationRepository,
        InMemoryDeliveryRepository,
        InMemoryFeedbackRepository,
        InMemoryAuditTrailRepository,
        InMemoryReadProjectionRepository,
        InMemoryRecoveryRepository,
    >;

    fn noop_raw_waker() -> RawWaker {
        fn clone(_: *const ()) -> RawWaker {
            noop_raw_waker()
        }
        fn wake(_: *const ()) {}
        fn wake_by_ref(_: *const ()) {}
        fn drop(_: *const ()) {}

        RawWaker::new(
            std::ptr::null(),
            &RawWakerVTable::new(clone, wake, wake_by_ref, drop),
        )
    }

    fn block_on<F>(future: F) -> F::Output
    where
        F: Future,
    {
        let waker = unsafe { Waker::from_raw(noop_raw_waker()) };
        let mut context = Context::from_waker(&waker);
        let mut future = pin!(future);

        loop {
            match Future::poll(future.as_mut(), &mut context) {
                Poll::Ready(output) => return output,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    fn query_metadata() -> QueryMetadata {
        QueryMetadata {
            request: RequestMetadata::new(
                bus_contracts::metadata::RequestId::new("request-query"),
                bus_contracts::metadata::TraceId::new("trace-query"),
                None,
                Timestamp::new("2026-05-31T00:00:00Z"),
            ),
            page: None,
            consistency: QueryConsistency::Eventual,
        }
    }

    fn privileged_query_actor(run: &TestRun) -> ActorContext {
        let mut actor = ActorContext::new(run.actor.actor_ref().clone(), RequestOrigin::Query);
        actor.role_refs.push(RoleRef::new("role_api_query_reader"));
        actor
    }

    fn scheduled_delivery(run: &TestRun) -> DeliveryRecord {
        let publication_builder = PublicationFixtureBuilder::new(run.clone());
        let backend_builder = BackendFixtureBuilder::new(run.clone());
        let material = PublicationMaterial::from_accept_publication_command(
            publication_builder.valid_material(),
            run.actor.clone(),
            CommandMetadata {
                request: run.metadata.request.clone(),
                reason: run.metadata.reason.clone(),
                external_ref: run.metadata.external_ref.clone(),
            },
        )
        .expect("publication material should be valid");
        let capability_ref = backend_builder.in_memory_capability();
        let semantic = TransportSemantic::derive(
            material,
            capability_ref,
            SubscriberScope {
                project_id: format!("project_{}", run.run_id),
                topic: format!("workitem.events.{}", run.run_id),
            },
        )
        .expect("transport semantic should derive");

        DeliveryRecord::schedule(
            semantic,
            SubscriberRef::new("subscriber_api_query"),
            IdempotencyKey::new(format!("idem_api_query_{}", run.run_id)),
        )
        .expect("delivery should schedule")
    }

    fn projection_for(delivery: &DeliveryRecord, run: &TestRun) -> TransportViewProjection {
        TransportViewProjection::derive(
            delivery.clone(),
            BusAuditEntry::record(
                bus_contracts::metadata::AuditRef::new(format!("audit_api_{}", run.run_id)),
                SubjectRef::Delivery(delivery.delivery_id.clone()),
                AuditAction::DeliveryDispatchStarted,
                run.actor.clone(),
                run.metadata.request.trace_id.clone(),
                Timestamp::new("2026-05-31T00:00:00Z"),
            ),
        )
        .expect("projection should derive")
    }

    fn failure_material_for(
        run: &TestRun,
        delivery_id: bus_contracts::metadata::DeliveryId,
    ) -> FailureMaterial {
        let feedback_builder = FeedbackFixtureBuilder::new(run.clone());
        let feedback = FeedbackResult::from_command(
            feedback_builder.fail_command(
                delivery_id.clone(),
                DeliveryAttemptId::new("attempt_api_failure"),
            ),
            run.actor.clone(),
        )
        .expect("failure feedback should build");
        let history = DeliveryHistoryEntry::transition(
            delivery_id,
            DeliveryStatus::Dispatching,
            DeliveryStatus::Failed,
            HistoryReason::delivery_failed(),
            Timestamp::new("2026-05-31T00:00:15Z"),
        );

        FailureMaterial::from_feedback(
            feedback,
            history,
            bus_contracts::metadata::AuditRef::new(format!("audit_api_failure_{}", run.run_id)),
        )
        .expect("failure material should derive")
    }

    fn failure_summary_projection_for(
        material: &FailureMaterial,
        run: &TestRun,
    ) -> FailureSummaryProjection {
        FailureSummaryProjection::derive(
            material.clone(),
            BusAuditEntry::record(
                material.audit_ref.clone(),
                SubjectRef::Delivery(material.delivery_id.clone()),
                AuditAction::DeliveryFailed(
                    bus_contracts::metadata::FailureReason::dispatch_failed(),
                ),
                run.actor.clone(),
                run.metadata.request.trace_id.clone(),
                Timestamp::new("2026-05-31T00:00:16Z"),
            ),
        )
        .expect("failure summary projection should derive")
    }

    fn build_api(
        _run: &TestRun,
    ) -> (
        BusQueryApi<QueryService>,
        InMemoryReadProjectionRepository,
        InMemoryDeliveryRepository,
        InMemoryRecoveryRepository,
    ) {
        let store = SharedMemoryStore::new();
        let publication_repository = InMemoryPublicationRepository::new(store.clone());
        let delivery_repository = InMemoryDeliveryRepository::new(store.clone());
        let feedback_repository = InMemoryFeedbackRepository::new(store.clone());
        let audit_repository = InMemoryAuditTrailRepository::new(store.clone());
        let projection_repository = InMemoryReadProjectionRepository::new(store.clone());
        let recovery_repository = InMemoryRecoveryRepository::new(store);
        let service = ReadOutputService::new(ReadOutputServiceDeps {
            publication_repository,
            delivery_repository: delivery_repository.clone(),
            feedback_repository,
            audit_repository,
            projection_repository: projection_repository.clone(),
            recovery_repository: recovery_repository.clone(),
        });

        (
            BusQueryApi::new(service),
            projection_repository,
            delivery_repository,
            recovery_repository,
        )
    }

    #[test]
    fn get_transport_view_maps_not_found_to_404() {
        let run = TestRunBuilder::new("api-out-missing").build();
        let (api, _projection_repository, delivery_repository, _recovery_repository) =
            build_api(&run);
        let delivery = scheduled_delivery(&run);
        delivery_repository
            .seed_committed(delivery.clone())
            .expect("delivery should seed");

        let error = block_on(api.get_transport_view(
            GetTransportViewQuery {
                transport_view_id: bus_contracts::metadata::TransportViewId::from_delivery_id(
                    &delivery.delivery_id,
                ),
            },
            run.actor.clone(),
            query_metadata(),
        ))
        .expect_err("missing projection should map to api error");

        assert_eq!(error.status_code, 404);
        assert_eq!(error.code, "not_found.transport_view");
    }

    #[test]
    fn get_transport_view_returns_query_view() {
        let run = TestRunBuilder::new("api-out-current").build();
        let (api, projection_repository, delivery_repository, _recovery_repository) =
            build_api(&run);
        let delivery = scheduled_delivery(&run);
        delivery_repository
            .seed_committed(delivery.clone())
            .expect("delivery should seed");
        let projection = projection_for(&delivery, &run);
        projection_repository
            .seed_transport_view_projection(projection.clone())
            .expect("projection should seed");

        let view = block_on(api.get_transport_view(
            GetTransportViewQuery {
                transport_view_id: projection.view_id.clone(),
            },
            run.actor.clone(),
            query_metadata(),
        ))
        .expect("query view should load");

        assert_eq!(
            view.consistency_marker,
            bus_contracts::metadata::ConsistencyMarker::Committed
        );
        assert_eq!(view.transport_semantic, DeliveryMode::AtLeastOnce);
    }

    #[test]
    fn get_failure_summary_returns_api_view_without_decision_content() {
        let run = TestRunBuilder::new("api-out-failure-summary").build();
        let (api, projection_repository, _delivery_repository, recovery_repository) =
            build_api(&run);
        let actor = privileged_query_actor(&run);
        let delivery = scheduled_delivery(&run);
        let material = failure_material_for(&run, delivery.delivery_id.clone());
        recovery_repository
            .seed_failure_material(material.clone())
            .expect("failure material should seed");
        let projection = failure_summary_projection_for(&material, &run);
        projection_repository
            .seed_failure_summary_projection(projection.clone())
            .expect("failure summary projection should seed");

        let view = block_on(api.get_failure_summary(
            GetFailureSummaryQuery {
                failure_summary_id: projection.summary_id.clone(),
                authorization_ref: Some(AuthorizationRef::new("auth_api_failure_summary")),
            },
            actor,
            query_metadata(),
        ))
        .expect("failure summary api should load");

        assert_eq!(view.failure_summary_id, projection.summary_id);
        assert_eq!(
            view.failure_material_ref,
            FailureMaterialRef::from(material.failure_material_id)
        );
        assert_eq!(view.failure_kind, FailureKind::TransportFailure);
        assert_eq!(view.governance_decision_ref, None);
    }

    #[test]
    fn get_failure_summary_maps_missing_authorization_reference_to_422() {
        let run = TestRunBuilder::new("api-out-failure-summary-auth").build();
        let (api, projection_repository, _delivery_repository, recovery_repository) =
            build_api(&run);
        let actor = privileged_query_actor(&run);
        let delivery = scheduled_delivery(&run);
        let material = failure_material_for(&run, delivery.delivery_id.clone());
        recovery_repository
            .seed_failure_material(material.clone())
            .expect("failure material should seed");
        let projection = failure_summary_projection_for(&material, &run);
        projection_repository
            .seed_failure_summary_projection(projection.clone())
            .expect("failure summary projection should seed");

        let error = block_on(api.get_failure_summary(
            GetFailureSummaryQuery {
                failure_summary_id: projection.summary_id,
                authorization_ref: None,
            },
            actor,
            query_metadata(),
        ))
        .expect_err("missing authorization reference should map to api error");

        assert_eq!(error.status_code, 422);
        assert_eq!(error.code, "boundary.authorization_ref_required");
    }
}
