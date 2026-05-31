//! Recovery orchestration services and retry-cycle job flow.

use bus_contracts::commands::{
    MoveDeliveryToDeadLetterCommand, PrepareReplayCommand, RequestRetryCommand,
};
use bus_contracts::jobs::{RetryCycleResult, RunRetryCycleJob};
use bus_contracts::metadata::{
    ActorContext, AuditRef, BackendDeliveryResult, DeliveryStatus, HistoryReason, JobMetadata,
    RecoveryPolicyConfigRef, RetryScanCursor, Timestamp,
};
use bus_contracts::receipts::{DeadLetterResult, ReplayPreparationResult, RetryPlanResult};
use bus_domain::audit::{
    AuditAction, BusAuditEntry, PrivilegedAccessDecision, PrivilegedAccessRejectionReason,
    PrivilegedAccessScope, SubjectRef,
};
use bus_domain::delivery::{DeliveryHistoryEntry, DeliveryRecord};
use bus_domain::recovery::{
    DeadLetterEntry, FailureMaterial, RecoveryEligibilityPolicy, ReplayPreparation, RetryPlan,
};

use crate::errors::{ApplicationError, RepositoryError, TransportPortError};
use crate::ports::{
    AuditTrailRepository, BackendDispatchContext, BusRecordKind, ClockPort, DeliveryRepository,
    IdGeneratorPort, RecoveryRepository, RollbackReason, TransportBackendPort, UnitOfWork,
    UnitOfWorkHandle, UnitOfWorkPurpose,
};

/// The retry-request use-case contract.
pub trait RequestRetryUseCase: Send + Sync {
    /// Creates one controlled retry plan for a failed delivery.
    async fn request_retry(
        &self,
        command: RequestRetryCommand,
        actor: ActorContext,
        meta: bus_contracts::metadata::CommandMetadata,
    ) -> Result<RetryPlanResult, ApplicationError>;
}

/// The dead-letter use-case contract.
pub trait MoveToDeadLetterUseCase: Send + Sync {
    /// Moves one failed delivery into the dead-letter path.
    async fn move_to_dead_letter(
        &self,
        command: MoveDeliveryToDeadLetterCommand,
        actor: ActorContext,
        meta: bus_contracts::metadata::CommandMetadata,
    ) -> Result<DeadLetterResult, ApplicationError>;
}

/// The retry-cycle job use-case contract.
pub trait RetryCycleUseCase: Send + Sync {
    /// Runs one due-retry batch.
    async fn run_retry_cycle(
        &self,
        job: RunRetryCycleJob,
        actor: ActorContext,
        meta: JobMetadata,
    ) -> Result<RetryCycleResult, ApplicationError>;
}

/// The replay-preparation use-case contract.
pub trait ReplayPreparationUseCase: Send + Sync {
    /// Prepares one replay entry from a dead-letter entry and approval chain.
    async fn prepare(
        &self,
        command: PrepareReplayCommand,
        actor: ActorContext,
        meta: bus_contracts::metadata::CommandMetadata,
    ) -> Result<ReplayPreparationResult, ApplicationError>;
}

/// Dependencies for recovery orchestration.
pub struct RecoveryOrchestrationServiceDeps<D, R, A, U, C, G, T> {
    /// Delivery truth repository.
    pub delivery_repository: D,
    /// Recovery truth repository.
    pub recovery_repository: R,
    /// Audit repository.
    pub audit_repository: A,
    /// Unit-of-work boundary.
    pub unit_of_work: U,
    /// Clock source.
    pub clock: C,
    /// Record ID generator.
    pub id_generator: G,
    /// Backend dispatch adapter.
    pub transport_backend: T,
}

/// Dependencies for replay preparation.
pub struct ReplayPreparationServiceDeps<R, A, U, C, G> {
    /// Recovery truth repository.
    pub recovery_repository: R,
    /// Audit repository.
    pub audit_repository: A,
    /// Unit-of-work boundary.
    pub unit_of_work: U,
    /// Clock source.
    pub clock: C,
    /// Record ID generator.
    pub id_generator: G,
}

/// Recovery orchestration application service.
pub struct RecoveryOrchestrationService<D, R, A, U, C, G, T> {
    deps: RecoveryOrchestrationServiceDeps<D, R, A, U, C, G, T>,
    policy: RecoveryEligibilityPolicy,
}

/// Replay preparation application service.
pub struct ReplayPreparationService<R, A, U, C, G> {
    deps: ReplayPreparationServiceDeps<R, A, U, C, G>,
    policy: RecoveryEligibilityPolicy,
}

enum RetryPlanRunOutcome {
    Retried(RetryPlan),
    Exhausted(RetryPlan),
}

impl<D, R, A, U, C, G, T> RecoveryOrchestrationService<D, R, A, U, C, G, T> {
    /// Creates a new recovery orchestration service.
    pub fn new(deps: RecoveryOrchestrationServiceDeps<D, R, A, U, C, G, T>) -> Self {
        Self {
            deps,
            policy: RecoveryEligibilityPolicy::from_config(RecoveryPolicyConfigRef::new(
                "policy.bus.default_recovery",
            )),
        }
    }
}

impl<R, A, U, C, G> ReplayPreparationService<R, A, U, C, G> {
    /// Creates a new replay-preparation service.
    pub fn new(deps: ReplayPreparationServiceDeps<R, A, U, C, G>) -> Self {
        Self {
            deps,
            policy: RecoveryEligibilityPolicy::from_config(RecoveryPolicyConfigRef::new(
                "policy.bus.default_recovery",
            )),
        }
    }
}

impl<D, R, A, U, C, G, T> RecoveryOrchestrationService<D, R, A, U, C, G, T>
where
    D: DeliveryRepository,
    R: RecoveryRepository,
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

    fn next_audit_ref(&self) -> Result<AuditRef, ApplicationError> {
        Ok(AuditRef::new(
            self.deps
                .id_generator
                .next_record_id(BusRecordKind::AuditEntry)?,
        ))
    }

    fn validate_request_retry(command: &RequestRetryCommand) -> Result<(), ApplicationError> {
        if command.failure_material_ref.as_str().trim().is_empty() {
            return Err(ApplicationError::validation(
                "validation.failure_material_ref",
                "failure_material_ref is required",
            ));
        }
        if command.retry_policy_ref.as_str().trim().is_empty() {
            return Err(ApplicationError::validation(
                "validation.retry_policy_ref",
                "retry_policy_ref is required",
            ));
        }
        if command.requested_reason.as_str().trim().is_empty() {
            return Err(ApplicationError::validation(
                "validation.retry_request_reason",
                "requested_reason is required",
            ));
        }

        Ok(())
    }

    fn validate_move_to_dead_letter(
        command: &MoveDeliveryToDeadLetterCommand,
    ) -> Result<(), ApplicationError> {
        if command.failure_material_ref.as_str().trim().is_empty() {
            return Err(ApplicationError::validation(
                "validation.failure_material_ref",
                "failure_material_ref is required",
            ));
        }
        if command.dead_letter_reason.as_str().trim().is_empty() {
            return Err(ApplicationError::validation(
                "validation.dead_letter_reason",
                "dead_letter_reason is required",
            ));
        }
        if command
            .operator_note_ref
            .as_ref()
            .is_some_and(|value| value.as_str().trim().is_empty())
        {
            return Err(ApplicationError::validation(
                "validation.operator_note_ref",
                "operator_note_ref must not be blank",
            ));
        }

        Ok(())
    }

    fn retry_requested_audit(
        audit_ref: AuditRef,
        retry_plan: &RetryPlan,
        actor: ActorContext,
        trace_ref: bus_contracts::metadata::TraceContextRef,
        occurred_at: Timestamp,
    ) -> BusAuditEntry {
        BusAuditEntry::record(
            audit_ref,
            SubjectRef::RetryPlan(retry_plan.retry_plan_id.clone()),
            AuditAction::RetryRequested,
            actor,
            trace_ref,
            occurred_at,
        )
    }

    fn retry_attempted_audit(
        audit_ref: AuditRef,
        retry_plan: &RetryPlan,
        actor: ActorContext,
        trace_ref: bus_contracts::metadata::TraceContextRef,
        occurred_at: Timestamp,
    ) -> BusAuditEntry {
        BusAuditEntry::record(
            audit_ref,
            SubjectRef::RetryPlan(retry_plan.retry_plan_id.clone()),
            AuditAction::RetryAttempted,
            actor,
            trace_ref,
            occurred_at,
        )
    }

    fn retry_exhausted_audit(
        audit_ref: AuditRef,
        retry_plan: &RetryPlan,
        actor: ActorContext,
        trace_ref: bus_contracts::metadata::TraceContextRef,
        occurred_at: Timestamp,
    ) -> BusAuditEntry {
        BusAuditEntry::record(
            audit_ref,
            SubjectRef::RetryPlan(retry_plan.retry_plan_id.clone()),
            AuditAction::RetryExhausted,
            actor,
            trace_ref,
            occurred_at,
        )
    }

    fn dead_letter_created_audit(
        audit_ref: AuditRef,
        entry: &DeadLetterEntry,
        actor: ActorContext,
        trace_ref: bus_contracts::metadata::TraceContextRef,
        occurred_at: Timestamp,
    ) -> BusAuditEntry {
        BusAuditEntry::record(
            audit_ref,
            SubjectRef::DeadLetter(entry.dead_letter_id.clone()),
            AuditAction::DeadLetterCreated,
            actor,
            trace_ref,
            occurred_at,
        )
    }

    async fn load_failed_delivery(
        &self,
        delivery_id: &bus_contracts::metadata::DeliveryId,
        uow: &UnitOfWorkHandle,
    ) -> Result<DeliveryRecord, ApplicationError> {
        self.deps
            .delivery_repository
            .get_for_update(delivery_id, uow)
            .await?
            .ok_or_else(|| {
                ApplicationError::not_found(
                    "not_found.delivery",
                    format!("delivery {} was not found", delivery_id.as_str()),
                    None,
                )
            })
    }

    async fn load_failure_material(
        &self,
        failure_material_id: &bus_contracts::metadata::FailureMaterialId,
    ) -> Result<FailureMaterial, ApplicationError> {
        self.deps
            .recovery_repository
            .get_failure_material(failure_material_id)
            .await?
            .ok_or_else(|| {
                ApplicationError::not_found(
                    "not_found.failure_material",
                    format!(
                        "failure material {} was not found",
                        failure_material_id.as_str()
                    ),
                    None,
                )
            })
    }

    async fn run_retry_plan(
        &self,
        mut retry_plan: RetryPlan,
        actor: ActorContext,
        meta: JobMetadata,
        scan_time: Timestamp,
    ) -> Result<RetryPlanRunOutcome, ApplicationError> {
        let uow = self
            .deps
            .unit_of_work
            .begin(UnitOfWorkPurpose::RunRetryCycle, actor.clone())
            .await?;
        let mut delivery = match self
            .deps
            .delivery_repository
            .get_for_update(retry_plan.delivery_id(), &uow)
            .await?
        {
            Some(delivery) => delivery,
            None => {
                return self
                    .rollback_with(
                        uow,
                        ApplicationError::not_found(
                            "not_found.delivery",
                            format!(
                                "delivery {} was not found",
                                retry_plan.delivery_id().as_str()
                            ),
                            None,
                        ),
                    )
                    .await;
            }
        };
        let expected_delivery_version = delivery.version();
        let expected_retry_version = retry_plan.version();

        if !retry_plan.has_remaining_attempts() {
            if let Err(error) = retry_plan.mark_exhausted(actor.clone()) {
                return self.rollback_with(uow, ApplicationError::from(error)).await;
            }
            if let Err(error) = self
                .deps
                .recovery_repository
                .save_retry_plan(retry_plan.clone(), Some(expected_retry_version), &uow)
                .await
            {
                return self.rollback_with(uow, ApplicationError::from(error)).await;
            }
            let audit_ref = self.next_audit_ref()?;
            let audit = Self::retry_exhausted_audit(
                audit_ref,
                &retry_plan,
                actor,
                meta.trace_ref,
                scan_time,
            );
            if let Err(error) = self.deps.audit_repository.append(audit, &uow).await {
                return self.rollback_with(uow, ApplicationError::from(error)).await;
            }
            self.deps
                .unit_of_work
                .commit(uow)
                .await
                .map_err(ApplicationError::from)?;

            return Ok(RetryPlanRunOutcome::Exhausted(retry_plan));
        }

        if delivery.status == DeliveryStatus::Failed {
            if let Err(error) = delivery.reschedule_for_retry(actor.clone()) {
                return self.rollback_with(uow, ApplicationError::from(error)).await;
            }
            let rescheduled_history = DeliveryHistoryEntry::transition(
                delivery.delivery_id.clone(),
                DeliveryStatus::Failed,
                DeliveryStatus::Scheduled,
                HistoryReason::retry_rescheduled(),
                scan_time.clone(),
            );
            if let Err(error) = delivery.append_history(rescheduled_history) {
                return self.rollback_with(uow, ApplicationError::from(error)).await;
            }
        }

        let mut attempt = match delivery
            .start_attempt(delivery.backend_capability_ref().clone(), scan_time.clone())
        {
            Ok(attempt) => attempt,
            Err(error) => return self.rollback_with(uow, ApplicationError::from(error)).await,
        };
        let dispatching_history = DeliveryHistoryEntry::transition(
            delivery.delivery_id.clone(),
            DeliveryStatus::Scheduled,
            DeliveryStatus::Dispatching,
            HistoryReason::dispatching_started(),
            scan_time.clone(),
        );
        if let Err(error) = delivery.append_history(dispatching_history) {
            return self.rollback_with(uow, ApplicationError::from(error)).await;
        }

        let backend_id = bus_contracts::metadata::BackendId::new(
            delivery.backend_capability_ref().capability_id.as_str(),
        );
        let dispatch_result = self
            .deps
            .transport_backend
            .dispatch(
                delivery.transport_semantic().clone(),
                attempt.clone(),
                BackendDispatchContext::from_job(meta.clone(), backend_id),
            )
            .await;
        let finished_at = scan_time.clone();
        let retry_result = match dispatch_result {
            Ok(result) => {
                if let Err(error) = attempt.finish(result.clone(), finished_at.clone()) {
                    return self.rollback_with(uow, ApplicationError::from(error)).await;
                }
                if let Err(error) = delivery.mark_delivered(attempt.clone(), actor.clone()) {
                    return self.rollback_with(uow, ApplicationError::from(error)).await;
                }
                let delivered_history = DeliveryHistoryEntry::transition(
                    delivery.delivery_id.clone(),
                    DeliveryStatus::Dispatching,
                    DeliveryStatus::Delivered,
                    HistoryReason::delivery_arrived(),
                    finished_at.clone(),
                );
                if let Err(error) = delivery.append_history(delivered_history) {
                    return self.rollback_with(uow, ApplicationError::from(error)).await;
                }
                result
            }
            Err(error) => {
                let normalized = BackendDeliveryResult::failed(None);
                if let Err(domain_error) = attempt.finish(normalized.clone(), finished_at.clone()) {
                    return self
                        .rollback_with(uow, ApplicationError::from(domain_error))
                        .await;
                }
                if let Err(domain_error) = delivery.sync_attempt(attempt.clone()) {
                    return self
                        .rollback_with(uow, ApplicationError::from(domain_error))
                        .await;
                }
                let failure_reason = match error {
                    TransportPortError::BackendUnavailable
                    | TransportPortError::DispatchTimeout => {
                        bus_contracts::metadata::FailureReason::backend_unavailable()
                    }
                    _ => bus_contracts::metadata::FailureReason::dispatch_failed(),
                };
                if let Err(domain_error) = delivery.mark_failed(failure_reason, actor.clone()) {
                    return self
                        .rollback_with(uow, ApplicationError::from(domain_error))
                        .await;
                }
                let failed_history = DeliveryHistoryEntry::transition(
                    delivery.delivery_id.clone(),
                    DeliveryStatus::Dispatching,
                    DeliveryStatus::Failed,
                    HistoryReason::delivery_failed(),
                    finished_at.clone(),
                );
                if let Err(domain_error) = delivery.append_history(failed_history) {
                    return self
                        .rollback_with(uow, ApplicationError::from(domain_error))
                        .await;
                }
                normalized
            }
        };

        if let Err(error) = retry_plan.mark_attempted(attempt.attempt_id.clone(), retry_result) {
            return self.rollback_with(uow, ApplicationError::from(error)).await;
        }
        if let Err(error) = self
            .deps
            .delivery_repository
            .save(delivery, expected_delivery_version, &uow)
            .await
        {
            return self.rollback_with(uow, ApplicationError::from(error)).await;
        }
        if let Err(error) = self
            .deps
            .recovery_repository
            .save_retry_plan(retry_plan.clone(), Some(expected_retry_version), &uow)
            .await
        {
            return self.rollback_with(uow, ApplicationError::from(error)).await;
        }

        let audit_ref = self.next_audit_ref()?;
        let audit =
            Self::retry_attempted_audit(audit_ref, &retry_plan, actor, meta.trace_ref, finished_at);
        if let Err(error) = self.deps.audit_repository.append(audit, &uow).await {
            return self.rollback_with(uow, ApplicationError::from(error)).await;
        }
        self.deps
            .unit_of_work
            .commit(uow)
            .await
            .map_err(ApplicationError::from)?;

        Ok(RetryPlanRunOutcome::Retried(retry_plan))
    }
}

impl<D, R, A, U, C, G, T> RequestRetryUseCase for RecoveryOrchestrationService<D, R, A, U, C, G, T>
where
    D: DeliveryRepository,
    R: RecoveryRepository,
    A: AuditTrailRepository,
    U: UnitOfWork,
    C: ClockPort,
    G: IdGeneratorPort,
    T: TransportBackendPort,
{
    async fn request_retry(
        &self,
        command: RequestRetryCommand,
        actor: ActorContext,
        meta: bus_contracts::metadata::CommandMetadata,
    ) -> Result<RetryPlanResult, ApplicationError> {
        Self::validate_request_retry(&command)?;

        let uow = self
            .deps
            .unit_of_work
            .begin(UnitOfWorkPurpose::RequestRetry, actor.clone())
            .await?;
        let delivery = match self.load_failed_delivery(&command.delivery_id, &uow).await {
            Ok(delivery) => delivery,
            Err(error) => return self.rollback_with(uow, error).await,
        };
        let material = match self
            .load_failure_material(&command.failure_material_ref.clone().into())
            .await
        {
            Ok(material) => material,
            Err(error) => return self.rollback_with(uow, error).await,
        };
        let now = self.deps.clock.now();
        let retry_plan = match RetryPlan::create(
            delivery.clone(),
            material.failure_reason.clone(),
            command.retry_policy_ref,
            command.max_attempts,
            now.clone(),
        ) {
            Ok(retry_plan) => retry_plan,
            Err(error) => return self.rollback_with(uow, ApplicationError::from(error)).await,
        };
        if let Err(error) = self.policy.can_retry(delivery, retry_plan.clone()) {
            return self.rollback_with(uow, ApplicationError::from(error)).await;
        }
        if let Err(error) = self
            .deps
            .recovery_repository
            .save_retry_plan(retry_plan.clone(), None, &uow)
            .await
        {
            let application_error = match error {
                RepositoryError::UniqueViolation => ApplicationError::conflict(
                    "conflict.retry_plan_active",
                    "an active retry plan already exists for the delivery",
                    None,
                ),
                _ => ApplicationError::from(error),
            };
            return self.rollback_with(uow, application_error).await;
        }

        let audit_ref = self.next_audit_ref()?;
        let audit = Self::retry_requested_audit(
            audit_ref.clone(),
            &retry_plan,
            actor,
            meta.request.trace_id,
            now,
        );
        if let Err(error) = self.deps.audit_repository.append(audit, &uow).await {
            return self.rollback_with(uow, ApplicationError::from(error)).await;
        }
        self.deps
            .unit_of_work
            .commit(uow)
            .await
            .map_err(ApplicationError::from)?;

        Ok(RetryPlanResult::scheduled(
            retry_plan.retry_plan_id,
            retry_plan.delivery_id,
            retry_plan.remaining_attempts,
            retry_plan.next_attempt_at,
            audit_ref,
        ))
    }
}

impl<D, R, A, U, C, G, T> MoveToDeadLetterUseCase
    for RecoveryOrchestrationService<D, R, A, U, C, G, T>
where
    D: DeliveryRepository,
    R: RecoveryRepository,
    A: AuditTrailRepository,
    U: UnitOfWork,
    C: ClockPort,
    G: IdGeneratorPort,
    T: TransportBackendPort,
{
    async fn move_to_dead_letter(
        &self,
        command: MoveDeliveryToDeadLetterCommand,
        actor: ActorContext,
        meta: bus_contracts::metadata::CommandMetadata,
    ) -> Result<DeadLetterResult, ApplicationError> {
        Self::validate_move_to_dead_letter(&command)?;

        let uow = self
            .deps
            .unit_of_work
            .begin(UnitOfWorkPurpose::MoveDeliveryToDeadLetter, actor.clone())
            .await?;
        let mut delivery = match self.load_failed_delivery(&command.delivery_id, &uow).await {
            Ok(delivery) => delivery,
            Err(error) => return self.rollback_with(uow, error).await,
        };
        let expected_version = delivery.version();
        let material = match self
            .load_failure_material(&command.failure_material_ref.clone().into())
            .await
        {
            Ok(material) => material,
            Err(error) => return self.rollback_with(uow, error).await,
        };
        let history = match self
            .deps
            .delivery_repository
            .load_history(&command.delivery_id)
            .await
        {
            Ok(history) => history,
            Err(error) => return self.rollback_with(uow, ApplicationError::from(error)).await,
        };
        if history
            .last()
            .map(|entry| entry.to_status != DeliveryStatus::Failed)
            .unwrap_or(true)
        {
            return self
                .rollback_with(
                    uow,
                    ApplicationError::conflict(
                        "conflict.dead_letter_history",
                        "delivery is missing the failed history required for dead letter",
                        None,
                    ),
                )
                .await;
        }
        if let Err(error) = self
            .policy
            .can_dead_letter(delivery.clone(), material.clone())
        {
            return self.rollback_with(uow, ApplicationError::from(error)).await;
        }
        let entry = match DeadLetterEntry::from_failed_delivery(delivery.clone(), material.clone())
        {
            Ok(entry) => entry,
            Err(error) => return self.rollback_with(uow, ApplicationError::from(error)).await,
        };
        if let Err(error) = delivery.mark_dead_lettered(entry.dead_letter_id.clone(), actor.clone())
        {
            return self.rollback_with(uow, ApplicationError::from(error)).await;
        }
        let occurred_at = self.deps.clock.now();
        let history_entry = DeliveryHistoryEntry::transition(
            delivery.delivery_id.clone(),
            DeliveryStatus::Failed,
            DeliveryStatus::DeadLettered,
            HistoryReason::dead_lettered(),
            occurred_at.clone(),
        );
        if let Err(error) = delivery.append_history(history_entry) {
            return self.rollback_with(uow, ApplicationError::from(error)).await;
        }
        if let Err(error) = self
            .deps
            .recovery_repository
            .save_dead_letter(entry.clone(), material.clone(), &uow)
            .await
        {
            let application_error = match error {
                RepositoryError::UniqueViolation => ApplicationError::conflict(
                    "conflict.dead_letter_active",
                    "an active dead-letter entry already exists for the delivery",
                    None,
                ),
                _ => ApplicationError::from(error),
            };
            return self.rollback_with(uow, application_error).await;
        }
        if let Err(error) = self
            .deps
            .delivery_repository
            .save(delivery.clone(), expected_version, &uow)
            .await
        {
            return self.rollback_with(uow, ApplicationError::from(error)).await;
        }

        let audit_ref = self.next_audit_ref()?;
        let audit = Self::dead_letter_created_audit(
            audit_ref.clone(),
            &entry,
            actor,
            meta.request.trace_id,
            occurred_at,
        );
        if let Err(error) = self.deps.audit_repository.append(audit, &uow).await {
            return self.rollback_with(uow, ApplicationError::from(error)).await;
        }
        self.deps
            .unit_of_work
            .commit(uow)
            .await
            .map_err(ApplicationError::from)?;

        Ok(DeadLetterResult::opened(
            entry.dead_letter_id,
            delivery.delivery_id,
            material.failure_material_id.into(),
            audit_ref,
        ))
    }
}

impl<D, R, A, U, C, G, T> RetryCycleUseCase for RecoveryOrchestrationService<D, R, A, U, C, G, T>
where
    D: DeliveryRepository,
    R: RecoveryRepository,
    A: AuditTrailRepository,
    U: UnitOfWork,
    C: ClockPort,
    G: IdGeneratorPort,
    T: TransportBackendPort,
{
    async fn run_retry_cycle(
        &self,
        job: RunRetryCycleJob,
        actor: ActorContext,
        meta: JobMetadata,
    ) -> Result<RetryCycleResult, ApplicationError> {
        let retry_plans = self
            .deps
            .recovery_repository
            .find_due_retry(
                job.cursor.clone(),
                bus_contracts::metadata::PageLimit::new(job.batch_size),
                job.now.clone(),
            )
            .await?;
        let mut result = RetryCycleResult::start(meta.job_run_id.clone(), job.cursor);

        if let Some(last) = retry_plans.last() {
            result.set_next_cursor(RetryScanCursor::new(last.retry_plan_id.as_str()));
        }

        for retry_plan in retry_plans {
            match self
                .run_retry_plan(retry_plan, actor.clone(), meta.clone(), job.now.clone())
                .await
            {
                Ok(RetryPlanRunOutcome::Retried(plan)) => {
                    let _ = plan;
                    result.record_retried();
                }
                Ok(RetryPlanRunOutcome::Exhausted(plan)) => {
                    let _ = plan;
                    result.record_exhausted();
                }
                Err(_) => result.record_failed(),
            }
        }

        Ok(result)
    }
}

impl<R, A, U, C, G> ReplayPreparationService<R, A, U, C, G>
where
    R: RecoveryRepository,
    A: AuditTrailRepository,
    U: UnitOfWork,
    C: ClockPort,
    G: IdGeneratorPort,
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

    fn next_audit_ref(&self) -> Result<AuditRef, ApplicationError> {
        Ok(AuditRef::new(
            self.deps
                .id_generator
                .next_record_id(BusRecordKind::AuditEntry)?,
        ))
    }

    fn validate_prepare_replay(command: &PrepareReplayCommand) -> Result<(), ApplicationError> {
        if command.audit_chain_ref.as_str().trim().is_empty() {
            return Err(ApplicationError::validation(
                "validation.audit_chain_ref",
                "audit_chain_ref is required",
            ));
        }
        if command.approval_ref.as_str().trim().is_empty() {
            return Err(ApplicationError::validation(
                "validation.replay_approval_ref",
                "approval_ref is required",
            ));
        }
        if command.replay_reason.as_str().trim().is_empty() {
            return Err(ApplicationError::validation(
                "validation.replay_reason",
                "replay_reason is required",
            ));
        }

        Ok(())
    }

    fn validate_privileged_actor(
        actor: &ActorContext,
    ) -> Result<(), PrivilegedAccessRejectionReason> {
        if actor.role_refs.is_empty() {
            return Err(PrivilegedAccessRejectionReason::MissingRoleHint);
        }

        Ok(())
    }

    fn privileged_access_error(reason: PrivilegedAccessRejectionReason) -> ApplicationError {
        match reason {
            PrivilegedAccessRejectionReason::MissingAuthorizationRef => {
                ApplicationError::boundary_violation(
                    "boundary.authorization_ref_required",
                    "a trusted authorization_ref is required for replay preparation",
                    None,
                )
            }
            PrivilegedAccessRejectionReason::MissingRoleHint => {
                ApplicationError::boundary_violation(
                    "boundary.privileged_role_hint_required",
                    "a trusted actor role hint is required for replay preparation",
                    None,
                )
            }
        }
    }

    async fn append_access_audit(
        &self,
        subject_ref: SubjectRef,
        decision: PrivilegedAccessDecision,
        actor: ActorContext,
        meta: &bus_contracts::metadata::CommandMetadata,
    ) -> Result<(), ApplicationError> {
        let entry = BusAuditEntry::record(
            self.next_audit_ref()?,
            subject_ref,
            AuditAction::PrivilegedAccess {
                scope: PrivilegedAccessScope::ReplayPreparation,
                decision,
            },
            actor,
            meta.request.trace_id.clone(),
            meta.request.requested_at.clone(),
        );

        self.deps
            .audit_repository
            .append_access(entry)
            .await
            .map(|_| ())
            .map_err(ApplicationError::from)
    }
}

impl<R, A, U, C, G> ReplayPreparationUseCase for ReplayPreparationService<R, A, U, C, G>
where
    R: RecoveryRepository,
    A: AuditTrailRepository,
    U: UnitOfWork,
    C: ClockPort,
    G: IdGeneratorPort,
{
    async fn prepare(
        &self,
        command: PrepareReplayCommand,
        actor: ActorContext,
        meta: bus_contracts::metadata::CommandMetadata,
    ) -> Result<ReplayPreparationResult, ApplicationError> {
        Self::validate_prepare_replay(&command)?;
        if let Err(reason) = Self::validate_privileged_actor(&actor) {
            self.append_access_audit(
                SubjectRef::DeadLetter(command.dead_letter_id.clone()),
                PrivilegedAccessDecision::Rejected(reason),
                actor,
                &meta,
            )
            .await?;
            return Err(Self::privileged_access_error(reason));
        }

        let uow = self
            .deps
            .unit_of_work
            .begin(UnitOfWorkPurpose::PrepareReplay, actor.clone())
            .await?;
        let entry = match self
            .deps
            .recovery_repository
            .get_dead_letter(&command.dead_letter_id)
            .await?
        {
            Some(entry) => entry,
            None => {
                return self
                    .rollback_with(
                        uow,
                        ApplicationError::not_found(
                            "not_found.dead_letter",
                            format!(
                                "dead letter {} was not found",
                                command.dead_letter_id.as_str()
                            ),
                            None,
                        ),
                    )
                    .await;
            }
        };
        let audit_chain = self
            .deps
            .audit_repository
            .load_chain(&command.audit_chain_ref)
            .await?;
        if audit_chain.is_empty() {
            return self
                .rollback_with(
                    uow,
                    ApplicationError::boundary_violation(
                        "boundary.audit_chain_missing",
                        "a trusted audit chain is required for replay preparation",
                        None,
                    ),
                )
                .await;
        }
        if let Err(error) = self
            .policy
            .can_prepare_replay(entry.clone(), command.audit_chain_ref.clone())
        {
            return self.rollback_with(uow, ApplicationError::from(error)).await;
        }
        let mut preparation = match ReplayPreparation::prepare(entry, actor.clone()) {
            Ok(preparation) => preparation,
            Err(error) => return self.rollback_with(uow, ApplicationError::from(error)).await,
        };
        if let Err(error) = preparation.mark_ready(command.approval_ref, actor.clone()) {
            return self.rollback_with(uow, ApplicationError::from(error)).await;
        }
        if let Err(error) = self
            .deps
            .recovery_repository
            .save_replay_preparation(preparation.clone(), &uow)
            .await
        {
            let application_error = match error {
                RepositoryError::UniqueViolation => ApplicationError::conflict(
                    "conflict.replay_preparation_exists",
                    "a replay preparation already exists for the dead letter and approval",
                    None,
                ),
                _ => ApplicationError::from(error),
            };
            return self.rollback_with(uow, application_error).await;
        }
        let occurred_at = self.deps.clock.now();
        let audit_ref = self.next_audit_ref()?;
        let audit = BusAuditEntry::record(
            audit_ref.clone(),
            SubjectRef::ReplayPreparation(preparation.replay_id.clone()),
            AuditAction::ReplayPreparationReady,
            actor,
            meta.request.trace_id,
            occurred_at,
        );
        if let Err(error) = self.deps.audit_repository.append(audit, &uow).await {
            return self.rollback_with(uow, ApplicationError::from(error)).await;
        }
        self.deps
            .unit_of_work
            .commit(uow)
            .await
            .map_err(ApplicationError::from)?;

        Ok(ReplayPreparationResult::ready(
            preparation.replay_id,
            preparation.dead_letter_id,
            audit_ref,
        ))
    }
}
