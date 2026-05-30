//! In-memory repository adapters.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bus_application::{
    AuditTrailRepository, BackendCapabilityReport, BackendDispatchContext, DeliveryRepository,
    FeedbackRepository, IdempotencyRepository, PublicationRepository, RepositoryError,
    TransportBackendPort, TransportPortError, UnitOfWorkHandle,
};
use bus_contracts::metadata::{
    BackendCapabilityRef, BackendDeliveryRef, DeliveryId, DeliveryScanCursor, FeedbackId,
    IdempotencyKey, PublicationId, Version,
};
use bus_domain::audit::BusAuditEntry;
use bus_domain::backend::BackendCapabilityPolicy;
use bus_domain::delivery::{DeliveryHistoryEntry, DeliveryRecord};
use bus_domain::feedback::FeedbackResult;
use bus_domain::idempotency::{IdempotencyAnchor, IdempotencyConflict, IdempotencyScope};
use bus_domain::publication::PublicationAcceptance;

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
