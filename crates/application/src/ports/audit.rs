//! Audit repository port.

use bus_domain::audit::BusAuditEntry;

use crate::errors::RepositoryError;
use crate::ports::UnitOfWorkHandle;

/// Append-only audit repository.
pub trait AuditTrailRepository: Send + Sync {
    /// Appends a committed audit entry to the current transaction.
    async fn append(
        &self,
        entry: BusAuditEntry,
        uow: &UnitOfWorkHandle,
    ) -> Result<u64, RepositoryError>;
}
