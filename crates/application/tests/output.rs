use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use bus_application::{
    ReadOutputService, ReadOutputServiceDeps, ReadOutputUseCase, ReadProjectionRepository,
};
use bus_contracts::fixtures::{
    BackendFixtureBuilder, PublicationFixtureBuilder, TestRun, TestRunBuilder,
};
use bus_contracts::metadata::{
    DeliveryStatus, IdempotencyKey, SubscriberRef, SubscriberScope, Timestamp,
};
use bus_contracts::queries::GetTransportViewQuery;
use bus_contracts::views::TransportView;
use bus_domain::audit::{AuditAction, BusAuditEntry, SubjectRef};
use bus_domain::delivery::DeliveryRecord;
use bus_domain::publication::{PublicationMaterial, TransportSemantic};
use bus_domain::read_output::TransportViewProjection;
use bus_infra::{
    InMemoryAuditTrailRepository, InMemoryDeliveryRepository, InMemoryFeedbackRepository,
    InMemoryPublicationRepository, InMemoryReadProjectionRepository, InMemoryRecoveryRepository,
    SharedMemoryStore,
};

type OutputService = ReadOutputService<
    InMemoryPublicationRepository,
    InMemoryDeliveryRepository,
    InMemoryFeedbackRepository,
    InMemoryAuditTrailRepository,
    InMemoryReadProjectionRepository,
    InMemoryRecoveryRepository,
>;

struct Harness {
    service: OutputService,
    delivery_repository: InMemoryDeliveryRepository,
    audit_repository: InMemoryAuditTrailRepository,
    projection_repository: InMemoryReadProjectionRepository,
}

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

fn build_harness() -> Harness {
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
        audit_repository: audit_repository.clone(),
        projection_repository: projection_repository.clone(),
        recovery_repository,
    });

    Harness {
        service,
        delivery_repository,
        audit_repository,
        projection_repository,
    }
}

fn scheduled_delivery(run: &TestRun) -> DeliveryRecord {
    let publication_builder = PublicationFixtureBuilder::new(run.clone());
    let backend_builder = BackendFixtureBuilder::new(run.clone());
    let material = PublicationMaterial::from_accept_publication_command(
        publication_builder.valid_material(),
        run.actor.clone(),
        run.metadata.clone(),
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
        SubscriberRef::new("subscriber_output"),
        IdempotencyKey::new(format!("idem_output_{}", run.run_id)),
    )
    .expect("delivery should schedule")
}

fn projection_for(delivery: &DeliveryRecord, run: &TestRun) -> TransportViewProjection {
    TransportViewProjection::derive(
        delivery.clone(),
        BusAuditEntry::record(
            bus_contracts::metadata::AuditRef::new(format!("audit_output_{}", run.run_id)),
            SubjectRef::Delivery(delivery.delivery_id.clone()),
            AuditAction::DeliveryDispatchStarted,
            run.actor.clone(),
            run.metadata.request.trace_id.clone(),
            Timestamp::new("2026-05-31T00:00:00Z"),
        ),
    )
    .expect("projection should derive")
}

#[test]
fn get_transport_view_returns_committed_projection_without_writes() {
    let run = TestRunBuilder::new("out-001").build();
    let harness = build_harness();
    let delivery = scheduled_delivery(&run);
    harness
        .delivery_repository
        .seed_committed(delivery.clone())
        .expect("delivery should seed");
    let projection = projection_for(&delivery, &run);
    harness
        .projection_repository
        .seed_transport_view_projection(projection.clone())
        .expect("projection should seed");
    let before_delivery = harness
        .delivery_repository
        .committed(&delivery.delivery_id)
        .expect("delivery should exist");
    let before_audits = harness.audit_repository.committed_entries().len();

    let view = block_on(harness.service.get_transport_view(
        GetTransportViewQuery {
            transport_view_id: projection.view_id.clone(),
        },
        run.actor.clone(),
    ))
    .expect("transport view should load");

    assert_eq!(
        view,
        TransportView {
            transport_view_id: projection.view_id.clone(),
            delivery_id: delivery.delivery_id.clone(),
            transport_status: DeliveryStatus::Scheduled,
            transport_semantic: bus_contracts::metadata::DeliveryMode::AtLeastOnce,
            projection_version: projection.version.clone(),
            consistency_marker: bus_contracts::metadata::ConsistencyMarker::Committed,
        }
    );
    assert_eq!(
        harness
            .delivery_repository
            .committed(&delivery.delivery_id)
            .expect("delivery should remain committed"),
        before_delivery
    );
    assert_eq!(
        harness.audit_repository.committed_entries().len(),
        before_audits
    );
}

#[test]
fn get_transport_view_returns_stale_marker_without_rebuild() {
    let run = TestRunBuilder::new("out-002-stale").build();
    let harness = build_harness();
    let delivery = scheduled_delivery(&run);
    harness
        .delivery_repository
        .seed_committed(delivery.clone())
        .expect("delivery should seed");
    let mut projection = projection_for(&delivery, &run);
    projection
        .mark_stale(bus_contracts::metadata::AuditRef::new("audit_output_stale"))
        .expect("projection should become stale");
    harness
        .projection_repository
        .seed_transport_view_projection(projection.clone())
        .expect("projection should seed");

    let view = block_on(harness.service.get_transport_view(
        GetTransportViewQuery {
            transport_view_id: projection.view_id.clone(),
        },
        run.actor.clone(),
    ))
    .expect("stale transport view should still load");

    assert_eq!(
        view.consistency_marker,
        bus_contracts::metadata::ConsistencyMarker::Stale
    );
    let stored = block_on(
        harness
            .projection_repository
            .get_transport_view_projection(&projection.view_id),
    )
    .expect("projection read should succeed")
    .expect("projection should remain committed");
    assert_eq!(
        stored.status,
        bus_domain::read_output::ProjectionStatus::Stale
    );
}

#[test]
fn get_transport_view_missing_projection_returns_not_found() {
    let run = TestRunBuilder::new("out-002-missing").build();
    let harness = build_harness();
    let delivery = scheduled_delivery(&run);
    harness
        .delivery_repository
        .seed_committed(delivery.clone())
        .expect("delivery should seed");
    let projection_id =
        bus_contracts::metadata::TransportViewId::from_delivery_id(&delivery.delivery_id);

    let error = block_on(harness.service.get_transport_view(
        GetTransportViewQuery {
            transport_view_id: projection_id,
        },
        run.actor.clone(),
    ))
    .expect_err("missing projection should return not found");

    assert_eq!(error.code(), "not_found.transport_view");
    assert_eq!(
        harness
            .delivery_repository
            .committed(&delivery.delivery_id)
            .expect("delivery should remain committed")
            .status,
        DeliveryStatus::Scheduled
    );
}
