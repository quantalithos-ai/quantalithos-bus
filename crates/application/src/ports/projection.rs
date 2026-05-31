//! Read-projection repository port.

use bus_contracts::metadata::{BackendId, FailureSummaryId, ProjectionVersion, TransportViewId};
use bus_contracts::views::BackendHealthView;
use bus_domain::read_output::{FailureSummaryProjection, TransportViewProjection};

use crate::errors::RepositoryError;
use crate::ports::UnitOfWorkHandle;

/// Repository port for read-only transport, failure, and backend-health projections.
pub trait ReadProjectionRepository: Send + Sync {
    /// Inserts or updates one transport-view projection inside the current transaction.
    async fn upsert_transport_view(
        &self,
        projection: TransportViewProjection,
        uow: &UnitOfWorkHandle,
    ) -> Result<ProjectionVersion, RepositoryError>;

    /// Inserts or updates one failure-summary projection inside the current transaction.
    async fn upsert_failure_summary(
        &self,
        projection: FailureSummaryProjection,
        uow: &UnitOfWorkHandle,
    ) -> Result<ProjectionVersion, RepositoryError>;

    /// Inserts or updates one backend-health view inside the current transaction.
    async fn upsert_backend_health(
        &self,
        view: BackendHealthView,
        uow: &UnitOfWorkHandle,
    ) -> Result<ProjectionVersion, RepositoryError>;

    /// Loads one committed transport-view projection by identifier.
    async fn get_transport_view_projection(
        &self,
        transport_view_id: &TransportViewId,
    ) -> Result<Option<TransportViewProjection>, RepositoryError>;

    /// Loads one committed failure-summary projection by identifier.
    async fn get_failure_summary_projection(
        &self,
        failure_summary_id: &FailureSummaryId,
    ) -> Result<Option<FailureSummaryProjection>, RepositoryError>;

    /// Loads one committed backend-health view by backend identifier.
    async fn get_backend_health_view(
        &self,
        backend_id: &BackendId,
    ) -> Result<Option<BackendHealthView>, RepositoryError>;
}
