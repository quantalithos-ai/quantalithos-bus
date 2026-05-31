//! Publication repository port.

use bus_contracts::metadata::{PublicationId, Version};
use bus_domain::publication::{PublicationAcceptance, PublicationMaterial};

use crate::errors::RepositoryError;
use crate::ports::UnitOfWorkHandle;

/// Repository for committed publication acceptance truth.
pub trait PublicationRepository: Send + Sync {
    /// Stores the committed publication material snapshot inside the current transaction.
    async fn store_material(
        &self,
        material: PublicationMaterial,
        uow: &UnitOfWorkHandle,
    ) -> Result<(), RepositoryError>;

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

    /// Reads the committed publication material snapshot.
    async fn get_material(
        &self,
        publication_id: &PublicationId,
    ) -> Result<Option<PublicationMaterial>, RepositoryError>;
}
