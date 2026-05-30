//! Job DTOs for bus operations workers.

use serde::{Deserialize, Serialize};

use crate::metadata::{BackendId, DeliveryScanCursor, JobRunId};

/// Scans schedulable deliveries and progresses them through the default backend path.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunDeliveryProgressionJob {
    /// The unique job run identifier.
    pub job_run_id: JobRunId,
    /// The schedulable-delivery scan cursor.
    pub cursor: DeliveryScanCursor,
    /// The maximum number of deliveries to scan in the current batch.
    pub batch_size: u32,
    /// The logical backend identifier selected for the batch.
    pub backend_id: BackendId,
}
