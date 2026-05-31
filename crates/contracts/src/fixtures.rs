//! Reusable fixture builders for contract and domain tests.

use crate::commands::{
    AcceptPublicationCommand, MoveDeliveryToDeadLetterCommand, PrepareReplayCommand,
    RecordDeliveryFeedbackCommand, RequestRetryCommand,
};
use crate::events::{
    BackendDeliverySignalInput, CommittedOutboxFact, CommittedOutboxFactInput,
    CommittedOutboxFactPage, DeliveryTimeoutSignalInput,
};
use crate::jobs::{
    DeliveryProgressionResult, OutboxRelayJobResult, RetryCycleResult, RunDeliveryProgressionJob,
    RunOutboxRelayJob, RunRetryCycleJob,
};
use crate::metadata::{
    ActorContext, ActorKind, ActorRef, AttemptCount, AttemptLimit, AuditChainRef,
    BackendCapabilityRef, BackendId, BackendKind, BackendProfileRef, BackendResultRef,
    BackendStatus, CapabilityVersion, CommandMetadata, CommittedOutboxFactRef, ConsistencyMarker,
    ConsumerMarker, CoreEventEnvelopeRef, CoreEventRef, DeadLetterId, DeadLetterReason,
    DeadLetterStatus, DeliveryAttemptId, DeliveryId, DeliveryMode, DeliveryScanCursor,
    DeliveryStatus, EventId, EventMetadata, EventSourceRef, ExternalFeedbackRef,
    FailureMaterialRef, FeedbackId, FeedbackKind, FeedbackReason, FeedbackRecordStatus,
    JobMetadata, JobRunId, JobTriggerSource, OperatorNoteRef, OutboxCursor, PayloadDigest,
    PayloadKind, PayloadRef, PublicationId, ReplayApprovalRef, ReplayPreparationId,
    ReplayPreparationStatus, ReplayReason, RequestId, RequestMetadata, RequestOrigin, RetryPlanId,
    RetryPlanStatus, RetryPolicyRef, RetryRequestReason, RetryScanCursor, SourceRecordRef,
    SourceSystem, TargetScope, TimeoutReason, Timestamp, TraceId,
};
use crate::queries::GetDeliveryStatusQuery;
use crate::receipts::{
    BackendSignalNormalizedResult, BackendSignalResult, BackendSignalStatus, DeadLetterResult,
    FeedbackRecordResult, OutboxRelayResult, OutboxRelayStatus, ReplayPreparationResult,
    RetryPlanResult, TimeoutRecordResult, TimeoutRecordStatus,
};
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

    /// Returns deterministic consumer metadata for feedback signal tests.
    pub fn event_metadata(&self) -> EventMetadata {
        EventMetadata {
            trace_ref: TraceId::new(format!("trace-event-{}", self.run.run_id)),
        }
    }

    /// Returns a delivered backend signal for the provided attempt.
    pub fn delivered_backend_signal(
        &self,
        delivery_id: DeliveryId,
        attempt_id: DeliveryAttemptId,
        backend_capability_ref: BackendCapabilityRef,
    ) -> BackendDeliverySignalInput {
        BackendDeliverySignalInput {
            event_id: EventId::new(format!("event_backend_signal_{}", self.run.run_id)),
            source_ref: EventSourceRef::new(format!("backend_adapter_{}", self.run.run_id)),
            delivery_id,
            attempt_id,
            backend_capability_ref,
            backend_status: BackendStatus::Delivered,
            backend_result_ref: BackendResultRef::new(format!(
                "backend_result_{}",
                self.run.run_id
            )),
            idempotency_key: core_contracts::metadata::IdempotencyKey::new(format!(
                "idem_backend_signal_{}",
                self.run.run_id
            )),
        }
    }

    /// Returns a failed backend signal for the provided attempt.
    pub fn failed_backend_signal(
        &self,
        delivery_id: DeliveryId,
        attempt_id: DeliveryAttemptId,
        backend_capability_ref: BackendCapabilityRef,
    ) -> BackendDeliverySignalInput {
        BackendDeliverySignalInput {
            backend_status: BackendStatus::Failed,
            ..self.delivered_backend_signal(delivery_id, attempt_id, backend_capability_ref)
        }
    }

    /// Returns a timeout signal for the provided attempt.
    pub fn timeout_signal(
        &self,
        delivery_id: DeliveryId,
        attempt_id: DeliveryAttemptId,
    ) -> DeliveryTimeoutSignalInput {
        DeliveryTimeoutSignalInput {
            event_id: EventId::new(format!("event_timeout_signal_{}", self.run.run_id)),
            source_ref: EventSourceRef::new(format!("scheduler_{}", self.run.run_id)),
            delivery_id,
            attempt_id,
            timeout_reason: TimeoutReason::DispatchTimeout,
            occurred_at: Timestamp::new("2026-05-30T00:00:20Z"),
            idempotency_key: core_contracts::metadata::IdempotencyKey::new(format!(
                "idem_timeout_signal_{}",
                self.run.run_id
            )),
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

    /// Returns a valid backend-signal receipt DTO.
    pub fn backend_signal_result(
        &self,
        delivery_id: DeliveryId,
        attempt_id: DeliveryAttemptId,
    ) -> BackendSignalResult {
        BackendSignalResult {
            delivery_id,
            attempt_id,
            signal_status: BackendSignalStatus::Recorded,
            normalized_result: Some(BackendSignalNormalizedResult::Ack),
            feedback_id: Some(FeedbackId::new(format!("feedback_{}", self.run.run_id))),
            audit_ref: crate::metadata::AuditRef::new(format!("audit_{}", self.run.run_id)),
        }
    }

    /// Returns a valid timeout-signal receipt DTO.
    pub fn timeout_record_result(&self, delivery_id: DeliveryId) -> TimeoutRecordResult {
        TimeoutRecordResult {
            delivery_id,
            feedback_id: Some(FeedbackId::new(format!("feedback_{}", self.run.run_id))),
            feedback_status: TimeoutRecordStatus::TimeoutRecorded,
            recovery_candidate: true,
            audit_ref: crate::metadata::AuditRef::new(format!("audit_{}", self.run.run_id)),
        }
    }
}

/// Builds deterministic recovery command and receipt fixtures for a run.
#[derive(Clone, Debug)]
pub struct RecoveryFixtureBuilder {
    run: TestRun,
}

impl RecoveryFixtureBuilder {
    /// Creates a new recovery fixture builder.
    pub fn new(run: TestRun) -> Self {
        Self { run }
    }

    /// Returns a stable failure-material reference for the current run.
    pub fn failure_material_ref(&self) -> FailureMaterialRef {
        FailureMaterialRef::new(format!("failure_material_{}", self.run.run_id))
    }

    /// Returns a stable retry-policy reference for the current run.
    pub fn retry_policy_ref(&self) -> RetryPolicyRef {
        RetryPolicyRef::new(format!("retry_policy_{}", self.run.run_id))
    }

    /// Returns a stable dead-letter identifier for the current run.
    pub fn dead_letter_id(&self) -> DeadLetterId {
        DeadLetterId::new(format!("dead_letter_{}", self.run.run_id))
    }

    /// Returns a stable audit-chain reference for the current run.
    pub fn audit_chain_ref(&self) -> AuditChainRef {
        AuditChainRef::new(format!("audit_chain_{}", self.run.run_id))
    }

    /// Returns a stable replay-approval reference for the current run.
    pub fn replay_approval_ref(&self) -> ReplayApprovalRef {
        ReplayApprovalRef::new(format!("approval_{}", self.run.run_id))
    }

    /// Returns the stable origin cursor for retry-plan scans.
    pub fn retry_cursor(&self) -> RetryScanCursor {
        RetryScanCursor::origin()
    }

    /// Returns a retry command for the provided delivery identifier.
    pub fn request_retry_command(&self, delivery_id: DeliveryId) -> RequestRetryCommand {
        RequestRetryCommand {
            delivery_id,
            failure_material_ref: self.failure_material_ref(),
            retry_policy_ref: self.retry_policy_ref(),
            requested_reason: RetryRequestReason::new("transient_backend_failure"),
            max_attempts: AttemptLimit::new(3),
        }
    }

    /// Returns a dead-letter command for the provided delivery identifier.
    pub fn move_to_dead_letter_command(
        &self,
        delivery_id: DeliveryId,
    ) -> MoveDeliveryToDeadLetterCommand {
        MoveDeliveryToDeadLetterCommand {
            delivery_id,
            failure_material_ref: self.failure_material_ref(),
            dead_letter_reason: DeadLetterReason::new("retry_exhausted"),
            operator_note_ref: Some(OperatorNoteRef::new(format!(
                "operator_note_{}",
                self.run.run_id
            ))),
        }
    }

    /// Returns a replay-preparation command for the provided dead-letter identifier.
    pub fn prepare_replay_command(&self, dead_letter_id: DeadLetterId) -> PrepareReplayCommand {
        PrepareReplayCommand {
            dead_letter_id,
            audit_chain_ref: self.audit_chain_ref(),
            approval_ref: self.replay_approval_ref(),
            replay_reason: ReplayReason::new("operator_approved_replay"),
        }
    }

    /// Returns a retry-plan result DTO fixture.
    pub fn retry_plan_result(&self, delivery_id: DeliveryId) -> RetryPlanResult {
        RetryPlanResult {
            retry_plan_id: RetryPlanId::new(format!("retry_plan_{}", self.run.run_id)),
            delivery_id,
            retry_status: RetryPlanStatus::Scheduled,
            remaining_attempts: AttemptCount::new(3),
            next_run_at: Timestamp::new("2026-05-31T00:05:00Z"),
            audit_ref: crate::metadata::AuditRef::new(format!("audit_{}", self.run.run_id)),
        }
    }

    /// Returns a dead-letter result DTO fixture.
    pub fn dead_letter_result(&self, delivery_id: DeliveryId) -> DeadLetterResult {
        DeadLetterResult {
            dead_letter_id: self.dead_letter_id(),
            delivery_id,
            dead_letter_status: DeadLetterStatus::Open,
            failure_material_ref: self.failure_material_ref(),
            audit_ref: crate::metadata::AuditRef::new(format!("audit_{}", self.run.run_id)),
        }
    }

    /// Returns a replay-preparation result DTO fixture.
    pub fn replay_preparation_result(
        &self,
        dead_letter_id: DeadLetterId,
    ) -> ReplayPreparationResult {
        ReplayPreparationResult {
            replay_preparation_id: ReplayPreparationId::new(format!(
                "replay_preparation_{}",
                self.run.run_id
            )),
            dead_letter_id,
            replay_preparation_status: ReplayPreparationStatus::Ready,
            audit_ref: crate::metadata::AuditRef::new(format!("audit_{}", self.run.run_id)),
        }
    }

    /// Returns a retry-cycle job input DTO fixture.
    pub fn run_retry_cycle_job(&self) -> RunRetryCycleJob {
        RunRetryCycleJob {
            job_run_id: JobRunId::new(format!("job_run_{}", self.run.run_id)),
            cursor: self.retry_cursor(),
            batch_size: 50,
            now: Timestamp::new("2026-05-31T00:05:00Z"),
        }
    }

    /// Returns a retry-cycle summary DTO fixture.
    pub fn retry_cycle_result(&self) -> RetryCycleResult {
        RetryCycleResult {
            job_run_id: JobRunId::new(format!("job_run_{}", self.run.run_id)),
            scanned: 3,
            retried: 2,
            exhausted: 1,
            next_cursor: RetryScanCursor::new(format!("retry_cursor_next_{}", self.run.run_id)),
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
    use crate::receipts::{
        BackendSignalResult, DeadLetterResult, FeedbackRecordResult, PublicationAcceptanceResult,
        ReplayPreparationResult, RetryPlanResult, TimeoutRecordResult,
    };

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
    fn request_retry_command_roundtrip() {
        let run = TestRunBuilder::new("rec-001").build();
        let delivery_builder = DeliveryFixtureBuilder::new(run.clone());
        let builder = RecoveryFixtureBuilder::new(run);
        let command = builder.request_retry_command(delivery_builder.delivery_id());

        roundtrip(&command);
    }

    #[test]
    fn move_delivery_to_dead_letter_command_roundtrip() {
        let run = TestRunBuilder::new("rec-002").build();
        let delivery_builder = DeliveryFixtureBuilder::new(run.clone());
        let builder = RecoveryFixtureBuilder::new(run);
        let command = builder.move_to_dead_letter_command(delivery_builder.delivery_id());

        roundtrip(&command);
    }

    #[test]
    fn prepare_replay_command_roundtrip() {
        let run = TestRunBuilder::new("rec-003").build();
        let builder = RecoveryFixtureBuilder::new(run);
        let command = builder.prepare_replay_command(builder.dead_letter_id());

        roundtrip(&command);
    }

    #[test]
    fn retry_plan_result_roundtrip() {
        let run = TestRunBuilder::new("rec-004").build();
        let delivery_builder = DeliveryFixtureBuilder::new(run.clone());
        let builder = RecoveryFixtureBuilder::new(run);
        let result: RetryPlanResult = builder.retry_plan_result(delivery_builder.delivery_id());

        roundtrip(&result);
    }

    #[test]
    fn dead_letter_result_roundtrip() {
        let run = TestRunBuilder::new("rec-005").build();
        let delivery_builder = DeliveryFixtureBuilder::new(run.clone());
        let builder = RecoveryFixtureBuilder::new(run);
        let result: DeadLetterResult = builder.dead_letter_result(delivery_builder.delivery_id());

        roundtrip(&result);
    }

    #[test]
    fn replay_preparation_result_roundtrip() {
        let run = TestRunBuilder::new("rec-006").build();
        let builder = RecoveryFixtureBuilder::new(run);
        let result: ReplayPreparationResult =
            builder.replay_preparation_result(builder.dead_letter_id());

        roundtrip(&result);
    }

    #[test]
    fn backend_delivery_signal_input_roundtrip() {
        let run = TestRunBuilder::new("sig-001").build();
        let backend_builder = BackendFixtureBuilder::new(run.clone());
        let delivery_builder = DeliveryFixtureBuilder::new(run.clone());
        let builder = FeedbackFixtureBuilder::new(run);
        let input = builder.delivered_backend_signal(
            delivery_builder.delivery_id(),
            DeliveryAttemptId::new("attempt_sig_001"),
            backend_builder.in_memory_capability(),
        );

        roundtrip(&input);
    }

    #[test]
    fn delivery_timeout_signal_input_roundtrip() {
        let run = TestRunBuilder::new("sig-002").build();
        let delivery_builder = DeliveryFixtureBuilder::new(run.clone());
        let builder = FeedbackFixtureBuilder::new(run);
        let input = builder.timeout_signal(
            delivery_builder.delivery_id(),
            DeliveryAttemptId::new("attempt_sig_002"),
        );

        roundtrip(&input);
    }

    #[test]
    fn backend_signal_result_roundtrip() {
        let run = TestRunBuilder::new("sig-003").build();
        let delivery_builder = DeliveryFixtureBuilder::new(run.clone());
        let builder = FeedbackFixtureBuilder::new(run);
        let receipt: BackendSignalResult = builder.backend_signal_result(
            delivery_builder.delivery_id(),
            DeliveryAttemptId::new("attempt_sig_003"),
        );

        roundtrip(&receipt);
    }

    #[test]
    fn timeout_record_result_roundtrip() {
        let run = TestRunBuilder::new("sig-004").build();
        let delivery_builder = DeliveryFixtureBuilder::new(run.clone());
        let builder = FeedbackFixtureBuilder::new(run);
        let receipt: TimeoutRecordResult =
            builder.timeout_record_result(delivery_builder.delivery_id());

        roundtrip(&receipt);
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
    fn run_retry_cycle_job_roundtrip() {
        let run = TestRunBuilder::new("job-005").build();
        let builder = RecoveryFixtureBuilder::new(run);

        roundtrip(&builder.run_retry_cycle_job());
    }

    #[test]
    fn retry_cycle_result_roundtrip() {
        let run = TestRunBuilder::new("job-006").build();
        let builder = RecoveryFixtureBuilder::new(run);

        roundtrip(&builder.retry_cycle_result());
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
