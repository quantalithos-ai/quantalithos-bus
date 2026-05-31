//! Recovery truth repository port.

use bus_contracts::metadata::{
    DeadLetterId, FailureMaterialId, PageLimit, RetryScanCursor, Timestamp, Version,
};
use bus_domain::recovery::{DeadLetterEntry, FailureMaterial, ReplayPreparation, RetryPlan};

use crate::errors::RepositoryError;
use crate::ports::UnitOfWorkHandle;

/// Repository port for bus-owned recovery truth.
pub trait RecoveryRepository: Send + Sync {
    /// Creates or updates one retry plan inside the current transaction.
    async fn save_retry_plan(
        &self,
        retry_plan: RetryPlan,
        expected_version: Option<Version>,
        uow: &UnitOfWorkHandle,
    ) -> Result<Version, RepositoryError>;

    /// Scans due retry plans for one retry-cycle batch.
    async fn find_due_retry(
        &self,
        cursor: RetryScanCursor,
        limit: PageLimit,
        now: Timestamp,
    ) -> Result<Vec<RetryPlan>, RepositoryError>;

    /// Saves one dead-letter entry and the linked failure material atomically.
    async fn save_dead_letter(
        &self,
        entry: DeadLetterEntry,
        material: FailureMaterial,
        uow: &UnitOfWorkHandle,
    ) -> Result<Version, RepositoryError>;

    /// Loads one committed dead-letter entry by identifier.
    async fn get_dead_letter(
        &self,
        dead_letter_id: &DeadLetterId,
    ) -> Result<Option<DeadLetterEntry>, RepositoryError>;

    /// Saves one replay preparation inside the current transaction.
    async fn save_replay_preparation(
        &self,
        preparation: ReplayPreparation,
        uow: &UnitOfWorkHandle,
    ) -> Result<Version, RepositoryError>;

    /// Loads one committed failure material by identifier.
    async fn get_failure_material(
        &self,
        failure_material_id: &FailureMaterialId,
    ) -> Result<Option<FailureMaterial>, RepositoryError>;
}
