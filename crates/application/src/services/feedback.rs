//! Delivery feedback and inbound-signal write-path service.

use bus_contracts::commands::RecordDeliveryFeedbackCommand;
use bus_contracts::events::{BackendDeliverySignalInput, DeliveryTimeoutSignalInput};
use bus_contracts::metadata::{
    ActorContext, AuditRef, BackendDeliveryResult, BackendDeliveryStatus, CommandMetadata,
    DeliveryAttemptId, DeliveryAttemptRef, DeliveryStatus, EventMetadata, HistoryReason,
    IdempotencyKey, TraceContextRef,
};
use bus_contracts::receipts::{
    BackendSignalNormalizedResult, BackendSignalResult, FeedbackRecordResult, TimeoutRecordResult,
};
use bus_domain::audit::{AuditAction, BusAuditEntry, SubjectRef};
use bus_domain::delivery::{DeliveryHistoryEntry, DeliveryRecord};
use bus_domain::feedback::{FeedbackResult, FeedbackSource};
use bus_domain::idempotency::{
    IdempotencyAnchor, IdempotencyConflict, IdempotencyScope, RecordRef, RequestDigest,
};

use crate::errors::{ApplicationError, RepositoryError};
use crate::ports::{
    AuditTrailRepository, BusRecordKind, ClockPort, DeliveryRepository, FeedbackRepository,
    IdGeneratorPort, IdempotencyRepository, RollbackReason, TransportBackendPort, UnitOfWork,
    UnitOfWorkHandle, UnitOfWorkPurpose,
};

/// The delivery feedback command use-case contract.
pub trait DeliveryFeedbackUseCase: Send + Sync {
    /// Records one delivery feedback command.
    async fn record(
        &self,
        command: RecordDeliveryFeedbackCommand,
        actor: ActorContext,
        meta: CommandMetadata,
    ) -> Result<FeedbackRecordResult, ApplicationError>;
}

/// The backend signal consumer use-case contract.
pub trait BackendSignalUseCase: Send + Sync {
    /// Records one backend delivery signal.
    async fn record_backend_signal(
        &self,
        input: BackendDeliverySignalInput,
        actor: ActorContext,
        meta: EventMetadata,
    ) -> Result<BackendSignalResult, ApplicationError>;
}

/// The timeout signal consumer use-case contract.
pub trait TimeoutSignalUseCase: Send + Sync {
    /// Records one delivery timeout signal.
    async fn record_timeout(
        &self,
        input: DeliveryTimeoutSignalInput,
        actor: ActorContext,
        meta: EventMetadata,
    ) -> Result<TimeoutRecordResult, ApplicationError>;
}

/// Dependencies for the feedback recording service.
pub struct FeedbackRecordingServiceDeps<F, D, I, A, U, C, G, T> {
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
    /// Transport backend adapter used for signal normalization.
    pub transport_backend: T,
}

/// Delivery feedback application service.
pub struct FeedbackRecordingService<F, D, I, A, U, C, G, T> {
    deps: FeedbackRecordingServiceDeps<F, D, I, A, U, C, G, T>,
}

impl<F, D, I, A, U, C, G, T> FeedbackRecordingService<F, D, I, A, U, C, G, T> {
    /// Creates a new feedback recording service.
    pub fn new(deps: FeedbackRecordingServiceDeps<F, D, I, A, U, C, G, T>) -> Self {
        Self { deps }
    }
}

impl<F, D, I, A, U, C, G, T> FeedbackRecordingService<F, D, I, A, U, C, G, T>
where
    F: FeedbackRepository,
    D: DeliveryRepository,
    I: IdempotencyRepository,
    A: AuditTrailRepository,
    U: UnitOfWork,
    C: ClockPort,
    G: IdGeneratorPort,
    T: TransportBackendPort,
{
    async fn rollback_with<TValue>(
        &self,
        handle: UnitOfWorkHandle,
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

    fn next_audit_ref(&self) -> Result<AuditRef, ApplicationError> {
        Ok(AuditRef::new(
            self.deps
                .id_generator
                .next_record_id(BusRecordKind::AuditEntry)?,
        ))
    }

    fn next_anchor_id(&self) -> Result<String, ApplicationError> {
        self.deps
            .id_generator
            .next_record_id(BusRecordKind::IdempotencyAnchor)
            .map_err(ApplicationError::from)
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

    fn backend_signal_outcome(
        feedback: &FeedbackResult,
    ) -> Result<BackendSignalNormalizedResult, ApplicationError> {
        match feedback.status {
            bus_contracts::metadata::FeedbackStatus::Ack => Ok(BackendSignalNormalizedResult::Ack),
            bus_contracts::metadata::FeedbackStatus::Fail => {
                Ok(BackendSignalNormalizedResult::Fail)
            }
            _ => Err(ApplicationError::from(RepositoryError::CorruptedRecord)),
        }
    }

    async fn load_feedback(
        &self,
        anchor: &IdempotencyAnchor,
    ) -> Result<FeedbackResult, ApplicationError> {
        let feedback_id = match &anchor.bound_record_ref {
            RecordRef::Feedback(feedback_id) => feedback_id,
            RecordRef::Publication(_) => {
                return Err(ApplicationError::from(RepositoryError::CorruptedRecord));
            }
        };

        self.deps
            .feedback_repository
            .get(feedback_id)
            .await?
            .ok_or_else(|| ApplicationError::from(RepositoryError::CorruptedRecord))
    }

    async fn load_existing_feedback_result(
        &self,
        anchor: &IdempotencyAnchor,
    ) -> Result<FeedbackRecordResult, ApplicationError> {
        let feedback = self.load_feedback(anchor).await?;
        Self::result_from_feedback(&feedback)
    }

    async fn load_existing_backend_signal_result(
        &self,
        anchor: &IdempotencyAnchor,
        delivery_id: bus_contracts::metadata::DeliveryId,
        attempt_id: DeliveryAttemptId,
    ) -> Result<BackendSignalResult, ApplicationError> {
        let feedback = self.load_feedback(anchor).await?;
        let audit_ref = feedback
            .audit_ref()
            .cloned()
            .ok_or_else(|| ApplicationError::from(RepositoryError::CorruptedRecord))?;

        Ok(BackendSignalResult::duplicate(
            delivery_id,
            attempt_id,
            Self::backend_signal_outcome(&feedback)?,
            feedback.feedback_id,
            audit_ref,
        ))
    }

    async fn load_existing_timeout_result(
        &self,
        anchor: &IdempotencyAnchor,
        delivery_id: bus_contracts::metadata::DeliveryId,
    ) -> Result<TimeoutRecordResult, ApplicationError> {
        let feedback = self.load_feedback(anchor).await?;
        if feedback.status != bus_contracts::metadata::FeedbackStatus::Timeout {
            return Err(ApplicationError::from(RepositoryError::CorruptedRecord));
        }
        let audit_ref = feedback
            .audit_ref()
            .cloned()
            .ok_or_else(|| ApplicationError::from(RepositoryError::CorruptedRecord))?;

        Ok(TimeoutRecordResult::duplicate(
            delivery_id,
            feedback.feedback_id,
            true,
            audit_ref,
        ))
    }

    async fn begin_idempotency_conflict<TValue>(
        &self,
        purpose: UnitOfWorkPurpose,
        scope: IdempotencyScope,
        key: IdempotencyKey,
        existing_anchor: IdempotencyAnchor,
        incoming_digest: RequestDigest,
        actor: ActorContext,
        trace_ref: TraceContextRef,
    ) -> Result<TValue, ApplicationError> {
        let handle = self.deps.unit_of_work.begin(purpose, actor.clone()).await?;
        let now = self.deps.clock.now();
        let audit_ref = self.next_audit_ref()?;
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

    fn load_attempt<'a>(
        delivery: &'a DeliveryRecord,
        attempt_id: &DeliveryAttemptId,
    ) -> Result<&'a bus_domain::delivery::DeliveryAttempt, ApplicationError> {
        delivery
            .attempts()
            .iter()
            .find(|candidate| candidate.attempt_id == *attempt_id)
            .ok_or_else(|| {
                ApplicationError::not_found(
                    "not_found.delivery_attempt",
                    format!("delivery attempt {} was not found", attempt_id.as_str()),
                    None,
                )
            })
    }

    fn validate_current_attempt(
        delivery: &DeliveryRecord,
        attempt_id: &DeliveryAttemptId,
        conflict_code: &'static str,
        conflict_message: &'static str,
    ) -> Result<(), ApplicationError> {
        let attempt_ref = DeliveryAttemptRef::new(attempt_id.as_str());
        Self::load_attempt(delivery, attempt_id)?;

        if delivery.last_attempt_ref != Some(attempt_ref) {
            return Err(ApplicationError::conflict(
                conflict_code,
                conflict_message,
                None,
            ));
        }

        Ok(())
    }

    fn validate_feedback_attempt(
        delivery: &DeliveryRecord,
        attempt_id: &DeliveryAttemptId,
    ) -> Result<(), ApplicationError> {
        Self::validate_current_attempt(
            delivery,
            attempt_id,
            "conflict.late_feedback",
            "feedback does not target the current delivery attempt",
        )?;

        let attempt = Self::load_attempt(delivery, attempt_id)?;
        if !attempt.is_finished() {
            return Err(ApplicationError::conflict(
                "conflict.delivery_attempt_unfinished",
                "delivery attempt is not finished",
                None,
            ));
        }

        Ok(())
    }

    fn feedback_audit_entry(
        &self,
        audit_ref: AuditRef,
        delivery_id: bus_contracts::metadata::DeliveryId,
        feedback: &FeedbackResult,
        actor: ActorContext,
        trace_ref: TraceContextRef,
    ) -> BusAuditEntry {
        BusAuditEntry::record(
            audit_ref,
            SubjectRef::Delivery(delivery_id),
            AuditAction::FeedbackRecorded(feedback.status),
            actor,
            trace_ref,
            feedback.observed_at.clone(),
        )
    }

    fn backend_signal_anchor(
        &self,
        scope: IdempotencyScope,
        key: IdempotencyKey,
        request_digest: RequestDigest,
        feedback_id: bus_contracts::metadata::FeedbackId,
        trace_ref: TraceContextRef,
    ) -> Result<IdempotencyAnchor, ApplicationError> {
        Ok(IdempotencyAnchor::bind(
            self.next_anchor_id()?,
            scope,
            key,
            request_digest,
            RecordRef::Feedback(feedback_id),
            self.deps.clock.now(),
            trace_ref,
        ))
    }
}

impl<F, D, I, A, U, C, G, T> DeliveryFeedbackUseCase
    for FeedbackRecordingService<F, D, I, A, U, C, G, T>
where
    F: FeedbackRepository,
    D: DeliveryRepository,
    I: IdempotencyRepository,
    A: AuditTrailRepository,
    U: UnitOfWork,
    C: ClockPort,
    G: IdGeneratorPort,
    T: TransportBackendPort,
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
                return self.load_existing_feedback_result(&existing_anchor).await;
            }

            return self
                .begin_idempotency_conflict(
                    UnitOfWorkPurpose::RecordDeliveryFeedback,
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
        if let Err(error) = Self::validate_feedback_attempt(&delivery, &command.attempt_id) {
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

        let audit_ref = match self.next_audit_ref() {
            Ok(audit_ref) => audit_ref,
            Err(error) => return self.rollback_with(uow, error).await,
        };
        feedback.attach_audit_ref(audit_ref.clone());
        let audit_entry = self.feedback_audit_entry(
            audit_ref.clone(),
            delivery.delivery_id.clone(),
            &feedback,
            actor,
            meta.request.trace_id.clone(),
        );
        let anchor = match self.backend_signal_anchor(
            scope,
            idempotency_key,
            request_digest,
            feedback.feedback_id.clone(),
            meta.request.trace_id,
        ) {
            Ok(anchor) => anchor,
            Err(error) => return self.rollback_with(uow, error).await,
        };

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

impl<F, D, I, A, U, C, G, T> BackendSignalUseCase
    for FeedbackRecordingService<F, D, I, A, U, C, G, T>
where
    F: FeedbackRepository,
    D: DeliveryRepository,
    I: IdempotencyRepository,
    A: AuditTrailRepository,
    U: UnitOfWork,
    C: ClockPort,
    G: IdGeneratorPort,
    T: TransportBackendPort,
{
    async fn record_backend_signal(
        &self,
        input: BackendDeliverySignalInput,
        actor: ActorContext,
        meta: EventMetadata,
    ) -> Result<BackendSignalResult, ApplicationError> {
        let scope = IdempotencyScope::for_backend_signal(&input);
        let request_digest = RequestDigest::from_backend_signal_input(&input)?;

        if let Some(existing_anchor) = self
            .deps
            .idempotency_repository
            .find(&scope, &input.idempotency_key)
            .await?
        {
            if existing_anchor.matches(&request_digest) {
                return self
                    .load_existing_backend_signal_result(
                        &existing_anchor,
                        input.delivery_id,
                        input.attempt_id,
                    )
                    .await;
            }

            return self
                .begin_idempotency_conflict(
                    UnitOfWorkPurpose::ConsumeBackendDeliverySignal,
                    scope,
                    input.idempotency_key,
                    existing_anchor,
                    request_digest,
                    actor,
                    meta.trace_ref,
                )
                .await;
        }

        let uow = self
            .deps
            .unit_of_work
            .begin(
                UnitOfWorkPurpose::ConsumeBackendDeliverySignal,
                actor.clone(),
            )
            .await?;
        let normalized = match self
            .deps
            .transport_backend
            .normalize_signal(input.clone())
            .await
        {
            Ok(normalized) => normalized,
            Err(error) => return self.rollback_with(uow, ApplicationError::from(error)).await,
        };
        let mut delivery = match self
            .deps
            .delivery_repository
            .get_for_update(&input.delivery_id, &uow)
            .await?
        {
            Some(delivery) => delivery,
            None => {
                let audit_ref = match self.next_audit_ref() {
                    Ok(audit_ref) => audit_ref,
                    Err(error) => return self.rollback_with(uow, error).await,
                };
                let audit_entry = BusAuditEntry::record(
                    audit_ref.clone(),
                    SubjectRef::Delivery(input.delivery_id.clone()),
                    AuditAction::BackendSignalIgnored,
                    actor,
                    meta.trace_ref,
                    self.deps.clock.now(),
                );
                if let Err(error) = self.deps.audit_repository.append(audit_entry, &uow).await {
                    return self.rollback_with(uow, ApplicationError::from(error)).await;
                }
                self.deps
                    .unit_of_work
                    .commit(uow)
                    .await
                    .map_err(ApplicationError::from)?;

                return Ok(BackendSignalResult::ignored(
                    input.delivery_id,
                    input.attempt_id,
                    audit_ref,
                ));
            }
        };

        if let Err(error) = Self::validate_current_attempt(
            &delivery,
            &input.attempt_id,
            "conflict.backend_signal_attempt_mismatch",
            "backend signal does not target the current delivery attempt",
        ) {
            return self.rollback_with(uow, error).await;
        }

        let expected_version = delivery.version();
        let from_status = delivery.status;
        let occurred_at = self.deps.clock.now();
        let attempt = match delivery.finish_attempt(
            &input.attempt_id,
            normalized.clone(),
            occurred_at.clone(),
        ) {
            Ok(attempt) => attempt,
            Err(error) => return self.rollback_with(uow, ApplicationError::from(error)).await,
        };
        let source = match FeedbackSource::from_backend_signal(
            input.attempt_id.clone(),
            input.backend_status,
            input.backend_result_ref.clone(),
        ) {
            Ok(source) => source,
            Err(error) => return self.rollback_with(uow, ApplicationError::from(error)).await,
        };
        let mut feedback = match FeedbackResult::from_backend_signal(
            delivery.delivery_id.clone(),
            source,
            input.backend_status,
            actor.clone(),
            occurred_at.clone(),
        ) {
            Ok(feedback) => feedback,
            Err(error) => return self.rollback_with(uow, ApplicationError::from(error)).await,
        };

        match normalized.status {
            BackendDeliveryStatus::Delivered => {
                if let Err(error) = delivery.mark_delivered(attempt, actor.clone()) {
                    return self.rollback_with(uow, ApplicationError::from(error)).await;
                }
                let history = DeliveryHistoryEntry::transition(
                    delivery.delivery_id.clone(),
                    from_status,
                    DeliveryStatus::Delivered,
                    HistoryReason::delivery_arrived(),
                    occurred_at.clone(),
                );
                if let Err(error) = delivery.append_history(history) {
                    return self.rollback_with(uow, ApplicationError::from(error)).await;
                }
            }
            BackendDeliveryStatus::Failed => {
                if let Err(error) = delivery.mark_failed(feedback.failure_reason(), actor.clone()) {
                    return self.rollback_with(uow, ApplicationError::from(error)).await;
                }
                let history = DeliveryHistoryEntry::transition(
                    delivery.delivery_id.clone(),
                    from_status,
                    DeliveryStatus::Failed,
                    HistoryReason::delivery_failed(),
                    occurred_at.clone(),
                );
                if let Err(error) = delivery.append_history(history) {
                    return self.rollback_with(uow, ApplicationError::from(error)).await;
                }
            }
        }

        let audit_ref = match self.next_audit_ref() {
            Ok(audit_ref) => audit_ref,
            Err(error) => return self.rollback_with(uow, error).await,
        };
        feedback.attach_audit_ref(audit_ref.clone());
        let audit_entry = self.feedback_audit_entry(
            audit_ref.clone(),
            delivery.delivery_id.clone(),
            &feedback,
            actor,
            meta.trace_ref.clone(),
        );
        let anchor = match self.backend_signal_anchor(
            scope,
            input.idempotency_key,
            request_digest,
            feedback.feedback_id.clone(),
            meta.trace_ref,
        ) {
            Ok(anchor) => anchor,
            Err(error) => return self.rollback_with(uow, error).await,
        };

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

        let normalized_result = Self::backend_signal_outcome(&feedback)?;

        Ok(BackendSignalResult::recorded(
            feedback.delivery_id,
            input.attempt_id,
            normalized_result,
            feedback.feedback_id,
            audit_ref,
        ))
    }
}

impl<F, D, I, A, U, C, G, T> TimeoutSignalUseCase
    for FeedbackRecordingService<F, D, I, A, U, C, G, T>
where
    F: FeedbackRepository,
    D: DeliveryRepository,
    I: IdempotencyRepository,
    A: AuditTrailRepository,
    U: UnitOfWork,
    C: ClockPort,
    G: IdGeneratorPort,
    T: TransportBackendPort,
{
    async fn record_timeout(
        &self,
        input: DeliveryTimeoutSignalInput,
        actor: ActorContext,
        meta: EventMetadata,
    ) -> Result<TimeoutRecordResult, ApplicationError> {
        let scope = IdempotencyScope::for_timeout_signal(&input);
        let request_digest = RequestDigest::from_timeout_signal_input(&input)?;

        if let Some(existing_anchor) = self
            .deps
            .idempotency_repository
            .find(&scope, &input.idempotency_key)
            .await?
        {
            if existing_anchor.matches(&request_digest) {
                return self
                    .load_existing_timeout_result(&existing_anchor, input.delivery_id)
                    .await;
            }

            return self
                .begin_idempotency_conflict(
                    UnitOfWorkPurpose::ConsumeTimeoutSignal,
                    scope,
                    input.idempotency_key,
                    existing_anchor,
                    request_digest,
                    actor,
                    meta.trace_ref,
                )
                .await;
        }

        let uow = self
            .deps
            .unit_of_work
            .begin(UnitOfWorkPurpose::ConsumeTimeoutSignal, actor.clone())
            .await?;
        let mut delivery = match self
            .deps
            .delivery_repository
            .get_for_update(&input.delivery_id, &uow)
            .await?
        {
            Some(delivery) => delivery,
            None => {
                let audit_ref = match self.next_audit_ref() {
                    Ok(audit_ref) => audit_ref,
                    Err(error) => return self.rollback_with(uow, error).await,
                };
                let audit_entry = BusAuditEntry::record(
                    audit_ref.clone(),
                    SubjectRef::Delivery(input.delivery_id.clone()),
                    AuditAction::TimeoutSignalIgnored,
                    actor,
                    meta.trace_ref,
                    self.deps.clock.now(),
                );
                if let Err(error) = self.deps.audit_repository.append(audit_entry, &uow).await {
                    return self.rollback_with(uow, ApplicationError::from(error)).await;
                }
                self.deps
                    .unit_of_work
                    .commit(uow)
                    .await
                    .map_err(ApplicationError::from)?;

                return Ok(TimeoutRecordResult::ignored(input.delivery_id, audit_ref));
            }
        };

        if let Err(error) = Self::validate_current_attempt(
            &delivery,
            &input.attempt_id,
            "conflict.timeout_attempt_mismatch",
            "timeout signal does not target the current delivery attempt",
        ) {
            return self.rollback_with(uow, error).await;
        }

        let expected_version = delivery.version();
        let from_status = delivery.status;
        match from_status {
            DeliveryStatus::Dispatching => {
                if let Err(error) = delivery.finish_attempt(
                    &input.attempt_id,
                    BackendDeliveryResult::failed(None),
                    input.occurred_at.clone(),
                ) {
                    return self.rollback_with(uow, ApplicationError::from(error)).await;
                }
            }
            DeliveryStatus::Delivered => {
                let attempt = match Self::load_attempt(&delivery, &input.attempt_id) {
                    Ok(attempt) => attempt,
                    Err(error) => return self.rollback_with(uow, error).await,
                };
                if !attempt.is_finished() {
                    return self
                        .rollback_with(
                            uow,
                            ApplicationError::conflict(
                                "conflict.delivery_attempt_unfinished",
                                "delivery attempt is not finished",
                                None,
                            ),
                        )
                        .await;
                }
            }
            _ => {
                return self
                    .rollback_with(
                        uow,
                        ApplicationError::conflict(
                            "conflict.timeout_not_allowed",
                            "timeout signal is only allowed for dispatching or delivered delivery",
                            None,
                        ),
                    )
                    .await;
            }
        }

        let source = match FeedbackSource::from_timeout_signal(
            input.attempt_id.clone(),
            input.timeout_reason,
        ) {
            Ok(source) => source,
            Err(error) => return self.rollback_with(uow, ApplicationError::from(error)).await,
        };
        let mut feedback = match FeedbackResult::timeout(
            delivery.delivery_id.clone(),
            source,
            input.timeout_reason,
            actor.clone(),
            input.occurred_at.clone(),
        ) {
            Ok(feedback) => feedback,
            Err(error) => return self.rollback_with(uow, ApplicationError::from(error)).await,
        };

        if let Err(error) = delivery.mark_failed(feedback.failure_reason(), actor.clone()) {
            return self.rollback_with(uow, ApplicationError::from(error)).await;
        }
        let history = DeliveryHistoryEntry::transition(
            delivery.delivery_id.clone(),
            from_status,
            DeliveryStatus::Failed,
            HistoryReason::feedback_timeout(),
            input.occurred_at.clone(),
        );
        if let Err(error) = delivery.append_history(history) {
            return self.rollback_with(uow, ApplicationError::from(error)).await;
        }

        let recovery_candidate = true;
        let audit_ref = match self.next_audit_ref() {
            Ok(audit_ref) => audit_ref,
            Err(error) => return self.rollback_with(uow, error).await,
        };
        feedback.attach_audit_ref(audit_ref.clone());
        let audit_entry = self.feedback_audit_entry(
            audit_ref.clone(),
            delivery.delivery_id.clone(),
            &feedback,
            actor,
            meta.trace_ref.clone(),
        );
        let anchor = match self.backend_signal_anchor(
            scope,
            input.idempotency_key,
            request_digest,
            feedback.feedback_id.clone(),
            meta.trace_ref,
        ) {
            Ok(anchor) => anchor,
            Err(error) => return self.rollback_with(uow, error).await,
        };

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
            .save(delivery, expected_version, &uow)
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

        Ok(TimeoutRecordResult::recorded(
            feedback.delivery_id,
            feedback.feedback_id,
            recovery_candidate,
            audit_ref,
        ))
    }
}
