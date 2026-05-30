//! Shared in-memory state with staged transaction support.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use bus_application::{
    CommitReceipt, RepositoryError, UnitOfWorkError, UnitOfWorkHandle, UnitOfWorkPurpose,
};
use bus_contracts::metadata::{DeliveryId, IdempotencyKey, PublicationId, Version};
use bus_domain::audit::BusAuditEntry;
use bus_domain::delivery::DeliveryRecord;
use bus_domain::idempotency::{IdempotencyAnchor, IdempotencyConflict, IdempotencyScope};
use bus_domain::publication::PublicationAcceptance;

#[derive(Clone, Default)]
pub struct SharedMemoryStore {
    inner: Arc<Mutex<MemoryStoreInner>>,
}

#[derive(Default)]
struct MemoryStoreInner {
    next_transaction_id: u64,
    next_audit_sequence: u64,
    deliveries: BTreeMap<DeliveryId, DeliveryRecord>,
    delivery_versions: BTreeMap<DeliveryId, Version>,
    publications: BTreeMap<PublicationId, PublicationAcceptance>,
    publication_versions: BTreeMap<PublicationId, Version>,
    anchors: HashMap<(IdempotencyScope, IdempotencyKey), IdempotencyAnchor>,
    conflicts: Vec<IdempotencyConflict>,
    audits: Vec<BusAuditEntry>,
    transactions: HashMap<u64, StagedTransaction>,
}

#[derive(Default)]
struct StagedTransaction {
    _purpose: Option<UnitOfWorkPurpose>,
    deliveries: BTreeMap<DeliveryId, DeliveryRecord>,
    publications: BTreeMap<PublicationId, PublicationAcceptance>,
    anchors: HashMap<(IdempotencyScope, IdempotencyKey), IdempotencyAnchor>,
    conflicts: Vec<IdempotencyConflict>,
    audits: Vec<BusAuditEntry>,
}

impl SharedMemoryStore {
    /// Creates a fresh shared memory store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Begins a staged write transaction.
    pub fn begin_transaction(
        &self,
        purpose: UnitOfWorkPurpose,
    ) -> Result<UnitOfWorkHandle, UnitOfWorkError> {
        let mut inner = self.inner.lock().expect("memory store lock poisoned");
        inner.next_transaction_id += 1;
        let transaction_id = inner.next_transaction_id;
        inner.transactions.insert(
            transaction_id,
            StagedTransaction {
                _purpose: Some(purpose),
                ..StagedTransaction::default()
            },
        );

        Ok(UnitOfWorkHandle {
            transaction_id,
            purpose,
        })
    }

    /// Commits a staged transaction.
    pub fn commit_transaction(
        &self,
        handle: UnitOfWorkHandle,
    ) -> Result<CommitReceipt, UnitOfWorkError> {
        let mut inner = self.inner.lock().expect("memory store lock poisoned");
        let staged = inner
            .transactions
            .remove(&handle.transaction_id)
            .ok_or(UnitOfWorkError::InvalidHandle)?;

        for (delivery_id, delivery) in staged.deliveries {
            inner
                .deliveries
                .insert(delivery_id.clone(), delivery.clone());
            inner
                .delivery_versions
                .insert(delivery_id, delivery.version());
        }
        for (publication_id, acceptance) in staged.publications {
            let version = inner
                .publication_versions
                .get(&publication_id)
                .copied()
                .unwrap_or(0);
            inner
                .publications
                .insert(publication_id.clone(), acceptance);
            inner
                .publication_versions
                .insert(publication_id, version + 1);
        }
        for (key, anchor) in staged.anchors {
            inner.anchors.insert(key, anchor);
        }
        inner.conflicts.extend(staged.conflicts);
        for audit in staged.audits {
            inner.next_audit_sequence += 1;
            inner.audits.push(audit);
        }

        Ok(CommitReceipt {
            transaction_id: handle.transaction_id,
        })
    }

    /// Rolls back a staged transaction.
    pub fn rollback_transaction(&self, handle: UnitOfWorkHandle) -> Result<(), UnitOfWorkError> {
        let mut inner = self.inner.lock().expect("memory store lock poisoned");
        inner
            .transactions
            .remove(&handle.transaction_id)
            .map(|_| ())
            .ok_or(UnitOfWorkError::InvalidHandle)
    }

    /// Stages a publication acceptance insert.
    pub fn stage_publication(
        &self,
        transaction_id: u64,
        acceptance: PublicationAcceptance,
    ) -> Result<Version, RepositoryError> {
        let mut inner = self.inner.lock().expect("memory store lock poisoned");
        if inner.publications.contains_key(&acceptance.publication_id) {
            return Err(RepositoryError::UniqueViolation);
        }
        let staged = inner
            .transactions
            .get_mut(&transaction_id)
            .ok_or(RepositoryError::Unavailable)?;
        if staged.publications.contains_key(&acceptance.publication_id) {
            return Err(RepositoryError::UniqueViolation);
        }

        staged
            .publications
            .insert(acceptance.publication_id.clone(), acceptance.clone());
        Ok(inner
            .publication_versions
            .get(&acceptance.publication_id)
            .copied()
            .unwrap_or(0)
            + 1)
    }

    /// Returns a committed publication acceptance.
    pub fn publication(&self, publication_id: &PublicationId) -> Option<PublicationAcceptance> {
        self.inner
            .lock()
            .expect("memory store lock poisoned")
            .publications
            .get(publication_id)
            .cloned()
    }

    /// Stages a new idempotency anchor.
    pub fn stage_idempotency_anchor(
        &self,
        transaction_id: u64,
        anchor: IdempotencyAnchor,
    ) -> Result<(), RepositoryError> {
        let mut inner = self.inner.lock().expect("memory store lock poisoned");
        let key = (anchor.scope.clone(), anchor.key.clone());
        if inner.anchors.contains_key(&key) {
            return Err(RepositoryError::UniqueViolation);
        }
        let staged = inner
            .transactions
            .get_mut(&transaction_id)
            .ok_or(RepositoryError::Unavailable)?;
        if staged.anchors.contains_key(&key) {
            return Err(RepositoryError::UniqueViolation);
        }
        staged.anchors.insert(key, anchor);
        Ok(())
    }

    /// Returns a committed idempotency anchor.
    pub fn idempotency_anchor(
        &self,
        scope: &IdempotencyScope,
        key: &IdempotencyKey,
    ) -> Option<IdempotencyAnchor> {
        self.inner
            .lock()
            .expect("memory store lock poisoned")
            .anchors
            .get(&(scope.clone(), key.clone()))
            .cloned()
    }

    /// Stages an idempotency conflict summary.
    pub fn stage_idempotency_conflict(
        &self,
        transaction_id: u64,
        _scope: IdempotencyScope,
        _key: IdempotencyKey,
        conflict: IdempotencyConflict,
    ) -> Result<(), RepositoryError> {
        let mut inner = self.inner.lock().expect("memory store lock poisoned");
        let staged = inner
            .transactions
            .get_mut(&transaction_id)
            .ok_or(RepositoryError::Unavailable)?;
        staged.conflicts.push(conflict);
        Ok(())
    }

    /// Returns committed conflict summaries.
    pub fn idempotency_conflicts(&self) -> Vec<IdempotencyConflict> {
        self.inner
            .lock()
            .expect("memory store lock poisoned")
            .conflicts
            .clone()
    }

    /// Stages an audit entry.
    pub fn stage_audit_entry(
        &self,
        transaction_id: u64,
        entry: BusAuditEntry,
    ) -> Result<u64, RepositoryError> {
        let mut inner = self.inner.lock().expect("memory store lock poisoned");
        let next_sequence = inner.next_audit_sequence + 1;
        let staged = inner
            .transactions
            .get_mut(&transaction_id)
            .ok_or(RepositoryError::Unavailable)?;
        staged.audits.push(entry);
        Ok(next_sequence)
    }

    /// Returns committed audit entries.
    pub fn audit_entries(&self) -> Vec<BusAuditEntry> {
        self.inner
            .lock()
            .expect("memory store lock poisoned")
            .audits
            .clone()
    }

    /// Seeds a committed delivery aggregate for tests and fake workers.
    pub fn seed_delivery(&self, delivery: DeliveryRecord) -> Result<Version, RepositoryError> {
        let mut inner = self.inner.lock().expect("memory store lock poisoned");
        if inner.deliveries.contains_key(&delivery.delivery_id) {
            return Err(RepositoryError::UniqueViolation);
        }

        let mut committed = delivery;
        committed.set_version(1);
        inner
            .delivery_versions
            .insert(committed.delivery_id.clone(), committed.version());
        inner
            .deliveries
            .insert(committed.delivery_id.clone(), committed);
        Ok(1)
    }

    /// Stages a delivery aggregate update.
    pub fn stage_delivery_save(
        &self,
        transaction_id: u64,
        delivery: DeliveryRecord,
        expected_version: Version,
    ) -> Result<Version, RepositoryError> {
        let mut inner = self.inner.lock().expect("memory store lock poisoned");
        let committed_version = inner
            .delivery_versions
            .get(&delivery.delivery_id)
            .copied()
            .ok_or(RepositoryError::VersionConflict)?;
        if committed_version != expected_version {
            return Err(RepositoryError::VersionConflict);
        }
        let staged = inner
            .transactions
            .get_mut(&transaction_id)
            .ok_or(RepositoryError::Unavailable)?;
        let mut staged_delivery = delivery;
        staged_delivery.set_version(expected_version + 1);
        let new_version = staged_delivery.version();
        staged
            .deliveries
            .insert(staged_delivery.delivery_id.clone(), staged_delivery);

        Ok(new_version)
    }

    /// Returns a committed delivery aggregate.
    pub fn delivery(&self, delivery_id: &DeliveryId) -> Option<DeliveryRecord> {
        self.inner
            .lock()
            .expect("memory store lock poisoned")
            .deliveries
            .get(delivery_id)
            .cloned()
    }

    /// Returns committed schedulable deliveries ordered by identifier.
    pub fn schedulable_deliveries(&self, cursor: &str, limit: u32) -> Vec<DeliveryRecord> {
        let inner = self.inner.lock().expect("memory store lock poisoned");
        let has_after_cursor = inner
            .deliveries
            .keys()
            .any(|delivery_id| delivery_id.as_str() > cursor);

        inner
            .deliveries
            .values()
            .filter(|delivery| {
                delivery.status == bus_contracts::metadata::DeliveryStatus::Scheduled
                    && (!has_after_cursor || delivery.delivery_id.as_str() > cursor)
            })
            .take(limit as usize)
            .cloned()
            .collect()
    }
}
