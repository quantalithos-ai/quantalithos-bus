//! Inbound consumer adapters for the bus workspace.

use bus_application::{
    ApplicationError, BackendSignalUseCase, OutboxPublicationAcceptanceUseCase,
    TimeoutSignalUseCase,
};
use bus_contracts::events::{
    BackendDeliverySignalInput, CommittedOutboxFactInput, DeliveryTimeoutSignalInput,
};
use bus_contracts::metadata::{ActorContext, EventMetadata};
use bus_contracts::receipts::{BackendSignalResult, OutboxRelayResult, TimeoutRecordResult};

/// Stable consumer-level error categories.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConsumerErrorKind {
    /// The item was rejected by domain or boundary rules.
    Rejected,
    /// The item may be retried safely.
    Retryable,
    /// The item failed for an internal or unresolved reason.
    Failed,
}

/// A consumer error mapped from the application layer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConsumerError {
    /// The resulting consumer error kind.
    pub result_kind: ConsumerErrorKind,
    /// The stable external error code.
    pub code: String,
    /// Whether the item may be retried automatically.
    pub retryable: bool,
}

impl From<ApplicationError> for ConsumerError {
    fn from(error: ApplicationError) -> Self {
        let retryable = error.retryable();
        let result_kind = if retryable {
            ConsumerErrorKind::Retryable
        } else if error.is_rejected_item() {
            ConsumerErrorKind::Rejected
        } else {
            ConsumerErrorKind::Failed
        };

        Self {
            result_kind,
            code: error.code().to_owned(),
            retryable,
        }
    }
}

/// Consumer for committed upstream outbox facts.
pub struct OutboxRelayConsumer<S> {
    publication_service: S,
}

impl<S> OutboxRelayConsumer<S> {
    /// Creates a new outbox-relay consumer.
    pub fn new(publication_service: S) -> Self {
        Self {
            publication_service,
        }
    }
}

impl<S> OutboxRelayConsumer<S>
where
    S: OutboxPublicationAcceptanceUseCase,
{
    /// Consumes one committed outbox fact.
    pub async fn consume(
        &self,
        input: CommittedOutboxFactInput,
        actor: ActorContext,
        meta: EventMetadata,
    ) -> Result<OutboxRelayResult, ConsumerError> {
        self.publication_service
            .accept_from_outbox_fact(input, actor, meta)
            .await
            .map_err(ConsumerError::from)
    }
}

/// Consumer for normalized backend delivery signals.
pub struct BackendSignalConsumer<S> {
    feedback_service: S,
}

impl<S> BackendSignalConsumer<S> {
    /// Creates a new backend-signal consumer.
    pub fn new(feedback_service: S) -> Self {
        Self { feedback_service }
    }
}

impl<S> BackendSignalConsumer<S>
where
    S: BackendSignalUseCase,
{
    /// Consumes one backend delivery signal.
    pub async fn consume(
        &self,
        input: BackendDeliverySignalInput,
        actor: ActorContext,
        meta: EventMetadata,
    ) -> Result<BackendSignalResult, ConsumerError> {
        self.feedback_service
            .record_backend_signal(input, actor, meta)
            .await
            .map_err(ConsumerError::from)
    }
}

/// Consumer for delivery timeout signals.
pub struct TimeoutSignalConsumer<S> {
    feedback_service: S,
}

impl<S> TimeoutSignalConsumer<S> {
    /// Creates a new timeout-signal consumer.
    pub fn new(feedback_service: S) -> Self {
        Self { feedback_service }
    }
}

impl<S> TimeoutSignalConsumer<S>
where
    S: TimeoutSignalUseCase,
{
    /// Consumes one delivery timeout signal.
    pub async fn consume(
        &self,
        input: DeliveryTimeoutSignalInput,
        actor: ActorContext,
        meta: EventMetadata,
    ) -> Result<TimeoutRecordResult, ConsumerError> {
        self.feedback_service
            .record_timeout(input, actor, meta)
            .await
            .map_err(ConsumerError::from)
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::pin;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    use bus_application::{
        FeedbackRecordingService, FeedbackRecordingServiceDeps, PublicationAcceptanceService,
        PublicationAcceptanceServiceDeps,
    };
    use bus_contracts::fixtures::{
        BackendFixtureBuilder, FeedbackFixtureBuilder, OutboxFixtureBuilder,
        PublicationFixtureBuilder, TestRun, TestRunBuilder,
    };
    use bus_contracts::metadata::{
        DeliveryStatus, IdempotencyKey, PublicationAcceptanceStatus, SourceSystem, SubscriberRef,
        SubscriberScope, Timestamp,
    };
    use bus_contracts::receipts::{BackendSignalStatus, OutboxRelayStatus, TimeoutRecordStatus};
    use bus_domain::delivery::{DeliveryHistoryEntry, DeliveryRecord};
    use bus_domain::publication::{PublicationMaterial, TransportSemantic};
    use bus_infra::{
        DeterministicIdGenerator, FixedClockAdapter, InMemoryAuditTrailRepository,
        InMemoryDeliveryRepository, InMemoryFeedbackRepository, InMemoryIdempotencyRepository,
        InMemoryPublicationRepository, InMemoryTransportBackendAdapter, InMemoryUnitOfWork,
        SharedMemoryStore,
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

    type TestService = PublicationAcceptanceService<
        InMemoryPublicationRepository,
        InMemoryIdempotencyRepository,
        InMemoryAuditTrailRepository,
        InMemoryUnitOfWork,
        FixedClockAdapter,
        DeterministicIdGenerator,
    >;

    type FeedbackService = FeedbackRecordingService<
        InMemoryFeedbackRepository,
        InMemoryDeliveryRepository,
        InMemoryIdempotencyRepository,
        InMemoryAuditTrailRepository,
        InMemoryUnitOfWork,
        FixedClockAdapter,
        DeterministicIdGenerator,
        InMemoryTransportBackendAdapter,
    >;

    struct Harness {
        consumer: OutboxRelayConsumer<TestService>,
        publication_repository: InMemoryPublicationRepository,
    }

    struct FeedbackHarness {
        backend_consumer: BackendSignalConsumer<FeedbackService>,
        timeout_consumer: TimeoutSignalConsumer<FeedbackService>,
        delivery_repository: InMemoryDeliveryRepository,
        feedback_repository: InMemoryFeedbackRepository,
        audit_repository: InMemoryAuditTrailRepository,
    }

    fn harness(
        run_id: &str,
    ) -> (
        Harness,
        bus_contracts::fixtures::TestRun,
        OutboxFixtureBuilder,
    ) {
        let run = TestRunBuilder::new(run_id).build();
        let builder = OutboxFixtureBuilder::new(run.clone());
        let store = SharedMemoryStore::new();
        let publication_repository = InMemoryPublicationRepository::new(store.clone());
        let service = PublicationAcceptanceService::new(PublicationAcceptanceServiceDeps {
            publication_repository: publication_repository.clone(),
            idempotency_repository: InMemoryIdempotencyRepository::new(store.clone()),
            audit_repository: InMemoryAuditTrailRepository::new(store.clone()),
            unit_of_work: InMemoryUnitOfWork::new(store),
            clock: FixedClockAdapter::new(run.metadata.request.requested_at.clone()),
            id_generator: DeterministicIdGenerator::default(),
        });

        (
            Harness {
                consumer: OutboxRelayConsumer::new(service),
                publication_repository,
            },
            run,
            builder,
        )
    }

    fn feedback_harness(
        run_id: &str,
    ) -> (
        FeedbackHarness,
        TestRun,
        FeedbackFixtureBuilder,
        BackendFixtureBuilder,
    ) {
        let run = TestRunBuilder::new(run_id).build();
        let feedback_builder = FeedbackFixtureBuilder::new(run.clone());
        let backend_builder = BackendFixtureBuilder::new(run.clone());
        let store = SharedMemoryStore::new();
        let delivery_repository = InMemoryDeliveryRepository::new(store.clone());
        let feedback_repository = InMemoryFeedbackRepository::new(store.clone());
        let idempotency_repository = InMemoryIdempotencyRepository::new(store.clone());
        let audit_repository = InMemoryAuditTrailRepository::new(store.clone());
        let backend_adapter =
            InMemoryTransportBackendAdapter::new(backend_builder.in_memory_capability());
        let backend_service = FeedbackRecordingService::new(FeedbackRecordingServiceDeps {
            feedback_repository: feedback_repository.clone(),
            delivery_repository: delivery_repository.clone(),
            idempotency_repository: idempotency_repository.clone(),
            audit_repository: audit_repository.clone(),
            unit_of_work: InMemoryUnitOfWork::new(store.clone()),
            clock: FixedClockAdapter::new(Timestamp::new("2026-05-30T00:00:10Z")),
            id_generator: DeterministicIdGenerator::new(),
            transport_backend: backend_adapter.clone(),
        });
        let timeout_service = FeedbackRecordingService::new(FeedbackRecordingServiceDeps {
            feedback_repository: feedback_repository.clone(),
            delivery_repository: delivery_repository.clone(),
            idempotency_repository,
            audit_repository: audit_repository.clone(),
            unit_of_work: InMemoryUnitOfWork::new(store),
            clock: FixedClockAdapter::new(Timestamp::new("2026-05-30T00:00:10Z")),
            id_generator: DeterministicIdGenerator::new(),
            transport_backend: backend_adapter,
        });

        (
            FeedbackHarness {
                backend_consumer: BackendSignalConsumer::new(backend_service),
                timeout_consumer: TimeoutSignalConsumer::new(timeout_service),
                delivery_repository,
                feedback_repository,
                audit_repository,
            },
            run,
            feedback_builder,
            backend_builder,
        )
    }

    fn seed_dispatching_delivery(
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
        delivery
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
        repository
            .seed_committed(delivery.clone())
            .expect("delivery should seed");

        delivery
    }

    #[test]
    fn outbox_consumer_accepts_committed_fact() {
        let (harness, run, builder) = harness("obx-consumer-001");

        let result = block_on(harness.consumer.consume(
            builder.committed_fact_input(),
            run.actor.clone(),
            builder.event_metadata(),
        ))
        .expect("consumer should accept the committed fact");

        assert_eq!(result.relay_status, OutboxRelayStatus::Accepted);
        assert_eq!(
            harness
                .publication_repository
                .committed(&result.publication_id)
                .expect("accepted publication should be committed")
                .status,
            PublicationAcceptanceStatus::Accepted
        );
    }

    #[test]
    fn outbox_consumer_returns_duplicate_for_replayed_fact() {
        let (harness, run, builder) = harness("obx-consumer-002");
        let input = builder.committed_fact_input();

        let first = block_on(harness.consumer.consume(
            input.clone(),
            run.actor.clone(),
            builder.event_metadata(),
        ))
        .expect("first consume should succeed");
        let second = block_on(harness.consumer.consume(
            input,
            run.actor.clone(),
            builder.event_metadata(),
        ))
        .expect("duplicate consume should return existing result");

        assert_eq!(first.publication_id, second.publication_id);
        assert_eq!(second.relay_status, OutboxRelayStatus::Duplicate);
    }

    #[test]
    fn outbox_consumer_rejects_missing_core_event_ref() {
        let (harness, run, builder) = harness("obx-consumer-003");
        let mut input = builder.committed_fact_input();
        input.core_event_ref = bus_contracts::metadata::CoreEventRef::new("");
        input.source_system = SourceSystem::new("l0-core");

        let error = block_on(
            harness
                .consumer
                .consume(input, run.actor, builder.event_metadata()),
        )
        .expect_err("missing core_event_ref should be rejected");

        assert_eq!(error.result_kind, ConsumerErrorKind::Rejected);
        assert_eq!(error.code, "validation.core_event_ref_missing");
    }

    #[test]
    fn backend_signal_consumer_records_delivered_signal() {
        let (harness, run, builder, backend_builder) = feedback_harness("sig-consumer-001");
        let delivery = seed_dispatching_delivery(&harness.delivery_repository, &run);
        let input = builder.delivered_backend_signal(
            delivery.delivery_id.clone(),
            delivery
                .last_attempt_ref
                .clone()
                .expect("attempt ref should exist")
                .as_str()
                .into(),
            backend_builder.in_memory_capability(),
        );

        let result = block_on(harness.backend_consumer.consume(
            input,
            run.actor.clone(),
            builder.event_metadata(),
        ))
        .expect("backend signal should commit");

        assert_eq!(result.signal_status, BackendSignalStatus::Recorded);
        assert_eq!(
            harness
                .feedback_repository
                .committed(
                    result
                        .feedback_id
                        .as_ref()
                        .expect("feedback id should be returned"),
                )
                .expect("feedback should be committed")
                .status,
            bus_contracts::metadata::FeedbackStatus::Ack
        );
    }

    #[test]
    fn backend_signal_consumer_returns_duplicate_for_replayed_signal() {
        let (harness, run, builder, backend_builder) = feedback_harness("sig-consumer-002");
        let delivery = seed_dispatching_delivery(&harness.delivery_repository, &run);
        let input = builder.delivered_backend_signal(
            delivery.delivery_id.clone(),
            delivery
                .last_attempt_ref
                .clone()
                .expect("attempt ref should exist")
                .as_str()
                .into(),
            backend_builder.in_memory_capability(),
        );

        block_on(harness.backend_consumer.consume(
            input.clone(),
            run.actor.clone(),
            builder.event_metadata(),
        ))
        .expect("first signal should commit");
        let duplicate = block_on(harness.backend_consumer.consume(
            input,
            run.actor.clone(),
            builder.event_metadata(),
        ))
        .expect("duplicate signal should reuse committed result");

        assert_eq!(duplicate.signal_status, BackendSignalStatus::Duplicate);
        assert_eq!(harness.feedback_repository.committed_all().len(), 1);
    }

    #[test]
    fn backend_signal_consumer_ignores_unknown_delivery_without_feedback_truth() {
        let (harness, run, builder, backend_builder) = feedback_harness("sig-consumer-003");
        let input = builder.delivered_backend_signal(
            "delivery_missing".into(),
            "attempt_missing".into(),
            backend_builder.in_memory_capability(),
        );

        let result = block_on(harness.backend_consumer.consume(
            input,
            run.actor.clone(),
            builder.event_metadata(),
        ))
        .expect("unknown delivery should be ignored safely");

        assert_eq!(result.signal_status, BackendSignalStatus::Ignored);
        assert!(harness.feedback_repository.committed_all().is_empty());
        assert_eq!(harness.audit_repository.committed_entries().len(), 1);
    }

    #[test]
    fn backend_signal_consumer_rejects_private_body_like_result_reference() {
        let (harness, run, builder, backend_builder) = feedback_harness("sig-consumer-004");
        let delivery = seed_dispatching_delivery(&harness.delivery_repository, &run);
        let mut input = builder.delivered_backend_signal(
            delivery.delivery_id.clone(),
            delivery
                .last_attempt_ref
                .clone()
                .expect("attempt ref should exist")
                .as_str()
                .into(),
            backend_builder.in_memory_capability(),
        );
        input.backend_result_ref = "{\"private\":\"body\"}".into();

        let error = block_on(harness.backend_consumer.consume(
            input,
            run.actor.clone(),
            builder.event_metadata(),
        ))
        .expect_err("private-body like backend result references should be rejected");

        assert_eq!(error.result_kind, ConsumerErrorKind::Rejected);
        assert_eq!(error.code, "boundary.backend_private_field_rejected");
    }

    #[test]
    fn timeout_signal_consumer_records_timeout_feedback_and_duplicate() {
        let (harness, run, builder, _) = feedback_harness("sig-consumer-005");
        let delivery = seed_dispatching_delivery(&harness.delivery_repository, &run);
        let input = builder.timeout_signal(
            delivery.delivery_id.clone(),
            delivery
                .last_attempt_ref
                .clone()
                .expect("attempt ref should exist")
                .as_str()
                .into(),
        );

        let recorded = block_on(harness.timeout_consumer.consume(
            input.clone(),
            run.actor.clone(),
            builder.event_metadata(),
        ))
        .expect("timeout signal should record");
        let duplicate = block_on(harness.timeout_consumer.consume(
            input,
            run.actor.clone(),
            builder.event_metadata(),
        ))
        .expect("duplicate timeout should reuse committed result");

        assert_eq!(
            recorded.feedback_status,
            TimeoutRecordStatus::TimeoutRecorded
        );
        assert_eq!(duplicate.feedback_status, TimeoutRecordStatus::Duplicate);
        assert_eq!(harness.feedback_repository.committed_all().len(), 1);
        assert_eq!(
            harness
                .delivery_repository
                .committed(&delivery.delivery_id)
                .expect("delivery should be committed")
                .status,
            DeliveryStatus::Failed
        );
    }
}
