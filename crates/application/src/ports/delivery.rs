//! Delivery truth repository port.

use bus_contracts::metadata::{DeliveryId, DeliveryScanCursor, Version};
use bus_domain::delivery::{DeliveryHistoryEntry, DeliveryRecord};

use crate::errors::RepositoryError;
use crate::ports::UnitOfWorkHandle;

/// Repository port for bus-owned delivery truth.
pub trait DeliveryRepository: Send + Sync {
    /// Locks and loads a delivery inside a write transaction.
    async fn get_for_update(
        &self,
        delivery_id: &DeliveryId,
        uow: &UnitOfWorkHandle,
    ) -> Result<Option<DeliveryRecord>, RepositoryError>;

    /// Saves the current delivery aggregate using optimistic concurrency.
    async fn save(
        &self,
        delivery: DeliveryRecord,
        expected_version: Version,
        uow: &UnitOfWorkHandle,
    ) -> Result<Version, RepositoryError>;

    /// Scans committed schedulable deliveries for a batch job.
    async fn find_schedulable(
        &self,
        cursor: DeliveryScanCursor,
        limit: u32,
    ) -> Result<Vec<DeliveryRecord>, RepositoryError>;

    /// Loads committed history entries for a delivery.
    async fn load_history(
        &self,
        delivery_id: &DeliveryId,
    ) -> Result<Vec<DeliveryHistoryEntry>, RepositoryError>;
}
