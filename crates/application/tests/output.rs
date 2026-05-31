use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use bus_application::{
    AuditTrailRepository, OutboxPublisherService, OutboxPublisherServiceDeps,
    OutboxPublisherUseCase, PublisherPortError, ReadOutputService, ReadOutputServiceDeps,
    ReadOutputUseCase, ReadProjectionRepository, UnitOfWork, UnitOfWorkPurpose,
};
use bus_contracts::events::{
    BUS_OUTBOUND_EVENT_SCHEMA_VERSION, BusOutboundEvent, BusOutboundEventBatch,
    BusOutboundEventPayload, FailureMaterialAvailableEvent, PublicationAcceptedEvent,
    TransportViewUpdatedEvent,
};
use bus_contracts::fixtures::{
    BackendFixtureBuilder, FeedbackFixtureBuilder, PublicationFixtureBuilder, TestRun,
    TestRunBuilder,
};
use bus_contracts::metadata::{
    ActorContext, AuditRef, AuthorizationRef, ConsistencyMarker, DeliveryAttemptId, DeliveryMode,
    DeliveryStatus, EventId, FailureKind, FailureMaterialRef, HistoryReason, IdempotencyKey,
    PageRequest, QueryConsistency, QueryMetadata, RequestMetadata, RequestOrigin, RoleRef,
    SourceRecordRef, SubscriberRef, SubscriberScope, TargetScope, Timestamp,
};
use bus_contracts::queries::{
    AuditFilter, GetBusAuditTrailQuery, GetFailureSummaryQuery, GetTransportViewQuery,
};
use bus_contracts::views::{FailureSummaryView, TransportView};
use bus_domain::audit::{
    AuditAction, BusAuditEntry, PrivilegedAccessDecision, PrivilegedAccessRejectionReason,
    PrivilegedAccessScope, SubjectRef,
};
use bus_domain::delivery::{DeliveryHistoryEntry, DeliveryRecord};
use bus_domain::feedback::FeedbackResult;
use bus_domain::publication::{PayloadBoundaryGuard, PublicationMaterial, TransportSemantic};
use bus_domain::read_output::{
    FailureSummaryProjection, ProjectionStatus, TransportViewProjection,
};
use bus_domain::recovery::FailureMaterial;
use bus_infra::{
    InMemoryAuditTrailRepository, InMemoryDeliveryRepository, InMemoryFeedbackRepository,
    InMemoryOutboxPublisherAdapter, InMemoryPublicationRepository,
    InMemoryReadProjectionRepository, InMemoryRecoveryRepository, InMemoryUnitOfWork,
    SharedMemoryStore, TapOutputRecord,
};

type OutputService = ReadOutputService<
    InMemoryPublicationRepository,
    InMemoryDeliveryRepository,
    InMemoryFeedbackRepository,
    InMemoryAuditTrailRepository,
    InMemoryReadProjectionRepository,
    InMemoryRecoveryRepository,
>;

type PublisherService = OutboxPublisherService<InMemoryOutboxPublisherAdapter>;

struct Harness {
    service: OutputService,
    publisher_service: PublisherService,
    delivery_repository: InMemoryDeliveryRepository,
    audit_repository: InMemoryAuditTrailRepository,
    projection_repository: InMemoryReadProjectionRepository,
    recovery_repository: InMemoryRecoveryRepository,
    unit_of_work: InMemoryUnitOfWork,
    publisher: InMemoryOutboxPublisherAdapter,
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

fn query_metadata(run: &TestRun) -> QueryMetadata {
    QueryMetadata {
        request: RequestMetadata::new(
            run.metadata.request.request_id.clone(),
            run.metadata.request.trace_id.clone(),
            None,
            run.metadata.request.requested_at.clone(),
        ),
        page: None,
        consistency: QueryConsistency::Eventual,
    }
}

fn privileged_query_actor(run: &TestRun) -> ActorContext {
    let mut actor = ActorContext::new(run.actor.actor_ref().clone(), RequestOrigin::Query);
    actor.role_refs.push(RoleRef::new("role_output_reader"));
    actor
}

fn build_harness() -> Harness {
    let store = SharedMemoryStore::new();
    let publication_repository = InMemoryPublicationRepository::new(store.clone());
    let delivery_repository = InMemoryDeliveryRepository::new(store.clone());
    let feedback_repository = InMemoryFeedbackRepository::new(store.clone());
    let audit_repository = InMemoryAuditTrailRepository::new(store.clone());
    let projection_repository = InMemoryReadProjectionRepository::new(store.clone());
    let recovery_repository = InMemoryRecoveryRepository::new(store.clone());
    let unit_of_work = InMemoryUnitOfWork::new(store);
    let publisher = InMemoryOutboxPublisherAdapter::new(Timestamp::new("2026-05-31T00:00:40Z"));
    let service = ReadOutputService::new(ReadOutputServiceDeps {
        publication_repository,
        delivery_repository: delivery_repository.clone(),
        feedback_repository,
        audit_repository: audit_repository.clone(),
        projection_repository: projection_repository.clone(),
        recovery_repository: recovery_repository.clone(),
    });
    let publisher_service = OutboxPublisherService::new(OutboxPublisherServiceDeps {
        publisher: publisher.clone(),
    });

    Harness {
        service,
        publisher_service,
        delivery_repository,
        audit_repository,
        projection_repository,
        recovery_repository,
        unit_of_work,
        publisher,
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
            AuditRef::new(format!("audit_output_{}", run.run_id)),
            SubjectRef::Delivery(delivery.delivery_id.clone()),
            AuditAction::DeliveryDispatchStarted,
            run.actor.clone(),
            run.metadata.request.trace_id.clone(),
            Timestamp::new("2026-05-31T00:00:00Z"),
        ),
    )
    .expect("projection should derive")
}

fn failure_material_for(
    run: &TestRun,
    delivery_id: bus_contracts::metadata::DeliveryId,
) -> FailureMaterial {
    let feedback_builder = FeedbackFixtureBuilder::new(run.clone());
    let feedback = FeedbackResult::from_command(
        feedback_builder.fail_command(
            delivery_id.clone(),
            DeliveryAttemptId::new("attempt_failure"),
        ),
        run.actor.clone(),
    )
    .expect("failure feedback should build");
    let history = DeliveryHistoryEntry::transition(
        delivery_id,
        DeliveryStatus::Dispatching,
        DeliveryStatus::Failed,
        HistoryReason::delivery_failed(),
        Timestamp::new("2026-05-31T00:00:15Z"),
    );

    FailureMaterial::from_feedback(
        feedback,
        history,
        AuditRef::new(format!("audit_failure_{}", run.run_id)),
    )
    .expect("failure material should derive")
}

fn failure_summary_projection_for(
    material: &FailureMaterial,
    run: &TestRun,
) -> FailureSummaryProjection {
    FailureSummaryProjection::derive(
        material.clone(),
        BusAuditEntry::record(
            material.audit_ref.clone(),
            SubjectRef::Delivery(material.delivery_id.clone()),
            AuditAction::DeliveryFailed(bus_contracts::metadata::FailureReason::dispatch_failed()),
            run.actor.clone(),
            run.metadata.request.trace_id.clone(),
            Timestamp::new("2026-05-31T00:00:16Z"),
        ),
    )
    .expect("failure summary projection should derive")
}

fn transport_view_event(
    projection: &TransportViewProjection,
    delivery_id: bus_contracts::metadata::DeliveryId,
) -> BusOutboundEvent {
    BusOutboundEvent {
        event_id: EventId::new(format!("event_{}", projection.view_id.as_str())),
        payload: BusOutboundEventPayload::TransportViewUpdated(TransportViewUpdatedEvent {
            schema_version: BUS_OUTBOUND_EVENT_SCHEMA_VERSION.to_owned(),
            transport_view_id: projection.view_id.clone(),
            delivery_id,
            projection_version: projection.version.clone(),
            consistency_marker: match projection.status {
                ProjectionStatus::Active => ConsistencyMarker::Committed,
                ProjectionStatus::Building
                | ProjectionStatus::Stale
                | ProjectionStatus::Rebuilding => ConsistencyMarker::Stale,
            },
        }),
    }
}

fn failure_material_event(material: &FailureMaterial) -> BusOutboundEvent {
    BusOutboundEvent {
        event_id: EventId::new(format!("event_{}", material.failure_material_id.as_str())),
        payload: BusOutboundEventPayload::FailureMaterialAvailable(FailureMaterialAvailableEvent {
            schema_version: BUS_OUTBOUND_EVENT_SCHEMA_VERSION.to_owned(),
            failure_material_ref: FailureMaterialRef::from(material.failure_material_id.clone()),
            delivery_id: material.delivery_id.clone(),
            failure_kind: FailureKind::TransportFailure,
            audit_ref: material.audit_ref.clone(),
        }),
    }
}

fn publication_accepted_event_with_payload_ref(payload_ref: &str) -> BusOutboundEvent {
    BusOutboundEvent {
        event_id: EventId::new("event_publication_invalid"),
        payload: BusOutboundEventPayload::PublicationAccepted(PublicationAcceptedEvent {
            schema_version: BUS_OUTBOUND_EVENT_SCHEMA_VERSION.to_owned(),
            publication_id: bus_contracts::metadata::PublicationId::new("pub_invalid"),
            core_event_ref: bus_contracts::metadata::CoreEventRef::new("core_event_invalid"),
            core_event_envelope_ref: None,
            source_record_ref: SourceRecordRef::new("source_record_invalid"),
            payload_ref: bus_contracts::metadata::PayloadRef::new(payload_ref),
            delivery_mode: DeliveryMode::AtLeastOnce,
            target_scope: TargetScope {
                project_id: "project_invalid".to_owned(),
                topic: "topic.invalid".to_owned(),
            },
            audit_ref: AuditRef::new("audit_invalid"),
        }),
    }
}

fn append_audit_entry(harness: &Harness, entry: BusAuditEntry) {
    let uow = block_on(
        harness
            .unit_of_work
            .begin(UnitOfWorkPurpose::AcceptPublication, entry.actor.clone()),
    )
    .expect("uow should begin");
    block_on(harness.audit_repository.append(entry, &uow)).expect("audit should append");
    block_on(harness.unit_of_work.commit(uow)).expect("uow should commit");
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
        query_metadata(&run),
    ))
    .expect("transport view should load");

    assert_eq!(
        view,
        TransportView {
            transport_view_id: projection.view_id.clone(),
            delivery_id: delivery.delivery_id.clone(),
            transport_status: DeliveryStatus::Scheduled,
            transport_semantic: DeliveryMode::AtLeastOnce,
            projection_version: projection.version.clone(),
            consistency_marker: ConsistencyMarker::Committed,
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
        .mark_stale(AuditRef::new("audit_output_stale"))
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
        query_metadata(&run),
    ))
    .expect("stale transport view should still load");

    assert_eq!(view.consistency_marker, ConsistencyMarker::Stale);
    let stored = block_on(
        harness
            .projection_repository
            .get_transport_view_projection(&projection.view_id),
    )
    .expect("projection read should succeed")
    .expect("projection should remain committed");
    assert_eq!(stored.status, ProjectionStatus::Stale);
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
        query_metadata(&run),
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

#[test]
fn get_failure_summary_returns_authorized_view_without_decision_content() {
    let run = TestRunBuilder::new("out-003").build();
    let harness = build_harness();
    let actor = privileged_query_actor(&run);
    let delivery = scheduled_delivery(&run);
    let material = failure_material_for(&run, delivery.delivery_id.clone());
    harness
        .recovery_repository
        .seed_failure_material(material.clone())
        .expect("failure material should seed");
    let projection = failure_summary_projection_for(&material, &run);
    harness
        .projection_repository
        .seed_failure_summary_projection(projection.clone())
        .expect("failure summary projection should seed");

    let view = block_on(harness.service.get_failure_summary(
        GetFailureSummaryQuery {
            failure_summary_id: projection.summary_id.clone(),
            authorization_ref: Some(AuthorizationRef::new("auth_output_failure_summary")),
        },
        actor,
        query_metadata(&run),
    ))
    .expect("failure summary should load");

    assert_eq!(
        view,
        FailureSummaryView {
            failure_summary_id: projection.summary_id,
            delivery_id: material.delivery_id,
            failure_material_ref: FailureMaterialRef::from(material.failure_material_id),
            failure_kind: FailureKind::TransportFailure,
            governance_decision_ref: None,
        }
    );
    let audits = harness.audit_repository.committed_entries();
    assert_eq!(audits.len(), 1);
    assert_eq!(
        audits[0].action,
        AuditAction::PrivilegedAccess {
            scope: PrivilegedAccessScope::FailureSummary,
            decision: PrivilegedAccessDecision::Granted,
        }
    );
}

#[test]
fn get_failure_summary_rejects_missing_authorization_reference_with_access_audit() {
    let run = TestRunBuilder::new("out-003-missing-auth").build();
    let harness = build_harness();
    let actor = privileged_query_actor(&run);
    let delivery = scheduled_delivery(&run);
    let material = failure_material_for(&run, delivery.delivery_id.clone());
    harness
        .recovery_repository
        .seed_failure_material(material.clone())
        .expect("failure material should seed");
    let projection = failure_summary_projection_for(&material, &run);
    harness
        .projection_repository
        .seed_failure_summary_projection(projection.clone())
        .expect("failure summary projection should seed");

    let error = block_on(harness.service.get_failure_summary(
        GetFailureSummaryQuery {
            failure_summary_id: projection.summary_id.clone(),
            authorization_ref: None,
        },
        actor,
        query_metadata(&run),
    ))
    .expect_err("missing authorization reference must be rejected");

    assert_eq!(error.code(), "boundary.authorization_ref_required");
    let audits = harness.audit_repository.committed_entries();
    assert_eq!(audits.len(), 1);
    assert_eq!(
        audits[0].action,
        AuditAction::PrivilegedAccess {
            scope: PrivilegedAccessScope::FailureSummary,
            decision: PrivilegedAccessDecision::Rejected(
                PrivilegedAccessRejectionReason::MissingAuthorizationRef,
            ),
        }
    );
}

#[test]
fn get_bus_audit_trail_returns_append_only_monotonic_sequence() {
    let run = TestRunBuilder::new("out-005").build();
    let harness = build_harness();
    let actor = privileged_query_actor(&run);
    let delivery = scheduled_delivery(&run);
    let publication = PublicationFixtureBuilder::new(run.clone()).valid_material();

    append_audit_entry(
        &harness,
        BusAuditEntry::record(
            AuditRef::new("audit_sequence_01"),
            SubjectRef::Publication(bus_contracts::metadata::PublicationId::new("pub_sequence")),
            AuditAction::PublicationAccepted,
            run.actor.clone(),
            run.metadata.request.trace_id.clone(),
            Timestamp::new("2026-05-31T00:00:01Z"),
        ),
    );
    append_audit_entry(
        &harness,
        BusAuditEntry::record(
            AuditRef::new("audit_sequence_02"),
            SubjectRef::Delivery(delivery.delivery_id.clone()),
            AuditAction::DeliveryDispatchStarted,
            run.actor.clone(),
            run.metadata.request.trace_id.clone(),
            Timestamp::new("2026-05-31T00:00:02Z"),
        ),
    );
    append_audit_entry(
        &harness,
        BusAuditEntry::record(
            AuditRef::new("audit_sequence_03"),
            SubjectRef::Delivery(delivery.delivery_id.clone()),
            AuditAction::DeliveryFailed(bus_contracts::metadata::FailureReason::dispatch_failed()),
            run.actor.clone(),
            run.metadata.request.trace_id.clone(),
            Timestamp::new("2026-05-31T00:00:03Z"),
        ),
    );

    let trail = block_on(harness.service.get_bus_audit_trail(
        GetBusAuditTrailQuery {
            filter: AuditFilter {
                record_ref: Some(delivery.delivery_id.as_str().to_owned()),
                event_kind: None,
            },
            page: PageRequest {
                limit: 10,
                page_token: None,
            },
            authorization_ref: Some(AuthorizationRef::new("auth_output_audit_trail")),
        },
        actor,
        query_metadata(&run),
    ))
    .expect("audit trail should load");

    assert_eq!(
        publication.source_record_ref.as_str(),
        publication.source_record_ref.as_str()
    );
    assert_eq!(trail.items.len(), 2);
    assert_eq!(trail.items[0].audit_sequence, 2);
    assert_eq!(trail.items[1].audit_sequence, 3);
    assert!(trail.items[0].audit_sequence < trail.items[1].audit_sequence);
    assert_eq!(trail.next_cursor, None);
    let audits = harness.audit_repository.committed_entries();
    assert_eq!(audits.len(), 4);
    assert_eq!(
        audits[3].action,
        AuditAction::PrivilegedAccess {
            scope: PrivilegedAccessScope::BusAuditTrail,
            decision: PrivilegedAccessDecision::Granted,
        }
    );
}

#[test]
fn publish_batch_exposes_tap_output_to_fake_observability_consumer() {
    let run = TestRunBuilder::new("out-006-batch").build();
    let harness = build_harness();
    let delivery = scheduled_delivery(&run);
    let projection = projection_for(&delivery, &run);
    let material = failure_material_for(&run, delivery.delivery_id.clone());
    let batch = BusOutboundEventBatch {
        items: vec![
            transport_view_event(&projection, delivery.delivery_id.clone()),
            failure_material_event(&material),
        ],
    };

    let receipt = block_on(
        harness
            .publisher_service
            .publish_batch_committed(batch, run.metadata.request.trace_id.clone()),
    )
    .expect("batch publish should succeed");

    assert_eq!(receipt.receipts.len(), 2);
    let tap_output = harness.publisher.tap_outputs();
    assert_eq!(
        tap_output,
        vec![
            TapOutputRecord {
                event: transport_view_event(&projection, delivery.delivery_id.clone()),
                trace_ref: run.metadata.request.trace_id.clone(),
                published_at: Timestamp::new("2026-05-31T00:00:40Z"),
            },
            TapOutputRecord {
                event: failure_material_event(&material),
                trace_ref: run.metadata.request.trace_id.clone(),
                published_at: Timestamp::new("2026-05-31T00:00:40Z"),
            },
        ]
    );
    assert_eq!(
        harness
            .publisher
            .shared_tap_sink()
            .lock()
            .expect("tap sink lock should succeed")
            .len(),
        2
    );
    assert_eq!(harness.publisher.publish_evidence().len(), 2);
}

#[test]
fn publisher_retryable_failure_preserves_committed_projection_and_records_evidence() {
    let run = TestRunBuilder::new("out-006-retryable").build();
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
    let event = transport_view_event(&projection, delivery.delivery_id.clone());
    harness
        .publisher
        .fail_next_publish(PublisherPortError::RetryableFailure);

    let error = block_on(
        harness
            .publisher_service
            .publish_committed(event.clone(), run.metadata.request.trace_id.clone()),
    )
    .expect_err("retryable failure should surface");

    assert_eq!(error.code(), "dependency.outbound_publisher_unavailable");
    assert!(error.retryable());
    assert_eq!(
        harness
            .delivery_repository
            .committed(&delivery.delivery_id)
            .expect("delivery should remain committed")
            .status,
        DeliveryStatus::Scheduled
    );
    assert_eq!(
        block_on(
            harness
                .projection_repository
                .get_transport_view_projection(&projection.view_id)
        )
        .expect("projection read should succeed")
        .expect("projection should remain committed"),
        projection
    );
    let evidence = harness.publisher.publish_evidence();
    assert_eq!(evidence.len(), 1);
    assert_eq!(evidence[0].event_id, event.event_id);
    assert_eq!(
        evidence[0].status,
        bus_application::PublishEvidenceStatus::RetryableFailed
    );
    assert!(harness.publisher.published_events().is_empty());
}

#[test]
fn publish_committed_rejects_inline_payload_reference_before_sink_call() {
    let harness = build_harness();
    let guard = PayloadBoundaryGuard::default_for_bus();
    let event = publication_accepted_event_with_payload_ref("{\"payload\":\"inline\"}");

    assert!(guard.rejects_body(PublicationMaterial {
        publication_id: bus_contracts::metadata::PublicationId::new("pub_guard"),
        source_system: bus_contracts::metadata::SourceSystem::new("source_guard"),
        source_record_ref: SourceRecordRef::new("source_record_guard"),
        core_event_ref: bus_contracts::metadata::CoreEventRef::new("core_event_guard"),
        core_event_envelope_ref: None,
        payload_ref: bus_contracts::metadata::PayloadRef::new("{\"payload\":\"inline\"}"),
        payload_kind: bus_contracts::metadata::PayloadKind::ArtifactRef,
        payload_digest: bus_contracts::metadata::PayloadDigest::new("sha256:guard"),
        delivery_mode: DeliveryMode::AtLeastOnce,
        target_scope: TargetScope {
            project_id: "project_guard".to_owned(),
            topic: "topic.guard".to_owned(),
        },
        outbox_fact_ref: None,
        actor: bus_contracts::metadata::ActorContext::new(
            bus_contracts::metadata::ActorRef::new(
                "actor_guard",
                bus_contracts::metadata::ActorKind::Human,
            ),
            RequestOrigin::Command,
        ),
        trace_ref: bus_contracts::metadata::TraceContextRef::new("trace_guard"),
    }));

    let error = block_on(harness.publisher_service.publish_committed(
        event,
        bus_contracts::metadata::TraceContextRef::new("trace_inline_payload"),
    ))
    .expect_err("inline payload reference should be rejected");

    assert_eq!(error.code(), "boundary.outbound_event_payload_ref_rejected");
    assert!(harness.publisher.published_events().is_empty());
    assert!(harness.publisher.publish_evidence().is_empty());
}
