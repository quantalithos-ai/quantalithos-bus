//! Delivery feedback write-path service.

use bus_contracts::commands::RecordDeliveryFeedbackCommand;
use bus_contracts::metadata::{
    ActorContext, AuditRef, CommandMetadata, DeliveryAttemptId, DeliveryAttemptRef, DeliveryStatus,
    HistoryReason, IdempotencyKey, TraceContextRef,
};
use bus_contracts::receipts::FeedbackRecordResult;
use bus_domain::audit::{AuditAction, BusAuditEntry, SubjectRef};
use bus_domain::delivery::{DeliveryHistoryEntry, DeliveryRecord};
use bus_domain::feedback::FeedbackResult;
use bus_domain::idempotency::{
    IdempotencyAnchor, IdempotencyConflict, IdempotencyScope, RecordRef, RequestDigest,
};

use crate::errors::{ApplicationError, RepositoryError};
use crate::ports::{
    AuditTrailRepository, BusRecordKind, ClockPort, DeliveryRepository, FeedbackRepository,
    IdGeneratorPort, IdempotencyRepository, RollbackReason, UnitOfWork, UnitOfWorkPurpose,
};

/// The delivery feedback use-case contract.
pub trait DeliveryFeedbackUseCase: Send + Sync {
    /// Records one delivery feedback command.
    async fn record(
        &self,
        command: RecordDeliveryFeedbackCommand,
        actor: ActorContext,
        meta: CommandMetadata,
    ) -> Result<FeedbackRecordResult, ApplicationError>;
}

/// Dependencies for the feedback recording service.
pub struct FeedbackRecordingServiceDeps<F, D, I, A, U, C, G> {
    /// Feedback truth repository.
    pub feedback_repository: F,
    /// Delivery truth repository.
    pub delivery_repository: D,
    /// Idempotency anchor repository.
    pub idempotency_repository: I,
    /// Audit repository.
    pub audit_repository: A,
    /// Unit-of-work boundary.
    pub unit_of_work: U,
    /// Clock source.
    pub clock: C,
    /// Record ID generator.
    pub id_generator: G,
}

/// Delivery feedback application service.
pub struct FeedbackRecordingService<F, D, I, A, U, C, G> {
    deps: FeedbackRecordingServiceDeps<F, D, I, A, U, C, G>,
}

impl<F, D, I, A, U, C, G> FeedbackRecordingService<F, D, I, A, U, C, G> {
    /// Creates a new feedback recording service.
    pub fn new(deps: FeedbackRecordingServiceDeps<F, D, I, A, U, C, G>) -> Self {
        Self { deps }
    }
}

impl<F, D, I, A, U, C, G> FeedbackRecordingService<F, D, I, A, U, C, G>
where
    F: FeedbackRepository,
    D: DeliveryRepository,
    I: IdempotencyRepository,
    A: AuditTrailRepository,
    U: UnitOfWork,
    C: ClockPort,
    G: IdGeneratorPort,
{
    async fn rollback_with<TValue>(
        &self,
        handle: crate::ports::UnitOfWorkHandle,
        error: ApplicationError,
    ) -> Result<TValue, ApplicationError> {
        let code = error.code();
        self.deps
            .unit_of_work
            .rollback(handle, RollbackReason::ApplicationError(code))
            .await
            .map_err(ApplicationError::from)?;

        Err(error)
    }

    fn idempotency_key(meta: &CommandMetadata) -> Result<IdempotencyKey, ApplicationError> {
        meta.request.idempotency_key.clone().ok_or_else(|| {
            ApplicationError::validation(
                "validation.idempotency_key_missing",
                "x-idempotency-key is required",
            )
        })
    }

    fn result_from_feedback(
        feedback: &FeedbackResult,
    ) -> Result<FeedbackRecordResult, ApplicationError> {
        let audit_ref = feedback
            .audit_ref()
            .cloned()
            .ok_or_else(|| ApplicationError::from(RepositoryError::CorruptedRecord))?;

        Ok(FeedbackRecordResult::recorded(
            feedback.feedback_id.clone(),
            feedback.delivery_id.clone(),
            feedback.implied_delivery_status(),
            audit_ref,
        ))
    }

    async fn load_existing_result(
        &self,
        anchor: &IdempotencyAnchor,
    ) -> Result<FeedbackRecordResult, ApplicationError> {
        let feedback_id = match &anchor.bound_record_ref {
            RecordRef::Feedback(feedback_id) => feedback_id,
            RecordRef::Publication(_) => {
                return Err(ApplicationError::from(RepositoryError::CorruptedRecord));
            }
        };
        let feedback = self
            .deps
            .feedback_repository
            .get(feedback_id)
            .await?
            .ok_or_else(|| ApplicationError::from(RepositoryError::CorruptedRecord))?;

        Self::result_from_feedback(&feedback)
    }

    async fn begin_idempotency_conflict(
        &self,
        scope: IdempotencyScope,
        key: IdempotencyKey,
        existing_anchor: IdempotencyAnchor,
        incoming_digest: RequestDigest,
        actor: ActorContext,
        trace_ref: TraceContextRef,
    ) -> Result<FeedbackRecordResult, ApplicationError> {
        let handle = self
            .deps
            .unit_of_work
            .begin(UnitOfWorkPurpose::RecordDeliveryFeedback, actor.clone())
            .await?;
        let now = self.deps.clock.now();
        let audit_ref = AuditRef::new(
            self.deps
                .id_generator
                .next_record_id(BusRecordKind::AuditEntry)?,
        );
        let conflict = IdempotencyConflict {
            scope: scope.clone(),
            key: key.clone(),
            existing_digest: existing_anchor.request_digest.clone(),
            incoming_digest,
            occurred_at: now.clone(),
            trace_ref: trace_ref.clone(),
        };
        let audit_entry = BusAuditEntry::record(
            audit_ref.clone(),
            SubjectRef::IdempotencyKey {
                scope: scope.clone(),
                key: key.clone(),
            },
            AuditAction::IdempotencyConflict,
            actor,
            trace_ref,
            now,
        );

        if let Err(error) = self
            .deps
            .idempotency_repository
            .mark_conflict(scope, key, conflict, &handle)
            .await
        {
            return self
                .rollback_with(handle, ApplicationError::from(error))
                .await;
        }
        if let Err(error) = self
            .deps
            .audit_repository
            .append(audit_entry, &handle)
            .await
        {
            return self
                .rollback_with(handle, ApplicationError::from(error))
                .await;
        }
        self.deps
            .unit_of_work
            .commit(handle)
            .await
            .map_err(ApplicationError::from)?;

        Err(ApplicationError::conflict(
            "conflict.idempotency_request_mismatch",
            "idempotency key was reused for a different request",
            Some(audit_ref.as_str().to_owned()),
        ))
    }

    fn validate_attempt(
        delivery: &DeliveryRecord,
        attempt_id: &DeliveryAttemptId,
    ) -> Result<(), ApplicationError> {
        let attempt_ref = DeliveryAttemptRef::new(attempt_id.as_str());
        let attempt = delivery
            .attempts()
            .iter()
            .find(|candidate| candidate.attempt_id == *attempt_id)
            .ok_or_else(|| {
                ApplicationError::not_found(
                    "not_found.delivery_attempt",
                    format!("delivery attempt {} was not found", attempt_id.as_str()),
                    None,
                )
            })?;

        if delivery.last_attempt_ref != Some(attempt_ref) {
            return Err(ApplicationError::conflict(
                "conflict.late_feedback",
                "feedback does not target the current delivery attempt",
                None,
            ));
        }
        if !attempt.is_finished() {
            return Err(ApplicationError::conflict(
                "conflict.delivery_attempt_unfinished",
                "delivery attempt is not finished",
                None,
            ));
        }

        Ok(())
    }
}

impl<F, D, I, A, U, C, G> DeliveryFeedbackUseCase for FeedbackRecordingService<F, D, I, A, U, C, G>
where
    F: FeedbackRepository,
    D: DeliveryRepository,
    I: IdempotencyRepository,
    A: AuditTrailRepository,
    U: UnitOfWork,
    C: ClockPort,
    G: IdGeneratorPort,
{
    async fn record(
        &self,
        command: RecordDeliveryFeedbackCommand,
        actor: ActorContext,
        meta: CommandMetadata,
    ) -> Result<FeedbackRecordResult, ApplicationError> {
        let idempotency_key = Self::idempotency_key(&meta)?;
        let scope = IdempotencyScope::for_record_delivery_feedback(&command);
        let request_digest = RequestDigest::from_record_delivery_feedback_command(&command)?;

        if let Some(existing_anchor) = self
            .deps
            .idempotency_repository
            .find(&scope, &idempotency_key)
            .await?
        {
            if existing_anchor.matches(&request_digest) {
                return self.load_existing_result(&existing_anchor).await;
            }

            return self
                .begin_idempotency_conflict(
                    scope,
                    idempotency_key,
                    existing_anchor,
                    request_digest,
                    actor,
                    meta.request.trace_id,
                )
                .await;
        }

        let uow = self
            .deps
            .unit_of_work
            .begin(UnitOfWorkPurpose::RecordDeliveryFeedback, actor.clone())
            .await?;
        let mut delivery = match self
            .deps
            .delivery_repository
            .get_for_update(&command.delivery_id, &uow)
            .await?
        {
            Some(delivery) => delivery,
            None => {
                return self
                    .rollback_with(
                        uow,
                        ApplicationError::not_found(
                            "not_found.delivery",
                            format!("delivery {} was not found", command.delivery_id.as_str()),
                            None,
                        ),
                    )
                    .await;
            }
        };
        if let Err(error) = Self::validate_attempt(&delivery, &command.attempt_id) {
            return self.rollback_with(uow, error).await;
        }

        let expected_version = delivery.version();
        let mut feedback = match FeedbackResult::from_command(command.clone(), actor.clone()) {
            Ok(feedback) => feedback,
            Err(error) => return self.rollback_with(uow, ApplicationError::from(error)).await,
        };
        let from_status = delivery.status;

        match feedback.status {
            bus_contracts::metadata::FeedbackStatus::Ack => {
                if let Err(error) = delivery.mark_completed(feedback.clone(), actor.clone()) {
                    return self.rollback_with(uow, ApplicationError::from(error)).await;
                }
                let history = DeliveryHistoryEntry::transition(
                    delivery.delivery_id.clone(),
                    from_status,
                    DeliveryStatus::Completed,
                    HistoryReason::feedback_ack(),
                    feedback.observed_at.clone(),
                );
                if let Err(error) = delivery.append_history(history) {
                    return self.rollback_with(uow, ApplicationError::from(error)).await;
                }
            }
            bus_contracts::metadata::FeedbackStatus::Fail => {
                if let Err(error) = delivery.mark_failed(feedback.failure_reason(), actor.clone()) {
                    return self.rollback_with(uow, ApplicationError::from(error)).await;
                }
                let history = DeliveryHistoryEntry::transition(
                    delivery.delivery_id.clone(),
                    from_status,
                    DeliveryStatus::Failed,
                    HistoryReason::feedback_fail(),
                    feedback.observed_at.clone(),
                );
                if let Err(error) = delivery.append_history(history) {
                    return self.rollback_with(uow, ApplicationError::from(error)).await;
                }
            }
            bus_contracts::metadata::FeedbackStatus::Timeout
            | bus_contracts::metadata::FeedbackStatus::Duplicate => {
                return self
                    .rollback_with(
                        uow,
                        ApplicationError::validation(
                            "validation.feedback_kind",
                            "feedback command only accepts ack or fail kinds",
                        ),
                    )
                    .await;
            }
        }

        let audit_ref = AuditRef::new(
            self.deps
                .id_generator
                .next_record_id(BusRecordKind::AuditEntry)?,
        );
        feedback.attach_audit_ref(audit_ref.clone());
        let audit_entry = BusAuditEntry::record(
            audit_ref.clone(),
            SubjectRef::Delivery(delivery.delivery_id.clone()),
            AuditAction::FeedbackRecorded(feedback.status),
            actor,
            meta.request.trace_id.clone(),
            feedback.observed_at.clone(),
        );
        let anchor = IdempotencyAnchor::bind(
            self.deps
                .id_generator
                .next_record_id(BusRecordKind::IdempotencyAnchor)?,
            scope,
            idempotency_key,
            request_digest,
            RecordRef::Feedback(feedback.feedback_id.clone()),
            self.deps.clock.now(),
            meta.request.trace_id,
        );

        if let Err(error) = self
            .deps
            .feedback_repository
            .insert(feedback.clone(), &uow)
            .await
        {
            let mapped = match error {
                RepositoryError::UniqueViolation => ApplicationError::conflict(
                    "conflict.feedback_already_recorded",
                    "feedback source was already recorded",
                    None,
                ),
                other => ApplicationError::from(other),
            };

            return self.rollback_with(uow, mapped).await;
        }
        if let Err(error) = self
            .deps
            .delivery_repository
            .save(delivery.clone(), expected_version, &uow)
            .await
        {
            return self.rollback_with(uow, ApplicationError::from(error)).await;
        }
        if let Err(error) = self.deps.audit_repository.append(audit_entry, &uow).await {
            return self.rollback_with(uow, ApplicationError::from(error)).await;
        }
        if let Err(error) = self.deps.idempotency_repository.bind(anchor, &uow).await {
            return self.rollback_with(uow, ApplicationError::from(error)).await;
        }
        self.deps
            .unit_of_work
            .commit(uow)
            .await
            .map_err(ApplicationError::from)?;

        Ok(FeedbackRecordResult::recorded(
            feedback.feedback_id,
            feedback.delivery_id,
            delivery.status,
            audit_ref,
        ))
    }
}
