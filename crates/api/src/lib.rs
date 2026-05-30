//! Minimal command APIs for publication acceptance and delivery feedback.

use bus_application::{
    ApplicationError, DeliveryFeedbackUseCase, ProtocolErrorCategory, PublicationAcceptanceUseCase,
};
use bus_contracts::commands::{AcceptPublicationCommand, RecordDeliveryFeedbackCommand};
use bus_contracts::metadata::{ActorContext, CommandMetadata, RequestId, TraceContextRef};
use bus_contracts::receipts::{FeedbackRecordResult, PublicationAcceptanceResult};

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

/// Minimal command API surface for the delivery feedback write path.
pub struct DeliveryFeedbackApi<U> {
    feedback_recording: U,
}

impl<U> DeliveryFeedbackApi<U> {
    /// Creates a new feedback API wrapper.
    pub fn new(feedback_recording: U) -> Self {
        Self { feedback_recording }
    }
}

impl<U> DeliveryFeedbackApi<U>
where
    U: DeliveryFeedbackUseCase,
{
    /// Records one delivery feedback command into the bus write path.
    pub async fn record_feedback(
        &self,
        command: RecordDeliveryFeedbackCommand,
        actor: ActorContext,
        meta: CommandMetadata,
    ) -> Result<FeedbackRecordResult, ApiError> {
        self.feedback_recording
            .record(command, actor, meta.clone())
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
        FeedbackRecordingService, FeedbackRecordingServiceDeps, PublicationAcceptanceService,
        PublicationAcceptanceServiceDeps, RepositoryError,
    };
    use bus_contracts::fixtures::{
        BackendFixtureBuilder, FeedbackFixtureBuilder, PublicationFixtureBuilder, TestRun,
        TestRunBuilder,
    };
    use bus_contracts::metadata::{
        BackendDeliveryResult, CoreEventRef, DeliveryStatus, IdempotencyKey, PayloadRef,
        PublicationAcceptanceStatus, SubscriberRef, SubscriberScope, Timestamp,
    };
    use bus_domain::audit::AuditAction;
    use bus_domain::delivery::{DeliveryHistoryEntry, DeliveryRecord};
    use bus_domain::idempotency::IdempotencyScope;
    use bus_domain::publication::{PublicationMaterial, PublicationRejectReason};
    use bus_infra::{
        DeterministicIdGenerator, FixedClockAdapter, InMemoryAuditTrailRepository,
        InMemoryDeliveryRepository, InMemoryFeedbackRepository, InMemoryIdempotencyRepository,
        InMemoryPublicationRepository, InMemoryUnitOfWork, SharedMemoryStore,
    };

    use super::{BusCommandApi, DeliveryFeedbackApi};

    type PublicationService = PublicationAcceptanceService<
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
    >;

    struct Harness {
        api: BusCommandApi<PublicationService>,
        publication_repository: InMemoryPublicationRepository,
        idempotency_repository: InMemoryIdempotencyRepository,
        audit_repository: InMemoryAuditTrailRepository,
    }

    struct FeedbackHarness {
        api: DeliveryFeedbackApi<FeedbackService>,
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

    fn build_feedback_harness(run: &TestRun) -> FeedbackHarness {
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

        FeedbackHarness {
            api: DeliveryFeedbackApi::new(service),
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
        let semantic = bus_domain::publication::TransportSemantic::derive(
            material,
            capability_ref.clone(),
            SubscriberScope {
                project_id: format!("project_{}", run.run_id),
                topic: format!("workitem.events.{}", run.run_id),
            },
        )
        .expect("transport semantic should derive");
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
                BackendDeliveryResult::delivered(Some("backend_delivery_api_feedback".into())),
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
    fn accept_publication_missing_core_event_ref_returns_validation_with_rejected_truth() {
        let run = TestRunBuilder::new("api-pub-003").build();
        let builder = PublicationFixtureBuilder::new(run.clone());
        let mut command = builder.valid_material();
        command.core_event_ref = CoreEventRef::new("");
        let material = PublicationMaterial::from_accept_publication_command(
            command.clone(),
            run.actor.clone(),
            run.metadata.clone(),
        )
        .expect("missing core_event_ref still forms material for rejection");
        let harness = build_harness(&run);

        let error = block_on(harness.api.accept_publication(
            command.clone(),
            run.actor.clone(),
            run.metadata.clone(),
        ))
        .expect_err("missing core_event_ref must fail validation");

        assert_eq!(error.status_code, 400);
        assert_eq!(error.code, "validation.core_event_ref_missing");

        let committed = harness
            .publication_repository
            .committed(&material.publication_id)
            .expect("rejected publication should be committed");
        assert_eq!(committed.status, PublicationAcceptanceStatus::Rejected);
        assert_eq!(
            committed.reject_reason,
            Some(PublicationRejectReason::MissingCoreEventRef)
        );
        assert_eq!(harness.audit_repository.committed_entries().len(), 1);

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
                .is_some()
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

    #[test]
    fn record_feedback_returns_completed_result_and_commits_truth() {
        let run = TestRunBuilder::new("api-fdb-001").build();
        let harness = build_feedback_harness(&run);
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

        let result = block_on(harness.api.record_feedback(
            command.clone(),
            run.actor.clone(),
            run.metadata.clone(),
        ))
        .expect("ack feedback should record");

        assert_eq!(result.delivery_status, DeliveryStatus::Completed);
        assert_eq!(
            result.feedback_status,
            bus_contracts::metadata::FeedbackRecordStatus::Recorded
        );
        assert_eq!(harness.feedback_repository.committed_all().len(), 1);

        let committed = harness
            .delivery_repository
            .committed(&delivery.delivery_id)
            .expect("delivery should be committed");
        assert_eq!(committed.status, DeliveryStatus::Completed);
        assert_eq!(committed.history().len(), 3);
        assert_eq!(harness.audit_repository.committed_entries().len(), 1);

        let key = run
            .metadata
            .request
            .idempotency_key
            .as_ref()
            .expect("fixture idempotency key");
        assert!(
            harness
                .idempotency_repository
                .committed_anchor(
                    &IdempotencyScope::for_record_delivery_feedback(&command),
                    key
                )
                .is_some()
        );
    }

    #[test]
    fn record_feedback_same_key_same_digest_returns_existing_result() {
        let run = TestRunBuilder::new("api-fdb-002").build();
        let harness = build_feedback_harness(&run);
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

        let first = block_on(harness.api.record_feedback(
            command.clone(),
            run.actor.clone(),
            run.metadata.clone(),
        ))
        .expect("first feedback should record");
        let second = block_on(harness.api.record_feedback(
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
    fn record_feedback_late_or_unknown_feedback_returns_not_found_or_conflict() {
        let missing_run = TestRunBuilder::new("api-fdb-003").build();
        let missing_harness = build_feedback_harness(&missing_run);
        let missing_builder = FeedbackFixtureBuilder::new(missing_run.clone());
        let missing_command =
            missing_builder.ack_command("delivery_missing".into(), "attempt_missing".into());

        let missing_error = block_on(missing_harness.api.record_feedback(
            missing_command,
            missing_run.actor.clone(),
            missing_run.metadata.clone(),
        ))
        .expect_err("unknown delivery should return not found");

        assert_eq!(missing_error.status_code, 404);
        assert_eq!(missing_error.code, "not_found.delivery");
        assert!(
            missing_harness
                .feedback_repository
                .committed_all()
                .is_empty()
        );

        let late_run = TestRunBuilder::new("api-fdb-004").build();
        let late_harness = build_feedback_harness(&late_run);
        let delivery = seed_delivered_delivery(&late_harness.delivery_repository, &late_run);
        let builder = FeedbackFixtureBuilder::new(late_run.clone());
        let ack_command = builder.ack_command(
            delivery.delivery_id.clone(),
            delivery
                .last_attempt_ref
                .clone()
                .expect("attempt ref should exist")
                .as_str()
                .into(),
        );
        block_on(late_harness.api.record_feedback(
            ack_command,
            late_run.actor.clone(),
            late_run.metadata.clone(),
        ))
        .expect("first ack should complete delivery");
        let mut late_meta = late_run.metadata.clone();
        late_meta.request.idempotency_key = Some("idem-api-fdb-004-late".into());
        let late_command = builder.fail_command(
            delivery.delivery_id.clone(),
            delivery
                .last_attempt_ref
                .clone()
                .expect("attempt ref should exist")
                .as_str()
                .into(),
        );

        let late_error = block_on(late_harness.api.record_feedback(
            late_command,
            late_run.actor,
            late_meta,
        ))
        .expect_err("late feedback should conflict");

        assert_eq!(late_error.status_code, 409);
        assert_eq!(late_error.code, "conflict.delivery_state");
        assert_eq!(late_harness.feedback_repository.committed_all().len(), 1);
    }
}
