use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use bus_application::{
    DeliveryFeedbackUseCase, FeedbackRecordingService, FeedbackRecordingServiceDeps,
};
use bus_contracts::fixtures::{
    BackendFixtureBuilder, FeedbackFixtureBuilder, PublicationFixtureBuilder, TestRun,
    TestRunBuilder,
};
use bus_contracts::metadata::{
    BackendDeliveryResult, DeliveryStatus, IdempotencyKey, SubscriberRef, SubscriberScope,
    Timestamp,
};
use bus_domain::audit::AuditAction;
use bus_domain::delivery::{DeliveryHistoryEntry, DeliveryRecord};
use bus_domain::idempotency::{IdempotencyScope, RecordRef};
use bus_domain::publication::{PublicationMaterial, TransportSemantic};
use bus_infra::{
    DeterministicIdGenerator, FixedClockAdapter, InMemoryAuditTrailRepository,
    InMemoryDeliveryRepository, InMemoryFeedbackRepository, InMemoryIdempotencyRepository,
    InMemoryUnitOfWork, SharedMemoryStore,
};

type FeedbackService = FeedbackRecordingService<
    InMemoryFeedbackRepository,
    InMemoryDeliveryRepository,
    InMemoryIdempotencyRepository,
    InMemoryAuditTrailRepository,
    InMemoryUnitOfWork,
    FixedClockAdapter,
    DeterministicIdGenerator,
>;

struct Harness {
    service: FeedbackService,
    delivery_repository: InMemoryDeliveryRepository,
    feedback_repository: InMemoryFeedbackRepository,
    idempotency_repository: InMemoryIdempotencyRepository,
    audit_repository: InMemoryAuditTrailRepository,
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

fn build_harness(run: &TestRun) -> Harness {
    let store = SharedMemoryStore::new();
    let delivery_repository = InMemoryDeliveryRepository::new(store.clone());
    let feedback_repository = InMemoryFeedbackRepository::new(store.clone());
    let idempotency_repository = InMemoryIdempotencyRepository::new(store.clone());
    let audit_repository = InMemoryAuditTrailRepository::new(store.clone());
    let service = FeedbackRecordingService::new(FeedbackRecordingServiceDeps {
        feedback_repository: feedback_repository.clone(),
        delivery_repository: delivery_repository.clone(),
        idempotency_repository: idempotency_repository.clone(),
        audit_repository: audit_repository.clone(),
        unit_of_work: InMemoryUnitOfWork::new(store),
        clock: FixedClockAdapter::new(run.metadata.request.requested_at.clone()),
        id_generator: DeterministicIdGenerator::new(),
    });

    Harness {
        service,
        delivery_repository,
        feedback_repository,
        idempotency_repository,
        audit_repository,
    }
}

fn seed_delivered_delivery(
    repository: &InMemoryDeliveryRepository,
    run: &TestRun,
) -> DeliveryRecord {
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
        capability_ref.clone(),
        SubscriberScope {
            project_id: format!("project_{}", run.run_id),
            topic: format!("workitem.events.{}", run.run_id),
        },
    )
    .expect("transport semantic should be derived");
    let mut delivery = DeliveryRecord::schedule(
        semantic,
        SubscriberRef::new("subscriber_alpha"),
        IdempotencyKey::new(format!("idem_delivery_{}", run.run_id)),
    )
    .expect("delivery should schedule");
    let mut attempt = delivery
        .start_attempt(capability_ref, Timestamp::new("2026-05-30T00:00:01Z"))
        .expect("delivery should start");
    delivery
        .append_history(DeliveryHistoryEntry::transition(
            delivery.delivery_id.clone(),
            DeliveryStatus::Scheduled,
            DeliveryStatus::Dispatching,
            bus_contracts::metadata::HistoryReason::dispatching_started(),
            Timestamp::new("2026-05-30T00:00:01Z"),
        ))
        .expect("dispatch history should append");
    attempt
        .finish(
            BackendDeliveryResult::delivered(Some("backend_delivery_feedback".into())),
            Timestamp::new("2026-05-30T00:00:02Z"),
        )
        .expect("attempt should finish");
    delivery
        .mark_delivered(attempt, run.actor.clone())
        .expect("delivery should become delivered");
    delivery
        .append_history(DeliveryHistoryEntry::transition(
            delivery.delivery_id.clone(),
            DeliveryStatus::Dispatching,
            DeliveryStatus::Delivered,
            bus_contracts::metadata::HistoryReason::delivery_arrived(),
            Timestamp::new("2026-05-30T00:00:02Z"),
        ))
        .expect("delivered history should append");
    repository
        .seed_committed(delivery.clone())
        .expect("delivery should seed");

    delivery
}

#[test]
fn record_ack_feedback_commits_truth_history_and_idempotency_anchor() {
    let run = TestRunBuilder::new("svc-fdb-001").build();
    let harness = build_harness(&run);
    let delivery = seed_delivered_delivery(&harness.delivery_repository, &run);
    let builder = FeedbackFixtureBuilder::new(run.clone());
    let command = builder.ack_command(
        delivery.delivery_id.clone(),
        delivery
            .last_attempt_ref
            .clone()
            .expect("delivered delivery should have attempt ref")
            .as_str()
            .into(),
    );

    let result = block_on(harness.service.record(
        command.clone(),
        run.actor.clone(),
        run.metadata.clone(),
    ))
    .expect("ack feedback should record");

    assert_eq!(result.delivery_status, DeliveryStatus::Completed);

    let feedback = harness
        .feedback_repository
        .committed(&result.feedback_id)
        .expect("feedback should be committed");
    assert_eq!(
        feedback.status,
        bus_contracts::metadata::FeedbackStatus::Ack
    );
    assert_eq!(feedback.audit_ref(), Some(&result.audit_ref));

    let committed = harness
        .delivery_repository
        .committed(&delivery.delivery_id)
        .expect("delivery should be committed");
    assert_eq!(committed.status, DeliveryStatus::Completed);
    assert_eq!(committed.history().len(), 3);
    assert_eq!(
        committed
            .history()
            .last()
            .expect("history should exist")
            .reason,
        bus_contracts::metadata::HistoryReason::feedback_ack()
    );

    let key = run
        .metadata
        .request
        .idempotency_key
        .as_ref()
        .expect("fixture should include idempotency key");
    let anchor = harness
        .idempotency_repository
        .committed_anchor(
            &IdempotencyScope::for_record_delivery_feedback(&command),
            key,
        )
        .expect("feedback anchor should bind");
    assert_eq!(
        anchor.bound_record_ref,
        RecordRef::Feedback(result.feedback_id.clone())
    );

    let audits = harness.audit_repository.committed_entries();
    assert_eq!(audits.len(), 1);
    assert_eq!(
        audits[0].action,
        AuditAction::FeedbackRecorded(bus_contracts::metadata::FeedbackStatus::Ack)
    );
}

#[test]
fn record_feedback_same_key_same_digest_returns_existing_result() {
    let run = TestRunBuilder::new("svc-fdb-002").build();
    let harness = build_harness(&run);
    let delivery = seed_delivered_delivery(&harness.delivery_repository, &run);
    let builder = FeedbackFixtureBuilder::new(run.clone());
    let command = builder.ack_command(
        delivery.delivery_id.clone(),
        delivery
            .last_attempt_ref
            .clone()
            .expect("attempt ref should exist")
            .as_str()
            .into(),
    );

    let first = block_on(harness.service.record(
        command.clone(),
        run.actor.clone(),
        run.metadata.clone(),
    ))
    .expect("first feedback should commit");
    let second = block_on(harness.service.record(
        command.clone(),
        run.actor.clone(),
        run.metadata.clone(),
    ))
    .expect("same digest should return existing result");

    assert_eq!(first, second);
    assert_eq!(harness.feedback_repository.committed_all().len(), 1);
    assert_eq!(harness.audit_repository.committed_entries().len(), 1);
}

#[test]
fn record_feedback_same_key_different_digest_returns_conflict() {
    let run = TestRunBuilder::new("svc-fdb-003").build();
    let harness = build_harness(&run);
    let delivery = seed_delivered_delivery(&harness.delivery_repository, &run);
    let builder = FeedbackFixtureBuilder::new(run.clone());
    let command = builder.ack_command(
        delivery.delivery_id.clone(),
        delivery
            .last_attempt_ref
            .clone()
            .expect("attempt ref should exist")
            .as_str()
            .into(),
    );

    block_on(
        harness
            .service
            .record(command.clone(), run.actor.clone(), run.metadata.clone()),
    )
    .expect("first feedback should commit");

    let conflicting = builder.fail_command(
        delivery.delivery_id.clone(),
        delivery
            .last_attempt_ref
            .clone()
            .expect("attempt ref should exist")
            .as_str()
            .into(),
    );
    let error = block_on(harness.service.record(
        conflicting,
        run.actor.clone(),
        run.metadata.clone(),
    ))
    .expect_err("different digest should conflict");

    assert_eq!(error.code(), "conflict.idempotency_request_mismatch");
    assert_eq!(harness.feedback_repository.committed_all().len(), 1);
    assert_eq!(
        harness.idempotency_repository.committed_conflicts().len(),
        1
    );
}

#[test]
fn record_feedback_rejects_unknown_delivery_without_orphan_feedback() {
    let run = TestRunBuilder::new("svc-fdb-004").build();
    let harness = build_harness(&run);
    let builder = FeedbackFixtureBuilder::new(run.clone());
    let command = builder.ack_command("delivery_missing".into(), "attempt_missing".into());

    let error = block_on(harness.service.record(command, run.actor, run.metadata))
        .expect_err("unknown delivery should be rejected");

    assert_eq!(error.code(), "not_found.delivery");
    assert!(harness.feedback_repository.committed_all().is_empty());
}
