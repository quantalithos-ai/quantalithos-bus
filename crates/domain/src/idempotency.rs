//! Idempotency and request-digest domain objects.

use bus_contracts::commands::{AcceptPublicationCommand, RecordDeliveryFeedbackCommand};
use bus_contracts::events::CommittedOutboxFactInput;
use bus_contracts::metadata::{
    FeedbackId, IdempotencyKey, PublicationId, Timestamp, TraceContextRef,
};

use crate::errors::DomainError;

/// Distinguishes which inbound boundary owns an idempotency scope.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum IdempotencyEntryKind {
    /// A synchronous command boundary.
    Command,
    /// An inbound event-consumer boundary.
    Event,
}

/// Declares which business action is protected by idempotency.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum IdempotencyAction {
    /// The publication acceptance command.
    AcceptPublication,
    /// The delivery feedback command.
    RecordDeliveryFeedback,
    /// The committed outbox fact consumer.
    ConsumeCommittedOutboxFact,
}

/// A stable idempotency scope.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct IdempotencyScope {
    /// The inbound boundary kind.
    pub entry_kind: IdempotencyEntryKind,
    /// The protected business action.
    pub action: IdempotencyAction,
    /// The optional logical boundary reference.
    pub boundary_ref: Option<String>,
}

impl IdempotencyScope {
    /// Builds the command scope for `AcceptPublication`.
    pub fn for_accept_publication_command(command: &AcceptPublicationCommand) -> Self {
        Self {
            entry_kind: IdempotencyEntryKind::Command,
            action: IdempotencyAction::AcceptPublication,
            boundary_ref: Some(format!(
                "{}::{}",
                command.target_scope.project_id, command.target_scope.topic
            )),
        }
    }

    /// Builds the command scope for `RecordDeliveryFeedback`.
    pub fn for_record_delivery_feedback(command: &RecordDeliveryFeedbackCommand) -> Self {
        Self {
            entry_kind: IdempotencyEntryKind::Command,
            action: IdempotencyAction::RecordDeliveryFeedback,
            boundary_ref: Some(command.delivery_id.as_str().to_owned()),
        }
    }

    /// Builds the event scope for `ConsumeCommittedOutboxFact`.
    pub fn for_outbox_fact(input: &CommittedOutboxFactInput) -> Self {
        Self {
            entry_kind: IdempotencyEntryKind::Event,
            action: IdempotencyAction::ConsumeCommittedOutboxFact,
            boundary_ref: Some(format!(
                "{}::{}",
                input.source_ref.as_str(),
                input.event_id.as_str()
            )),
        }
    }
}

/// The algorithm version used to derive a request digest.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DigestAlgorithmVersion {
    /// Stable FNV-1a 64-bit hashing over the canonical JSON payload.
    Fnv1a64V1,
}

/// A stable digest used to compare repeated requests.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestDigest {
    /// The canonical digest value.
    pub value: String,
    /// The digest algorithm version.
    pub algorithm_version: DigestAlgorithmVersion,
}

impl RequestDigest {
    /// Computes the request digest for `AcceptPublication`.
    pub fn from_accept_publication_command(
        command: &AcceptPublicationCommand,
    ) -> Result<Self, DomainError> {
        let payload = serde_json::to_vec(command).map_err(|_| DomainError::InvalidRequestDigest)?;
        Ok(Self::from_canonical_payload(payload))
    }

    /// Computes the request digest for `RecordDeliveryFeedback`.
    pub fn from_record_delivery_feedback_command(
        command: &RecordDeliveryFeedbackCommand,
    ) -> Result<Self, DomainError> {
        let payload = serde_json::to_vec(command).map_err(|_| DomainError::InvalidRequestDigest)?;
        Ok(Self::from_canonical_payload(payload))
    }

    /// Computes the request digest for `ConsumeCommittedOutboxFact`.
    pub fn from_outbox_fact_input(input: &CommittedOutboxFactInput) -> Result<Self, DomainError> {
        let payload = serde_json::to_vec(input).map_err(|_| DomainError::InvalidRequestDigest)?;
        Ok(Self::from_canonical_payload(payload))
    }

    fn from_canonical_payload(payload: Vec<u8>) -> Self {
        let mut hash = 0xcbf29ce484222325_u64;
        for byte in payload {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }

        Self {
            value: format!("{hash:016x}"),
            algorithm_version: DigestAlgorithmVersion::Fnv1a64V1,
        }
    }
}

/// A stable local record reference bound to an idempotency anchor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecordRef {
    /// A publication acceptance result reference.
    Publication(PublicationId),
    /// A feedback result reference.
    Feedback(FeedbackId),
}

/// A committed idempotency anchor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdempotencyAnchor {
    /// The stable anchor identifier.
    pub anchor_id: String,
    /// The protected scope.
    pub scope: IdempotencyScope,
    /// The protected key.
    pub key: IdempotencyKey,
    /// The digest of the committed request.
    pub request_digest: RequestDigest,
    /// The committed result reference.
    pub bound_record_ref: RecordRef,
    /// The anchor creation timestamp.
    pub created_at: Timestamp,
    /// The trace reference attached to the anchor.
    pub trace_ref: TraceContextRef,
}

impl IdempotencyAnchor {
    /// Binds an idempotency key to a committed publication result.
    pub fn bind(
        anchor_id: String,
        scope: IdempotencyScope,
        key: IdempotencyKey,
        request_digest: RequestDigest,
        record_ref: RecordRef,
        created_at: Timestamp,
        trace_ref: TraceContextRef,
    ) -> Self {
        Self {
            anchor_id,
            scope,
            key,
            request_digest,
            bound_record_ref: record_ref,
            created_at,
            trace_ref,
        }
    }

    /// Returns whether the stored digest matches the incoming request.
    pub fn matches(&self, digest: &RequestDigest) -> bool {
        &self.request_digest == digest
    }

    /// Returns whether the anchor is bound to the provided record.
    pub fn is_bound_to(&self, record_ref: &RecordRef) -> bool {
        &self.bound_record_ref == record_ref
    }
}

/// A persisted idempotency conflict summary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdempotencyConflict {
    /// The protected scope.
    pub scope: IdempotencyScope,
    /// The reused idempotency key.
    pub key: IdempotencyKey,
    /// The previously committed digest.
    pub existing_digest: RequestDigest,
    /// The incoming conflicting digest.
    pub incoming_digest: RequestDigest,
    /// The conflict timestamp.
    pub occurred_at: Timestamp,
    /// The trace reference attached to the conflicting request.
    pub trace_ref: TraceContextRef,
}

#[cfg(test)]
mod tests {
    use bus_contracts::fixtures::{
        DeliveryFixtureBuilder, FeedbackFixtureBuilder, OutboxFixtureBuilder,
        PublicationFixtureBuilder, TestRunBuilder,
    };

    use super::*;

    #[test]
    fn outbox_fact_scope_uses_source_and_event_identity() {
        let run = TestRunBuilder::new("idem-001").build();
        let builder = OutboxFixtureBuilder::new(run);
        let input = builder.committed_fact_input();

        let scope = IdempotencyScope::for_outbox_fact(&input);

        assert_eq!(scope.entry_kind, IdempotencyEntryKind::Event);
        assert_eq!(scope.action, IdempotencyAction::ConsumeCommittedOutboxFact);
        assert_eq!(
            scope.boundary_ref,
            Some(format!(
                "{}::{}",
                input.source_ref.as_str(),
                input.event_id.as_str()
            ))
        );
    }

    #[test]
    fn outbox_fact_digest_changes_with_fact_content() {
        let run = TestRunBuilder::new("idem-002").build();
        let builder = OutboxFixtureBuilder::new(run.clone());
        let first = builder.committed_fact_input();
        let mut second = builder.committed_fact_input();
        second.payload_digest = bus_contracts::metadata::PayloadDigest::new("sha256:changed");

        let first_digest =
            RequestDigest::from_outbox_fact_input(&first).expect("digest should be computed");
        let second_digest =
            RequestDigest::from_outbox_fact_input(&second).expect("digest should be computed");

        assert_ne!(first_digest, second_digest);

        let command_builder = PublicationFixtureBuilder::new(run);
        let command_digest =
            RequestDigest::from_accept_publication_command(&command_builder.valid_material())
                .expect("command digest should be computed");
        assert_ne!(first_digest, command_digest);
    }

    #[test]
    fn feedback_command_scope_and_digest_follow_delivery_identity() {
        let run = TestRunBuilder::new("idem-003").build();
        let delivery_builder = DeliveryFixtureBuilder::new(run.clone());
        let feedback_builder = FeedbackFixtureBuilder::new(run);
        let command =
            feedback_builder.ack_command(delivery_builder.delivery_id(), "attempt_idem_003".into());

        let scope = IdempotencyScope::for_record_delivery_feedback(&command);
        let first_digest = RequestDigest::from_record_delivery_feedback_command(&command)
            .expect("feedback digest should be computed");
        let mut second = command.clone();
        second.external_feedback_ref = "external_feedback_changed".into();
        let second_digest = RequestDigest::from_record_delivery_feedback_command(&second)
            .expect("feedback digest should be computed");

        assert_eq!(scope.entry_kind, IdempotencyEntryKind::Command);
        assert_eq!(scope.action, IdempotencyAction::RecordDeliveryFeedback);
        assert_eq!(
            scope.boundary_ref,
            Some(command.delivery_id.as_str().to_owned())
        );
        assert_ne!(first_digest, second_digest);
    }
}
