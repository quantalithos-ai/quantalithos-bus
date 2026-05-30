//! Feedback truth repository port.

use bus_contracts::metadata::{FeedbackId, Version};
use bus_domain::feedback::FeedbackResult;

use crate::errors::RepositoryError;
use crate::ports::UnitOfWorkHandle;

/// Repository port for bus-owned feedback truth.
pub trait FeedbackRepository: Send + Sync {
    /// Inserts a committed feedback result inside the current transaction.
    async fn insert(
        &self,
        feedback: FeedbackResult,
        uow: &UnitOfWorkHandle,
    ) -> Result<Version, RepositoryError>;

    /// Loads one committed feedback result by identifier.
    async fn get(
        &self,
        feedback_id: &FeedbackId,
    ) -> Result<Option<FeedbackResult>, RepositoryError>;
}
