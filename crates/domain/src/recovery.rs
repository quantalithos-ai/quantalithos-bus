//! Recovery domain objects and guard policies for retry, dead-letter, and replay preparation.

use bus_contracts::metadata::{
    ActorContext, AttemptCount, AttemptLimit, AuditChainRef, AuditRef, CloseReason, DeadLetterId,
    DeadLetterRef, DeadLetterStatus, DeliveryAttemptId, DeliveryHistoryRef, DeliveryStatus,
    FailureMaterialId, FailureReason, RecoveryPolicyConfigRef, RecoveryPolicyRef, RecoveryReason,
    ReplayApprovalRef, ReplayPreparationId, ReplayPreparationRef, ReplayPreparationStatus,
    ReplayRejectReason, RetryPlanId, RetryPlanStatus, RetryPolicyRef, Timestamp, Version,
};

use crate::audit::BusAuditEntry;
use crate::delivery::{DeliveryHistoryEntry, DeliveryRecord};
use crate::errors::DomainError;
use crate::feedback::FeedbackResult;

/// A controlled retry plan for one failed delivery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetryPlan {
    /// The stable retry-plan identifier.
    pub retry_plan_id: RetryPlanId,
    /// The associated delivery identifier.
    pub delivery_id: bus_contracts::metadata::DeliveryId,
    /// The next time when retry may run.
    pub next_attempt_at: Timestamp,
    /// The maximum number of attempts allowed by the request.
    pub max_attempts: AttemptLimit,
    /// The remaining attempt budget for the plan.
    pub remaining_attempts: AttemptCount,
    /// The current retry-plan status.
    pub status: RetryPlanStatus,
    version: Version,
}

impl RetryPlan {
    /// Creates a retry plan from one failed delivery and a validated retry request.
    pub fn create(
        delivery: DeliveryRecord,
        reason: FailureReason,
        policy_ref: RetryPolicyRef,
        max_attempts: AttemptLimit,
        now: Timestamp,
    ) -> Result<Self, DomainError> {
        if delivery.status != DeliveryStatus::Failed {
            return Err(DomainError::RetryNotAllowed);
        }
        if reason.as_str().trim().is_empty() {
            return Err(DomainError::InvalidRetryPlan("failure_reason"));
        }
        if policy_ref.as_str().trim().is_empty() {
            return Err(DomainError::InvalidRetryPlan("retry_policy_ref"));
        }
        if max_attempts.is_zero() {
            return Err(DomainError::InvalidRetryPlan("max_attempts"));
        }
        if now.as_str().trim().is_empty() {
            return Err(DomainError::InvalidRetryPlan("next_attempt_at"));
        }

        Ok(Self {
            retry_plan_id: RetryPlanId::new(format!(
                "retry_plan_{}_{}_{}",
                sanitize(delivery.delivery_id.as_str()),
                sanitize(policy_ref.as_str()),
                sanitize(now.as_str())
            )),
            delivery_id: delivery.delivery_id,
            next_attempt_at: now,
            max_attempts,
            remaining_attempts: AttemptCount::from(max_attempts),
            status: RetryPlanStatus::Scheduled,
            version: 0,
        })
    }

    /// Returns whether the retry plan still has remaining attempts.
    pub fn has_remaining_attempts(&self) -> bool {
        self.status == RetryPlanStatus::Scheduled && self.remaining_attempts.get() > 0
    }

    /// Returns the associated delivery identifier.
    pub fn delivery_id(&self) -> &bus_contracts::metadata::DeliveryId {
        &self.delivery_id
    }

    /// Records one executed retry attempt while keeping the plan scheduled.
    pub fn mark_attempted(
        &mut self,
        attempt_id: DeliveryAttemptId,
        _result: bus_contracts::metadata::BackendDeliveryResult,
    ) -> Result<(), DomainError> {
        if self.status != RetryPlanStatus::Scheduled {
            return Err(DomainError::RetryNotAllowed);
        }
        if attempt_id.as_str().trim().is_empty() {
            return Err(DomainError::InvalidRetryPlan("attempt_id"));
        }
        if self.remaining_attempts.get() == 0 {
            return Err(DomainError::RetryExhaustedCannotDispatch);
        }

        self.remaining_attempts = AttemptCount::new(self.remaining_attempts.get() - 1);
        Ok(())
    }

    /// Marks the retry plan as exhausted once no remaining attempts exist.
    pub fn mark_exhausted(&mut self, _actor: ActorContext) -> Result<(), DomainError> {
        if self.status != RetryPlanStatus::Scheduled || self.remaining_attempts.get() > 0 {
            return Err(DomainError::InvalidRetryPlanTransition {
                from: self.status,
                to: RetryPlanStatus::Exhausted,
            });
        }

        self.status = RetryPlanStatus::Exhausted;
        Ok(())
    }

    /// Cancels a scheduled retry plan.
    pub fn cancel(
        &mut self,
        _actor: ActorContext,
        reason: RecoveryReason,
    ) -> Result<(), DomainError> {
        if reason.as_str().trim().is_empty() {
            return Err(DomainError::InvalidRetryPlan("recovery_reason"));
        }
        if self.status != RetryPlanStatus::Scheduled {
            return Err(DomainError::InvalidRetryPlanTransition {
                from: self.status,
                to: RetryPlanStatus::Cancelled,
            });
        }

        self.status = RetryPlanStatus::Cancelled;
        Ok(())
    }

    /// Returns the committed version used by persistence adapters.
    pub fn version(&self) -> Version {
        self.version
    }

    /// Overwrites the committed version after repository persistence.
    pub fn set_version(&mut self, version: Version) {
        self.version = version;
    }
}

/// One dead-letter entry attached to a failed delivery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeadLetterEntry {
    /// The stable dead-letter identifier.
    pub dead_letter_id: DeadLetterId,
    /// The associated delivery identifier.
    pub delivery_id: bus_contracts::metadata::DeliveryId,
    /// The stable failure reason linked into the dead-letter path.
    pub failure_reason: FailureReason,
    /// The trusted delivery-history reference used by replay preparation.
    pub history_ref: DeliveryHistoryRef,
    /// The trusted audit-chain reference used by replay preparation.
    pub audit_chain_ref: AuditChainRef,
    /// The current dead-letter status.
    pub status: DeadLetterStatus,
    version: Version,
}

impl DeadLetterEntry {
    /// Creates a dead-letter entry from one failed delivery and existing failure material.
    pub fn from_failed_delivery(
        delivery: DeliveryRecord,
        material: FailureMaterial,
    ) -> Result<Self, DomainError> {
        if delivery.status != DeliveryStatus::Failed {
            return Err(DomainError::DeadLetterNotAllowed);
        }
        if material.delivery_id != delivery.delivery_id {
            return Err(DomainError::DeadLetterNotAllowed);
        }

        let history = delivery
            .history()
            .last()
            .ok_or(DomainError::InvalidDeadLetterEntry("history_ref"))?;
        let history_ref = DeliveryHistoryRef::from(history.history_id.clone());
        let audit_chain_ref = AuditChainRef::from_audit_ref(&material.audit_ref);

        if history_ref.as_str().trim().is_empty() {
            return Err(DomainError::InvalidDeadLetterEntry("history_ref"));
        }
        if audit_chain_ref.as_str().trim().is_empty() {
            return Err(DomainError::InvalidDeadLetterEntry("audit_chain_ref"));
        }

        Ok(Self {
            dead_letter_id: DeadLetterId::new(format!(
                "dead_letter_{}_{}",
                sanitize(delivery.delivery_id.as_str()),
                sanitize(material.failure_material_id.as_str())
            )),
            delivery_id: delivery.delivery_id,
            failure_reason: material.failure_reason,
            history_ref,
            audit_chain_ref,
            status: DeadLetterStatus::Open,
            version: 0,
        })
    }

    /// Moves an open dead-letter entry into review.
    pub fn start_review(&mut self, _actor: ActorContext) -> Result<(), DomainError> {
        if self.status != DeadLetterStatus::Open {
            return Err(DomainError::InvalidDeadLetterTransition {
                from: self.status,
                to: DeadLetterStatus::Reviewing,
            });
        }

        self.status = DeadLetterStatus::Reviewing;
        Ok(())
    }

    /// Closes an open or reviewing dead-letter entry.
    pub fn close(&mut self, _actor: ActorContext, reason: CloseReason) -> Result<(), DomainError> {
        if reason.as_str().trim().is_empty() {
            return Err(DomainError::InvalidDeadLetterEntry("close_reason"));
        }
        if !matches!(
            self.status,
            DeadLetterStatus::Open | DeadLetterStatus::Reviewing
        ) {
            return Err(DomainError::InvalidDeadLetterTransition {
                from: self.status,
                to: DeadLetterStatus::Closed,
            });
        }

        self.status = DeadLetterStatus::Closed;
        Ok(())
    }

    /// Returns whether the dead-letter entry keeps the trusted history and audit chain.
    pub fn has_trusted_chain(&self) -> bool {
        !self.history_ref.as_str().trim().is_empty()
            && !self.audit_chain_ref.as_str().trim().is_empty()
    }

    /// Returns the committed version used by persistence adapters.
    pub fn version(&self) -> Version {
        self.version
    }

    /// Overwrites the committed version after repository persistence.
    pub fn set_version(&mut self, version: Version) {
        self.version = version;
    }
}

/// Replay preparation derived from one dead-letter entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayPreparation {
    /// The stable replay-preparation identifier.
    pub replay_id: ReplayPreparationId,
    /// The associated dead-letter identifier.
    pub dead_letter_id: DeadLetterId,
    /// The current replay-preparation status.
    pub status: ReplayPreparationStatus,
    /// The trusted audit-chain reference used by later replay execution.
    pub audit_chain_ref: AuditChainRef,
    /// The external replay-approval reference once the preparation becomes ready.
    pub approval_ref: Option<ReplayApprovalRef>,
    version: Version,
}

impl ReplayPreparation {
    /// Creates a draft replay preparation from one trusted dead-letter entry.
    pub fn prepare(entry: DeadLetterEntry, _actor: ActorContext) -> Result<Self, DomainError> {
        if entry.status == DeadLetterStatus::Closed {
            return Err(DomainError::DeadLetterClosed);
        }
        if !entry.has_trusted_chain() {
            return Err(DomainError::ReplayPreparationNotAllowed);
        }

        Ok(Self {
            replay_id: ReplayPreparationId::new(format!(
                "replay_preparation_{}",
                sanitize(entry.dead_letter_id.as_str())
            )),
            dead_letter_id: entry.dead_letter_id,
            status: ReplayPreparationStatus::Draft,
            audit_chain_ref: entry.audit_chain_ref,
            approval_ref: None,
            version: 0,
        })
    }

    /// Marks the draft replay preparation as ready with an approval reference.
    pub fn mark_ready(
        &mut self,
        approval_ref: ReplayApprovalRef,
        _actor: ActorContext,
    ) -> Result<(), DomainError> {
        if approval_ref.as_str().trim().is_empty() {
            return Err(DomainError::InvalidReplayPreparation("approval_ref"));
        }
        if self.status != ReplayPreparationStatus::Draft {
            return Err(DomainError::InvalidReplayPreparationTransition {
                from: self.status,
                to: ReplayPreparationStatus::Ready,
            });
        }
        if self.audit_chain_ref.as_str().trim().is_empty() {
            return Err(DomainError::InvalidReplayPreparation("audit_chain_ref"));
        }

        self.status = ReplayPreparationStatus::Ready;
        self.approval_ref = Some(approval_ref);
        Ok(())
    }

    /// Rejects the draft replay preparation.
    pub fn reject(
        &mut self,
        reason: ReplayRejectReason,
        _actor: ActorContext,
    ) -> Result<(), DomainError> {
        if reason.as_str().trim().is_empty() {
            return Err(DomainError::InvalidReplayPreparation("reject_reason"));
        }
        if self.status != ReplayPreparationStatus::Draft {
            return Err(DomainError::InvalidReplayPreparationTransition {
                from: self.status,
                to: ReplayPreparationStatus::Rejected,
            });
        }

        self.status = ReplayPreparationStatus::Rejected;
        Ok(())
    }

    /// Supersedes the draft replay preparation with a later preparation reference.
    pub fn supersede(
        &mut self,
        new_ref: ReplayPreparationRef,
        _actor: ActorContext,
    ) -> Result<(), DomainError> {
        if new_ref.as_str().trim().is_empty() {
            return Err(DomainError::InvalidReplayPreparation("replacement_ref"));
        }
        if self.status != ReplayPreparationStatus::Draft {
            return Err(DomainError::InvalidReplayPreparationTransition {
                from: self.status,
                to: ReplayPreparationStatus::Superseded,
            });
        }

        self.status = ReplayPreparationStatus::Superseded;
        Ok(())
    }

    /// Returns whether replay preparation requires a trusted chain.
    pub fn requires_trusted_chain(&self) -> bool {
        true
    }

    /// Returns the committed version used by persistence adapters.
    pub fn version(&self) -> Version {
        self.version
    }

    /// Overwrites the committed version after repository persistence.
    pub fn set_version(&mut self, version: Version) {
        self.version = version;
    }
}

/// Failure material exposed to governance, operators, and projections.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FailureMaterial {
    /// The stable failure-material identifier.
    pub failure_material_id: FailureMaterialId,
    /// The associated delivery identifier.
    pub delivery_id: bus_contracts::metadata::DeliveryId,
    /// The stable delivery failure reason.
    pub failure_reason: FailureReason,
    /// The linked dead-letter reference, if the material already entered DLQ.
    pub dead_letter_ref: Option<DeadLetterRef>,
    /// The audit reference that anchors the material to committed truth.
    pub audit_ref: AuditRef,
    version: Version,
}

impl FailureMaterial {
    /// Creates failure material from a committed failure feedback and matching history entry.
    pub fn from_feedback(
        feedback: FeedbackResult,
        history: DeliveryHistoryEntry,
        audit_ref: AuditRef,
    ) -> Result<Self, DomainError> {
        if !feedback.is_failure() {
            return Err(DomainError::InvalidFailureMaterial("feedback.status"));
        }
        if history.delivery_id != feedback.delivery_id {
            return Err(DomainError::InvalidFailureMaterial("history.delivery_id"));
        }
        if history.to_status != DeliveryStatus::Failed {
            return Err(DomainError::InvalidFailureMaterial("history.to_status"));
        }
        if audit_ref.as_str().trim().is_empty() {
            return Err(DomainError::InvalidFailureMaterial("audit_ref"));
        }
        let delivery_id = feedback.delivery_id.clone();
        let failure_reason = feedback.failure_reason();

        Ok(Self {
            failure_material_id: FailureMaterialId::new(format!(
                "failure_material_{}_{}",
                sanitize(delivery_id.as_str()),
                sanitize(audit_ref.as_str())
            )),
            delivery_id,
            failure_reason,
            dead_letter_ref: None,
            audit_ref,
            version: 0,
        })
    }

    /// Creates failure material linked to one dead-letter entry and audit entry.
    pub fn from_dead_letter(
        entry: DeadLetterEntry,
        audit: BusAuditEntry,
    ) -> Result<Self, DomainError> {
        if !entry.has_trusted_chain() {
            return Err(DomainError::InvalidFailureMaterial("audit_chain_ref"));
        }
        if audit.audit_ref.as_str().trim().is_empty() {
            return Err(DomainError::InvalidFailureMaterial("audit_ref"));
        }

        Ok(Self {
            failure_material_id: FailureMaterialId::new(format!(
                "failure_material_{}_{}",
                sanitize(entry.delivery_id.as_str()),
                sanitize(entry.dead_letter_id.as_str())
            )),
            delivery_id: entry.delivery_id,
            failure_reason: entry.failure_reason,
            dead_letter_ref: Some(DeadLetterRef::from(entry.dead_letter_id)),
            audit_ref: audit.audit_ref,
            version: 0,
        })
    }

    /// Returns whether the material is a governance decision.
    pub fn is_governance_decision(&self) -> bool {
        false
    }

    /// Returns whether the material is already linked to a dead-letter entry.
    pub fn has_dead_letter(&self) -> bool {
        self.dead_letter_ref.is_some()
    }

    /// Returns the committed version used by persistence adapters.
    pub fn version(&self) -> Version {
        self.version
    }

    /// Overwrites the committed version after repository persistence.
    pub fn set_version(&mut self, version: Version) {
        self.version = version;
    }
}

/// Guard policy for retry, dead-letter, and replay-preparation eligibility.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryEligibilityPolicy {
    /// The policy reference attached to the current runtime.
    pub policy_ref: RecoveryPolicyRef,
}

impl RecoveryEligibilityPolicy {
    /// Builds the runtime recovery policy from one config reference.
    pub fn from_config(config_ref: RecoveryPolicyConfigRef) -> RecoveryEligibilityPolicy {
        RecoveryEligibilityPolicy {
            policy_ref: RecoveryPolicyRef::new(format!(
                "recovery_policy_{}",
                sanitize(config_ref.as_str())
            )),
        }
    }

    /// Returns whether controlled retry is allowed for the provided delivery and plan.
    pub fn can_retry(&self, delivery: DeliveryRecord, plan: RetryPlan) -> Result<(), DomainError> {
        if delivery.status != DeliveryStatus::Failed || plan.delivery_id != delivery.delivery_id {
            return Err(DomainError::RetryNotAllowed);
        }
        if plan.status != RetryPlanStatus::Scheduled {
            return Err(DomainError::RetryNotAllowed);
        }
        if plan.max_attempts.is_zero() {
            return Err(DomainError::InvalidRetryPlan("max_attempts"));
        }
        if plan.remaining_attempts.get() > plan.max_attempts.get() {
            return Err(DomainError::InvalidRetryPlan("remaining_attempts"));
        }
        if !plan.has_remaining_attempts() {
            return Err(DomainError::RetryExhaustedCannotDispatch);
        }

        Ok(())
    }

    /// Returns whether dead-letter handling is allowed for the provided delivery and material.
    pub fn can_dead_letter(
        &self,
        delivery: DeliveryRecord,
        material: FailureMaterial,
    ) -> Result<(), DomainError> {
        if delivery.status != DeliveryStatus::Failed || material.delivery_id != delivery.delivery_id
        {
            return Err(DomainError::DeadLetterNotAllowed);
        }
        if material.is_governance_decision() {
            return Err(DomainError::DeadLetterNotAllowed);
        }
        if material.audit_ref.as_str().trim().is_empty() {
            return Err(DomainError::InvalidFailureMaterial("audit_ref"));
        }

        Ok(())
    }

    /// Returns whether replay preparation is allowed for the provided dead-letter entry and chain.
    pub fn can_prepare_replay(
        &self,
        entry: DeadLetterEntry,
        audit_chain_ref: AuditChainRef,
    ) -> Result<(), DomainError> {
        if entry.status == DeadLetterStatus::Closed {
            return Err(DomainError::DeadLetterClosed);
        }
        if !entry.has_trusted_chain() {
            return Err(DomainError::ReplayPreparationNotAllowed);
        }
        if audit_chain_ref.as_str().trim().is_empty() || entry.audit_chain_ref != audit_chain_ref {
            return Err(DomainError::ReplayPreparationNotAllowed);
        }

        Ok(())
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
        BackendFixtureBuilder, PublicationFixtureBuilder, RecoveryFixtureBuilder, TestRunBuilder,
    };
    use bus_contracts::metadata::{
        AuditRef, DeliveryStatus, FeedbackReason, HistoryReason, SubscriberRef, SubscriberScope,
    };

    use super::*;
    use crate::delivery::DeliveryHistoryEntry;
    use crate::feedback::FeedbackSource;
    use crate::publication::{PublicationMaterial, TransportSemantic};

    fn scheduled_delivery(run_id: &str) -> (DeliveryRecord, ActorContext, BackendFixtureBuilder) {
        let run = TestRunBuilder::new(run_id).build();
        let actor = run.actor.clone();
        let publication_builder = PublicationFixtureBuilder::new(run.clone());
        let backend_builder = BackendFixtureBuilder::new(run.clone());
        let material = PublicationMaterial::from_accept_publication_command(
            publication_builder.valid_material(),
            actor.clone(),
            run.metadata,
        )
        .expect("fixture should create valid publication material");

        let semantic = TransportSemantic::derive(
            material,
            backend_builder.in_memory_capability(),
            SubscriberScope {
                project_id: format!("project_{}", run.run_id),
                topic: format!("workitem.events.{}", run.run_id),
            },
        )
        .expect("fixture should derive transport semantic");

        let delivery = DeliveryRecord::schedule(
            semantic,
            SubscriberRef::new("subscriber_alpha"),
            "idem-recovery-domain".into(),
        )
        .expect("fixture should schedule delivery");

        (delivery, actor, backend_builder)
    }

    fn failed_delivery_fixture(
        run_id: &str,
    ) -> (
        DeliveryRecord,
        ActorContext,
        FailureMaterial,
        RecoveryFixtureBuilder,
    ) {
        let (mut delivery, actor, backend_builder) = scheduled_delivery(run_id);
        let run = TestRunBuilder::new(run_id).build();
        let recovery_builder = RecoveryFixtureBuilder::new(run.clone());
        let capability = backend_builder.in_memory_capability();
        let mut attempt = delivery
            .start_attempt(capability, Timestamp::new("2026-05-31T00:00:01Z"))
            .expect("failed fixture should start attempt");
        let dispatching_history = DeliveryHistoryEntry::transition(
            delivery.delivery_id.clone(),
            DeliveryStatus::Scheduled,
            DeliveryStatus::Dispatching,
            HistoryReason::dispatching_started(),
            Timestamp::new("2026-05-31T00:00:01Z"),
        );
        delivery
            .append_history(dispatching_history)
            .expect("dispatching history should append");

        attempt
            .finish(
                bus_contracts::metadata::BackendDeliveryResult::delivered(Some(
                    "backend_delivery_recovery".into(),
                )),
                Timestamp::new("2026-05-31T00:00:02Z"),
            )
            .expect("attempt should finish as delivered");
        delivery
            .mark_delivered(attempt.clone(), actor.clone())
            .expect("fixture should reach delivered");
        let delivered_history = DeliveryHistoryEntry::transition(
            delivery.delivery_id.clone(),
            DeliveryStatus::Dispatching,
            DeliveryStatus::Delivered,
            HistoryReason::delivery_arrived(),
            Timestamp::new("2026-05-31T00:00:02Z"),
        );
        delivery
            .append_history(delivered_history)
            .expect("delivered history should append");

        let feedback = FeedbackResult::fail(
            delivery.delivery_id.clone(),
            FeedbackSource::new(
                attempt.attempt_id.clone(),
                format!("external_feedback_{run_id}").into(),
            )
            .expect("feedback source should be valid"),
            FeedbackReason::new("subscriber_failed"),
            actor.clone(),
            Timestamp::new("2026-05-31T00:00:03Z"),
        )
        .expect("fixture should create failure feedback");

        delivery
            .mark_failed(feedback.failure_reason(), actor.clone())
            .expect("fixture should reach failed");
        let failed_history = DeliveryHistoryEntry::transition(
            delivery.delivery_id.clone(),
            DeliveryStatus::Delivered,
            DeliveryStatus::Failed,
            HistoryReason::feedback_fail(),
            Timestamp::new("2026-05-31T00:00:03Z"),
        );
        delivery
            .append_history(failed_history.clone())
            .expect("failed history should append");

        let failure_material = FailureMaterial::from_feedback(
            feedback,
            failed_history,
            AuditRef::new(format!("audit_{run_id}")),
        )
        .expect("fixture should create failure material");

        (delivery, actor, failure_material, recovery_builder)
    }

    #[test]
    fn retry_plan_create_uses_max_attempts_for_remaining_attempts() {
        let (delivery, _, material, recovery_builder) = failed_delivery_fixture("recovery-001");
        let plan = RetryPlan::create(
            delivery.clone(),
            material.failure_reason.clone(),
            recovery_builder.retry_policy_ref(),
            AttemptLimit::new(3),
            Timestamp::new("2026-05-31T00:05:00Z"),
        )
        .expect("failed delivery should create retry plan");
        let policy = RecoveryEligibilityPolicy::from_config("default_policy".into());

        assert_eq!(plan.status, RetryPlanStatus::Scheduled);
        assert_eq!(plan.max_attempts, AttemptLimit::new(3));
        assert_eq!(plan.remaining_attempts, AttemptCount::new(3));
        assert!(policy.can_retry(delivery, plan).is_ok());
    }

    #[test]
    fn retry_plan_rejects_non_failed_delivery() {
        let (delivery, _, _) = scheduled_delivery("recovery-002");
        let error = RetryPlan::create(
            delivery,
            FailureReason::dispatch_failed(),
            RetryPolicyRef::new("retry_policy_default"),
            AttemptLimit::new(3),
            Timestamp::new("2026-05-31T00:05:00Z"),
        )
        .expect_err("non-failed delivery must not create retry");

        assert_eq!(error, DomainError::RetryNotAllowed);
    }

    #[test]
    fn retry_plan_mark_exhausted_requires_zero_remaining_attempts() {
        let (delivery, actor, material, recovery_builder) = failed_delivery_fixture("recovery-003");
        let mut plan = RetryPlan::create(
            delivery,
            material.failure_reason,
            recovery_builder.retry_policy_ref(),
            AttemptLimit::new(2),
            Timestamp::new("2026-05-31T00:05:00Z"),
        )
        .expect("failed delivery should create retry plan");

        let error = plan
            .mark_exhausted(actor.clone())
            .expect_err("scheduled retry with remaining attempts must stay scheduled");
        assert_eq!(
            error,
            DomainError::InvalidRetryPlanTransition {
                from: RetryPlanStatus::Scheduled,
                to: RetryPlanStatus::Exhausted,
            }
        );

        plan.remaining_attempts = AttemptCount::new(0);
        plan.mark_exhausted(actor)
            .expect("zero remaining attempts may exhaust the plan");
        assert_eq!(plan.status, RetryPlanStatus::Exhausted);
    }

    #[test]
    fn dead_letter_entry_tracks_trusted_chain_and_review_lifecycle() {
        let (mut delivery, actor, material, _) = failed_delivery_fixture("recovery-004");
        let mut entry = DeadLetterEntry::from_failed_delivery(delivery.clone(), material.clone())
            .expect("failed delivery should create dead-letter entry");

        assert_eq!(entry.status, DeadLetterStatus::Open);
        assert!(entry.has_trusted_chain());

        delivery
            .mark_dead_lettered(entry.dead_letter_id.clone(), actor.clone())
            .expect("failed delivery should enter dead-lettered state");
        assert_eq!(delivery.status, DeliveryStatus::DeadLettered);

        entry
            .start_review(actor.clone())
            .expect("open dead-letter entry should enter review");
        assert_eq!(entry.status, DeadLetterStatus::Reviewing);

        entry
            .close(actor, CloseReason::new("operator_closed"))
            .expect("reviewing dead-letter entry should close");
        assert_eq!(entry.status, DeadLetterStatus::Closed);

        let linked = FailureMaterial::from_dead_letter(
            entry.clone(),
            BusAuditEntry::record(
                AuditRef::new("audit_dead_letter_link"),
                crate::audit::SubjectRef::Delivery(delivery.delivery_id.clone()),
                crate::audit::AuditAction::DeliveryFailed(FailureReason::dispatch_failed()),
                TestRunBuilder::new("recovery-004-audit").build().actor,
                "trace_dead_letter_link".into(),
                Timestamp::new("2026-05-31T00:00:04Z"),
            ),
        )
        .expect("dead-letter output should produce linked failure material");

        assert!(linked.has_dead_letter());
        assert!(!linked.is_governance_decision());
        assert_eq!(
            linked.dead_letter_ref,
            Some(DeadLetterRef::from(entry.dead_letter_id))
        );
        assert_eq!(material.dead_letter_ref, None);
    }

    #[test]
    fn replay_preparation_requires_matching_trusted_chain() {
        let (_, _, material, recovery_builder) = failed_delivery_fixture("recovery-005");
        let (delivery, _actor, _) = scheduled_delivery("recovery-005b");
        let error = DeadLetterEntry::from_failed_delivery(delivery, material)
            .expect_err("non-failed delivery must not create dead-letter entry");
        assert_eq!(error, DomainError::DeadLetterNotAllowed);

        let (delivery, actor, material, _) = failed_delivery_fixture("recovery-005c");
        let entry =
            DeadLetterEntry::from_failed_delivery(delivery, material).expect("entry should exist");
        let policy = RecoveryEligibilityPolicy::from_config("default_policy".into());
        let mismatch = AuditChainRef::new("audit_chain_other");
        let error = policy
            .can_prepare_replay(entry.clone(), mismatch)
            .expect_err("mismatched audit chain must be rejected");
        assert_eq!(error, DomainError::ReplayPreparationNotAllowed);

        let mut preparation =
            ReplayPreparation::prepare(entry.clone(), actor.clone()).expect("draft should build");
        assert_eq!(preparation.status, ReplayPreparationStatus::Draft);
        preparation
            .mark_ready(recovery_builder.replay_approval_ref(), actor)
            .expect("draft replay preparation should become ready");
        assert_eq!(preparation.status, ReplayPreparationStatus::Ready);
        assert_eq!(
            preparation.approval_ref,
            Some(recovery_builder.replay_approval_ref())
        );
    }

    #[test]
    fn replay_preparation_rejects_closed_dead_letter_and_blank_approval() {
        let (delivery, actor, material, _) = failed_delivery_fixture("recovery-006");
        let mut entry =
            DeadLetterEntry::from_failed_delivery(delivery, material).expect("entry should exist");
        entry
            .close(actor.clone(), CloseReason::new("operator_closed"))
            .expect("open entry should close");
        let policy = RecoveryEligibilityPolicy::from_config("default_policy".into());

        let error = policy
            .can_prepare_replay(entry.clone(), entry.audit_chain_ref.clone())
            .expect_err("closed dead-letter entry must reject replay preparation");
        assert_eq!(error, DomainError::DeadLetterClosed);

        let (delivery, actor, material, _) = failed_delivery_fixture("recovery-006b");
        let entry =
            DeadLetterEntry::from_failed_delivery(delivery, material).expect("entry should exist");
        let mut preparation =
            ReplayPreparation::prepare(entry, actor.clone()).expect("draft should build");
        let error = preparation
            .mark_ready(ReplayApprovalRef::new(""), actor)
            .expect_err("blank approval ref must be rejected");

        assert_eq!(error, DomainError::InvalidReplayPreparation("approval_ref"));
    }
}
