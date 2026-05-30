//! Inbound consumer adapters for the bus workspace.

use bus_application::{ApplicationError, OutboxPublicationAcceptanceUseCase};
use bus_contracts::events::CommittedOutboxFactInput;
use bus_contracts::metadata::{ActorContext, EventMetadata};
use bus_contracts::receipts::OutboxRelayResult;

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

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::pin;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    use bus_application::{PublicationAcceptanceService, PublicationAcceptanceServiceDeps};
    use bus_contracts::fixtures::{OutboxFixtureBuilder, TestRunBuilder};
    use bus_contracts::metadata::{PublicationAcceptanceStatus, SourceSystem};
    use bus_contracts::receipts::OutboxRelayStatus;
    use bus_infra::{
        DeterministicIdGenerator, FixedClockAdapter, InMemoryAuditTrailRepository,
        InMemoryIdempotencyRepository, InMemoryPublicationRepository, InMemoryUnitOfWork,
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

    struct Harness {
        consumer: OutboxRelayConsumer<TestService>,
        publication_repository: InMemoryPublicationRepository,
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
}
