//! In-memory repository adapters.

use std::sync::{Arc, Mutex};

use bus_application::{
    AuditTrailRepository, IdempotencyRepository, PublicationRepository, RepositoryError,
    UnitOfWorkHandle,
};
use bus_contracts::metadata::{IdempotencyKey, PublicationId, Version};
use bus_domain::audit::BusAuditEntry;
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
