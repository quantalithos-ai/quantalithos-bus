//! Delivery progression write-path service.

use bus_contracts::jobs::{DeliveryProgressionResult, RunDeliveryProgressionJob};
use bus_contracts::metadata::{
    ActorContext, AuditRef, DeliveryId, DeliveryScanCursor, DeliveryStatus, HistoryReason,
    JobMetadata, Timestamp,
};
use bus_domain::audit::{AuditAction, BusAuditEntry, SubjectRef};
use bus_domain::delivery::{DeliveryHistoryEntry, DeliveryRecord};

use crate::errors::{ApplicationError, TransportPortError};
use crate::ports::{
    AuditTrailRepository, ClockPort, DeliveryRepository, RollbackReason, TransportBackendPort,
    UnitOfWork, UnitOfWorkPurpose,
};

/// The delivery progression use-case contract.
pub trait DeliveryProgressionUseCase: Send + Sync {
    /// Progresses one schedulable batch through the default backend path.
    async fn progress_batch(
        &self,
        job: RunDeliveryProgressionJob,
        actor: ActorContext,
        meta: JobMetadata,
    ) -> Result<DeliveryProgressionResult, ApplicationError>;
}

/// Dependencies for the delivery progression service.
pub struct DeliveryProgressionServiceDeps<R, T, A, U, C> {
    /// Delivery truth repository.
    pub delivery_repository: R,
    /// Transport backend port.
    pub transport_backend: T,
    /// Audit repository.
    pub audit_repository: A,
    /// Unit-of-work boundary.
    pub unit_of_work: U,
    /// Clock source.
    pub clock: C,
}

/// The result of one committed delivery progression item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeliveryProgressionItemResult {
    /// The delivery identifier.
    pub delivery_id: DeliveryId,
    /// The final committed status after progression.
    pub final_status: DeliveryStatus,
    /// Whether the failure evidence is retryable.
    pub retryable: bool,
    /// Whether manual intervention is required.
    pub manual_action_required: bool,
}

impl DeliveryProgressionItemResult {
    fn from_delivery(
        delivery: &DeliveryRecord,
        retryable: bool,
        manual_action_required: bool,
    ) -> Self {
        Self {
            delivery_id: delivery.delivery_id.clone(),
            final_status: delivery.status,
            retryable,
            manual_action_required,
        }
    }
}

/// Delivery progression application service.
pub struct DeliveryProgressionService<R, T, A, U, C> {
    deps: DeliveryProgressionServiceDeps<R, T, A, U, C>,
}

impl<R, T, A, U, C> DeliveryProgressionService<R, T, A, U, C> {
    /// Creates a new delivery progression service.
    pub fn new(deps: DeliveryProgressionServiceDeps<R, T, A, U, C>) -> Self {
        Self { deps }
    }
}

impl<R, T, A, U, C> DeliveryProgressionService<R, T, A, U, C>
where
    R: DeliveryRepository,
    T: TransportBackendPort,
    A: AuditTrailRepository,
    U: UnitOfWork,
    C: ClockPort,
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

    async fn progress_one(
        &self,
        delivery_id: DeliveryId,
        backend_id: bus_contracts::metadata::BackendId,
        actor: ActorContext,
        meta: JobMetadata,
    ) -> Result<DeliveryProgressionItemResult, ApplicationError> {
        let uow = self
            .deps
            .unit_of_work
            .begin(UnitOfWorkPurpose::RunDeliveryProgression, actor.clone())
            .await?;
        let mut delivery = match self
            .deps
            .delivery_repository
            .get_for_update(&delivery_id, &uow)
            .await?
        {
            Some(delivery) => delivery,
            None => {
                return self
                    .rollback_with(
                        uow,
                        ApplicationError::not_found(
                            "not_found.delivery",
                            format!("delivery {} was not found", delivery_id.as_str()),
                            None,
                        ),
                    )
                    .await;
            }
        };
        let expected_version = delivery.version();
        let started_at = self.deps.clock.now();
        let from_status = delivery.status;
        let mut attempt = match delivery.start_attempt(
            delivery.backend_capability_ref().clone(),
            started_at.clone(),
        ) {
            Ok(attempt) => attempt,
            Err(error) => return self.rollback_with(uow, ApplicationError::from(error)).await,
        };

        let dispatching_history = DeliveryHistoryEntry::transition(
            delivery.delivery_id.clone(),
            from_status,
            DeliveryStatus::Dispatching,
            HistoryReason::dispatching_started(),
            started_at.clone(),
        );
        if let Err(error) = delivery.append_history(dispatching_history) {
            return self.rollback_with(uow, ApplicationError::from(error)).await;
        }

        let dispatch_context =
            crate::ports::BackendDispatchContext::from_job(meta.clone(), backend_id);
        let dispatch_result = self
            .deps
            .transport_backend
            .dispatch(
                delivery.transport_semantic().clone(),
                attempt.clone(),
                dispatch_context,
            )
            .await;
        let finished_at = self.deps.clock.now();
        let mut retryable = false;
        let mut manual_action_required = false;
        let second_audit;

        match dispatch_result {
            Ok(result) => {
                if let Err(error) = attempt.finish(result, finished_at.clone()) {
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
                second_audit = BusAuditEntry::record(
                    audit_ref_for(&delivery.delivery_id, "delivered", &finished_at),
                    SubjectRef::Delivery(delivery.delivery_id.clone()),
                    AuditAction::DeliveryDelivered,
                    actor.clone(),
                    meta.trace_ref.clone(),
                    finished_at.clone(),
                );
            }
            Err(error) => {
                retryable = matches!(
                    error,
                    TransportPortError::BackendUnavailable | TransportPortError::DispatchTimeout
                );
                manual_action_required = matches!(error, TransportPortError::PrivateBodyViolation);
                if let Err(domain_error) = attempt.finish(
                    bus_contracts::metadata::BackendDeliveryResult::failed(None),
                    finished_at.clone(),
                ) {
                    return self
                        .rollback_with(uow, ApplicationError::from(domain_error))
                        .await;
                }
                if let Err(domain_error) = delivery.sync_attempt(attempt.clone()) {
                    return self
                        .rollback_with(uow, ApplicationError::from(domain_error))
                        .await;
                }
                let failure_reason = if retryable {
                    bus_contracts::metadata::FailureReason::backend_unavailable()
                } else {
                    bus_contracts::metadata::FailureReason::dispatch_failed()
                };
                if let Err(domain_error) =
                    delivery.mark_failed(failure_reason.clone(), actor.clone())
                {
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
                second_audit = BusAuditEntry::record(
                    audit_ref_for(&delivery.delivery_id, "failed", &finished_at),
                    SubjectRef::Delivery(delivery.delivery_id.clone()),
                    AuditAction::DeliveryFailed(failure_reason),
                    actor.clone(),
                    meta.trace_ref.clone(),
                    finished_at.clone(),
                );
            }
        }

        let started_audit = BusAuditEntry::record(
            audit_ref_for(&delivery.delivery_id, "dispatching", &started_at),
            SubjectRef::Delivery(delivery.delivery_id.clone()),
            AuditAction::DeliveryDispatchStarted,
            actor,
            meta.trace_ref,
            started_at,
        );
        if let Err(error) = self
            .deps
            .delivery_repository
            .save(delivery.clone(), expected_version, &uow)
            .await
        {
            return self.rollback_with(uow, ApplicationError::from(error)).await;
        }
        if let Err(error) = self.deps.audit_repository.append(started_audit, &uow).await {
            return self.rollback_with(uow, ApplicationError::from(error)).await;
        }
        if let Err(error) = self.deps.audit_repository.append(second_audit, &uow).await {
            return self.rollback_with(uow, ApplicationError::from(error)).await;
        }
        self.deps
            .unit_of_work
            .commit(uow)
            .await
            .map_err(ApplicationError::from)?;

        Ok(DeliveryProgressionItemResult::from_delivery(
            &delivery,
            retryable,
            manual_action_required,
        ))
    }
}

impl<R, T, A, U, C> DeliveryProgressionUseCase for DeliveryProgressionService<R, T, A, U, C>
where
    R: DeliveryRepository,
    T: TransportBackendPort,
    A: AuditTrailRepository,
    U: UnitOfWork,
    C: ClockPort,
{
    async fn progress_batch(
        &self,
        job: RunDeliveryProgressionJob,
        actor: ActorContext,
        meta: JobMetadata,
    ) -> Result<DeliveryProgressionResult, ApplicationError> {
        let deliveries = self
            .deps
            .delivery_repository
            .find_schedulable(job.cursor.clone(), job.batch_size)
            .await?;
        let next_cursor = deliveries
            .last()
            .map(|delivery| DeliveryScanCursor::new(delivery.delivery_id.as_str()))
            .unwrap_or_else(|| job.cursor.clone());
        let mut result =
            DeliveryProgressionResult::start(meta.job_run_id.clone(), next_cursor.clone());

        for candidate in deliveries {
            match self
                .progress_one(
                    candidate.delivery_id.clone(),
                    job.backend_id.clone(),
                    actor.clone(),
                    meta.clone(),
                )
                .await
            {
                Ok(item) if item.final_status == DeliveryStatus::Delivered => {
                    result.record_dispatched();
                }
                Ok(_) => {
                    result.record_failed();
                }
                Err(error)
                    if matches!(
                        error.category(),
                        crate::errors::ProtocolErrorCategory::Conflict
                            | crate::errors::ProtocolErrorCategory::NotFound
                    ) =>
                {
                    result.record_skipped();
                }
                Err(_) => {
                    result.record_failed();
                }
            }
        }

        result.set_next_cursor(next_cursor);
        Ok(result)
    }
}

fn audit_ref_for(delivery_id: &DeliveryId, label: &str, occurred_at: &Timestamp) -> AuditRef {
    AuditRef::new(format!(
        "audit_{}_{}_{}",
        sanitize(delivery_id.as_str()),
        label,
        sanitize(occurred_at.as_str())
    ))
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};
    use std::future::Future;
    use std::pin::pin;
    use std::sync::{Arc, Mutex};
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    use bus_contracts::fixtures::{
        BackendFixtureBuilder, DeliveryFixtureBuilder, PublicationFixtureBuilder, TestRun,
        TestRunBuilder,
    };
    use bus_contracts::metadata::{
        BackendCapabilityRef, DeliveryStatus, IdempotencyKey, SubscriberRef, SubscriberScope,
    };
    use bus_domain::audit::{AuditAction, BusAuditEntry};
    use bus_domain::backend::BackendCapabilityPolicy;
    use bus_domain::delivery::DeliveryRecord;
    use bus_domain::publication::{PublicationMaterial, TransportSemantic};

    use super::*;

    #[derive(Clone, Default)]
    struct TestState {
        inner: Arc<Mutex<TestStateInner>>,
    }

    #[derive(Default)]
    struct TestStateInner {
        next_transaction_id: u64,
        deliveries: BTreeMap<DeliveryId, DeliveryRecord>,
        staged_deliveries: HashMap<u64, BTreeMap<DeliveryId, DeliveryRecord>>,
        audits: Vec<BusAuditEntry>,
        staged_audits: HashMap<u64, Vec<BusAuditEntry>>,
    }

    #[derive(Clone)]
    struct TestDeliveryRepository {
        state: TestState,
    }

    impl TestDeliveryRepository {
        fn new(state: TestState) -> Self {
            Self { state }
        }

        fn seed_committed(&self, delivery: DeliveryRecord) {
            let mut inner = self.state.inner.lock().expect("test state lock poisoned");
            let mut committed = delivery;
            committed.set_version(1);
            inner
                .deliveries
                .insert(committed.delivery_id.clone(), committed);
        }

        fn committed(&self, delivery_id: &DeliveryId) -> Option<DeliveryRecord> {
            self.state
                .inner
                .lock()
                .expect("test state lock poisoned")
                .deliveries
                .get(delivery_id)
                .cloned()
        }
    }

    impl crate::ports::DeliveryRepository for TestDeliveryRepository {
        async fn get(
            &self,
            delivery_id: &DeliveryId,
        ) -> Result<Option<DeliveryRecord>, crate::errors::RepositoryError> {
            Ok(self.committed(delivery_id))
        }

        async fn get_for_update(
            &self,
            delivery_id: &DeliveryId,
            _uow: &crate::ports::UnitOfWorkHandle,
        ) -> Result<Option<DeliveryRecord>, crate::errors::RepositoryError> {
            Ok(self.committed(delivery_id))
        }

        async fn save(
            &self,
            delivery: DeliveryRecord,
            expected_version: bus_contracts::metadata::Version,
            uow: &crate::ports::UnitOfWorkHandle,
        ) -> Result<bus_contracts::metadata::Version, crate::errors::RepositoryError> {
            let mut inner = self.state.inner.lock().expect("test state lock poisoned");
            let committed = inner
                .deliveries
                .get(&delivery.delivery_id)
                .ok_or(crate::errors::RepositoryError::VersionConflict)?;
            if committed.version() != expected_version {
                return Err(crate::errors::RepositoryError::VersionConflict);
            }
            let mut staged_delivery = delivery;
            staged_delivery.set_version(expected_version + 1);
            let new_version = staged_delivery.version();
            inner
                .staged_deliveries
                .entry(uow.transaction_id)
                .or_default()
                .insert(staged_delivery.delivery_id.clone(), staged_delivery);

            Ok(new_version)
        }

        async fn find_schedulable(
            &self,
            _cursor: DeliveryScanCursor,
            limit: u32,
        ) -> Result<Vec<DeliveryRecord>, crate::errors::RepositoryError> {
            Ok(self
                .state
                .inner
                .lock()
                .expect("test state lock poisoned")
                .deliveries
                .values()
                .filter(|delivery| delivery.status == DeliveryStatus::Scheduled)
                .take(limit as usize)
                .cloned()
                .collect())
        }

        async fn load_history(
            &self,
            delivery_id: &DeliveryId,
        ) -> Result<Vec<bus_domain::delivery::DeliveryHistoryEntry>, crate::errors::RepositoryError>
        {
            Ok(self
                .committed(delivery_id)
                .map(|delivery| delivery.history().to_vec())
                .unwrap_or_default())
        }
    }

    #[derive(Clone)]
    struct TestAuditRepository {
        state: TestState,
    }

    impl TestAuditRepository {
        fn new(state: TestState) -> Self {
            Self { state }
        }

        fn committed_entries(&self) -> Vec<BusAuditEntry> {
            self.state
                .inner
                .lock()
                .expect("test state lock poisoned")
                .audits
                .clone()
        }
    }

    impl crate::ports::AuditTrailRepository for TestAuditRepository {
        async fn append(
            &self,
            entry: BusAuditEntry,
            uow: &crate::ports::UnitOfWorkHandle,
        ) -> Result<u64, crate::errors::RepositoryError> {
            let mut inner = self.state.inner.lock().expect("test state lock poisoned");
            let next_sequence = inner.audits.len() as u64 + 1;
            inner
                .staged_audits
                .entry(uow.transaction_id)
                .or_default()
                .push(entry);
            Ok(next_sequence)
        }

        async fn append_access(
            &self,
            entry: BusAuditEntry,
        ) -> Result<u64, crate::errors::RepositoryError> {
            let mut inner = self.state.inner.lock().expect("test state lock poisoned");
            inner.audits.push(entry);
            Ok(inner.audits.len() as u64)
        }

        async fn list(
            &self,
            _filter: bus_contracts::queries::AuditFilter,
            _page: bus_contracts::metadata::PageRequest,
        ) -> Result<bus_contracts::views::BusAuditTrailView, crate::errors::RepositoryError>
        {
            Ok(bus_contracts::views::BusAuditTrailView {
                items: Vec::new(),
                next_cursor: None,
            })
        }

        async fn load_chain(
            &self,
            chain_ref: &bus_contracts::metadata::AuditChainRef,
        ) -> Result<bus_domain::audit::AuditChain, crate::errors::RepositoryError> {
            Ok(bus_domain::audit::AuditChain {
                chain_ref: chain_ref.clone(),
                entries: Vec::new(),
            })
        }
    }

    #[derive(Clone)]
    struct TestUnitOfWork {
        state: TestState,
        fail_next_commit: Arc<Mutex<Option<crate::errors::UnitOfWorkError>>>,
    }

    impl TestUnitOfWork {
        fn new(state: TestState) -> Self {
            Self {
                state,
                fail_next_commit: Arc::new(Mutex::new(None)),
            }
        }

        fn fail_next_commit(&self, error: crate::errors::UnitOfWorkError) {
            *self
                .fail_next_commit
                .lock()
                .expect("commit failpoint lock poisoned") = Some(error);
        }
    }

    impl crate::ports::UnitOfWork for TestUnitOfWork {
        async fn begin(
            &self,
            purpose: crate::ports::UnitOfWorkPurpose,
            _actor: ActorContext,
        ) -> Result<crate::ports::UnitOfWorkHandle, crate::errors::UnitOfWorkError> {
            let mut inner = self.state.inner.lock().expect("test state lock poisoned");
            inner.next_transaction_id += 1;
            let transaction_id = inner.next_transaction_id;
            inner
                .staged_deliveries
                .insert(transaction_id, BTreeMap::new());
            inner.staged_audits.insert(transaction_id, Vec::new());

            Ok(crate::ports::UnitOfWorkHandle {
                transaction_id,
                purpose,
            })
        }

        async fn commit(
            &self,
            handle: crate::ports::UnitOfWorkHandle,
        ) -> Result<crate::ports::CommitReceipt, crate::errors::UnitOfWorkError> {
            if let Some(error) = self
                .fail_next_commit
                .lock()
                .expect("commit failpoint lock poisoned")
                .take()
            {
                return Err(error);
            }

            let mut inner = self.state.inner.lock().expect("test state lock poisoned");
            if let Some(staged) = inner.staged_deliveries.remove(&handle.transaction_id) {
                for (delivery_id, delivery) in staged {
                    inner.deliveries.insert(delivery_id, delivery);
                }
            }
            if let Some(staged) = inner.staged_audits.remove(&handle.transaction_id) {
                inner.audits.extend(staged);
            }

            Ok(crate::ports::CommitReceipt {
                transaction_id: handle.transaction_id,
            })
        }

        async fn rollback(
            &self,
            handle: crate::ports::UnitOfWorkHandle,
            _reason: crate::ports::RollbackReason,
        ) -> Result<(), crate::errors::UnitOfWorkError> {
            let mut inner = self.state.inner.lock().expect("test state lock poisoned");
            inner.staged_deliveries.remove(&handle.transaction_id);
            inner.staged_audits.remove(&handle.transaction_id);
            Ok(())
        }
    }

    #[derive(Clone)]
    struct TestClock {
        now: Timestamp,
    }

    impl TestClock {
        fn new(now: Timestamp) -> Self {
            Self { now }
        }
    }

    impl crate::ports::ClockPort for TestClock {
        fn now(&self) -> Timestamp {
            self.now.clone()
        }
    }

    #[derive(Clone)]
    struct TestBackend {
        capability_ref: BackendCapabilityRef,
        available: Arc<Mutex<bool>>,
        failures: Arc<Mutex<HashMap<DeliveryId, crate::errors::TransportPortError>>>,
    }

    impl TestBackend {
        fn new(capability_ref: BackendCapabilityRef) -> Self {
            Self {
                capability_ref,
                available: Arc::new(Mutex::new(true)),
                failures: Arc::new(Mutex::new(HashMap::new())),
            }
        }

        fn set_available(&self, available: bool) {
            *self
                .available
                .lock()
                .expect("backend availability lock poisoned") = available;
        }
    }

    impl crate::ports::TransportBackendPort for TestBackend {
        async fn dispatch(
            &self,
            semantic: TransportSemantic,
            attempt: bus_domain::delivery::DeliveryAttempt,
            _context: crate::ports::BackendDispatchContext,
        ) -> Result<bus_contracts::metadata::BackendDeliveryResult, crate::errors::TransportPortError>
        {
            if let Some(error) = self
                .failures
                .lock()
                .expect("backend failure map lock poisoned")
                .remove(&attempt.delivery_id)
            {
                return Err(error);
            }
            if !*self
                .available
                .lock()
                .expect("backend availability lock poisoned")
            {
                return Err(crate::errors::TransportPortError::BackendUnavailable);
            }

            let policy = BackendCapabilityPolicy::from_capability(self.capability_ref.clone());
            if policy.rejects_raw_backend_leak(semantic.clone()) {
                return Err(crate::errors::TransportPortError::PrivateBodyViolation);
            }
            if !policy.allows_mapping(semantic, self.capability_ref.clone()) {
                return Err(crate::errors::TransportPortError::CapabilityMismatch);
            }

            Ok(bus_contracts::metadata::BackendDeliveryResult::delivered(
                Some(bus_contracts::metadata::BackendDeliveryRef::new(format!(
                    "backend_delivery_{}",
                    attempt.attempt_id.as_str()
                ))),
            ))
        }

        async fn normalize_signal(
            &self,
            signal: bus_contracts::events::BackendDeliverySignalInput,
        ) -> Result<bus_contracts::metadata::BackendDeliveryResult, crate::errors::TransportPortError>
        {
            if signal.backend_capability_ref != self.capability_ref {
                return Err(crate::errors::TransportPortError::CapabilityMismatch);
            }

            let backend_ref = Some(bus_contracts::metadata::BackendDeliveryRef::new(
                signal.backend_result_ref.as_str(),
            ));
            match signal.backend_status {
                bus_contracts::metadata::BackendStatus::Delivered => Ok(
                    bus_contracts::metadata::BackendDeliveryResult::delivered(backend_ref),
                ),
                bus_contracts::metadata::BackendStatus::Failed => Ok(
                    bus_contracts::metadata::BackendDeliveryResult::failed(backend_ref),
                ),
            }
        }

        async fn check_capability(
            &self,
            capability_ref: BackendCapabilityRef,
        ) -> Result<crate::ports::BackendCapabilityReport, crate::errors::TransportPortError>
        {
            if capability_ref != self.capability_ref {
                return Err(crate::errors::TransportPortError::CapabilityMismatch);
            }

            Ok(crate::ports::BackendCapabilityReport {
                capability_ref,
                available: *self
                    .available
                    .lock()
                    .expect("backend availability lock poisoned"),
            })
        }
    }

    type TestService = DeliveryProgressionService<
        TestDeliveryRepository,
        TestBackend,
        TestAuditRepository,
        TestUnitOfWork,
        TestClock,
    >;

    struct Harness {
        run: TestRun,
        job: RunDeliveryProgressionJob,
        meta: JobMetadata,
        service: TestService,
        delivery_repository: TestDeliveryRepository,
        audit_repository: TestAuditRepository,
        backend: TestBackend,
        unit_of_work: TestUnitOfWork,
    }

    fn noop_raw_waker() -> RawWaker {
        fn clone(_: *const ()) -> RawWaker {
            noop_raw_waker()
        }
        fn wake(_: *const ()) {}
        fn wake_by_ref(_: *const ()) {}
        fn drop(_: *const ()) {}

        RawWaker::new(
            std::ptr::null(),
            &RawWakerVTable::new(clone, wake, wake_by_ref, drop),
        )
    }

    fn block_on<F>(future: F) -> F::Output
    where
        F: Future,
    {
        let waker = unsafe { Waker::from_raw(noop_raw_waker()) };
        let mut context = Context::from_waker(&waker);
        let mut future = pin!(future);

        loop {
            match Future::poll(future.as_mut(), &mut context) {
                Poll::Ready(output) => return output,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    fn build_harness(run_id: &str, adapter_capability: BackendCapabilityRef) -> Harness {
        let builder = TestRunBuilder::new(run_id);
        let run = builder.build();
        let delivery_builder = DeliveryFixtureBuilder::new(run.clone());
        let state = TestState::default();
        let delivery_repository = TestDeliveryRepository::new(state.clone());
        let audit_repository = TestAuditRepository::new(state.clone());
        let unit_of_work = TestUnitOfWork::new(state);
        let backend = TestBackend::new(adapter_capability);
        let service = DeliveryProgressionService::new(DeliveryProgressionServiceDeps {
            delivery_repository: delivery_repository.clone(),
            transport_backend: backend.clone(),
            audit_repository: audit_repository.clone(),
            unit_of_work: unit_of_work.clone(),
            clock: TestClock::new(run.metadata.request.requested_at.clone()),
        });

        Harness {
            run,
            job: delivery_builder.run_delivery_progression_job(),
            meta: delivery_builder.run_delivery_progression_metadata(),
            service,
            delivery_repository,
            audit_repository,
            backend,
            unit_of_work,
        }
    }

    fn scheduled_delivery(
        run: &TestRun,
        capability_ref: BackendCapabilityRef,
        subscriber_ref: &str,
    ) -> DeliveryRecord {
        let publication_builder = PublicationFixtureBuilder::new(run.clone());
        let material = PublicationMaterial::from_accept_publication_command(
            publication_builder.valid_material(),
            run.actor.clone(),
            run.metadata.clone(),
        )
        .expect("fixture should create publication material");
        let semantic = TransportSemantic::derive(
            material,
            capability_ref,
            SubscriberScope {
                project_id: format!("project_{}", run.run_id),
                topic: format!("workitem.events.{}", run.run_id),
            },
        )
        .expect("fixture should derive transport semantic");

        DeliveryRecord::schedule(
            semantic,
            SubscriberRef::new(subscriber_ref),
            IdempotencyKey::new(format!("idem_delivery_{}_{}", run.run_id, subscriber_ref)),
        )
        .expect("fixture should schedule delivery")
    }

    #[test]
    fn progress_batch_delivers_scheduled_delivery_and_records_history() {
        let run = TestRunBuilder::new("delivery-service-001").build();
        let capability = BackendFixtureBuilder::new(run.clone()).in_memory_capability();
        let harness = build_harness("delivery-service-001", capability.clone());
        let delivery = scheduled_delivery(&run, capability, "subscriber_alpha");
        let delivery_id = delivery.delivery_id.clone();

        harness.delivery_repository.seed_committed(delivery);

        let result = block_on(harness.service.progress_batch(
            harness.job.clone(),
            harness.run.actor.clone(),
            harness.meta.clone(),
        ))
        .expect("delivery progression should succeed");

        assert_eq!(result.scanned, 1);
        assert_eq!(result.dispatched, 1);
        assert_eq!(result.skipped, 0);
        assert_eq!(result.failed(), 0);

        let committed = harness
            .delivery_repository
            .committed(&delivery_id)
            .expect("delivery should be committed");
        assert_eq!(committed.status, DeliveryStatus::Delivered);
        assert_eq!(committed.attempts().len(), 1);
        assert_eq!(committed.history().len(), 2);
        assert!(
            committed.history()[0]
                .describes_transition(DeliveryStatus::Scheduled, DeliveryStatus::Dispatching)
        );
        assert!(
            committed.history()[1]
                .describes_transition(DeliveryStatus::Dispatching, DeliveryStatus::Delivered)
        );
        assert!(!format!("{committed:?}").contains("feedback"));

        let audits = harness.audit_repository.committed_entries();
        assert_eq!(audits.len(), 2);
        assert_eq!(audits[0].action, AuditAction::DeliveryDispatchStarted);
        assert_eq!(audits[1].action, AuditAction::DeliveryDelivered);
    }

    #[test]
    fn progress_batch_marks_backend_unavailable_delivery_failed() {
        let run = TestRunBuilder::new("delivery-service-002").build();
        let capability = BackendFixtureBuilder::new(run.clone()).in_memory_capability();
        let harness = build_harness("delivery-service-002", capability.clone());
        let delivery = scheduled_delivery(&run, capability, "subscriber_beta");
        let delivery_id = delivery.delivery_id.clone();

        harness.delivery_repository.seed_committed(delivery);
        harness.backend.set_available(false);

        let result = block_on(harness.service.progress_batch(
            harness.job.clone(),
            harness.run.actor.clone(),
            harness.meta.clone(),
        ))
        .expect("backend unavailable should still commit failed evidence");

        assert_eq!(result.scanned, 1);
        assert_eq!(result.dispatched, 0);
        assert_eq!(result.skipped, 0);
        assert_eq!(result.failed(), 1);

        let committed = harness
            .delivery_repository
            .committed(&delivery_id)
            .expect("failed delivery should remain committed");
        assert_eq!(committed.status, DeliveryStatus::Failed);
        assert_eq!(committed.history().len(), 2);

        let audits = harness.audit_repository.committed_entries();
        assert_eq!(audits.len(), 2);
        assert_eq!(
            audits[1].action,
            AuditAction::DeliveryFailed(
                bus_contracts::metadata::FailureReason::backend_unavailable()
            )
        );
    }

    #[test]
    fn progress_batch_marks_capability_mismatch_failed_without_secret_leak() {
        let run = TestRunBuilder::new("delivery-service-003").build();
        let backend_builder = BackendFixtureBuilder::new(run.clone());
        let delivery_capability = backend_builder.in_memory_capability();
        let mismatched_capability = BackendCapabilityRef::from_profile(
            "profile_other_backend".into(),
            bus_contracts::metadata::BackendKind::InMemory,
            bus_contracts::metadata::CapabilityVersion::new("v2"),
        );
        let harness = build_harness("delivery-service-003", mismatched_capability);
        let delivery = scheduled_delivery(&run, delivery_capability, "subscriber_gamma");
        let delivery_id = delivery.delivery_id.clone();

        harness.delivery_repository.seed_committed(delivery);

        let result = block_on(harness.service.progress_batch(
            harness.job.clone(),
            harness.run.actor.clone(),
            harness.meta.clone(),
        ))
        .expect("capability mismatch should commit failed evidence");

        assert_eq!(result.scanned, 1);
        assert_eq!(result.dispatched, 0);
        assert_eq!(result.failed(), 1);

        let committed = harness
            .delivery_repository
            .committed(&delivery_id)
            .expect("delivery should remain committed");
        assert_eq!(committed.status, DeliveryStatus::Failed);
        assert!(!format!("{committed:?}").contains("secret"));

        let audits = harness.audit_repository.committed_entries();
        assert_eq!(
            audits[1].action,
            AuditAction::DeliveryFailed(bus_contracts::metadata::FailureReason::dispatch_failed())
        );
    }

    #[test]
    fn progress_one_returns_manual_action_on_commit_uncertain() {
        let run = TestRunBuilder::new("delivery-service-004").build();
        let capability = BackendFixtureBuilder::new(run.clone()).in_memory_capability();
        let harness = build_harness("delivery-service-004", capability.clone());
        let delivery = scheduled_delivery(&run, capability, "subscriber_delta");
        let delivery_id = delivery.delivery_id.clone();

        harness.delivery_repository.seed_committed(delivery);
        harness
            .unit_of_work
            .fail_next_commit(crate::errors::UnitOfWorkError::CommitUncertain);

        let error = block_on(harness.service.progress_one(
            delivery_id.clone(),
            harness.job.backend_id.clone(),
            harness.run.actor.clone(),
            harness.meta.clone(),
        ))
        .expect_err("commit uncertainty should surface as an application error");

        assert_eq!(error.code(), "internal.commit_uncertain");
        assert!(error.requires_manual_action());

        let committed = harness
            .delivery_repository
            .committed(&delivery_id)
            .expect("seeded delivery should still exist");
        assert_eq!(committed.status, DeliveryStatus::Scheduled);
        assert!(harness.audit_repository.committed_entries().is_empty());
    }
}
