//! Minimal command API for publication acceptance.

use bus_application::{ApplicationError, ProtocolErrorCategory, PublicationAcceptanceUseCase};
use bus_contracts::commands::AcceptPublicationCommand;
use bus_contracts::metadata::{ActorContext, CommandMetadata, RequestId, TraceContextRef};
use bus_contracts::receipts::PublicationAcceptanceResult;

/// A stable API error envelope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApiError {
    /// HTTP status code equivalent.
    pub status_code: u16,
    /// Stable protocol error code.
    pub code: String,
    /// User-facing message.
    pub message: String,
    /// Request identifier.
    pub request_id: RequestId,
    /// Trace identifier.
    pub trace_id: TraceContextRef,
    /// Whether the request may be retried automatically.
    pub retryable: bool,
    /// Optional auditable details reference.
    pub details_ref: Option<String>,
}

impl ApiError {
    /// Maps an application error into the API envelope.
    pub fn from_application(error: ApplicationError, meta: &CommandMetadata) -> Self {
        let status_code = match error.category() {
            ProtocolErrorCategory::Validation => 400,
            ProtocolErrorCategory::NotFound => 404,
            ProtocolErrorCategory::Conflict => 409,
            ProtocolErrorCategory::BoundaryViolation => 422,
            ProtocolErrorCategory::Dependency => 503,
            ProtocolErrorCategory::Internal => 500,
        };

        Self {
            status_code,
            code: error.code().to_owned(),
            message: error.message().to_owned(),
            request_id: meta.request.request_id.clone(),
            trace_id: meta.request.trace_id.clone(),
            retryable: error.retryable(),
            details_ref: error.details_ref().map(ToOwned::to_owned),
        }
    }
}

/// Minimal command API surface for the publication write path.
pub struct BusCommandApi<U> {
    publication_acceptance: U,
}

impl<U> BusCommandApi<U> {
    /// Creates a new command API wrapper.
    pub fn new(publication_acceptance: U) -> Self {
        Self {
            publication_acceptance,
        }
    }
}

impl<U> BusCommandApi<U>
where
    U: PublicationAcceptanceUseCase,
{
    /// Accepts publication material into the bus write path.
    pub async fn accept_publication(
        &self,
        command: AcceptPublicationCommand,
        actor: ActorContext,
        meta: CommandMetadata,
    ) -> Result<PublicationAcceptanceResult, ApiError> {
        self.publication_acceptance
            .accept(command, actor, meta.clone())
            .await
            .map_err(|error| ApiError::from_application(error, &meta))
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::pin;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    use bus_application::{
        PublicationAcceptanceService, PublicationAcceptanceServiceDeps, RepositoryError,
    };
    use bus_contracts::fixtures::{PublicationFixtureBuilder, TestRun, TestRunBuilder};
    use bus_contracts::metadata::{CoreEventRef, PayloadRef, PublicationAcceptanceStatus};
    use bus_domain::audit::AuditAction;
    use bus_domain::idempotency::IdempotencyScope;
    use bus_domain::publication::{PublicationMaterial, PublicationRejectReason};
    use bus_infra::{
        DeterministicIdGenerator, FixedClockAdapter, InMemoryAuditTrailRepository,
        InMemoryIdempotencyRepository, InMemoryPublicationRepository, InMemoryUnitOfWork,
        SharedMemoryStore,
    };

    use super::BusCommandApi;

    type PublicationService = PublicationAcceptanceService<
        InMemoryPublicationRepository,
        InMemoryIdempotencyRepository,
        InMemoryAuditTrailRepository,
        InMemoryUnitOfWork,
        FixedClockAdapter,
        DeterministicIdGenerator,
    >;

    struct Harness {
        api: BusCommandApi<PublicationService>,
        publication_repository: InMemoryPublicationRepository,
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
        let publication_repository = InMemoryPublicationRepository::new(store.clone());
        let idempotency_repository = InMemoryIdempotencyRepository::new(store.clone());
        let audit_repository = InMemoryAuditTrailRepository::new(store.clone());
        let service = PublicationAcceptanceService::new(PublicationAcceptanceServiceDeps {
            publication_repository: publication_repository.clone(),
            idempotency_repository: idempotency_repository.clone(),
            audit_repository: audit_repository.clone(),
            unit_of_work: InMemoryUnitOfWork::new(store),
            clock: FixedClockAdapter::new(run.metadata.request.requested_at.clone()),
            id_generator: DeterministicIdGenerator::new(),
        });

        Harness {
            api: BusCommandApi::new(service),
            publication_repository,
            idempotency_repository,
            audit_repository,
        }
    }

    #[test]
    fn accept_publication_commits_truth_audit_and_idempotency_anchor() {
        let run = TestRunBuilder::new("api-pub-001").build();
        let builder = PublicationFixtureBuilder::new(run.clone());
        let command = builder.valid_material();
        let harness = build_harness(&run);

        let result = block_on(harness.api.accept_publication(
            command.clone(),
            run.actor.clone(),
            run.metadata.clone(),
        ))
        .expect("publication should be accepted");

        assert_eq!(
            result.acceptance_status,
            PublicationAcceptanceStatus::Accepted
        );

        let committed = harness
            .publication_repository
            .committed(&result.publication_id)
            .expect("accepted publication should be committed");
        assert_eq!(committed.status, PublicationAcceptanceStatus::Accepted);

        let scope = IdempotencyScope::for_accept_publication_command(&command);
        let key = run
            .metadata
            .request
            .idempotency_key
            .as_ref()
            .expect("fixture idempotency key");
        let anchor = harness
            .idempotency_repository
            .committed_anchor(&scope, key)
            .expect("accepted path should bind anchor");
        assert_eq!(
            anchor.bound_record_ref,
            bus_domain::idempotency::RecordRef::Publication(result.publication_id.clone())
        );

        let audits = harness.audit_repository.committed_entries();
        assert_eq!(audits.len(), 1);
        assert_eq!(audits[0].action, AuditAction::PublicationAccepted);
    }

    #[test]
    fn accept_publication_boundary_violation_returns_422_and_commits_rejection() {
        let run = TestRunBuilder::new("api-pub-002").build();
        let builder = PublicationFixtureBuilder::new(run.clone());
        let mut command = builder.valid_material();
        command.payload_ref = PayloadRef::new("{\"payload\":\"secret\"}");
        let material = PublicationMaterial::from_accept_publication_command(
            command.clone(),
            run.actor.clone(),
            run.metadata.clone(),
        )
        .expect("body-like payload still forms material");
        let harness = build_harness(&run);

        let error = block_on(harness.api.accept_publication(
            command.clone(),
            run.actor.clone(),
            run.metadata.clone(),
        ))
        .expect_err("payload body must be rejected");

        assert_eq!(error.status_code, 422);
        assert_eq!(error.code, "boundary.payload_body_rejected");

        let committed = harness
            .publication_repository
            .committed(&material.publication_id)
            .expect("rejected publication should be committed");
        assert_eq!(committed.status, PublicationAcceptanceStatus::Rejected);
        assert_eq!(
            committed.reject_reason,
            Some(PublicationRejectReason::PayloadBoundaryViolation)
        );

        let audits = harness.audit_repository.committed_entries();
        assert_eq!(audits.len(), 1);
        assert_eq!(
            audits[0].action,
            AuditAction::PublicationRejected(PublicationRejectReason::PayloadBoundaryViolation)
        );
        assert!(!format!("{committed:?}").contains("{\"payload\":\"secret\"}"));
        assert!(!format!("{audits:?}").contains("{\"payload\":\"secret\"}"));
    }

    #[test]
    fn accept_publication_missing_core_event_ref_returns_validation_without_truth() {
        let run = TestRunBuilder::new("api-pub-003").build();
        let builder = PublicationFixtureBuilder::new(run.clone());
        let mut command = builder.valid_material();
        command.core_event_ref = CoreEventRef::new("");
        let harness = build_harness(&run);

        let error = block_on(harness.api.accept_publication(
            command.clone(),
            run.actor.clone(),
            run.metadata.clone(),
        ))
        .expect_err("missing core_event_ref must fail validation");

        assert_eq!(error.status_code, 400);
        assert_eq!(error.code, "validation.publication_material");
        assert!(harness.audit_repository.committed_entries().is_empty());

        let key = run
            .metadata
            .request
            .idempotency_key
            .as_ref()
            .expect("fixture idempotency key");
        let scope = IdempotencyScope::for_accept_publication_command(&command);
        assert!(
            harness
                .idempotency_repository
                .committed_anchor(&scope, key)
                .is_none()
        );
    }

    #[test]
    fn accept_publication_same_key_same_digest_returns_existing_result() {
        let run = TestRunBuilder::new("api-pub-004").build();
        let builder = PublicationFixtureBuilder::new(run.clone());
        let command = builder.valid_material();
        let harness = build_harness(&run);

        let first = block_on(harness.api.accept_publication(
            command.clone(),
            run.actor.clone(),
            run.metadata.clone(),
        ))
        .expect("first request should be accepted");
        let second = block_on(harness.api.accept_publication(
            command.clone(),
            run.actor.clone(),
            run.metadata.clone(),
        ))
        .expect("same digest should return existing result");

        assert_eq!(first, second);
        assert_eq!(harness.audit_repository.committed_entries().len(), 1);
    }

    #[test]
    fn accept_publication_same_key_different_digest_returns_conflict() {
        let run = TestRunBuilder::new("api-pub-005").build();
        let builder = PublicationFixtureBuilder::new(run.clone());
        let command = builder.valid_material();
        let harness = build_harness(&run);

        block_on(harness.api.accept_publication(
            command.clone(),
            run.actor.clone(),
            run.metadata.clone(),
        ))
        .expect("first request should be accepted");

        let mut conflicting_command = command.clone();
        conflicting_command.payload_ref = PayloadRef::new("artifact_ref_api-pub-005_conflict");

        let error = block_on(harness.api.accept_publication(
            conflicting_command.clone(),
            run.actor.clone(),
            run.metadata.clone(),
        ))
        .expect_err("same key with different digest must conflict");

        assert_eq!(error.status_code, 409);
        assert_eq!(error.code, "conflict.idempotency_request_mismatch");
        assert_eq!(
            harness.idempotency_repository.committed_conflicts().len(),
            1
        );
        assert_eq!(harness.audit_repository.committed_entries().len(), 2);

        let scope = IdempotencyScope::for_accept_publication_command(&command);
        let key = run
            .metadata
            .request
            .idempotency_key
            .as_ref()
            .expect("fixture idempotency key");
        let anchor = harness
            .idempotency_repository
            .committed_anchor(&scope, key)
            .expect("accepted anchor should remain committed");
        let existing_result = block_on(harness.api.accept_publication(
            command.clone(),
            run.actor.clone(),
            run.metadata.clone(),
        ))
        .expect("original request should still return existing result");
        assert_eq!(
            anchor.bound_record_ref,
            bus_domain::idempotency::RecordRef::Publication(existing_result.publication_id)
        );
    }

    #[test]
    fn accept_publication_rolls_back_staged_truth_when_audit_append_fails() {
        let run = TestRunBuilder::new("api-pub-006").build();
        let builder = PublicationFixtureBuilder::new(run.clone());
        let command = builder.valid_material();
        let material = PublicationMaterial::from_accept_publication_command(
            command.clone(),
            run.actor.clone(),
            run.metadata.clone(),
        )
        .expect("valid material");
        let harness = build_harness(&run);
        harness
            .audit_repository
            .fail_next_append(RepositoryError::Unavailable);

        let error = block_on(harness.api.accept_publication(
            command.clone(),
            run.actor.clone(),
            run.metadata.clone(),
        ))
        .expect_err("audit failure should abort the transaction");

        assert_eq!(error.status_code, 503);
        assert_eq!(error.code, "dependency.repository_unavailable");
        assert!(
            harness
                .publication_repository
                .committed(&material.publication_id)
                .is_none()
        );
        assert!(harness.audit_repository.committed_entries().is_empty());

        let scope = IdempotencyScope::for_accept_publication_command(&command);
        let key = run
            .metadata
            .request
            .idempotency_key
            .as_ref()
            .expect("fixture idempotency key");
        assert!(
            harness
                .idempotency_repository
                .committed_anchor(&scope, key)
                .is_none()
        );
    }
}
