//! Delivery lifecycle records, attempts, and history entries.

use bus_contracts::metadata::{
    ActorContext, AttemptCount, AttemptNo, BackendCapabilityRef, BackendDeliveryResult,
    BackendDeliveryStatus, DeadLetterId, DeliveryAttemptId, DeliveryAttemptRef, DeliveryHistoryId,
    DeliveryId, DeliveryStatus, DeliveryTransitionRuleRef, FailureReason, HistoryReason,
    IdempotencyKey, PublicationId, SubscriberRef, Timestamp, Version,
};

use crate::errors::DomainError;
use crate::feedback::FeedbackResult;
use crate::publication::TransportSemantic;

/// The bus-owned truth record for a subscriber delivery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeliveryRecord {
    /// The delivery identifier.
    pub delivery_id: DeliveryId,
    /// The associated publication identifier.
    pub publication_id: PublicationId,
    /// The target subscriber reference.
    pub subscriber_ref: SubscriberRef,
    /// The current lifecycle status.
    pub status: DeliveryStatus,
    /// The number of recorded attempts.
    pub attempt_count: AttemptCount,
    /// The bus-level idempotency key.
    pub idempotency_key: IdempotencyKey,
    /// The reference to the latest attempt, if any.
    pub last_attempt_ref: Option<DeliveryAttemptRef>,
    transport_semantic: TransportSemantic,
    version: Version,
    attempts: Vec<DeliveryAttempt>,
    history: Vec<DeliveryHistoryEntry>,
}

impl DeliveryRecord {
    /// Schedules a new delivery from a derived transport semantic.
    pub fn schedule(
        semantic: TransportSemantic,
        subscriber_ref: SubscriberRef,
        key: IdempotencyKey,
    ) -> Result<Self, DomainError> {
        if !semantic.requires_durable_record() {
            return Err(DomainError::NonDurableTransportSemantic);
        }
        if subscriber_ref.as_str().trim().is_empty() {
            return Err(DomainError::InvalidDeliveryRecord("subscriber_ref"));
        }
        if key.as_str().trim().is_empty() {
            return Err(DomainError::InvalidDeliveryRecord("idempotency_key"));
        }

        Ok(Self {
            delivery_id: DeliveryId::new(format!(
                "delivery_{}_{}",
                sanitize(semantic.publication_id.as_str()),
                sanitize(subscriber_ref.as_str())
            )),
            publication_id: semantic.publication_id.clone(),
            subscriber_ref,
            status: DeliveryStatus::Scheduled,
            attempt_count: AttemptCount::default(),
            idempotency_key: key,
            last_attempt_ref: None,
            transport_semantic: semantic,
            version: 0,
            attempts: Vec::new(),
            history: Vec::new(),
        })
    }

    /// Returns the committed aggregate version used for optimistic updates.
    pub fn version(&self) -> Version {
        self.version
    }

    /// Overwrites the aggregate version after a repository rehydrate or save.
    pub fn set_version(&mut self, version: Version) {
        self.version = version;
    }

    /// Returns the platform transport semantic associated with the delivery.
    pub fn transport_semantic(&self) -> &TransportSemantic {
        &self.transport_semantic
    }

    /// Returns the backend capability reference attached to the semantic.
    pub fn backend_capability_ref(&self) -> &BackendCapabilityRef {
        &self.transport_semantic.backend_capability_ref
    }

    /// Returns the committed attempts tracked by the delivery aggregate.
    pub fn attempts(&self) -> &[DeliveryAttempt] {
        &self.attempts
    }

    /// Returns the append-only history tracked by the delivery aggregate.
    pub fn history(&self) -> &[DeliveryHistoryEntry] {
        &self.history
    }

    /// Synchronizes a finished attempt back into the aggregate.
    pub fn sync_attempt(&mut self, attempt: DeliveryAttempt) -> Result<(), DomainError> {
        if attempt.delivery_id != self.delivery_id {
            return Err(DomainError::AttemptDoesNotBelongToDelivery);
        }

        let stored = self
            .attempts
            .iter_mut()
            .find(|candidate| candidate.attempt_id == attempt.attempt_id)
            .ok_or(DomainError::AttemptRefMismatch)?;

        *stored = attempt;
        Ok(())
    }

    /// Finishes the current delivery attempt using a normalized backend result.
    pub fn finish_attempt(
        &mut self,
        attempt_id: &DeliveryAttemptId,
        result: BackendDeliveryResult,
        occurred_at: Timestamp,
    ) -> Result<DeliveryAttempt, DomainError> {
        if self.last_attempt_ref != Some(DeliveryAttemptRef::new(attempt_id.as_str())) {
            return Err(DomainError::AttemptRefMismatch);
        }

        let attempt = self
            .attempts
            .iter_mut()
            .find(|candidate| candidate.attempt_id == *attempt_id)
            .ok_or(DomainError::AttemptRefMismatch)?;
        attempt.finish(result, occurred_at)?;

        Ok(attempt.clone())
    }

    /// Appends a delivery-history entry after a valid state transition.
    pub fn append_history(&mut self, entry: DeliveryHistoryEntry) -> Result<(), DomainError> {
        if entry.delivery_id != self.delivery_id {
            return Err(DomainError::InvalidDeliveryRecord("history.delivery_id"));
        }

        let lifecycle = DeliveryLifecycle::default_for_bus();
        if !lifecycle.requires_history(entry.from_status, entry.to_status) {
            return Err(DomainError::InvalidDeliveryRecord("history.transition"));
        }

        self.history.push(entry);
        Ok(())
    }

    /// Starts a new delivery attempt and moves the record into dispatching.
    pub fn start_attempt(
        &mut self,
        capability_ref: BackendCapabilityRef,
        occurred_at: Timestamp,
    ) -> Result<DeliveryAttempt, DomainError> {
        if capability_ref.capability_id.as_str().trim().is_empty() {
            return Err(DomainError::InvalidDeliveryRecord("backend_capability_ref"));
        }

        match self.status {
            DeliveryStatus::Completed | DeliveryStatus::DeadLettered => {
                return Err(DomainError::TerminalDeliveryStateReopenRejected);
            }
            DeliveryStatus::Scheduled => {}
            current => {
                return Err(DomainError::InvalidDeliveryTransition {
                    from: current,
                    to: DeliveryStatus::Dispatching,
                });
            }
        }

        let attempt_no = self.attempt_count.next_attempt_no();
        let attempt = DeliveryAttempt::start(self.delivery_id.clone(), attempt_no, occurred_at);
        self.attempt_count = self.attempt_count.increment();
        self.last_attempt_ref = Some(DeliveryAttemptRef::new(attempt.attempt_id.as_str()));
        self.status = DeliveryStatus::Dispatching;
        self.attempts.push(attempt.clone());

        Ok(attempt)
    }

    /// Marks the current dispatching delivery as delivered.
    pub fn mark_delivered(
        &mut self,
        attempt: DeliveryAttempt,
        _actor: ActorContext,
    ) -> Result<(), DomainError> {
        if self.status != DeliveryStatus::Dispatching {
            return Err(DomainError::InvalidDeliveryTransition {
                from: self.status,
                to: DeliveryStatus::Delivered,
            });
        }
        if attempt.delivery_id != self.delivery_id {
            return Err(DomainError::AttemptDoesNotBelongToDelivery);
        }
        if self.last_attempt_ref != Some(DeliveryAttemptRef::new(attempt.attempt_id.as_str())) {
            return Err(DomainError::AttemptRefMismatch);
        }
        if !attempt.is_finished() {
            return Err(DomainError::AttemptNotFinished);
        }
        if attempt.result_status != Some(BackendDeliveryStatus::Delivered) {
            return Err(DomainError::AttemptOutcomeDoesNotDeliver);
        }

        self.sync_attempt(attempt)?;
        self.status = DeliveryStatus::Delivered;
        Ok(())
    }

    /// Marks the delivery as failed after a normalized failure reason exists.
    pub fn mark_failed(
        &mut self,
        reason: FailureReason,
        _actor: ActorContext,
    ) -> Result<(), DomainError> {
        if reason.as_str().trim().is_empty() {
            return Err(DomainError::InvalidDeliveryRecord("failure_reason"));
        }
        if matches!(
            self.status,
            DeliveryStatus::Completed | DeliveryStatus::DeadLettered
        ) {
            return Err(DomainError::TerminalDeliveryStateReopenRejected);
        }
        if !self.can_transition_to(DeliveryStatus::Failed) {
            return Err(DomainError::InvalidDeliveryTransition {
                from: self.status,
                to: DeliveryStatus::Failed,
            });
        }

        self.status = DeliveryStatus::Failed;
        Ok(())
    }

    /// Marks the failed delivery as dead-lettered by one recovery flow.
    pub fn mark_dead_lettered(
        &mut self,
        dead_letter_id: DeadLetterId,
        _actor: ActorContext,
    ) -> Result<(), DomainError> {
        if dead_letter_id.as_str().trim().is_empty() {
            return Err(DomainError::InvalidDeliveryRecord("dead_letter_id"));
        }
        if self.status != DeliveryStatus::Failed {
            return Err(DomainError::DeadLetterNotAllowed);
        }
        if !self.can_transition_to(DeliveryStatus::DeadLettered) {
            return Err(DomainError::InvalidDeliveryTransition {
                from: self.status,
                to: DeliveryStatus::DeadLettered,
            });
        }

        self.status = DeliveryStatus::DeadLettered;
        Ok(())
    }

    /// Marks the delivery as completed using a committed ack feedback result.
    pub fn mark_completed(
        &mut self,
        feedback: FeedbackResult,
        _actor: ActorContext,
    ) -> Result<(), DomainError> {
        if feedback.delivery_id != self.delivery_id {
            return Err(DomainError::InvalidFeedbackResult("feedback.delivery_id"));
        }
        if !feedback.is_success() {
            return Err(DomainError::InvalidFeedbackResult("feedback.status"));
        }
        match self.status {
            DeliveryStatus::Delivered => {}
            DeliveryStatus::Completed | DeliveryStatus::DeadLettered => {
                return Err(DomainError::TerminalDeliveryStateReopenRejected);
            }
            current => {
                return Err(DomainError::InvalidDeliveryTransition {
                    from: current,
                    to: DeliveryStatus::Completed,
                });
            }
        }

        let attempt_ref = DeliveryAttemptRef::new(feedback.source.attempt_id.as_str());
        if self.last_attempt_ref != Some(attempt_ref.clone()) {
            return Err(DomainError::AttemptRefMismatch);
        }

        let attempt = self
            .attempts
            .iter()
            .find(|candidate| candidate.attempt_id == feedback.source.attempt_id)
            .ok_or(DomainError::AttemptRefMismatch)?;
        if !attempt.is_finished() {
            return Err(DomainError::AttemptNotFinished);
        }
        if attempt.result_status != Some(BackendDeliveryStatus::Delivered) {
            return Err(DomainError::AttemptOutcomeDoesNotDeliver);
        }

        self.status = DeliveryStatus::Completed;
        Ok(())
    }

    /// Returns whether the record may transition to the provided target status.
    pub fn can_transition_to(&self, target_status: DeliveryStatus) -> bool {
        DeliveryLifecycle::default_for_bus().can_transition(self.status, target_status)
    }
}

/// A single dispatch attempt for a delivery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeliveryAttempt {
    /// The attempt identifier.
    pub attempt_id: DeliveryAttemptId,
    /// The parent delivery identifier.
    pub delivery_id: DeliveryId,
    /// The one-based attempt number within the delivery.
    pub attempt_no: AttemptNo,
    /// The normalized backend delivery reference, if any.
    pub backend_ref: Option<bus_contracts::metadata::BackendDeliveryRef>,
    /// The attempt start timestamp.
    pub started_at: Timestamp,
    /// The attempt finish timestamp, if the attempt already finished.
    pub finished_at: Option<Timestamp>,
    result_status: Option<BackendDeliveryStatus>,
}

impl DeliveryAttempt {
    /// Starts a new delivery attempt.
    pub fn start(
        delivery_id: DeliveryId,
        attempt_no: AttemptNo,
        started_at: Timestamp,
    ) -> DeliveryAttempt {
        DeliveryAttempt {
            attempt_id: DeliveryAttemptId::new(format!(
                "attempt_{}_{}",
                sanitize(delivery_id.as_str()),
                attempt_no.get()
            )),
            delivery_id,
            attempt_no,
            backend_ref: None,
            started_at,
            finished_at: None,
            result_status: None,
        }
    }

    /// Finishes the attempt using a normalized backend result.
    pub fn finish(
        &mut self,
        result: BackendDeliveryResult,
        occurred_at: Timestamp,
    ) -> Result<(), DomainError> {
        if self.is_finished() {
            return Err(DomainError::AttemptAlreadyFinished);
        }
        if occurred_at.as_str() < self.started_at.as_str() {
            return Err(DomainError::AttemptFinishedBeforeStart);
        }

        self.backend_ref = result.backend_ref;
        self.finished_at = Some(occurred_at);
        self.result_status = Some(result.status);

        Ok(())
    }

    /// Returns whether the attempt already finished.
    pub fn is_finished(&self) -> bool {
        self.finished_at.is_some()
    }
}

/// Delivery transition policy for the P0 bus lifecycle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeliveryLifecycle {
    /// The rule-set reference bound to the policy instance.
    pub allowed_transitions_ref: DeliveryTransitionRuleRef,
}

impl DeliveryLifecycle {
    /// Builds the default delivery lifecycle rules for the bus.
    pub fn default_for_bus() -> Self {
        Self {
            allowed_transitions_ref: DeliveryTransitionRuleRef::new(
                "policy.bus.default_delivery_transitions",
            ),
        }
    }

    /// Returns whether the provided state transition is allowed.
    pub fn can_transition(&self, from_status: DeliveryStatus, to_status: DeliveryStatus) -> bool {
        matches!(
            (from_status, to_status),
            (DeliveryStatus::Scheduled, DeliveryStatus::Dispatching)
                | (DeliveryStatus::Scheduled, DeliveryStatus::Failed)
                | (DeliveryStatus::Dispatching, DeliveryStatus::Delivered)
                | (DeliveryStatus::Dispatching, DeliveryStatus::Failed)
                | (DeliveryStatus::Delivered, DeliveryStatus::Completed)
                | (DeliveryStatus::Delivered, DeliveryStatus::Failed)
                | (DeliveryStatus::Failed, DeliveryStatus::Scheduled)
                | (DeliveryStatus::Failed, DeliveryStatus::DeadLettered)
        )
    }

    /// Returns whether the delivery is in a terminal state that must not be reopened.
    pub fn rejects_reopen(&self, delivery: DeliveryRecord) -> bool {
        matches!(
            delivery.status,
            DeliveryStatus::Completed | DeliveryStatus::DeadLettered
        )
    }

    /// Returns whether the transition must emit a history entry.
    pub fn requires_history(&self, from_status: DeliveryStatus, to_status: DeliveryStatus) -> bool {
        self.can_transition(from_status, to_status) && from_status != to_status
    }
}

/// An append-only history entry for a delivery state transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeliveryHistoryEntry {
    /// The history entry identifier.
    pub history_id: DeliveryHistoryId,
    /// The target delivery identifier.
    pub delivery_id: DeliveryId,
    /// The source delivery status.
    pub from_status: DeliveryStatus,
    /// The target delivery status.
    pub to_status: DeliveryStatus,
    /// The stable transition reason.
    pub reason: HistoryReason,
    /// The transition timestamp.
    pub occurred_at: Timestamp,
}

impl DeliveryHistoryEntry {
    /// Creates a new append-only history entry for a delivery transition.
    pub fn transition(
        delivery_id: DeliveryId,
        from_status: DeliveryStatus,
        to_status: DeliveryStatus,
        reason: HistoryReason,
        occurred_at: Timestamp,
    ) -> DeliveryHistoryEntry {
        DeliveryHistoryEntry {
            history_id: DeliveryHistoryId::new(format!(
                "history_{}_{}_{}_{}",
                sanitize(delivery_id.as_str()),
                format!("{from_status:?}").to_ascii_lowercase(),
                format!("{to_status:?}").to_ascii_lowercase(),
                sanitize(occurred_at.as_str())
            )),
            delivery_id,
            from_status,
            to_status,
            reason,
            occurred_at,
        }
    }

    /// Returns whether the entry describes the provided transition.
    pub fn describes_transition(
        &self,
        from_status: DeliveryStatus,
        to_status: DeliveryStatus,
    ) -> bool {
        self.from_status == from_status && self.to_status == to_status
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
    use bus_contracts::fixtures::{
        BackendFixtureBuilder, PublicationFixtureBuilder, TestRunBuilder,
    };
    use bus_contracts::metadata::{ActorContext, HistoryReason, SubscriberScope};

    use super::*;
    use crate::feedback::{FeedbackResult, FeedbackSource};
    use crate::publication::{PublicationMaterial, TransportSemantic};

    fn scheduled_record() -> (DeliveryRecord, ActorContext, BackendCapabilityRef) {
        let run = TestRunBuilder::new("delivery-domain-001").build();
        let actor = run.actor.clone();
        let publication_builder = PublicationFixtureBuilder::new(run.clone());
        let backend_builder = BackendFixtureBuilder::new(run.clone());
        let material = PublicationMaterial::from_accept_publication_command(
            publication_builder.valid_material(),
            actor.clone(),
            run.metadata,
        )
        .expect("fixture should create valid material");
        let capability_ref = backend_builder.in_memory_capability();
        let semantic = TransportSemantic::derive(
            material,
            capability_ref.clone(),
            SubscriberScope {
                project_id: format!("project_{}", run.run_id),
                topic: format!("workitem.events.{}", run.run_id),
            },
        )
        .expect("fixture should derive semantic");

        let record = DeliveryRecord::schedule(
            semantic,
            SubscriberRef::new("subscriber_alpha"),
            "idem-delivery-domain-001".into(),
        )
        .expect("fixture should schedule delivery");

        (record, actor, capability_ref)
    }

    #[test]
    fn delivery_record_starts_scheduled() {
        let (record, _, _) = scheduled_record();

        assert_eq!(record.status, DeliveryStatus::Scheduled);
        assert_eq!(record.attempt_count, AttemptCount::new(0));
        assert!(record.last_attempt_ref.is_none());
    }

    #[test]
    fn delivery_record_starts_attempt_and_marks_delivered() {
        let (mut record, actor, capability_ref) = scheduled_record();
        let mut attempt = record
            .start_attempt(capability_ref, Timestamp::new("2026-05-30T00:00:01Z"))
            .expect("scheduled delivery should start");

        assert_eq!(record.status, DeliveryStatus::Dispatching);
        assert_eq!(record.attempt_count, AttemptCount::new(1));
        assert_eq!(
            record.last_attempt_ref,
            Some(DeliveryAttemptRef::new(attempt.attempt_id.as_str()))
        );

        attempt
            .finish(
                BackendDeliveryResult::delivered(Some("backend_delivery_001".into())),
                Timestamp::new("2026-05-30T00:00:02Z"),
            )
            .expect("attempt should finish");

        record
            .mark_delivered(attempt, actor)
            .expect("dispatching delivery should become delivered");

        assert_eq!(record.status, DeliveryStatus::Delivered);
    }

    #[test]
    fn delivery_record_marks_failed_from_dispatching() {
        let (mut record, actor, capability_ref) = scheduled_record();

        record
            .start_attempt(capability_ref, Timestamp::new("2026-05-30T00:00:01Z"))
            .expect("scheduled delivery should start");
        record
            .mark_failed(FailureReason::backend_unavailable(), actor)
            .expect("dispatching delivery may fail");

        assert_eq!(record.status, DeliveryStatus::Failed);
    }

    #[test]
    fn delivery_record_marks_dead_lettered_from_failed() {
        let (mut record, actor, capability_ref) = scheduled_record();

        record
            .start_attempt(capability_ref, Timestamp::new("2026-05-30T00:00:01Z"))
            .expect("scheduled delivery should start");
        record
            .mark_failed(FailureReason::dispatch_failed(), actor.clone())
            .expect("dispatching delivery may fail");
        record
            .mark_dead_lettered(DeadLetterId::new("dead_letter_001"), actor)
            .expect("failed delivery may enter dead-lettered state");

        assert_eq!(record.status, DeliveryStatus::DeadLettered);
    }

    #[test]
    fn delivery_record_rejects_dead_letter_before_failure() {
        let (mut record, actor, capability_ref) = scheduled_record();

        record
            .start_attempt(capability_ref, Timestamp::new("2026-05-30T00:00:01Z"))
            .expect("scheduled delivery should start");
        let error = record
            .mark_dead_lettered(DeadLetterId::new("dead_letter_002"), actor)
            .expect_err("non-failed delivery must not enter dead letter");

        assert_eq!(error, DomainError::DeadLetterNotAllowed);
    }

    #[test]
    fn delivery_record_marks_completed_from_ack_feedback() {
        let (mut record, actor, capability_ref) = scheduled_record();
        let mut attempt = record
            .start_attempt(capability_ref, Timestamp::new("2026-05-30T00:00:01Z"))
            .expect("scheduled delivery should start");
        attempt
            .finish(
                BackendDeliveryResult::delivered(Some("backend_delivery_003".into())),
                Timestamp::new("2026-05-30T00:00:02Z"),
            )
            .expect("attempt should finish");
        record
            .mark_delivered(attempt.clone(), actor.clone())
            .expect("dispatching delivery should become delivered");
        let feedback = FeedbackResult::ack(
            record.delivery_id.clone(),
            FeedbackSource::new(attempt.attempt_id.clone(), "external_feedback_003".into())
                .expect("feedback source should be valid"),
            Some("subscriber_processed".into()),
            actor.clone(),
            Timestamp::new("2026-05-30T00:00:03Z"),
        )
        .expect("ack feedback should be valid");

        record
            .mark_completed(feedback, actor)
            .expect("delivered delivery should complete");

        assert_eq!(record.status, DeliveryStatus::Completed);
    }

    #[test]
    fn delivery_record_rejects_completion_from_old_attempt_feedback() {
        let (mut record, actor, capability_ref) = scheduled_record();
        let mut attempt = record
            .start_attempt(capability_ref, Timestamp::new("2026-05-30T00:00:01Z"))
            .expect("scheduled delivery should start");
        attempt
            .finish(
                BackendDeliveryResult::delivered(Some("backend_delivery_004".into())),
                Timestamp::new("2026-05-30T00:00:02Z"),
            )
            .expect("attempt should finish");
        record
            .mark_delivered(attempt, actor.clone())
            .expect("dispatching delivery should become delivered");
        let feedback = FeedbackResult::ack(
            record.delivery_id.clone(),
            FeedbackSource::new(
                "attempt_feedback_old".into(),
                "external_feedback_004".into(),
            )
            .expect("feedback source should be valid"),
            Some("subscriber_processed".into()),
            actor.clone(),
            Timestamp::new("2026-05-30T00:00:03Z"),
        )
        .expect("ack feedback should be valid");

        let error = record
            .mark_completed(feedback, actor)
            .expect_err("feedback for an old attempt must conflict");

        assert_eq!(error, DomainError::AttemptRefMismatch);
    }

    #[test]
    fn delivery_record_rejects_skip_attempt_transition() {
        let (mut record, actor, _) = scheduled_record();
        let attempt = DeliveryAttempt::start(
            record.delivery_id.clone(),
            AttemptNo::new(1),
            Timestamp::new("2026-05-30T00:00:01Z"),
        );

        let error = record
            .mark_delivered(attempt, actor)
            .expect_err("scheduled delivery cannot skip dispatching");

        assert_eq!(
            error,
            DomainError::InvalidDeliveryTransition {
                from: DeliveryStatus::Scheduled,
                to: DeliveryStatus::Delivered,
            }
        );
    }

    #[test]
    fn delivery_record_rejects_reopening_after_delivered() {
        let (mut record, actor, capability_ref) = scheduled_record();
        let mut attempt = record
            .start_attempt(
                capability_ref.clone(),
                Timestamp::new("2026-05-30T00:00:01Z"),
            )
            .expect("scheduled delivery should start");
        attempt
            .finish(
                BackendDeliveryResult::delivered(Some("backend_delivery_002".into())),
                Timestamp::new("2026-05-30T00:00:02Z"),
            )
            .expect("attempt should finish");
        record
            .mark_delivered(attempt, actor)
            .expect("dispatching delivery should become delivered");

        let error = record
            .start_attempt(capability_ref, Timestamp::new("2026-05-30T00:00:03Z"))
            .expect_err("delivered delivery cannot reopen without feedback or retry flow");

        assert_eq!(
            error,
            DomainError::InvalidDeliveryTransition {
                from: DeliveryStatus::Delivered,
                to: DeliveryStatus::Dispatching,
            }
        );
    }

    #[test]
    fn delivery_attempt_rejects_finishing_twice() {
        let (record, _, _) = scheduled_record();
        let mut attempt = DeliveryAttempt::start(
            record.delivery_id,
            AttemptNo::new(1),
            Timestamp::new("2026-05-30T00:00:01Z"),
        );
        let first_result = BackendDeliveryResult::failed(Some("backend_delivery_003".into()));

        attempt
            .finish(first_result, Timestamp::new("2026-05-30T00:00:02Z"))
            .expect("first finish should succeed");
        let error = attempt
            .finish(
                BackendDeliveryResult::delivered(Some("backend_delivery_004".into())),
                Timestamp::new("2026-05-30T00:00:03Z"),
            )
            .expect_err("second finish should be rejected");

        assert_eq!(error, DomainError::AttemptAlreadyFinished);
    }

    #[test]
    fn delivery_history_entry_describes_transition() {
        let (record, _, _) = scheduled_record();
        let history = DeliveryHistoryEntry::transition(
            record.delivery_id,
            DeliveryStatus::Scheduled,
            DeliveryStatus::Dispatching,
            HistoryReason::dispatching_started(),
            Timestamp::new("2026-05-30T00:00:01Z"),
        );

        assert!(
            history.describes_transition(DeliveryStatus::Scheduled, DeliveryStatus::Dispatching)
        );
        assert_eq!(history.reason, HistoryReason::dispatching_started());
    }

    #[test]
    fn delivery_lifecycle_requires_history_for_allowed_transition() {
        let lifecycle = DeliveryLifecycle::default_for_bus();

        assert!(lifecycle.requires_history(DeliveryStatus::Dispatching, DeliveryStatus::Delivered));
        assert!(!lifecycle.requires_history(DeliveryStatus::Scheduled, DeliveryStatus::Scheduled));
    }
}
