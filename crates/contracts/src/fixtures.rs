//! Reusable fixture builders for contract and domain tests.

use crate::commands::{AcceptPublicationCommand, RecordDeliveryFeedbackCommand};
use crate::events::{CommittedOutboxFact, CommittedOutboxFactInput, CommittedOutboxFactPage};
use crate::jobs::{
    DeliveryProgressionResult, OutboxRelayJobResult, RunDeliveryProgressionJob, RunOutboxRelayJob,
};
use crate::metadata::{
    ActorContext, ActorKind, ActorRef, BackendCapabilityRef, BackendId, BackendKind,
    BackendProfileRef, CapabilityVersion, CommandMetadata, CommittedOutboxFactRef,
    ConsistencyMarker, ConsumerMarker, CoreEventEnvelopeRef, CoreEventRef, DeliveryAttemptId,
    DeliveryId, DeliveryMode, DeliveryScanCursor, DeliveryStatus, EventId, EventMetadata,
    EventSourceRef, ExternalFeedbackRef, FeedbackId, FeedbackKind, FeedbackReason,
    FeedbackRecordStatus, JobMetadata, JobRunId, JobTriggerSource, OutboxCursor, PayloadDigest,
    PayloadKind, PayloadRef, PublicationId, RequestId, RequestMetadata, RequestOrigin,
    SourceRecordRef, SourceSystem, TargetScope, Timestamp, TraceId,
};
use crate::queries::GetDeliveryStatusQuery;
use crate::receipts::{FeedbackRecordResult, OutboxRelayResult, OutboxRelayStatus};
use crate::views::DeliveryStatusView;

/// The shared baseline data for a deterministic test run.
#[derive(Clone, Debug)]
pub struct TestRun {
    /// The unique test run identifier.
    pub run_id: String,
    /// The actor context associated with the run.
    pub actor: ActorContext,
    /// The command metadata associated with the run.
    pub metadata: CommandMetadata,
}

/// Builds deterministic test run data keyed by a run identifier.
#[derive(Clone, Debug)]
pub struct TestRunBuilder {
    run_id: String,
}

impl TestRunBuilder {
    /// Creates a new deterministic run builder.
    pub fn new(run_id: impl Into<String>) -> Self {
        Self {
            run_id: run_id.into(),
        }
    }

    /// Builds the actor and metadata used by the current test run.
    pub fn build(&self) -> TestRun {
        let run_id = self.run_id.clone();

        TestRun {
            actor: ActorContext::new(
                ActorRef::new(format!("actor-{run_id}"), ActorKind::Human),
                RequestOrigin::Command,
            ),
            metadata: CommandMetadata {
                request: RequestMetadata::new(
                    RequestId::new(format!("request-{run_id}")),
                    TraceId::new(format!("trace-{run_id}")),
                    Some(core_contracts::metadata::IdempotencyKey::new(format!(
                        "idem-{run_id}"
                    ))),
                    Timestamp::new("2026-05-30T00:00:00Z"),
                ),
                reason: None,
                external_ref: None,
            },
            run_id,
        }
    }

    /// Builds deterministic job metadata for the current run.
    pub fn build_job_metadata(&self) -> JobMetadata {
        let run_id = self.run_id.clone();

        JobMetadata {
            job_run_id: JobRunId::new(format!("job_run_{run_id}")),
            trace_ref: TraceId::new(format!("trace-job-{run_id}")),
            trigger_source: JobTriggerSource::Scheduler,
        }
    }
}

/// Builds publication command fixtures for a deterministic run.
#[derive(Clone, Debug)]
pub struct PublicationFixtureBuilder {
    run: TestRun,
}

impl PublicationFixtureBuilder {
    /// Creates a new publication fixture builder for the provided run.
    pub fn new(run: TestRun) -> Self {
        Self { run }
    }

    /// Returns a valid publication acceptance command for the current run.
    pub fn valid_material(&self) -> AcceptPublicationCommand {
        let run_id = &self.run.run_id;

        AcceptPublicationCommand {
            source_system: SourceSystem::new(format!("l2-process-{run_id}")),
            source_record_ref: SourceRecordRef::new(format!("process_event_{run_id}")),
            core_event_ref: CoreEventRef::new(format!("core_event_contract_{run_id}")),
            payload_ref: PayloadRef::new(format!("artifact_ref_{run_id}")),
            payload_kind: PayloadKind::ArtifactRef,
            payload_digest: PayloadDigest::new(format!("sha256:{run_id}")),
            delivery_mode: DeliveryMode::AtLeastOnce,
            target_scope: TargetScope {
                project_id: format!("project_{run_id}"),
                topic: format!("workitem.events.{run_id}"),
            },
        }
    }

    /// Returns the underlying test run baseline.
    pub fn run(&self) -> &TestRun {
        &self.run
    }
}

/// Builds deterministic backend capability fixtures for a run.
#[derive(Clone, Debug)]
pub struct BackendFixtureBuilder {
    run: TestRun,
}

impl BackendFixtureBuilder {
    /// Creates a new backend fixture builder.
    pub fn new(run: TestRun) -> Self {
        Self { run }
    }

    /// Returns the default in-memory backend capability reference.
    pub fn in_memory_capability(&self) -> BackendCapabilityRef {
        BackendCapabilityRef::from_profile(
            BackendProfileRef::new(format!("profile_{}", self.run.run_id)),
            BackendKind::InMemory,
            CapabilityVersion::new("v1"),
        )
    }

    /// Returns a backend capability reference that should be rejected as a leak.
    pub fn tainted_capability(&self) -> BackendCapabilityRef {
        BackendCapabilityRef::from_profile(
            BackendProfileRef::new("amqp://user:secret@example.internal"),
            BackendKind::InMemory,
            CapabilityVersion::new("v1"),
        )
    }

    /// Returns the logical backend identifier used by job DTO fixtures.
    pub fn backend_id(&self) -> BackendId {
        BackendId::new(format!("backend_{}", self.run.run_id))
    }
}

/// Builds deterministic committed outbox fact fixtures for a run.
#[derive(Clone, Debug)]
pub struct OutboxFixtureBuilder {
    run: TestRun,
}

impl OutboxFixtureBuilder {
    /// Creates a new outbox fixture builder.
    pub fn new(run: TestRun) -> Self {
        Self { run }
    }

    /// Returns the stable origin cursor for a new source scan.
    pub fn origin_cursor(&self) -> OutboxCursor {
        OutboxCursor::origin()
    }

    /// Returns the stable source-consumer marker for the bus.
    pub fn consumer_marker(&self) -> ConsumerMarker {
        ConsumerMarker::bus()
    }

    /// Returns the first committed outbox fact for the current run.
    pub fn committed_fact(&self) -> CommittedOutboxFact {
        self.committed_fact_with_suffix("01")
    }

    /// Returns a second committed outbox fact for cursor and paging tests.
    pub fn second_committed_fact(&self) -> CommittedOutboxFact {
        self.committed_fact_with_suffix("02")
    }

    /// Returns a one-item committed-fact page.
    pub fn committed_fact_page(&self) -> CommittedOutboxFactPage {
        let fact = self.committed_fact();
        CommittedOutboxFactPage {
            next_cursor: OutboxCursor::new(format!(
                "outbox_cursor_{}",
                fact.committed_fact_ref.as_str()
            )),
            items: vec![fact],
        }
    }

    /// Returns the consumer-ready input converted from the first committed fact.
    pub fn committed_fact_input(&self) -> CommittedOutboxFactInput {
        CommittedOutboxFactInput::from_fact(self.committed_fact())
    }

    /// Returns a committed fact with an empty core event reference for rejection tests.
    pub fn committed_fact_missing_core_event_ref(&self) -> CommittedOutboxFact {
        let mut fact = self.committed_fact();
        fact.core_event_ref = CoreEventRef::new("");
        fact
    }

    /// Returns a valid outbox-relay job input DTO.
    pub fn run_outbox_relay_job(&self) -> RunOutboxRelayJob {
        RunOutboxRelayJob {
            job_run_id: JobRunId::new(format!("job_run_{}", self.run.run_id)),
            cursor: self.origin_cursor(),
            batch_size: 50,
            dry_run: false,
        }
    }

    /// Returns deterministic job metadata for outbox relay.
    pub fn run_outbox_relay_metadata(&self) -> JobMetadata {
        JobMetadata {
            job_run_id: JobRunId::new(format!("job_run_{}", self.run.run_id)),
            trace_ref: TraceId::new(format!("trace-job-{}", self.run.run_id)),
            trigger_source: JobTriggerSource::Scheduler,
        }
    }

    /// Returns deterministic consumer metadata for the current run.
    pub fn event_metadata(&self) -> EventMetadata {
        EventMetadata {
            trace_ref: TraceId::new(format!("trace-event-{}", self.run.run_id)),
        }
    }

    /// Returns a valid outbox-relay summary DTO.
    pub fn outbox_relay_result(&self) -> OutboxRelayJobResult {
        OutboxRelayJobResult {
            job_run_id: JobRunId::new(format!("job_run_{}", self.run.run_id)),
            scanned: 2,
            accepted: 1,
            rejected: 1,
            next_cursor: OutboxCursor::new(format!("outbox_cursor_next_{}", self.run.run_id)),
        }
    }

    /// Returns a valid outbox-relay receipt DTO.
    pub fn relay_receipt(&self) -> OutboxRelayResult {
        OutboxRelayResult {
            publication_id: PublicationId::new(format!("pub_{}", self.run.run_id)),
            relay_status: OutboxRelayStatus::Accepted,
            audit_ref: crate::metadata::AuditRef::new(format!("audit_{}", self.run.run_id)),
        }
    }

    fn committed_fact_with_suffix(&self, suffix: &str) -> CommittedOutboxFact {
        let run_id = &self.run.run_id;

        CommittedOutboxFact {
            event_id: EventId::new(format!("event_{run_id}_{suffix}")),
            source_ref: EventSourceRef::new(format!("l0_core_outbox_{run_id}")),
            core_event_envelope_ref: CoreEventEnvelopeRef::new(format!(
                "core_event_envelope_{run_id}_{suffix}"
            )),
            core_event_ref: CoreEventRef::new(format!("core_event_contract_{run_id}_{suffix}")),
            committed_fact_ref: CommittedOutboxFactRef::new(format!(
                "outbox_fact_{run_id}_{suffix}"
            )),
            source_system: SourceSystem::new("l0-core"),
            source_record_ref: SourceRecordRef::new(format!("core_record_{run_id}_{suffix}")),
            payload_ref: PayloadRef::new(format!("artifact_ref_{run_id}_{suffix}")),
            payload_kind: PayloadKind::ArtifactRef,
            payload_digest: PayloadDigest::new(format!("sha256:{run_id}:{suffix}")),
            delivery_mode: DeliveryMode::AtLeastOnce,
            target_scope: TargetScope {
                project_id: format!("project_{run_id}"),
                topic: format!("workitem.events.{run_id}"),
            },
            idempotency_key: core_contracts::metadata::IdempotencyKey::new(format!(
                "idem_outbox_{run_id}_{suffix}"
            )),
        }
    }
}

/// Builds deterministic delivery query and job fixtures for a run.
#[derive(Clone, Debug)]
pub struct DeliveryFixtureBuilder {
    run: TestRun,
}

impl DeliveryFixtureBuilder {
    /// Creates a new delivery fixture builder.
    pub fn new(run: TestRun) -> Self {
        Self { run }
    }

    /// Returns a stable delivery identifier.
    pub fn delivery_id(&self) -> DeliveryId {
        DeliveryId::new(format!("delivery_{}", self.run.run_id))
    }

    /// Returns a valid delivery-status query DTO.
    pub fn delivery_status_query(&self) -> GetDeliveryStatusQuery {
        GetDeliveryStatusQuery {
            delivery_id: self.delivery_id(),
        }
    }

    /// Returns a valid delivery-status view DTO.
    pub fn delivery_status_view(&self, status: DeliveryStatus) -> DeliveryStatusView {
        DeliveryStatusView {
            delivery_id: self.delivery_id(),
            publication_id: PublicationId::new(format!("pub_{}", self.run.run_id)),
            delivery_status: status,
            current_attempt_id: Some(DeliveryAttemptId::new(format!(
                "attempt_{}",
                self.run.run_id
            ))),
            last_feedback_id: Some(FeedbackId::new(format!("feedback_{}", self.run.run_id))),
            consistency_marker: ConsistencyMarker::Committed,
        }
    }

    /// Returns a valid delivery-progression job input DTO.
    pub fn run_delivery_progression_job(&self) -> RunDeliveryProgressionJob {
        RunDeliveryProgressionJob {
            job_run_id: JobRunId::new(format!("job_run_{}", self.run.run_id)),
            cursor: DeliveryScanCursor::new(format!("delivery_cursor_{}", self.run.run_id)),
            batch_size: 50,
            backend_id: BackendId::new(format!("backend_{}", self.run.run_id)),
        }
    }

    /// Returns deterministic job metadata for delivery progression.
    pub fn run_delivery_progression_metadata(&self) -> JobMetadata {
        JobMetadata {
            job_run_id: JobRunId::new(format!("job_run_{}", self.run.run_id)),
            trace_ref: TraceId::new(format!("trace-job-{}", self.run.run_id)),
            trigger_source: JobTriggerSource::Scheduler,
        }
    }

    /// Returns a valid delivery-progression summary DTO.
    pub fn delivery_progression_result(&self) -> DeliveryProgressionResult {
        DeliveryProgressionResult {
            job_run_id: JobRunId::new(format!("job_run_{}", self.run.run_id)),
            scanned: 2,
            dispatched: 1,
            skipped: 0,
            next_cursor: DeliveryScanCursor::new(format!(
                "delivery_cursor_next_{}",
                self.run.run_id
            )),
        }
    }
}

/// Builds deterministic feedback command and receipt fixtures for a run.
#[derive(Clone, Debug)]
pub struct FeedbackFixtureBuilder {
    run: TestRun,
}

impl FeedbackFixtureBuilder {
    /// Creates a new feedback fixture builder.
    pub fn new(run: TestRun) -> Self {
        Self { run }
    }

    /// Returns a valid feedback-recording command for the provided delivery attempt.
    pub fn ack_command(
        &self,
        delivery_id: DeliveryId,
        attempt_id: DeliveryAttemptId,
    ) -> RecordDeliveryFeedbackCommand {
        RecordDeliveryFeedbackCommand {
            delivery_id,
            attempt_id,
            feedback_kind: FeedbackKind::Ack,
            feedback_reason: FeedbackReason::new("subscriber_processed"),
            observed_at: Timestamp::new("2026-05-30T00:00:10Z"),
            external_feedback_ref: ExternalFeedbackRef::new(format!(
                "external_feedback_{}",
                self.run.run_id
            )),
        }
    }

    /// Returns a fail feedback command for the provided delivery attempt.
    pub fn fail_command(
        &self,
        delivery_id: DeliveryId,
        attempt_id: DeliveryAttemptId,
    ) -> RecordDeliveryFeedbackCommand {
        RecordDeliveryFeedbackCommand {
            feedback_kind: FeedbackKind::Fail,
            feedback_reason: FeedbackReason::new("subscriber_failed"),
            ..self.ack_command(delivery_id, attempt_id)
        }
    }

    /// Returns a valid feedback-recording receipt DTO.
    pub fn feedback_record_result(
        &self,
        delivery_id: DeliveryId,
        delivery_status: DeliveryStatus,
    ) -> FeedbackRecordResult {
        FeedbackRecordResult {
            feedback_id: FeedbackId::new(format!("feedback_{}", self.run.run_id)),
            delivery_id,
            feedback_status: FeedbackRecordStatus::Recorded,
            delivery_status,
            audit_ref: crate::metadata::AuditRef::new(format!("audit_{}", self.run.run_id)),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde::Serialize;
    use serde::de::DeserializeOwned;

    use super::*;
    use crate::metadata::{
        AuditRef, DeliveryStatus, PublicationAcceptanceStatus, PublicationId, RejectionReasonRef,
    };
    use crate::receipts::{FeedbackRecordResult, PublicationAcceptanceResult};

    fn roundtrip<T>(value: &T)
    where
        T: Clone + DeserializeOwned + Eq + Serialize + std::fmt::Debug,
    {
        let encoded = serde_json::to_value(value).expect("value should serialize");
        let decoded: T =
            serde_json::from_value(encoded).expect("value should deserialize after roundtrip");
        assert_eq!(decoded, *value);
    }

    #[test]
    fn accept_publication_command_roundtrip() {
        let run = TestRunBuilder::new("pub-001").build();
        let builder = PublicationFixtureBuilder::new(run);
        let command = builder.valid_material();

        roundtrip(&command);
    }

    #[test]
    fn publication_acceptance_result_roundtrip() {
        roundtrip(&PublicationAcceptanceResult {
            publication_id: PublicationId::new("pub-001"),
            acceptance_status: PublicationAcceptanceStatus::Rejected,
            rejection_reason_ref: Some(RejectionReasonRef::new("boundary.payload_body_rejected")),
            audit_ref: AuditRef::new("audit-001"),
        });
    }

    #[test]
    fn job_metadata_roundtrip() {
        let builder = TestRunBuilder::new("job-001");

        roundtrip(&builder.build_job_metadata());
    }

    #[test]
    fn event_metadata_roundtrip() {
        let run = TestRunBuilder::new("evt-001").build();
        let builder = OutboxFixtureBuilder::new(run);

        roundtrip(&builder.event_metadata());
    }

    #[test]
    fn delivery_progression_result_roundtrip() {
        let run = TestRunBuilder::new("job-002").build();
        let builder = DeliveryFixtureBuilder::new(run);

        roundtrip(&builder.delivery_progression_result());
    }

    #[test]
    fn record_delivery_feedback_command_roundtrip() {
        let run = TestRunBuilder::new("fdb-001").build();
        let delivery_builder = DeliveryFixtureBuilder::new(run.clone());
        let builder = FeedbackFixtureBuilder::new(run);
        let command = builder.ack_command(
            delivery_builder.delivery_id(),
            DeliveryAttemptId::new("attempt_fdb_001"),
        );

        roundtrip(&command);
    }

    #[test]
    fn feedback_record_result_roundtrip() {
        roundtrip(&FeedbackRecordResult {
            feedback_id: FeedbackId::new("feedback-001"),
            delivery_id: DeliveryId::new("delivery-001"),
            feedback_status: FeedbackRecordStatus::Recorded,
            delivery_status: DeliveryStatus::Completed,
            audit_ref: AuditRef::new("audit-002"),
        });
    }

    #[test]
    fn committed_outbox_fact_roundtrip() {
        let run = TestRunBuilder::new("obx-001").build();
        let builder = OutboxFixtureBuilder::new(run);

        roundtrip(&builder.committed_fact());
    }

    #[test]
    fn committed_outbox_fact_page_roundtrip() {
        let run = TestRunBuilder::new("obx-002").build();
        let builder = OutboxFixtureBuilder::new(run);

        roundtrip(&builder.committed_fact_page());
    }

    #[test]
    fn committed_outbox_fact_input_roundtrip() {
        let run = TestRunBuilder::new("obx-003").build();
        let builder = OutboxFixtureBuilder::new(run);

        roundtrip(&builder.committed_fact_input());
    }

    #[test]
    fn committed_outbox_fact_rejects_payload_body_field() {
        let run = TestRunBuilder::new("obx-004").build();
        let builder = OutboxFixtureBuilder::new(run);
        let mut encoded =
            serde_json::to_value(builder.committed_fact()).expect("fact should serialize");

        encoded
            .as_object_mut()
            .expect("fact should serialize as an object")
            .insert(
                "payload_body".to_owned(),
                serde_json::Value::String("{\"secret\":\"value\"}".to_owned()),
            );

        let error = serde_json::from_value::<CommittedOutboxFact>(encoded)
            .expect_err("payload_body should be rejected");

        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn accept_publication_command_requires_core_event_ref_field() {
        let run = TestRunBuilder::new("pub-002").build();
        let builder = PublicationFixtureBuilder::new(run);
        let mut encoded =
            serde_json::to_value(builder.valid_material()).expect("command should serialize");

        encoded
            .as_object_mut()
            .expect("command should serialize as an object")
            .remove("core_event_ref");

        let error = serde_json::from_value::<AcceptPublicationCommand>(encoded)
            .expect_err("missing core_event_ref should fail");

        assert!(error.to_string().contains("core_event_ref"));
    }

    #[test]
    fn accept_publication_command_rejects_payload_body_field() {
        let run = TestRunBuilder::new("pub-003").build();
        let builder = PublicationFixtureBuilder::new(run);
        let mut encoded =
            serde_json::to_value(builder.valid_material()).expect("command should serialize");

        encoded
            .as_object_mut()
            .expect("command should serialize as an object")
            .insert(
                "payload_body".to_owned(),
                serde_json::Value::String("{\"secret\":\"value\"}".to_owned()),
            );

        let error = serde_json::from_value::<AcceptPublicationCommand>(encoded)
            .expect_err("payload_body should be rejected");

        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn accept_publication_command_rejects_transport_semantic_field() {
        let run = TestRunBuilder::new("pub-004").build();
        let builder = PublicationFixtureBuilder::new(run);
        let mut encoded =
            serde_json::to_value(builder.valid_material()).expect("command should serialize");

        encoded
            .as_object_mut()
            .expect("command should serialize as an object")
            .insert(
                "transport_semantic".to_owned(),
                serde_json::Value::String("at_least_once".to_owned()),
            );

        let error = serde_json::from_value::<AcceptPublicationCommand>(encoded)
            .expect_err("transport_semantic should be rejected");

        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn accept_publication_command_rejects_legacy_delivery_mode_value() {
        let run = TestRunBuilder::new("pub-005").build();
        let builder = PublicationFixtureBuilder::new(run);
        let mut encoded =
            serde_json::to_value(builder.valid_material()).expect("command should serialize");

        encoded
            .as_object_mut()
            .expect("command should serialize as an object")
            .insert(
                "delivery_mode".to_owned(),
                serde_json::Value::String("broadcast".to_owned()),
            );

        let error = serde_json::from_value::<AcceptPublicationCommand>(encoded)
            .expect_err("legacy delivery_mode should be rejected");

        assert!(error.to_string().contains("unknown variant"));
    }

    #[test]
    fn get_delivery_status_query_roundtrip() {
        let run = TestRunBuilder::new("dlv-001").build();
        let builder = DeliveryFixtureBuilder::new(run);

        roundtrip(&builder.delivery_status_query());
    }

    #[test]
    fn delivery_status_view_roundtrip() {
        let run = TestRunBuilder::new("dlv-002").build();
        let builder = DeliveryFixtureBuilder::new(run);

        roundtrip(&builder.delivery_status_view(DeliveryStatus::Delivered));
    }

    #[test]
    fn run_delivery_progression_job_roundtrip() {
        let run = TestRunBuilder::new("job-001").build();
        let builder = DeliveryFixtureBuilder::new(run);

        roundtrip(&builder.run_delivery_progression_job());
    }

    #[test]
    fn run_outbox_relay_job_roundtrip() {
        let run = TestRunBuilder::new("job-003").build();
        let builder = OutboxFixtureBuilder::new(run);

        roundtrip(&builder.run_outbox_relay_job());
    }

    #[test]
    fn outbox_relay_job_result_roundtrip() {
        let run = TestRunBuilder::new("job-004").build();
        let builder = OutboxFixtureBuilder::new(run);

        roundtrip(&builder.outbox_relay_result());
    }

    #[test]
    fn outbox_relay_result_roundtrip() {
        let run = TestRunBuilder::new("obx-005").build();
        let builder = OutboxFixtureBuilder::new(run);

        roundtrip(&builder.relay_receipt());
    }

    #[test]
    fn backend_capability_fixture_roundtrip() {
        let run = TestRunBuilder::new("bnd-001").build();
        let builder = BackendFixtureBuilder::new(run);

        roundtrip(&builder.in_memory_capability());
    }
}
