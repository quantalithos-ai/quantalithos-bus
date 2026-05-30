//! Reusable fixture builders for publication contract and domain tests.

use crate::commands::AcceptPublicationCommand;
use crate::metadata::{
    ActorContext, ActorKind, ActorRef, CommandMetadata, CoreEventRef, DeliveryMode, PayloadDigest,
    PayloadKind, PayloadRef, RequestId, RequestMetadata, RequestOrigin, SourceRecordRef,
    SourceSystem, TargetScope, Timestamp, TraceId,
};

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

#[cfg(test)]
mod tests {
    use serde::Serialize;
    use serde::de::DeserializeOwned;

    use super::*;
    use crate::metadata::{
        AuditRef, PublicationAcceptanceStatus, PublicationId, RejectionReasonRef,
    };
    use crate::receipts::PublicationAcceptanceResult;

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
}
