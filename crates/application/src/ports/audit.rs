//! Audit repository port.

use bus_contracts::metadata::AuditChainRef;
use bus_domain::audit::{AuditChain, BusAuditEntry};

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

    /// Loads one committed audit chain by reference.
    async fn load_chain(&self, chain_ref: &AuditChainRef) -> Result<AuditChain, RepositoryError>;
}
