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
            .get_publication_acceptance(query, actor)
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
            .get_delivery_status(query, actor)
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
            .list_delivery_history(query, actor)
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
            .get_transport_view(query, actor)
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
            .get_failure_summary(query, actor)
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
            .get_bus_audit_trail(query, actor)
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
            .get_backend_health_view(query, actor)
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
        BackendFixtureBuilder, PublicationFixtureBuilder, TestRun, TestRunBuilder,
    };
    use bus_contracts::metadata::{
        CommandMetadata, DeliveryMode, IdempotencyKey, QueryConsistency, QueryMetadata,
        RequestMetadata, SubscriberRef, SubscriberScope, Timestamp,
    };
    use bus_contracts::queries::GetTransportViewQuery;
    use bus_domain::audit::{AuditAction, BusAuditEntry, SubjectRef};
    use bus_domain::delivery::DeliveryRecord;
    use bus_domain::publication::{PublicationMaterial, TransportSemantic};
    use bus_domain::read_output::TransportViewProjection;
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

    fn build_api(
        _run: &TestRun,
    ) -> (
        BusQueryApi<QueryService>,
        InMemoryReadProjectionRepository,
        InMemoryDeliveryRepository,
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
            recovery_repository,
        });

        (
            BusQueryApi::new(service),
            projection_repository,
            delivery_repository,
        )
    }

    #[test]
    fn get_transport_view_maps_not_found_to_404() {
        let run = TestRunBuilder::new("api-out-missing").build();
        let (api, _projection_repository, delivery_repository) = build_api(&run);
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
        let (api, projection_repository, delivery_repository) = build_api(&run);
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
}
