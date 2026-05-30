//! Publication acceptance write-path service.

use bus_contracts::commands::AcceptPublicationCommand;
use bus_contracts::events::CommittedOutboxFactInput;
use bus_contracts::metadata::{
    ActorContext, AuditRef, CommandMetadata, EventMetadata, IdempotencyKey,
    PublicationAcceptanceStatus, TraceContextRef,
};
use bus_contracts::receipts::{OutboxRelayResult, PublicationAcceptanceResult};
use bus_domain::audit::{AuditAction, BusAuditEntry, SubjectRef};
use bus_domain::idempotency::{
    IdempotencyAnchor, IdempotencyConflict, IdempotencyScope, RecordRef, RequestDigest,
};
use bus_domain::publication::{
    PayloadBoundaryGuard, PublicationAcceptance, PublicationMaterial, PublicationRejectReason,
};

use crate::errors::{ApplicationError, RepositoryError};
use crate::ports::{
    AuditTrailRepository, BusRecordKind, ClockPort, IdGeneratorPort, IdempotencyRepository,
    PublicationRepository, RollbackReason, UnitOfWork, UnitOfWorkPurpose,
};

/// The publication acceptance use-case contract.
pub trait PublicationAcceptanceUseCase: Send + Sync {
    /// Accepts or rejects publication material.
    async fn accept(
        &self,
        command: AcceptPublicationCommand,
        actor: ActorContext,
        meta: CommandMetadata,
    ) -> Result<PublicationAcceptanceResult, ApplicationError>;
}

/// The committed-outbox publication acceptance use-case contract.
pub trait OutboxPublicationAcceptanceUseCase: Send + Sync {
    /// Accepts or rejects one committed outbox fact.
    async fn accept_from_outbox_fact(
        &self,
        input: CommittedOutboxFactInput,
        actor: ActorContext,
        meta: EventMetadata,
    ) -> Result<OutboxRelayResult, ApplicationError>;
}

/// Dependencies for the publication acceptance service.
pub struct PublicationAcceptanceServiceDeps<P, I, A, U, C, G> {
    /// Publication truth repository.
    pub publication_repository: P,
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

/// The publication acceptance write-path service.
pub struct PublicationAcceptanceService<P, I, A, U, C, G> {
    deps: PublicationAcceptanceServiceDeps<P, I, A, U, C, G>,
    payload_guard: PayloadBoundaryGuard,
}

impl<P, I, A, U, C, G> PublicationAcceptanceService<P, I, A, U, C, G> {
    /// Creates a new publication acceptance service.
    pub fn new(deps: PublicationAcceptanceServiceDeps<P, I, A, U, C, G>) -> Self {
        Self {
            deps,
            payload_guard: PayloadBoundaryGuard::default_for_bus(),
        }
    }
}

impl<P, I, A, U, C, G> PublicationAcceptanceService<P, I, A, U, C, G>
where
    P: PublicationRepository,
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

    fn accepted_result(
        acceptance: &PublicationAcceptance,
    ) -> Result<PublicationAcceptanceResult, ApplicationError> {
        let audit_ref = acceptance
            .decision_audit_ref
            .clone()
            .ok_or_else(|| ApplicationError::from(RepositoryError::CorruptedRecord))?;

        Ok(PublicationAcceptanceResult::accepted(
            acceptance.publication_id.clone(),
            audit_ref,
        ))
    }

    fn rejected_result(
        acceptance: &PublicationAcceptance,
        reason: PublicationRejectReason,
    ) -> Result<PublicationAcceptanceResult, ApplicationError> {
        let audit_ref = acceptance
            .decision_audit_ref
            .clone()
            .ok_or_else(|| ApplicationError::from(RepositoryError::CorruptedRecord))?;

        Ok(PublicationAcceptanceResult::rejected(
            acceptance.publication_id.clone(),
            reason.reason_ref(),
            audit_ref,
        ))
    }

    fn boundary_error_from_result(result: &PublicationAcceptanceResult) -> ApplicationError {
        ApplicationError::boundary_violation(
            "boundary.payload_body_rejected",
            "payload body is not accepted by bus protocol",
            Some(result.audit_ref.as_str().to_owned()),
        )
    }

    fn validation_error_from_rejected_result(
        result: &PublicationAcceptanceResult,
    ) -> ApplicationError {
        ApplicationError::Validation(crate::errors::ValidationError {
            code: "validation.core_event_ref_missing",
            message: "core_event_ref is required".to_owned(),
            details_ref: Some(result.audit_ref.as_str().to_owned()),
        })
    }

    fn error_from_rejected_result(result: &PublicationAcceptanceResult) -> ApplicationError {
        if result.rejection_reason_ref.as_ref()
            == Some(&PublicationRejectReason::PayloadBoundaryViolation.reason_ref())
        {
            Self::boundary_error_from_result(result)
        } else {
            Self::validation_error_from_rejected_result(result)
        }
    }

    async fn load_existing_result(
        &self,
        anchor: &IdempotencyAnchor,
    ) -> Result<PublicationAcceptanceResult, ApplicationError> {
        let publication_id = match &anchor.bound_record_ref {
            RecordRef::Publication(publication_id) => publication_id,
        };

        let acceptance = self
            .deps
            .publication_repository
            .get(publication_id)
            .await?
            .ok_or_else(|| ApplicationError::from(RepositoryError::CorruptedRecord))?;

        match acceptance.status {
            PublicationAcceptanceStatus::Accepted => Self::accepted_result(&acceptance),
            PublicationAcceptanceStatus::Rejected => {
                let reason = acceptance
                    .reject_reason
                    .ok_or_else(|| ApplicationError::from(RepositoryError::CorruptedRecord))?;
                Self::rejected_result(&acceptance, reason)
            }
            PublicationAcceptanceStatus::Pending => {
                Err(ApplicationError::from(RepositoryError::CorruptedRecord))
            }
        }
    }

    fn duplicate_outbox_result(existing_result: &PublicationAcceptanceResult) -> OutboxRelayResult {
        OutboxRelayResult::duplicate(
            existing_result.publication_id.clone(),
            existing_result.audit_ref.clone(),
        )
    }

    async fn begin_idempotency_conflict(
        &self,
        purpose: UnitOfWorkPurpose,
        scope: IdempotencyScope,
        key: IdempotencyKey,
        existing_anchor: IdempotencyAnchor,
        incoming_digest: RequestDigest,
        actor: ActorContext,
        trace_ref: TraceContextRef,
    ) -> Result<PublicationAcceptanceResult, ApplicationError> {
        let handle = self.deps.unit_of_work.begin(purpose, actor.clone()).await?;
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

    async fn commit_publication_decision(
        &self,
        purpose: UnitOfWorkPurpose,
        mut acceptance: PublicationAcceptance,
        actor: ActorContext,
        trace_ref: TraceContextRef,
        request_digest: RequestDigest,
        scope: IdempotencyScope,
        idempotency_key: IdempotencyKey,
        reason: Option<PublicationRejectReason>,
    ) -> Result<PublicationAcceptanceResult, ApplicationError> {
        let handle = self.deps.unit_of_work.begin(purpose, actor.clone()).await?;
        let now = self.deps.clock.now();
        let audit_ref = AuditRef::new(
            self.deps
                .id_generator
                .next_record_id(BusRecordKind::AuditEntry)?,
        );

        if let Some(reason) = reason {
            acceptance
                .reject(reason, actor.clone(), audit_ref.clone())
                .map_err(ApplicationError::from)?;
        } else {
            acceptance
                .accept(actor.clone(), now.clone(), audit_ref.clone())
                .map_err(ApplicationError::from)?;
        }

        let audit_action = match reason {
            Some(reason) => AuditAction::PublicationRejected(reason),
            None => AuditAction::PublicationAccepted,
        };
        let audit_entry = BusAuditEntry::record(
            audit_ref.clone(),
            SubjectRef::Publication(acceptance.publication_id.clone()),
            audit_action,
            actor,
            trace_ref.clone(),
            now.clone(),
        );
        let anchor = IdempotencyAnchor::bind(
            self.deps
                .id_generator
                .next_record_id(BusRecordKind::IdempotencyAnchor)?,
            scope,
            idempotency_key,
            request_digest,
            RecordRef::Publication(acceptance.publication_id.clone()),
            now,
            trace_ref,
        );

        if let Err(error) = self
            .deps
            .publication_repository
            .insert(acceptance.clone(), &handle)
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
        if let Err(error) = self.deps.idempotency_repository.bind(anchor, &handle).await {
            return self
                .rollback_with(handle, ApplicationError::from(error))
                .await;
        }
        self.deps
            .unit_of_work
            .commit(handle)
            .await
            .map_err(ApplicationError::from)?;

        match reason {
            Some(reason) => Self::rejected_result(&acceptance, reason),
            None => Self::accepted_result(&acceptance),
        }
    }
}

impl<P, I, A, U, C, G> PublicationAcceptanceUseCase
    for PublicationAcceptanceService<P, I, A, U, C, G>
where
    P: PublicationRepository,
    I: IdempotencyRepository,
    A: AuditTrailRepository,
    U: UnitOfWork,
    C: ClockPort,
    G: IdGeneratorPort,
{
    async fn accept(
        &self,
        command: AcceptPublicationCommand,
        actor: ActorContext,
        meta: CommandMetadata,
    ) -> Result<PublicationAcceptanceResult, ApplicationError> {
        let idempotency_key = Self::idempotency_key(&meta)?;
        let scope = IdempotencyScope::for_accept_publication_command(&command);
        let request_digest = RequestDigest::from_accept_publication_command(&command)?;

        if let Some(existing_anchor) = self
            .deps
            .idempotency_repository
            .find(&scope, &idempotency_key)
            .await?
        {
            if existing_anchor.matches(&request_digest) {
                let existing_result = self.load_existing_result(&existing_anchor).await?;
                return if existing_result.acceptance_status == PublicationAcceptanceStatus::Accepted
                {
                    Ok(existing_result)
                } else {
                    Err(Self::error_from_rejected_result(&existing_result))
                };
            }

            return self
                .begin_idempotency_conflict(
                    UnitOfWorkPurpose::AcceptPublication,
                    scope,
                    idempotency_key,
                    existing_anchor,
                    request_digest,
                    actor,
                    meta.request.trace_id,
                )
                .await;
        }

        let material = PublicationMaterial::from_accept_publication_command(
            command,
            actor.clone(),
            meta.clone(),
        )?;
        let acceptance = PublicationAcceptance::start_pending(material.clone(), actor.clone())
            .map_err(ApplicationError::from)?;

        if !material.has_core_contract() {
            let result = self
                .commit_publication_decision(
                    UnitOfWorkPurpose::AcceptPublication,
                    acceptance,
                    actor,
                    meta.request.trace_id,
                    request_digest,
                    scope,
                    idempotency_key,
                    Some(PublicationRejectReason::MissingCoreEventRef),
                )
                .await?;
            return Err(Self::validation_error_from_rejected_result(&result));
        }

        if self.payload_guard.rejects_body(material) {
            let result = self
                .commit_publication_decision(
                    UnitOfWorkPurpose::AcceptPublication,
                    acceptance,
                    actor,
                    meta.request.trace_id,
                    request_digest,
                    scope,
                    idempotency_key,
                    Some(PublicationRejectReason::PayloadBoundaryViolation),
                )
                .await?;
            return Err(Self::boundary_error_from_result(&result));
        }

        self.commit_publication_decision(
            UnitOfWorkPurpose::AcceptPublication,
            acceptance,
            actor,
            meta.request.trace_id,
            request_digest,
            scope,
            idempotency_key,
            None,
        )
        .await
    }
}

impl<P, I, A, U, C, G> OutboxPublicationAcceptanceUseCase
    for PublicationAcceptanceService<P, I, A, U, C, G>
where
    P: PublicationRepository,
    I: IdempotencyRepository,
    A: AuditTrailRepository,
    U: UnitOfWork,
    C: ClockPort,
    G: IdGeneratorPort,
{
    async fn accept_from_outbox_fact(
        &self,
        input: CommittedOutboxFactInput,
        actor: ActorContext,
        meta: EventMetadata,
    ) -> Result<OutboxRelayResult, ApplicationError> {
        let idempotency_key = input.idempotency_key.clone();
        let scope = IdempotencyScope::for_outbox_fact(&input);
        let request_digest = RequestDigest::from_outbox_fact_input(&input)?;

        if let Some(existing_anchor) = self
            .deps
            .idempotency_repository
            .find(&scope, &idempotency_key)
            .await?
        {
            if existing_anchor.matches(&request_digest) {
                let existing_result = self.load_existing_result(&existing_anchor).await?;
                return if existing_result.acceptance_status == PublicationAcceptanceStatus::Accepted
                {
                    Ok(Self::duplicate_outbox_result(&existing_result))
                } else {
                    Err(Self::error_from_rejected_result(&existing_result))
                };
            }

            return self
                .begin_idempotency_conflict(
                    UnitOfWorkPurpose::ConsumeCommittedOutboxFact,
                    scope,
                    idempotency_key,
                    existing_anchor,
                    request_digest,
                    actor,
                    meta.trace_ref,
                )
                .await
                .map(|_| unreachable!("outbox conflict always returns an error"));
        }

        let material = PublicationMaterial::from_outbox_fact(input, actor.clone(), meta.clone())?;
        let acceptance = PublicationAcceptance::start_pending(material.clone(), actor.clone())
            .map_err(ApplicationError::from)?;

        if !material.has_core_contract() {
            let result = self
                .commit_publication_decision(
                    UnitOfWorkPurpose::ConsumeCommittedOutboxFact,
                    acceptance,
                    actor,
                    meta.trace_ref,
                    request_digest,
                    scope,
                    idempotency_key,
                    Some(PublicationRejectReason::MissingCoreEventRef),
                )
                .await?;
            return Err(Self::validation_error_from_rejected_result(&result));
        }

        if self.payload_guard.rejects_body(material) {
            let result = self
                .commit_publication_decision(
                    UnitOfWorkPurpose::ConsumeCommittedOutboxFact,
                    acceptance,
                    actor,
                    meta.trace_ref,
                    request_digest,
                    scope,
                    idempotency_key,
                    Some(PublicationRejectReason::PayloadBoundaryViolation),
                )
                .await?;
            return Err(Self::boundary_error_from_result(&result));
        }

        let result = self
            .commit_publication_decision(
                UnitOfWorkPurpose::ConsumeCommittedOutboxFact,
                acceptance,
                actor,
                meta.trace_ref,
                request_digest,
                scope,
                idempotency_key,
                None,
            )
            .await?;

        Ok(OutboxRelayResult::accepted(
            result.publication_id,
            result.audit_ref,
        ))
    }
}
