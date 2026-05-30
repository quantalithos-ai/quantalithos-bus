//! Transport backend port contracts.

use bus_contracts::events::BackendDeliverySignalInput;
use bus_contracts::metadata::BackendDeliveryResult;
use bus_contracts::metadata::{
    BackendCapabilityRef, BackendId, JobMetadata, JobRunId, JobTriggerSource, TraceContextRef,
};
use bus_domain::delivery::DeliveryAttempt;
use bus_domain::publication::TransportSemantic;

use crate::errors::TransportPortError;

/// Context supplied to a backend dispatch call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendDispatchContext {
    /// The job run that triggered the dispatch.
    pub job_run_id: JobRunId,
    /// The trace reference associated with the batch.
    pub trace_ref: TraceContextRef,
    /// The trigger source for the current job run.
    pub trigger_source: JobTriggerSource,
    /// The logical backend selected for the batch.
    pub backend_id: BackendId,
}

impl BackendDispatchContext {
    /// Builds a dispatch context from job metadata and the selected backend.
    pub fn from_job(meta: JobMetadata, backend_id: BackendId) -> Self {
        Self {
            job_run_id: meta.job_run_id,
            trace_ref: meta.trace_ref,
            trigger_source: meta.trigger_source,
            backend_id,
        }
    }
}

/// The normalized backend capability report returned by a fake or real adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendCapabilityReport {
    /// The backend capability under inspection.
    pub capability_ref: BackendCapabilityRef,
    /// Whether the capability is currently available for dispatch.
    pub available: bool,
}

/// Outbound backend dispatch port.
pub trait TransportBackendPort: Send + Sync {
    /// Dispatches one attempt through the backend.
    async fn dispatch(
        &self,
        semantic: TransportSemantic,
        attempt: DeliveryAttempt,
        context: BackendDispatchContext,
    ) -> Result<BackendDeliveryResult, TransportPortError>;

    /// Normalizes one backend delivery signal into a bus-owned attempt result.
    async fn normalize_signal(
        &self,
        signal: BackendDeliverySignalInput,
    ) -> Result<BackendDeliveryResult, TransportPortError>;

    /// Checks whether the backend capability is currently available.
    async fn check_capability(
        &self,
        capability_ref: BackendCapabilityRef,
    ) -> Result<BackendCapabilityReport, TransportPortError>;
}
