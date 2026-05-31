use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use bus_application::{
    AuditTrailRepository, MoveToDeadLetterUseCase, RecoveryOrchestrationService,
    RecoveryOrchestrationServiceDeps, ReplayPreparationService, ReplayPreparationServiceDeps,
    ReplayPreparationUseCase, RequestRetryUseCase, UnitOfWork, UnitOfWorkPurpose,
};
use bus_contracts::fixtures::{
    BackendFixtureBuilder, FeedbackFixtureBuilder, PublicationFixtureBuilder,
    RecoveryFixtureBuilder, TestRun, TestRunBuilder,
};
use bus_contracts::metadata::{
    AuditChainRef, AuditRef, DeliveryStatus, FeedbackKind, FeedbackReason, FeedbackStatus,
    HistoryReason, IdempotencyKey, SubscriberRef, SubscriberScope, Timestamp,
};
use bus_domain::audit::{AuditAction, BusAuditEntry, SubjectRef};
use bus_domain::delivery::{DeliveryHistoryEntry, DeliveryRecord};
use bus_domain::feedback::FeedbackResult;
use bus_domain::publication::{PublicationMaterial, TransportSemantic};
use bus_domain::recovery::FailureMaterial;
use bus_infra::{
    DeterministicIdGenerator, FixedClockAdapter, InMemoryAuditTrailRepository,
    InMemoryDeliveryRepository, InMemoryRecoveryRepository, InMemoryTransportBackendAdapter,
    InMemoryUnitOfWork, SharedMemoryStore,
};

type RecoveryService = RecoveryOrchestrationService<
    InMemoryDeliveryRepository,
    InMemoryRecoveryRepository,
    InMemoryAuditTrailRepository,
    InMemoryUnitOfWork,
    FixedClockAdapter,
    DeterministicIdGenerator,
    InMemoryTransportBackendAdapter,
>;

type ReplayService = ReplayPreparationService<
    InMemoryRecoveryRepository,
    InMemoryAuditTrailRepository,
    InMemoryUnitOfWork,
    FixedClockAdapter,
    DeterministicIdGenerator,
>;

struct Harness {
    recovery_service: RecoveryService,
    replay_service: ReplayService,
    delivery_repository: InMemoryDeliveryRepository,
    recovery_repository: InMemoryRecoveryRepository,
    audit_repository: InMemoryAuditTrailRepository,
    unit_of_work: InMemoryUnitOfWork,
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
    let recovery_repository = InMemoryRecoveryRepository::new(store.clone());
    let audit_repository = InMemoryAuditTrailRepository::new(store.clone());
    let unit_of_work = InMemoryUnitOfWork::new(store.clone());
    let clock = FixedClockAdapter::new(Timestamp::new("2026-05-31T00:05:00Z"));
    let id_generator = DeterministicIdGenerator::new();
    let backend = InMemoryTransportBackendAdapter::new(
        BackendFixtureBuilder::new(run.clone()).in_memory_capability(),
    );
    let recovery_service = RecoveryOrchestrationService::new(RecoveryOrchestrationServiceDeps {
        delivery_repository: delivery_repository.clone(),
        recovery_repository: recovery_repository.clone(),
        audit_repository: audit_repository.clone(),
        unit_of_work: unit_of_work.clone(),
        clock: clock.clone(),
        id_generator: id_generator.clone(),
        transport_backend: backend.clone(),
    });
    let replay_service = ReplayPreparationService::new(ReplayPreparationServiceDeps {
        recovery_repository: recovery_repository.clone(),
        audit_repository: audit_repository.clone(),
        unit_of_work: unit_of_work.clone(),
        clock,
        id_generator,
    });

    Harness {
        recovery_service,
        replay_service,
        delivery_repository,
        recovery_repository,
        audit_repository,
        unit_of_work,
    }
}

fn delivered_delivery_fixture(run: &TestRun) -> DeliveryRecord {
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
        .start_attempt(capability_ref, Timestamp::new("2026-05-31T00:00:01Z"))
        .expect("delivery should start");
    delivery
        .append_history(DeliveryHistoryEntry::transition(
            delivery.delivery_id.clone(),
            DeliveryStatus::Scheduled,
            DeliveryStatus::Dispatching,
            HistoryReason::dispatching_started(),
            Timestamp::new("2026-05-31T00:00:01Z"),
        ))
        .expect("dispatch history should append");
    attempt
        .finish(
            bus_contracts::metadata::BackendDeliveryResult::delivered(Some(
                "backend_delivery_recovery".into(),
            )),
            Timestamp::new("2026-05-31T00:00:02Z"),
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
            HistoryReason::delivery_arrived(),
            Timestamp::new("2026-05-31T00:00:02Z"),
        ))
        .expect("delivered history should append");

    delivery
}

fn seed_failed_delivery_with_material(
    harness: &Harness,
    run: &TestRun,
) -> (DeliveryRecord, FailureMaterial) {
    let mut delivery = delivered_delivery_fixture(run);
    let feedback_builder = FeedbackFixtureBuilder::new(run.clone());
    let fail_command = bus_contracts::commands::RecordDeliveryFeedbackCommand {
        delivery_id: delivery.delivery_id.clone(),
        attempt_id: delivery
            .last_attempt_ref
            .clone()
            .expect("attempt ref should exist")
            .as_str()
            .into(),
        feedback_kind: FeedbackKind::Fail,
        feedback_reason: FeedbackReason::new("subscriber_failed"),
        observed_at: Timestamp::new("2026-05-31T00:00:03Z"),
        external_feedback_ref: feedback_builder
            .fail_command(
                delivery.delivery_id.clone(),
                delivery
                    .last_attempt_ref
                    .clone()
                    .expect("attempt ref should exist")
                    .as_str()
                    .into(),
            )
            .external_feedback_ref,
    };
    let feedback = FeedbackResult::from_command(fail_command, run.actor.clone())
        .expect("failed feedback should build");
    delivery
        .mark_failed(feedback.failure_reason(), run.actor.clone())
        .expect("delivery should become failed");
    let failed_history = DeliveryHistoryEntry::transition(
        delivery.delivery_id.clone(),
        DeliveryStatus::Delivered,
        DeliveryStatus::Failed,
        HistoryReason::feedback_fail(),
        Timestamp::new("2026-05-31T00:00:03Z"),
    );
    delivery
        .append_history(failed_history.clone())
        .expect("failed history should append");

    let audit_ref = AuditRef::new(format!("audit_failure_{}", run.run_id));
    let failure_material =
        FailureMaterial::from_feedback(feedback, failed_history, audit_ref.clone())
            .expect("failure material should build");
    harness
        .delivery_repository
        .seed_committed(delivery.clone())
        .expect("failed delivery should seed");
    harness
        .recovery_repository
        .seed_failure_material(failure_material.clone())
        .expect("failure material should seed");

    let audit_entry = BusAuditEntry::record(
        audit_ref,
        SubjectRef::Delivery(delivery.delivery_id.clone()),
        AuditAction::FeedbackRecorded(FeedbackStatus::Fail),
        run.actor.clone(),
        run.metadata.request.trace_id.clone(),
        Timestamp::new("2026-05-31T00:00:03Z"),
    );
    let uow = block_on(
        harness
            .unit_of_work
            .begin(UnitOfWorkPurpose::RecordDeliveryFeedback, run.actor.clone()),
    )
    .expect("audit seed should begin");
    block_on(harness.audit_repository.append(audit_entry, &uow)).expect("audit seed should append");
    block_on(harness.unit_of_work.commit(uow)).expect("audit seed should commit");

    (delivery, failure_material)
}

#[test]
fn request_retry_commits_scheduled_plan_with_remaining_attempts() {
    let run = TestRunBuilder::new("svc-rec-001").build();
    let harness = build_harness(&run);
    let (delivery, failure_material) = seed_failed_delivery_with_material(&harness, &run);
    let builder = RecoveryFixtureBuilder::new(run.clone());
    let mut command = builder.request_retry_command(delivery.delivery_id.clone());
    command.failure_material_ref = failure_material.failure_material_id.clone().into();

    let result = block_on(harness.recovery_service.request_retry(
        command,
        run.actor.clone(),
        run.metadata.clone(),
    ))
    .expect("retry request should commit");

    assert_eq!(result.delivery_id, delivery.delivery_id);
    assert_eq!(result.remaining_attempts.get(), 3);

    let committed_plan = harness
        .recovery_repository
        .committed_retry_plan(&result.retry_plan_id)
        .expect("retry plan should be committed");
    assert_eq!(committed_plan.remaining_attempts.get(), 3);

    let audits = harness.audit_repository.committed_entries();
    assert_eq!(audits.len(), 2);
    assert_eq!(audits[1].action, AuditAction::RetryRequested);
}

#[test]
fn move_to_dead_letter_links_existing_failure_material_and_updates_delivery() {
    let run = TestRunBuilder::new("svc-rec-002").build();
    let harness = build_harness(&run);
    let (delivery, failure_material) = seed_failed_delivery_with_material(&harness, &run);
    let builder = RecoveryFixtureBuilder::new(run.clone());
    let mut command = builder.move_to_dead_letter_command(delivery.delivery_id.clone());
    command.failure_material_ref = failure_material.failure_material_id.clone().into();

    let result = block_on(harness.recovery_service.move_to_dead_letter(
        command,
        run.actor.clone(),
        run.metadata.clone(),
    ))
    .expect("dead-letter move should commit");

    let committed_entry = harness
        .recovery_repository
        .committed_dead_letter(&result.dead_letter_id)
        .expect("dead letter should commit");
    assert_eq!(
        committed_entry.status,
        bus_contracts::metadata::DeadLetterStatus::Open
    );

    let committed_material = harness
        .recovery_repository
        .committed_failure_material(&result.failure_material_ref.clone().into())
        .expect("failure material should remain committed");
    assert_eq!(
        committed_material.dead_letter_ref,
        Some(result.dead_letter_id.clone().into())
    );

    let committed_delivery = harness
        .delivery_repository
        .committed(&delivery.delivery_id)
        .expect("delivery should update");
    assert_eq!(committed_delivery.status, DeliveryStatus::DeadLettered);
    assert_eq!(
        committed_delivery
            .history()
            .last()
            .expect("history should exist")
            .reason,
        HistoryReason::dead_lettered()
    );

    let audits = harness.audit_repository.committed_entries();
    assert_eq!(audits.len(), 2);
    assert_eq!(audits[1].action, AuditAction::DeadLetterCreated);
}

#[test]
fn prepare_replay_requires_a_non_blank_approval_reference() {
    let run = TestRunBuilder::new("svc-rec-003a").build();
    let harness = build_harness(&run);
    let (delivery, failure_material) = seed_failed_delivery_with_material(&harness, &run);
    let builder = RecoveryFixtureBuilder::new(run.clone());
    let mut dead_letter_command = builder.move_to_dead_letter_command(delivery.delivery_id.clone());
    dead_letter_command.failure_material_ref = failure_material.failure_material_id.clone().into();
    let dead_letter = block_on(harness.recovery_service.move_to_dead_letter(
        dead_letter_command,
        run.actor.clone(),
        run.metadata.clone(),
    ))
    .expect("dead letter should commit");

    let mut command = builder.prepare_replay_command(dead_letter.dead_letter_id);
    command.audit_chain_ref = AuditChainRef::from_audit_ref(&failure_material.audit_ref);
    command.approval_ref = "".into();

    let error = block_on(harness.replay_service.prepare(
        command,
        run.actor.clone(),
        run.metadata.clone(),
    ))
    .expect_err("blank approval must be rejected");

    assert_eq!(
        error.category(),
        bus_application::ProtocolErrorCategory::Validation
    );
}

#[test]
fn prepare_replay_rejects_unknown_audit_chain_without_creating_ready_state() {
    let run = TestRunBuilder::new("svc-rec-003b").build();
    let harness = build_harness(&run);
    let (delivery, failure_material) = seed_failed_delivery_with_material(&harness, &run);
    let builder = RecoveryFixtureBuilder::new(run.clone());
    let mut dead_letter_command = builder.move_to_dead_letter_command(delivery.delivery_id.clone());
    dead_letter_command.failure_material_ref = failure_material.failure_material_id.clone().into();
    let dead_letter = block_on(harness.recovery_service.move_to_dead_letter(
        dead_letter_command,
        run.actor.clone(),
        run.metadata.clone(),
    ))
    .expect("dead letter should commit");

    let mut command = builder.prepare_replay_command(dead_letter.dead_letter_id.clone());
    command.audit_chain_ref = AuditChainRef::new("audit_chain_missing");

    let error = block_on(harness.replay_service.prepare(
        command,
        run.actor.clone(),
        run.metadata.clone(),
    ))
    .expect_err("unknown audit chain must be rejected");

    assert_eq!(
        error.category(),
        bus_application::ProtocolErrorCategory::BoundaryViolation
    );
    assert_eq!(harness.audit_repository.committed_entries().len(), 2);
}

#[test]
fn prepare_replay_commits_ready_preparation_after_dead_letter() {
    let run = TestRunBuilder::new("svc-rec-004").build();
    let harness = build_harness(&run);
    let (delivery, failure_material) = seed_failed_delivery_with_material(&harness, &run);
    let builder = RecoveryFixtureBuilder::new(run.clone());
    let mut dead_letter_command = builder.move_to_dead_letter_command(delivery.delivery_id.clone());
    dead_letter_command.failure_material_ref = failure_material.failure_material_id.clone().into();
    let dead_letter = block_on(harness.recovery_service.move_to_dead_letter(
        dead_letter_command,
        run.actor.clone(),
        run.metadata.clone(),
    ))
    .expect("dead letter should commit");

    let mut command = builder.prepare_replay_command(dead_letter.dead_letter_id.clone());
    command.audit_chain_ref = AuditChainRef::from_audit_ref(&failure_material.audit_ref);

    let result = block_on(harness.replay_service.prepare(
        command,
        run.actor.clone(),
        run.metadata.clone(),
    ))
    .expect("replay preparation should commit");

    let committed = harness
        .recovery_repository
        .committed_replay_preparation(&result.replay_preparation_id)
        .expect("replay preparation should be committed");
    assert_eq!(
        committed.status,
        bus_contracts::metadata::ReplayPreparationStatus::Ready
    );
    assert_eq!(committed.approval_ref, Some(builder.replay_approval_ref()));

    let audits = harness.audit_repository.committed_entries();
    assert_eq!(audits.len(), 3);
    assert_eq!(audits[2].action, AuditAction::ReplayPreparationReady);
}
