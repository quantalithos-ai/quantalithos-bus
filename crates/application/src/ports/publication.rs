//! Publication repository port.

use bus_contracts::metadata::{PublicationId, Version};
use bus_domain::publication::PublicationAcceptance;

use crate::errors::RepositoryError;
use crate::ports::UnitOfWorkHandle;

/// Repository for committed publication acceptance truth.
pub trait PublicationRepository: Send + Sync {
    /// Inserts a new publication acceptance record.
    async fn insert(
        &self,
        acceptance: PublicationAcceptance,
        uow: &UnitOfWorkHandle,
    ) -> Result<Version, RepositoryError>;

    /// Reads a committed publication acceptance.
    async fn get(
        &self,
        publication_id: &PublicationId,
    ) -> Result<Option<PublicationAcceptance>, RepositoryError>;
}
