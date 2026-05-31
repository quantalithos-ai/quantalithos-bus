//! In-memory repository adapters.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bus_application::{
    AuditTrailRepository, BackendCapabilityReport, BackendDispatchContext, DeliveryRepository,
    FeedbackRepository, IdempotencyRepository, PublicationRepository, ReadProjectionRepository,
    RecoveryRepository, RepositoryError, TransportBackendPort, TransportPortError,
    UnitOfWorkHandle,
};
use bus_contracts::events::BackendDeliverySignalInput;
use bus_contracts::metadata::{
    AuditChainRef, BackendCapabilityRef, BackendDeliveryRef, BackendId, BackendStatus,
    DeadLetterId, DeliveryId, DeliveryScanCursor, FailureMaterialId, FailureSummaryId, FeedbackId,
    IdempotencyKey, PageLimit, PageRequest, PublicationId, ReplayPreparationId, RetryPlanId,
    RetryScanCursor, Timestamp, TransportViewId, Version,
};
use bus_contracts::queries::AuditFilter;
use bus_contracts::views::{BackendHealthView, BusAuditTrailItemView, BusAuditTrailView};
use bus_domain::audit::{AuditChain, BusAuditEntry};
use bus_domain::backend::BackendCapabilityPolicy;
use bus_domain::delivery::{DeliveryHistoryEntry, DeliveryRecord};
use bus_domain::feedback::FeedbackResult;
use bus_domain::idempotency::{IdempotencyAnchor, IdempotencyConflict, IdempotencyScope};
use bus_domain::publication::{PublicationAcceptance, PublicationMaterial};
use bus_domain::read_output::{FailureSummaryProjection, TransportViewProjection};
use bus_domain::recovery::{DeadLetterEntry, FailureMaterial, ReplayPreparation, RetryPlan};

use crate::store::SharedMemoryStore;

/// In-memory publication repository.
#[derive(Clone)]
pub struct InMemoryPublicationRepository {
    store: SharedMemoryStore,
}

impl InMemoryPublicationRepository {
    /// Creates a new repository over the shared memory store.
    pub fn new(store: SharedMemoryStore) -> Self {
        Self { store }
    }

    /// Returns a committed publication acceptance for tests.
    pub fn committed(&self, publication_id: &PublicationId) -> Option<PublicationAcceptance> {
        self.store.publication(publication_id)
    }
}

impl PublicationRepository for InMemoryPublicationRepository {
    async fn store_material(
        &self,
        material: PublicationMaterial,
        uow: &UnitOfWorkHandle,
    ) -> Result<(), RepositoryError> {
        self.store
            .stage_publication_material(uow.transaction_id, material)
    }

    async fn insert(
        &self,
        acceptance: PublicationAcceptance,
        uow: &UnitOfWorkHandle,
    ) -> Result<Version, RepositoryError> {
        self.store.stage_publication(uow.transaction_id, acceptance)
    }

    async fn get(
        &self,
        publication_id: &PublicationId,
    ) -> Result<Option<PublicationAcceptance>, RepositoryError> {
        Ok(self.store.publication(publication_id))
    }

    async fn get_material(
        &self,
        publication_id: &PublicationId,
    ) -> Result<Option<PublicationMaterial>, RepositoryError> {
        Ok(self.store.publication_material(publication_id))
    }
}

/// In-memory idempotency repository.
#[derive(Clone)]
pub struct InMemoryIdempotencyRepository {
    store: SharedMemoryStore,
}

impl InMemoryIdempotencyRepository {
    /// Creates a new repository over the shared memory store.
    pub fn new(store: SharedMemoryStore) -> Self {
        Self { store }
    }

    /// Returns a committed anchor for tests.
    pub fn committed_anchor(
        &self,
        scope: &IdempotencyScope,
        key: &IdempotencyKey,
    ) -> Option<IdempotencyAnchor> {
        self.store.idempotency_anchor(scope, key)
    }

    /// Returns committed conflict records for tests.
    pub fn committed_conflicts(&self) -> Vec<IdempotencyConflict> {
        self.store.idempotency_conflicts()
    }
}

impl IdempotencyRepository for InMemoryIdempotencyRepository {
    async fn find(
        &self,
        scope: &IdempotencyScope,
        key: &IdempotencyKey,
    ) -> Result<Option<IdempotencyAnchor>, RepositoryError> {
        Ok(self.store.idempotency_anchor(scope, key))
    }

    async fn bind(
        &self,
        anchor: IdempotencyAnchor,
        uow: &UnitOfWorkHandle,
    ) -> Result<(), RepositoryError> {
        self.store
            .stage_idempotency_anchor(uow.transaction_id, anchor)
    }

    async fn mark_conflict(
        &self,
        scope: IdempotencyScope,
        key: IdempotencyKey,
        conflict: IdempotencyConflict,
        uow: &UnitOfWorkHandle,
    ) -> Result<(), RepositoryError> {
        self.store
            .stage_idempotency_conflict(uow.transaction_id, scope, key, conflict)
    }
}

/// In-memory audit repository.
#[derive(Clone)]
pub struct InMemoryAuditTrailRepository {
    store: SharedMemoryStore,
    fail_next_append: Arc<Mutex<Option<RepositoryError>>>,
}

impl InMemoryAuditTrailRepository {
    /// Creates a new repository over the shared memory store.
    pub fn new(store: SharedMemoryStore) -> Self {
        Self {
            store,
            fail_next_append: Arc::new(Mutex::new(None)),
        }
    }

    /// Fails the next append call with the provided error.
    pub fn fail_next_append(&self, error: RepositoryError) {
        *self
            .fail_next_append
            .lock()
            .expect("audit failpoint lock poisoned") = Some(error);
    }

    /// Returns committed audit entries for tests.
    pub fn committed_entries(&self) -> Vec<BusAuditEntry> {
        self.store.audit_entries()
    }
}

impl AuditTrailRepository for InMemoryAuditTrailRepository {
    async fn append(
        &self,
        entry: BusAuditEntry,
        uow: &UnitOfWorkHandle,
    ) -> Result<u64, RepositoryError> {
        if let Some(error) = self
            .fail_next_append
            .lock()
            .expect("audit failpoint lock poisoned")
            .take()
        {
            return Err(error);
        }

        self.store.stage_audit_entry(uow.transaction_id, entry)
    }

    async fn list(
        &self,
        filter: AuditFilter,
        page: PageRequest,
    ) -> Result<BusAuditTrailView, RepositoryError> {
        let offset = page
            .page_token
            .as_ref()
            .and_then(|token| token.as_str().parse::<usize>().ok())
            .unwrap_or(0);
        let limit = usize::try_from(page.limit).unwrap_or(usize::MAX);
        let entries = self.store.audit_entries();
        let filtered = entries
            .into_iter()
            .enumerate()
            .filter(|(_, entry)| {
                filter
                    .record_ref
                    .as_ref()
                    .is_none_or(|record_ref| entry.subject_ref.record_ref() == *record_ref)
                    && filter
                        .event_kind
                        .as_ref()
                        .is_none_or(|event_kind| entry.action.event_kind() == *event_kind)
            })
            .collect::<Vec<_>>();
        let items = filtered
            .iter()
            .skip(offset)
            .take(limit)
            .map(|(index, entry)| BusAuditTrailItemView {
                audit_ref: entry.audit_ref.clone(),
                audit_sequence: (*index as u64) + 1,
                record_ref: entry.subject_ref.record_ref(),
                event_kind: entry.action.event_kind(),
                actor_ref: entry.actor.actor_ref().clone(),
                occurred_at: entry.occurred_at.clone(),
            })
            .collect::<Vec<_>>();
        let next_cursor = (offset + items.len() < filtered.len())
            .then(|| bus_contracts::metadata::PageToken::new((offset + items.len()).to_string()));

        Ok(BusAuditTrailView { items, next_cursor })
    }

    async fn load_chain(&self, chain_ref: &AuditChainRef) -> Result<AuditChain, RepositoryError> {
        Ok(self.store.audit_chain(chain_ref))
    }
}

/// In-memory feedback repository.
#[derive(Clone)]
pub struct InMemoryFeedbackRepository {
    store: SharedMemoryStore,
}

impl InMemoryFeedbackRepository {
    /// Creates a new repository over the shared memory store.
    pub fn new(store: SharedMemoryStore) -> Self {
        Self { store }
    }

    /// Returns one committed feedback result for tests.
    pub fn committed(&self, feedback_id: &FeedbackId) -> Option<FeedbackResult> {
        self.store.feedback(feedback_id)
    }

    /// Returns all committed feedback results for tests.
    pub fn committed_all(&self) -> Vec<FeedbackResult> {
        self.store.feedbacks()
    }
}

impl FeedbackRepository for InMemoryFeedbackRepository {
    async fn insert(
        &self,
        feedback: FeedbackResult,
        uow: &UnitOfWorkHandle,
    ) -> Result<Version, RepositoryError> {
        self.store.stage_feedback(uow.transaction_id, feedback)
    }

    async fn get(
        &self,
        feedback_id: &FeedbackId,
    ) -> Result<Option<FeedbackResult>, RepositoryError> {
        Ok(self.store.feedback(feedback_id))
    }

    async fn find_by_delivery(
        &self,
        delivery_id: &DeliveryId,
        page: PageRequest,
    ) -> Result<Vec<FeedbackResult>, RepositoryError> {
        let offset = page
            .page_token
            .as_ref()
            .and_then(|token| token.as_str().parse::<usize>().ok())
            .unwrap_or(0);
        let limit = usize::try_from(page.limit).unwrap_or(usize::MAX);

        Ok(self
            .store
            .feedbacks_by_delivery(delivery_id)
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect())
    }
}

/// In-memory delivery repository.
#[derive(Clone)]
pub struct InMemoryDeliveryRepository {
    store: SharedMemoryStore,
}

impl InMemoryDeliveryRepository {
    /// Creates a new repository over the shared memory store.
    pub fn new(store: SharedMemoryStore) -> Self {
        Self { store }
    }

    /// Seeds a committed delivery aggregate for tests.
    pub fn seed_committed(&self, delivery: DeliveryRecord) -> Result<Version, RepositoryError> {
        self.store.seed_delivery(delivery)
    }

    /// Returns a committed delivery aggregate for tests.
    pub fn committed(&self, delivery_id: &DeliveryId) -> Option<DeliveryRecord> {
        self.store.delivery(delivery_id)
    }
}

impl DeliveryRepository for InMemoryDeliveryRepository {
    async fn get(
        &self,
        delivery_id: &DeliveryId,
    ) -> Result<Option<DeliveryRecord>, RepositoryError> {
        Ok(self.store.delivery(delivery_id))
    }

    async fn get_for_update(
        &self,
        delivery_id: &DeliveryId,
        _uow: &UnitOfWorkHandle,
    ) -> Result<Option<DeliveryRecord>, RepositoryError> {
        Ok(self.store.delivery(delivery_id))
    }

    async fn save(
        &self,
        delivery: DeliveryRecord,
        expected_version: Version,
        uow: &UnitOfWorkHandle,
    ) -> Result<Version, RepositoryError> {
        self.store
            .stage_delivery_save(uow.transaction_id, delivery, expected_version)
    }

    async fn find_schedulable(
        &self,
        cursor: DeliveryScanCursor,
        limit: u32,
    ) -> Result<Vec<DeliveryRecord>, RepositoryError> {
        Ok(self.store.schedulable_deliveries(cursor.as_str(), limit))
    }

    async fn load_history(
        &self,
        delivery_id: &DeliveryId,
    ) -> Result<Vec<DeliveryHistoryEntry>, RepositoryError> {
        Ok(self
            .store
            .delivery(delivery_id)
            .map(|delivery| delivery.history().to_vec())
            .unwrap_or_default())
    }
}

/// In-memory read-projection repository.
#[derive(Clone)]
pub struct InMemoryReadProjectionRepository {
    store: SharedMemoryStore,
}

impl InMemoryReadProjectionRepository {
    /// Creates a new repository over the shared memory store.
    pub fn new(store: SharedMemoryStore) -> Self {
        Self { store }
    }

    /// Seeds a committed transport-view projection for tests.
    pub fn seed_transport_view_projection(
        &self,
        projection: TransportViewProjection,
    ) -> Result<bus_contracts::metadata::ProjectionVersion, RepositoryError> {
        self.store.seed_transport_view_projection(projection)
    }

    /// Seeds a committed failure-summary projection for tests.
    pub fn seed_failure_summary_projection(
        &self,
        projection: FailureSummaryProjection,
    ) -> Result<bus_contracts::metadata::ProjectionVersion, RepositoryError> {
        self.store.seed_failure_summary_projection(projection)
    }

    /// Seeds a committed backend-health view for tests.
    pub fn seed_backend_health_view(
        &self,
        view: BackendHealthView,
    ) -> Result<bus_contracts::metadata::ProjectionVersion, RepositoryError> {
        self.store.seed_backend_health_view(view)
    }
}

impl ReadProjectionRepository for InMemoryReadProjectionRepository {
    async fn upsert_transport_view(
        &self,
        projection: TransportViewProjection,
        uow: &UnitOfWorkHandle,
    ) -> Result<bus_contracts::metadata::ProjectionVersion, RepositoryError> {
        self.store
            .stage_transport_view_projection(uow.transaction_id, projection)
    }

    async fn upsert_failure_summary(
        &self,
        projection: FailureSummaryProjection,
        uow: &UnitOfWorkHandle,
    ) -> Result<bus_contracts::metadata::ProjectionVersion, RepositoryError> {
        self.store
            .stage_failure_summary_projection(uow.transaction_id, projection)
    }

    async fn upsert_backend_health(
        &self,
        view: BackendHealthView,
        uow: &UnitOfWorkHandle,
    ) -> Result<bus_contracts::metadata::ProjectionVersion, RepositoryError> {
        self.store
            .stage_backend_health_view(uow.transaction_id, view)
    }

    async fn get_transport_view_projection(
        &self,
        transport_view_id: &TransportViewId,
    ) -> Result<Option<TransportViewProjection>, RepositoryError> {
        Ok(self.store.transport_view_projection(transport_view_id))
    }

    async fn get_failure_summary_projection(
        &self,
        failure_summary_id: &FailureSummaryId,
    ) -> Result<Option<FailureSummaryProjection>, RepositoryError> {
        Ok(self.store.failure_summary_projection(failure_summary_id))
    }

    async fn get_backend_health_view(
        &self,
        backend_id: &BackendId,
    ) -> Result<Option<BackendHealthView>, RepositoryError> {
        Ok(self.store.backend_health_view(backend_id))
    }
}

/// In-memory recovery repository.
#[derive(Clone)]
pub struct InMemoryRecoveryRepository {
    store: SharedMemoryStore,
}

impl InMemoryRecoveryRepository {
    /// Creates a new repository over the shared memory store.
    pub fn new(store: SharedMemoryStore) -> Self {
        Self { store }
    }

    /// Seeds committed failure material for tests.
    pub fn seed_failure_material(
        &self,
        material: FailureMaterial,
    ) -> Result<Version, RepositoryError> {
        self.store.seed_failure_material(material)
    }

    /// Seeds a committed retry plan for tests.
    pub fn seed_retry_plan(&self, retry_plan: RetryPlan) -> Result<Version, RepositoryError> {
        self.store.seed_retry_plan(retry_plan)
    }

    /// Seeds a committed dead-letter entry for tests.
    pub fn seed_dead_letter(&self, entry: DeadLetterEntry) -> Result<Version, RepositoryError> {
        self.store.seed_dead_letter(entry)
    }

    /// Returns one committed retry plan for tests.
    pub fn committed_retry_plan(&self, retry_plan_id: &RetryPlanId) -> Option<RetryPlan> {
        self.store.retry_plan(retry_plan_id)
    }

    /// Returns one committed dead-letter entry for tests.
    pub fn committed_dead_letter(&self, dead_letter_id: &DeadLetterId) -> Option<DeadLetterEntry> {
        self.store.dead_letter(dead_letter_id)
    }

    /// Returns one committed replay preparation for tests.
    pub fn committed_replay_preparation(
        &self,
        replay_preparation_id: &ReplayPreparationId,
    ) -> Option<ReplayPreparation> {
        self.store.replay_preparation(replay_preparation_id)
    }

    /// Returns one committed failure material for tests.
    pub fn committed_failure_material(
        &self,
        failure_material_id: &FailureMaterialId,
    ) -> Option<FailureMaterial> {
        self.store.failure_material(failure_material_id)
    }
}

impl RecoveryRepository for InMemoryRecoveryRepository {
    async fn save_retry_plan(
        &self,
        retry_plan: RetryPlan,
        expected_version: Option<Version>,
        uow: &UnitOfWorkHandle,
    ) -> Result<Version, RepositoryError> {
        self.store
            .stage_retry_plan_save(uow.transaction_id, retry_plan, expected_version)
    }

    async fn find_due_retry(
        &self,
        cursor: RetryScanCursor,
        limit: PageLimit,
        now: Timestamp,
    ) -> Result<Vec<RetryPlan>, RepositoryError> {
        Ok(self.store.due_retry_plans(&cursor, limit, &now))
    }

    async fn save_dead_letter(
        &self,
        entry: DeadLetterEntry,
        material: FailureMaterial,
        uow: &UnitOfWorkHandle,
    ) -> Result<Version, RepositoryError> {
        self.store
            .stage_dead_letter_save(uow.transaction_id, entry, material)
    }

    async fn get_dead_letter(
        &self,
        dead_letter_id: &DeadLetterId,
    ) -> Result<Option<DeadLetterEntry>, RepositoryError> {
        Ok(self.store.dead_letter(dead_letter_id))
    }

    async fn save_replay_preparation(
        &self,
        preparation: ReplayPreparation,
        uow: &UnitOfWorkHandle,
    ) -> Result<Version, RepositoryError> {
        self.store
            .stage_replay_preparation_save(uow.transaction_id, preparation)
    }

    async fn get_failure_material(
        &self,
        failure_material_id: &FailureMaterialId,
    ) -> Result<Option<FailureMaterial>, RepositoryError> {
        Ok(self.store.failure_material(failure_material_id))
    }
}

/// In-memory fake backend adapter.
#[derive(Clone)]
pub struct InMemoryTransportBackendAdapter {
    capability_ref: BackendCapabilityRef,
    available: Arc<Mutex<bool>>,
    failures: Arc<Mutex<HashMap<DeliveryId, TransportPortError>>>,
}

impl InMemoryTransportBackendAdapter {
    /// Creates a new fake backend adapter for the provided capability.
    pub fn new(capability_ref: BackendCapabilityRef) -> Self {
        Self {
            capability_ref,
            available: Arc::new(Mutex::new(true)),
            failures: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Marks the adapter as available or unavailable.
    pub fn set_available(&self, available: bool) {
        *self
            .available
            .lock()
            .expect("backend availability lock poisoned") = available;
    }

    /// Injects a failure for the next dispatch of the provided delivery.
    pub fn fail_delivery(&self, delivery_id: DeliveryId, error: TransportPortError) {
        self.failures
            .lock()
            .expect("backend failure map lock poisoned")
            .insert(delivery_id, error);
    }
}

impl TransportBackendPort for InMemoryTransportBackendAdapter {
    async fn dispatch(
        &self,
        semantic: bus_domain::publication::TransportSemantic,
        attempt: bus_domain::delivery::DeliveryAttempt,
        _context: BackendDispatchContext,
    ) -> Result<bus_contracts::metadata::BackendDeliveryResult, TransportPortError> {
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
            return Err(TransportPortError::BackendUnavailable);
        }

        let policy = BackendCapabilityPolicy::from_capability(self.capability_ref.clone());
        if policy.rejects_raw_backend_leak(semantic.clone()) {
            return Err(TransportPortError::PrivateBodyViolation);
        }
        if !policy.allows_mapping(semantic, self.capability_ref.clone()) {
            return Err(TransportPortError::CapabilityMismatch);
        }

        Ok(bus_contracts::metadata::BackendDeliveryResult::delivered(
            Some(BackendDeliveryRef::new(format!(
                "backend_delivery_{}",
                attempt.attempt_id.as_str()
            ))),
        ))
    }

    async fn normalize_signal(
        &self,
        signal: BackendDeliverySignalInput,
    ) -> Result<bus_contracts::metadata::BackendDeliveryResult, TransportPortError> {
        if signal.backend_capability_ref != self.capability_ref {
            return Err(TransportPortError::CapabilityMismatch);
        }
        if result_ref_looks_private(signal.backend_result_ref.as_str()) {
            return Err(TransportPortError::PrivateBodyViolation);
        }

        let backend_ref = Some(BackendDeliveryRef::new(signal.backend_result_ref.as_str()));
        match signal.backend_status {
            BackendStatus::Delivered => Ok(
                bus_contracts::metadata::BackendDeliveryResult::delivered(backend_ref),
            ),
            BackendStatus::Failed => Ok(bus_contracts::metadata::BackendDeliveryResult::failed(
                backend_ref,
            )),
        }
    }

    async fn check_capability(
        &self,
        capability_ref: BackendCapabilityRef,
    ) -> Result<BackendCapabilityReport, TransportPortError> {
        if capability_ref != self.capability_ref {
            return Err(TransportPortError::CapabilityMismatch);
        }

        Ok(BackendCapabilityReport {
            capability_ref,
            available: *self
                .available
                .lock()
                .expect("backend availability lock poisoned"),
        })
    }
}

fn result_ref_looks_private(value: &str) -> bool {
    value.contains('{') || value.contains('}') || value.contains('\n')
}
