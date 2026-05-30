//! Feedback results and source identity for committed delivery outcomes.

use bus_contracts::commands::RecordDeliveryFeedbackCommand;
use bus_contracts::metadata::{
    ActorContext, AuditRef, DeliveryAttemptId, DeliveryId, DeliveryStatus, ExternalFeedbackRef,
    FailureReason, FeedbackId, FeedbackKind, FeedbackReason, FeedbackStatus, Timestamp, Version,
};

use crate::errors::DomainError;

/// The stable source identity for one feedback result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeedbackSource {
    /// The delivery attempt that produced the feedback.
    pub attempt_id: DeliveryAttemptId,
    /// The stable external feedback reference.
    pub external_feedback_ref: ExternalFeedbackRef,
}

impl FeedbackSource {
    /// Builds a feedback source identity from protocol references.
    pub fn new(
        attempt_id: DeliveryAttemptId,
        external_feedback_ref: ExternalFeedbackRef,
    ) -> Result<Self, DomainError> {
        if attempt_id.as_str().trim().is_empty() {
            return Err(DomainError::InvalidFeedbackResult("attempt_id"));
        }
        if external_feedback_ref.as_str().trim().is_empty() {
            return Err(DomainError::InvalidFeedbackResult("external_feedback_ref"));
        }

        Ok(Self {
            attempt_id,
            external_feedback_ref,
        })
    }
}

/// The committed bus-level feedback truth for one delivery attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeedbackResult {
    /// The stable feedback identifier.
    pub feedback_id: FeedbackId,
    /// The associated delivery identifier.
    pub delivery_id: DeliveryId,
    /// The normalized feedback result status.
    pub status: FeedbackStatus,
    /// The optional normalized feedback reason.
    pub reason: Option<FeedbackReason>,
    /// The observed timestamp attached to the feedback.
    pub observed_at: Timestamp,
    /// The actor that supplied or normalized the feedback.
    pub actor: ActorContext,
    /// The external source identity for the feedback.
    pub source: FeedbackSource,
    version: Version,
    audit_ref: Option<AuditRef>,
}

impl FeedbackResult {
    /// Builds a feedback result from the command protocol.
    pub fn from_command(
        command: RecordDeliveryFeedbackCommand,
        actor: ActorContext,
    ) -> Result<Self, DomainError> {
        let source = FeedbackSource::new(command.attempt_id, command.external_feedback_ref)?;

        match command.feedback_kind {
            FeedbackKind::Ack => Self::ack(
                command.delivery_id,
                source,
                Some(command.feedback_reason),
                actor,
                command.observed_at,
            ),
            FeedbackKind::Fail => Self::fail(
                command.delivery_id,
                source,
                command.feedback_reason,
                actor,
                command.observed_at,
            ),
        }
    }

    /// Creates an acknowledged feedback result.
    pub fn ack(
        delivery_id: DeliveryId,
        source: FeedbackSource,
        reason: Option<FeedbackReason>,
        actor: ActorContext,
        observed_at: Timestamp,
    ) -> Result<Self, DomainError> {
        Self::build(
            delivery_id,
            FeedbackStatus::Ack,
            reason,
            actor,
            observed_at,
            source,
        )
    }

    /// Creates a failed feedback result.
    pub fn fail(
        delivery_id: DeliveryId,
        source: FeedbackSource,
        reason: FeedbackReason,
        actor: ActorContext,
        observed_at: Timestamp,
    ) -> Result<Self, DomainError> {
        Self::build(
            delivery_id,
            FeedbackStatus::Fail,
            Some(reason),
            actor,
            observed_at,
            source,
        )
    }

    fn build(
        delivery_id: DeliveryId,
        status: FeedbackStatus,
        reason: Option<FeedbackReason>,
        actor: ActorContext,
        observed_at: Timestamp,
        source: FeedbackSource,
    ) -> Result<Self, DomainError> {
        if delivery_id.as_str().trim().is_empty() {
            return Err(DomainError::InvalidFeedbackResult("delivery_id"));
        }
        if observed_at.as_str().trim().is_empty() {
            return Err(DomainError::InvalidFeedbackResult("observed_at"));
        }
        if reason
            .as_ref()
            .is_some_and(|candidate| candidate.as_str().trim().is_empty())
        {
            return Err(DomainError::InvalidFeedbackResult("feedback_reason"));
        }

        Ok(Self {
            feedback_id: FeedbackId::new(format!(
                "feedback_{}_{}",
                sanitize(delivery_id.as_str()),
                sanitize(source.external_feedback_ref.as_str())
            )),
            delivery_id,
            status,
            reason,
            observed_at,
            actor,
            source,
            version: 0,
            audit_ref: None,
        })
    }

    /// Returns the committed version used for optimistic writes.
    pub fn version(&self) -> Version {
        self.version
    }

    /// Overwrites the committed version after repository persistence.
    pub fn set_version(&mut self, version: Version) {
        self.version = version;
    }

    /// Attaches the committed audit reference to the feedback truth.
    pub fn attach_audit_ref(&mut self, audit_ref: AuditRef) {
        self.audit_ref = Some(audit_ref);
    }

    /// Returns the attached audit reference, if one exists.
    pub fn audit_ref(&self) -> Option<&AuditRef> {
        self.audit_ref.as_ref()
    }

    /// Returns whether the feedback is a successful acknowledgement.
    pub fn is_success(&self) -> bool {
        self.status == FeedbackStatus::Ack
    }

    /// Returns whether the feedback should drive recovery handling.
    pub fn is_failure(&self) -> bool {
        matches!(self.status, FeedbackStatus::Fail | FeedbackStatus::Timeout)
    }

    /// Returns whether the feedback only indicates a duplicate hit.
    pub fn is_duplicate(&self) -> bool {
        self.status == FeedbackStatus::Duplicate
    }

    /// Returns the delivery status implied by this feedback result.
    pub fn implied_delivery_status(&self) -> DeliveryStatus {
        match self.status {
            FeedbackStatus::Ack => DeliveryStatus::Completed,
            FeedbackStatus::Fail | FeedbackStatus::Timeout => DeliveryStatus::Failed,
            FeedbackStatus::Duplicate => DeliveryStatus::Delivered,
        }
    }

    /// Derives the failure reason that should be written to the delivery truth.
    pub fn failure_reason(&self) -> FailureReason {
        match self.status {
            FeedbackStatus::Fail => self
                .reason
                .clone()
                .map(|reason| FailureReason::new(reason.as_str()))
                .unwrap_or_else(|| FailureReason::new("feedback_failed")),
            FeedbackStatus::Timeout => FailureReason::new("delivery_timeout"),
            FeedbackStatus::Ack | FeedbackStatus::Duplicate => {
                FailureReason::new("feedback_not_failure")
            }
        }
    }
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

#[cfg(test)]
mod tests {
    use bus_contracts::fixtures::{DeliveryFixtureBuilder, FeedbackFixtureBuilder, TestRunBuilder};
    use bus_contracts::metadata::{FeedbackStatus, Timestamp};

    use super::*;

    #[test]
    fn from_command_creates_ack_feedback_result() {
        let run = TestRunBuilder::new("feedback-domain-001").build();
        let delivery_builder = DeliveryFixtureBuilder::new(run.clone());
        let fixture_builder = FeedbackFixtureBuilder::new(run.clone());
        let command = fixture_builder.ack_command(
            delivery_builder.delivery_id(),
            DeliveryAttemptId::new("attempt_feedback_domain_001"),
        );

        let feedback =
            FeedbackResult::from_command(command.clone(), run.actor).expect("feedback should map");

        assert_eq!(feedback.delivery_id, command.delivery_id);
        assert_eq!(feedback.status, FeedbackStatus::Ack);
        assert_eq!(
            feedback.reason,
            Some(FeedbackReason::new("subscriber_processed"))
        );
        assert_eq!(feedback.source.attempt_id, command.attempt_id);
    }

    #[test]
    fn from_command_creates_fail_feedback_result() {
        let run = TestRunBuilder::new("feedback-domain-002").build();
        let delivery_builder = DeliveryFixtureBuilder::new(run.clone());
        let fixture_builder = FeedbackFixtureBuilder::new(run.clone());
        let command = fixture_builder.fail_command(
            delivery_builder.delivery_id(),
            DeliveryAttemptId::new("attempt_feedback_domain_002"),
        );

        let feedback =
            FeedbackResult::from_command(command.clone(), run.actor).expect("feedback should map");

        assert_eq!(feedback.status, FeedbackStatus::Fail);
        assert!(feedback.is_failure());
        assert_eq!(
            feedback.failure_reason(),
            FailureReason::new(command.feedback_reason.as_str())
        );
    }

    #[test]
    fn feedback_result_rejects_blank_external_reference() {
        let run = TestRunBuilder::new("feedback-domain-003").build();
        let source = FeedbackSource::new(DeliveryAttemptId::new("attempt_003"), "".into())
            .expect_err("blank external reference must be rejected");

        assert_eq!(
            source,
            DomainError::InvalidFeedbackResult("external_feedback_ref")
        );
        assert_eq!(
            run.metadata.request.requested_at,
            Timestamp::new("2026-05-30T00:00:00Z")
        );
    }
}
