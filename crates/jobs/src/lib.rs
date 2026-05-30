//! Operations job runners for the bus workspace.

use bus_application::{ApplicationError, DeliveryProgressionUseCase};
use bus_contracts::jobs::{DeliveryProgressionResult, RunDeliveryProgressionJob};
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

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::pin;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    use bus_application::{
        DeliveryProgressionService, DeliveryProgressionServiceDeps, TransportPortError,
    };
    use bus_contracts::fixtures::{
        BackendFixtureBuilder, DeliveryFixtureBuilder, PublicationFixtureBuilder, TestRun,
        TestRunBuilder,
    };
    use bus_contracts::metadata::{IdempotencyKey, SubscriberRef, SubscriberScope};
    use bus_domain::delivery::DeliveryRecord;
    use bus_domain::publication::{PublicationMaterial, TransportSemantic};
    use bus_infra::{
        FixedClockAdapter, InMemoryAuditTrailRepository, InMemoryDeliveryRepository,
        InMemoryTransportBackendAdapter, InMemoryUnitOfWork, SharedMemoryStore,
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
}
