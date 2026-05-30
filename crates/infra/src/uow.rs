//! In-memory unit-of-work adapter.

use std::sync::{Arc, Mutex};

use bus_application::{
    CommitReceipt, RollbackReason, UnitOfWork, UnitOfWorkError, UnitOfWorkHandle, UnitOfWorkPurpose,
};
use bus_contracts::metadata::ActorContext;

use crate::store::SharedMemoryStore;

/// An in-memory unit-of-work implementation with staged writes.
#[derive(Clone)]
pub struct InMemoryUnitOfWork {
    store: SharedMemoryStore,
    fail_next_commit: Arc<Mutex<Option<UnitOfWorkError>>>,
}

impl InMemoryUnitOfWork {
    /// Creates a new in-memory unit of work.
    pub fn new(store: SharedMemoryStore) -> Self {
        Self {
            store,
            fail_next_commit: Arc::new(Mutex::new(None)),
        }
    }

    /// Fails the next commit with the provided error.
    pub fn fail_next_commit(&self, error: UnitOfWorkError) {
        *self
            .fail_next_commit
            .lock()
            .expect("uow failpoint lock poisoned") = Some(error);
    }
}

impl UnitOfWork for InMemoryUnitOfWork {
    async fn begin(
        &self,
        purpose: UnitOfWorkPurpose,
        _actor: ActorContext,
    ) -> Result<UnitOfWorkHandle, UnitOfWorkError> {
        self.store.begin_transaction(purpose)
    }

    async fn commit(&self, handle: UnitOfWorkHandle) -> Result<CommitReceipt, UnitOfWorkError> {
        if let Some(error) = self
            .fail_next_commit
            .lock()
            .expect("uow failpoint lock poisoned")
            .take()
        {
            return Err(error);
        }

        self.store.commit_transaction(handle)
    }

    async fn rollback(
        &self,
        handle: UnitOfWorkHandle,
        _reason: RollbackReason,
    ) -> Result<(), UnitOfWorkError> {
        self.store.rollback_transaction(handle)
    }
}
