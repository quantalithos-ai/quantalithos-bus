//! Operations job runners for the bus workspace.

use bus_application::{
    ApplicationError, DeliveryProgressionUseCase, OutboxRelayUseCase, RetryCycleUseCase,
};
use bus_contracts::jobs::{
    DeliveryProgressionResult, OutboxRelayJobResult, RetryCycleResult, RunDeliveryProgressionJob,
    RunOutboxRelayJob, RunRetryCycleJob,
};
use bus_contracts::metadata::{ActorContext, JobMetadata};

/// Stable job-level error categories.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobErrorKind {
    /// The job may be retried safely.
    Retryable,
    /// The job requires operator or developer intervention.
    Failed,
}

/// A job-runner error mapped from the application layer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobError {
    /// The resulting job error kind.
    pub result_kind: JobErrorKind,
    /// The stable external error code.
    pub code: String,
    /// The number of failed items summarized by this error.
    pub failed_items: u32,
    /// Whether the whole job may be retried automatically.
    pub retryable: bool,
}

impl From<ApplicationError> for JobError {
    fn from(error: ApplicationError) -> Self {
        let retryable = error.retryable();
        Self {
            result_kind: if retryable {
                JobErrorKind::Retryable
            } else {
                JobErrorKind::Failed
            },
            code: error.code().to_owned(),
            failed_items: 0,
            retryable,
        }
    }
}

/// Job runner for the delivery default path.
pub struct DeliveryProgressionJobRunner<S> {
    delivery_service: S,
}

impl<S> DeliveryProgressionJobRunner<S> {
    /// Creates a new delivery-progression job runner.
    pub fn new(delivery_service: S) -> Self {
        Self { delivery_service }
    }
}

impl<S> DeliveryProgressionJobRunner<S>
where
    S: DeliveryProgressionUseCase,
{
    /// Runs one delivery-progression batch.
    pub async fn run(
        &self,
        job: RunDeliveryProgressionJob,
        actor: ActorContext,
        meta: JobMetadata,
    ) -> Result<DeliveryProgressionResult, JobError> {
        self.delivery_service
            .progress_batch(job, actor, meta)
            .await
            .map_err(JobError::from)
    }
}

/// Job runner for committed outbox relay.
pub struct OutboxRelayJobRunner<S> {
    relay_service: S,
}

impl<S> OutboxRelayJobRunner<S> {
    /// Creates a new outbox-relay job runner.
    pub fn new(relay_service: S) -> Self {
        Self { relay_service }
    }
}

/// Job runner for due retry plans.
pub struct RetryCycleJobRunner<S> {
    recovery_service: S,
}

impl<S> RetryCycleJobRunner<S> {
    /// Creates a new retry-cycle job runner.
    pub fn new(recovery_service: S) -> Self {
        Self { recovery_service }
    }
}

impl<S> RetryCycleJobRunner<S>
where
    S: RetryCycleUseCase,
{
    /// Runs one retry-cycle batch.
    pub async fn run(
        &self,
        job: RunRetryCycleJob,
        actor: ActorContext,
        meta: JobMetadata,
    ) -> Result<RetryCycleResult, JobError> {
        self.recovery_service
            .run_retry_cycle(job, actor, meta)
            .await
            .map_err(JobError::from)
    }
}

impl<S> OutboxRelayJobRunner<S>
where
    S: OutboxRelayUseCase,
{
    /// Runs one outbox-relay batch.
    pub async fn run(
        &self,
        job: RunOutboxRelayJob,
        actor: ActorContext,
        meta: JobMetadata,
    ) -> Result<OutboxRelayJobResult, JobError> {
        self.relay_service
            .run(job, actor, meta)
            .await
            .map_err(JobError::from)
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::pin;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    use bus_application::{
        AuditTrailRepository, DeliveryProgressionService, DeliveryProgressionServiceDeps,
        OutboxRelayService, OutboxRelayServiceDeps, PublicationAcceptanceService,
        PublicationAcceptanceServiceDeps, RecoveryOrchestrationService,
        RecoveryOrchestrationServiceDeps, RequestRetryUseCase, SourcePortError, TransportPortError,
        UnitOfWork, UnitOfWorkPurpose,
    };
    use bus_contracts::fixtures::{
        BackendFixtureBuilder, DeliveryFixtureBuilder, FeedbackFixtureBuilder,
        OutboxFixtureBuilder, PublicationFixtureBuilder, RecoveryFixtureBuilder, TestRun,
        TestRunBuilder,
    };
    use bus_contracts::metadata::{
        AuditRef, DeliveryStatus, FeedbackStatus, HistoryReason, IdempotencyKey, JobMetadata,
        JobRunId, JobTriggerSource, SubscriberRef, SubscriberScope, Timestamp, TraceId,
    };
    use bus_domain::audit::{AuditAction, BusAuditEntry, SubjectRef};
    use bus_domain::delivery::{DeliveryHistoryEntry, DeliveryRecord};
    use bus_domain::feedback::FeedbackResult;
    use bus_domain::publication::{PublicationMaterial, TransportSemantic};
    use bus_domain::recovery::{FailureMaterial, RetryPlan};
    use bus_infra::{
        DeterministicIdGenerator, FixedClockAdapter, InMemoryAuditTrailRepository,
        InMemoryDeliveryRepository, InMemoryIdempotencyRepository, InMemoryOutboxFactSourceAdapter,
        InMemoryPublicationRepository, InMemoryRecoveryRepository, InMemoryTransportBackendAdapter,
        InMemoryUnitOfWork, SharedMemoryStore, SharedOutboxSource,
    };

    use super::*;

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

    fn scheduled_delivery(run: &TestRun, subscriber_ref: &str) -> DeliveryRecord {
        let backend_capability = BackendFixtureBuilder::new(run.clone()).in_memory_capability();
        let publication_builder = PublicationFixtureBuilder::new(run.clone());
        let material = PublicationMaterial::from_accept_publication_command(
            publication_builder.valid_material(),
            run.actor.clone(),
            run.metadata.clone(),
        )
        .expect("fixture should create publication material");
        let semantic = TransportSemantic::derive(
            material,
            backend_capability,
            SubscriberScope {
                project_id: format!("project_{}", run.run_id),
                topic: format!("workitem.events.{}", run.run_id),
            },
        )
        .expect("fixture should derive transport semantic");

        DeliveryRecord::schedule(
            semantic,
            SubscriberRef::new(subscriber_ref),
            IdempotencyKey::new(format!("idem_job_{}_{}", run.run_id, subscriber_ref)),
        )
        .expect("fixture should schedule delivery")
    }

    type RecoveryJobService = RecoveryOrchestrationService<
        InMemoryDeliveryRepository,
        InMemoryRecoveryRepository,
        InMemoryAuditTrailRepository,
        InMemoryUnitOfWork,
        FixedClockAdapter,
        DeterministicIdGenerator,
        InMemoryTransportBackendAdapter,
    >;

    fn retry_runner(
        run: &TestRun,
    ) -> (
        RetryCycleJobRunner<RecoveryJobService>,
        RecoveryJobService,
        InMemoryDeliveryRepository,
        InMemoryRecoveryRepository,
        InMemoryAuditTrailRepository,
        InMemoryUnitOfWork,
    ) {
        let store = SharedMemoryStore::new();
        let delivery_repository = InMemoryDeliveryRepository::new(store.clone());
        let recovery_repository = InMemoryRecoveryRepository::new(store.clone());
        let audit_repository = InMemoryAuditTrailRepository::new(store.clone());
        let unit_of_work = InMemoryUnitOfWork::new(store.clone());
        let clock = FixedClockAdapter::new(Timestamp::new("2026-05-31T00:05:00Z"));
        let ids = DeterministicIdGenerator::new();
        let backend = InMemoryTransportBackendAdapter::new(
            BackendFixtureBuilder::new(run.clone()).in_memory_capability(),
        );
        let recovery_deps = RecoveryOrchestrationServiceDeps {
            delivery_repository: delivery_repository.clone(),
            recovery_repository: recovery_repository.clone(),
            audit_repository: audit_repository.clone(),
            unit_of_work: unit_of_work.clone(),
            clock,
            id_generator: ids,
            transport_backend: backend,
        };
        let recovery_service =
            RecoveryOrchestrationService::new(RecoveryOrchestrationServiceDeps {
                delivery_repository: recovery_deps.delivery_repository.clone(),
                recovery_repository: recovery_deps.recovery_repository.clone(),
                audit_repository: recovery_deps.audit_repository.clone(),
                unit_of_work: recovery_deps.unit_of_work.clone(),
                clock: recovery_deps.clock.clone(),
                id_generator: recovery_deps.id_generator.clone(),
                transport_backend: recovery_deps.transport_backend.clone(),
            });
        let runner = RetryCycleJobRunner::new(RecoveryOrchestrationService::new(recovery_deps));

        (
            runner,
            recovery_service,
            delivery_repository,
            recovery_repository,
            audit_repository,
            unit_of_work,
        )
    }

    fn failed_delivery_with_material(
        run: &TestRun,
    ) -> (DeliveryRecord, FailureMaterial, BusAuditEntry) {
        let mut delivery = scheduled_delivery(run, "retry_subscriber");
        let capability_ref = BackendFixtureBuilder::new(run.clone()).in_memory_capability();
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
            .expect("dispatching history should append");
        attempt
            .finish(
                bus_contracts::metadata::BackendDeliveryResult::delivered(Some(
                    "backend_delivery_retry".into(),
                )),
                Timestamp::new("2026-05-31T00:00:02Z"),
            )
            .expect("attempt should finish as delivered");
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

        let feedback_builder = FeedbackFixtureBuilder::new(run.clone());
        let fail_command = feedback_builder.fail_command(
            delivery.delivery_id.clone(),
            delivery
                .last_attempt_ref
                .clone()
                .expect("attempt ref should exist")
                .as_str()
                .into(),
        );
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
        let audit_ref = AuditRef::new(format!("audit_retry_failure_{}", run.run_id));
        let material = FailureMaterial::from_feedback(feedback, failed_history, audit_ref.clone())
            .expect("failure material should build");
        let audit = BusAuditEntry::record(
            audit_ref,
            SubjectRef::Delivery(delivery.delivery_id.clone()),
            AuditAction::FeedbackRecorded(FeedbackStatus::Fail),
            run.actor.clone(),
            run.metadata.request.trace_id.clone(),
            Timestamp::new("2026-05-31T00:00:03Z"),
        );

        (delivery, material, audit)
    }

    fn recovery_job_metadata(run: &TestRun) -> JobMetadata {
        JobMetadata {
            job_run_id: JobRunId::new(format!("job_run_{}", run.run_id)),
            trace_ref: TraceId::new(format!("trace-job-{}", run.run_id)),
            trigger_source: JobTriggerSource::Scheduler,
        }
    }

    fn outbox_runner(
        run: &TestRun,
        source: SharedOutboxSource,
    ) -> (
        OutboxRelayJobRunner<
            OutboxRelayService<
                InMemoryOutboxFactSourceAdapter,
                PublicationAcceptanceService<
                    InMemoryPublicationRepository,
                    InMemoryIdempotencyRepository,
                    InMemoryAuditTrailRepository,
                    InMemoryUnitOfWork,
                    FixedClockAdapter,
                    DeterministicIdGenerator,
                >,
            >,
        >,
        InMemoryPublicationRepository,
    ) {
        let store = SharedMemoryStore::new();
        let publication_repository = InMemoryPublicationRepository::new(store.clone());
        let acceptance_service =
            PublicationAcceptanceService::new(PublicationAcceptanceServiceDeps {
                publication_repository: publication_repository.clone(),
                idempotency_repository: InMemoryIdempotencyRepository::new(store.clone()),
                audit_repository: InMemoryAuditTrailRepository::new(store.clone()),
                unit_of_work: InMemoryUnitOfWork::new(store),
                clock: FixedClockAdapter::new(run.metadata.request.requested_at.clone()),
                id_generator: DeterministicIdGenerator::default(),
            });
        let relay_service = OutboxRelayService::new(OutboxRelayServiceDeps {
            outbox_source: InMemoryOutboxFactSourceAdapter::new(source),
            publication_service: acceptance_service,
        });

        (
            OutboxRelayJobRunner::new(relay_service),
            publication_repository,
        )
    }

    fn expected_publication_id(
        run: &TestRun,
        fact: bus_contracts::events::CommittedOutboxFact,
        builder: &OutboxFixtureBuilder,
    ) -> bus_contracts::metadata::PublicationId {
        PublicationMaterial::from_outbox_fact(
            bus_contracts::events::CommittedOutboxFactInput::from_fact(fact),
            run.actor.clone(),
            builder.event_metadata(),
        )
        .expect("fixture should build publication material")
        .publication_id
    }

    #[test]
    fn delivery_progression_job_runner_isolates_success_and_failure_items() {
        let builder = TestRunBuilder::new("job-runner-001");
        let run = builder.build();
        let backend_capability = BackendFixtureBuilder::new(run.clone()).in_memory_capability();
        let delivery_builder = DeliveryFixtureBuilder::new(run.clone());
        let store = SharedMemoryStore::new();
        let delivery_repository = InMemoryDeliveryRepository::new(store.clone());
        let audit_repository = InMemoryAuditTrailRepository::new(store.clone());
        let backend = InMemoryTransportBackendAdapter::new(backend_capability);
        let service = DeliveryProgressionService::new(DeliveryProgressionServiceDeps {
            delivery_repository: delivery_repository.clone(),
            transport_backend: backend.clone(),
            audit_repository,
            unit_of_work: InMemoryUnitOfWork::new(store),
            clock: FixedClockAdapter::new(run.metadata.request.requested_at.clone()),
        });
        let runner = DeliveryProgressionJobRunner::new(service);
        let first = scheduled_delivery(&run, "subscriber_success");
        let first_id = first.delivery_id.clone();
        let second = scheduled_delivery(&run, "subscriber_failure");
        let second_id = second.delivery_id.clone();

        delivery_repository
            .seed_committed(first)
            .expect("first seed should succeed");
        delivery_repository
            .seed_committed(second)
            .expect("second seed should succeed");
        backend.fail_delivery(second_id.clone(), TransportPortError::BackendUnavailable);

        let result = block_on(runner.run(
            delivery_builder.run_delivery_progression_job(),
            run.actor.clone(),
            delivery_builder.run_delivery_progression_metadata(),
        ))
        .expect("job runner should return a partial-success summary");

        assert_eq!(result.scanned, 2);
        assert_eq!(result.dispatched, 1);
        assert_eq!(result.skipped, 0);
        assert_eq!(result.failed(), 1);
        assert_eq!(
            delivery_repository
                .committed(&first_id)
                .expect("first delivery should remain committed")
                .status,
            bus_contracts::metadata::DeliveryStatus::Delivered
        );
        assert_eq!(
            delivery_repository
                .committed(&second_id)
                .expect("second delivery should remain committed")
                .status,
            bus_contracts::metadata::DeliveryStatus::Failed
        );
    }

    #[test]
    fn outbox_relay_job_runner_accepts_committed_batch_and_advances_cursor() {
        let run = TestRunBuilder::new("job-outbox-001").build();
        let builder = OutboxFixtureBuilder::new(run.clone());
        let source = SharedOutboxSource::new();
        let first = builder.committed_fact();
        let second = builder.second_committed_fact();
        let first_publication_id = expected_publication_id(&run, first.clone(), &builder);
        source
            .seed_committed(first)
            .expect("first fact should seed");
        source
            .seed_committed(second)
            .expect("second fact should seed");
        let (runner, publication_repository) = outbox_runner(&run, source);

        let result = block_on(runner.run(
            builder.run_outbox_relay_job(),
            run.actor.clone(),
            builder.run_outbox_relay_metadata(),
        ));

        let result = result.expect("outbox relay should succeed");
        assert_eq!(result.scanned, 2);
        assert_eq!(result.accepted, 2);
        assert_eq!(result.rejected, 0);
        assert_eq!(result.failed(), 0);
        assert_ne!(result.next_cursor, builder.origin_cursor());
        assert_eq!(
            publication_repository
                .committed(&first_publication_id)
                .expect("first publication should be committed")
                .status,
            bus_contracts::metadata::PublicationAcceptanceStatus::Accepted
        );
    }

    #[test]
    fn outbox_relay_job_runner_continues_after_rejected_item() {
        let run = TestRunBuilder::new("job-outbox-002").build();
        let builder = OutboxFixtureBuilder::new(run.clone());
        let source = SharedOutboxSource::new();
        source
            .seed_committed(builder.committed_fact_missing_core_event_ref())
            .expect("rejected fact should seed");
        let accepted_fact = builder.second_committed_fact();
        let accepted_publication_id =
            expected_publication_id(&run, accepted_fact.clone(), &builder);
        source
            .seed_committed(accepted_fact)
            .expect("accepted fact should seed");
        let (runner, publication_repository) = outbox_runner(&run, source);

        let result = block_on(runner.run(
            builder.run_outbox_relay_job(),
            run.actor.clone(),
            builder.run_outbox_relay_metadata(),
        ))
        .expect("partial rejected batch should still return a summary");

        assert_eq!(result.scanned, 2);
        assert_eq!(result.accepted, 1);
        assert_eq!(result.rejected, 1);
        assert_eq!(result.failed(), 0);
        assert_eq!(
            publication_repository
                .committed(&accepted_publication_id)
                .expect("accepted publication should still commit")
                .status,
            bus_contracts::metadata::PublicationAcceptanceStatus::Accepted
        );
    }

    #[test]
    fn outbox_relay_job_runner_maps_source_unavailable_to_retryable_error() {
        let run = TestRunBuilder::new("job-outbox-003").build();
        let builder = OutboxFixtureBuilder::new(run.clone());
        let source = SharedOutboxSource::new();
        source.fail_next_poll(SourcePortError::Unavailable);
        let (runner, _) = outbox_runner(&run, source);

        let error = block_on(runner.run(
            builder.run_outbox_relay_job(),
            run.actor,
            builder.run_outbox_relay_metadata(),
        ))
        .expect_err("source unavailability should fail the whole job");

        assert_eq!(error.result_kind, JobErrorKind::Retryable);
        assert_eq!(error.code, "dependency.outbox_source_unavailable");
    }

    #[test]
    fn outbox_relay_job_runner_replays_after_ack_failure_without_duplicate_truth() {
        let run = TestRunBuilder::new("job-outbox-004").build();
        let builder = OutboxFixtureBuilder::new(run.clone());
        let source = SharedOutboxSource::new();
        let fact = builder.committed_fact();
        let publication_id = expected_publication_id(&run, fact.clone(), &builder);
        source
            .seed_committed(fact.clone())
            .expect("fact should seed");
        source.fail_next_ack(SourcePortError::AckFailed);
        let (runner, publication_repository) = outbox_runner(&run, source.clone());

        let first = block_on(runner.run(
            builder.run_outbox_relay_job(),
            run.actor.clone(),
            builder.run_outbox_relay_metadata(),
        ))
        .expect("ack failure should stay in the batch summary");
        let second = block_on(runner.run(
            builder.run_outbox_relay_job(),
            run.actor,
            builder.run_outbox_relay_metadata(),
        ))
        .expect("replayed fact should deduplicate and ack successfully");

        assert_eq!(first.accepted, 0);
        assert_eq!(first.rejected, 0);
        assert_eq!(first.failed(), 1);
        assert_eq!(first.next_cursor, builder.origin_cursor());
        assert_eq!(second.accepted, 1);
        assert_eq!(second.failed(), 0);
        assert_eq!(
            publication_repository
                .committed(&publication_id)
                .expect("publication should remain committed exactly once")
                .status,
            bus_contracts::metadata::PublicationAcceptanceStatus::Accepted
        );
        assert_eq!(
            source.acknowledged_marker(&fact.committed_fact_ref),
            Some(bus_contracts::metadata::ConsumerMarker::bus())
        );
    }

    #[test]
    fn retry_cycle_job_runner_dispatches_due_retry_plans() {
        let run = TestRunBuilder::new("job-retry-001").build();
        let builder = RecoveryFixtureBuilder::new(run.clone());
        let (
            runner,
            recovery_service,
            delivery_repository,
            recovery_repository,
            audit_repository,
            unit_of_work,
        ) = retry_runner(&run);
        let (delivery, material, audit) = failed_delivery_with_material(&run);
        delivery_repository
            .seed_committed(delivery.clone())
            .expect("failed delivery should seed");
        recovery_repository
            .seed_failure_material(material.clone())
            .expect("failure material should seed");
        let uow = block_on(
            unit_of_work.begin(UnitOfWorkPurpose::RecordDeliveryFeedback, run.actor.clone()),
        )
        .expect("audit seed should begin");
        block_on(audit_repository.append(audit, &uow)).expect("audit seed should append");
        block_on(unit_of_work.commit(uow)).expect("audit seed should commit");
        let mut retry_command = builder.request_retry_command(delivery.delivery_id.clone());
        retry_command.failure_material_ref = material.failure_material_id.clone().into();
        let retry_result = block_on(recovery_service.request_retry(
            retry_command,
            run.actor.clone(),
            run.metadata.clone(),
        ))
        .expect("retry plan should commit");

        let result = block_on(runner.run(
            builder.run_retry_cycle_job(),
            run.actor.clone(),
            recovery_job_metadata(&run),
        ))
        .expect("retry cycle should succeed");

        assert_eq!(result.scanned, 1);
        assert_eq!(result.retried, 1);
        assert_eq!(result.exhausted, 0);
        let committed_delivery = delivery_repository
            .committed(&delivery.delivery_id)
            .expect("delivery should remain committed");
        assert_eq!(committed_delivery.status, DeliveryStatus::Delivered);
        let committed_plan = recovery_repository
            .committed_retry_plan(&retry_result.retry_plan_id)
            .expect("retry plan should update");
        assert_eq!(committed_plan.remaining_attempts.get(), 2);
    }

    #[test]
    fn retry_cycle_job_runner_marks_zero_budget_plan_as_exhausted() {
        let run = TestRunBuilder::new("job-retry-002").build();
        let builder = RecoveryFixtureBuilder::new(run.clone());
        let (
            runner,
            _recovery_service,
            delivery_repository,
            recovery_repository,
            audit_repository,
            unit_of_work,
        ) = retry_runner(&run);
        let (delivery, material, audit) = failed_delivery_with_material(&run);
        delivery_repository
            .seed_committed(delivery.clone())
            .expect("failed delivery should seed");
        recovery_repository
            .seed_failure_material(material.clone())
            .expect("failure material should seed");
        let uow = block_on(
            unit_of_work.begin(UnitOfWorkPurpose::RecordDeliveryFeedback, run.actor.clone()),
        )
        .expect("audit seed should begin");
        block_on(audit_repository.append(audit, &uow)).expect("audit seed should append");
        block_on(unit_of_work.commit(uow)).expect("audit seed should commit");
        let mut retry_plan = RetryPlan::create(
            delivery.clone(),
            material.failure_reason.clone(),
            builder.retry_policy_ref(),
            bus_contracts::metadata::AttemptLimit::new(1),
            Timestamp::new("2026-05-31T00:04:00Z"),
        )
        .expect("retry plan should build");
        retry_plan
            .mark_attempted(
                "attempt_retry_prior".into(),
                bus_contracts::metadata::BackendDeliveryResult::failed(None),
            )
            .expect("retry plan should consume its only attempt");
        let retry_plan_id = retry_plan.retry_plan_id.clone();
        recovery_repository
            .seed_retry_plan(retry_plan)
            .expect("retry plan should seed");

        let result = block_on(runner.run(
            builder.run_retry_cycle_job(),
            run.actor.clone(),
            recovery_job_metadata(&run),
        ))
        .expect("retry cycle should succeed");

        assert_eq!(result.scanned, 1);
        assert_eq!(result.retried, 0);
        assert_eq!(result.exhausted, 1);
        let committed_plan = recovery_repository
            .committed_retry_plan(&retry_plan_id)
            .expect("retry plan should remain committed");
        assert_eq!(
            committed_plan.status,
            bus_contracts::metadata::RetryPlanStatus::Exhausted
        );
        assert_eq!(
            delivery_repository
                .committed(&delivery.delivery_id)
                .expect("delivery should remain committed")
                .status,
            DeliveryStatus::Failed
        );
    }
}
