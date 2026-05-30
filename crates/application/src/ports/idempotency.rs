//! Idempotency repository port.

use bus_contracts::metadata::IdempotencyKey;
use bus_domain::idempotency::{IdempotencyAnchor, IdempotencyConflict, IdempotencyScope};

use crate::errors::RepositoryError;
use crate::ports::UnitOfWorkHandle;

/// Idempotency repository.
pub trait IdempotencyRepository: Send + Sync {
    /// Finds a committed idempotency anchor.
    async fn find(
        &self,
        scope: &IdempotencyScope,
        key: &IdempotencyKey,
    ) -> Result<Option<IdempotencyAnchor>, RepositoryError>;

    /// Binds a new idempotency anchor.
    async fn bind(
        &self,
        anchor: IdempotencyAnchor,
        uow: &UnitOfWorkHandle,
    ) -> Result<(), RepositoryError>;

    /// Persists an idempotency conflict summary.
    async fn mark_conflict(
        &self,
        scope: IdempotencyScope,
        key: IdempotencyKey,
        conflict: IdempotencyConflict,
        uow: &UnitOfWorkHandle,
    ) -> Result<(), RepositoryError>;
}
